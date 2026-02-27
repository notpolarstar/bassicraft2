use winit::keyboard::KeyCode;

use crate::camera;
use crate::chunk;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemStack {
    pub item_id: u32,
    pub count: u8,
}

pub const MAX_STACK: u8 = 64;
pub const INVENTORY_SIZE: usize = 36;
pub const CRAFT_SIZE: usize = 4;

impl ItemStack {
    pub const EMPTY: Self = Self { item_id: 0, count: 0 };

    pub fn new(item_id: u32, count: u8) -> Self {
        Self { item_id, count }
    }

    pub fn is_empty(self) -> bool {
        self.item_id == 0 || self.count == 0
    }

    pub fn merge(&mut self, other: ItemStack) -> u8 {
        if other.is_empty() {
            return 0;
        }
        if self.is_empty() {
            *self = other;
            return 0;
        }
        if self.item_id != other.item_id {
            return other.count;
        }
        let space = MAX_STACK.saturating_sub(self.count);
        let take = space.min(other.count);
        self.count += take;
        other.count - take
    }
}

pub struct Player {
    pub camera: camera::Camera,
    pub projection: camera::Projection,
    pub camera_controller: camera::CameraController,

    pub inventory: [ItemStack; INVENTORY_SIZE],
    pub craft_input: [ItemStack; CRAFT_SIZE],
    pub craft_output: ItemStack,
    pub held_item: ItemStack,

    pub selected_hotbar_slot: usize,
    pub cursor_locked: bool,
    pub show_inventory: bool,
}

pub const MAX_BLOCK_POINT_DISTANCE: f32 = 9.0;
pub const HOTBAR_SIZE: usize = 9;

impl Player {
    pub fn new(pos: [f32; 3], config: &wgpu::SurfaceConfiguration) -> Player {
        let camera = camera::Camera::new(pos, cgmath::Deg(-90.0), cgmath::Deg(-20.0));
        let projection =
            camera::Projection::new(config.width, config.height, cgmath::Deg(45.0), 0.1, 1000.0);
        let camera_controller = camera::CameraController::new(4.0, 0.4);

        let mut inventory = [ItemStack::EMPTY; INVENTORY_SIZE];
        for i in 0..HOTBAR_SIZE {
            inventory[i] = ItemStack::new((i + 1) as u32, 10);
        }

        Self {
            camera,
            projection,
            camera_controller,
            inventory,
            craft_input: [ItemStack::EMPTY; CRAFT_SIZE],
            craft_output: ItemStack::EMPTY,
            held_item: ItemStack::EMPTY,
            selected_hotbar_slot: 0,
            cursor_locked: false,
            show_inventory: false,
        }
    }

    pub fn selected_block(&self) -> u32 {
        self.inventory[self.selected_hotbar_slot].item_id
    }

    pub fn change_selected_block(&mut self, num: usize) {
        self.selected_hotbar_slot = num.min(HOTBAR_SIZE - 1);
    }

    pub fn set_hotbar_slot(&mut self, block_type: u32) {
        self.inventory[self.selected_hotbar_slot] = ItemStack::new(block_type, MAX_STACK);
    }

    pub fn consume_selected(&mut self) -> bool {
        let slot = &mut self.inventory[self.selected_hotbar_slot];
        if slot.is_empty() {
            return false;
        }
        slot.count -= 1;
        if slot.count == 0 {
            *slot = ItemStack::EMPTY;
        }
        true
    }

    pub fn pick_up(&mut self, item_id: u32, count: u8) -> u8 {
        let mut remaining = count;
        for slot in self.inventory.iter_mut() {
            if remaining == 0 { break; }
            if slot.item_id == item_id && slot.count < MAX_STACK {
                let space = MAX_STACK - slot.count;
                let add = space.min(remaining);
                slot.count += add;
                remaining -= add;
            }
        }
        for slot in self.inventory.iter_mut() {
            if remaining == 0 { break; }
            if slot.is_empty() {
                let add = remaining.min(MAX_STACK);
                *slot = ItemStack::new(item_id, add);
                remaining -= add;
            }
        }
        remaining
    }

    pub fn get_block_pointed_at(&self, chunks: &Vec<chunk::Chunk>) -> Option<[i32; 3]> {
        let max_distance = MAX_BLOCK_POINT_DISTANCE;
        let step = 0.05;
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
                        if !block.is_air() && !block.is_fluid() {
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

    pub fn get_block_placement_pos(&self, chunks: &Vec<chunk::Chunk>) -> Option<[i32; 3]> {
        let max_distance = MAX_BLOCK_POINT_DISTANCE;
        let step = 0.1;
        let mut distance = 0.0;

        let origin = self.camera.position;
        let direction = self.camera.direction();

        const CHUNK_X_SIZE: i32 = 16;
        const CHUNK_Z_SIZE: i32 = 16;

        let mut last_empty_pos: Option<[i32; 3]> = None;

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
                        if !block.is_air() && !block.is_fluid() {
                            return last_empty_pos;
                        } else {
                            last_empty_pos = Some(world_block_pos);
                        }
                    } else {
                        last_empty_pos = Some(world_block_pos);
                    }
                    break;
                }
            }

            distance += step;
        }

        None
    }

    pub fn process_keyboard(&mut self, key: KeyCode, state: bool) -> bool {
        let amount = if state { 1.0 } else { 0.0 };
        match key {
            KeyCode::KeyW | KeyCode::ArrowUp => {
                self.camera_controller.amount_forward = amount;
                true
            }
            KeyCode::KeyS | KeyCode::ArrowDown => {
                self.camera_controller.amount_backward = amount;
                true
            }
            KeyCode::KeyA | KeyCode::ArrowLeft => {
                self.camera_controller.amount_left = amount;
                true
            }
            KeyCode::KeyD | KeyCode::ArrowRight => {
                self.camera_controller.amount_right = amount;
                true
            }
            KeyCode::Space => {
                self.camera_controller.amount_up = amount;
                true
            }
            KeyCode::ShiftLeft => {
                self.camera_controller.amount_down = amount;
                true
            }
            KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 | KeyCode::Digit4
            | KeyCode::Digit5 | KeyCode::Digit6 | KeyCode::Digit7 | KeyCode::Digit8
            | KeyCode::Digit9 => {
                if !state {
                    return false;
                }
                self.change_selected_block(key as usize - KeyCode::Digit1 as usize);
                true
            }
            KeyCode::KeyE => {
                if !state {
                    return false;
                }
                self.show_inventory = !self.show_inventory;
                self.cursor_locked = !self.show_inventory;
                true
            }
            _ => false,
        }
    }
}