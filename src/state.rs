use std::{iter, sync::Arc};

use wgpu::util::DeviceExt;
use winit::{
    event::{MouseButton, MouseScrollDelta},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::Window,
};

use crate::game::{Game, GameStates};
use crate::instance::{INV_SIZE, Instance, InstanceRaw};
use crate::model::Vertex;

pub(crate) fn create_wboit_textures(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
) {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };

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

pub struct State {
    pub(crate) egui_renderer: crate::gui::EguiRenderer,

    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) is_surface_configured: bool,
    render_pipeline: wgpu::RenderPipeline,

    pub(crate) window: Arc<Window>,

    #[allow(dead_code)]
    diffuse_texture: crate::texture::Texture,
    diffuse_bind_group: wgpu::BindGroup,

    pub(crate) mouse_pressed: bool,

    model_rendering_pipeline: wgpu::RenderPipeline,
    particle_rendering_pipeline: wgpu::RenderPipeline,

    pub(crate) multiplayer_panel: crate::gui::MultiplayerPanel,

    wboit_accum_texture: wgpu::Texture,
    wboit_reveal_texture: wgpu::Texture,
    wboit_accum_view: wgpu::TextureView,
    wboit_reveal_view: wgpu::TextureView,

    wboit_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group: wgpu::BindGroup,
    composite_bind_group_layout: wgpu::BindGroupLayout,

    pub(crate) texture_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) camera_bind_group_layout: wgpu::BindGroupLayout,

    pub(crate) game: Option<Game>,
    pub(crate) game_state: GameStates,
}

impl State {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<State> {
        let size = window.inner_size();
        let max_size = 2048;
        let width = size.width.min(max_size).max(1);
        let height = size.height.min(max_size).max(1);

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
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
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
        let diffuse_texture = crate::texture::Texture::from_bytes(
            &device,
            &queue,
            diffuse_bytes,
            "texture_atlas.png",
        )
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
        let particle_shader =
            device.create_shader_module(wgpu::include_wgsl!("particle_shader.wgsl"));

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
                buffers: &[crate::block::BlockVertex::desc()],
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::texture::Texture::DEPTH_FORMAT,
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
                buffers: &[crate::block::BlockVertex::desc()],
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
                format: crate::texture::Texture::DEPTH_FORMAT,
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

        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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

        let egui_renderer = crate::gui::EguiRenderer::new(&device, config.format, None, 1, &window);

        let model_rendering_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Model Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &model_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[crate::model::ModelVertex::desc(), InstanceRaw::desc()],
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
                    format: crate::texture::Texture::DEPTH_FORMAT,
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

        let particle_rendering_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Particle Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &particle_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        crate::particles::ParticleVertex::desc(),
                        InstanceRaw::desc(),
                    ],
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
                    format: crate::texture::Texture::DEPTH_FORMAT,
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

        let mut state = Self {
            egui_renderer,
            surface,
            device,
            queue,
            config,
            is_surface_configured: false,
            render_pipeline,
            window,
            diffuse_texture,
            diffuse_bind_group,
            mouse_pressed: false,
            model_rendering_pipeline,
            particle_rendering_pipeline,
            multiplayer_panel: crate::gui::MultiplayerPanel::default(),
            wboit_accum_texture,
            wboit_reveal_texture,
            wboit_accum_view,
            wboit_reveal_view,
            wboit_pipeline,
            composite_pipeline,
            composite_bind_group,
            composite_bind_group_layout,
            texture_bind_group_layout,
            camera_bind_group_layout,
            game: None,
            game_state: GameStates::MainMenu,
        };
        // TODO call start_game() from the title screen instead of here :(
        state.start_game().await;
        Ok(state)
    }
}

impl State {
    pub(crate) async fn start_game(&mut self) {
        let game = Game::new(
            &self.device,
            &self.queue,
            &self.config,
            &self.texture_bind_group_layout,
            &self.camera_bind_group_layout,
            &mut self.egui_renderer,
            self.config.format,
        )
        .await
        .unwrap();
        self.game = Some(game);
        self.game_state = GameStates::InGame;
        self.lock_cursor();
    }

