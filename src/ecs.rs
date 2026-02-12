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
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            behaviour_type: BehaviourType::Passive,
            sight_range: 16.0,
            movement_speed: 2.0,
            target_position: None,
            path: VecDeque::new(),
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
                    transform.position.y = old_pos.y;
                    physics.velocity.y = 0.0;
                    if physics.velocity.y < 0.0 || old_pos.y > transform.position.y {
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
            let mut query = self.world.query::<(&Transform, &mut Behaviour, &mut Physics)>();
            for (transform, mut behaviour, mut physics) in query.iter_mut(&mut self.world) {
                match behaviour.behaviour_type {
                    BehaviourType::Passive => {}
                    BehaviourType::Stationary => {
                        physics.velocity.x = 0.0;
                        physics.velocity.z = 0.0;
                    }
                    BehaviourType::FollowPlayer => {
                        let direction = player_position - transform.position;
                        let distance = direction.magnitude();
                        
                        if distance < behaviour.sight_range && distance > 1.0 {
                            let normalized = direction.normalize();
                            physics.velocity.x = normalized.x * behaviour.movement_speed;
                            physics.velocity.z = normalized.z * behaviour.movement_speed;
                        }
                    }
                    BehaviourType::Wander => {
                        if behaviour.target_position.is_none() || 
                           (transform.position - behaviour.target_position.unwrap()).magnitude() < 1.0 {
                            let offset = Vector3::new(
                                ((transform.position.x * 12.9898 + transform.position.z * 78.233).sin() * 43758.5453).fract() * 20.0 - 10.0,
                                0.0,
                                ((transform.position.x * 43.9898 + transform.position.z * 12.233).sin() * 23758.5453).fract() * 20.0 - 10.0,
                            );
                            behaviour.target_position = Some(transform.position + offset);
                        }
                        
                        if let Some(target) = behaviour.target_position {
                            let direction = target - transform.position;
                            let distance = direction.magnitude();
                            
                            if distance > 0.5 {
                                let normalized = direction.normalize();
                                physics.velocity.x = normalized.x * behaviour.movement_speed;
                                physics.velocity.z = normalized.z * behaviour.movement_speed;
                            }
                        }
                    }
                    BehaviourType::Aggressive => {
                        let direction = player_position - transform.position;
                        let distance = direction.magnitude();
                        
                        if distance < behaviour.sight_range {
                            let normalized = direction.normalize();
                            physics.velocity.x = normalized.x * behaviour.movement_speed * 1.5;
                            physics.velocity.z = normalized.z * behaviour.movement_speed * 1.5;
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
            movement_speed: 3.0,
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
