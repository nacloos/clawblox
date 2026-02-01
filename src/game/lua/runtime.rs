use mlua::{Lua, MultiValue, ObjectLike, Result, UserData, UserDataMethods, Value};
use std::sync::{Arc, Mutex};

use super::instance::{Instance, InstanceData};
use super::services::{register_raycast_params, PlayersService, RunService, WorkspaceService};
use super::types::register_all_types;

pub struct GameDataModel {
    pub workspace: WorkspaceService,
    pub players: PlayersService,
    pub run_service: RunService,
}

impl GameDataModel {
    pub fn new() -> Self {
        Self {
            workspace: WorkspaceService::new(),
            players: PlayersService::new(),
            run_service: RunService::new(true),
        }
    }
}

#[derive(Clone)]
pub struct Game {
    pub data_model: Arc<Mutex<GameDataModel>>,
}

impl Game {
    pub fn new() -> Self {
        Self {
            data_model: Arc::new(Mutex::new(GameDataModel::new())),
        }
    }

    pub fn workspace(&self) -> WorkspaceService {
        self.data_model.lock().unwrap().workspace.clone()
    }

    pub fn players(&self) -> PlayersService {
        self.data_model.lock().unwrap().players.clone()
    }

    pub fn run_service(&self) -> RunService {
        self.data_model.lock().unwrap().run_service.clone()
    }
}

impl UserData for Game {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetService", |lua, this, name: String| {
            let dm = this.data_model.lock().unwrap();
            match name.as_str() {
                "Workspace" => Ok(Value::UserData(lua.create_userdata(dm.workspace.clone())?)),
                "Players" => Ok(Value::UserData(lua.create_userdata(dm.players.clone())?)),
                "RunService" => Ok(Value::UserData(lua.create_userdata(dm.run_service.clone())?)),
                _ => Ok(Value::Nil),
            }
        });

        methods.add_meta_method(mlua::MetaMethod::Index, |lua, this, key: String| {
            let dm = this.data_model.lock().unwrap();
            match key.as_str() {
                "Workspace" => Ok(Value::UserData(lua.create_userdata(dm.workspace.clone())?)),
                "Players" => Ok(Value::UserData(lua.create_userdata(dm.players.clone())?)),
                _ => Ok(Value::Nil),
            }
        });
    }
}

pub struct LuaRuntime {
    lua: Lua,
    game: Game,
    script_loaded: bool,
}

impl LuaRuntime {
    pub fn new() -> Result<Self> {
        let lua = Lua::new();

        register_all_types(&lua)?;

        super::instance::register_instance(&lua)?;

        register_raycast_params(&lua)?;

        let game = Game::new();
        lua.globals().set("game", game.clone())?;

        lua.globals().set("Workspace", game.workspace())?;
        lua.globals().set("Players", game.players())?;

        let wait_fn = lua.create_function(|_, seconds: Option<f64>| {
            let _seconds = seconds.unwrap_or(0.0);
            Ok(())
        })?;
        lua.globals().set("wait", wait_fn)?;

        let print_fn = lua.create_function(|_, args: MultiValue| {
            let msg: Vec<String> = args
                .iter()
                .map(|v| match v {
                    Value::Nil => "nil".to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                    Value::UserData(ud) => {
                        ud.to_string().unwrap_or_else(|_| "[userdata]".to_string())
                    }
                    _ => format!("{:?}", v),
                })
                .collect();
            println!("[Lua] {}", msg.join("\t"));
            Ok(())
        })?;
        lua.globals().set("print", print_fn)?;

        let warn_fn = lua.create_function(|_, args: MultiValue| {
            let msg: Vec<String> = args
                .iter()
                .map(|v| match v {
                    Value::Nil => "nil".to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                    _ => format!("{:?}", v),
                })
                .collect();
            eprintln!("[Lua WARN] {}", msg.join("\t"));
            Ok(())
        })?;
        lua.globals().set("warn", warn_fn)?;

        let math_table = lua.globals().get::<mlua::Table>("math")?;
        math_table.set("huge", f64::INFINITY)?;

        let random_fn =
            lua.create_function(|_, (min, max): (Option<i64>, Option<i64>)| match (min, max) {
                (None, None) => Ok(rand::random::<f64>()),
                (Some(max), None) => Ok((rand::random::<f64>() * max as f64).floor()),
                (Some(min), Some(max)) => {
                    let range = (max - min + 1) as f64;
                    Ok((rand::random::<f64>() * range).floor() + min as f64)
                }
                _ => Ok(0.0),
            })?;
        math_table.set("random", random_fn)?;

        let table_table = lua.globals().get::<mlua::Table>("table")?;

        let insert_fn = lua.create_function(
            |_, (tbl, pos_or_val, val): (mlua::Table, Value, Option<Value>)| {
                match val {
                    Some(v) => {
                        let pos: i64 = match pos_or_val {
                            Value::Number(n) => n as i64,
                            Value::Integer(i) => i,
                            _ => return Err(mlua::Error::runtime("Invalid position")),
                        };
                        tbl.raw_insert(pos, v)?;
                    }
                    None => {
                        let len = tbl.raw_len();
                        tbl.raw_insert(len as i64 + 1, pos_or_val)?;
                    }
                }
                Ok(())
            },
        )?;
        table_table.set("insert", insert_fn)?;

        let remove_fn =
            lua.create_function(|_, (tbl, pos): (mlua::Table, Option<i64>)| -> Result<Value> {
                let pos = pos.unwrap_or(tbl.raw_len() as i64);
                let val = tbl.raw_get(pos)?;
                tbl.raw_remove(pos)?;
                Ok(val)
            })?;
        table_table.set("remove", remove_fn)?;

        Ok(Self {
            lua,
            game,
            script_loaded: false,
        })
    }

