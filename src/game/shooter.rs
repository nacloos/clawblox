use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use uuid::Uuid;

pub const ARENA_SIZE: f32 = 50.0;
pub const WALL_HEIGHT: f32 = 3.0;
pub const PLAYER_SPEED: f32 = 5.0;
pub const BULLET_SPEED: f32 = 30.0;
pub const BULLET_DAMAGE: i32 = 20;
pub const INITIAL_HEALTH: i32 = 100;
pub const INITIAL_AMMO: i32 = 30;
pub const RESPAWN_TIME: f32 = 5.0;
pub const PICKUP_RESPAWN_TIME: f32 = 10.0;

#[derive(Component)]
pub struct Player {
    pub agent_id: Uuid,
    pub speed: f32,
    pub target_position: Option<Vec3>,
    pub ammo: i32,
    pub score: i32,
}

#[derive(Component)]
pub struct Health {
    pub current: i32,
    pub max: i32,
    pub respawn_timer: f32,
}

#[derive(Component)]
pub struct Bullet {
    pub shooter_id: Uuid,
    pub direction: Vec3,
    pub origin: Vec3,
}

#[derive(Component)]
pub struct AiEnemy {
    pub speed: f32,
    pub move_direction: Vec3,
    pub time_since_direction_change: f32,
    pub shoot_cooldown: f32,
}

#[derive(Component)]
pub struct Pickup {
    pub pickup_type: PickupType,
    pub respawn_timer: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickupType {
    Health,
    Ammo,
}

#[derive(Component)]
pub struct Wall;

#[derive(Component)]
pub struct Ground;

pub fn spawn_arena(commands: &mut Commands) {
    commands.spawn((
        Ground,
        Transform::from_translation(Vec3::ZERO),
        Collider::cuboid(ARENA_SIZE, 0.1, ARENA_SIZE),
        RigidBody::Fixed,
    ));

    let wall_positions = [
        (Vec3::new(ARENA_SIZE, WALL_HEIGHT / 2.0, 0.0), Vec3::new(0.5, WALL_HEIGHT / 2.0, ARENA_SIZE)),
        (Vec3::new(-ARENA_SIZE, WALL_HEIGHT / 2.0, 0.0), Vec3::new(0.5, WALL_HEIGHT / 2.0, ARENA_SIZE)),
        (Vec3::new(0.0, WALL_HEIGHT / 2.0, ARENA_SIZE), Vec3::new(ARENA_SIZE, WALL_HEIGHT / 2.0, 0.5)),
        (Vec3::new(0.0, WALL_HEIGHT / 2.0, -ARENA_SIZE), Vec3::new(ARENA_SIZE, WALL_HEIGHT / 2.0, 0.5)),
    ];

    for (pos, half_extents) in wall_positions {
        commands.spawn((
            Wall,
            Transform::from_translation(pos),
            Collider::cuboid(half_extents.x, half_extents.y, half_extents.z),
            RigidBody::Fixed,
        ));
    }

    let obstacle_positions = [
        Vec3::new(10.0, 1.5, 10.0),
        Vec3::new(-10.0, 1.5, -10.0),
        Vec3::new(20.0, 1.5, -15.0),
        Vec3::new(-15.0, 1.5, 20.0),
    ];

    for pos in obstacle_positions {
        commands.spawn((
            Wall,
            Transform::from_translation(pos),
            Collider::cuboid(2.0, 1.5, 2.0),
            RigidBody::Fixed,
        ));
    }
}

pub fn spawn_pickups(commands: &mut Commands) {
    let pickup_positions = [
        (Vec3::new(15.0, 0.5, 0.0), PickupType::Ammo),
        (Vec3::new(-15.0, 0.5, 0.0), PickupType::Ammo),
        (Vec3::new(0.0, 0.5, 15.0), PickupType::Health),
        (Vec3::new(0.0, 0.5, -15.0), PickupType::Health),
        (Vec3::new(25.0, 0.5, 25.0), PickupType::Ammo),
        (Vec3::new(-25.0, 0.5, -25.0), PickupType::Health),
    ];

    for (pos, pickup_type) in pickup_positions {
        commands.spawn((
            Pickup {
                pickup_type,
                respawn_timer: 0.0,
            },
            Transform::from_translation(pos),
            Collider::ball(0.5),
            Sensor,
        ));
    }
}

pub fn spawn_ai_enemies(commands: &mut Commands) {
    let enemy_positions = [
        Vec3::new(30.0, 1.0, 30.0),
        Vec3::new(-30.0, 1.0, 30.0),
        Vec3::new(30.0, 1.0, -30.0),
        Vec3::new(-30.0, 1.0, -30.0),
    ];

    for pos in enemy_positions {
        commands.spawn((
            AiEnemy {
                speed: 3.0,
                move_direction: Vec3::X,
                time_since_direction_change: 0.0,
                shoot_cooldown: 0.0,
            },
            Health {
                current: 80,
                max: 80,
                respawn_timer: 0.0,
            },
            Transform::from_translation(pos),
            RigidBody::KinematicVelocityBased,
            Velocity::default(),
            Collider::capsule_y(0.5, 0.3),
        ));
    }
}

pub fn spawn_player(commands: &mut Commands, agent_id: Uuid, position: Vec3) -> Entity {
    commands
        .spawn((
            Player {
                agent_id,
                speed: PLAYER_SPEED,
                target_position: None,
                ammo: INITIAL_AMMO,
                score: 0,
            },
            Health {
                current: INITIAL_HEALTH,
                max: INITIAL_HEALTH,
                respawn_timer: 0.0,
            },
            Transform::from_translation(position),
            RigidBody::KinematicVelocityBased,
            Velocity::default(),
            Collider::capsule_y(0.5, 0.3),
        ))
        .id()
}

pub fn spawn_bullet(commands: &mut Commands, shooter_id: Uuid, origin: Vec3, direction: Vec3) {
    commands.spawn((
        Bullet {
            shooter_id,
            direction: direction.normalize(),
            origin,
        },
        Transform::from_translation(origin),
        RigidBody::KinematicVelocityBased,
        Velocity::default(),
        Collider::ball(0.1),
    ));
}

pub fn get_spawn_position(player_index: usize) -> Vec3 {
    let spawn_points = [
        Vec3::new(-40.0, 1.0, -40.0),
        Vec3::new(40.0, 1.0, -40.0),
        Vec3::new(-40.0, 1.0, 40.0),
        Vec3::new(40.0, 1.0, 40.0),
    ];
    spawn_points[player_index % spawn_points.len()]
}
