use noise::{NoiseFn, OpenSimplex};

use crate::block::{Block, BlockVertex, Face, FaceDirections};

pub const CHUNK_X_SIZE: usize = 16;
pub const CHUNK_Y_SIZE: usize = 256;
pub const CHUNK_Z_SIZE: usize = 16;

const VOLUME: usize = CHUNK_X_SIZE * CHUNK_Y_SIZE * CHUNK_Z_SIZE;

#[inline(always)]
pub fn block_index(x: usize, y: usize, z: usize) -> usize {
    x * CHUNK_Y_SIZE * CHUNK_Z_SIZE + y * CHUNK_Z_SIZE + z
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<BlockVertex>,
    pub indices: Vec<u32>,
    pub num_elements: u32,
}

impl Mesh {
    pub fn build(
        pos: [i32; 2],
        block_types: &[u32],
        back_blocks:  Option<&Vec<Vec<u32>>>,
        front_blocks: Option<&Vec<Vec<u32>>>,
        left_blocks:  Option<&Vec<Vec<u32>>>,
        right_blocks: Option<&Vec<Vec<u32>>>,
    ) -> Self {
        // estimate 3 visible faces per solid block, 4 verts and 6 indices each
        let solid_count = block_types.iter().filter(|&&b| b != 0).count();
        let est_faces = solid_count * 3;
        let mut vertices = Vec::with_capacity(est_faces * 4);
        let mut indices  = Vec::with_capacity(est_faces * 6);
        let mut vert_count: u32 = 0;

        let chunk_ox = CHUNK_X_SIZE as f32 * pos[0] as f32;
        let chunk_oz = CHUNK_Z_SIZE as f32 * pos[1] as f32;

        let face_indices = Face::get_indices();

        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let mat = block_types[block_index(x, y, z)];
                    if mat == 0 { continue; }

                    let ox = x as f32 + chunk_ox;
                    let oy = y as f32;
                    let oz = z as f32 + chunk_oz;

                    // BACK (−z)
                    let back_solid = if z == 0 {
                        back_blocks.and_then(|b| b.get(x)?.get(y).copied())
                            .map(Block::is_blocktype_solid).unwrap_or(false)
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x, y, z - 1)])
                    };
                    if !back_solid {
                        emit_face(&mut vertices, &mut indices, &face_indices, &mut vert_count,
                                  FaceDirections::BACK, mat, ox, oy, oz);
                    }

                    // FRONT (+z)
                    let front_solid = if z == CHUNK_Z_SIZE - 1 {
                        front_blocks.and_then(|b| b.get(x)?.get(y).copied())
                            .map(Block::is_blocktype_solid).unwrap_or(false)
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x, y, z + 1)])
                    };
                    if !front_solid {
                        emit_face(&mut vertices, &mut indices, &face_indices, &mut vert_count,
                                  FaceDirections::FRONT, mat, ox, oy, oz);
                    }

                    // LEFT (−x)
                    let left_solid = if x == 0 {
                        left_blocks.and_then(|b| b.get(z)?.get(y).copied())
                            .map(Block::is_blocktype_solid).unwrap_or(false)
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x - 1, y, z)])
                    };
                    if !left_solid {
                        emit_face(&mut vertices, &mut indices, &face_indices, &mut vert_count,
                                  FaceDirections::LEFT, mat, ox, oy, oz);
                    }

                    // RIGHT (+x)
                    let right_solid = if x == CHUNK_X_SIZE - 1 {
                        right_blocks.and_then(|b| b.get(z)?.get(y).copied())
                            .map(Block::is_blocktype_solid).unwrap_or(false)
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x + 1, y, z)])
                    };
                    if !right_solid {
                        emit_face(&mut vertices, &mut indices, &face_indices, &mut vert_count,
                                  FaceDirections::RIGHT, mat, ox, oy, oz);
                    }

                    // TOP (+y)
                    let top_solid = if y == CHUNK_Y_SIZE - 1 {
                        false
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x, y + 1, z)])
                    };
                    if !top_solid {
                        emit_face(&mut vertices, &mut indices, &face_indices, &mut vert_count,
                                  FaceDirections::TOP, mat, ox, oy, oz);
                    }

                    // BOTTOM (−y)
                    let bot_solid = if y == 0 {
                        false
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x, y - 1, z)])
                    };
                    if !bot_solid {
                        emit_face(&mut vertices, &mut indices, &face_indices, &mut vert_count,
                                  FaceDirections::BOTTOM, mat, ox, oy, oz);
                    }
                }
            }
        }

        Self {
            num_elements: indices.len() as u32,
            vertices,
            indices,
        }
    }
}

#[inline(always)]
fn emit_face(
    vertices: &mut Vec<BlockVertex>,
    indices:  &mut Vec<u32>,
    face_indices: &[u8; 6],
    vert_count: &mut u32,
    dir: FaceDirections,
    mat: u32,
    ox: f32, oy: f32, oz: f32,
) {
    let template = dir.get_verts(mat);
    for v in &template {
        vertices.push(BlockVertex {
            position: [
                v.position[0] + ox,
                v.position[1] + oy,
                v.position[2] + oz,
            ],
            tex_coords: v.tex_coords,
        });
    }
    for &i in face_indices {
        indices.push(i as u32 + *vert_count);
    }
    *vert_count += 4;
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub pos: [i32; 2],

    pub block_types: Vec<u32>,

    // cached neighbour boundary slices used during meshing
    // order: [back, front, left, right]
    pub boundary: [Option<Vec<Vec<u32>>>; 4],

    pub mesh: Mesh,
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}

