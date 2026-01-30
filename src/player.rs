use crate::camera;

pub struct Player {
    pub camera: camera::Camera,
    pub projection: camera::Projection,
    pub camera_controller: camera::CameraController,
}

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
}