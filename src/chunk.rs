use std::{default, vec};

use noise::{NoiseFn, OpenSimplex};

use crate::block::{Block, BlockVertex, Face};

const CHUNK_X_SIZE: usize = 16;
const CHUNK_Y_SIZE: usize = 256;
const CHUNK_Z_SIZE: usize = 16;

#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<BlockVertex>,
    pub indices: Vec<u32>,
    pub num_elements: u32,
}

impl Mesh {
    pub fn new(pos: [i32; 2], blocks: &Vec<Vec<Vec<Block>>>) -> Self {
        let mut vertices: Vec<BlockVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut num_elements: u32 = 0;

        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let block = &blocks[x][y][z];

                    block.faces.iter()
                        .filter_map(|face| face.as_ref())
                        .for_each(|f| {
                            vertices.extend(
                                f.verts.iter().map(|v| {
                                    // THIS IS BAD ! Later : send the pos of the chunk in the shader and move them there

                                    let mut v = v.clone();
                                    v.position[0] += x as f32 + CHUNK_X_SIZE as f32 * pos[0] as f32;
                                    v.position[1] += y as f32;
                                    v.position[2] += z as f32 + CHUNK_Z_SIZE as f32 * pos[1] as f32;
                                    v
                                })
                            );
                            indices.extend(Face::get_indices().iter().map(|&i| i as u32 + num_elements));
                            num_elements += 4;
                        });
                }
            }
        }

        // println!("MESH : {:?}", vertices);

        Self {
            vertices,
            num_elements: indices.clone().len() as u32,
            indices,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub pos: [i32; 2],
    pub blocks: Vec<Vec<Vec<Block>>>,
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

    fn generate_blocks(pos: [i32; 2], noise_fn: OpenSimplex) -> Vec<Vec<Vec<Block>>> {
        let mut block_types = vec![vec![vec![0u32; CHUNK_Z_SIZE]; CHUNK_Y_SIZE]; CHUNK_X_SIZE];

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
                    block_types[x][y][z] = block_type;
                }
            }
        }

        let mut blocks = vec![vec![vec![Block::new(0, [false; 6]); CHUNK_Z_SIZE]; CHUNK_Y_SIZE]; CHUNK_X_SIZE];
        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let block_type = block_types[x][y][z];
                    if block_type == 0 {
                        blocks[x][y][z] = Block::new(0, [false; 6]);
                        continue;
                    }
                    let mut close_blocks = [false; 6];
                    // BACK (-z)
                    close_blocks[0] = if z == 0 {
                        false
                    } else {
                        block_types[x][y][z-1] != 0
                    };
                    // FRONT (+z)
                    close_blocks[1] = if z == CHUNK_Z_SIZE-1 {
                        false
                    } else {
                        block_types[x][y][z+1] != 0
                    };
                    // LEFT (-x)
                    close_blocks[2] = if x == 0 {
                        false
                    } else {
                        block_types[x-1][y][z] != 0
                    };
                    // RIGHT (+x)
                    close_blocks[3] = if x == CHUNK_X_SIZE-1 {
                        false
                    } else {
                        block_types[x+1][y][z] != 0
                    };
                    // TOP (+y)
                    close_blocks[4] = if y == CHUNK_Y_SIZE-1 {
                        false
                    } else {
                        block_types[x][y+1][z] != 0
                    };
                    // BOTTOM (-y)
                    close_blocks[5] = if y == 0 {
                        false
                    } else {
                        block_types[x][y-1][z] != 0
                    };
                    blocks[x][y][z] = Block::new(block_type, close_blocks);
                }
            }
        }
        blocks
    }

    fn update_block_faces(&mut self) {
        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let block_type = self.blocks[x][y][z].mat;
                    if block_type == 0 {
                        self.blocks[x][y][z] = Block::new(0, [false; 6]);
                        continue;
                    }
                    let mut close_blocks = [false; 6];
                    // BACK (-z)
                    close_blocks[0] = if z == 0 {
                        false
                    } else {
                        self.blocks[x][y][z-1].mat != 0
                    };
                    // FRONT (+z)
                    close_blocks[1] = if z == CHUNK_Z_SIZE-1 {
                        false
                    } else {
                        self.blocks[x][y][z+1].mat != 0
                    };
                    // LEFT (-x)
                    close_blocks[2] = if x == 0 {
                        false
                    } else {
                        self.blocks[x-1][y][z].mat != 0
                    };
                    // RIGHT (+x)
                    close_blocks[3] = if x == CHUNK_X_SIZE-1 {
                        false
                    } else {
                        self.blocks[x+1][y][z].mat != 0
                    };
                    // TOP (+y)
                    close_blocks[4] = if y == CHUNK_Y_SIZE-1 {
                        false
                    } else {
                        self.blocks[x][y+1][z].mat != 0
                    };
                    // BOTTOM (-y)
                    close_blocks[5] = if y == 0 {
                        false
                    } else {
                        self.blocks[x][y-1][z].mat != 0
                    };
                    self.blocks[x][y][z] = Block::new(block_type, close_blocks);
                }
            }
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
        Some(self.blocks[x][y][z].clone())
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
        self.blocks[x][y][z] = Block::new(0, [false; 6]);
        self.update_block_faces();
        self.mesh = Mesh::new(self.pos, &self.blocks);
    }

    pub fn place_block(&mut self, pos: [i32; 3]) {
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
        if self.blocks[x][y][z].mat != 0 {
            return;
        }
        self.blocks[x][y][z] = Block::new(8, [false; 6]);
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
        
        self.blocks[local_x][local_y][local_z].mat != 0
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
}
