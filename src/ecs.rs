use bevy_ecs::prelude::*;
use cgmath::{Vector3, Quaternion, InnerSpace, Zero};
use std::collections::VecDeque;

use crate::random;

#[derive(Component, Clone, Copy, Debug)]
pub struct Transform {
    pub position: Vector3<f32>,
    pub rotation: Quaternion<f32>,
    pub scale: Vector3<f32>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vector3::zero(),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct Physics {
    pub velocity: Vector3<f32>,
    pub acceleration: Vector3<f32>,
    pub mass: f32,
    pub friction: f32,
    pub gravity_enabled: bool,
    pub on_ground: bool,
}

impl Default for Physics {
    fn default() -> Self {
        Self {
            velocity: Vector3::zero(),
            acceleration: Vector3::zero(),
            mass: 1.0,
            friction: 0.9,
            gravity_enabled: true,
            on_ground: false,
        }
    }
}

#[derive(Component, Clone)]
pub struct Model {
    pub model_name: String,
    pub model_handle: Option<usize>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Collider {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            width: 0.6,
            height: 1.8,
            depth: 0.6,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum BehaviourType {
    Passive,
    FollowPlayer,
    Wander,
    Aggressive,
    Stationary,
}

#[derive(Component, Clone, Debug)]
pub struct Behaviour {
    pub behaviour_type: BehaviourType,
    pub sight_range: f32,
    pub movement_speed: f32,
    pub target_position: Option<Vector3<f32>>,
    pub path: VecDeque<Vector3<f32>>,
    pub random_seed: f32,
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            behaviour_type: BehaviourType::Passive,
            sight_range: 16.0,
            movement_speed: 2.0,
            target_position: None,
            path: VecDeque::new(),
            random_seed: 0.0,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct EntityInfo {
    pub name: &'static str,
    pub entity_type: EntityType,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntityType {
    Mob,
    Item,
    Projectile,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Player;

#[derive(Component, Clone, Copy, Debug)]
pub struct NetId {
    pub id: u32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct RemoteEntity {
    pub net_id: u32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct RemotePlayer {
    pub player_id: u32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct NetworkTarget {
    pub position: Vector3<f32>,
    pub rotation: Quaternion<f32>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PlayerInput {
    pub forward: f32,
    pub backward: f32,
    pub left: f32,
    pub right: f32,
    pub jump: bool,
    pub movement_speed: f32,
}

impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            forward: 0.0,
            backward: 0.0,
            left: 0.0,
            right: 0.0,
            jump: false,
            movement_speed: 6.5,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Particle {
    pub block_type: u32,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            block_type: 0,
            lifetime: 0.0,
            max_lifetime: 1.0,
        }
    }
}

pub fn health_system(
    mut commands: Commands,
    query: Query<(Entity, &Health)>,
) {
    for (entity, health) in query.iter() {
        if health.current <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

pub fn particle_lifetime_system(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Particle)>,
    time: Res<Time>,
) {
    let dt = time.delta_seconds();
    for (entity, mut particle) in query.iter_mut() {
        particle.lifetime += dt;
        if particle.lifetime >= particle.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Resource, Default)]
pub struct Time {
    delta: f32,
}

impl Time {
    pub fn delta_seconds(&self) -> f32 {
        self.delta
    }

    pub fn update(&mut self, delta: f32) {
        self.delta = delta;
    }
}

pub struct EcsWorld {
    pub world: World,
    pub schedule: Schedule,
    net_id_counter: u32,
}

impl EcsWorld {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Time::default());
        
        let mut schedule = Schedule::default();

        schedule.add_systems((health_system, particle_lifetime_system));
        
        Self { world, schedule, net_id_counter: 1 }
    }

    pub fn alloc_net_id(&mut self) -> u32 {
        let id = self.net_id_counter;
        self.net_id_counter += 1;
        id
    }

    fn is_block_solid_at(chunks: &[crate::chunk::Chunk], world_pos: Vector3<f32>) -> bool {
        const CHUNK_X_SIZE: i32 = 16;
        const CHUNK_Z_SIZE: i32 = 16;
        
        let block_pos = [
            world_pos.x.floor() as i32,
            world_pos.y.floor() as i32,
            world_pos.z.floor() as i32,
        ];
        
        let chunk_x = block_pos[0].div_euclid(CHUNK_X_SIZE);
        let chunk_z = block_pos[2].div_euclid(CHUNK_Z_SIZE);
        
        let local_x = block_pos[0].rem_euclid(CHUNK_X_SIZE);
        let local_y = block_pos[1];
        let local_z = block_pos[2].rem_euclid(CHUNK_Z_SIZE);
        
        for chunk in chunks {
            if chunk.pos[0] == chunk_x && chunk.pos[1] == chunk_z {
                if let Some(block) = chunk.get_block([local_x, local_y, local_z]) {
                    return !block.is_air();
                }
                return false;
            }
        }
        false
    }

    fn check_collision(chunks: &[crate::chunk::Chunk], position: Vector3<f32>, collider: &Collider) -> bool {
        let half_width = collider.width / 2.0;
        let half_depth = collider.depth / 2.0;

        let check_points = [
            Vector3::new(position.x - half_width, position.y, position.z - half_depth),
            Vector3::new(position.x + half_width, position.y, position.z - half_depth),
            Vector3::new(position.x - half_width, position.y, position.z + half_depth),
            Vector3::new(position.x + half_width, position.y, position.z + half_depth),

            Vector3::new(position.x, position.y, position.z),

            Vector3::new(position.x - half_width, position.y + collider.height / 2.0, position.z - half_depth),
            Vector3::new(position.x + half_width, position.y + collider.height / 2.0, position.z - half_depth),
            Vector3::new(position.x - half_width, position.y + collider.height / 2.0, position.z + half_depth),
            Vector3::new(position.x + half_width, position.y + collider.height / 2.0, position.z + half_depth),

            Vector3::new(position.x - half_width, position.y + collider.height, position.z - half_depth),
            Vector3::new(position.x + half_width, position.y + collider.height, position.z - half_depth),
            Vector3::new(position.x - half_width, position.y + collider.height, position.z + half_depth),
            Vector3::new(position.x + half_width, position.y + collider.height, position.z + half_depth),
        ];
        
        for point in &check_points {
            if Self::is_block_solid_at(chunks, *point) {
                return true;
            }
        }
        
        false
    }
    
    pub fn update(&mut self, dt: f32, chunks: &[crate::chunk::Chunk], camera_yaw: f32) {
        if let Some(mut time) = self.world.get_resource_mut::<Time>() {
            time.update(dt);
        }
        
        {
            let mut query = self.world.query::<(&Player, &PlayerInput, &mut Physics)>();
            for (_player, input, mut physics) in query.iter_mut(&mut self.world) {
                let forward = input.forward - input.backward;
                let right = input.right - input.left;

                let forward_dir = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                let right_dir = Vector3::new(-camera_yaw.sin(), 0.0, camera_yaw.cos());

                let move_dir = forward_dir * forward + right_dir * right;
                if move_dir.magnitude2() > 0.01 {
                    let normalized = move_dir.normalize();
                    physics.velocity.x = normalized.x * input.movement_speed;
                    physics.velocity.z = normalized.z * input.movement_speed;
                }

                if input.jump && physics.on_ground {
                    physics.velocity.y = 8.0;
                }
            }
        }

        let player_position = {
            let mut query = self.world.query::<(&Player, &Transform)>();
            query.iter(&self.world)
                .next()
                .map(|(_, t)| t.position)
                .unwrap_or(Vector3::zero())
        };
        
        {
            let mut query = self.world.query::<(&mut Transform, &mut Physics, &Collider)>();
            for (mut transform, mut physics, collider) in query.iter_mut(&mut self.world) {
                const GRAVITY: f32 = -20.0;

                if physics.gravity_enabled && !physics.on_ground {
                    physics.acceleration.y = GRAVITY;
                } else {
                    physics.acceleration.y = 0.0;
                }

                let accel = physics.acceleration;
                physics.velocity += accel * dt;

                let friction = physics.friction;
                physics.velocity.x *= friction;
                physics.velocity.z *= friction;

                let old_pos = transform.position;

                transform.position.x += physics.velocity.x * dt;
                if Self::check_collision(chunks, transform.position, collider) {
                    transform.position.x = old_pos.x;
                    physics.velocity.x = 0.0;
                }

                transform.position.y += physics.velocity.y * dt;
                let y_collision = Self::check_collision(chunks, transform.position, collider);
                if y_collision {
                    let was_falling = physics.velocity.y < 0.0;
                    transform.position.y = old_pos.y;
                    physics.velocity.y = 0.0;
                    if was_falling {
                        physics.on_ground = true;
                    }
                } else {
                    physics.on_ground = false;
                }

                transform.position.z += physics.velocity.z * dt;
                if Self::check_collision(chunks, transform.position, collider) {
                    transform.position.z = old_pos.z;
                    physics.velocity.z = 0.0;
                }

                // temp, is here to see if collision messes up
                if transform.position.y < 0.0 {
                    transform.position.y = 0.0;
                    physics.velocity.y = 0.0;
                    physics.on_ground = true;
                }
            }
        }

        {
            let mut query = self.world.query::<(&mut Transform, &mut Behaviour, &mut Physics)>();
            for (mut transform, mut behaviour, mut physics) in query.iter_mut(&mut self.world) {
                match behaviour.behaviour_type {
                    BehaviourType::Passive | BehaviourType::Stationary => {
                        physics.velocity.x *= 0.8;
                        physics.velocity.z *= 0.8;
                    }
                    BehaviourType::FollowPlayer => {
                        let direction = player_position - transform.position;
                        let distance = direction.magnitude();
                        
                        if distance <= behaviour.sight_range && distance > 1.0 {
                            let normalized = direction.normalize();

                            let target_yaw = (-normalized.x).atan2(-normalized.z);
                            let current_yaw = 2.0 * transform.rotation.v.y.atan2(transform.rotation.s);
                            let mut yaw_diff = target_yaw - current_yaw;

                            while yaw_diff > std::f32::consts::PI { yaw_diff -= 2.0 * std::f32::consts::PI; }
                            while yaw_diff < -std::f32::consts::PI { yaw_diff += 2.0 * std::f32::consts::PI; }

                            let rotation_speed = 5.0 * dt;
                            let yaw_change = yaw_diff.clamp(-rotation_speed, rotation_speed);
                            let new_yaw = current_yaw + yaw_change;
                            transform.rotation = Quaternion::new(
                                (new_yaw / 2.0).cos(),
                                0.0,
                                (new_yaw / 2.0).sin(),
                                0.0,
                            );

                            let check_near = transform.position + Vector3::new(normalized.x * 0.5, 0.0, normalized.z * 0.5);
                            let check_mid = transform.position + Vector3::new(normalized.x * 1.0, 0.0, normalized.z * 1.0);
                            let check_far = transform.position + Vector3::new(normalized.x * 1.5, 0.0, normalized.z * 1.5);

                            let obstacle_near = Self::is_block_solid_at(chunks, check_near) || 
                                              Self::is_block_solid_at(chunks, check_near + Vector3::new(0.0, 1.0, 0.0));
                            let obstacle_mid = Self::is_block_solid_at(chunks, check_mid) || 
                                             Self::is_block_solid_at(chunks, check_mid + Vector3::new(0.0, 1.0, 0.0));
                            let obstacle_far = Self::is_block_solid_at(chunks, check_far);

                            if (obstacle_near || obstacle_mid || obstacle_far) && physics.on_ground {
                                physics.velocity.y = 8.0;
                            }

                            if obstacle_near {
                                let perpendicular = Vector3::new(-normalized.z, 0.0, normalized.x);
                                let left_path = transform.position + perpendicular * 1.0;
                                let right_path = transform.position - perpendicular * 1.0;
                                
                                if !Self::is_block_solid_at(chunks, left_path) {
                                    physics.velocity.x = perpendicular.x * behaviour.movement_speed;
                                    physics.velocity.z = perpendicular.z * behaviour.movement_speed;
                                } else if !Self::is_block_solid_at(chunks, right_path) {
                                    physics.velocity.x = -perpendicular.x * behaviour.movement_speed;
                                    physics.velocity.z = -perpendicular.z * behaviour.movement_speed;
                                } else {
                                    physics.velocity.x = normalized.x * behaviour.movement_speed * 0.3;
                                    physics.velocity.z = normalized.z * behaviour.movement_speed * 0.3;
                                }
                            } else {
                                physics.velocity.x = normalized.x * behaviour.movement_speed;
                                physics.velocity.z = normalized.z * behaviour.movement_speed;
                            }
                        } else {
                            physics.velocity.x *= 0.8;
                            physics.velocity.z *= 0.8;
                        }
                    }
                    BehaviourType::Aggressive => {
                        let direction = player_position - transform.position;
                        let distance = direction.magnitude();
                        
                        if distance > 1.0 {
                            let normalized = direction.normalize();

                            let target_yaw = (-normalized.x).atan2(-normalized.z);
                            let current_yaw = 2.0 * transform.rotation.v.y.atan2(transform.rotation.s);
                            let mut yaw_diff = target_yaw - current_yaw;
                            
                            while yaw_diff > std::f32::consts::PI { yaw_diff -= 2.0 * std::f32::consts::PI; }
                            while yaw_diff < -std::f32::consts::PI { yaw_diff += 2.0 * std::f32::consts::PI; }
                            
                            let rotation_speed = 8.0 * dt;
                            let yaw_change = yaw_diff.clamp(-rotation_speed, rotation_speed);
                            let new_yaw = current_yaw + yaw_change;
                            transform.rotation = Quaternion::new(
                                (new_yaw / 2.0).cos(),
                                0.0,
                                (new_yaw / 2.0).sin(),
                                0.0,
                            );

                            let check_near = transform.position + Vector3::new(normalized.x * 0.7, 0.0, normalized.z * 0.7);
                            let check_mid = transform.position + Vector3::new(normalized.x * 1.2, 0.0, normalized.z * 1.2);
                            
                            let obstacle_near = Self::is_block_solid_at(chunks, check_near) || 
                                              Self::is_block_solid_at(chunks, check_near + Vector3::new(0.0, 1.0, 0.0));
                            let obstacle_mid = Self::is_block_solid_at(chunks, check_mid);
                            
                            if (obstacle_near || obstacle_mid) && physics.on_ground {
                                physics.velocity.y = 8.5;
                            }
                            
                            if obstacle_near {
                                let perpendicular = Vector3::new(-normalized.z, 0.0, normalized.x);
                                let left_clear = !Self::is_block_solid_at(chunks, transform.position + perpendicular * 1.5);
                                
                                if left_clear {
                                    let alt_dir = (normalized + perpendicular * 0.5).normalize();
                                    physics.velocity.x = alt_dir.x * behaviour.movement_speed * 1.5;
                                    physics.velocity.z = alt_dir.z * behaviour.movement_speed * 1.5;
                                } else {
                                    let alt_dir = (normalized - perpendicular * 0.5).normalize();
                                    physics.velocity.x = alt_dir.x * behaviour.movement_speed * 1.5;
                                    physics.velocity.z = alt_dir.z * behaviour.movement_speed * 1.5;
                                }
                            } else {
                                physics.velocity.x = normalized.x * behaviour.movement_speed * 1.5;
                                physics.velocity.z = normalized.z * behaviour.movement_speed * 1.5;
                            }
                        }
                    }
                    BehaviourType::Wander => {
                        let is_stuck = physics.velocity.magnitude() < 0.1 && physics.on_ground;
                        
                        let reached_target = if let Some(target) = behaviour.target_position {
                            (transform.position - target).magnitude() < 2.5
                        } else {
                            true
                        };

                        if behaviour.target_position.is_none() || reached_target || is_stuck {
                            let mut attempts = 0;
                            let mut valid_target = None;

                            while attempts < 16 && valid_target.is_none() {
                                let angle = ((behaviour.random_seed * 91.3 + transform.position.x * 12.9898 + transform.position.z * 78.233 + attempts as f32 * 43.1).sin() * 43758.5453).fract() * 2.0 * std::f32::consts::PI;
                                let distance = 5.0 + ((behaviour.random_seed * 73.4 + transform.position.x * 43.9898 + attempts as f32 * 17.3).sin() * 23758.5453).fract() * 10.0;
                                
                                let offset = Vector3::new(
                                    angle.cos() * distance,
                                    0.0,
                                    angle.sin() * distance,
                                );
                                let candidate = transform.position + offset;

                                if !Self::is_block_solid_at(chunks, candidate) {
                                    let mut has_ground = false;
                                    for y_check in 0..5 {
                                        let ground_check = Vector3::new(candidate.x, candidate.y - y_check as f32, candidate.z);
                                        if Self::is_block_solid_at(chunks, ground_check) {
                                            has_ground = true;
                                            break;
                                        }
                                    }
                                    
                                    if has_ground {
                                        valid_target = Some(candidate);
                                    }
                                }
                                attempts += 1;
                            }

                            if let Some(target) = valid_target {
                                behaviour.target_position = Some(target);
                            } else {
                                let fallback_angle = ((behaviour.random_seed * 123.4 + transform.position.x * 78.233 + transform.position.z * 127.1).sin() * 43758.5453).fract() * 2.0 * std::f32::consts::PI;
                                let fallback_target = transform.position + Vector3::new(
                                    fallback_angle.cos() * 8.0,
                                    0.0,
                                    fallback_angle.sin() * 8.0,
                                );
                                behaviour.target_position = Some(fallback_target);
                            }
                        }

                        if let Some(target) = behaviour.target_position {
                            let direction = target - transform.position;
                            let normalized = direction.normalize();

                            let target_yaw = (-normalized.x).atan2(-normalized.z);
                            let current_yaw = 2.0 * transform.rotation.v.y.atan2(transform.rotation.s);
                            let mut yaw_diff = target_yaw - current_yaw;
                            
                            while yaw_diff > std::f32::consts::PI { yaw_diff -= 2.0 * std::f32::consts::PI; }
                            while yaw_diff < -std::f32::consts::PI { yaw_diff += 2.0 * std::f32::consts::PI; }

                            let rotation_speed = 3.0 * dt;
                            let yaw_change = yaw_diff.clamp(-rotation_speed, rotation_speed);
                            let new_yaw = current_yaw + yaw_change;
                            transform.rotation = Quaternion::new(
                                (new_yaw / 2.0).cos(),
                                0.0,
                                (new_yaw / 2.0).sin(),
                                0.0,
                            );

                            physics.velocity.x = normalized.x * behaviour.movement_speed;
                            physics.velocity.z = normalized.z * behaviour.movement_speed;

                            let check_near = transform.position + Vector3::new(normalized.x * 0.8, 0.0, normalized.z * 0.8);
                            let check_mid = transform.position + Vector3::new(normalized.x * 1.5, 0.0, normalized.z * 1.5);
                            
                            let obstacle_near = Self::is_block_solid_at(chunks, check_near) || 
                                              Self::is_block_solid_at(chunks, check_near + Vector3::new(0.0, 1.0, 0.0));
                            let obstacle_mid = Self::is_block_solid_at(chunks, check_mid);

                            if (obstacle_near || obstacle_mid) && physics.on_ground {
                                physics.velocity.y = 7.5;
                            }
                        }
                    }
                }
            }
        }

        self.schedule.run(&mut self.world);

        {
            use std::collections::HashMap;

            let dynamic: Vec<(Entity, Vector3<f32>, Collider)> = {
                let mut q = self.world.query::<(Entity, &Transform, &Physics, &Collider, Option<&Particle>)>();
                q.iter(&self.world)
                    .filter(|(_, _, _, _, p)| p.is_none())
                    .map(|(e, t, _, c, _)| (e, t.position, *c))
                    .collect()
            };

            let statics: Vec<(Vector3<f32>, Collider)> = {
                let mut q = self.world.query_filtered::<(&Transform, &Collider), Without<Physics>>();
                q.iter(&self.world)
                    .map(|(t, c)| (t.position, *c))
                    .collect()
            };

            let mut pushes: HashMap<Entity, Vector3<f32>> = HashMap::new();

            for i in 0..dynamic.len() {
                for j in (i + 1)..dynamic.len() {
                    let (ea, pos_a, col_a) = dynamic[i];
                    let (eb, pos_b, col_b) = dynamic[j];

                    let ca = Vector3::new(pos_a.x, pos_a.y + col_a.height * 0.5, pos_a.z);
                    let cb = Vector3::new(pos_b.x, pos_b.y + col_b.height * 0.5, pos_b.z);

                    let half_ax = col_a.width  * 0.5;
                    let half_ay = col_a.height * 0.5;
                    let half_az = col_a.depth  * 0.5;
                    let half_bx = col_b.width  * 0.5;
                    let half_by = col_b.height * 0.5;
                    let half_bz = col_b.depth  * 0.5;

                    let diff = cb - ca;
                    let overlap_x = (half_ax + half_bx) - diff.x.abs();
                    let overlap_y = (half_ay + half_by) - diff.y.abs();
                    let overlap_z = (half_az + half_bz) - diff.z.abs();

                    if overlap_x <= 0.0 || overlap_y <= 0.0 || overlap_z <= 0.0 { continue; }

                    let (push_dir, push_amt) = if overlap_x <= overlap_z {
                        let sign = if diff.x >= 0.0 { -1.0_f32 } else { 1.0_f32 };
                        (Vector3::new(sign, 0.0, 0.0), overlap_x * 0.5)
                    } else {
                        let sign = if diff.z >= 0.0 { -1.0_f32 } else { 1.0_f32 };
                        (Vector3::new(0.0, 0.0, sign), overlap_z * 0.5)
                    };

                    let push = push_dir * push_amt;
                    *pushes.entry(ea).or_insert(Vector3::zero()) += push;
                    *pushes.entry(eb).or_insert(Vector3::zero()) -= push;
                }
            }

            for (ent, pos_d, col_d) in &dynamic {
                for (pos_s, col_s) in &statics {
                    let cd = Vector3::new(pos_d.x, pos_d.y + col_d.height * 0.5, pos_d.z);
                    let cs = Vector3::new(pos_s.x, pos_s.y + col_s.height * 0.5, pos_s.z);

                    let half_dx = col_d.width  * 0.5;
                    let half_dy = col_d.height * 0.5;
                    let half_dz = col_d.depth  * 0.5;
                    let half_sx = col_s.width  * 0.5;
                    let half_sy = col_s.height * 0.5;
                    let half_sz = col_s.depth  * 0.5;

                    let diff = cs - cd;
                    let overlap_x = (half_dx + half_sx) - diff.x.abs();
                    let overlap_y = (half_dy + half_sy) - diff.y.abs();
                    let overlap_z = (half_dz + half_sz) - diff.z.abs();

                    if overlap_x <= 0.0 || overlap_y <= 0.0 || overlap_z <= 0.0 { continue; }

                    let push = if overlap_x <= overlap_z {
                        let sign = if diff.x >= 0.0 { -1.0_f32 } else { 1.0_f32 };
                        Vector3::new(sign * overlap_x, 0.0, 0.0)
                    } else {
                        let sign = if diff.z >= 0.0 { -1.0_f32 } else { 1.0_f32 };
                        Vector3::new(0.0, 0.0, sign * overlap_z)
                    };
                    *pushes.entry(*ent).or_insert(Vector3::zero()) += push;
                }
            }

            let mut q = self.world.query::<(Entity, &mut Transform, &mut Physics, &Collider, Option<&Particle>)>();
            for (entity, mut transform, mut physics, collider, is_particle) in q.iter_mut(&mut self.world) {
                if is_particle.is_some() { continue; }
                if let Some(&push) = pushes.get(&entity) {
                    let new_pos = transform.position + push;
                    if !Self::check_collision(chunks, new_pos, collider) {
                        transform.position = new_pos;
                        if push.x.abs() > 0.0 { physics.velocity.x *= 0.3; }
                        if push.z.abs() > 0.0 { physics.velocity.z *= 0.3; }
                    }
                }
            }
        }

        {
            let alpha = (15.0_f32 * dt).min(1.0);
            let inv = 1.0 - alpha;
            let mut q = self.world.query::<(&mut Transform, &NetworkTarget)>();
            for (mut transform, target) in q.iter_mut(&mut self.world) {
                transform.position = transform.position + (target.position - transform.position) * alpha;

                let cs = transform.rotation.s;
                let cv = transform.rotation.v;
                let mut ts = target.rotation.s;
                let mut tv = target.rotation.v;
                if cs * ts + cv.x * tv.x + cv.y * tv.y + cv.z * tv.z < 0.0 {
                    ts = -ts; tv = -tv;
                }
                let bs = cs * inv + ts * alpha;
                let bx = cv.x * inv + tv.x * alpha;
                let by = cv.y * inv + tv.y * alpha;
                let bz = cv.z * inv + tv.z * alpha;
                let len = (bs * bs + bx * bx + by * by + bz * bz).sqrt();
                if len > 1e-6 {
                    transform.rotation = Quaternion::new(bs / len, bx / len, by / len, bz / len);
                }
            }
        }
    }
    
    pub fn get_entities_render_data(&mut self) -> Vec<(Vector3<f32>, Quaternion<f32>, String)> {
        let mut entities = Vec::new();
        let mut query = self.world.query::<(&Transform, &Model)>();
        
        for (transform, model) in query.iter(&self.world) {
            entities.push((transform.position, transform.rotation, model.model_name.clone()));
        }
        
        entities
    }
    
    pub fn get_particles_render_data(&mut self) -> Vec<(Vector3<f32>, u32, f32)> {
        let mut particles = Vec::new();
        let mut query = self.world.query::<(&Transform, &Particle)>();
        
        for (transform, particle) in query.iter(&self.world) {
            let alpha = 1.0 - (particle.lifetime / particle.max_lifetime);
            particles.push((transform.position, particle.block_type, alpha));
        }
        
        particles
    }

    pub fn sync_remote_players(
        &mut self,
        states: &[crate::network::PlayerState],
        my_id: Option<u32>,
    ) {
        use std::collections::{HashMap, HashSet};

        let existing: HashMap<u32, Entity> = {
            let mut q = self.world.query::<(Entity, &RemotePlayer)>();
            q.iter(&self.world).map(|(e, rp)| (rp.player_id, e)).collect()
        };

        let snapshot_ids: HashSet<u32> = states.iter().map(|s| s.id).collect();

        let to_despawn: Vec<Entity> = existing
            .iter()
            .filter(|(id, _)| !snapshot_ids.contains(id))
            .map(|(_, &e)| e)
            .collect();
        for entity in to_despawn {
            self.world.despawn(entity);
        }

        for state in states {
            if Some(state.id) == my_id {
                continue;
            }
            let pos = Vector3::new(state.position.x, state.position.y, state.position.z);
            
            let model_yaw = -state.yaw - std::f32::consts::FRAC_PI_2;
            let rotation = Quaternion::new(
                (model_yaw / 2.0).cos(),
                0.0,
                (model_yaw / 2.0).sin(),
                0.0,
            );

            if let Some(&entity) = existing.get(&state.id) {
                if let Some(mut target) = self.world.get_mut::<NetworkTarget>(entity) {
                    target.position = pos;
                    target.rotation = rotation;
                }
            } else {
                spawn_remote_player(&mut self.world, state.id, pos);
            }
        }
    }

    pub fn get_networked_entities_data(&mut self) -> Vec<crate::network::EntityState> {
        let mut result = Vec::new();
        let mut q = self.world.query::<(&Transform, &Model, &NetId)>();
        for (transform, model, net_id) in q.iter(&self.world) {
            let yaw = 2.0 * transform.rotation.v.y.atan2(transform.rotation.s);
            result.push(crate::network::EntityState {
                net_id: net_id.id,
                position: crate::network::Vec3Net::new(
                    transform.position.x,
                    transform.position.y,
                    transform.position.z,
                ),
                yaw,
                model_name: model.model_name.clone(),
            });
        }
        result
    }

    pub fn sync_network_entities(&mut self, entities: &[crate::network::EntityState]) {
        use std::collections::{HashMap, HashSet};

        let existing: HashMap<u32, Entity> = {
            let mut q = self.world.query::<(Entity, &RemoteEntity)>();
            q.iter(&self.world).map(|(e, re)| (re.net_id, e)).collect()
        };

        let incoming_ids: HashSet<u32> = entities.iter().map(|s| s.net_id).collect();

        let to_despawn: Vec<Entity> = existing
            .iter()
            .filter(|(id, _)| !incoming_ids.contains(id))
            .map(|(_, &e)| e)
            .collect();
        for e in to_despawn {
            self.world.despawn(e);
        }

        for state in entities {
            let pos = Vector3::new(state.position.x, state.position.y, state.position.z);
            let rotation = Quaternion::new(
                (state.yaw / 2.0).cos(),
                0.0,
                (state.yaw / 2.0).sin(),
                0.0,
            );

            if let Some(&entity) = existing.get(&state.net_id) {
                if let Some(mut target) = self.world.get_mut::<NetworkTarget>(entity) {
                    target.position = pos;
                    target.rotation = rotation;
                }
            } else {
                self.world.spawn((
                    RemoteEntity { net_id: state.net_id },
                    Transform { position: pos, rotation, ..Default::default() },
                    Model { model_name: state.model_name.clone(), model_handle: None },
                    Collider::default(),
                    NetworkTarget { position: pos, rotation },
                ));
            }
        }
    }

    pub fn remove_remote_player(&mut self, player_id: u32) {
        let entity = {
            let mut q = self.world.query::<(Entity, &RemotePlayer)>();
            q.iter(&self.world)
                .find(|(_, rp)| rp.player_id == player_id)
                .map(|(e, _)| e)
        };
        if let Some(entity) = entity {
            self.world.despawn(entity);
        }
    }

    pub fn clear_remote_entities(&mut self) {
        let to_despawn: Vec<Entity> = {
            let mut q = self.world.query_filtered::<Entity, Or<(With<RemotePlayer>, With<RemoteEntity>)>>();
            q.iter(&self.world).collect()
        };
        for entity in to_despawn {
            self.world.despawn(entity);
        }
    }

    pub fn clear_local_mobs(&mut self) {
        let to_despawn: Vec<Entity> = {
            let mut q = self.world.query_filtered::<Entity, With<NetId>>();
            q.iter(&self.world).collect()
        };
        for entity in to_despawn {
            self.world.despawn(entity);
        }
    }

    pub fn get_player_position(&mut self) -> Option<Vector3<f32>> {
        let mut query = self.world.query::<(&Player, &Transform)>();
        query.iter(&self.world)
            .next()
            .map(|(_, t)| t.position)
    }
    
    pub fn update_player_input(&mut self, forward: f32, backward: f32, left: f32, right: f32, jump: bool) {
        let mut query = self.world.query::<(&Player, &mut PlayerInput)>();
        for (_player, mut input) in query.iter_mut(&mut self.world) {
            input.forward = forward;
            input.backward = backward;
            input.left = left;
            input.right = right;
            input.jump = jump;
        }
    }
}

pub fn spawn_following_mob(world: &mut World, position: Vector3<f32>, model_name: String, net_id: u32) -> Entity {
    let random_seed = random::get_random_f32().unwrap();
    let mut entity = world.spawn_empty();
    entity.insert((
        Transform {
            position,
            ..Default::default()
        },
        Physics::default(),
        Collider::default(),
        Behaviour {
            behaviour_type: BehaviourType::FollowPlayer,
            movement_speed: 4.0,
            sight_range: 20.0,
            random_seed,
            ..Default::default()
        },
        Health::default(),
        Model {
            model_name,
            model_handle: None,
        },
        EntityInfo {
            name: "follow",
            entity_type: EntityType::Mob,
        },
        NetId { id: net_id },
    ));
    entity.id()
}

pub fn spawn_wandering_mob(world: &mut World, position: Vector3<f32>, model_name: String, net_id: u32) -> Entity {
    let random_seed = random::get_random_f32().unwrap();
    let mut entity = world.spawn_empty();
    entity.insert((
        Transform {
            position,
            ..Default::default()
        },
        Physics::default(),
        Collider::default(),
        Behaviour {
            behaviour_type: BehaviourType::Wander,
            movement_speed: 2.0,
            random_seed,
            ..Default::default()
        },
        Health::default(),
        Model {
            model_name,
            model_handle: None,
        },
        EntityInfo {
            name: "wander",
            entity_type: EntityType::Mob,
        },
        NetId { id: net_id },
    ));
    entity.id()
}

pub fn spawn_player(world: &mut World, position: Vector3<f32>) -> Entity {
    let mut entity = world.spawn_empty();
    entity.insert((
        Player,
        Transform {
            position,
            ..Default::default()
        },
        Physics {
            friction: 0.8,
            ..Default::default()
        },
        Collider {
            width: 0.6,
            height: 1.8,
            depth: 0.6,
        },
        PlayerInput::default(),
        Health {
            current: 100.0,
            max: 100.0,
        },
    ));
    entity.id()
}

pub fn spawn_remote_player(world: &mut World, player_id: u32, position: Vector3<f32>) -> Entity {
    let identity = Quaternion::new(1.0, 0.0, 0.0, 0.0);
    let mut entity = world.spawn_empty();
    entity.insert((
        RemotePlayer { player_id },
        Transform {
            position,
            ..Default::default()
        },
        Model {
            model_name: "steve.obj".to_string(),
            model_handle: None,
        },
        Collider::default(),
        NetworkTarget { position, rotation: identity },
    ));
    entity.id()
}

pub fn spawn_particle(world: &mut World, position: Vector3<f32>, block_type: u32, velocity: Vector3<f32>) -> Entity {
    let random_factor = ((position.x * 12.9898 + position.y * 78.233 + position.z * 45.164).sin() * 43758.5453).fract();
    
    let mut entity = world.spawn_empty();
    entity.insert((
        Transform {
            position,
            scale: Vector3::new(0.15, 0.15, 0.15),
            ..Default::default()
        },
        Physics {
            velocity,
            gravity_enabled: true,
            friction: 0.98,
            ..Default::default()
        },
        Collider {
            width: 0.15,
            height: 0.15,
            depth: 0.15,
        },
        Particle {
            block_type,
            lifetime: 0.0,
            max_lifetime: 0.8 + random_factor * 0.4,
        },
    ));
    entity.id()
}
