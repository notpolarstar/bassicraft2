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
}
