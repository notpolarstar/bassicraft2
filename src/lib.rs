use std::{iter, sync::Arc};

use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};
use cgmath::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use model::Vertex;

mod random;

mod camera;
mod player;
mod model;
mod resources;
mod texture;

mod world;
mod texture_atlas;
mod block;
mod chunk;

mod gui;

mod ecs;

mod particles;

mod network;

// #[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::from_cols(
    cgmath::Vector4::new(1.0, 0.0, 0.0, 0.0),
    cgmath::Vector4::new(0.0, 1.0, 0.0, 0.0),
    cgmath::Vector4::new(0.0, 0.0, 0.5, 0.0),
    cgmath::Vector4::new(0.0, 0.0, 0.5, 1.0),
);

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_position: [f32; 4],
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    fn new() -> Self {
        use cgmath::SquareMatrix;
        Self {
            view_position: [0.0; 4],
            view_proj: cgmath::Matrix4::identity().into(),
        }
    }

    fn update_view_proj(&mut self, camera: &camera::Camera, projection: &camera::Projection) {
        self.view_position = camera.position.to_homogeneous().into();
        self.view_proj = (projection.calc_matrix() * camera.calc_matrix()).into();
    }
}

struct Instance {
    position: cgmath::Vector3<f32>,
    rotation: cgmath::Quaternion<f32>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
}

impl Instance {
    fn to_raw(&self) -> InstanceRaw {
        InstanceRaw {
            model: (cgmath::Matrix4::from_translation(self.position)
                * cgmath::Matrix4::from(self.rotation))
            .into(),
        }
    }
}

