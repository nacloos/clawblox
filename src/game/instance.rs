use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use uuid::Uuid;

use super::actions::{GameAction, QueuedAction};
use super::lua::LuaRuntime;
use super::shooter::{
    get_spawn_position, spawn_ai_enemies, spawn_arena, spawn_bullet, spawn_pickups, spawn_player,
    AiEnemy, Bullet, Health, Pickup, PickupType, Player, PICKUP_RESPAWN_TIME, RESPAWN_TIME,
};
use super::systems::SpawnPoint;

pub struct GameInstance {
    pub game_id: Uuid,
    pub world: World,
    pub schedule: Schedule,
    pub tick: u64,
    pub players: HashMap<Uuid, Entity>,
    pub action_receiver: Receiver<QueuedAction>,
    pub action_sender: Sender<QueuedAction>,
    pub status: GameStatus,
    pub lua_runtime: Option<LuaRuntime>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameStatus {
    Waiting,
    Playing,
    Finished,
}

impl GameInstance {
    pub fn new(game_id: Uuid) -> Self {
        let (action_sender, action_receiver) = crossbeam_channel::unbounded();

        let mut world = World::new();

        world.insert_resource(Time::<Fixed>::from_hz(60.0));

        let mut commands_queue = CommandQueue::default();
        {
            let mut commands = Commands::new(&mut commands_queue, &world);
            spawn_arena(&mut commands);
            spawn_pickups(&mut commands);
            spawn_ai_enemies(&mut commands);

            let spawn_positions = [
                Vec3::new(-40.0, 1.0, -40.0),
                Vec3::new(40.0, 1.0, -40.0),
                Vec3::new(-40.0, 1.0, 40.0),
                Vec3::new(40.0, 1.0, 40.0),
            ];
            for pos in spawn_positions {
                commands.spawn((SpawnPoint, Transform::from_translation(pos)));
            }
        }
        commands_queue.apply(&mut world);

        let schedule = Schedule::default();

        Self {
            game_id,
            world,
            schedule,
            tick: 0,
            players: HashMap::new(),
            action_receiver,
            action_sender,
            status: GameStatus::Waiting,
            lua_runtime: None,
        }
    }

    pub fn new_with_script(game_id: Uuid, script: &str) -> Self {
        let mut instance = Self::new(game_id);
        instance.load_script(script);
        instance
    }

    pub fn load_script(&mut self, source: &str) {
        match LuaRuntime::new() {
            Ok(mut runtime) => {
                if let Err(e) = runtime.load_script(source) {
                    eprintln!("[Lua Error] Failed to load script: {}", e);
                } else {
                    self.lua_runtime = Some(runtime);
                }
            }
            Err(e) => {
                eprintln!("[Lua Error] Failed to create runtime: {}", e);
            }
        }
    }

    pub fn add_player(&mut self, agent_id: Uuid) -> bool {
        if self.players.contains_key(&agent_id) {
            return false;
        }

        let position = get_spawn_position(self.players.len());
        let mut commands_queue = CommandQueue::default();
        let entity = {
            let mut commands = Commands::new(&mut commands_queue, &self.world);
            spawn_player(&mut commands, agent_id, position)
        };
        commands_queue.apply(&mut self.world);

        self.players.insert(agent_id, entity);

        if let Some(runtime) = &self.lua_runtime {
            let player_name = format!("Player_{}", agent_id.as_simple());
            let player = runtime.add_player(agent_id.as_u128() as u64, &player_name);
            if let Err(e) = runtime.fire_player_added(&player) {
                eprintln!("[Lua Error] Failed to fire PlayerAdded: {}", e);
            }
        }

        if self.players.len() >= 1 && self.status == GameStatus::Waiting {
            self.status = GameStatus::Playing;
        }

        true
    }

