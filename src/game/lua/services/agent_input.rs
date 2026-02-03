use mlua::{Lua, MultiValue, UserData, UserDataFields, UserDataMethods, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::game::lua::events::{create_signal, track_yielded_threads, RBXScriptSignal};
use crate::game::lua::instance::Instance;

/// Represents an input from an agent
#[derive(Clone, Debug)]
pub struct AgentInput {
    pub input_type: String,
    pub data: serde_json::Value,
}

impl AgentInput {
    pub fn new(input_type: String, data: serde_json::Value) -> Self {
        Self { input_type, data }
    }
}

pub struct AgentInputServiceData {
    /// Pending inputs per user_id
    pub pending_inputs: HashMap<u64, Vec<AgentInput>>,
    /// InputReceived event signal
    pub input_received: RBXScriptSignal,
}

impl AgentInputServiceData {
    pub fn new() -> Self {
        Self {
            pending_inputs: HashMap::new(),
            input_received: create_signal("InputReceived"),
        }
    }
}

#[derive(Clone)]
pub struct AgentInputService {
    pub data: Arc<Mutex<AgentInputServiceData>>,
}

impl AgentInputService {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(AgentInputServiceData::new())),
        }
    }

    /// Queue an input for a player (called from Rust API)
    pub fn queue_input(&self, user_id: u64, input: AgentInput) {
        let mut data = self.data.lock().unwrap();
        data.pending_inputs
            .entry(user_id)
            .or_insert_with(Vec::new)
            .push(input);
    }

    /// Fire the InputReceived event for a player (called from Rust, passes to Lua)
    pub fn fire_input_received(
        &self,
        lua: &Lua,
        player: &Instance,
        input_type: &str,
        input_data: &serde_json::Value,
    ) -> mlua::Result<()> {
        let signal = self.data.lock().unwrap().input_received.clone();

        // Convert serde_json::Value to Lua table
        let lua_data = json_to_lua_value(lua, input_data)?;

        // Fire the generic InputReceived event
        let threads = signal.fire_as_coroutines(
            lua,
            MultiValue::from_iter([
                Value::UserData(lua.create_userdata(player.clone())?),
                Value::String(lua.create_string(input_type)?),
                lua_data,
            ]),
        )?;
        track_yielded_threads(lua, threads)?;

        // Handle GUI click events specially
        if input_type == "GuiClick" {
            if let Some(element_id) = input_data.get("element_id").and_then(|v| v.as_u64()) {
                self.handle_gui_click(lua, player, element_id)?;
            }
        }

        Ok(())
    }

    /// Handle a GUI click by finding the element and firing its MouseButton1Click signal
    fn handle_gui_click(&self, lua: &Lua, player: &Instance, element_id: u64) -> mlua::Result<()> {
        // Get PlayerGui from player
        let player_gui = {
            let data = player.data.lock().unwrap();
            data.player_data
                .as_ref()
                .and_then(|pd| pd.player_gui.as_ref())
                .and_then(|weak| weak.upgrade())
                .map(Instance::from_ref)
        };

        let Some(player_gui) = player_gui else {
            return Ok(());
        };

        // Find the GUI element with the matching ID
        if let Some(element) = Self::find_gui_element_by_id(&player_gui, element_id) {
            // Fire MouseButton1Click signal if this element has one
            let signal = {
                let data = element.data.lock().unwrap();
                data.gui_data
                    .as_ref()
                    .and_then(|g| g.mouse_button1_click.clone())
            };

            if let Some(signal) = signal {
                let threads = signal.fire_as_coroutines(lua, MultiValue::new())?;
                track_yielded_threads(lua, threads)?;
            }
        }

        Ok(())
    }

    /// Recursively find a GUI element by its instance ID
    fn find_gui_element_by_id(instance: &Instance, target_id: u64) -> Option<Instance> {
        // Check if this instance matches
        if instance.id().0 == target_id {
            return Some(instance.clone());
        }

        // Search children recursively
        for child in instance.get_children() {
            if let Some(found) = Self::find_gui_element_by_id(&child, target_id) {
                return Some(found);
            }
        }

        None
    }

    /// Get and clear pending inputs for a user (called from Lua via GetInputs)
    pub fn get_inputs(&self, user_id: u64) -> Vec<AgentInput> {
        let mut data = self.data.lock().unwrap();
        data.pending_inputs.remove(&user_id).unwrap_or_default()
    }

    /// Check if there are pending inputs for a user
    pub fn has_pending_inputs(&self, user_id: u64) -> bool {
        let data = self.data.lock().unwrap();
        data.pending_inputs
            .get(&user_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }
}

