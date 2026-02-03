use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use super::async_bridge::AsyncBridge;
use super::lua::instance::{attributes_to_json, ClassName, Instance, TextXAlignment, TextYAlignment};
use super::lua::services::AgentInput;
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
    pub player_names: HashMap<Uuid, String>, // agent_id -> player name
    pub action_receiver: Receiver<QueuedAction>,
    pub action_sender: Sender<QueuedAction>,
    pub status: GameStatus,
    /// Counter for generating unique user IDs that fit in Lua's safe integer range (< 2^53)
    next_user_id: u64,
    /// Time when the game instance was created (for server_time_ms calculation)
    start_time: Instant,
    /// Async bridge for database operations (DataStoreService)
    async_bridge: Option<Arc<AsyncBridge>>,
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
    ///
    /// # Arguments
    /// * `game_id` - Unique identifier for this game instance
    /// * `async_bridge` - Optional async bridge for DataStoreService support
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        let (action_sender, action_receiver) = crossbeam_channel::unbounded();

        Self {
            game_id,
            lua_runtime: None,
            physics: PhysicsWorld::new(),
            tick: 0,
            players: HashMap::new(),
            player_hrp_ids: HashMap::new(),
            player_names: HashMap::new(),
            action_receiver,
            action_sender,
            status: GameStatus::Playing,
            next_user_id: 1, // Start at 1, stays well within Lua's safe integer range
            start_time: Instant::now(),
            async_bridge,
        }
    }

    /// Returns milliseconds since game instance was created
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Creates a new game instance with a Lua script
    ///
    /// # Arguments
    /// * `game_id` - Unique identifier for this game instance
    /// * `script` - Lua script source code to execute
    /// * `async_bridge` - Optional async bridge for DataStoreService support
    pub fn new_with_script(
        game_id: Uuid,
        script: &str,
        async_bridge: Option<Arc<AsyncBridge>>,
    ) -> Self {
        let mut instance = Self::new(game_id, async_bridge);
        instance.load_script(script);
        instance
    }

    /// Loads and executes a Lua script
    pub fn load_script(&mut self, source: &str) {
        match LuaRuntime::new(self.game_id, self.async_bridge.clone()) {
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
    pub fn add_player(&mut self, agent_id: Uuid, name: &str) -> bool {
        if self.players.contains_key(&agent_id) {
            return false;
        }

        // Use counter-based user_id to ensure it fits in Lua's safe integer range (< 2^53)
        let user_id = self.next_user_id;
        self.next_user_id += 1;
        self.players.insert(agent_id, user_id);
        self.player_names.insert(agent_id, name.to_string());

        if let Some(runtime) = &self.lua_runtime {
            let (player, hrp_id) = runtime.add_player(user_id, name);

            // Offset spawn position based on player count to avoid overlap
            let player_index = self.players.len() as f32;
            let spawn_x = (player_index % 4.0 - 1.5) * 3.0; // -4.5, -1.5, 1.5, 4.5
            let spawn_z = (player_index / 4.0).floor() * 3.0;

            // Register character controller for player movement
            self.physics.add_character(hrp_id, [spawn_x, 6.0, spawn_z], 1.0, 5.0);
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
            // Remove player name
            self.player_names.remove(&agent_id);

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

    /// Queues an agent input for the AgentInputService in Lua
    pub fn queue_agent_input(&self, user_id: u64, input_type: String, data: serde_json::Value) {
        if let Some(runtime) = &self.lua_runtime {
            let input = AgentInput::new(input_type, data);
            runtime.queue_agent_input(user_id, input);
        }
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

        // Sync Lua humanoid MoveTo targets to physics character controllers
        self.sync_humanoid_move_targets();

        // Update character controller movement
        self.update_character_movement(dt);

        // Step physics simulation
        self.physics.step(dt);

        // Sync physics results back to Lua (for Anchored=false parts and characters)
        self.sync_physics_to_lua();

        // Process weld constraints (update Part1 positions based on Part0)
        self.process_welds();

        // Process agent inputs (fire InputReceived events)
        if let Some(runtime) = &self.lua_runtime {
            if let Err(e) = runtime.process_agent_inputs() {
                eprintln!("[Lua Error] Failed to process agent inputs: {}", e);
            }
        }

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
    /// - Removes physics bodies for parts that were destroyed in Lua
    fn sync_lua_to_physics(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        // Collect all active Lua part IDs
        let mut active_lua_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

        for part in descendants {
            let data = part.data.lock().unwrap();

            if let Some(part_data) = &data.part_data {
                let lua_id = data.id.0;
                active_lua_ids.insert(lua_id);

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

        // Remove physics parts that no longer exist in Lua (destroyed parts)
        let orphaned_ids: Vec<u64> = self.physics.get_all_part_ids()
            .into_iter()
            .filter(|id| !active_lua_ids.contains(id))
            .collect();

        for lua_id in orphaned_ids {
            self.physics.remove_part(lua_id);
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

    /// Process weld constraints - update Part1 position based on Part0's CFrame
    fn process_welds(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        // Collect welds and their data first to avoid borrow conflicts
        let mut weld_updates: Vec<(
            crate::game::lua::instance::InstanceRef,  // Part1 ref
            crate::game::lua::types::Vector3,         // new position
        )> = Vec::new();

        for instance in &descendants {
            let data = instance.data.lock().unwrap();

            if let Some(weld_data) = &data.weld_data {
                if !weld_data.enabled {
                    continue;
                }

                // Get Part0 and Part1 references
                let part0_ref = weld_data.part0.as_ref().and_then(|w| w.upgrade());
                let part1_ref = weld_data.part1.as_ref().and_then(|w| w.upgrade());

                if let (Some(part0_ref), Some(part1_ref)) = (part0_ref, part1_ref) {
                    // Get Part0's CFrame
                    let part0_data = part0_ref.lock().unwrap();
                    if let Some(p0_part) = &part0_data.part_data {
                        let part0_cframe = p0_part.cframe;
                        let c0 = weld_data.c0;
                        let c1 = weld_data.c1;
                        drop(part0_data);

                        // Calculate Part1's new position:
                        // Part1.CFrame = Part0.CFrame * C0 * C1:Inverse()
                        // For simplicity (no rotation yet), just use position offset:
                        // Part1.Position = Part0.Position + C0.Position - C1.Position
                        let new_pos = crate::game::lua::types::Vector3::new(
                            part0_cframe.position.x + c0.position.x - c1.position.x,
                            part0_cframe.position.y + c0.position.y - c1.position.y,
                            part0_cframe.position.z + c0.position.z - c1.position.z,
                        );

                        weld_updates.push((part1_ref, new_pos));
                    }
                }
            }
        }

        // Apply updates
        for (part1_ref, new_pos) in weld_updates {
            let mut part1_data = part1_ref.lock().unwrap();
            if let Some(p1_part) = &mut part1_data.part_data {
                p1_part.position = new_pos;
                p1_part.cframe.position = new_pos;
            }
        }
    }

    /// Syncs Lua humanoid MoveTo targets to physics character controllers
    fn sync_humanoid_move_targets(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        // For each player, check if their humanoid has a move target
        for (&agent_id, &user_id) in &self.players {
            let Some(&hrp_id) = self.player_hrp_ids.get(&agent_id) else {
                continue;
            };

            // Get player's character and humanoid
            let Some(player) = runtime.players().get_player_by_user_id(user_id) else {
                continue;
            };

            let player_data = player.data.lock().unwrap();
            let Some(character_weak) = player_data.player_data.as_ref().and_then(|pd| pd.character.as_ref()) else {
                continue;
            };
            let Some(character_ref) = character_weak.upgrade() else {
                continue;
            };
            drop(player_data);

            // Find humanoid in character
            let character_data = character_ref.lock().unwrap();
            for child_ref in &character_data.children {
                let mut child_data = child_ref.lock().unwrap();
                if let Some(humanoid) = &mut child_data.humanoid_data {
                    if let Some(target) = humanoid.move_to_target.take() {
                        self.physics.set_character_target(hrp_id, Some([target.x, target.y, target.z]));
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
        const CHARACTER_HALF_HEIGHT: f32 = 2.5; // Capsule half-height (5.0 total height / 2)
        const SNAP_THRESHOLD: f32 = 0.5; // Max distance to snap to ground
        const MAX_RAYCAST_DIST: f32 = 10.0; // How far down to look for ground
        const GROUND_OFFSET: f32 = 0.02; // Small gap to prevent move_shape detecting floor penetration

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
                    // Grounded - snap to ground (with small offset to prevent move_shape penetration)
                    (ground_y + CHARACTER_HALF_HEIGHT + GROUND_OFFSET, 0.0)
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
    pub fn get_player_observation(&self, agent_id: Uuid) -> Option<PlayerObservation> {
        let user_id = *self.players.get(&agent_id)?;

        let runtime = self.lua_runtime.as_ref()?;
        let player = runtime.players().get_player_by_user_id(user_id)?;

        // Get position from character's HumanoidRootPart
        let position = self.get_player_position(agent_id).unwrap_or([0.0, 3.0, 0.0]);

        // Get health from humanoid
        let health = self.get_player_health(agent_id).unwrap_or(100);

        // Read all player attributes generically and convert to JSON
        let player_data = player.data.lock().unwrap();
        let attributes = attributes_to_json(&player_data.attributes);
        drop(player_data);

        // Get other players (with LOS filtering)
        let other_players = self.get_other_players(agent_id, position);

        // Get world entities
        let world = self.get_world_info();

        Some(PlayerObservation {
            tick: self.tick,
            game_status: self.get_game_status_from_lua(),
            player: PlayerInfo {
                id: agent_id,
                position,
                health,
                attributes,
            },
            other_players,
            world,
            events: Vec::new(),
        })
    }

    /// Get the game status from Lua if available
    fn get_game_status_from_lua(&self) -> String {
        // Default to instance status
        match self.status {
            GameStatus::Waiting => "waiting".to_string(),
            GameStatus::Playing => "active".to_string(),
            GameStatus::Finished => "finished".to_string(),
        }
    }

    /// Get player position from their HumanoidRootPart
    fn get_player_position(&self, agent_id: Uuid) -> Option<[f32; 3]> {
        let hrp_id = *self.player_hrp_ids.get(&agent_id)?;
        self.physics.get_character_position(hrp_id)
    }

    /// Get player health from their Humanoid
    fn get_player_health(&self, agent_id: Uuid) -> Option<i32> {
        let user_id = *self.players.get(&agent_id)?;
        let runtime = self.lua_runtime.as_ref()?;
        let player = runtime.players().get_player_by_user_id(user_id)?;

        let player_data = player.data.lock().unwrap();
        let character = player_data
            .player_data
            .as_ref()?
            .character
            .as_ref()?
            .upgrade()?;
        drop(player_data);

        let char_data = character.lock().unwrap();
        for child in &char_data.children {
            let child_data = child.lock().unwrap();
            if child_data.name == "Humanoid" {
                if let Some(humanoid) = &child_data.humanoid_data {
                    return Some(humanoid.health as i32);
                }
            }
        }
        None
    }

    /// Get world info (all visible parts from Workspace)
    fn get_world_info(&self) -> WorldInfo {
        let mut entities = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                if let Some(part_data) = &data.part_data {
                    entities.push(WorldEntity {
                        id: data.id.0,
                        name: data.name.clone(),
                        position: [part_data.position.x, part_data.position.y, part_data.position.z],
                        size: [part_data.size.x, part_data.size.y, part_data.size.z],
                        color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                        material: Some(part_data.material.name().to_string()),
                        anchored: part_data.anchored,
                    });
                }
            }
        }

        WorldInfo { entities }
    }

    /// Get info about other players with distance and line-of-sight filtering
    fn get_other_players(&self, exclude_agent_id: Uuid, observer_pos: [f32; 3]) -> Vec<OtherPlayerInfo> {
        const MAX_VISIBILITY_DISTANCE: f32 = 100.0;

        let mut others = Vec::new();

        // Get observer's body handle for LOS exclusion
        let observer_body = self.player_hrp_ids.get(&exclude_agent_id)
            .and_then(|&hrp_id| self.physics.get_character_state(hrp_id))
            .map(|state| state.body_handle);

        for (&agent_id, &user_id) in &self.players {
            if agent_id == exclude_agent_id {
                continue;
            }

            let position = self.get_player_position(agent_id).unwrap_or([0.0, 0.0, 0.0]);

            // Distance culling
            let dx = position[0] - observer_pos[0];
            let dy = position[1] - observer_pos[1];
            let dz = position[2] - observer_pos[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();

            if distance > MAX_VISIBILITY_DISTANCE {
                continue;
            }

            // Line-of-sight check (only for nearby players)
            if !self.physics.has_line_of_sight(observer_pos, position, observer_body) {
                continue;
            }

            let health = self.get_player_health(agent_id).unwrap_or(100);

            // Get all attributes generically and convert to JSON
            let attributes = if let Some(runtime) = &self.lua_runtime {
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    let data = player.data.lock().unwrap();
                    attributes_to_json(&data.attributes)
                } else {
                    std::collections::HashMap::new()
                }
            } else {
                std::collections::HashMap::new()
            };

            others.push(OtherPlayerInfo {
                id: agent_id,
                position,
                health,
                attributes,
            });
        }

        others
    }

    /// Serializes a GUI instance tree to GuiElement for frontend rendering
    fn serialize_gui_tree(instance: &Instance) -> Option<GuiElement> {
        let data = instance.data.lock().unwrap();

        // Only serialize GUI classes
        let element_type = match data.class_name {
            ClassName::ScreenGui => "ScreenGui",
            ClassName::Frame => "Frame",
            ClassName::TextLabel => "TextLabel",
            ClassName::TextButton => "TextButton",
            ClassName::ImageLabel => "ImageLabel",
            ClassName::ImageButton => "ImageButton",
            _ => return None,
        };

        let gui_data = data.gui_data.as_ref();

        // Build element with optional GUI properties
        let mut element = GuiElement {
            id: data.id.0,
            element_type: element_type.to_string(),
            name: data.name.clone(),
            position: gui_data.map(|g| UDim2Json {
                x_scale: g.position.x.scale,
                x_offset: g.position.x.offset,
                y_scale: g.position.y.scale,
                y_offset: g.position.y.offset,
            }),
            size: gui_data.map(|g| UDim2Json {
                x_scale: g.size.x.scale,
                x_offset: g.size.x.offset,
                y_scale: g.size.y.scale,
                y_offset: g.size.y.offset,
            }),
            anchor_point: gui_data.map(|g| [g.anchor_point.0, g.anchor_point.1]),
            rotation: gui_data.map(|g| g.rotation),
            z_index: gui_data.map(|g| g.z_index),
            visible: gui_data.map(|g| g.visible),
            background_color: gui_data.map(|g| [g.background_color.r, g.background_color.g, g.background_color.b]),
            background_transparency: gui_data.map(|g| g.background_transparency),
            border_color: gui_data.map(|g| [g.border_color.r, g.border_color.g, g.border_color.b]),
            border_size_pixel: gui_data.map(|g| g.border_size_pixel),
            text: gui_data.and_then(|g| g.text.clone()),
            text_color: gui_data.and_then(|g| g.text_color.map(|c| [c.r, c.g, c.b])),
            text_size: gui_data.and_then(|g| g.text_size),
            text_transparency: gui_data.and_then(|g| g.text_transparency),
            text_x_alignment: gui_data.map(|g| match g.text_x_alignment {
                TextXAlignment::Left => "Left".to_string(),
                TextXAlignment::Center => "Center".to_string(),
                TextXAlignment::Right => "Right".to_string(),
            }),
            text_y_alignment: gui_data.map(|g| match g.text_y_alignment {
                TextYAlignment::Top => "Top".to_string(),
                TextYAlignment::Center => "Center".to_string(),
                TextYAlignment::Bottom => "Bottom".to_string(),
            }),
            image: gui_data.and_then(|g| g.image.clone()),
            image_color: gui_data.and_then(|g| g.image_color.map(|c| [c.r, c.g, c.b])),
            image_transparency: gui_data.and_then(|g| g.image_transparency),
            display_order: gui_data.map(|g| g.display_order),
            enabled: gui_data.map(|g| g.enabled),
            children: Vec::new(),
        };

        // Recursively serialize children (release lock first to avoid deadlock)
        let children_refs: Vec<_> = data.children.iter().cloned().collect();
        drop(data);

        for child_ref in children_refs {
            let child = Instance::from_ref(child_ref);
            if let Some(child_element) = Self::serialize_gui_tree(&child) {
                element.children.push(child_element);
            }
        }

        Some(element)
    }

    /// Gets the spectator observation (full world state from Lua Workspace)
    pub fn get_spectator_observation(&self) -> SpectatorObservation {
        let mut entities = Vec::new();
        let mut players = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            // Collect all parts from Workspace
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                if let Some(part_data) = &data.part_data {
                    // Only include rotation if it's not identity
                    let rot = part_data.cframe.rotation;
                    let is_identity = (rot[0][0] - 1.0).abs() < 0.001
                        && rot[0][1].abs() < 0.001
                        && rot[0][2].abs() < 0.001
                        && rot[1][0].abs() < 0.001
                        && (rot[1][1] - 1.0).abs() < 0.001
                        && rot[1][2].abs() < 0.001
                        && rot[2][0].abs() < 0.001
                        && rot[2][1].abs() < 0.001
                        && (rot[2][2] - 1.0).abs() < 0.001;

                    // Check for BillboardGui children
                    let billboard_gui = Self::collect_billboard_gui(&data.children);

                    entities.push(SpectatorEntity {
                        id: data.id.0 as u32,
                        entity_type: "part".to_string(),
                        position: [
                            part_data.position.x,
                            part_data.position.y,
                            part_data.position.z,
                        ],
                        rotation: if is_identity { None } else { Some(rot) },
                        size: Some([part_data.size.x, part_data.size.y, part_data.size.z]),
                        color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                        material: Some(part_data.material.name().to_string()),
                        shape: Some(part_data.shape.name().to_string()),
                        health: None,
                        pickup_type: None,
                        billboard_gui,
                    });
                }
            }

            // Collect player info
            for (&agent_id, &user_id) in &self.players {
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    let player_data = player.data.lock().unwrap();

                    // Get position and health in one pass (avoid redundant locking)
                    let (position, health) = player_data.player_data.as_ref()
                        .and_then(|pd| pd.character.as_ref())
                        .and_then(|weak| weak.upgrade())
                        .map(|char_ref| {
                            let char = char_ref.lock().unwrap();

                            // Get position from HumanoidRootPart
                            let pos = char.model_data.as_ref()
                                .and_then(|m| m.primary_part.as_ref())
                                .and_then(|weak| weak.upgrade())
                                .and_then(|hrp_data| {
                                    let hrp = hrp_data.lock().unwrap();
                                    hrp.part_data.as_ref().map(|p| [p.position.x, p.position.y, p.position.z])
                                })
                                .unwrap_or([0.0, 3.0, 0.0]);

                            // Get health from Humanoid (while we have character locked)
                            let hp = char.children.iter()
                                .find_map(|child| {
                                    let child_data = child.lock().unwrap();
                                    if child_data.name == "Humanoid" {
                                        child_data.humanoid_data.as_ref().map(|h| h.health as i32)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(100);

                            (pos, hp)
                        })
                        .unwrap_or(([0.0, 3.0, 0.0], 100));

                    // Get player name from our cache, or fall back to Player_<uuid>
                    let name = self.player_names.get(&agent_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Player_{}", agent_id.as_simple()));

                    // Get all attributes and convert to JSON Value
                    let attrs = attributes_to_json(&player_data.attributes);
                    let attributes = if attrs.is_empty() {
                        None
                    } else {
                        Some(serde_json::to_value(&attrs).unwrap_or(serde_json::Value::Null))
                    };

                    // Serialize PlayerGui tree
                    let gui = player_data.player_data.as_ref()
                        .and_then(|pd| pd.player_gui.as_ref())
                        .and_then(|weak| weak.upgrade())
                        .map(|player_gui_ref| {
                            let player_gui = Instance::from_ref(player_gui_ref);
                            // Get all ScreenGui children
                            player_gui.get_children()
                                .iter()
                                .filter_map(|child| Self::serialize_gui_tree(child))
                                .collect::<Vec<_>>()
                        })
                        .filter(|v: &Vec<GuiElement>| !v.is_empty());

                    players.push(SpectatorPlayerInfo {
                        id: agent_id,
                        name,
                        position,
                        health,
                        attributes,
                        gui,
                    });
                    drop(player_data);
                }
            }
        }

        SpectatorObservation {
            tick: self.tick,
            server_time_ms: self.elapsed_ms(),
            game_status: match self.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            players,
            entities,
        }
    }

    /// Collects BillboardGui data from a part's children
    fn collect_billboard_gui(
        children: &[crate::game::lua::instance::InstanceRef],
    ) -> Option<BillboardGuiJson> {
        use crate::game::lua::instance::ClassName;

        for child_ref in children {
            let child_data = child_ref.lock().unwrap();

            if child_data.class_name == ClassName::BillboardGui {
                if let Some(billboard_data) = &child_data.billboard_gui_data {
                    // Collect TextLabel children
                    let mut labels = Vec::new();

                    for label_ref in &child_data.children {
                        let label_data = label_ref.lock().unwrap();

                        if label_data.class_name == ClassName::TextLabel {
                            if let Some(gui_data) = &label_data.gui_data {
                                labels.push(BillboardLabelJson {
                                    text: gui_data.text.clone().unwrap_or_default(),
                                    color: [
                                        gui_data.text_color.map(|c| c.r).unwrap_or(1.0),
                                        gui_data.text_color.map(|c| c.g).unwrap_or(1.0),
                                        gui_data.text_color.map(|c| c.b).unwrap_or(1.0),
                                    ],
                                    size: gui_data.text_size.unwrap_or(14.0) as f32,
                                });
                            }
                        }
                    }

                    return Some(BillboardGuiJson {
                        studs_offset: [
                            billboard_data.studs_offset.x,
                            billboard_data.studs_offset.y,
                            billboard_data.studs_offset.z,
                        ],
                        always_on_top: billboard_data.always_on_top,
                        labels,
                    });
                }
            }
        }

        None
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
    pub other_players: Vec<OtherPlayerInfo>,
    pub world: WorldInfo,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldInfo {
    pub entities: Vec<WorldEntity>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldEntity {
    pub id: u64,
    pub name: String,
    pub position: [f32; 3],
    pub size: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    pub anchored: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerInfo {
    pub id: Uuid,
    pub position: [f32; 3],
    pub health: i32,
    /// Game-specific attributes set by Lua scripts
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OtherPlayerInfo {
    pub id: Uuid,
    pub position: [f32; 3],
    pub health: i32,
    /// Game-specific attributes set by Lua scripts
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
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
    /// Milliseconds since game instance was created (for client clock synchronization)
    pub server_time_ms: u64,
    pub game_status: String,
    pub players: Vec<SpectatorPlayerInfo>,
    pub entities: Vec<SpectatorEntity>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorPlayerInfo {
    pub id: Uuid,
    pub name: String,
    pub position: [f32; 3],
    pub health: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gui: Option<Vec<GuiElement>>,
}

/// Serialized GUI element for frontend rendering
#[derive(Debug, Clone, serde::Serialize)]
pub struct GuiElement {
    pub id: u64,
    #[serde(rename = "type")]
    pub element_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<UDim2Json>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<UDim2Json>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_point: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_transparency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_size_pixel: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_transparency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_x_alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_y_alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_transparency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_order: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    pub children: Vec<GuiElement>,
}

/// UDim2 serialization format
#[derive(Debug, Clone, serde::Serialize)]
pub struct UDim2Json {
    pub x_scale: f32,
    pub x_offset: i32,
    pub y_scale: f32,
    pub y_offset: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorEntity {
    pub id: u32,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub position: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[[f32; 3]; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billboard_gui: Option<BillboardGuiJson>,
}

/// BillboardGui serialization for 3D floating labels
#[derive(Debug, Clone, serde::Serialize)]
pub struct BillboardGuiJson {
    pub studs_offset: [f32; 3],
    pub always_on_top: bool,
    pub labels: Vec<BillboardLabelJson>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BillboardLabelJson {
    pub text: String,
    pub color: [f32; 3],
    pub size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::lua::instance::AttributeValue;

    #[test]
    fn test_player_goto_action() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

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
        assert!(instance.add_player(agent_id, "TestPlayer"));

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

    #[test]
    fn test_observation_includes_world_entities() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a script that creates some geometry
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            local platform = Instance.new("Part")
            platform.Name = "CenterPlatform"
            platform.Size = Vector3.new(10, 1, 10)
            platform.Position = Vector3.new(0, 5, 0)
            platform.Anchored = true
            platform.Parent = Workspace
        "#);

        let agent_id = Uuid::new_v4();
        instance.add_player(agent_id, "TestPlayer");

        // Run a tick to sync physics
        instance.tick();

        let obs = instance.get_player_observation(agent_id).unwrap();

        // Should have world entities
        assert!(!obs.world.entities.is_empty(), "World should contain entities");

        // Should have Floor and CenterPlatform
        let names: Vec<&str> = obs.world.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Floor"), "Should contain Floor");
        assert!(names.contains(&"CenterPlatform"), "Should contain CenterPlatform");

        // Check Floor properties
        let floor = obs.world.entities.iter().find(|e| e.name == "Floor").unwrap();
        assert_eq!(floor.size, [100.0, 1.0, 100.0]);
        assert!(floor.anchored);
    }

    #[test]
    fn test_los_blocked_by_wall() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Create a wall between two player spawn points
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            local wall = Instance.new("Part")
            wall.Name = "Wall"
            wall.Size = Vector3.new(20, 10, 2)
            wall.Position = Vector3.new(0, 5, 0)
            wall.Anchored = true
            wall.Parent = Workspace
        "#);

        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();
        instance.add_player(agent_a, "PlayerA");
        instance.add_player(agent_b, "PlayerB");

        // Run a tick to sync physics
        instance.tick();

        // Position players on opposite sides of the wall
        // Player A at z=-10, Player B at z=+10, wall at z=0
        let hrp_a = *instance.player_hrp_ids.get(&agent_a).unwrap();
        let hrp_b = *instance.player_hrp_ids.get(&agent_b).unwrap();

        // Use set_character_target and tick to move them (or directly set position)
        instance.physics.move_character_and_set_y(hrp_a, 0.0, 0.0, 2.0, 0.0);
        instance.physics.move_character_and_set_y(hrp_b, 0.0, 0.0, 2.0, 0.0);

        // Manually set positions behind the wall
        if let Some(state) = instance.physics.get_character_state(hrp_a) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![0.0, 2.0, -10.0], true);
            }
        }
        if let Some(state) = instance.physics.get_character_state(hrp_b) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![0.0, 2.0, 10.0], true);
            }
        }

        // Update query pipeline
        instance.physics.query_pipeline.update(&instance.physics.collider_set);

        let obs = instance.get_player_observation(agent_a).unwrap();

        // Agent A should NOT see Agent B (blocked by wall)
        assert!(obs.other_players.is_empty(), "Should not see player through wall");
    }

    #[test]
    fn test_los_clear() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Just a floor, no obstructions
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace
        "#);

        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();
        instance.add_player(agent_a, "PlayerA");
        instance.add_player(agent_b, "PlayerB");

        // Run several ticks to sync physics
        for _ in 0..5 {
            instance.tick();
        }

        // Position players at same height on the floor with clear LOS
        let hrp_a = *instance.player_hrp_ids.get(&agent_a).unwrap();
        let hrp_b = *instance.player_hrp_ids.get(&agent_b).unwrap();

        if let Some(state) = instance.physics.get_character_state(hrp_a) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![-10.0, 2.0, 0.0], true);
            }
        }
        if let Some(state) = instance.physics.get_character_state(hrp_b) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![10.0, 2.0, 0.0], true);
            }
        }

        // Update query pipeline
        instance.physics.query_pipeline.update(&instance.physics.collider_set);

        let obs = instance.get_player_observation(agent_a).unwrap();

        // Agent A should see Agent B (clear LOS)
        assert_eq!(obs.other_players.len(), 1, "Should see other player with clear LOS");
        assert_eq!(obs.other_players[0].id, agent_b);
    }

    #[test]
    fn test_shooting_and_killing() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a minimal shooting test script (no debug prints, fast startup)
        instance.load_script(r#"
            local Players = game:GetService("Players")
            local AgentInputService = game:GetService("AgentInputService")

            -- Create floor
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            -- Initialize player weapons
            Players.PlayerAdded:Connect(function(player)
                player:SetAttribute("CurrentWeapon", 1)
                player:SetAttribute("Kills", 0)
            end)

            -- Handle Fire input
            if AgentInputService then
                AgentInputService.InputReceived:Connect(function(player, inputType, data)
                    if inputType == "Fire" and data and data.direction then
                        local character = player.Character
                        if not character then return end

                        local hrp = character:FindFirstChild("HumanoidRootPart")
                        if not hrp then return end

                        local origin = hrp.Position + Vector3.new(0, 1, 0)
                        local dir = Vector3.new(data.direction[1], data.direction[2], data.direction[3])

                        -- Raycast to find target
                        local raycastParams = RaycastParams.new()
                        raycastParams.FilterType = Enum.RaycastFilterType.Exclude
                        raycastParams.FilterDescendantsInstances = {character}

                        local result = Workspace:Raycast(origin, dir * 100, raycastParams)
                        if result then
                            -- Check if we hit a player
                            local hitPart = result.Instance
                            local current = hitPart
                            while current do
                                if current:FindFirstChild("Humanoid") then
                                    -- Found a character, deal damage
                                    local humanoid = current:FindFirstChild("Humanoid")
                                    humanoid:TakeDamage(25)
                                    print("[HIT] Dealt 25 damage, health now:", humanoid.Health)

                                    if humanoid.Health <= 0 then
                                        player:SetAttribute("Kills", (player:GetAttribute("Kills") or 0) + 1)
                                        player:SetAttribute("CurrentWeapon", (player:GetAttribute("CurrentWeapon") or 1) + 1)
                                        print("[KILL] Player got kill, weapon now:", player:GetAttribute("CurrentWeapon"))
                                    end
                                    break
                                end
                                current = current.Parent
                            end
                        else
                            print("[MISS] Raycast hit nothing")
                        end
                    end
                end)
            end
            print("Shooting test script loaded")
        "#);

        // Add two players
        let attacker_id = Uuid::new_v4();
        let victim_id = Uuid::new_v4();
        instance.add_player(attacker_id, "Attacker");
        instance.add_player(victim_id, "Victim");

        let attacker_user_id = *instance.players.get(&attacker_id).unwrap();

        // Run a few ticks to initialize
        for _ in 0..5 {
            instance.tick();
        }

        // Position players facing each other
        let hrp_attacker = *instance.player_hrp_ids.get(&attacker_id).unwrap();
        let hrp_victim = *instance.player_hrp_ids.get(&victim_id).unwrap();

        if let Some(state) = instance.physics.get_character_state(hrp_attacker) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![-5.0, 2.0, 0.0], true);
            }
        }
        if let Some(state) = instance.physics.get_character_state(hrp_victim) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![5.0, 2.0, 0.0], true);
            }
        }

        // Sync physics to Lua
        instance.tick();

        let initial_health = instance.get_player_health(victim_id).unwrap_or(100);

        // Fire towards victim (+X direction)
        let fire_input = serde_json::json!({ "direction": [1.0, 0.0, 0.0] });
        instance.queue_agent_input(attacker_user_id, "Fire".to_string(), fire_input);

        instance.tick();

        let health_after_shot = instance.get_player_health(victim_id).unwrap_or(100);

        assert!(
            health_after_shot < initial_health,
            "Victim should have taken damage: {} -> {}",
            initial_health,
            health_after_shot
        );

        // Keep shooting until dead
        let mut shots = 1;
        while instance.get_player_health(victim_id).unwrap_or(0) > 0 && shots < 10 {
            let fire_input = serde_json::json!({ "direction": [1.0, 0.0, 0.0] });
            instance.queue_agent_input(attacker_user_id, "Fire".to_string(), fire_input);
            instance.tick();
            shots += 1;
        }

        let final_health = instance.get_player_health(victim_id).unwrap_or(0);
        assert!(final_health <= 0, "Victim should be dead after {} shots", shots);

        // Check attacker stats
        if let Some(runtime) = &instance.lua_runtime {
            if let Some(player) = runtime.players().get_player_by_user_id(attacker_user_id) {
                let data = player.data.lock().unwrap();

                // Verify kills attribute was set to 1
                if let Some(AttributeValue::Number(kills)) = data.attributes.get("Kills") {
                    assert_eq!(*kills as i32, 1, "Expected 1 kill");
                } else {
                    panic!("Kills attribute not found or not a number");
                }

                // Verify weapon was upgraded to 2
                if let Some(AttributeValue::Number(weapon)) = data.attributes.get("CurrentWeapon") {
                    assert_eq!(*weapon as i32, 2, "Expected weapon to be upgraded to 2");
                } else {
                    panic!("CurrentWeapon attribute not found or not a number");
                }
            }
        }
    }
}
