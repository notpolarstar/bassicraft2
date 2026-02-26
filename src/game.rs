use cgmath::{Deg, Matrix4, Point3, Vector3, perspective};
use wgpu::util::DeviceExt;

use crate::instance::{INV_SIZE, InstanceRaw};
use crate::model::Vertex;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CameraUniform {
    pub(crate) view_position: [f32; 4],
    pub(crate) view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub(crate) fn new() -> Self {
        use cgmath::SquareMatrix;
        Self {
            view_position: [0.0; 4],
            view_proj: cgmath::Matrix4::identity().into(),
        }
    }

    pub(crate) fn update_view_proj(
        &mut self,
        camera: &crate::camera::Camera,
        projection: &crate::camera::Projection,
    ) {
        self.view_position = camera.position.to_homogeneous().into();
        self.view_proj = (projection.calc_matrix() * camera.calc_matrix()).into();
    }
}

pub enum GameStates {
    MainMenu,
    Options,
    WorldSelection,
    InGame,
    PauseMenu,
}

pub(crate) struct GameEguiResources {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) texture_bind_group: wgpu::BindGroup,
    pub(crate) ui_camera_bind_group: wgpu::BindGroup,
    pub(crate) block_meshes: Vec<(wgpu::Buffer, wgpu::Buffer, u32)>,
}

pub struct Game {
    pub(crate) player: crate::player::Player,
    pub(crate) camera_uniform: CameraUniform,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) camera_bind_group: wgpu::BindGroup,
    pub(crate) depth_texture: crate::texture::Texture,
    pub(crate) obj_model: crate::model::Model,
    pub(crate) loaded_models: std::collections::HashMap<String, crate::model::Model>,
    pub(crate) world: crate::world::World,
    pub(crate) ecs_world: crate::ecs::EcsWorld,
    pub(crate) particle_vertex_buffers: std::collections::HashMap<u32, wgpu::Buffer>,
    pub(crate) particle_index_buffer: wgpu::Buffer,
    pub(crate) particle_instance_buffer: wgpu::Buffer,
    pub(crate) particle_instance_capacity: usize,
    pub(crate) entity_instance_buffers: std::collections::HashMap<String, (wgpu::Buffer, usize)>,
    pub(crate) net_client: Option<crate::network::NetClient>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) net_server: Option<crate::network::NetServer>,
    pub(crate) net_tick_accumulator: f32,
}

impl Game {
    pub(crate) async fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> anyhow::Result<(Game, GameEguiResources)> {
        let player = crate::player::Player::new([0.0, 100.0, 10.0], config);

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&player.camera, &player.projection);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let depth_texture =
            crate::texture::Texture::create_depth_texture(device, config, "depth_texture");

        let obj_model =
            crate::resources::load_model("steve.obj", device, queue, texture_bind_group_layout)
                .await
                .unwrap();

        let mut loaded_models = std::collections::HashMap::new();

        // TEMP. LOAD MODELS DYNAMICALLY LATER !!!!!!!
        if let Ok(m) = crate::resources::load_model("cube.obj", device, queue, texture_bind_group_layout).await {
            loaded_models.insert("cube.obj".to_string(), m);
        }
        if let Ok(m) = crate::resources::load_model("steve.obj", device, queue, texture_bind_group_layout).await {
            loaded_models.insert("steve.obj".to_string(), m);
        }
        if let Ok(m) = crate::resources::load_model("Creeper.obj", device, queue, texture_bind_group_layout).await {
            loaded_models.insert("Creeper.obj".to_string(), m);
        }

        let world = crate::world::World::new(device, queue, 0x1f6c2);

        const MAX_PARTICLES: usize = 1000;