/// Convert serde_json::Value to Lua Value
fn json_to_lua_value(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj.iter() {
                table.set(k.as_str(), json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

impl UserData for AgentInputService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("InputReceived", |_, this| {
            Ok(this.data.lock().unwrap().input_received.clone())
        });

        fields.add_field_method_get("Name", |_, _| Ok("AgentInputService".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("AgentInputService".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // GetInputs(player) - returns array of pending inputs and clears them
        methods.add_method("GetInputs", |lua, this, player: Instance| {
            let user_id = player
                .data
                .lock()
                .unwrap()
                .player_data
                .as_ref()
                .map(|pd| pd.user_id)
                .unwrap_or(0);

            let inputs = this.get_inputs(user_id);

            // Convert to Lua table array
            let result = lua.create_table()?;
            for (i, input) in inputs.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("type", input.input_type.as_str())?;
                entry.set("data", json_to_lua_value(lua, &input.data)?)?;
                result.set(i + 1, entry)?;
            }

            Ok(result)
        });

        // HasPendingInputs(player) - check if there are pending inputs
        methods.add_method("HasPendingInputs", |_, this, player: Instance| {
            let user_id = player
                .data
                .lock()
                .unwrap()
                .player_data
                .as_ref()
                .map(|pd| pd.user_id)
                .unwrap_or(0);

            Ok(this.has_pending_inputs(user_id))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_input_service_creation() {
        let service = AgentInputService::new();
        assert!(!service.has_pending_inputs(1));
    }

    #[test]
    fn test_queue_and_get_inputs() {
        let service = AgentInputService::new();

        // Queue some inputs
        service.queue_input(
            1,
            AgentInput::new(
                "Fire".to_string(),
                serde_json::json!({"direction": [1.0, 0.0, 0.0]}),
            ),
        );
        service.queue_input(
            1,
            AgentInput::new(
                "MoveTo".to_string(),
                serde_json::json!({"position": [10.0, 0.0, 5.0]}),
            ),
        );

        assert!(service.has_pending_inputs(1));

        // Get inputs (should clear them)
        let inputs = service.get_inputs(1);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].input_type, "Fire");
        assert_eq!(inputs[1].input_type, "MoveTo");

        // Should be empty now
        assert!(!service.has_pending_inputs(1));
        assert!(service.get_inputs(1).is_empty());
    }

    #[test]
    fn test_multiple_users() {
        let service = AgentInputService::new();

        service.queue_input(1, AgentInput::new("Fire".to_string(), serde_json::json!({})));
        service.queue_input(2, AgentInput::new("Melee".to_string(), serde_json::json!({})));

        assert!(service.has_pending_inputs(1));
        assert!(service.has_pending_inputs(2));

        let inputs1 = service.get_inputs(1);
        assert_eq!(inputs1.len(), 1);
        assert_eq!(inputs1[0].input_type, "Fire");

        // User 2's inputs should still be there
        assert!(service.has_pending_inputs(2));

        let inputs2 = service.get_inputs(2);
        assert_eq!(inputs2.len(), 1);
        assert_eq!(inputs2[0].input_type, "Melee");
    }
}
