#![allow(dead_code)]

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::shooter::{AiEnemy, Bullet, Health, Pickup, Player, BULLET_DAMAGE, BULLET_SPEED};

pub fn movement_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Velocity, &Player)>,
) {
    for (mut transform, mut velocity, player) in query.iter_mut() {
        if let Some(target) = player.target_position {
            let direction = target - transform.translation;
            let distance = direction.length();

            if distance > 0.5 {
                let move_dir = direction.normalize();
                velocity.linvel = move_dir * player.speed;
            } else {
                velocity.linvel = Vec3::ZERO;
            }
        }
    }
}

pub fn bullet_system(
    mut commands: Commands,
    time: Res<Time>,
    mut bullets: Query<(Entity, &mut Transform, &Bullet)>,
) {
    for (entity, mut transform, bullet) in bullets.iter_mut() {
        transform.translation += bullet.direction * BULLET_SPEED * time.delta_secs();

        let distance = (transform.translation - bullet.origin).length();
        if distance > 100.0 {
            commands.entity(entity).despawn();
        }
    }
}

pub fn bullet_collision_system(
    mut commands: Commands,
    bullets: Query<(Entity, &Transform, &Bullet)>,
    mut targets: Query<(Entity, &Transform, &mut Health), Without<Bullet>>,
) {
    for (bullet_entity, bullet_transform, bullet) in bullets.iter() {
        for (target_entity, target_transform, mut health) in targets.iter_mut() {
            let distance = (bullet_transform.translation - target_transform.translation).length();
            if distance < 1.0 {
                health.current = (health.current - BULLET_DAMAGE).max(0);
                commands.entity(bullet_entity).despawn();
                break;
            }
        }
    }
}

pub fn ai_enemy_system(
    time: Res<Time>,
    mut enemies: Query<(&mut Transform, &mut Velocity, &mut AiEnemy)>,
    players: Query<&Transform, (With<Player>, Without<AiEnemy>)>,
) {
    for (mut transform, mut velocity, mut enemy) in enemies.iter_mut() {
        enemy.time_since_direction_change += time.delta_secs();

        if enemy.time_since_direction_change > 3.0 {
            enemy.time_since_direction_change = 0.0;
            let angle = rand::random::<f32>() * std::f32::consts::TAU;
            enemy.move_direction = Vec3::new(angle.cos(), 0.0, angle.sin());
        }

        velocity.linvel = enemy.move_direction * enemy.speed;

        if let Some(player_transform) = players.iter().next() {
            let to_player = player_transform.translation - transform.translation;
            if to_player.length() < 15.0 {
                let dir = to_player.normalize();
                velocity.linvel = dir * enemy.speed;
            }
        }
    }
}

pub fn pickup_respawn_system(mut pickups: Query<(&mut Transform, &mut Pickup)>) {
    for (mut transform, mut pickup) in pickups.iter_mut() {
        if pickup.respawn_timer > 0.0 {
            pickup.respawn_timer -= 1.0 / 60.0;
            if pickup.respawn_timer <= 0.0 {
                pickup.respawn_timer = 0.0;
            }
        }
    }
}

pub fn death_respawn_system(
    mut players: Query<(&mut Transform, &mut Health, &Player)>,
    spawn_points: Query<&Transform, (With<SpawnPoint>, Without<Player>)>,
) {
    for (mut transform, mut health, player) in players.iter_mut() {
        if health.current <= 0 && health.respawn_timer > 0.0 {
            health.respawn_timer -= 1.0 / 60.0;
            if health.respawn_timer <= 0.0 {
                health.current = health.max;
                health.respawn_timer = 0.0;
                if let Some(spawn) = spawn_points.iter().next() {
                    transform.translation = spawn.translation;
                }
            }
        }
    }
}

#[derive(Component)]
pub struct SpawnPoint;
