use crate::camera;
use crate::chunk;

pub struct Player {
    pub camera: camera::Camera,
    pub projection: camera::Projection,
    pub camera_controller: camera::CameraController,
}

pub const MAX_BLOCK_POINT_DISTANCE: f32 = 9.0;

impl Player {
    pub fn new(pos: [f32; 3], config: &wgpu::SurfaceConfiguration) -> Player {
        let camera = camera::Camera::new(pos, cgmath::Deg(-90.0), cgmath::Deg(-20.0));
        let projection =
            camera::Projection::new(config.width, config.height, cgmath::Deg(45.0), 0.1, 1000.0);

        let camera_controller = camera::CameraController::new(4.0, 0.4);

        Self {
            camera: camera,
            projection: projection,
            camera_controller: camera_controller,
        }
    }

    pub fn get_block_pointed_at(&self, chunks: &Vec<chunk::Chunk>) -> Option<[i32; 3]> {
        let max_distance = MAX_BLOCK_POINT_DISTANCE;
        let step = 0.1;
        let mut distance = 0.0;

        let origin = self.camera.position;
        let direction = self.camera.direction();

        const CHUNK_X_SIZE: i32 = 16;
        const CHUNK_Z_SIZE: i32 = 16;

        while distance < max_distance {
            let point = [
                origin.x + direction.x * distance,
                origin.y + direction.y * distance,
                origin.z + direction.z * distance,
            ];
            let world_block_pos = [
                point[0].floor() as i32,
                point[1].floor() as i32,
                point[2].floor() as i32,
            ];

            let chunk_x = world_block_pos[0].div_euclid(CHUNK_X_SIZE);
            let chunk_z = world_block_pos[2].div_euclid(CHUNK_Z_SIZE);

            let local_x = world_block_pos[0].rem_euclid(CHUNK_X_SIZE);
            let local_y = world_block_pos[1];
            let local_z = world_block_pos[2].rem_euclid(CHUNK_Z_SIZE);

            for chunk in chunks {
                if chunk.pos[0] == chunk_x && chunk.pos[1] == chunk_z {
                    if let Some(block) = chunk.get_block([local_x, local_y, local_z]) {
                        if !block.is_air() {
                            return Some(world_block_pos);
                        }
                    }
                    break;
                }
            }

            distance += step;
        }

        None
    }
}