use anyhow::*;

use crate::{texture};

const BLOCK_TEXTURE_SIZE: u8 = 16;
const TEXTURE_ATLAS_X_SIZE: u8 = 16;
const TEXTURE_ATLAS_Y_SIZE: u8 = 16;

const BLOCK_TEXTURE_SIZE_TOTAL: f32 = 1.0 / BLOCK_TEXTURE_SIZE as f32;

#[derive(Clone, Debug)]
pub struct TextureAtlas {
    pub diffuse_texure: texture::Texture,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub diffuse_bind_group: wgpu::BindGroup,
}

impl TextureAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let diffuse_bytes = include_bytes!("../res/texture_atlas.png");
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

        Self {
            diffuse_texure: diffuse_texture,
            texture_bind_group_layout: texture_bind_group_layout,
            diffuse_bind_group: diffuse_bind_group,
        }
    }

    pub fn get_block_texture_from_type(mat: u32) -> [[f32; 2]; 4] {
        let x = (mat % TEXTURE_ATLAS_X_SIZE as u32) as f32;
        let y = (mat / TEXTURE_ATLAS_X_SIZE as u32) as f32;

        let tex_size = BLOCK_TEXTURE_SIZE_TOTAL;

        [
            [x * tex_size,         y * tex_size],
            [(x + 1.0) * tex_size, y * tex_size],
            [(x + 1.0) * tex_size, (y + 1.0) * tex_size],
            [x * tex_size,         (y + 1.0) * tex_size],
        ]
    }
}