impl InstanceRaw {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We'll have to reassemble the mat4 in the shader.
                wgpu::VertexAttribute {
                    offset: 0,
                    // While our vertex shader only uses locations 0, and 1 now, in later tutorials, we'll
                    // be using 2, 3, and 4, for Vertex. We'll start at slot 5, not conflict with them later
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

//TEMP
const INV_SIZE: u32 = 256;

pub enum GameStates {
    MainMenu,
    Options,
    WorldSelection,
    InGame,
    PauseMenu,
}

pub struct State {
    egui_renderer: gui::EguiRenderer,
    
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,
    render_pipeline: wgpu::RenderPipeline,

    // vertex_buffer: wgpu::Buffer,
    // index_buffer: wgpu::Buffer,
    window: Arc<Window>,

    // num_vertices: u32,
    diffuse_texture: texture::Texture,
    diffuse_bind_group: wgpu::BindGroup,

    // camera: Camera,
    // camera: camera::Camera,
    // projection: camera::Projection,

    player: player::Player,

    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    // camera_controller: CameraController,
    // camera_controller: camera::CameraController,

    depth_texture: texture::Texture,

    obj_model: model::Model,
    loaded_models: std::collections::HashMap<String, model::Model>,

    world: world::World,

    mouse_pressed: bool,

    model_rendering_pipeline: wgpu::RenderPipeline,
    
    ecs_world: ecs::EcsWorld,

    particle_rendering_pipeline: wgpu::RenderPipeline,

    particle_vertex_buffers: std::collections::HashMap<u32, wgpu::Buffer>,
    particle_index_buffer: wgpu::Buffer,
    particle_instance_buffer: wgpu::Buffer,
    particle_instance_capacity: usize,

    net_client: Option<network::NetClient>,
    #[cfg(not(target_arch = "wasm32"))]
    net_server: Option<network::NetServer>,
    multiplayer_panel: gui::MultiplayerPanel,
    net_tick_accumulator: f32,

    wboit_accum_texture: wgpu::Texture,
    wboit_reveal_texture: wgpu::Texture,
    wboit_accum_view: wgpu::TextureView,
    wboit_reveal_view: wgpu::TextureView,

    wboit_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group: wgpu::BindGroup,
    composite_bind_group_layout: wgpu::BindGroupLayout,
}

fn create_wboit_textures(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };

    let accum_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("WBOIT Accum Texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let accum_view = accum_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let reveal_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("WBOIT Reveal Texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let reveal_view = reveal_texture.create_view(&wgpu::TextureViewDescriptor::default());

    (accum_texture, accum_view, reveal_texture, reveal_view)
}

impl State {
    async fn new(window: Arc<Window>) -> anyhow::Result<State> {
        let size = window.inner_size();
        let max_size = 2048;
        let width = size.width.min(max_size).max(1);
        let height = size.height.min(max_size).max(1);

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                // required_features: wgpu::Features::POLYGON_MODE_LINE,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                required_limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::defaults()
                },
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        // Shader code in this tutorial assumes an Srgb surface texture. Using a different
        // one will result all the colors comming out darker. If you want to support non
        // Srgb surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };

        let diffuse_bytes = include_bytes!("../img/texture_atlas.png");
        // let diffuse_image = image::load_from_memory(diffuse_bytes).unwrap();
        // let diffuse_rgba = diffuse_image.to_rgba8();

        // use image::GenericImageView;
        // let dimentions = diffuse_image.dimensions();

        let diffuse_texture =
            texture::Texture::from_bytes(&device, &queue, diffuse_bytes, "texture_atlas.png")
                .unwrap();

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("diffuse_bind_group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let model_shader = device.create_shader_module(wgpu::include_wgsl!("model_shader.wgsl"));
        let particle_shader = device.create_shader_module(wgpu::include_wgsl!("particle_shader.wgsl"));

        // let camera = Camera {
        //     eye: (0.0, 1.0, 2.0).into(),
        //     target: (0.0, 0.0, 0.0).into(),
        //     up: cgmath::Vector3::unit_y(),
        //     aspect: config.width as f32 / config.height as f32,
        //     fovy: 45.0,
        //     znear: 0.1,
        //     zfar: 100.0,
        // };

        // let camera = camera::Camera::new((0.0, 100.0, 10.0), cgmath::Deg(-90.0), cgmath::Deg(-20.0));
        // let projection =
        //     camera::Projection::new(config.width, config.height, cgmath::Deg(45.0), 0.1, 1000.0);

        // let camera_controller = camera::CameraController::new(4.0, 0.4);

        let player = player::Player::new([0.0, 100.0, 10.0], &config);

        let mut camera_uniform = CameraUniform::new();
        // camera_uniform.update_view_proj(&camera);
        // camera_uniform.update_view_proj(&camera, &projection);
        camera_uniform.update_view_proj(&player.camera, &player.projection);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_layout_group"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // let camera_controller = CameraController::new(0.05);

        let depth_texture =
            texture::Texture::create_depth_texture(&device, &config, "depth_texture");

        let accum_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let reveal_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                // buffers: &[Vertex::desc(), InstanceRaw::desc()],
                // buffers: &[model::ModelVertex::desc(), InstanceRaw::desc()],
                buffers: &[block::BlockVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                // polygon_mode: wgpu::PolygonMode::Line,
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let (wboit_accum_texture, wboit_accum_view, wboit_reveal_texture, wboit_reveal_view) =
            create_wboit_textures(&device, width, height);

        let transparent_shader =
            device.create_shader_module(wgpu::include_wgsl!("transparent_shader.wgsl"));

        let wboit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("WBOIT Transparent Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &transparent_shader,
                entry_point: Some("vs_main"),
                buffers: &[block::BlockVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &transparent_shader,
                entry_point: Some("fs_main"),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(accum_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(reveal_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let composite_shader =
            device.create_shader_module(wgpu::include_wgsl!("composite_shader.wgsl"));

        let wboit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("WBOIT Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let composite_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("WBOIT Composite BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WBOIT Composite BG"),
            layout: &composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&wboit_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&wboit_accum_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&wboit_reveal_view),
                },
            ],
        });

        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("WBOIT Composite Pipeline Layout"),
                bind_group_layouts: &[&composite_bind_group_layout],
                push_constant_ranges: &[],
            });

        let composite_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("WBOIT Composite Pipeline"),
                layout: Some(&composite_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &composite_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &composite_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        let obj_model =
            resources::load_model("steve.obj", &device, &queue, &texture_bind_group_layout)
                .await
                .unwrap();

        let mut loaded_models = std::collections::HashMap::new();
        
        // TEMP. LOAD MODELS DYNAMICALLY LATER !!!!!!!
        if let Ok(cube_model) = resources::load_model("cube.obj", &device, &queue, &texture_bind_group_layout).await {
            loaded_models.insert("cube.obj".to_string(), cube_model);
        }

        if let Ok(steve_model) = resources::load_model("steve.obj", &device, &queue, &texture_bind_group_layout).await {
            loaded_models.insert("steve.obj".to_string(), steve_model);
        }

        if let Ok(creeper_model) = resources::load_model("Creeper.obj", &device, &queue, &texture_bind_group_layout).await {
            loaded_models.insert("Creeper.obj".to_string(), creeper_model);
        }

        let world = world::World::new(&device, &queue, 0x1f6c2);

        let mut egui_renderer = gui::EguiRenderer::new(
            &device,
            config.format,
            None,
            1,
            &window,
        );

        let ui_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI Block Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[block::BlockVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let model_rendering_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Model Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &model_shader,
                entry_point: Some("vs_main"),
                buffers: &[model::ModelVertex::desc(), InstanceRaw::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &model_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let particle_rendering_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &particle_shader,
                entry_point: Some("vs_main"),
                buffers: &[particles::ParticleVertex::desc(), InstanceRaw::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &particle_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        const MAX_PARTICLES: usize = 1000;
        
        use particles::ParticleVertex;
        
        let particle_indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        let particle_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Index Buffer Pool"),
            contents: bytemuck::cast_slice(&particle_indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        
        let particle_instance_buffer_size = (MAX_PARTICLES * std::mem::size_of::<InstanceRaw>()) as u64;
        let particle_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Instance Buffer Pool"),
            size: particle_instance_buffer_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut particle_vertex_buffers = std::collections::HashMap::new();
        for block_type in 0..INV_SIZE {
            let tex_x = ((block_type % 16) as f32) / 16.0;
            let tex_y = ((block_type / 16) as f32) / 16.0;
            let tex_size = 1.0 / 16.0;
            
            let particle_vertices = vec![
                ParticleVertex { position: [-0.5, -0.5,  0.0], tex_coords: [tex_x, tex_y + tex_size] },
                ParticleVertex { position: [ 0.5, -0.5,  0.0], tex_coords: [tex_x + tex_size, tex_y + tex_size] },
                ParticleVertex { position: [ 0.5,  0.5,  0.0], tex_coords: [tex_x + tex_size, tex_y] },
                ParticleVertex { position: [-0.5,  0.5,  0.0], tex_coords: [tex_x, tex_y] },
            ];
            
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Particle Vertex Buffer {}", block_type)),
                contents: bytemuck::cast_slice(&particle_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            
            particle_vertex_buffers.insert(block_type, vertex_buffer);
        }

        use crate::block::BlockVertex;

        let mut block_meshes = Vec::new();
        
        for block_type in 0..INV_SIZE {
            let tex_x = (block_type % 16) as f32 / 16.0;
            let tex_y = (block_type / 16) as f32 / 16.0;
            // 0.5 / atlas_px (256 px atlas) BAD HARDCODED FIX LATER
            let half = 0.5 / (16.0 * 16.0_f32);
            let tex_coords = [tex_x + half, tex_y + half, tex_x + 0.0625 - half, tex_y + 0.0625 - half];
            
            let vertices = vec![
                // Front face
                BlockVertex { position: [-0.5, -0.5,  0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[3], false) },
                BlockVertex { position: [ 0.5, -0.5,  0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[3], false) },
                BlockVertex { position: [ 0.5,  0.5,  0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[1], false) },
                BlockVertex { position: [-0.5,  0.5,  0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[1], false) },
                // Back face
                BlockVertex { position: [ 0.5, -0.5, -0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[3], false) },
                BlockVertex { position: [-0.5, -0.5, -0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[3], false) },
                BlockVertex { position: [-0.5,  0.5, -0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[1], false) },
                BlockVertex { position: [ 0.5,  0.5, -0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[1], false) },
                // Left face
                BlockVertex { position: [-0.5, -0.5, -0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[3], false) },
                BlockVertex { position: [-0.5, -0.5,  0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[3], false) },
                BlockVertex { position: [-0.5,  0.5,  0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[1], false) },
                BlockVertex { position: [-0.5,  0.5, -0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[1], false) },
                // Right face
                BlockVertex { position: [ 0.5, -0.5,  0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[3], false) },
                BlockVertex { position: [ 0.5, -0.5, -0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[3], false) },
                BlockVertex { position: [ 0.5,  0.5, -0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[1], false) },
                BlockVertex { position: [ 0.5,  0.5,  0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[1], false) },
                // Top face
                BlockVertex { position: [-0.5,  0.5,  0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[3], false) },
                BlockVertex { position: [ 0.5,  0.5,  0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[3], false) },
                BlockVertex { position: [ 0.5,  0.5, -0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[1], false) },
                BlockVertex { position: [-0.5,  0.5, -0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[1], false) },
                // Bottom face
                BlockVertex { position: [-0.5, -0.5, -0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[3], false) },
                BlockVertex { position: [ 0.5, -0.5, -0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[3], false) },
                BlockVertex { position: [ 0.5, -0.5,  0.5], packed: BlockVertex::pack(tex_coords[2], tex_coords[1], false) },
                BlockVertex { position: [-0.5, -0.5,  0.5], packed: BlockVertex::pack(tex_coords[0], tex_coords[1], false) },
            ];
            
            let indices: Vec<u32> = (0..6)
                .flat_map(|i| {
                    let base = i * 4;
                    vec![base, base + 1, base + 2, base + 2, base + 3, base]
                })
                .collect();

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("UI Block {} Vertex Buffer", block_type)),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("UI Block {} Index Buffer", block_type)),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            
            block_meshes.push((vertex_buffer, index_buffer, indices.len() as u32));
        }

        use cgmath::{Matrix4, Vector3, Point3, Deg, perspective};
        let ui_camera_pos = Vector3::new(1.5, 1.5, 1.5);
        let ui_camera_target = Vector3::new(0.0, 0.0, 0.0);
        let ui_view = Matrix4::look_at_rh(
            Point3::new(ui_camera_pos.x, ui_camera_pos.y, ui_camera_pos.z),
            Point3::new(ui_camera_target.x, ui_camera_target.y, ui_camera_target.z),
            Vector3::unit_y(),
        );
        let ui_proj = perspective(Deg(45.0), 1.0, 0.1, 100.0);
        
        let ui_camera_uniform = CameraUniform {
            view_position: [ui_camera_pos.x, ui_camera_pos.y, ui_camera_pos.z, 1.0],
            view_proj: (ui_proj * ui_view).into(),
        };

        let ui_camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UI Camera Buffer"),
            contents: bytemuck::cast_slice(&[ui_camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let ui_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("UI Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ui_camera_buffer.as_entire_binding(),
            }],
        });

        egui_renderer.set_block_render_resources(
            ui_render_pipeline,
            world.texture_atlas.diffuse_bind_group.clone(),
            ui_camera_bind_group,
            block_meshes,
        );

        let mut ecs_world = ecs::EcsWorld::new();

        let player_start_pos = cgmath::Vector3::new(0.0, 100.0, 10.0);
        ecs::spawn_player(&mut ecs_world.world, player_start_pos);

        // test entities
        for i in 0..5 {
            let net_id = ecs_world.alloc_net_id();
            ecs::spawn_wandering_mob(
                &mut ecs_world.world,
                cgmath::Vector3::new(5.0 + i as f32 * 2.0, 95.0, 5.0),
                "cube.obj".to_string(),
                net_id,
            );
        }
        
        for i in 0..5 {
            let net_id = ecs_world.alloc_net_id();
            ecs::spawn_following_mob(
                &mut ecs_world.world,
                cgmath::Vector3::new(-5.0 - i as f32 * 2.0, 95.0, -5.0),
                "Creeper.obj".to_string(),
                net_id,
            );
        }

        Ok(Self {
            egui_renderer,
            surface,
            device,
            queue,
            config,
            is_surface_configured: false,
            render_pipeline,
            // vertex_buffer,
            // index_buffer,
            window,
            // num_vertices,
            diffuse_texture,
            diffuse_bind_group,
            player,
            // camera,
            // projection,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            // camera_controller,
            depth_texture,
            obj_model,
            loaded_models,
            world: world,
            mouse_pressed: false,
            model_rendering_pipeline,
            ecs_world,
            particle_rendering_pipeline,
            particle_vertex_buffers,
            particle_index_buffer,
            particle_instance_buffer,
            particle_instance_capacity: MAX_PARTICLES,
            net_client: None,
            #[cfg(not(target_arch = "wasm32"))]
            net_server: None,
            multiplayer_panel: gui::MultiplayerPanel::default(),
            net_tick_accumulator: 0.0,
            wboit_accum_texture,
            wboit_reveal_texture,
            wboit_accum_view,
            wboit_reveal_view,
            wboit_pipeline,
            composite_pipeline,
            composite_bind_group,
            composite_bind_group_layout,
            // cursor_locked: false,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let max_size = 2048;
        let width = width.min(max_size);
        let height = height.min(max_size);

        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;

            self.player.projection.resize(width, height);

            self.depth_texture =
                texture::Texture::create_depth_texture(&self.device, &self.config, "depth_texture");

            let (accum_tex, accum_view, reveal_tex, reveal_view) =
                create_wboit_textures(&self.device, width, height);
            self.wboit_accum_texture  = accum_tex;
            self.wboit_accum_view     = accum_view;
            self.wboit_reveal_texture = reveal_tex;
            self.wboit_reveal_view    = reveal_view;

            let wboit_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("WBOIT Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            self.composite_bind_group =
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("WBOIT Composite BG"),
                    layout: &self.composite_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&wboit_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&self.wboit_accum_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&self.wboit_reveal_view),
                        },
                    ],
                });
        }
    }

    fn handle_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        match button {
            MouseButton::Left => {
                self.mouse_pressed = pressed;
                if pressed && !self.player.show_inventory {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        let window = web_sys::window().unwrap();
                        let document = window.document().unwrap();
                        let canvas = document.get_element_by_id("canvas").unwrap();
                        let html_canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
                        html_canvas.request_pointer_lock();
                        html_canvas.request_fullscreen();
                        self.lock_cursor();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        use winit::window::CursorGrabMode;
                        self.window
                            .set_cursor_grab(CursorGrabMode::Confined)
                            .or_else(|_e| self.window.set_cursor_grab(CursorGrabMode::Locked))
                            .unwrap();

                        let size = self.window.inner_size();
                        let center = winit::dpi::PhysicalPosition::new(
                            size.width as f64 / 2.0,
                            size.height as f64 / 2.0,
                        );
                        let _ = self.window.set_cursor_position(center);
                        self.lock_cursor();
                    }

                    if let Some(pos) = self.player.get_block_pointed_at(&self.world.chunks) {
                        if let Some(block_type) = self.world.break_block(&self.device, &self.queue, pos) {
                            if block_type != 0 {
                                if let Some(client) = &self.net_client {
                                    client.send(&network::ClientMessage::BreakBlock {
                                        x: pos[0], y: pos[1], z: pos[2],
                                    });
                                }
                                #[cfg(not(target_arch = "wasm32"))]
                                if let Some(server) = &self.net_server {
                                    server.broadcast(&network::ServerMessage::BlockUpdate {
                                        x: pos[0], y: pos[1], z: pos[2], block_type: 0,
                                    });
                                }

                                for _ in 0..8 {
                                    let random1 = random::get_random_f32_normalized().unwrap();
                                    let random2 = random::get_random_f32_normalized().unwrap();
                                    let random3 = random::get_random_f32_normalized().unwrap();
                                    let random4 = random::get_random_f32_normalized().unwrap();
                                    let random5 = random::get_random_f32_normalized().unwrap();
                                    let random6 = random::get_random_f32_normalized().unwrap();
                                    
                                    let offset_x = random1 * 0.6 - 0.3;
                                    let offset_y = random2 * 0.6 - 0.3;
                                    let offset_z = random3 * 0.6 - 0.3;
                                    
                                    let particle_pos = cgmath::Vector3::new(
                                        pos[0] as f32 + 0.5 + offset_x,
                                        pos[1] as f32 + 0.5 + offset_y,
                                        pos[2] as f32 + 0.5 + offset_z,
                                    );
                                    
                                    let velocity = cgmath::Vector3::new(
                                        random4 * 4.0 - 2.0,
                                        random5 * 3.0 + 2.0,
                                        random6 * 4.0 - 2.0,
                                    );
                                    
                                    ecs::spawn_particle(&mut self.ecs_world.world, particle_pos, block_type - 1, velocity);
                                }
                            }
                        }
                    }

                }
            }
            MouseButton::Right => {
                self.mouse_pressed = pressed;
                if pressed {
                    if let Some(pos) = self.player.get_block_placement_pos(&self.world.chunks) {
                        let block_type = self.player.selected_block;
                        self.world.place_block(&self.device, &self.queue, pos, block_type);

                        if let Some(client) = &self.net_client {
                            client.send(&network::ClientMessage::PlaceBlock {
                                x: pos[0], y: pos[1], z: pos[2], block_type,
                            });
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some(server) = &self.net_server {
                            server.broadcast(&network::ServerMessage::BlockUpdate {
                                x: pos[0], y: pos[1], z: pos[2], block_type,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_mouse_scroll(&mut self, delta: &MouseScrollDelta) {
        self.player.camera_controller.handle_mouse_scroll(delta);
    }

    fn lock_cursor(&mut self) {
        self.player.cursor_locked = true;
        self.window.set_cursor_visible(false);
        
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let window = web_sys::window().unwrap();
            let document = window.document().unwrap();
            if let Some(canvas) = document.get_element_by_id("canvas") {
                let html_canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
                let _ = html_canvas.request_pointer_lock();
                let _ = html_canvas.request_fullscreen();
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use winit::window::CursorGrabMode;
            let _ = self.window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_e| self.window.set_cursor_grab(CursorGrabMode::Locked));
        }
    }

    fn unlock_cursor(&mut self) {
        self.player.cursor_locked = false;
        self.window.set_cursor_visible(true);
        
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                let _ = document.exit_pointer_lock();
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use winit::window::CursorGrabMode;
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
    }

    fn update(&mut self, dt: instant::Duration) {
        let dt_secs = dt.as_secs_f32().min(0.1);

        self.player.camera_controller.update_camera(&mut self.player.camera, dt);

        self.ecs_world.update_player_input(
            self.player.camera_controller.amount_forward,
            self.player.camera_controller.amount_backward,
            self.player.camera_controller.amount_left,
            self.player.camera_controller.amount_right,
            self.player.camera_controller.amount_up > 0.0, // jump
        );

        let camera_yaw = self.player.camera.yaw().0;

        self.ecs_world.update(dt_secs, &self.world.chunks, camera_yaw);

        if let Some(player_pos) = self.ecs_world.get_player_position() {
            self.player.camera.position = cgmath::Point3::new(
                player_pos.x,
                player_pos.y + 1.6, // camera at eye level
                player_pos.z,
            );
        }

        self.camera_uniform
            .update_view_proj(&self.player.camera, &self.player.projection);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );

        self.net_tick_accumulator += dt_secs;
        const NET_TICK_RATE: f32 = 1.0 / 20.0;
        if self.net_tick_accumulator >= NET_TICK_RATE {
            self.net_tick_accumulator = 0.0;
            self.network_tick();
        }

        let player_pos = &self.player.camera.position;
        let player_chunk = [
            (player_pos.x / chunk::CHUNK_X_SIZE as f32).floor() as i32,
            (player_pos.z / chunk::CHUNK_Z_SIZE as f32).floor() as i32,
        ];
        self.world.update_chunks(&self.device, &self.queue, player_chunk);
    }

    fn handle_multiplayer_action(&mut self, action: gui::MultiplayerAction) {
        match action {
            gui::MultiplayerAction::Host { port } => {
                #[cfg(not(target_arch = "wasm32"))]
                match network::NetServer::start(port) {
                    Ok(server) => {
                        self.multiplayer_panel.lan_address = Some(server.lan_address.clone());
                        self.multiplayer_panel.status = format!("Hosting on port {}", port);
                        self.multiplayer_panel.is_hosting = true;
                        match network::NetClient::connect(&format!("ws://127.0.0.1:{}", port)) {
                            Ok(client) => { self.net_client = Some(client); }
                            Err(e) => log::error!("Host self-connect failed: {}", e),
                        }
                        self.net_server = Some(server);
                        if let Some(srv) = &self.net_server {
                            let positions: Vec<[i32; 2]> = self.world.chunks.iter().map(|c| c.pos).collect();
                            srv.broadcast(&network::ServerMessage::AvailableChunks(positions));
                        }
                    }
                    Err(e) => {
                        self.multiplayer_panel.status = format!("Host failed: {}", e);
                    }
                }
            }
            gui::MultiplayerAction::Join { url } => {
                match network::NetClient::connect(&url) {
                    Ok(client) => {
                        self.net_client = Some(client);
                        self.multiplayer_panel.status = format!("Connecting to {}\u{2026}", url);
                    }
                    Err(e) => {
                        self.multiplayer_panel.status = format!("Connect failed: {}", e);
                    }
                }
            }
            gui::MultiplayerAction::Disconnect => {
                self.net_client = None;
                #[cfg(not(target_arch = "wasm32"))]
                { self.net_server = None; }
                self.ecs_world.clear_remote_entities();
                self.multiplayer_panel.is_connected = false;
                self.multiplayer_panel.is_hosting  = false;
                self.multiplayer_panel.lan_address = None;
                self.multiplayer_panel.status      = "Disconnected".to_string();
            }
            gui::MultiplayerAction::RequestChunks => {
                let msg = std::mem::take(&mut self.multiplayer_panel.chat_input);
                if !msg.is_empty() {
                    if let Some(client) = &self.net_client {
                        client.send(&network::ClientMessage::Chat { message: msg });
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(server) = &self.net_server {
                        let my_id = self.net_client.as_ref().and_then(|c| c.my_id).unwrap_or(0);
                        server.broadcast(&network::ServerMessage::Chat {
                            sender_id: my_id,
                            message: std::mem::take(&mut self.multiplayer_panel.chat_input),
                        });
                    }
                }
            }
            gui::MultiplayerAction::None => {}
        }
    }

    fn network_tick(&mut self) {
        if let Some(client) = &self.net_client {
            let cam = &self.player.camera;
            let pos = self.ecs_world.get_player_position()
                .unwrap_or_else(|| cgmath::Vector3::new(
                    cam.position.x, cam.position.y, cam.position.z
                ));
            client.send(&network::ClientMessage::PlayerInput {
                forward:  self.player.camera_controller.amount_forward,
                backward: self.player.camera_controller.amount_backward,
                left:     self.player.camera_controller.amount_left,
                right:    self.player.camera_controller.amount_right,
                jump:     self.player.camera_controller.amount_up > 0.0,
                yaw:      cam.yaw().0,
                pitch:    cam.pitch().0,
                position: network::Vec3Net::new(pos.x, pos.y, pos.z),
            });
        }

        let server_msgs: Vec<network::ServerMessage> = if let Some(client) = &mut self.net_client {
            client.poll()
        } else {
            Vec::new()
        };

        for msg in server_msgs {
            match msg {
                network::ServerMessage::Welcome { player_id, spawn } => {
                    self.multiplayer_panel.on_connected(player_id);
                    log::info!("Welcome! My player ID: {}, spawn: {:?}", player_id, spawn);
                    let is_host = {
                        #[cfg(not(target_arch = "wasm32"))]
                        { self.net_server.is_some() }
                        #[cfg(target_arch = "wasm32")]
                        { false }
                    };
                    if !is_host {
                        self.ecs_world.clear_local_mobs();
                    }
                }
                network::ServerMessage::PlayerStates(states) => {
                    let my_id = self.net_client.as_ref().and_then(|c| c.my_id);
                    self.ecs_world.sync_remote_players(&states, my_id);
                }
                network::ServerMessage::EntityStates(states) => {
                    let is_host = {
                        #[cfg(not(target_arch = "wasm32"))]
                        { self.net_server.is_some() }
                        #[cfg(target_arch = "wasm32")]
                        { false }
                    };
                    if !is_host {
                        self.ecs_world.sync_network_entities(&states);
                    }
                }
                network::ServerMessage::BlockUpdate { x, y, z, block_type } => {
                    if block_type == 0 {
                        self.world.break_block(&self.device, &self.queue, [x, y, z]);
                    } else {
                        self.world.place_block(&self.device, &self.queue, [x, y, z], block_type);
                    }
                }
                network::ServerMessage::ChunkData { cx, cz, blocks } => {
                    let mut i = 0;
                    while i + 3 < blocks.len() {
                        let lx = blocks[i]     as i32;
                        let ly = blocks[i + 1] as i32;
                        let lz = blocks[i + 2] as i32;
                        let mat = blocks[i + 3];
                        let world_x = cx * 16 + lx;
                        let world_z = cz * 16 + lz;
                        if mat != 0 {
                            self.world.place_block(&self.device, &self.queue, [world_x, ly, world_z], mat);
                        }
                        i += 4;
                    }
                }
                network::ServerMessage::AvailableChunks(positions) => {
                    if let Some(client) = &self.net_client {
                        for [cx, cz] in positions {
                            client.send(&network::ClientMessage::RequestChunk { cx, cz });
                        }
                    }
                }
                network::ServerMessage::PlayerLeft { player_id } => {
                    log::info!("Player {} left", player_id);
                    self.ecs_world.remove_remote_player(player_id);
                }
                network::ServerMessage::Chat { sender_id, message } => {
                    self.multiplayer_panel.push_chat(sender_id, message);
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(server) = &self.net_server {
            let events = server.poll_incoming();
            for ev in events {
                match ev.message {
                    network::ClientMessage::BreakBlock { x, y, z } => {
                        self.world.break_block(&self.device, &self.queue, [x, y, z]);
                        server.broadcast(&network::ServerMessage::BlockUpdate {
                            x, y, z, block_type: 0,
                        });
                    }
                    network::ClientMessage::PlaceBlock { x, y, z, block_type } => {
                        self.world.place_block(&self.device, &self.queue, [x, y, z], block_type);
                        server.broadcast(&network::ServerMessage::BlockUpdate {
                            x, y, z, block_type,
                        });
                    }
                    network::ClientMessage::RequestChunk { cx, cz } => {
                        if let Some(chunk) = self.world.chunks.iter().find(|c| c.pos == [cx, cz]) {
                            let blocks = network::serialize_chunk_blocks(chunk);
                            server.send_to_client(ev.player_id, &network::ServerMessage::ChunkData {
                                cx, cz, blocks,
                            });
                        }
                    }
                    network::ClientMessage::Chat { message } => {
                        server.broadcast(&network::ServerMessage::Chat {
                            sender_id: ev.player_id,
                            message,
                        });
                    }
                    network::ClientMessage::PlayerInput { .. } => {
                        let states = server.player_states_snapshot();
                        server.broadcast(&network::ServerMessage::PlayerStates(states));

                        let entity_states = self.ecs_world.get_networked_entities_data();
                        if !entity_states.is_empty() {
                            server.broadcast(&network::ServerMessage::EntityStates(entity_states));
                        }
                    }
                }
            }
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.window.request_redraw();

        // We can't render unless the surface is configured
        if !self.is_surface_configured {
            return Ok(());
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let entity_render_data = self.ecs_world.get_entities_render_data();
        let mut entities_by_model: std::collections::HashMap<String, Vec<InstanceRaw>> = std::collections::HashMap::new();
        
        for (pos, rot, model_name) in entity_render_data {
            let instance = Instance {
                position: pos,
                rotation: rot,
            }.to_raw();
            entities_by_model.entry(model_name).or_insert_with(Vec::new).push(instance);
        }

        let mut entity_buffers: std::collections::HashMap<String, (wgpu::Buffer, usize)> = std::collections::HashMap::new();
        for (model_name, instances) in &entities_by_model {
            if !instances.is_empty() {
                use wgpu::util::DeviceExt;
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("Entity Instance Buffer: {}", model_name)),
                    contents: bytemuck::cast_slice(instances),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                entity_buffers.insert(model_name.clone(), (buffer, instances.len()));
            }
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.world.texture_atlas.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);

            self.world.chunk_buffers.iter().filter(|cb| cb.vertex_buffer.size() > 0 && cb.indices_buffer.size() > 0).for_each(|cb| {
                render_pass.set_vertex_buffer(0, cb.vertex_buffer.slice(..));
                render_pass.set_index_buffer(cb.indices_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..cb.num_elements, 0, 0..1);
            });

            render_pass.set_pipeline(&self.model_rendering_pipeline);

            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);

            for (model_name, (buffer, instance_count)) in &entity_buffers {
                render_pass.set_vertex_buffer(1, buffer.slice(..));

                if let Some(model) = self.loaded_models.get(model_name) {
                    for mesh in &model.meshes {
                        let material = &model.materials[mesh.material];
                        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.set_bind_group(0, &material.bind_group, &[]);
                        render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
                        render_pass.draw_indexed(
                            0..mesh.num_elements,
                            0,
                            0..*instance_count as u32,
                        );
                    }
                } else {
                    for mesh in &self.obj_model.meshes {
                        let material = &self.obj_model.materials[mesh.material];
                        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.set_bind_group(0, &material.bind_group, &[]);
                        render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
                        render_pass.draw_indexed(
                            0..mesh.num_elements,
                            0,
                            0..*instance_count as u32,
                        );
                    }
                }
            }

            let particle_render_data = self.ecs_world.get_particles_render_data();
            if !particle_render_data.is_empty() {
                let mut particles_by_type: std::collections::HashMap<u32, Vec<cgmath::Vector3<f32>>> = std::collections::HashMap::new();
                for (pos, block_type, _alpha) in &particle_render_data {
                    particles_by_type.entry(*block_type).or_insert_with(Vec::new).push(*pos);
                }

                let mut all_instances: Vec<InstanceRaw> = Vec::new();
                let mut draw_ranges: Vec<(u32, u32, u32)> = Vec::new();

                for (block_type, positions) in &particles_by_type {
                    if positions.is_empty() {
                        continue;
                    }
                    let start = all_instances.len() as u32;
                    for pos in positions {
                        all_instances.push(Instance {
                            position: *pos,
                            rotation: cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
                        }.to_raw());
                    }
                    draw_ranges.push((*block_type, start, positions.len() as u32));
                }

                if !all_instances.is_empty() {
                    if all_instances.len() > self.particle_instance_capacity {
                        self.particle_instance_capacity = (all_instances.len() as f32 * 1.5) as usize;
                        let new_size = (self.particle_instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64;
                        self.particle_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("Particle Instance Buffer Pool"),
                            size: new_size,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                    }

                    self.queue.write_buffer(&self.particle_instance_buffer, 0, bytemuck::cast_slice(&all_instances));

                    let instance_size = std::mem::size_of::<InstanceRaw>() as u64;

                    render_pass.set_pipeline(&self.particle_rendering_pipeline);
                    render_pass.set_index_buffer(self.particle_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.set_bind_group(0, &self.world.texture_atlas.diffuse_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.camera_bind_group, &[]);

                    for (block_type, start, count) in draw_ranges {
                        if let Some(vertex_buffer) = self.particle_vertex_buffers.get(&block_type) {
                            let byte_start = start as u64 * instance_size;
                            let byte_end = byte_start + count as u64 * instance_size;
                            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            render_pass.set_vertex_buffer(1, self.particle_instance_buffer.slice(byte_start..byte_end));
                            render_pass.draw_indexed(0..6, 0, 0..count);
                        }
                    }
                }
            }
        }

        {
            let mut wboit_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("WBOIT Transparent Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.wboit_accum_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.wboit_reveal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            wboit_pass.set_pipeline(&self.wboit_pipeline);
            wboit_pass.set_bind_group(0, &self.world.texture_atlas.diffuse_bind_group, &[]);
            wboit_pass.set_bind_group(1, &self.camera_bind_group, &[]);

            self.world
                .chunk_buffers
                .iter()
                .filter(|cb| {
                    cb.vertex_buffer.size() > 0
                        && cb.transparent_num_elements > 0
                        && cb.transparent_indices_buffer.is_some()
                })
                .for_each(|cb| {
                    wboit_pass.set_vertex_buffer(0, cb.vertex_buffer.slice(..));
                    wboit_pass.set_index_buffer(
                        cb.transparent_indices_buffer.as_ref().unwrap().slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    wboit_pass.draw_indexed(0..cb.transparent_num_elements, 0, 0..1);
                });
        }

        {
            let mut composite_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("WBOIT Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            composite_pass.set_pipeline(&self.composite_pipeline);
            composite_pass.set_bind_group(0, &self.composite_bind_group, &[]);
            composite_pass.draw(0..3, 0..1);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        let mut pending_mp_action = gui::MultiplayerAction::None;

        self.egui_renderer.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.window,
            &view,
            screen_descriptor,
            |ctx| {
                let screen_rect = ctx.content_rect();
                let screen_size = screen_rect.size();
                let center = screen_rect.center();

                if !self.player.cursor_locked {
                    let dir = self.player.camera.direction();
                    let chunks_loaded = self.world.chunks.len();
                    gui::draw_stats_window(ctx, &gui::GameStats {
                        pos_x: self.player.camera.position.x,
                        pos_y: self.player.camera.position.y,
                        pos_z: self.player.camera.position.z,
                        dir_x: dir.x,
                        dir_y: dir.y,
                        dir_z: dir.z,
                        selected_block: self.player.selected_block,
                        chunks_loaded,
                        cursor_locked: self.player.cursor_locked,
                    }, &mut self.world.render_distance);

                    pending_mp_action = self.multiplayer_panel.draw(ctx);
                }

                if self.player.show_inventory {
                    if let Some(slot) = gui::draw_inventory_window(ctx, INV_SIZE) {
                        self.player.set_hotbar_slot(slot);
                    }
                }

                gui::draw_hotbar(
                    ctx,
                    &self.player.hotbar,
                    self.player.selected_hotbar_slot,
                    center,
                    screen_size.y,
                );

                if self.player.cursor_locked {
                    gui::draw_crosshair(ctx, center);
                }
            },
        );

        self.handle_multiplayer_action(pending_mp_action);

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        if code == KeyCode::KeyP && is_pressed {
            if self.player.cursor_locked {
                self.unlock_cursor();
            } else {
                self.lock_cursor();
            }
            return;
        }

        if code == KeyCode::KeyX && is_pressed {
            let pos = self.ecs_world.get_player_position().unwrap_or_else(|| {
                let p = &self.player.camera.position;
                cgmath::Vector3::new(p.x, p.y, p.z)
            });
            let net_id = self.ecs_world.alloc_net_id();
            ecs::spawn_following_mob(
                &mut self.ecs_world.world,
                pos,
                "Creeper.obj".to_string(),
                net_id,
            );
            return;
        }
        
        if !self.player.process_keyboard(code, is_pressed) {
            match (code, is_pressed) {
                (KeyCode::Escape, true) => event_loop.exit(),
                _ => {}
            }
        }

        if is_pressed {
            if self.player.show_inventory {
                self.unlock_cursor();
            } else {
                self.lock_cursor();
            }
        }
    }
}

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    state: Option<State>,
    last_time: instant::Instant,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<State>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            #[cfg(target_arch = "wasm32")]
            proxy,
            state: None,
            last_time: instant::Instant::now(),
        }
    }
}

impl ApplicationHandler<State> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes().with_maximized(true).with_title("Bassicraft 2");

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            const CANVAS_ID: &str = "canvas";

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_element = canvas.unchecked_into();
            window_attributes = window_attributes.with_canvas(Some(html_canvas_element));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        window.set_cursor_visible(true);

        #[cfg(not(target_arch = "wasm32"))]
        {
            // If we are not on web we can use pollster to
            // await the
            self.state = Some(pollster::block_on(State::new(window)).unwrap());
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(
                        proxy
                            .send_event(
                                State::new(window)
                                    .await
                                    .expect("Unable to create canvas!!!")
                            )
                            .is_ok()
                    )
                });
            }
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: State) {
        #[cfg(target_arch = "wasm32")]
        {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size().width,
                event.window.inner_size().height,
            );
        }
        self.state = Some(event);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let state = if let Some(state) = &mut self.state {
            state
        } else {
            return;
        };
        match event {
            DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                if dx.abs() < 0.001 && dy.abs() < 0.001 {
                    return;
                }

                if state.player.cursor_locked {
                    state.player.camera_controller.handle_mouse(dx, dy);
                }
            }
            _ => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(canvas) => canvas,
            None => return,
        };

        let event_consumed = state.egui_renderer.handle_input(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let dt = self.last_time.elapsed();
                self.last_time = instant::Instant::now();
                state.update(dt);
                match state.render() {
                    Ok(_) => {}
                    // Reconfigure the surface if it's lost or outdated
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.window.inner_size();
                        state.resize(size.width, size.height);
                    }
                    Err(e) => {
                        log::error!("Unable to render {}", e);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                if !event_consumed {
                    state.handle_mouse_button(button, btn_state.is_pressed());
                }
            }
            // WindowEvent::MouseWheel { delta, .. } => {
            //     state.handle_mouse_scroll(&delta);
            // }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => state.handle_key(event_loop, code, key_state.is_pressed()),
            _ => {}
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App::new(
        #[cfg(target_arch = "wasm32")]
        &event_loop,
    );
    event_loop.run_app(&mut app)?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().unwrap_throw();

    Ok(())
}
