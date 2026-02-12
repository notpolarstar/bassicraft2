use bevy_ecs::prelude::*;
use cgmath::{Vector3, Quaternion, InnerSpace, Zero};
use std::collections::VecDeque;

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

pub struct EcsWorld {
    pub world: World,
    pub schedule: Schedule,
}

impl EcsWorld {
    pub fn new() -> Self {
        let world = World::new();
        let mut schedule = Schedule::default();

        schedule.add_systems(health_system);
        
        Self { world, schedule }
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
    
    pub fn update(&mut self, dt: f32, player_position: Vector3<f32>, chunks: &[crate::chunk::Chunk]) {
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
    }
    
    pub fn get_entities_render_data(&mut self) -> Vec<(Vector3<f32>, Quaternion<f32>, String)> {
        let mut entities = Vec::new();
        let mut query = self.world.query::<(&Transform, &Model)>();
        
        for (transform, model) in query.iter(&self.world) {
            entities.push((transform.position, transform.rotation, model.model_name.clone()));
        }
        
        entities
    }
}

pub fn spawn_following_mob(world: &mut World, position: Vector3<f32>, model_name: String) -> Entity {
    let random_seed = ((position.x * 12.9898 + position.y * 78.233 + position.z * 45.164).sin() * 43758.5453).fract() * 1000.0;
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
    ));
    entity.id()
}

pub fn spawn_wandering_mob(world: &mut World, position: Vector3<f32>, model_name: String) -> Entity {
    let random_seed = ((position.x * 12.9898 + position.y * 78.233 + position.z * 45.164).sin() * 43758.5453).fract() * 1000.0;
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
    ));
    entity.id()
}
