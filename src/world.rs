use wgpu::util::DeviceExt;

use noise::{OpenSimplex};

use crate::{
    block::BlockVertex, 
    chunk::{Chunk, CHUNK_X_SIZE, CHUNK_Y_SIZE, CHUNK_Z_SIZE}, 
    texture_atlas::TextureAtlas
};

#[derive(Clone, Debug)]
pub struct ChunkBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub indices_buffer: wgpu::Buffer,
    pub num_elements: u32,
    vertex_capacity: usize,
    index_capacity: usize,
}

impl ChunkBuffer {
    pub fn new(device: &wgpu::Device, vertices: Vec<BlockVertex>, indices: Vec<u32>, num_elements: u32) -> Self {
        let vertex_capacity = vertices.len();
        let index_capacity = indices.len();
        
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
            vertex_buffer,
            indices_buffer,
            num_elements,
            vertex_capacity,
            index_capacity,
        }
    }
    
    pub fn update_or_recreate(
        &mut self, 
        device: &wgpu::Device, 
        queue: &wgpu::Queue,
        vertices: Vec<BlockVertex>, 
        indices: Vec<u32>, 
        num_elements: u32
    ) {
        if vertices.len() > self.vertex_capacity || indices.len() > self.index_capacity {
            self.vertex_capacity = (vertices.len() as f32 * 1.5) as usize;
            self.index_capacity = (indices.len() as f32 * 1.5) as usize;
            
            let vertex_buffer_size = (self.vertex_capacity * std::mem::size_of::<BlockVertex>()) as u64;
            let index_buffer_size = (self.index_capacity * std::mem::size_of::<u32>()) as u64;
            
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chunkbuffer vertex buffer"),
                size: vertex_buffer_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            
            self.indices_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chunkbuffer indices buffer"),
                size: index_buffer_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.indices_buffer, 0, bytemuck::cast_slice(&indices));
        self.num_elements = num_elements;
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
        let noise_gen = OpenSimplex::new(seed);

        let mut chunks = Vec::new();
        let mut chunk_buffers = Vec::new();

        const WORLD_SIZE: i32 = 5;

        for x in -WORLD_SIZE..WORLD_SIZE {
            for y in -WORLD_SIZE..WORLD_SIZE {
                let base_chunk = Chunk::new([x, y], noise_gen);
                chunks.push(base_chunk);
            }
        }

        for i in 0..chunks.len() {
            let pos = chunks[i].pos;
            
            let left_idx = chunks.iter().position(|c| c.pos == [pos[0] - 1, pos[1]]);
            let right_idx = chunks.iter().position(|c| c.pos == [pos[0] + 1, pos[1]]);
            let back_idx = chunks.iter().position(|c| c.pos == [pos[0], pos[1] - 1]);
            let front_idx = chunks.iter().position(|c| c.pos == [pos[0], pos[1] + 1]);
            
            let left_blocks = left_idx.map(|idx| Self::get_boundary_blocks(&chunks[idx], 3)); // right face of left chunk
            let right_blocks = right_idx.map(|idx| Self::get_boundary_blocks(&chunks[idx], 2)); // left face of right chunk
            let back_blocks = back_idx.map(|idx| Self::get_boundary_blocks(&chunks[idx], 1)); // front face of back chunk
            let front_blocks = front_idx.map(|idx| Self::get_boundary_blocks(&chunks[idx], 0)); // back face of front chunk
            
            Self::update_chunk_faces_with_neighbor_blocks(
                &mut chunks[i],
                left_blocks,
                right_blocks,
                back_blocks,
                front_blocks,
            );

            chunks[i].regenerate_mesh();
            let chunk_buffer = ChunkBuffer::new(
                device,
                std::mem::take(&mut chunks[i].mesh.vertices),
                std::mem::take(&mut chunks[i].mesh.indices),
                chunks[i].mesh.num_elements,
            );
            chunk_buffers.push(chunk_buffer);
        }

        Self {
            chunks: chunks,
            chunk_buffers: chunk_buffers,

            noise_gen: noise_gen,

            texture_atlas: TextureAtlas::new(device, queue),
        }
    }

    pub fn break_block(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pos: [i32; 3]) -> Option<u32> {
        if let Some((chunk_index, _)) = self.chunks.iter_mut().enumerate().find(|(_, c)| c.contains_block(pos)) {
            let chunk_pos = self.chunks[chunk_index].pos;
            let local_pos = self.chunks[chunk_index].get_local_pos(pos);
            
            let block_type = self.chunks[chunk_index].blocks.get(&(local_pos[0] as usize, local_pos[1] as usize, local_pos[2] as usize))
                .map(|b| b.mat);
            
            self.chunks[chunk_index].break_block(pos);
            self.update_chunk_mesh(device, queue, chunk_index);

            if local_pos[0] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] - 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[0] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] + 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] - 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] + 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            
            block_type
        } else {
            None
        }
    }

    pub fn place_block(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pos: [i32; 3], selected_block: u32) {
        if let Some((chunk_index, _)) = self.chunks.iter_mut().enumerate().find(|(_, c)| c.contains_position(pos)) {
            let chunk_pos = self.chunks[chunk_index].pos;
            let local_pos = self.chunks[chunk_index].get_local_pos(pos);
            
            self.chunks[chunk_index].place_block(pos, selected_block);
            self.update_chunk_mesh(device, queue, chunk_index);

            if local_pos[0] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] - 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[0] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] + 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] - 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] + 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
        }
    }
    
    fn find_chunk(&self, pos: [i32; 2]) -> Option<usize> {
        self.chunks.iter().position(|c| c.pos == pos)
    }

    fn get_boundary_blocks(chunk: &Chunk, face: usize) -> Vec<Vec<u32>> {
        let mut blocks = vec![vec![0u32; CHUNK_Y_SIZE]; match face {
            0 | 1 => CHUNK_X_SIZE,
            _ => CHUNK_Z_SIZE,
        }];
        
        match face {
            0 => {
                for x in 0..CHUNK_X_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[x][y] = chunk.blocks.get(&(x, y, 0)).map(|b| b.mat).unwrap_or(0);
                    }
                }
            }
            1 => {
                for x in 0..CHUNK_X_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[x][y] = chunk.blocks.get(&(x, y, CHUNK_Z_SIZE - 1)).map(|b| b.mat).unwrap_or(0);
                    }
                }
            }
            2 => {
                for z in 0..CHUNK_Z_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[z][y] = chunk.blocks.get(&(0, y, z)).map(|b| b.mat).unwrap_or(0);
                    }
                }
            }
            3 => {
                for z in 0..CHUNK_Z_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[z][y] = chunk.blocks.get(&(CHUNK_X_SIZE - 1, y, z)).map(|b| b.mat).unwrap_or(0);
                    }
                }
            }
            _ => {}
        }
        
        blocks
    }
    
    fn update_chunk_faces_with_neighbor_blocks(
        chunk: &mut Chunk,
        left_blocks: Option<Vec<Vec<u32>>>,
        right_blocks: Option<Vec<Vec<u32>>>,
        back_blocks: Option<Vec<Vec<u32>>>,
        front_blocks: Option<Vec<Vec<u32>>>,
    ) {
        use crate::chunk::{CHUNK_X_SIZE, CHUNK_Y_SIZE, CHUNK_Z_SIZE};
        use crate::block::Block;
        
        let keys: Vec<_> = chunk.blocks.keys().cloned().collect();
        for (x, y, z) in keys {
            let block_type = chunk.blocks.get(&(x, y, z)).map(|b| b.mat).unwrap_or(0);
            if block_type == 0 {
                continue;
            }
            
            let mut close_blocks = [false; 6];
            
            // BACK (-z)
            close_blocks[0] = if z == 0 {
                back_blocks.as_ref()
                    .and_then(|blocks| blocks.get(x).and_then(|col| col.get(y)))
                    .map(|&b| Block::is_blocktype_solid(b))
                    .unwrap_or(false)
            } else {
                chunk.blocks.get(&(x, y, z-1)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // FRONT (+z)
            close_blocks[1] = if z == CHUNK_Z_SIZE - 1 {
                front_blocks.as_ref()
                    .and_then(|blocks| blocks.get(x).and_then(|col| col.get(y)))
                    .map(|&b| Block::is_blocktype_solid(b))
                    .unwrap_or(false)
            } else {
                chunk.blocks.get(&(x, y, z+1)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // LEFT (-x)
            close_blocks[2] = if x == 0 {
                left_blocks.as_ref()
                    .and_then(|blocks| blocks.get(z).and_then(|col| col.get(y)))
                    .map(|&b| Block::is_blocktype_solid(b))
                    .unwrap_or(false)
            } else {
                chunk.blocks.get(&(x-1, y, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // RIGHT (+x)
            close_blocks[3] = if x == CHUNK_X_SIZE - 1 {
                right_blocks.as_ref()
                    .and_then(|blocks| blocks.get(z).and_then(|col| col.get(y)))
                    .map(|&b| Block::is_blocktype_solid(b))
                    .unwrap_or(false)
            } else {
                chunk.blocks.get(&(x+1, y, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // TOP (+y)
            close_blocks[4] = if y == CHUNK_Y_SIZE - 1 {
                false
            } else {
                chunk.blocks.get(&(x, y+1, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // BOTTOM (-y)
            close_blocks[5] = if y == 0 {
                false
            } else {
                chunk.blocks.get(&(x, y-1, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            chunk.blocks.insert((x, y, z), Block::new(block_type, close_blocks));
        }
    }
    
    fn update_chunk_mesh(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, chunk_index: usize) {
        let pos = self.chunks[chunk_index].pos;

        let left_idx = self.chunks.iter().position(|c| c.pos == [pos[0] - 1, pos[1]]);
        let right_idx = self.chunks.iter().position(|c| c.pos == [pos[0] + 1, pos[1]]);
        let back_idx = self.chunks.iter().position(|c| c.pos == [pos[0], pos[1] - 1]);
        let front_idx = self.chunks.iter().position(|c| c.pos == [pos[0], pos[1] + 1]);

        let left_blocks = left_idx.map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 3));
        let right_blocks = right_idx.map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 2));
        let back_blocks = back_idx.map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 1));
        let front_blocks = front_idx.map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 0));

        Self::update_chunk_faces_with_neighbor_blocks(
            &mut self.chunks[chunk_index],
            left_blocks,
            right_blocks,
            back_blocks,
            front_blocks,
        );

        self.chunks[chunk_index].regenerate_mesh();
        self.chunk_buffers[chunk_index].update_or_recreate(
            device,
            queue,
            std::mem::take(&mut self.chunks[chunk_index].mesh.vertices),
            std::mem::take(&mut self.chunks[chunk_index].mesh.indices),
            self.chunks[chunk_index].mesh.num_elements,
        );
    }
}
