mod random;

mod camera;
mod player;
mod model;
mod resources;
mod texture;

mod world;
mod texture_atlas;
mod block;
mod chunk;

mod gui;

mod ecs;

mod particles;

mod network;

mod instance;
mod game;
mod state;
mod app;

pub use app::{App, run};
pub use game::{Game, GameStates};
pub use state::State;
pub use instance::OPENGL_TO_WGPU_MATRIX;

#[cfg(target_arch = "wasm32")]
pub use app::run_web;
