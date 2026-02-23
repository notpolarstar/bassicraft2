use crate::{model::Vertex, texture_atlas};

pub type BlockType = u32;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlockVertex {
    pub position: [f32; 3],
    // packed UV + transparency flag. use BlockVertex::pack() to make
    pub packed: u32,
}

impl BlockVertex {
    const UV_BITS: u32 = 10;
    const UV_MAX: f32 = ((1u32 << Self::UV_BITS) - 1) as f32;

    #[inline]
    pub fn pack(tex_u: f32, tex_v: f32, is_transparent: bool) -> u32 {
        let u = (tex_u.clamp(0.0, 1.0) * Self::UV_MAX + 0.5) as u32 & 0x3FF;
        let v = (tex_v.clamp(0.0, 1.0) * Self::UV_MAX + 0.5) as u32 & 0x3FF;
        let t = if is_transparent { 1u32 } else { 0u32 };
        u | (v << Self::UV_BITS) | (t << 20)
    }
}

impl Vertex for BlockVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<BlockVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
            ],
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum FaceDirections {
    FRONT,
    BACK,
    LEFT,
    RIGHT,
    TOP,
    BOTTOM,
}

impl FaceDirections {
    pub fn get_verts(self, mat: BlockType) -> [BlockVertex; 4] {
        let tc = texture_atlas::TextureAtlas::get_block_texture_from_type(mat);
        let p = |i: usize| BlockVertex::pack(tc[i][0], tc[i][1], false);

        match self {
            FaceDirections::FRONT => [
                BlockVertex {
                    position: [0.0, 1.0, 1.0],
                    packed: p(0),
                },
                BlockVertex {
                    position: [0.0, 0.0, 1.0],
                    packed: p(1),
                },
                BlockVertex {
                    position: [1.0, 0.0, 1.0],
                    packed: p(2),
                },
                BlockVertex {
                    position: [1.0, 1.0, 1.0],
                    packed: p(3),
                },
            ],
            FaceDirections::BACK => [
                BlockVertex {
                    position: [1.0, 1.0, 0.0],
                    packed: p(0),
                },
                BlockVertex {
                    position: [1.0, 0.0, 0.0],
                    packed: p(1),
                },
                BlockVertex {
                    position: [0.0, 0.0, 0.0],
                    packed: p(2),
                },
                BlockVertex {
                    position: [0.0, 1.0, 0.0],
                    packed: p(3),
                },
            ],
            FaceDirections::LEFT => [
                BlockVertex {
                    position: [0.0, 1.0, 0.0],
                    packed: p(0),
                },
                BlockVertex {
                    position: [0.0, 0.0, 0.0],
                    packed: p(1),
                },
                BlockVertex {
                    position: [0.0, 0.0, 1.0],
                    packed: p(2),
                },
                BlockVertex {
                    position: [0.0, 1.0, 1.0],
                    packed: p(3),
                },
            ],
            FaceDirections::RIGHT => [
                BlockVertex {
                    position: [1.0, 1.0, 1.0],
                    packed: p(0),
                },
                BlockVertex {
                    position: [1.0, 0.0, 1.0],
                    packed: p(1),
                },
                BlockVertex {
                    position: [1.0, 0.0, 0.0],
                    packed: p(2),
                },
                BlockVertex {
                    position: [1.0, 1.0, 0.0],
                    packed: p(3),
                },
            ],
            FaceDirections::TOP => [
                BlockVertex {
                    position: [0.0, 1.0, 0.0],
                    packed: p(0),
                },
                BlockVertex {
                    position: [0.0, 1.0, 1.0],
                    packed: p(1),
                },
                BlockVertex {
                    position: [1.0, 1.0, 1.0],
                    packed: p(2),
                },
                BlockVertex {
                    position: [1.0, 1.0, 0.0],
                    packed: p(3),
                },
            ],
            FaceDirections::BOTTOM => [
                BlockVertex {
                    position: [0.0, 0.0, 1.0],
                    packed: p(0),
                },
                BlockVertex {
                    position: [0.0, 0.0, 0.0],
                    packed: p(1),
                },
                BlockVertex {
                    position: [1.0, 0.0, 0.0],
                    packed: p(2),
                },
                BlockVertex {
                    position: [1.0, 0.0, 1.0],
                    packed: p(3),
                },
            ],
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Face {
    pub verts: [BlockVertex; 4],
    pub dir: FaceDirections,
}

impl Face {
    fn new(dir: FaceDirections, mat: BlockType) -> Self {
        Self {
            verts: dir.get_verts(mat),
            dir: dir,
        }
    }

    pub fn get_indices() -> [u8; 6] {
        [0, 1, 2, 2, 3, 0]
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Block {
    pub mat: BlockType,
    pub faces: [Option<Face>; 6],
}

// TEMP, MAKE ACTUAL CORRECT BLOCK LOADING SYSTEM LATER FOR "SPECIAL" BLOCKS (DOORS, STAIRS, SHRUBS, FLOWERS, ...) INSTEAD OF HARDCODING EVERYTHING >:(
const NON_SOLID_BLOCKS: &[BlockType] = &[0, 12, 29, 30, 31, 39, 40, 50, 53, 56, 57, 64];
const FLUID_BLOCKS: &[BlockType] = &[208];

impl Block {
    pub fn new(mat: BlockType, close_blocks: [bool; 6]) -> Self {
        if mat == 0 {
            return Self {
                mat: mat,
                faces: [None; 6],
            };
        }

        use FaceDirections::*;
        let directions = [BACK, FRONT, LEFT, RIGHT, TOP, BOTTOM];

        let faces: [Option<Face>; 6] = directions
            .iter()
            .enumerate()
            .map(|(i, &dir)| {
                if close_blocks[i] {
                    None
                } else {
                    Some(Face::new(dir, mat))
                }
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        Self {
            mat: mat,
            faces: faces,
        }
    }

    pub fn is_air(&self) -> bool {
        self.mat == 0
    }

    pub fn is_blocktype_solid(mat: BlockType) -> bool {
        !NON_SOLID_BLOCKS.contains(&mat) && !FLUID_BLOCKS.contains(&mat)
    }

    pub fn is_blocktype_fluid(mat: BlockType) -> bool {
        FLUID_BLOCKS.contains(&mat)
    }
}
