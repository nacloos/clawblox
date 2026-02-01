use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use uuid::Uuid;

use super::lua::LuaRuntime;
use super::physics::PhysicsWorld;

/// A game instance that runs Lua scripts with Rapier physics.
/// This is the Roblox-like architecture where:
/// - Lua controls game logic via Workspace, Parts, etc.
/// - Rapier handles physics simulation for non-anchored parts
pub struct GameInstance {
    pub game_id: Uuid,
    pub lua_runtime: Option<LuaRuntime>,
    pub physics: PhysicsWorld,
    pub tick: u64,
    pub players: HashMap<Uuid, u64>, // agent_id -> lua player user_id
    pub action_receiver: Receiver<QueuedAction>,
    pub action_sender: Sender<QueuedAction>,
    pub status: GameStatus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameStatus {
    Waiting,
    Playing,
    Finished,
}

/// An action queued by a player
#[derive(Debug, Clone)]
pub struct QueuedAction {
    pub agent_id: Uuid,
    pub action: GameAction,
}

/// Available actions players can take
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameAction {
    Goto { position: [f32; 3] },
    Shoot { position: [f32; 3] },
    Interact { target_id: u32 },
    Wait,
}

impl GameInstance {
    /// Creates a new game instance without a script
    pub fn new(game_id: Uuid) -> Self {
        let (action_sender, action_receiver) = crossbeam_channel::unbounded();

        Self {
            game_id,
            lua_runtime: None,
            physics: PhysicsWorld::new(),
            tick: 0,
            players: HashMap::new(),
            action_receiver,
            action_sender,
            status: GameStatus::Playing,
        }
    }

    /// Creates a new game instance with a Lua script
    pub fn new_with_script(game_id: Uuid, script: &str) -> Self {
        let mut instance = Self::new(game_id);
        instance.load_script(script);
        instance
    }

    /// Loads and executes a Lua script
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

    /// Adds a player to the game
    pub fn add_player(&mut self, agent_id: Uuid) -> bool {
        if self.players.contains_key(&agent_id) {
            return false;
        }

        let user_id = agent_id.as_u128() as u64;
        self.players.insert(agent_id, user_id);

        if let Some(runtime) = &self.lua_runtime {
            let player_name = format!("Player_{}", agent_id.as_simple());
            let player = runtime.add_player(user_id, &player_name);
            if let Err(e) = runtime.fire_player_added(&player) {
                eprintln!("[Lua Error] Failed to fire PlayerAdded: {}", e);
            }
        }

        true
    }

    /// Removes a player from the game
    pub fn remove_player(&mut self, agent_id: Uuid) -> bool {
        if let Some(user_id) = self.players.remove(&agent_id) {
            if let Some(runtime) = &self.lua_runtime {
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    if let Err(e) = runtime.fire_player_removing(&player) {
                        eprintln!("[Lua Error] Failed to fire PlayerRemoving: {}", e);
                    }
                }
                runtime.remove_player(user_id);
            }
            true
        } else {
            false
        }
    }

    /// Queues an action for processing in the next tick
    pub fn queue_action(&self, agent_id: Uuid, action: GameAction) {
        let _ = self.action_sender.send(QueuedAction { agent_id, action });
    }

    /// Main game loop tick - called at 60 Hz
    pub fn tick(&mut self) {
        let dt = 1.0 / 60.0;

        // Process queued actions
        while let Ok(queued) = self.action_receiver.try_recv() {
            self.process_action(queued);
        }

        // Sync Lua workspace gravity to physics
        self.sync_gravity();

        // Sync new/changed Lua parts to physics
        self.sync_lua_to_physics();

        // Step physics simulation
        self.physics.step(dt);

        // Sync physics results back to Lua (for Anchored=false parts)
        self.sync_physics_to_lua();

        // Run Lua Heartbeat
        if let Some(runtime) = &self.lua_runtime {
            if let Err(e) = runtime.tick(dt) {
                eprintln!("[Lua Error] Tick error: {}", e);
            }
        }

        self.tick += 1;
    }

    /// Syncs Workspace.Gravity to physics world
    fn sync_gravity(&mut self) {
        if let Some(runtime) = &self.lua_runtime {
            let gravity = runtime.workspace().data.lock().unwrap().gravity;
            self.physics.set_gravity(gravity);
        }
    }

