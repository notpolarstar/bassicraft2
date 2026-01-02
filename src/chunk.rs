use std::{default, vec};

use noise::{OpenSimplex};

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
    pub fn new(blocks: &Vec<Vec<Vec<Block>>>) -> Self {
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
                                    let mut v = v.clone();
                                    v.position[0] += x as f32;
                                    v.position[1] += y as f32;
                                    v.position[2] += z as f32;
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
    pub fn new(pos: [i32; 2], ) -> Self {
        let blocks = vec![vec![vec![Block::new(17, [false; 6]); CHUNK_Z_SIZE]; CHUNK_Y_SIZE]; CHUNK_X_SIZE];

        let mesh = Mesh::new(&blocks);

        Self {
            pos,
            blocks: blocks,
            mesh: mesh,
        }
    }
}
