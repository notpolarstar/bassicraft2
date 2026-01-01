use wgpu::util::DeviceExt;

use noise::{OpenSimplex};

use crate::{block::BlockVertex, chunk::Chunk, texture_atlas::TextureAtlas};

#[derive(Clone, Debug)]
pub struct ChunkBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub indices_buffer: wgpu::Buffer,
    pub num_elements: u32,
}

impl ChunkBuffer {
    pub fn new(device: &wgpu::Device, vertices: Vec<BlockVertex>, indices: Vec<u32>, num_elements: u32) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunkbuffer vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let indices_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunkbuffer indices buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            vertex_buffer: vertex_buffer,
            indices_buffer: indices_buffer,
            num_elements: num_elements,
        }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub chunks: Vec<Chunk>,
    pub chunk_buffers: Vec<ChunkBuffer>,

    pub noise_gen: OpenSimplex,

    pub texture_atlas: TextureAtlas,
}

impl World {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, seed: u32) -> Self {
        // TEMP TEMP TEMP TODO ACTUAL WORLD GEN
        let base_chunk = Chunk::new([0, 0]);
        let mesh = base_chunk.mesh.clone();

        let chunks = vec![base_chunk];
        let chunk_buffers = vec![ChunkBuffer::new(device, mesh.vertices, mesh.indices, mesh.num_elements)];

        // println!("CHUNKBUFFER : {:?}", chunk_buffers);

        Self {
            chunks: chunks,
            chunk_buffers: chunk_buffers,

            noise_gen: OpenSimplex::new(seed),

            texture_atlas: TextureAtlas::new(device, queue),
        }
    }
}