    pub fn load_script(&mut self, source: &str) -> Result<()> {
        self.lua.load(source).exec()?;
        self.script_loaded = true;
        Ok(())
    }

    pub fn tick(&self, delta_time: f32) -> Result<()> {
        if !self.script_loaded {
            return Ok(());
        }

        let heartbeat = self.game.run_service().heartbeat();
        heartbeat.fire(
            &self.lua,
            MultiValue::from_iter([Value::Number(delta_time as f64)]),
        )?;

        Ok(())
    }

    pub fn add_player(&self, user_id: u64, name: &str) -> Instance {
        let player = Instance::from_data(InstanceData::new_player(user_id, name));
        self.game.players().add_player(player.clone());
        player
    }

    pub fn remove_player(&self, user_id: u64) {
        self.game.players().remove_player(user_id);
    }

    pub fn spawn_part(&self, name: &str) -> Instance {
        let part = Instance::from_data(InstanceData::new_part(name));
        self.game.workspace().add_child(part.clone());
        part
    }

    pub fn workspace(&self) -> WorkspaceService {
        self.game.workspace()
    }

    pub fn players(&self) -> PlayersService {
        self.game.players()
    }

    pub fn run_service(&self) -> RunService {
        self.game.run_service()
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn fire_player_added(&self, player: &Instance) -> Result<()> {
        let signal = self.game.players().data.lock().unwrap().player_added.clone();
        signal.fire(
            &self.lua,
            MultiValue::from_iter([Value::UserData(self.lua.create_userdata(player.clone())?)]),
        )?;
        Ok(())
    }

    pub fn fire_player_removing(&self, player: &Instance) -> Result<()> {
        let signal = self
            .game
            .players()
            .data
            .lock()
            .unwrap()
            .player_removing
            .clone();
        signal.fire(
            &self.lua,
            MultiValue::from_iter([Value::UserData(self.lua.create_userdata(player.clone())?)]),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = LuaRuntime::new().expect("Failed to create runtime");
        assert!(!runtime.script_loaded);
    }

    #[test]
    fn test_simple_script() {
        let mut runtime = LuaRuntime::new().expect("Failed to create runtime");
        runtime
            .load_script(
                r#"
            local part = Instance.new("Part")
            part.Name = "TestPart"
            part.Position = Vector3.new(10, 20, 30)
            part.Parent = Workspace
        "#,
            )
            .expect("Failed to load script");

        let children = runtime.workspace().get_children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name(), "TestPart");
    }

    #[test]
    fn test_heartbeat() {
        let mut runtime = LuaRuntime::new().expect("Failed to create runtime");
        runtime
            .load_script(
                r#"
            _G.tickCount = 0
            game:GetService("RunService").Heartbeat:Connect(function(dt)
                _G.tickCount = _G.tickCount + 1
            end)
        "#,
            )
            .expect("Failed to load script");

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0).expect("Failed to tick");
        }

        let tick_count: i64 = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("tickCount")
            .unwrap();
        assert_eq!(tick_count, 10);
    }
}
