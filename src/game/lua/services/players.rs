use mlua::{UserData, UserDataFields, UserDataMethods};
use std::sync::{Arc, Mutex};

use crate::game::lua::events::create_signal;
use crate::game::lua::events::RBXScriptSignal;
use crate::game::lua::instance::{ClassName, Instance};

pub struct PlayersServiceData {
    pub players: Vec<Instance>,
    pub local_player: Option<Instance>,
    pub max_players: u32,
    pub player_added: RBXScriptSignal,
    pub player_removing: RBXScriptSignal,
}

impl PlayersServiceData {
    pub fn new() -> Self {
        Self::with_max_players(100)
    }

    pub fn with_max_players(max_players: u32) -> Self {
        Self {
            players: Vec::new(),
            local_player: None,
            max_players,
            player_added: create_signal("PlayerAdded"),
            player_removing: create_signal("PlayerRemoving"),
        }
    }
}

#[derive(Clone)]
pub struct PlayersService {
    pub instance: Instance,
    pub data: Arc<Mutex<PlayersServiceData>>,
}

impl PlayersService {
    pub fn new() -> Self {
        Self::with_max_players(100)
    }

    pub fn with_max_players(max_players: u32) -> Self {
        let instance = Instance::new(ClassName::Players, "Players");
        Self {
            instance,
            data: Arc::new(Mutex::new(PlayersServiceData::with_max_players(max_players))),
        }
    }

    pub fn add_player(&self, player: Instance) {
        self.data.lock().unwrap().players.push(player.clone());
        player.set_parent(Some(&self.instance));
    }

    pub fn remove_player(&self, user_id: u64) {
        let mut data = self.data.lock().unwrap();

        // Find and destroy the player's character before removing them
        for player in &data.players {
            let pdata = player.data.lock().unwrap();
            if let Some(pd) = &pdata.player_data {
                if pd.user_id == user_id {
                    // Destroy the character by removing it from workspace
                    if let Some(char_weak) = &pd.character {
                        if let Some(char_ref) = char_weak.upgrade() {
                            let character = Instance::from_ref(char_ref);
                            drop(pdata); // Release lock before modifying
                            character.set_parent(None);
                            break;
                        }
                    }
                }
            }
        }

        // Remove the player from the list
        data.players.retain(|p| {
            let pdata = p.data.lock().unwrap();
            pdata
                .player_data
                .as_ref()
                .map(|pd| pd.user_id != user_id)
                .unwrap_or(true)
        });
    }

    pub fn get_players(&self) -> Vec<Instance> {
        self.data.lock().unwrap().players.clone()
    }

    pub fn get_player_by_user_id(&self, user_id: u64) -> Option<Instance> {
        self.data
            .lock()
            .unwrap()
            .players
            .iter()
            .find(|p| {
                let pdata = p.data.lock().unwrap();
                pdata
                    .player_data
                    .as_ref()
                    .map(|pd| pd.user_id == user_id)
                    .unwrap_or(false)
            })
            .cloned()
    }

    pub fn get_player_from_character(&self, character: &Instance) -> Option<Instance> {
        let char_id = character.id();
        self.data
            .lock()
            .unwrap()
            .players
            .iter()
            .find(|p| {
                let pdata = p.data.lock().unwrap();
                pdata
                    .player_data
                    .as_ref()
                    .and_then(|pd| pd.character.as_ref())
                    .and_then(|w| w.upgrade())
                    .map(|c| Instance::from_ref(c).id() == char_id)
                    .unwrap_or(false)
            })
            .cloned()
    }
}

impl UserData for PlayersService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("LocalPlayer", |_, this| {
            Ok(this.data.lock().unwrap().local_player.clone())
        });

        fields.add_field_method_get("MaxPlayers", |_, this| {
            Ok(this.data.lock().unwrap().max_players)
        });

        fields.add_field_method_get("PlayerAdded", |_, this| {
            Ok(this.data.lock().unwrap().player_added.clone())
        });

        fields.add_field_method_get("PlayerRemoving", |_, this| {
            Ok(this.data.lock().unwrap().player_removing.clone())
        });

        fields.add_field_method_get("Name", |_, _| Ok("Players".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("Players".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetPlayers", |_, this, ()| Ok(this.get_players()));

        methods.add_method("GetPlayerByUserId", |_, this, user_id: u64| {
            Ok(this.get_player_by_user_id(user_id))
        });

        methods.add_method("GetPlayerFromCharacter", |_, this, character: Instance| {
            Ok(this.get_player_from_character(&character))
        });

        methods.add_method("GetChildren", |_, this, ()| Ok(this.get_players()));

        methods.add_method(
            "FindFirstChild",
            |_, this, (name, _recursive): (String, Option<bool>)| {
                Ok(this.get_players().into_iter().find(|p| p.name() == name))
            },
        );
    }
}