        let particle_indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        let particle_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Index Buffer Pool"),
            contents: bytemuck::cast_slice(&particle_indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        let particle_instance_buffer_size =
            (MAX_PARTICLES * std::mem::size_of::<InstanceRaw>()) as u64;
        let particle_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Instance Buffer Pool"),
            size: particle_instance_buffer_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut particle_vertex_buffers = std::collections::HashMap::new();
        for block_type in 0..INV_SIZE {
            use crate::particles::ParticleVertex;
            let tex_x = ((block_type % 16) as f32) / 16.0;
            let tex_y = ((block_type / 16) as f32) / 16.0;
            let tex_size = 1.0 / 16.0;

            let particle_vertices = vec![
                ParticleVertex { position: [-0.5, -0.5, 0.0], tex_coords: [tex_x, tex_y + tex_size] },
                ParticleVertex { position: [ 0.5, -0.5, 0.0], tex_coords: [tex_x + tex_size, tex_y + tex_size] },
                ParticleVertex { position: [ 0.5,  0.5, 0.0], tex_coords: [tex_x + tex_size, tex_y] },
                ParticleVertex { position: [-0.5,  0.5, 0.0], tex_coords: [tex_x, tex_y] },
            ];

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Particle Vertex Buffer {}", block_type)),
                contents: bytemuck::cast_slice(&particle_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            particle_vertex_buffers.insert(block_type, vertex_buffer);
        }

        use crate::block::BlockVertex;

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("UI Render Pipeline Layout"),
                bind_group_layouts: &[texture_bind_group_layout, camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let ui_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI Block Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[BlockVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
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

        let mut block_meshes = Vec::new();
        for block_type in 0..INV_SIZE {
            let tex_x = (block_type % 16) as f32 / 16.0;
            let tex_y = (block_type / 16) as f32 / 16.0;
            let half = 0.5 / (16.0 * 16.0_f32);
            let tc = [tex_x + half, tex_y + half, tex_x + 0.0625 - half, tex_y + 0.0625 - half];

            let vertices = vec![
                BlockVertex { position: [-0.5, -0.5,  0.5], packed: BlockVertex::pack(tc[0], tc[3], false) },
                BlockVertex { position: [ 0.5, -0.5,  0.5], packed: BlockVertex::pack(tc[2], tc[3], false) },
                BlockVertex { position: [ 0.5,  0.5,  0.5], packed: BlockVertex::pack(tc[2], tc[1], false) },
                BlockVertex { position: [-0.5,  0.5,  0.5], packed: BlockVertex::pack(tc[0], tc[1], false) },
                BlockVertex { position: [ 0.5, -0.5, -0.5], packed: BlockVertex::pack(tc[0], tc[3], false) },
                BlockVertex { position: [-0.5, -0.5, -0.5], packed: BlockVertex::pack(tc[2], tc[3], false) },
                BlockVertex { position: [-0.5,  0.5, -0.5], packed: BlockVertex::pack(tc[2], tc[1], false) },
                BlockVertex { position: [ 0.5,  0.5, -0.5], packed: BlockVertex::pack(tc[0], tc[1], false) },
                BlockVertex { position: [-0.5, -0.5, -0.5], packed: BlockVertex::pack(tc[0], tc[3], false) },
                BlockVertex { position: [-0.5, -0.5,  0.5], packed: BlockVertex::pack(tc[2], tc[3], false) },
                BlockVertex { position: [-0.5,  0.5,  0.5], packed: BlockVertex::pack(tc[2], tc[1], false) },
                BlockVertex { position: [-0.5,  0.5, -0.5], packed: BlockVertex::pack(tc[0], tc[1], false) },
                BlockVertex { position: [ 0.5, -0.5,  0.5], packed: BlockVertex::pack(tc[0], tc[3], false) },
                BlockVertex { position: [ 0.5, -0.5, -0.5], packed: BlockVertex::pack(tc[2], tc[3], false) },
                BlockVertex { position: [ 0.5,  0.5, -0.5], packed: BlockVertex::pack(tc[2], tc[1], false) },
                BlockVertex { position: [ 0.5,  0.5,  0.5], packed: BlockVertex::pack(tc[0], tc[1], false) },
                BlockVertex { position: [-0.5,  0.5,  0.5], packed: BlockVertex::pack(tc[0], tc[3], false) },
                BlockVertex { position: [ 0.5,  0.5,  0.5], packed: BlockVertex::pack(tc[2], tc[3], false) },
                BlockVertex { position: [ 0.5,  0.5, -0.5], packed: BlockVertex::pack(tc[2], tc[1], false) },
                BlockVertex { position: [-0.5,  0.5, -0.5], packed: BlockVertex::pack(tc[0], tc[1], false) },
                BlockVertex { position: [-0.5, -0.5, -0.5], packed: BlockVertex::pack(tc[0], tc[3], false) },
                BlockVertex { position: [ 0.5, -0.5, -0.5], packed: BlockVertex::pack(tc[2], tc[3], false) },
                BlockVertex { position: [ 0.5, -0.5,  0.5], packed: BlockVertex::pack(tc[2], tc[1], false) },
                BlockVertex { position: [-0.5, -0.5,  0.5], packed: BlockVertex::pack(tc[0], tc[1], false) },
            ];

            let indices: Vec<u32> = (0..6u32)
                .flat_map(|i| {
                    let b = i * 4;
                    [b, b + 1, b + 2, b + 2, b + 3, b]
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

        let ui_camera_pos = Vector3::new(1.5f32, 1.5, 1.5);
        let ui_view = Matrix4::look_at_rh(
            Point3::new(ui_camera_pos.x, ui_camera_pos.y, ui_camera_pos.z),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::unit_y(),
        );
        let ui_proj = perspective(Deg(45.0f32), 1.0, 0.1, 100.0);
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
            layout: camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ui_camera_buffer.as_entire_binding(),
            }],
        });

        let egui_resources = GameEguiResources {
            pipeline: ui_render_pipeline,
            texture_bind_group: world.texture_atlas.diffuse_bind_group.clone(),
            ui_camera_bind_group,
            block_meshes,
        };

        let mut ecs_world = crate::ecs::EcsWorld::new();
        let player_start_pos = cgmath::Vector3::new(0.0, 100.0, 10.0);
        crate::ecs::spawn_player(&mut ecs_world.world, player_start_pos);

        for i in 0..5 {
            let net_id = ecs_world.alloc_net_id();
            crate::ecs::spawn_wandering_mob(
                &mut ecs_world.world,
                cgmath::Vector3::new(5.0 + i as f32 * 2.0, 95.0, 5.0),
                "cube.obj".to_string(),
                net_id,
            );
        }
        for i in 0..5 {
            let net_id = ecs_world.alloc_net_id();
            crate::ecs::spawn_following_mob(
                &mut ecs_world.world,
                cgmath::Vector3::new(-5.0 - i as f32 * 2.0, 95.0, -5.0),
                "Creeper.obj".to_string(),
                net_id,
            );
        }

        Ok((Self {
            player,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            depth_texture,
            obj_model,
            loaded_models,
            world,
            ecs_world,
            particle_vertex_buffers,
            particle_index_buffer,
            particle_instance_buffer,
            particle_instance_capacity: MAX_PARTICLES,
            entity_instance_buffers: std::collections::HashMap::new(),
            net_client: None,
            #[cfg(not(target_arch = "wasm32"))]
            net_server: None,
            net_tick_accumulator: 0.0,
        }, egui_resources))
    }
}
