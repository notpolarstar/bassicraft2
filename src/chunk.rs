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
    pub transparent_indices: Vec<u32>,
    pub transparent_num_elements: u32,
}

impl Mesh {
    pub fn build(
        pos: [i32; 2],
        block_types: &[u32],
        back_blocks: Option<&Vec<Vec<u32>>>,
        front_blocks: Option<&Vec<Vec<u32>>>,
        left_blocks: Option<&Vec<Vec<u32>>>,
        right_blocks: Option<&Vec<Vec<u32>>>,
    ) -> Self {
        // estimate 3 visible faces per solid block, 4 verts and 6 indices each
        let solid_count = block_types.iter().filter(|&&b| b != 0).count();
        let est_faces = solid_count * 3;
        let mut vertices = Vec::with_capacity(est_faces * 4);
        let mut indices = Vec::with_capacity(est_faces * 6);
        let mut transparent_indices: Vec<u32> = Vec::new();
        let mut vert_count: u32 = 0;

        let chunk_ox = CHUNK_X_SIZE as f32 * pos[0] as f32;
        let chunk_oz = CHUNK_Z_SIZE as f32 * pos[1] as f32;

        let face_indices = Face::get_indices();

        for x in 0..CHUNK_X_SIZE {
            for y in 0..CHUNK_Y_SIZE {
                for z in 0..CHUNK_Z_SIZE {
                    let mat = block_types[block_index(x, y, z)];
                    if mat == 0 {
                        continue;
                    }

                    let ox = x as f32 + chunk_ox;
                    let oy = y as f32;
                    let oz = z as f32 + chunk_oz;

                    let is_fluid = Block::is_blocktype_fluid(mat);

                    // BACK (−z)
                    if !is_fluid {
                        let back_solid = if z == 0 {
                            back_blocks
                                .and_then(|b| b.get(x)?.get(y).copied())
                                .map(Block::is_blocktype_solid)
                                .unwrap_or(false)
                        } else {
                            Block::is_blocktype_solid(block_types[block_index(x, y, z - 1)])
                        };
                        if !back_solid {
                            emit_face(
                                &mut vertices,
                                &mut indices,
                                &mut transparent_indices,
                                &face_indices,
                                &mut vert_count,
                                FaceDirections::BACK,
                                mat,
                                ox,
                                oy,
                                oz,
                                0,
                            );
                        }

                        // FRONT (+z)
                        let front_solid = if z == CHUNK_Z_SIZE - 1 {
                            front_blocks
                                .and_then(|b| b.get(x)?.get(y).copied())
                                .map(Block::is_blocktype_solid)
                                .unwrap_or(false)
                        } else {
                            Block::is_blocktype_solid(block_types[block_index(x, y, z + 1)])
                        };
                        if !front_solid {
                            emit_face(
                                &mut vertices,
                                &mut indices,
                                &mut transparent_indices,
                                &face_indices,
                                &mut vert_count,
                                FaceDirections::FRONT,
                                mat,
                                ox,
                                oy,
                                oz,
                                0,
                            );
                        }

                        // LEFT (−x)
                        let left_solid = if x == 0 {
                            left_blocks
                                .and_then(|b| b.get(z)?.get(y).copied())
                                .map(Block::is_blocktype_solid)
                                .unwrap_or(false)
                        } else {
                            Block::is_blocktype_solid(block_types[block_index(x - 1, y, z)])
                        };
                        if !left_solid {
                            emit_face(
                                &mut vertices,
                                &mut indices,
                                &mut transparent_indices,
                                &face_indices,
                                &mut vert_count,
                                FaceDirections::LEFT,
                                mat,
                                ox,
                                oy,
                                oz,
                                0,
                            );
                        }

                        // RIGHT (+x)
                        let right_solid = if x == CHUNK_X_SIZE - 1 {
                            right_blocks
                                .and_then(|b| b.get(z)?.get(y).copied())
                                .map(Block::is_blocktype_solid)
                                .unwrap_or(false)
                        } else {
                            Block::is_blocktype_solid(block_types[block_index(x + 1, y, z)])
                        };
                        if !right_solid {
                            emit_face(
                                &mut vertices,
                                &mut indices,
                                &mut transparent_indices,
                                &face_indices,
                                &mut vert_count,
                                FaceDirections::RIGHT,
                                mat,
                                ox,
                                oy,
                                oz,
                                0,
                            );
                        }

                        // BOTTOM (−y)
                        let bot_solid = if y == 0 {
                            false
                        } else {
                            Block::is_blocktype_solid(block_types[block_index(x, y - 1, z)])
                        };
                        if !bot_solid {
                            emit_face(
                                &mut vertices,
                                &mut indices,
                                &mut transparent_indices,
                                &face_indices,
                                &mut vert_count,
                                FaceDirections::BOTTOM,
                                mat,
                                ox,
                                oy,
                                oz,
                                0,
                            );
                        }
                    }

                    // TOP (+y)
                    let top_solid = if y == CHUNK_Y_SIZE - 1 {
                        false
                    } else {
                        Block::is_blocktype_solid(block_types[block_index(x, y + 1, z)])
                    };
                    let top_also_fluid = is_fluid
                        && y < CHUNK_Y_SIZE - 1
                        && Block::is_blocktype_fluid(block_types[block_index(x, y + 1, z)]);
                    if !top_solid && !top_also_fluid {
                        emit_face(
                            &mut vertices,
                            &mut indices,
                            &mut transparent_indices,
                            &face_indices,
                            &mut vert_count,
                            FaceDirections::TOP,
                            mat,
                            ox,
                            oy,
                            oz,
                            if is_fluid { 1 } else { 0 },
                        );
                    }
                }
            }
        }

        let transparent_num = transparent_indices.len() as u32;
        Self {
            num_elements: indices.len() as u32,
            vertices,
            indices,
            transparent_indices,
            transparent_num_elements: transparent_num,
        }
    }
}