    pub fn remove_player(&mut self, agent_id: Uuid) -> bool {
        if let Some(entity) = self.players.remove(&agent_id) {
            if let Some(runtime) = &self.lua_runtime {
                let user_id = agent_id.as_u128() as u64;
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    if let Err(e) = runtime.fire_player_removing(&player) {
                        eprintln!("[Lua Error] Failed to fire PlayerRemoving: {}", e);
                    }
                }
                runtime.remove_player(user_id);
            }

            let mut commands_queue = CommandQueue::default();
            {
                let mut commands = Commands::new(&mut commands_queue, &self.world);
                commands.entity(entity).despawn();
            }
            commands_queue.apply(&mut self.world);
            true
        } else {
            false
        }
    }

    pub fn queue_action(&self, agent_id: Uuid, action: GameAction) {
        let _ = self.action_sender.send(QueuedAction { agent_id, action });
    }

    pub fn tick(&mut self) {
        while let Ok(queued) = self.action_receiver.try_recv() {
            self.process_action(queued);
        }

        self.update_physics();

        if let Some(runtime) = &self.lua_runtime {
            let dt = 1.0 / 60.0;
            if let Err(e) = runtime.tick(dt) {
                eprintln!("[Lua Error] Tick error: {}", e);
            }
        }

        self.tick += 1;
    }

    fn process_action(&mut self, queued: QueuedAction) {
        let Some(&entity) = self.players.get(&queued.agent_id) else {
            return;
        };

        match queued.action {
            GameAction::Goto { position } => {
                if let Some(mut player) = self.world.get_mut::<Player>(entity) {
                    player.target_position = Some(Vec3::from_array(position));
                }
            }
            GameAction::Shoot { position } => {
                let (origin, ammo) = {
                    let player = self.world.get::<Player>(entity);
                    let transform = self.world.get::<Transform>(entity);
                    match (player, transform) {
                        (Some(p), Some(t)) if p.ammo > 0 => (t.translation, p.ammo),
                        _ => return,
                    }
                };

                if ammo > 0 {
                    if let Some(mut player) = self.world.get_mut::<Player>(entity) {
                        player.ammo -= 1;
                    }

                    let target = Vec3::from_array(position);
                    let direction = (target - origin).normalize();

                    let mut commands_queue = CommandQueue::default();
                    {
                        let mut commands = Commands::new(&mut commands_queue, &self.world);
                        spawn_bullet(
                            &mut commands,
                            queued.agent_id,
                            origin + direction * 1.0,
                            direction,
                        );
                    }
                    commands_queue.apply(&mut self.world);
                }
            }
            GameAction::Interact { target_id: _ } => {
                // TODO: Implement pickup interaction
            }
            GameAction::Wait => {}
        }
    }

    fn update_physics(&mut self) {
        self.schedule.run(&mut self.world);

        self.update_movement();
        self.update_bullets();
        self.update_ai();
        self.check_pickups();
        self.check_deaths();
    }

    fn update_movement(&mut self) {
        let dt = 1.0 / 60.0;

        let mut updates = Vec::new();
        {
            let mut query = self.world.query::<(Entity, &Player, &Transform)>();
            for (entity, player, transform) in query.iter(&self.world) {
                if let Some(target) = player.target_position {
                    let direction = target - transform.translation;
                    let distance = direction.length();

                    if distance > 0.5 {
                        let move_dir = direction.normalize();
                        let new_pos = transform.translation + move_dir * player.speed * dt;
                        updates.push((entity, new_pos));
                    }
                }
            }
        }

        for (entity, new_pos) in updates {
            if let Some(mut transform) = self.world.get_mut::<Transform>(entity) {
                transform.translation = new_pos;
            }
        }
    }