    #[allow(dead_code)]
    pub(crate) fn stop_game(&mut self) {
        self.game = None;
        self.game_state = GameStates::MainMenu;
        self.unlock_cursor();
    }
}

impl State {
    pub fn resize(&mut self, width: u32, height: u32) {
        let max_size = 2048;
        let width = width.min(max_size);
        let height = height.min(max_size);

        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;

            if let Some(game) = &mut self.game {
                game.player.projection.resize(width, height);
                game.depth_texture = crate::texture::Texture::create_depth_texture(
                    &self.device,
                    &self.config,
                    "depth_texture",
                );
            }

            let (accum_tex, accum_view, reveal_tex, reveal_view) =
                create_wboit_textures(&self.device, width, height);
            self.wboit_accum_texture = accum_tex;
            self.wboit_accum_view = accum_view;
            self.wboit_reveal_texture = reveal_tex;
            self.wboit_reveal_view = reveal_view;

            let wboit_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("WBOIT Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            self.composite_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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

    pub(crate) fn handle_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        match button {
            MouseButton::Left => {
                self.mouse_pressed = pressed;
                if pressed {
                    let show_inventory = self
                        .game
                        .as_ref()
                        .map(|g| g.player.show_inventory)
                        .unwrap_or(true);
                    if self.game.is_some() && !show_inventory {
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            let window = web_sys::window().unwrap();
                            let document = window.document().unwrap();
                            let canvas = document.get_element_by_id("canvas").unwrap();
                            let html_canvas: web_sys::HtmlCanvasElement =
                                canvas.dyn_into().unwrap();
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

                        if let Some(game) = &mut self.game {
                            if let Some(pos) = game.player.get_block_pointed_at(&game.world.chunks)
                            {
                                if let Some(block_type) =
                                    game.world.break_block(&self.device, &self.queue, pos)
                                {
                                    if block_type != 0 {
                                        if let Some(client) = &game.net_client {
                                            client.send(
                                                &crate::network::ClientMessage::BreakBlock {
                                                    x: pos[0],
                                                    y: pos[1],
                                                    z: pos[2],
                                                },
                                            );
                                        }
                                        #[cfg(not(target_arch = "wasm32"))]
                                        if let Some(server) = &game.net_server {
                                            server.broadcast(
                                                &crate::network::ServerMessage::BlockUpdate {
                                                    x: pos[0],
                                                    y: pos[1],
                                                    z: pos[2],
                                                    block_type: 0,
                                                },
                                            );
                                        }

                                        for _ in 0..8 {
                                            let r1 =
                                                crate::random::get_random_f32_normalized().unwrap();
                                            let r2 =
                                                crate::random::get_random_f32_normalized().unwrap();
                                            let r3 =
                                                crate::random::get_random_f32_normalized().unwrap();
                                            let r4 =
                                                crate::random::get_random_f32_normalized().unwrap();
                                            let r5 =
                                                crate::random::get_random_f32_normalized().unwrap();
                                            let r6 =
                                                crate::random::get_random_f32_normalized().unwrap();

                                            let particle_pos = cgmath::Vector3::new(
                                                pos[0] as f32 + 0.5 + r1 * 0.6 - 0.3,
                                                pos[1] as f32 + 0.5 + r2 * 0.6 - 0.3,
                                                pos[2] as f32 + 0.5 + r3 * 0.6 - 0.3,
                                            );
                                            let velocity = cgmath::Vector3::new(
                                                r4 * 4.0 - 2.0,
                                                r5 * 3.0 + 2.0,
                                                r6 * 4.0 - 2.0,
                                            );
                                            crate::ecs::spawn_particle(
                                                &mut game.ecs_world.world,
                                                particle_pos,
                                                block_type - 1,
                                                velocity,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            MouseButton::Right => {
                self.mouse_pressed = pressed;
                if pressed {
                    if let Some(game) = &mut self.game {
                        if let Some(pos) = game.player.get_block_placement_pos(&game.world.chunks) {
                            let block_type = game.player.selected_block;
                            game.world
                                .place_block(&self.device, &self.queue, pos, block_type);

                            if let Some(client) = &game.net_client {
                                client.send(&crate::network::ClientMessage::PlaceBlock {
                                    x: pos[0],
                                    y: pos[1],
                                    z: pos[2],
                                    block_type,
                                });
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            if let Some(server) = &game.net_server {
                                server.broadcast(&crate::network::ServerMessage::BlockUpdate {
                                    x: pos[0],
                                    y: pos[1],
                                    z: pos[2],
                                    block_type,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    pub(crate) fn handle_mouse_scroll(&mut self, delta: &MouseScrollDelta) {
        if let Some(game) = &mut self.game {
            game.player.camera_controller.handle_mouse_scroll(delta);
        }
    }

    pub(crate) fn lock_cursor(&mut self) {
        if let Some(game) = &mut self.game {
            game.player.cursor_locked = true;
        }
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
            let _ = self
                .window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_e| self.window.set_cursor_grab(CursorGrabMode::Locked));
        }
    }

    pub(crate) fn unlock_cursor(&mut self) {
        if let Some(game) = &mut self.game {
            game.player.cursor_locked = false;
        }
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
}

impl State {
    pub(crate) fn update(&mut self, dt: instant::Duration) {
        let Some(game) = &mut self.game else {
            return;
        };
        let dt_secs = dt.as_secs_f32().min(0.1);

        game.player
            .camera_controller
            .update_camera(&mut game.player.camera, dt);

        game.ecs_world.update_player_input(
            game.player.camera_controller.amount_forward,
            game.player.camera_controller.amount_backward,
            game.player.camera_controller.amount_left,
            game.player.camera_controller.amount_right,
            game.player.camera_controller.amount_up > 0.0,
        );

        let camera_yaw = game.player.camera.yaw().0;
        game.ecs_world
            .update(dt_secs, &game.world.chunks, camera_yaw);

        if let Some(player_pos) = game.ecs_world.get_player_position() {
            game.player.camera.position =
                cgmath::Point3::new(player_pos.x, player_pos.y + 1.6, player_pos.z);
        }

        game.camera_uniform
            .update_view_proj(&game.player.camera, &game.player.projection);
        self.queue.write_buffer(
            &game.camera_buffer,
            0,
            bytemuck::cast_slice(&[game.camera_uniform]),
        );

        const NET_TICK_RATE: f32 = 1.0 / 20.0;

        let needs_network_tick = {
            game.net_tick_accumulator += dt_secs;
            if game.net_tick_accumulator >= NET_TICK_RATE {
                game.net_tick_accumulator = 0.0;
                true
            } else {
                false
            }
        };

        let player_pos_chunk = {
            let p = &game.player.camera.position;
            [
                (p.x / crate::chunk::CHUNK_X_SIZE as f32).floor() as i32,
                (p.z / crate::chunk::CHUNK_Z_SIZE as f32).floor() as i32,
            ]
        };

        if needs_network_tick {
            self.network_tick();
        }

        if let Some(game) = &mut self.game {
            game.world
                .update_chunks(&self.device, &self.queue, player_pos_chunk);
        }
    }
}

impl State {
    pub(crate) fn handle_multiplayer_action(&mut self, action: crate::gui::MultiplayerAction) {
        let Some(game) = &mut self.game else {
            return;
        };
        match action {
            crate::gui::MultiplayerAction::Host { port } => {
                #[cfg(not(target_arch = "wasm32"))]
                match crate::network::NetServer::start(port) {
                    Ok(server) => {
                        self.multiplayer_panel.lan_address = Some(server.lan_address.clone());
                        self.multiplayer_panel.status = format!("Hosting on port {}", port);
                        self.multiplayer_panel.is_hosting = true;
                        match crate::network::NetClient::connect(&format!(
                            "ws://127.0.0.1:{}",
                            port
                        )) {
                            Ok(client) => {
                                game.net_client = Some(client);
                            }
                            Err(e) => log::error!("Host self-connect failed: {}", e),
                        }
                        game.net_server = Some(server);
                        if let Some(srv) = &game.net_server {
                            let positions: Vec<[i32; 2]> =
                                game.world.chunks.iter().map(|c| c.pos).collect();
                            srv.broadcast(&crate::network::ServerMessage::AvailableChunks(
                                positions,
                            ));
                        }
                    }
                    Err(e) => {
                        self.multiplayer_panel.status = format!("Host failed: {}", e);
                    }
                }
            }
            crate::gui::MultiplayerAction::Join { url } => {
                match crate::network::NetClient::connect(&url) {
                    Ok(client) => {
                        game.net_client = Some(client);
                        self.multiplayer_panel.status = format!("Connecting to {}\u{2026}", url);
                    }
                    Err(e) => {
                        self.multiplayer_panel.status = format!("Connect failed: {}", e);
                    }
                }
            }
            crate::gui::MultiplayerAction::Disconnect => {
                game.net_client = None;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    game.net_server = None;
                }
                game.ecs_world.clear_remote_entities();
                self.multiplayer_panel.is_connected = false;
                self.multiplayer_panel.is_hosting = false;
                self.multiplayer_panel.lan_address = None;
                self.multiplayer_panel.status = "Disconnected".to_string();
            }
            crate::gui::MultiplayerAction::RequestChunks => {
                let msg = std::mem::take(&mut self.multiplayer_panel.chat_input);
                if !msg.is_empty() {
                    if let Some(client) = &game.net_client {
                        client.send(&crate::network::ClientMessage::Chat { message: msg });
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(server) = &game.net_server {
                        let my_id = game.net_client.as_ref().and_then(|c| c.my_id).unwrap_or(0);
                        server.broadcast(&crate::network::ServerMessage::Chat {
                            sender_id: my_id,
                            message: std::mem::take(&mut self.multiplayer_panel.chat_input),
                        });
                    }
                }
            }
            crate::gui::MultiplayerAction::None => {}
        }
    }

    fn network_tick(&mut self) {
        let Some(game) = &mut self.game else {
            return;
        };

        if let Some(client) = &game.net_client {
            let cam = &game.player.camera;
            let pos = game.ecs_world.get_player_position().unwrap_or_else(|| {
                cgmath::Vector3::new(cam.position.x, cam.position.y, cam.position.z)
            });
            client.send(&crate::network::ClientMessage::PlayerInput {
                forward: game.player.camera_controller.amount_forward,
                backward: game.player.camera_controller.amount_backward,
                left: game.player.camera_controller.amount_left,
                right: game.player.camera_controller.amount_right,
                jump: game.player.camera_controller.amount_up > 0.0,
                yaw: cam.yaw().0,
                pitch: cam.pitch().0,
                position: crate::network::Vec3Net::new(pos.x, pos.y, pos.z),
            });
        }

        let server_msgs: Vec<crate::network::ServerMessage> =
            if let Some(client) = &mut game.net_client {
                client.poll()
            } else {
                Vec::new()
            };

        for msg in server_msgs {
            match msg {
                crate::network::ServerMessage::Welcome { player_id, spawn } => {
                    self.multiplayer_panel.on_connected(player_id);
                    log::info!("Welcome! My player ID: {}, spawn: {:?}", player_id, spawn);
                    let game = self.game.as_mut().unwrap();
                    let is_host = {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            game.net_server.is_some()
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            false
                        }
                    };
                    if !is_host {
                        game.ecs_world.clear_local_mobs();
                    }
                }
                crate::network::ServerMessage::PlayerStates(states) => {
                    let game = self.game.as_mut().unwrap();
                    let my_id = game.net_client.as_ref().and_then(|c| c.my_id);
                    game.ecs_world.sync_remote_players(&states, my_id);
                }
                crate::network::ServerMessage::EntityStates(states) => {
                    let game = self.game.as_mut().unwrap();
                    let is_host = {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            game.net_server.is_some()
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            false
                        }
                    };
                    if !is_host {
                        game.ecs_world.sync_network_entities(&states);
                    }
                }
                crate::network::ServerMessage::BlockUpdate {
                    x,
                    y,
                    z,
                    block_type,
                } => {
                    let game = self.game.as_mut().unwrap();
                    if block_type == 0 {
                        game.world.break_block(&self.device, &self.queue, [x, y, z]);
                    } else {
                        game.world
                            .place_block(&self.device, &self.queue, [x, y, z], block_type);
                    }
                }
                crate::network::ServerMessage::ChunkData { cx, cz, blocks } => {
                    let game = self.game.as_mut().unwrap();
                    let mut i = 0;
                    while i + 3 < blocks.len() {
                        let lx = blocks[i] as i32;
                        let ly = blocks[i + 1] as i32;
                        let lz = blocks[i + 2] as i32;
                        let mat = blocks[i + 3];
                        let world_x = cx * 16 + lx;
                        let world_z = cz * 16 + lz;
                        if mat != 0 {
                            game.world.place_block(
                                &self.device,
                                &self.queue,
                                [world_x, ly, world_z],
                                mat,
                            );
                        }
                        i += 4;
                    }
                }
                crate::network::ServerMessage::AvailableChunks(positions) => {
                    let game = self.game.as_ref().unwrap();
                    if let Some(client) = &game.net_client {
                        for [cx, cz] in positions {
                            client.send(&crate::network::ClientMessage::RequestChunk { cx, cz });
                        }
                    }
                }
                crate::network::ServerMessage::PlayerLeft { player_id } => {
                    let game = self.game.as_mut().unwrap();
                    log::info!("Player {} left", player_id);
                    game.ecs_world.remove_remote_player(player_id);
                }
                crate::network::ServerMessage::Chat { sender_id, message } => {
                    self.multiplayer_panel.push_chat(sender_id, message);
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let game = self.game.as_mut().unwrap();
            if let Some(server) = &game.net_server {
                let events = server.poll_incoming();
                for ev in events {
                    match ev.message {
                        crate::network::ClientMessage::BreakBlock { x, y, z } => {
                            game.world.break_block(&self.device, &self.queue, [x, y, z]);
                            server.broadcast(&crate::network::ServerMessage::BlockUpdate {
                                x,
                                y,
                                z,
                                block_type: 0,
                            });
                        }
                        crate::network::ClientMessage::PlaceBlock {
                            x,
                            y,
                            z,
                            block_type,
                        } => {
                            game.world.place_block(
                                &self.device,
                                &self.queue,
                                [x, y, z],
                                block_type,
                            );
                            server.broadcast(&crate::network::ServerMessage::BlockUpdate {
                                x,
                                y,
                                z,
                                block_type,
                            });
                        }
                        crate::network::ClientMessage::RequestChunk { cx, cz } => {
                            if let Some(chunk) =
                                game.world.chunks.iter().find(|c| c.pos == [cx, cz])
                            {
                                let blocks = crate::network::serialize_chunk_blocks(chunk);
                                server.send_to_client(
                                    ev.player_id,
                                    &crate::network::ServerMessage::ChunkData { cx, cz, blocks },
                                );
                            }
                        }
                        crate::network::ClientMessage::Chat { message } => {
                            server.broadcast(&crate::network::ServerMessage::Chat {
                                sender_id: ev.player_id,
                                message,
                            });
                        }
                        crate::network::ClientMessage::PlayerInput { .. } => {
                            let states = server.player_states_snapshot();
                            server.broadcast(&crate::network::ServerMessage::PlayerStates(states));

                            let entity_states = game.ecs_world.get_networked_entities_data();
                            if !entity_states.is_empty() {
                                server.broadcast(&crate::network::ServerMessage::EntityStates(
                                    entity_states,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

impl State {
    pub(crate) fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.window.request_redraw();

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

        if let Some(game) = &mut self.game {
            let entity_render_data = game.ecs_world.get_entities_render_data();
            let mut entities_by_model: std::collections::HashMap<String, Vec<InstanceRaw>> =
                std::collections::HashMap::new();
            for (pos, rot, model_name) in entity_render_data {
                let raw = Instance {
                    position: pos,
                    rotation: rot,
                }
                .to_raw();
                entities_by_model
                    .entry(model_name)
                    .or_insert_with(Vec::new)
                    .push(raw);
            }
            let instance_size = std::mem::size_of::<InstanceRaw>() as u64;
            for (model_name, instances) in &entities_by_model {
                if instances.is_empty() {
                    continue;
                }
                let needed = instances.len();
                let entry = game
                    .entity_instance_buffers
                    .entry(model_name.clone())
                    .or_insert_with(|| {
                        let cap = (needed as f32 * 1.5) as usize;
                        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some(&format!("Entity Instance Buffer: {}", model_name)),
                            size: cap as u64 * instance_size,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        (buf, cap)
                    });
                if needed > entry.1 {
                    entry.1 = (needed as f32 * 1.5) as usize;
                    entry.0 = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("Entity Instance Buffer: {}", model_name)),
                        size: entry.1 as u64 * instance_size,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }
                self.queue
                    .write_buffer(&entry.0, 0, bytemuck::cast_slice(instances));
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
                        view: &game.depth_texture.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &game.world.texture_atlas.diffuse_bind_group, &[]);
                render_pass.set_bind_group(1, &game.camera_bind_group, &[]);

                game.world
                    .chunk_buffers
                    .iter()
                    .filter(|cb| cb.vertex_buffer.size() > 0 && cb.indices_buffer.size() > 0)
                    .for_each(|cb| {
                        render_pass.set_vertex_buffer(0, cb.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(
                            cb.indices_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(0..cb.num_elements, 0, 0..1);
                    });

                render_pass.set_pipeline(&self.model_rendering_pipeline);
                render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
                render_pass.set_bind_group(1, &game.camera_bind_group, &[]);

                for (model_name, instances) in &entities_by_model {
                    if instances.is_empty() {
                        continue;
                    }
                    let instance_count = instances.len();
                    let Some((buffer, _)) = game.entity_instance_buffers.get(model_name) else {
                        continue;
                    };
                    render_pass.set_vertex_buffer(1, buffer.slice(..));
                    if let Some(model) = game.loaded_models.get(model_name) {
                        for mesh in &model.meshes {
                            let material = &model.materials[mesh.material];
                            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                            render_pass.set_index_buffer(
                                mesh.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            render_pass.set_bind_group(0, &material.bind_group, &[]);
                            render_pass.set_bind_group(1, &game.camera_bind_group, &[]);
                            render_pass.draw_indexed(
                                0..mesh.num_elements,
                                0,
                                0..instance_count as u32,
                            );
                        }
                    } else {
                        for mesh in &game.obj_model.meshes {
                            let material = &game.obj_model.materials[mesh.material];
                            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                            render_pass.set_index_buffer(
                                mesh.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            render_pass.set_bind_group(0, &material.bind_group, &[]);
                            render_pass.set_bind_group(1, &game.camera_bind_group, &[]);
                            render_pass.draw_indexed(
                                0..mesh.num_elements,
                                0,
                                0..instance_count as u32,
                            );
                        }
                    }
                }

                let particle_render_data = game.ecs_world.get_particles_render_data();
                if !particle_render_data.is_empty() {
                    let mut particles_by_type: std::collections::HashMap<
                        u32,
                        Vec<cgmath::Vector3<f32>>,
                    > = std::collections::HashMap::new();
                    for (pos, block_type, _alpha) in &particle_render_data {
                        particles_by_type
                            .entry(*block_type)
                            .or_insert_with(Vec::new)
                            .push(*pos);
                    }

                    let mut all_instances: Vec<InstanceRaw> = Vec::new();
                    let mut draw_ranges: Vec<(u32, u32, u32)> = Vec::new();

                    for (block_type, positions) in &particles_by_type {
                        if positions.is_empty() {
                            continue;
                        }
                        let start = all_instances.len() as u32;
                        for pos in positions {
                            all_instances.push(
                                Instance {
                                    position: *pos,
                                    rotation: cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
                                }
                                .to_raw(),
                            );
                        }
                        draw_ranges.push((*block_type, start, positions.len() as u32));
                    }

                    if !all_instances.is_empty() {
                        if all_instances.len() > game.particle_instance_capacity {
                            game.particle_instance_capacity =
                                (all_instances.len() as f32 * 1.5) as usize;
                            let new_size = (game.particle_instance_capacity
                                * std::mem::size_of::<InstanceRaw>())
                                as u64;
                            game.particle_instance_buffer =
                                self.device.create_buffer(&wgpu::BufferDescriptor {
                                    label: Some("Particle Instance Buffer Pool"),
                                    size: new_size,
                                    usage: wgpu::BufferUsages::VERTEX
                                        | wgpu::BufferUsages::COPY_DST,
                                    mapped_at_creation: false,
                                });
                        }
                        self.queue.write_buffer(
                            &game.particle_instance_buffer,
                            0,
                            bytemuck::cast_slice(&all_instances),
                        );
                        let instance_size = std::mem::size_of::<InstanceRaw>() as u64;

                        render_pass.set_pipeline(&self.particle_rendering_pipeline);
                        render_pass.set_index_buffer(
                            game.particle_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.set_bind_group(
                            0,
                            &game.world.texture_atlas.diffuse_bind_group,
                            &[],
                        );
                        render_pass.set_bind_group(1, &game.camera_bind_group, &[]);

                        for (block_type, start, count) in draw_ranges {
                            if let Some(vertex_buffer) =
                                game.particle_vertex_buffers.get(&block_type)
                            {
                                let byte_start = start as u64 * instance_size;
                                let byte_end = byte_start + count as u64 * instance_size;
                                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                                render_pass.set_vertex_buffer(
                                    1,
                                    game.particle_instance_buffer.slice(byte_start..byte_end),
                                );
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
                        view: &game.depth_texture.view,
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
                wboit_pass.set_bind_group(0, &game.world.texture_atlas.diffuse_bind_group, &[]);
                wboit_pass.set_bind_group(1, &game.camera_bind_group, &[]);

                game.world
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
        } else {
            // TODO: When no game, put main menu / options / whatever
            let mut _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Menu Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
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

        let mut pending_mp_action = crate::gui::MultiplayerAction::None;

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

                if let Some(game) = &mut self.game {
                    if !game.player.cursor_locked {
                        let dir = game.player.camera.direction();
                        let chunks_loaded = game.world.chunks.len();
                        crate::gui::draw_stats_window(
                            ctx,
                            &crate::gui::GameStats {
                                pos_x: game.player.camera.position.x,
                                pos_y: game.player.camera.position.y,
                                pos_z: game.player.camera.position.z,
                                dir_x: dir.x,
                                dir_y: dir.y,
                                dir_z: dir.z,
                                selected_block: game.player.selected_block,
                                chunks_loaded,
                                cursor_locked: game.player.cursor_locked,
                            },
                            &mut game.world.render_distance,
                        );

                        pending_mp_action = self.multiplayer_panel.draw(ctx);
                    }

                    if game.player.show_inventory {
                        if let Some(slot) = crate::gui::draw_inventory_window(ctx, INV_SIZE) {
                            game.player.set_hotbar_slot(slot);
                        }
                    }

                    crate::gui::draw_hotbar(
                        ctx,
                        &game.player.hotbar,
                        game.player.selected_hotbar_slot,
                        center,
                        screen_size.y,
                    );

                    if game.player.cursor_locked {
                        crate::gui::draw_crosshair(ctx, center);
                    }
                }
            },
        );

        self.handle_multiplayer_action(pending_mp_action);

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl State {
    pub(crate) fn handle_key(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        is_pressed: bool,
    ) {
        if code == KeyCode::KeyP && is_pressed {
            let cursor_locked = self
                .game
                .as_ref()
                .map(|g| g.player.cursor_locked)
                .unwrap_or(false);
            if cursor_locked {
                self.unlock_cursor();
            } else {
                self.lock_cursor();
            }
            return;
        }

        if code == KeyCode::KeyX && is_pressed {
            if let Some(game) = &mut self.game {
                let pos = game.ecs_world.get_player_position().unwrap_or_else(|| {
                    let p = &game.player.camera.position;
                    cgmath::Vector3::new(p.x, p.y, p.z)
                });
                let net_id = game.ecs_world.alloc_net_id();
                crate::ecs::spawn_following_mob(
                    &mut game.ecs_world.world,
                    pos,
                    "Creeper.obj".to_string(),
                    net_id,
                );
            }
            return;
        }

        let processed = self
            .game
            .as_mut()
            .map(|g| g.player.process_keyboard(code, is_pressed))
            .unwrap_or(false);

        if !processed {
            match (code, is_pressed) {
                (KeyCode::Escape, true) => event_loop.exit(),
                _ => {}
            }
        }

        if is_pressed {
            let show_inventory = self
                .game
                .as_ref()
                .map(|g| g.player.show_inventory)
                .unwrap_or(false);
            if show_inventory {
                self.unlock_cursor();
            } else if self.game.is_some() {
                self.lock_cursor();
            }
        }
    }
}