    /// Syncs Lua parts to the physics world
    /// - Creates physics bodies for new parts
    /// - Updates positions for anchored parts that moved in Lua
    fn sync_lua_to_physics(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        for part in descendants {
            let data = part.data.lock().unwrap();

            if let Some(part_data) = &data.part_data {
                let lua_id = data.id.0;

                if !self.physics.has_part(lua_id) {
                    // New part - add to physics
                    self.physics.add_part(
                        lua_id,
                        [part_data.position.x, part_data.position.y, part_data.position.z],
                        [0.0, 0.0, 0.0, 1.0], // TODO: extract rotation from CFrame
                        [part_data.size.x, part_data.size.y, part_data.size.z],
                        part_data.anchored,
                        part_data.can_collide,
                    );

                    // Apply initial velocity for dynamic parts
                    if !part_data.anchored {
                        if let Some(handle) = self.physics.get_handle(lua_id) {
                            self.physics.set_velocity(
                                handle,
                                [part_data.velocity.x, part_data.velocity.y, part_data.velocity.z],
                            );
                        }
                    }
                } else if part_data.anchored {
                    // Anchored part - update physics position from Lua
                    if let Some(handle) = self.physics.get_handle(lua_id) {
                        self.physics.set_kinematic_position(
                            handle,
                            [part_data.position.x, part_data.position.y, part_data.position.z],
                        );
                    }
                }
            }
        }
    }

    /// Syncs physics positions back to Lua for non-anchored parts
    fn sync_physics_to_lua(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        for part in descendants {
            let mut data = part.data.lock().unwrap();
            let lua_id = data.id.0;

            if let Some(part_data) = &mut data.part_data {
                if !part_data.anchored {
                    if let Some(handle) = self.physics.get_handle(lua_id) {
                        // Update position from physics
                        if let Some(pos) = self.physics.get_position(handle) {
                            part_data.position.x = pos[0];
                            part_data.position.y = pos[1];
                            part_data.position.z = pos[2];
                            part_data.cframe.position = part_data.position;
                        }

                        // Update velocity from physics
                        if let Some(vel) = self.physics.get_velocity(handle) {
                            part_data.velocity.x = vel[0];
                            part_data.velocity.y = vel[1];
                            part_data.velocity.z = vel[2];
                        }
                    }
                }
            }
        }
    }

    /// Processes a queued action from a player
    fn process_action(&mut self, queued: QueuedAction) {
        let Some(&_user_id) = self.players.get(&queued.agent_id) else {
            return;
        };

        match queued.action {
            GameAction::Goto { position: _ } => {
                // TODO: Implement player movement via Lua humanoid
            }
            GameAction::Shoot { position: _ } => {
                // TODO: Implement shooting via Lua
            }
            GameAction::Interact { target_id: _ } => {
                // TODO: Implement interaction via Lua
            }
            GameAction::Wait => {}
        }
    }

    /// Gets the observation for a specific player
    pub fn get_player_observation(&mut self, agent_id: Uuid) -> Option<PlayerObservation> {
        let _user_id = *self.players.get(&agent_id)?;

        // For now, return a basic observation
        // TODO: Implement proper per-player observations based on Lua state
        Some(PlayerObservation {
            tick: self.tick,
            game_status: match self.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            player: PlayerInfo {
                id: agent_id,
                position: [0.0, 0.0, 0.0],
                facing: [1.0, 0.0, 0.0],
                health: 100,
                ammo: 0,
                score: 0,
            },
            visible_entities: Vec::new(),
            events: Vec::new(),
        })
    }

    /// Gets the spectator observation (full world state from Lua Workspace)
    pub fn get_spectator_observation(&mut self) -> SpectatorObservation {
        let mut entities = Vec::new();
        let mut players = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            // Collect all parts from Workspace
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                if let Some(part_data) = &data.part_data {
                    entities.push(SpectatorEntity {
                        id: data.id.0 as u32,
                        entity_type: "part".to_string(),
                        position: [
                            part_data.position.x,
                            part_data.position.y,
                            part_data.position.z,
                        ],
                        size: Some([part_data.size.x, part_data.size.y, part_data.size.z]),
                        color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                        health: None,
                        pickup_type: None,
                    });
                }
            }

            // Collect player info
            for (&agent_id, &user_id) in &self.players {
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    let player_data = player.data.lock().unwrap();
                    players.push(SpectatorPlayerInfo {
                        id: agent_id,
                        position: [0.0, 0.0, 0.0], // TODO: Get from character
                        health: 100,
                        ammo: 0,
                        score: 0,
                    });
                    drop(player_data);
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

// ============================================================================
// Observation types
// ============================================================================

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
    pub size: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_type: Option<String>,
}