    fn update_bullets(&mut self) {
        let dt = 1.0 / 60.0;
        let bullet_speed = 30.0;

        let mut bullet_updates = Vec::new();
        let mut bullets_to_remove = Vec::new();

        {
            let mut query = self.world.query::<(Entity, &Bullet, &Transform)>();
            for (entity, bullet, transform) in query.iter(&self.world) {
                let new_pos = transform.translation + bullet.direction * bullet_speed * dt;
                let distance = (new_pos - bullet.origin).length();

                if distance > 100.0 {
                    bullets_to_remove.push(entity);
                } else {
                    bullet_updates.push((entity, new_pos));
                }
            }
        }

        for (entity, new_pos) in bullet_updates {
            if let Some(mut transform) = self.world.get_mut::<Transform>(entity) {
                transform.translation = new_pos;
            }
        }

        self.check_bullet_collisions();

        let mut commands_queue = CommandQueue::default();
        {
            let mut commands = Commands::new(&mut commands_queue, &self.world);
            for entity in bullets_to_remove {
                commands.entity(entity).despawn();
            }
        }
        commands_queue.apply(&mut self.world);
    }

    fn check_bullet_collisions(&mut self) {
        let mut hits = Vec::new();
        let mut bullets_to_remove = Vec::new();

        let bullets: Vec<_> = {
            let mut query = self.world.query::<(Entity, &Transform, &Bullet)>();
            query
                .iter(&self.world)
                .map(|(e, t, b)| (e, t.translation, b.shooter_id))
                .collect()
        };

        for (bullet_entity, bullet_pos, _shooter_id) in &bullets {
            let mut query = self.world.query::<(Entity, &Transform, &Health)>();
            for (target_entity, transform, _health) in query.iter(&self.world) {
                if self.world.get::<Bullet>(target_entity).is_some() {
                    continue;
                }

                let distance = (*bullet_pos - transform.translation).length();
                if distance < 1.0 {
                    hits.push((target_entity, 20));
                    bullets_to_remove.push(*bullet_entity);
                    break;
                }
            }
        }

        for (entity, damage) in hits {
            if let Some(mut health) = self.world.get_mut::<Health>(entity) {
                health.current = (health.current - damage).max(0);
            }
        }

        let mut commands_queue = CommandQueue::default();
        {
            let mut commands = Commands::new(&mut commands_queue, &self.world);
            for entity in bullets_to_remove {
                commands.entity(entity).despawn();
            }
        }
        commands_queue.apply(&mut self.world);
    }

    fn update_ai(&mut self) {
        let dt = 1.0 / 60.0;

        let player_positions: Vec<_> = {
            let mut query = self.world.query::<(&Transform, &Player)>();
            query.iter(&self.world).map(|(t, _)| t.translation).collect()
        };

        let mut updates = Vec::new();
        {
            let mut query = self.world.query::<(Entity, &AiEnemy, &Transform)>();
            for (entity, enemy, transform) in query.iter(&self.world) {
                let mut new_dir = enemy.move_direction;
                let mut time = enemy.time_since_direction_change + dt;

                if time > 3.0 {
                    time = 0.0;
                    let angle = rand::random::<f32>() * std::f32::consts::TAU;
                    new_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
                }

                if let Some(player_pos) = player_positions.first() {
                    let to_player = *player_pos - transform.translation;
                    if to_player.length() < 15.0 {
                        new_dir = to_player.normalize();
                    }
                }

                let new_pos = transform.translation + new_dir * enemy.speed * dt;
                updates.push((entity, new_pos, new_dir, time));
            }
        }

        for (entity, new_pos, new_dir, time) in updates {
            if let Some(mut transform) = self.world.get_mut::<Transform>(entity) {
                transform.translation = new_pos;
            }
            if let Some(mut enemy) = self.world.get_mut::<AiEnemy>(entity) {
                enemy.move_direction = new_dir;
                enemy.time_since_direction_change = time;
            }
        }
    }

