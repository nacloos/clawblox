use super::*;

fn attr_string(
    attrs: &std::collections::HashMap<String, AttributeValue>,
    key: &str,
) -> Option<String> {
    match attrs.get(key) {
        Some(AttributeValue::String(v)) => Some(v.clone()),
        _ => None,
    }
}

fn attr_bool(
    attrs: &std::collections::HashMap<String, AttributeValue>,
    key: &str,
) -> Option<bool> {
    match attrs.get(key) {
        Some(AttributeValue::Bool(v)) => Some(*v),
        _ => None,
    }
}

fn attr_color(
    attrs: &std::collections::HashMap<String, AttributeValue>,
    key: &str,
) -> Option<[f32; 3]> {
    match attrs.get(key) {
        Some(AttributeValue::Color3(c)) => Some([c.r, c.g, c.b]),
        Some(AttributeValue::Vector3(v)) => Some([v.x, v.y, v.z]),
        _ => None,
    }
}

fn build_render(
    data: &crate::game::lua::instance::InstanceData,
    part_data: &crate::game::lua::instance::PartData,
) -> SpectatorRender {
    let role = attr_string(&data.attributes, "RenderRole").unwrap_or_else(|| "unspecified".to_string());
    let preset_id = attr_string(&data.attributes, "RenderPresetId");
    let kind = if preset_id.is_some() { "preset" } else { "primitive" }.to_string();
    let primitive = attr_string(&data.attributes, "RenderPrimitive")
        .unwrap_or_else(|| part_data.shape.name().to_string().to_lowercase());
    let material = attr_string(&data.attributes, "RenderMaterial")
        .unwrap_or_else(|| part_data.material.name().to_string());
    let color = attr_color(&data.attributes, "RenderColor")
        .unwrap_or([part_data.color.r, part_data.color.g, part_data.color.b]);
    let is_static = attr_bool(&data.attributes, "RenderStatic")
        .unwrap_or(data.tags.contains("Static") || part_data.anchored);
    let visible = attr_bool(&data.attributes, "RenderVisible")
        .unwrap_or(part_data.transparency < 1.0);
    let casts_shadow = attr_bool(&data.attributes, "RenderCastsShadow")
        .unwrap_or(true);
    let receives_shadow = attr_bool(&data.attributes, "RenderReceivesShadow")
        .unwrap_or(true);

    SpectatorRender {
        kind,
        role,
        preset_id,
        primitive,
        material,
        color,
        is_static,
        casts_shadow,
        receives_shadow,
        visible,
        double_sided: false,
        transparency: if part_data.transparency != 0.0 { Some(part_data.transparency) } else { None },
    }
}

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
                    render: build_render(&data, part_data),
                    health: None,
                    pickup_type: None,
                    model_url: GameInstance::extract_model_url(&data.attributes),
                    model_yaw_offset_deg: GameInstance::extract_model_yaw_offset_deg(&data.attributes),
                    billboard_gui,
                });
            }
        }

        // Collect player info
        for (&agent_id, &user_id) in &instance.players {
            if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                let player_data = player.data.lock().unwrap();

                // Get position and health in one pass (avoid redundant locking)
                let (position, health, root_part_id, animator_id) = player_data
                    .player_data
                    .as_ref()
                    .and_then(|pd| pd.character.as_ref())
                    .and_then(|weak| weak.upgrade())
                    .map(|char_ref| {
                        let char = char_ref.lock().unwrap();

                        // Get position from HumanoidRootPart
                        let primary_part = char
                            .model_data
                            .as_ref()
                            .and_then(|m| m.primary_part.as_ref())
                            .and_then(|weak| weak.upgrade());
                        let (pos, root_part_id) = primary_part
                            .as_ref()
                            .and_then(|hrp_data| {
                                let hrp = hrp_data.lock().unwrap();
                                hrp.part_data.as_ref().map(|p| {
                                    (
                                        [p.position.x, p.position.y, p.position.z],
                                        Some(hrp.id.0 as u32),
                                    )
                                })
                            })
                            .unwrap_or(([0.0, 3.0, 0.0], None));

                        // Get health from Humanoid (while we have character locked)
                        let mut hp = 100;
                        let mut animator_id: Option<u64> = None;
                        for child in &char.children {
                            let child_data = child.lock().unwrap();
                            if child_data.name == "Humanoid" {
                                if let Some(humanoid) = &child_data.humanoid_data {
                                    hp = humanoid.health as i32;
                                }
                                animator_id = child_data.children.iter().find_map(|anim_child| {
                                    let anim_data = anim_child.lock().unwrap();
                                    if anim_data.class_name == ClassName::Animator {
                                        Some(anim_data.id.0)
                                    } else {
                                        None
                                    }
                                });
                                break;
                            }
                        }

                        (pos, hp, root_part_id, animator_id)
                    })
                    .unwrap_or(([0.0, 3.0, 0.0], 100, None, None));

                let active_animations = animator_id.and_then(|id| {
                    runtime
                        .lua()
                        .app_data_ref::<crate::game::lua::animation::AnimationScheduler>()
                        .map(|scheduler| {
                            let tracks = scheduler.active_tracks_for_animator(id);
                            tracks
                                .into_iter()
                                .map(|track| SpectatorPlayerAnimation {
                                    track_id: track.track_id,
                                    animation_id: track.animation_id,
                                    length: round_f32(track.length),
                                    priority: track.priority,
                                    time_position: round_f32(track.time_position),
                                    speed: round_f32(track.speed),
                                    looped: track.looped,
                                    is_playing: track.is_playing,
                                    is_stopping: track.is_stopping,
                                    weight_current: round_f32(track.weight_current),
                                    weight_target: round_f32(track.weight_target),
                                    effective_weight: round_f32(track.effective_weight),
                                })
                                .collect::<Vec<_>>()
                        })
                        .filter(|tracks| !tracks.is_empty())
                });

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
                    root_part_id,
                    health,
                    attributes,
                    gui,
                    active_animations,
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
        recent_events: instance.recent_replicated_events(),
    }
}
