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
            let mesh = chunks[i].mesh.clone();
            let chunk_buffer = ChunkBuffer::new(device, mesh.vertices, mesh.indices, mesh.num_elements);
            chunk_buffers.push(chunk_buffer);
        }

        Self {
            chunks: chunks,
            chunk_buffers: chunk_buffers,

            noise_gen: noise_gen,

            texture_atlas: TextureAtlas::new(device, queue),
        }
    }

    pub fn break_block(&mut self, device: &wgpu::Device, pos: [i32; 3]) {
        if let Some((chunk_index, _)) = self.chunks.iter_mut().enumerate().find(|(_, c)| c.contains_block(pos)) {
            let chunk_pos = self.chunks[chunk_index].pos;
            let local_pos = self.chunks[chunk_index].get_local_pos(pos);
            
            self.chunks[chunk_index].break_block(pos);
            self.update_chunk_mesh(device, chunk_index);

            if local_pos[0] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] - 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
            if local_pos[0] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] + 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
            if local_pos[2] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] - 1]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
            if local_pos[2] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] + 1]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
        }
    }

    pub fn place_block(&mut self, device: &wgpu::Device, pos: [i32; 3]) {
        if let Some((chunk_index, _)) = self.chunks.iter_mut().enumerate().find(|(_, c)| c.contains_position(pos)) {
            let chunk_pos = self.chunks[chunk_index].pos;
            let local_pos = self.chunks[chunk_index].get_local_pos(pos);
            
            self.chunks[chunk_index].place_block(pos);
            self.update_chunk_mesh(device, chunk_index);

            if local_pos[0] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] - 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
            if local_pos[0] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] + 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
            if local_pos[2] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] - 1]) {
                    self.update_chunk_mesh(device, idx);
                }
            }
            if local_pos[2] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] + 1]) {
                    self.update_chunk_mesh(device, idx);
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
                        blocks[x][y] = chunk.blocks[x][y][0].mat;
                    }
                }
            }
            1 => {
                for x in 0..CHUNK_X_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[x][y] = chunk.blocks[x][y][CHUNK_Z_SIZE - 1].mat;
                    }
                }
            }
            2 => {
                for z in 0..CHUNK_Z_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[z][y] = chunk.blocks[0][y][z].mat;
                    }
                }
            }
            3 => {
                for z in 0..CHUNK_Z_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[z][y] = chunk.blocks[CHUNK_X_SIZE - 1][y][z].mat;
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
        
        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let block_type = chunk.blocks[x][y][z].mat;
                    if block_type == 0 {
                        continue;
                    }
                    
                    let mut close_blocks = [false; 6];
                    
                    // BACK (-z)
                    close_blocks[0] = if z == 0 {
                        back_blocks.as_ref()
                            .and_then(|blocks| blocks.get(x).and_then(|col| col.get(y)))
                            .map(|&b| b != 0)
                            .unwrap_or(false)
                    } else {
                        chunk.blocks[x][y][z-1].mat != 0
                    };
                    
                    // FRONT (+z)
                    close_blocks[1] = if z == CHUNK_Z_SIZE - 1 {
                        front_blocks.as_ref()
                            .and_then(|blocks| blocks.get(x).and_then(|col| col.get(y)))
                            .map(|&b| b != 0)
                            .unwrap_or(false)
                    } else {
                        chunk.blocks[x][y][z+1].mat != 0
                    };
                    
                    // LEFT (-x)
                    close_blocks[2] = if x == 0 {
                        left_blocks.as_ref()
                            .and_then(|blocks| blocks.get(z).and_then(|col| col.get(y)))
                            .map(|&b| b != 0)
                            .unwrap_or(false)
                    } else {
                        chunk.blocks[x-1][y][z].mat != 0
                    };
                    
                    // RIGHT (+x)
                    close_blocks[3] = if x == CHUNK_X_SIZE - 1 {
                        right_blocks.as_ref()
                            .and_then(|blocks| blocks.get(z).and_then(|col| col.get(y)))
                            .map(|&b| b != 0)
                            .unwrap_or(false)
                    } else {
                        chunk.blocks[x+1][y][z].mat != 0
                    };
                    
                    // TOP (+y)
                    close_blocks[4] = if y == CHUNK_Y_SIZE - 1 {
                        false
                    } else {
                        chunk.blocks[x][y+1][z].mat != 0
                    };
                    
                    // BOTTOM (-y)
                    close_blocks[5] = if y == 0 {
                        false
                    } else {
                        chunk.blocks[x][y-1][z].mat != 0
                    };
                    
                    chunk.blocks[x][y][z] = Block::new(block_type, close_blocks);
                }
            }
        }
    }
    
    fn update_chunk_mesh(&mut self, device: &wgpu::Device, chunk_index: usize) {
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
        let mesh = self.chunks[chunk_index].mesh.clone();
        self.chunk_buffers[chunk_index] = ChunkBuffer::new(
            device,
            mesh.vertices,
            mesh.indices,
            mesh.num_elements,
        );
    }
}