    fn check_pickups(&mut self) {
        let player_data: Vec<_> = {
            let mut query = self.world.query::<(Entity, &Transform, &Player)>();
            query
                .iter(&self.world)
                .map(|(e, t, p)| (e, t.translation, p.agent_id))
                .collect()
        };

        let mut pickup_interactions = Vec::new();

        {
            let mut query = self.world.query::<(Entity, &Pickup, &Transform)>();
            for (pickup_entity, pickup, pickup_transform) in query.iter(&self.world) {
                if pickup.respawn_timer > 0.0 {
                    continue;
                }

                for (player_entity, player_pos, _) in &player_data {
                    let distance = (pickup_transform.translation - *player_pos).length();
                    if distance < 1.5 {
                        pickup_interactions.push((pickup_entity, *player_entity, pickup.pickup_type));
                        break;
                    }
                }
            }
        }

        for (pickup_entity, player_entity, pickup_type) in pickup_interactions {
            match pickup_type {
                PickupType::Health => {
                    if let Some(mut health) = self.world.get_mut::<Health>(player_entity) {
                        health.current = (health.current + 30).min(health.max);
                    }
                }
                PickupType::Ammo => {
                    if let Some(mut player) = self.world.get_mut::<Player>(player_entity) {
                        player.ammo += 10;
                    }
                }
            }

            if let Some(mut pickup) = self.world.get_mut::<Pickup>(pickup_entity) {
                pickup.respawn_timer = PICKUP_RESPAWN_TIME;
            }
        }

        let mut respawn_updates = Vec::new();
        {
            let mut query = self.world.query::<(Entity, &Pickup)>();
            for (entity, pickup) in query.iter(&self.world) {
                if pickup.respawn_timer > 0.0 {
                    respawn_updates.push((entity, pickup.respawn_timer - 1.0 / 60.0));
                }
            }
        }

        for (entity, new_timer) in respawn_updates {
            if let Some(mut pickup) = self.world.get_mut::<Pickup>(entity) {
                pickup.respawn_timer = new_timer.max(0.0);
            }
        }
    }

    fn check_deaths(&mut self) {
        let spawn_positions = [
            Vec3::new(-40.0, 1.0, -40.0),
            Vec3::new(40.0, 1.0, -40.0),
            Vec3::new(-40.0, 1.0, 40.0),
            Vec3::new(40.0, 1.0, 40.0),
        ];

        let mut respawn_updates = Vec::new();

        {
            let mut query = self.world.query::<(Entity, &Health, &Player)>();
            for (entity, health, _player) in query.iter(&self.world) {
                if health.current <= 0 {
                    if health.respawn_timer <= 0.0 {
                        respawn_updates.push((entity, RESPAWN_TIME, false));
                    } else {
                        let new_timer = health.respawn_timer - 1.0 / 60.0;
                        if new_timer <= 0.0 {
                            respawn_updates.push((entity, 0.0, true));
                        } else {
                            respawn_updates.push((entity, new_timer, false));
                        }
                    }
                }
            }
        }

        for (entity, timer, respawn) in respawn_updates {
            if respawn {
                let spawn_idx = rand::random::<usize>() % spawn_positions.len();
                if let Some(mut transform) = self.world.get_mut::<Transform>(entity) {
                    transform.translation = spawn_positions[spawn_idx];
                }
                if let Some(mut health) = self.world.get_mut::<Health>(entity) {
                    health.current = health.max;
                    health.respawn_timer = 0.0;
                }
            } else if let Some(mut health) = self.world.get_mut::<Health>(entity) {
                health.respawn_timer = timer;
            }
        }
    }

