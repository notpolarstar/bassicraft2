use std::{default, vec};
use std::collections::HashMap;

use noise::{NoiseFn, OpenSimplex};

use crate::block::{Block, BlockVertex, Face};

pub const CHUNK_X_SIZE: usize = 16;
pub const CHUNK_Y_SIZE: usize = 256;
pub const CHUNK_Z_SIZE: usize = 16;

#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<BlockVertex>,
    pub indices: Vec<u32>,
    pub num_elements: u32,
}

impl Mesh {
    pub fn new(pos: [i32; 2], blocks: &HashMap<(usize, usize, usize), Block>) -> Self {
        // 3 visible faces per block estimation, hard coded magic number but nice perf boost
        let estimated_faces = blocks.len() * 3;
        let mut vertices: Vec<BlockVertex> = Vec::with_capacity(estimated_faces * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_faces * 6);
        let mut num_elements: u32 = 0;

        let chunk_offset_x = CHUNK_X_SIZE as f32 * pos[0] as f32;
        let chunk_offset_z = CHUNK_Z_SIZE as f32 * pos[1] as f32;

        for (&(x, y, z), block) in blocks.iter() {
            let offset_x = x as f32 + chunk_offset_x;
            let offset_y = y as f32;
            let offset_z = z as f32 + chunk_offset_z;

            block.faces.iter()
                .filter_map(|face| face.as_ref())
                .for_each(|f| {
                    for vert in &f.verts {
                        vertices.push(BlockVertex {
                            position: [
                                vert.position[0] + offset_x,
                                vert.position[1] + offset_y,
                                vert.position[2] + offset_z,
                            ],
                            tex_coords: vert.tex_coords,
                        });
                    }
                    
                    indices.extend(Face::get_indices().iter().map(|&i| i as u32 + num_elements));
                    num_elements += 4;
                });
        }

        Self {
            vertices,
            num_elements: indices.len() as u32,
            indices,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub pos: [i32; 2],
    pub blocks: HashMap<(usize, usize, usize), Block>,
    pub mesh: Mesh,
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}

impl Chunk {
    pub fn new(pos: [i32; 2], noise_fn: OpenSimplex) -> Self {
        let blocks = Chunk::generate_blocks(pos, noise_fn);

        let mesh = Mesh::new(pos, &blocks);

        Self {
            pos,
            blocks: blocks,
            mesh: mesh,
        }
    }

    fn generate_blocks(pos: [i32; 2], noise_fn: OpenSimplex) -> HashMap<(usize, usize, usize), Block> {
        let mut block_types = vec![0u32; CHUNK_X_SIZE * CHUNK_Y_SIZE * CHUNK_Z_SIZE];
        
        #[inline]
        fn idx(x: usize, y: usize, z: usize) -> usize {
            x * CHUNK_Y_SIZE * CHUNK_Z_SIZE + y * CHUNK_Z_SIZE + z
        }

        for x in 0..CHUNK_X_SIZE {
            for z in 0..CHUNK_Z_SIZE {
                let noise_val = noise_fn.get([
                    (x as i32 + pos[0] * CHUNK_X_SIZE as i32) as f64 / 20.0,
                    (z as i32 + pos[1] * CHUNK_Z_SIZE as i32) as f64 / 20.0,
                ]);
                let ground_height = (noise_val * 10.0 + 80.0) as usize;
                const STONE_HEIGHT: usize = 60;
                for y in 0..CHUNK_Y_SIZE {
                    let block_type = if y < ground_height.saturating_sub(1) && y <= STONE_HEIGHT {
                        2
                    } else if y < ground_height.saturating_sub(1) && y > STONE_HEIGHT {
                        3
                    } else if y == ground_height.saturating_sub(1) && ground_height > 0 {
                        1
                    } else {
                        0
                    };
                    block_types[idx(x, y, z)] = block_type;
                }
            }
        }

        // approximate number of naturally generated blocks per chunk, hard coded magic number but nice perf boost
        let mut blocks = HashMap::with_capacity((CHUNK_X_SIZE * CHUNK_Y_SIZE * CHUNK_Z_SIZE) / 4);
        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let block_type = block_types[idx(x, y, z)];
                    if block_type == 0 {
                        continue;
                    }
                    let mut close_blocks = [false; 6];
                    // BACK (-z)
                    close_blocks[0] = if z == 0 {
                        false
                    } else {
                        !Block::is_blocktype_solid(block_types[idx(x, y, z-1)])
                    };
                    // FRONT (+z)
                    close_blocks[1] = if z == CHUNK_Z_SIZE-1 {
                        false
                    } else {
                        !Block::is_blocktype_solid(block_types[idx(x, y, z+1)])
                    };
                    // LEFT (-x)
                    close_blocks[2] = if x == 0 {
                        false
                    } else {
                        !Block::is_blocktype_solid(block_types[idx(x-1, y, z)])
                    };
                    // RIGHT (+x)
                    close_blocks[3] = if x == CHUNK_X_SIZE-1 {
                        false
                    } else {
                        !Block::is_blocktype_solid(block_types[idx(x+1, y, z)])
                    };
                    // TOP (+y)
                    close_blocks[4] = if y == CHUNK_Y_SIZE-1 {
                        false
                    } else {
                        !Block::is_blocktype_solid(block_types[idx(x, y+1, z)])
                    };
                    // BOTTOM (-y)
                    close_blocks[5] = if y == 0 {
                        false
                    } else {
                        !Block::is_blocktype_solid(block_types[idx(x, y-1, z)])
                    };
                    blocks.insert((x, y, z), Block::new(block_type, close_blocks));
                }
            }
        }
        blocks
    }

    fn update_block_faces(&mut self) {
        self.update_block_faces_with_neighbors(None, None, None, None);
    }

    pub fn update_block_faces_with_neighbors(
        &mut self,
        left_chunk: Option<&Chunk>,
        right_chunk: Option<&Chunk>,
        back_chunk: Option<&Chunk>,
        front_chunk: Option<&Chunk>,
    ) {
        let keys: Vec<_> = self.blocks.keys().cloned().collect();
        for (x, y, z) in keys {
            let block_type = self.blocks.get(&(x, y, z)).map(|b| b.mat).unwrap_or(0);
            if block_type == 0 {
                continue;
            }
            let mut close_blocks = [false; 6];
            
            // BACK (-z)
            close_blocks[0] = if z == 0 {
                back_chunk
                    .and_then(|chunk| chunk.get_block([x as i32, y as i32, (CHUNK_Z_SIZE - 1) as i32]))
                    .map(|block| Block::is_blocktype_solid(block.mat))
                    .unwrap_or(false)
            } else {
                self.blocks.get(&(x, y, z-1)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // FRONT (+z)
            close_blocks[1] = if z == CHUNK_Z_SIZE-1 {
                front_chunk
                    .and_then(|chunk| chunk.get_block([x as i32, y as i32, 0]))
                    .map(|block| Block::is_blocktype_solid(block.mat))
                    .unwrap_or(false)
            } else {
                self.blocks.get(&(x, y, z+1)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // LEFT (-x)
            close_blocks[2] = if x == 0 {
                left_chunk
                    .and_then(|chunk| chunk.get_block([(CHUNK_X_SIZE - 1) as i32, y as i32, z as i32]))
                    .map(|block| Block::is_blocktype_solid(block.mat))
                    .unwrap_or(false)
            } else {
                self.blocks.get(&(x-1, y, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // RIGHT (+x)
            close_blocks[3] = if x == CHUNK_X_SIZE-1 {
                right_chunk
                    .and_then(|chunk| chunk.get_block([0, y as i32, z as i32]))
                    .map(|block| Block::is_blocktype_solid(block.mat))
                    .unwrap_or(false)
            } else {
                self.blocks.get(&(x+1, y, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // TOP (+y)
            close_blocks[4] = if y == CHUNK_Y_SIZE-1 {
                false
            } else {
                self.blocks.get(&(x, y+1, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            // BOTTOM (-y)
            close_blocks[5] = if y == 0 {
                false
            } else {
                self.blocks.get(&(x, y-1, z)).map(|b| Block::is_blocktype_solid(b.mat)).unwrap_or(false)
            };
            
            self.blocks.insert((x, y, z), Block::new(block_type, close_blocks));
        }
    }

    pub fn get_block(&self, pos: [i32; 3]) -> Option<Block> {
        if pos[0] < 0 || pos[1] < 0 || pos[2] < 0 {
            return None;
        }
        let x = pos[0] as usize;
        let y = pos[1] as usize;
        let z = pos[2] as usize;
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return None;
        }
        self.blocks.get(&(x, y, z)).cloned()
    }

    pub fn break_block(&mut self, pos: [i32; 3]) {
        let local_x = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let local_y = pos[1];
        let local_z = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        
        if local_x < 0 || local_y < 0 || local_z < 0 {
            return;
        }
        let x = local_x as usize;
        let y = local_y as usize;
        let z = local_z as usize;
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return;
        }
        self.blocks.remove(&(x, y, z));
        self.update_block_faces();
        self.mesh = Mesh::new(self.pos, &self.blocks);
    }

    pub fn place_block(&mut self, pos: [i32; 3], block_type: u32) {
        let local_x = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let local_y = pos[1];
        let local_z = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        
        if local_x < 0 || local_y < 0 || local_z < 0 {
            return;
        }
        let x = local_x as usize;
        let y = local_y as usize;
        let z = local_z as usize;
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return;
        }
        if self.blocks.contains_key(&(x, y, z)) {
            return;
        }
        self.blocks.insert((x, y, z), Block::new(block_type, [false; 6]));
        self.update_block_faces();
        self.mesh = Mesh::new(self.pos, &self.blocks);
    }

    pub fn contains_block(&self, pos: [i32; 3]) -> bool {
        let chunk_world_x_min = self.pos[0] * CHUNK_X_SIZE as i32;
        let chunk_world_x_max = chunk_world_x_min + CHUNK_X_SIZE as i32;
        let chunk_world_z_min = self.pos[1] * CHUNK_Z_SIZE as i32;
        let chunk_world_z_max = chunk_world_z_min + CHUNK_Z_SIZE as i32;
        
        if pos[0] < chunk_world_x_min || pos[0] >= chunk_world_x_max {
            return false;
        }
        if pos[1] < 0 || pos[1] >= CHUNK_Y_SIZE as i32 {
            return false;
        }
        if pos[2] < chunk_world_z_min || pos[2] >= chunk_world_z_max {
            return false;
        }

        let local_x = (pos[0] - chunk_world_x_min) as usize;
        let local_y = pos[1] as usize;
        let local_z = (pos[2] - chunk_world_z_min) as usize;
        
        self.blocks.contains_key(&(local_x, local_y, local_z))
    }

    pub fn contains_position(&self, pos: [i32; 3]) -> bool {
        let chunk_world_x_min = self.pos[0] * CHUNK_X_SIZE as i32;
        let chunk_world_x_max = chunk_world_x_min + CHUNK_X_SIZE as i32;
        let chunk_world_z_min = self.pos[1] * CHUNK_Z_SIZE as i32;
        let chunk_world_z_max = chunk_world_z_min + CHUNK_Z_SIZE as i32;
        
        pos[0] >= chunk_world_x_min && pos[0] < chunk_world_x_max
            && pos[1] >= 0 && pos[1] < CHUNK_Y_SIZE as i32
            && pos[2] >= chunk_world_z_min && pos[2] < chunk_world_z_max
    }
    
    pub fn get_local_pos(&self, pos: [i32; 3]) -> [i32; 3] {
        let local_x = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let local_y = pos[1];
        let local_z = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        [local_x, local_y, local_z]
    }
    
    pub fn regenerate_mesh(&mut self) {
        self.mesh = Mesh::new(self.pos, &self.blocks);
    }
}