impl Chunk {
    pub fn new(pos: [i32; 2], noise_fn: OpenSimplex) -> Self {
        let block_types = Self::generate_block_types(pos, noise_fn);
        let mesh = Mesh::build(pos, &block_types, None, None, None, None);
        Self { pos, block_types, boundary: [None, None, None, None], mesh }
    }

    pub fn new_with_boundaries(
        pos: [i32; 2],
        noise_fn: OpenSimplex,
        back:  Option<Vec<Vec<u32>>>,
        front: Option<Vec<Vec<u32>>>,
        left:  Option<Vec<Vec<u32>>>,
        right: Option<Vec<Vec<u32>>>,
    ) -> Self {
        let block_types = Self::generate_block_types(pos, noise_fn);
        let mesh = Mesh::build(
            pos, &block_types,
            back.as_ref(), front.as_ref(), left.as_ref(), right.as_ref(),
        );
        Self {
            pos,
            block_types,
            boundary: [back, front, left, right],
            mesh,
        }
    }

    fn generate_block_types(pos: [i32; 2], noise_fn: OpenSimplex) -> Vec<u32> {
        let mut bt = vec![0u32; VOLUME];
        for x in 0..CHUNK_X_SIZE {
            for z in 0..CHUNK_Z_SIZE {
                let noise_val = noise_fn.get([
                    (x as i32 + pos[0] * CHUNK_X_SIZE as i32) as f64 / 20.0,
                    (z as i32 + pos[1] * CHUNK_Z_SIZE as i32) as f64 / 20.0,
                ]);
                let ground_height = (noise_val * 10.0 + 80.0) as usize;
                const STONE_HEIGHT: usize = 60;
                for y in 0..CHUNK_Y_SIZE {
                    bt[block_index(x, y, z)] = if y < ground_height.saturating_sub(1) && y <= STONE_HEIGHT {
                        2
                    } else if y < ground_height.saturating_sub(1) && y > STONE_HEIGHT {
                        3
                    } else if y == ground_height.saturating_sub(1) && ground_height > 0 {
                        1
                    } else {
                        0
                    };
                }
            }
        }
        bt
    }

    pub fn regenerate_mesh(&mut self) {
        self.mesh = Mesh::build(
            self.pos, &self.block_types,
            self.boundary[0].as_ref(),
            self.boundary[1].as_ref(),
            self.boundary[2].as_ref(),
            self.boundary[3].as_ref(),
        );
    }

    pub fn get_block_type(&self, x: usize, y: usize, z: usize) -> u32 {
        if x < CHUNK_X_SIZE && y < CHUNK_Y_SIZE && z < CHUNK_Z_SIZE {
            self.block_types[block_index(x, y, z)]
        } else {
            0
        }
    }

    pub fn get_block(&self, pos: [i32; 3]) -> Option<Block> {
        if pos[0] < 0 || pos[1] < 0 || pos[2] < 0 { return None; }
        let (x, y, z) = (pos[0] as usize, pos[1] as usize, pos[2] as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE { return None; }
        let mat = self.block_types[block_index(x, y, z)];
        if mat == 0 { return None; }
        Some(Block::new(mat, [false; 6]))
    }

    pub fn break_block(&mut self, pos: [i32; 3]) {
        let lx = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let ly = pos[1];
        let lz = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        if lx < 0 || ly < 0 || lz < 0 { return; }
        let (x, y, z) = (lx as usize, ly as usize, lz as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE { return; }
        self.block_types[block_index(x, y, z)] = 0;
        self.regenerate_mesh();
    }

    pub fn place_block(&mut self, pos: [i32; 3], block_type: u32) {
        let lx = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let ly = pos[1];
        let lz = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        if lx < 0 || ly < 0 || lz < 0 { return; }
        let (x, y, z) = (lx as usize, ly as usize, lz as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE { return; }
        if self.block_types[block_index(x, y, z)] != 0 { return; }
        self.block_types[block_index(x, y, z)] = block_type;
        self.regenerate_mesh();
    }

    pub fn contains_block(&self, pos: [i32; 3]) -> bool {
        let x_min = self.pos[0] * CHUNK_X_SIZE as i32;
        let z_min = self.pos[1] * CHUNK_Z_SIZE as i32;
        let lx = pos[0] - x_min;
        let ly = pos[1];
        let lz = pos[2] - z_min;
        if lx < 0 || ly < 0 || lz < 0 { return false; }
        let (x, y, z) = (lx as usize, ly as usize, lz as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE { return false; }
        self.block_types[block_index(x, y, z)] != 0
    }

    pub fn contains_position(&self, pos: [i32; 3]) -> bool {
        let x_min = self.pos[0] * CHUNK_X_SIZE as i32;
        let z_min = self.pos[1] * CHUNK_Z_SIZE as i32;
        pos[0] >= x_min && pos[0] < x_min + CHUNK_X_SIZE as i32
            && pos[1] >= 0 && pos[1] < CHUNK_Y_SIZE as i32
            && pos[2] >= z_min && pos[2] < z_min + CHUNK_Z_SIZE as i32
    }

    pub fn get_local_pos(&self, pos: [i32; 3]) -> [i32; 3] {
        [
            pos[0] - self.pos[0] * CHUNK_X_SIZE as i32,
            pos[1],
            pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32,
        ]
    }
}
