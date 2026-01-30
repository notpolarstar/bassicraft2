use crate::{model::Vertex, texture_atlas};

type BlockType = u32;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlockVertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
}

impl Vertex for BlockVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        // const ATTRIBS: [wgpu::VertexAttribute; 2] =
        //     wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

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
                    format: wgpu::VertexFormat::Float32x2,
                    offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
            ],
            // attributes: &ATTRIBS,
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
    fn get_verts(self, mat: BlockType) -> [BlockVertex; 4] {
        let tex_coords = texture_atlas::TextureAtlas::get_block_texture_from_type(mat);

        match self {
            FaceDirections::FRONT => [
                BlockVertex {position: [0.0, 1.0, 1.0], tex_coords: tex_coords[0]},
                BlockVertex {position: [0.0, 0.0, 1.0], tex_coords: tex_coords[1]},
                BlockVertex {position: [1.0, 0.0, 1.0], tex_coords: tex_coords[2]},
                BlockVertex {position: [1.0, 1.0, 1.0], tex_coords: tex_coords[3]},
            ],
            FaceDirections::BACK => [
                BlockVertex {position: [1.0, 1.0, 0.0], tex_coords: tex_coords[0]},
                BlockVertex {position: [1.0, 0.0, 0.0], tex_coords: tex_coords[1]},
                BlockVertex {position: [0.0, 0.0, 0.0], tex_coords: tex_coords[2]},
                BlockVertex {position: [0.0, 1.0, 0.0], tex_coords: tex_coords[3]},
            ],
            FaceDirections::LEFT => [
                BlockVertex {position: [0.0, 1.0, 0.0], tex_coords: tex_coords[0]},
                BlockVertex {position: [0.0, 0.0, 0.0], tex_coords: tex_coords[1]},
                BlockVertex {position: [0.0, 0.0, 1.0], tex_coords: tex_coords[2]},
                BlockVertex {position: [0.0, 1.0, 1.0], tex_coords: tex_coords[3]},
            ],
            FaceDirections::RIGHT => [
                BlockVertex {position: [1.0, 1.0, 1.0], tex_coords: tex_coords[0]},
                BlockVertex {position: [1.0, 0.0, 1.0], tex_coords: tex_coords[1]},
                BlockVertex {position: [1.0, 0.0, 0.0], tex_coords: tex_coords[2]},
                BlockVertex {position: [1.0, 1.0, 0.0], tex_coords: tex_coords[3]},
            ],
            FaceDirections::TOP => [
                BlockVertex {position: [0.0, 1.0, 0.0], tex_coords: tex_coords[0]},
                BlockVertex {position: [0.0, 1.0, 1.0], tex_coords: tex_coords[1]},
                BlockVertex {position: [1.0, 1.0, 1.0], tex_coords: tex_coords[2]},
                BlockVertex {position: [1.0, 1.0, 0.0], tex_coords: tex_coords[3]},
            ],
            FaceDirections::BOTTOM => [
                BlockVertex {position: [0.0, 0.0, 1.0], tex_coords: tex_coords[0]},
                BlockVertex {position: [0.0, 0.0, 0.0], tex_coords: tex_coords[1]},
                BlockVertex {position: [1.0, 0.0, 0.0], tex_coords: tex_coords[2]},
                BlockVertex {position: [1.0, 0.0, 1.0], tex_coords: tex_coords[3]},
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

impl Block {
    pub fn new(mat: BlockType, close_blocks: [bool; 6]) -> Self {
        if mat == 0 {
            return Self {
                mat: mat,
                faces: [None; 6],
            }
        }

        use FaceDirections::*;
        let directions = [BACK, FRONT, LEFT, RIGHT, TOP, BOTTOM];

        let faces: [Option<Face>; 6] = directions.iter().enumerate().map(|(i, &dir)| {
            if close_blocks[i] {
                None
            } else {
                Some(Face::new(dir, mat))
            }
        }).collect::<Vec<_>>().try_into().unwrap();

        Self {
            mat: mat,
            faces: faces,
        }
    }

    pub fn is_air(&self) -> bool {
        self.mat == 0
    }
}
