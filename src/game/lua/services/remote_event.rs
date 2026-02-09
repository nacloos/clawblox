use mlua::{Lua, Result, UserData, UserDataFields, UserDataMethods, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ReplicatedEvent {
    pub name: String,
    pub payload: serde_json::Value,
    pub reliable: bool,
}

pub struct RemoteEventServiceData {
    pub queued_events: Vec<ReplicatedEvent>,
}

impl RemoteEventServiceData {
    pub fn new() -> Self {
        Self {
            queued_events: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct RemoteEventService {
    pub data: Arc<Mutex<RemoteEventServiceData>>,
}

impl RemoteEventService {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(RemoteEventServiceData::new())),
        }
    }

    pub fn fire_all_clients(
        &self,
        name: String,
        payload: serde_json::Value,
        reliable: bool,
    ) {
        let mut data = self.data.lock().unwrap();
        data.queued_events.push(ReplicatedEvent {
            name,
            payload,
            reliable,
        });
    }

    pub fn drain_events(&self) -> Vec<ReplicatedEvent> {
        let mut data = self.data.lock().unwrap();
        std::mem::take(&mut data.queued_events)
    }
}

fn lua_value_to_json(lua: &Lua, value: Value) -> Result<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(v) => Ok(serde_json::Value::Bool(v)),
        Value::Integer(v) => Ok(serde_json::json!(v)),
        Value::Number(v) => Ok(serde_json::json!(v)),
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            let mut array_like = true;
            let mut max_index: usize = 0;
            let mut int_keys = Vec::<usize>::new();
            let mut map = BTreeMap::<String, serde_json::Value>::new();

            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                let json_v = lua_value_to_json(lua, v)?;
                match k {
                    Value::Integer(i) if i > 0 => {
                        let idx = i as usize;
                        int_keys.push(idx);
                        max_index = max_index.max(idx);
                        map.insert(idx.to_string(), json_v);
                    }
                    Value::Number(n) if n.fract() == 0.0 && n > 0.0 => {
                        let idx = n as usize;
                        int_keys.push(idx);
                        max_index = max_index.max(idx);
                        map.insert(idx.to_string(), json_v);
                    }
                    Value::String(s) => {
                        array_like = false;
                        map.insert(s.to_str()?.to_string(), json_v);
                    }
                    other => {
                        array_like = false;
                        map.insert(format!("{:?}", other), json_v);
                    }
                }
            }

            if array_like {
                if int_keys.is_empty() {
                    return Ok(serde_json::Value::Array(Vec::new()));
                }
                int_keys.sort_unstable();
                int_keys.dedup();
                let contiguous = int_keys.len() == max_index && int_keys.first() == Some(&1);
                if contiguous {
                    let mut out = Vec::with_capacity(max_index);
                    for idx in 1..=max_index {
                        out.push(map.remove(&idx.to_string()).unwrap_or(serde_json::Value::Null));
                    }
                    return Ok(serde_json::Value::Array(out));
                }
            }

            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k, v);
            }
            Ok(serde_json::Value::Object(obj))
        }
        Value::UserData(ud) => {
            if let Ok(v) = ud.borrow::<crate::game::lua::types::Vector3>() {
                return Ok(serde_json::json!([v.x, v.y, v.z]));
            }
            if let Ok(v) = ud.borrow::<crate::game::lua::types::Color3>() {
                return Ok(serde_json::json!([v.r, v.g, v.b]));
            }
            if let Ok(v) = ud.borrow::<crate::game::lua::types::CFrame>() {
                return Ok(serde_json::json!({
                    "position": [v.position.x, v.position.y, v.position.z],
                    "rotation": v.rotation,
                }));
            }
            Ok(serde_json::Value::String("[userdata]".to_string()))
        }
        _ => Ok(serde_json::Value::String(format!("{:?}", value))),
    }
}

impl UserData for RemoteEventService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, _| Ok("RemoteEventService".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("RemoteEventService".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "FireAllClients",
            |lua, this, (name, payload): (String, Value)| {
                let event_name = name.trim();
                if event_name.is_empty() {
                    return Ok(());
                }
                let payload_json = lua_value_to_json(lua, payload)?;
                this.fire_all_clients(event_name.to_string(), payload_json, true);
                Ok(())
            },
        );

        methods.add_method(
            "FireAllClientsUnreliable",
            |lua, this, (name, payload): (String, Value)| {
                let event_name = name.trim();
                if event_name.is_empty() {
                    return Ok(());
                }
                let payload_json = lua_value_to_json(lua, payload)?;
                this.fire_all_clients(event_name.to_string(), payload_json, false);
                Ok(())
            },
        );
    }
}
