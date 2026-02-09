use super::*;

pub(super) fn build_player_observation(
    instance: &GameInstance,
    agent_id: Uuid,
) -> Option<PlayerObservation> {
    let user_id = *instance.players.get(&agent_id)?;

    let runtime = instance.lua_runtime.as_ref()?;
    let player = runtime.players().get_player_by_user_id(user_id)?;

    // Get position from character's HumanoidRootPart (rounded to reduce payload size)
    let position = round_position(instance.get_player_position(agent_id).unwrap_or([0.0, 3.0, 0.0]));

    {
        let mut counts = instance.observation_log_counts.lock().unwrap();
        let count = counts.entry(agent_id).or_insert(0);
        if *count < 5 {
            let name = instance.player_names.get(&agent_id).cloned().unwrap_or_default();
            eprintln!(
                "[Obs] tick={} agent={} name={} pos=({:.2},{:.2},{:.2})",
                instance.tick,
                agent_id,
                name,
                position[0],
                position[1],
                position[2]
            );
            *count += 1;
        }
    }

    // Get health from humanoid
    let health = instance.get_player_health(agent_id).unwrap_or(100);

    // Read all player attributes generically and convert to JSON
    let player_data = player.data.lock().unwrap();
    let attributes = attributes_to_json(&player_data.attributes);
    drop(player_data);

    // Get other players (with LOS filtering)
    let other_players = instance.get_other_players(agent_id, position);

    // Get dynamic world entities only (static entities fetched via /map endpoint)
    let world = instance.get_dynamic_world_info();

    Some(PlayerObservation {
        tick: instance.tick,
        game_status: instance.get_game_status_from_lua(),
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

pub(super) fn build_map_info(instance: &GameInstance) -> MapInfo {
    let mut entities = Vec::new();

    if let Some(runtime) = &instance.lua_runtime {
        for part in runtime.workspace().get_descendants() {
            let data = part.data.lock().unwrap();

            // Only include entities with "Static" tag
            if !data.tags.contains("Static") {
                continue;
            }

            if let Some(part_data) = &data.part_data {
                let attrs = attributes_to_json(&data.attributes);
                entities.push(WorldEntity {
                    id: data.id.0,
                    name: data.name.clone(),
                    entity_type: Some("part".to_string()),
                    position: [part_data.position.x, part_data.position.y, part_data.position.z],
                    size: [part_data.size.x, part_data.size.y, part_data.size.z],
                    rotation: Some(part_data.cframe.rotation),
                    color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                    material: Some(part_data.material.name().to_string()),
                    shape: Some(part_data.shape.name().to_string()),
                    transparency: if part_data.transparency != 0.0 {
                        Some(part_data.transparency)
                    } else {
                        None
                    },
                    anchored: part_data.anchored,
                    attributes: if attrs.is_empty() { None } else { Some(attrs) },
                });
            }
        }
    }

    MapInfo { entities }
}

pub(super) fn build_spectator_observation(instance: &GameInstance) -> SpectatorObservation {
    let mut entities = Vec::new();
    let mut players = Vec::new();

    if let Some(runtime) = &instance.lua_runtime {
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
                let billboard_gui = GameInstance::collect_billboard_gui(&data.children);

                entities.push(SpectatorEntity {
                    id: data.id.0 as u32,
                    entity_type: "part".to_string(),
                    position: round_position([
                        part_data.position.x,
                        part_data.position.y,
                        part_data.position.z,
                    ]),
                    rotation: if is_identity { None } else { Some(rot) },
                    size: Some(round_position([
                        part_data.size.x,
                        part_data.size.y,
                        part_data.size.z,
                    ])),
                    color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                    material: Some(part_data.material.name().to_string()),
                    shape: Some(part_data.shape.name().to_string()),
                    health: None,
                    pickup_type: None,
                    model_url: GameInstance::extract_model_url(&data.attributes),
                    billboard_gui,
                });
            }
        }

        // Collect player info
        for (&agent_id, &user_id) in &instance.players {
            if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                let player_data = player.data.lock().unwrap();

                // Get position and health in one pass (avoid redundant locking)
                let (position, health) = player_data
                    .player_data
                    .as_ref()
                    .and_then(|pd| pd.character.as_ref())
                    .and_then(|weak| weak.upgrade())
                    .map(|char_ref| {
                        let char = char_ref.lock().unwrap();

                        // Get position from HumanoidRootPart
                        let pos = char
                            .model_data
                            .as_ref()
                            .and_then(|m| m.primary_part.as_ref())
                            .and_then(|weak| weak.upgrade())
                            .and_then(|hrp_data| {
                                let hrp = hrp_data.lock().unwrap();
                                hrp.part_data
                                    .as_ref()
                                    .map(|p| [p.position.x, p.position.y, p.position.z])
                            })
                            .unwrap_or([0.0, 3.0, 0.0]);

                        // Get health from Humanoid (while we have character locked)
                        let hp = char
                            .children
                            .iter()
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
                let name = instance
                    .player_names
                    .get(&agent_id)
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
                let gui = player_data
                    .player_data
                    .as_ref()
                    .and_then(|pd| pd.player_gui.as_ref())
                    .and_then(|weak| weak.upgrade())
                    .map(|player_gui_ref| {
                        let player_gui = Instance::from_ref(player_gui_ref);
                        // Get all ScreenGui children
                        player_gui
                            .get_children()
                            .iter()
                            .filter_map(|child| GameInstance::serialize_gui_tree(child))
                            .collect::<Vec<_>>()
                    })
                    .filter(|v: &Vec<GuiElement>| !v.is_empty());

                players.push(SpectatorPlayerInfo {
                    id: agent_id,
                    name,
                    position: round_position(position),
                    health,
                    attributes,
                    gui,
                });
                drop(player_data);
            }
        }
    }

    SpectatorObservation {
        instance_id: instance.instance_id,
        tick: instance.tick,
        server_time_ms: instance.elapsed_ms(),
        game_status: match instance.status {
            GameStatus::Waiting => "waiting".to_string(),
            GameStatus::Playing => "playing".to_string(),
            GameStatus::Finished => "finished".to_string(),
        },
        players,
        entities,
    }
}