#[inline(always)]
fn emit_face(
    vertices: &mut Vec<BlockVertex>,
    indices: &mut Vec<u32>,
    transparent_indices: &mut Vec<u32>,
    face_indices: &[u8; 6],
    vert_count: &mut u32,
    dir: FaceDirections,
    mat: u32,
    ox: f32,
    oy: f32,
    oz: f32,
    is_transparent: u32,
) {
    let template = dir.get_verts(mat);
    for v in &template {
        let packed = v.packed | (is_transparent << 20);
        vertices.push(BlockVertex {
            position: [v.position[0] + ox, v.position[1] + oy, v.position[2] + oz],
            packed,
        });
    }
    let target = if is_transparent != 0 {
        &mut *transparent_indices
    } else {
        &mut *indices
    };
    for &i in face_indices {
        target.push(i as u32 + *vert_count);
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
        Self {
            pos,
            block_types,
            boundary: [None, None, None, None],
            mesh,
        }
    }

    pub fn new_with_boundaries(
        pos: [i32; 2],
        noise_fn: OpenSimplex,
        back: Option<Vec<Vec<u32>>>,
        front: Option<Vec<Vec<u32>>>,
        left: Option<Vec<Vec<u32>>>,
        right: Option<Vec<Vec<u32>>>,
    ) -> Self {
        let block_types = Self::generate_block_types(pos, noise_fn);
        let mesh = Mesh::build(
            pos,
            &block_types,
            back.as_ref(),
            front.as_ref(),
            left.as_ref(),
            right.as_ref(),
        );
        Self {
            pos,
            block_types,
            boundary: [back, front, left, right],
            mesh,
        }
    }

    // block type constants, STOP HARDCODING EVERYTHING LATER
    const AIR: u32 = 0;
    const GRASS: u32 = 1;
    const STONE: u32 = 2;
    const DIRT: u32 = 3;
    const WATER: u32 = 208;
    const LEAVES: u32 = 53;
    const BEDROCK: u32 = 18;
    const SAND: u32 = 19;
    const GRAVEL: u32 = 20;
    const LOG: u32 = 21;

    const SEA_LEVEL: usize = 64;

    fn generate_block_types(pos: [i32; 2], noise_fn: OpenSimplex) -> Vec<u32> {
        let mut bt = vec![0u32; VOLUME];

        let mut heights = [[0usize; CHUNK_Z_SIZE]; CHUNK_X_SIZE];
        let mut is_river = [[false; CHUNK_Z_SIZE]; CHUNK_X_SIZE];
        let mut temperature = [[0.0f64; CHUNK_Z_SIZE]; CHUNK_X_SIZE];

        for x in 0..CHUNK_X_SIZE {
            for z in 0..CHUNK_Z_SIZE {
                let wx = (x as i32 + pos[0] * CHUNK_X_SIZE as i32) as f64;
                let wz = (z as i32 + pos[1] * CHUNK_Z_SIZE as i32) as f64;

                let continent = noise_fn.get([wx / 200.0, wz / 200.0]);
                let hills = noise_fn.get([wx / 50.0 + 500.0, wz / 50.0 + 500.0]);
                let detail = noise_fn.get([wx / 20.0, wz / 20.0]);
                let cliff_raw = noise_fn.get([wx / 80.0 + 1000.0, wz / 80.0 + 1000.0]);
                let cliff_factor = ((cliff_raw + 0.3) * 2.5).clamp(0.0, 1.0);

                let river_noise = noise_fn.get([wx / 120.0 + 2000.0, wz / 120.0 + 2000.0]);
                let river_width = 0.035;
                let col_is_river = river_noise.abs() < river_width;

                let temp = noise_fn.get([wx / 300.0 + 3000.0, wz / 300.0 + 3000.0]);

                let base = 68.0
                    + continent * 20.0
                    + hills * 12.0 * (1.0 + cliff_factor * 1.5)
                    + detail * 4.0;

                let h = if col_is_river {
                    base.min(Self::SEA_LEVEL as f64 - 2.0)
                } else {
                    base
                };

                heights[x][z] = (h.max(1.0) as usize).min(CHUNK_Y_SIZE - 1);
                is_river[x][z] = col_is_river;
                temperature[x][z] = temp;
            }
        }

        for x in 0..CHUNK_X_SIZE {
            for z in 0..CHUNK_Z_SIZE {
                let ground = heights[x][z];
                let river = is_river[x][z];
                let temp = temperature[x][z];
                let is_beach = ground <= Self::SEA_LEVEL + 2
                    && ground >= Self::SEA_LEVEL.saturating_sub(1)
                    && !river;
                let is_underwater = ground < Self::SEA_LEVEL;

                for y in 0..CHUNK_Y_SIZE {
                    let block = if y == 0 {
                        Self::BEDROCK
                    } else if y < ground.saturating_sub(4) {
                        Self::STONE
                    } else if y < ground {
                        if is_beach || (is_underwater && !river) || (river && y < ground) {
                            Self::SAND
                        } else {
                            Self::DIRT
                        }
                    } else if y == ground {
                        if river {
                            Self::GRAVEL
                        } else if is_beach || is_underwater {
                            Self::SAND
                        } else {
                            Self::GRASS
                        }
                    } else if y <= Self::SEA_LEVEL {
                        Self::WATER
                    } else {
                        Self::AIR
                    };
                    bt[block_index(x, y, z)] = block;
                }
            }
        }

        for x in 0..CHUNK_X_SIZE {
            for z in 0..CHUNK_Z_SIZE {
                let ground = heights[x][z];
                if ground <= Self::SEA_LEVEL + 1 {
                    continue;
                }
                if is_river[x][z] {
                    continue;
                }
                if bt[block_index(x, ground, z)] != Self::GRASS {
                    continue;
                }

                let wx = (x as i32 + pos[0] * CHUNK_X_SIZE as i32) as i64;
                let wz = (z as i32 + pos[1] * CHUNK_Z_SIZE as i32) as i64;
                if !Self::should_place_tree(wx, wz) {
                    continue;
                }

                if x < 2 || x >= CHUNK_X_SIZE - 2 || z < 2 || z >= CHUNK_Z_SIZE - 2 {
                    continue;
                }

                let trunk_height = 4 + (Self::hash_pos(wx, wz, 7) % 3) as usize; // 4-6
                let base_y = ground + 1;
                for dy in 0..trunk_height {
                    let y = base_y + dy;
                    if y >= CHUNK_Y_SIZE {
                        break;
                    }
                    bt[block_index(x, y, z)] = Self::LOG;
                }

                let canopy_base = base_y + trunk_height - 2;
                let canopy_top = base_y + trunk_height + 1;
                for ly in canopy_base..=canopy_top {
                    if ly >= CHUNK_Y_SIZE {
                        break;
                    }
                    let radius: i32 = if ly == canopy_top { 1 } else { 2 };
                    for dx in -radius..=radius {
                        for dz in -radius..=radius {
                            if dx.abs() == radius && dz.abs() == radius {
                                continue;
                            }
                            let tx = x as i32 + dx;
                            let tz = z as i32 + dz;
                            if tx < 0
                                || tx >= CHUNK_X_SIZE as i32
                                || tz < 0
                                || tz >= CHUNK_Z_SIZE as i32
                            {
                                continue;
                            }
                            let idx = block_index(tx as usize, ly, tz as usize);
                            if bt[idx] == Self::AIR {
                                bt[idx] = Self::LEAVES;
                            }
                        }
                    }
                }
            }
        }

        bt
    }

    fn should_place_tree(wx: i64, wz: i64) -> bool {
        Self::hash_pos(wx, wz, 0) % 40 == 0
    }

    fn hash_pos(wx: i64, wz: i64, salt: u64) -> u64 {
        let mut h = (wx as u64).wrapping_mul(73856093)
            ^ (wz as u64).wrapping_mul(19349663)
            ^ salt.wrapping_mul(83492791);
        h = h.wrapping_mul(h).wrapping_add(h);
        h ^ (h >> 16)
    }

    pub fn regenerate_mesh(&mut self) {
        self.mesh = Mesh::build(
            self.pos,
            &self.block_types,
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
        if pos[0] < 0 || pos[1] < 0 || pos[2] < 0 {
            return None;
        }
        let (x, y, z) = (pos[0] as usize, pos[1] as usize, pos[2] as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return None;
        }
        let mat = self.block_types[block_index(x, y, z)];
        if mat == 0 {
            return None;
        }
        Some(Block::new(mat, [false; 6]))
    }

    pub fn break_block(&mut self, pos: [i32; 3]) {
        let lx = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let ly = pos[1];
        let lz = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        if lx < 0 || ly < 0 || lz < 0 {
            return;
        }
        let (x, y, z) = (lx as usize, ly as usize, lz as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return;
        }
        self.block_types[block_index(x, y, z)] = 0;
        self.regenerate_mesh();
    }

    pub fn place_block(&mut self, pos: [i32; 3], block_type: u32) {
        let lx = pos[0] - self.pos[0] * CHUNK_X_SIZE as i32;
        let ly = pos[1];
        let lz = pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32;
        if lx < 0 || ly < 0 || lz < 0 {
            return;
        }
        let (x, y, z) = (lx as usize, ly as usize, lz as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return;
        }
        // if self.block_types[block_index(x, y, z)] != 0 {
        //     return;
        // }
        self.block_types[block_index(x, y, z)] = block_type;
        self.regenerate_mesh();
    }

    pub fn contains_block(&self, pos: [i32; 3]) -> bool {
        let x_min = self.pos[0] * CHUNK_X_SIZE as i32;
        let z_min = self.pos[1] * CHUNK_Z_SIZE as i32;
        let lx = pos[0] - x_min;
        let ly = pos[1];
        let lz = pos[2] - z_min;
        if lx < 0 || ly < 0 || lz < 0 {
            return false;
        }
        let (x, y, z) = (lx as usize, ly as usize, lz as usize);
        if x >= CHUNK_X_SIZE || y >= CHUNK_Y_SIZE || z >= CHUNK_Z_SIZE {
            return false;
        }
        self.block_types[block_index(x, y, z)] != 0
    }

    pub fn contains_position(&self, pos: [i32; 3]) -> bool {
        let x_min = self.pos[0] * CHUNK_X_SIZE as i32;
        let z_min = self.pos[1] * CHUNK_Z_SIZE as i32;
        pos[0] >= x_min
            && pos[0] < x_min + CHUNK_X_SIZE as i32
            && pos[1] >= 0
            && pos[1] < CHUNK_Y_SIZE as i32
            && pos[2] >= z_min
            && pos[2] < z_min + CHUNK_Z_SIZE as i32
    }

    pub fn get_local_pos(&self, pos: [i32; 3]) -> [i32; 3] {
        [
            pos[0] - self.pos[0] * CHUNK_X_SIZE as i32,
            pos[1],
            pos[2] - self.pos[1] * CHUNK_Z_SIZE as i32,
        ]
    }
}
