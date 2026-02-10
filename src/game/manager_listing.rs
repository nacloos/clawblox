use uuid::Uuid;

use crate::game::{GameInfo, GameManagerHandle, InstanceInfo};
use crate::game::instance::GameStatus;

pub fn list_instances(state: &GameManagerHandle) -> Vec<InstanceInfo> {
    state
        .instances
        .iter()
        .map(|entry| {
            let instance_id = *entry.key();
            let instance = entry.value().read();
            InstanceInfo {
                instance_id,
                game_id: instance.game_id,
                status: if instance.halted_error.is_some() {
                    "failed".to_string()
                } else {
                    match instance.status {
                        GameStatus::Waiting => "waiting".to_string(),
                        GameStatus::Playing => "playing".to_string(),
                        GameStatus::Finished => "finished".to_string(),
                    }
                },
                player_count: instance.players.len(),
                max_players: instance.max_players as usize,
                tick: instance.tick,
            }
        })
        .collect()
}

pub fn list_games(state: &GameManagerHandle) -> Vec<GameInfo> {
    let mut game_infos: std::collections::HashMap<Uuid, GameInfo> = std::collections::HashMap::new();

    for entry in state.instances.iter() {
        let instance = entry.value().read();
        let game_id = instance.game_id;

        let info = game_infos.entry(game_id).or_insert_with(|| GameInfo {
            id: game_id,
            status: "waiting".to_string(),
            player_count: 0,
            tick: 0,
        });

        info.player_count += instance.players.len();
        info.tick = info.tick.max(instance.tick);
        if instance.halted_error.is_some() {
            info.status = "failed".to_string();
        } else if instance.status == GameStatus::Playing {
            info.status = "playing".to_string();
        }
    }

    game_infos.into_values().collect()
}

pub fn get_game_info(state: &GameManagerHandle, game_id: Uuid) -> Option<GameInfo> {
    let instance_ids = state.game_instances.get(&game_id)?;

    let mut total_players = 0;
    let mut max_tick = 0;
    let mut any_playing = false;
    let mut any_failed = false;

    for &instance_id in instance_ids.value() {
        if let Some(handle) = state.instances.get(&instance_id) {
            let instance = handle.read();
            total_players += instance.players.len();
            max_tick = max_tick.max(instance.tick);
            if instance.halted_error.is_some() {
                any_failed = true;
            } else if instance.status == GameStatus::Playing {
                any_playing = true;
            }
        }
    }

    Some(GameInfo {
        id: game_id,
        status: if any_playing {
            "playing"
        } else if any_failed {
            "failed"
        } else {
            "waiting"
        }
        .to_string(),
        player_count: total_players,
        tick: max_tick,
    })
}
