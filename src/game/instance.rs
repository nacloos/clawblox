use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use uuid::Uuid;

use super::lua::LuaRuntime;
use super::physics::PhysicsWorld;

/// Walk speed for player characters (studs per second)
const WALK_SPEED: f32 = 16.0;

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
    pub player_hrp_ids: HashMap<Uuid, u64>, // agent_id -> HumanoidRootPart lua_id
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
            player_hrp_ids: HashMap::new(),
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
            let (player, hrp_id) = runtime.add_player(user_id, &player_name);

            // Offset spawn position based on player count to avoid overlap
            let player_index = self.players.len() as f32;
            let spawn_x = (player_index % 4.0 - 1.5) * 3.0; // -4.5, -1.5, 1.5, 4.5
            let spawn_z = (player_index / 4.0).floor() * 3.0;

            // Register character controller for player movement
            self.physics.add_character(hrp_id, [spawn_x, 5.0, spawn_z], 0.5, 2.0);
            self.player_hrp_ids.insert(agent_id, hrp_id);

            if let Err(e) = runtime.fire_player_added(&player) {
                eprintln!("[Lua Error] Failed to fire PlayerAdded: {}", e);
            }
        }

        true
    }

    /// Removes a player from the game
    pub fn remove_player(&mut self, agent_id: Uuid) -> bool {
        if let Some(user_id) = self.players.remove(&agent_id) {
            // Remove character controller
            if let Some(hrp_id) = self.player_hrp_ids.remove(&agent_id) {
                self.physics.remove_character(hrp_id);
            }

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

        // Sync new/changed Lua parts to physics (skip character-controlled parts)
        self.sync_lua_to_physics();

        // Update query pipeline so character controller can detect collisions with new geometry
        self.physics.query_pipeline.update(&self.physics.collider_set);

        // Update character controller movement
        self.update_character_movement(dt);

        // Step physics simulation
        self.physics.step(dt);

        // Sync physics results back to Lua (for Anchored=false parts and characters)
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
    /// - Creates physics bodies for new parts (skips character-controlled parts)
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

                // Skip parts managed by character controllers
                if self.physics.has_character(lua_id) {
                    continue;
                }

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

    /// Syncs physics positions back to Lua for non-anchored parts and character controllers
    fn sync_physics_to_lua(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        for part in descendants {
            let mut data = part.data.lock().unwrap();
            let lua_id = data.id.0;

            if let Some(part_data) = &mut data.part_data {
                // Check if this is a character-controlled part
                if self.physics.has_character(lua_id) {
                    if let Some(pos) = self.physics.get_character_position(lua_id) {
                        part_data.position.x = pos[0];
                        part_data.position.y = pos[1];
                        part_data.position.z = pos[2];
                        part_data.cframe.position = part_data.position;
                    }
                } else if !part_data.anchored {
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
            GameAction::Goto { position } => {
                if let Some(&hrp_id) = self.player_hrp_ids.get(&queued.agent_id) {
                    self.physics.set_character_target(hrp_id, Some(position));
                }
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

    /// Updates character controller movement towards targets
    /// Uses raycast for ground detection (Roblox-style) and character controller for horizontal collision
    fn update_character_movement(&mut self, dt: f32) {
        const CHARACTER_HALF_HEIGHT: f32 = 1.0; // Capsule half-height
        const SNAP_THRESHOLD: f32 = 0.5; // Max distance to snap to ground
        const MAX_RAYCAST_DIST: f32 = 10.0; // How far down to look for ground

        // Collect HRP IDs to process (avoid borrow issues)
        let hrp_ids: Vec<u64> = self.player_hrp_ids.values().copied().collect();

        for hrp_id in hrp_ids {
            // Get current position, target, and vertical velocity
            let (current_pos, target, vertical_velocity, body_handle) = {
                let Some(state) = self.physics.get_character_state(hrp_id) else {
                    continue;
                };
                let Some(pos) = self.physics.get_character_position(hrp_id) else {
                    continue;
                };
                (pos, state.target_position, state.vertical_velocity, state.body_handle)
            };

            // 1. Ground detection via raycast (current-frame, no delay)
            let ray_origin = [current_pos[0], current_pos[1], current_pos[2]];
            let ground_hit = self.physics.raycast_down(ray_origin, MAX_RAYCAST_DIST, Some(body_handle));

            // 2. Vertical movement based on ground detection
            let gravity = self.physics.gravity.y;
            let (new_y, new_vertical_velocity) = if let Some((distance, ground_y)) = ground_hit {
                // Distance from character center to ground
                let feet_clearance = distance - CHARACTER_HALF_HEIGHT;

                if feet_clearance <= SNAP_THRESHOLD && vertical_velocity <= 0.0 {
                    // Grounded - snap to ground
                    (ground_y + CHARACTER_HALF_HEIGHT, 0.0)
                } else {
                    // Above ground or moving up - apply gravity
                    let new_vel = vertical_velocity + gravity * dt;
                    let new_y = current_pos[1] + new_vel * dt;
                    (new_y, new_vel)
                }
            } else {
                // No ground detected - falling
                let new_vel = vertical_velocity + gravity * dt;
                let new_y = current_pos[1] + new_vel * dt;
                (new_y, new_vel)
            };

            // Update vertical velocity in state
            if let Some(state) = self.physics.get_character_state_mut(hrp_id) {
                state.vertical_velocity = new_vertical_velocity;
            }

            // 3. Calculate horizontal movement towards target
            let mut dx = 0.0f32;
            let mut dz = 0.0f32;

            if let Some(target) = target {
                let tx = target[0] - current_pos[0];
                let tz = target[2] - current_pos[2];
                let dist_xz = (tx * tx + tz * tz).sqrt();

                if dist_xz > 0.5 {
                    let speed = WALK_SPEED * dt;
                    dx = (tx / dist_xz) * speed;
                    dz = (tz / dist_xz) * speed;
                } else {
                    // Reached target, clear it
                    self.physics.set_character_target(hrp_id, None);
                }
            }

            // 4. Apply movement: horizontal with collision + vertical direct
            self.physics.move_character_and_set_y(hrp_id, dx, dz, new_y, dt);
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

                    // Get position from character's HumanoidRootPart
                    let position = player_data.player_data.as_ref()
                        .and_then(|pd| pd.character.as_ref())
                        .and_then(|weak| weak.upgrade())
                        .and_then(|char_data| {
                            let char = char_data.lock().unwrap();
                            char.model_data.as_ref()
                                .and_then(|m| m.primary_part.as_ref())
                                .and_then(|weak| weak.upgrade())
                                .and_then(|hrp_data| {
                                    let hrp = hrp_data.lock().unwrap();
                                    hrp.part_data.as_ref().map(|p| [p.position.x, p.position.y, p.position.z])
                                })
                        })
                        .unwrap_or([0.0, 3.0, 0.0]); // Default spawn height

                    players.push(SpectatorPlayerInfo {
                        id: agent_id,
                        position,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_goto_action() {
        let mut instance = GameInstance::new(Uuid::new_v4());

        // Load a simple script that creates a floor
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id));

        // Verify player has HRP registered
        assert!(instance.player_hrp_ids.contains_key(&agent_id));
        let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();

        // Get initial position
        let initial_pos = instance.physics.get_character_position(hrp_id).unwrap();
        println!("Initial player position: {:?}", initial_pos);

        // Queue a Goto action
        instance.queue_action(agent_id, GameAction::Goto { position: [10.0, 5.0, 10.0] });

        // Run 120 ticks (2 seconds at 60 Hz)
        for _ in 0..120 {
            instance.tick();
        }

        // Get final position
        let final_pos = instance.physics.get_character_position(hrp_id).unwrap();
        println!("Final player position: {:?}", final_pos);

        // Player should have moved towards target (10, 5, 10)
        let moved_x = final_pos[0] - initial_pos[0];
        let moved_z = final_pos[2] - initial_pos[2];
        let horizontal_distance = (moved_x * moved_x + moved_z * moved_z).sqrt();

        println!("Horizontal distance moved: {}", horizontal_distance);

        // At 16 studs/sec for 2 seconds, should move up to 32 studs
        // Target is ~14.14 studs away horizontally, should reach it
        assert!(horizontal_distance > 5.0, "Player should have moved significantly towards target");
    }
}