    pub fn get_player_observation(&mut self, agent_id: Uuid) -> Option<PlayerObservation> {
        let entity = *self.players.get(&agent_id)?;

        let (player_pos, player_health, player_ammo, player_score) = {
            let player = self.world.get::<Player>(entity)?;
            let transform = self.world.get::<Transform>(entity)?;
            let health = self.world.get::<Health>(entity)?;
            (
                transform.translation,
                health.current,
                player.ammo,
                player.score,
            )
        };

        let mut visible_entities = Vec::new();

        {
            let mut query = self.world.query::<(Entity, &Transform, &Health, &AiEnemy)>();
            for (e, t, h, _enemy) in query.iter(&self.world) {
                let distance = (t.translation - player_pos).length();
                if distance < 50.0 {
                    visible_entities.push(VisibleEntity {
                        id: e.index(),
                        entity_type: "enemy".to_string(),
                        position: t.translation.to_array(),
                        distance,
                        health: Some(h.current),
                        pickup_type: None,
                    });
                }
            }
        }

        {
            let mut query = self.world.query::<(Entity, &Transform, &Pickup)>();
            for (e, t, pickup) in query.iter(&self.world) {
                if pickup.respawn_timer > 0.0 {
                    continue;
                }
                let distance = (t.translation - player_pos).length();
                if distance < 30.0 {
                    visible_entities.push(VisibleEntity {
                        id: e.index(),
                        entity_type: "pickup".to_string(),
                        position: t.translation.to_array(),
                        distance,
                        health: None,
                        pickup_type: Some(match pickup.pickup_type {
                            PickupType::Health => "health".to_string(),
                            PickupType::Ammo => "ammo".to_string(),
                        }),
                    });
                }
            }
        }

        {
            let mut query = self.world.query::<(Entity, &Player, &Transform, &Health)>();
            for (other_entity, other_player, t, h) in query.iter(&self.world) {
                if other_player.agent_id == agent_id {
                    continue;
                }
                let distance = (t.translation - player_pos).length();
                if distance < 50.0 {
                    visible_entities.push(VisibleEntity {
                        id: other_entity.index(),
                        entity_type: "player".to_string(),
                        position: t.translation.to_array(),
                        distance,
                        health: Some(h.current),
                        pickup_type: None,
                    });
                }
            }
        }

        Some(PlayerObservation {
            tick: self.tick,
            game_status: match self.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            player: PlayerInfo {
                id: agent_id,
                position: player_pos.to_array(),
                facing: [1.0, 0.0, 0.0],
                health: player_health,
                ammo: player_ammo,
                score: player_score,
            },
            visible_entities,
            events: Vec::new(),
        })
    }

    pub fn get_spectator_observation(&mut self) -> SpectatorObservation {
        let mut players = Vec::new();
        let mut entities = Vec::new();

        {
            let mut query = self.world.query::<(&Player, &Transform, &Health)>();
            for (player, transform, health) in query.iter(&self.world) {
                players.push(SpectatorPlayerInfo {
                    id: player.agent_id,
                    position: transform.translation.to_array(),
                    health: health.current,
                    ammo: player.ammo,
                    score: player.score,
                });
            }
        }

        {
            let mut query = self.world.query::<(Entity, &Transform, &Health, &AiEnemy)>();
            for (e, t, h, _) in query.iter(&self.world) {
                entities.push(SpectatorEntity {
                    id: e.index(),
                    entity_type: "enemy".to_string(),
                    position: t.translation.to_array(),
                    health: Some(h.current),
                    pickup_type: None,
                });
            }
        }

        {
            let mut query = self.world.query::<(Entity, &Transform, &Pickup)>();
            for (e, t, pickup) in query.iter(&self.world) {
                if pickup.respawn_timer <= 0.0 {
                    entities.push(SpectatorEntity {
                        id: e.index(),
                        entity_type: "pickup".to_string(),
                        position: t.translation.to_array(),
                        health: None,
                        pickup_type: Some(match pickup.pickup_type {
                            PickupType::Health => "health".to_string(),
                            PickupType::Ammo => "ammo".to_string(),
                        }),
                    });
                }
            }
        }

        SpectatorObservation {
            tick: self.tick,
            game_status: match self.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            players,
            entities,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerObservation {
    pub tick: u64,
    pub game_status: String,
    pub player: PlayerInfo,
    pub visible_entities: Vec<VisibleEntity>,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerInfo {
    pub id: Uuid,
    pub position: [f32; 3],
    pub facing: [f32; 3],
    pub health: i32,
    pub ammo: i32,
    pub score: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VisibleEntity {
    pub id: u32,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub position: [f32; 3],
    pub distance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GameEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorObservation {
    pub tick: u64,
    pub game_status: String,
    pub players: Vec<SpectatorPlayerInfo>,
    pub entities: Vec<SpectatorEntity>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorPlayerInfo {
    pub id: Uuid,
    pub position: [f32; 3],
    pub health: i32,
    pub ammo: i32,
    pub score: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorEntity {
    pub id: u32,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub position: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_type: Option<String>,
}
