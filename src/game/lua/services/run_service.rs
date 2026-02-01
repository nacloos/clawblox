use mlua::{UserData, UserDataFields, UserDataMethods};
use std::sync::{Arc, Mutex};

use crate::game::lua::events::{create_signal, RBXScriptSignal};

pub struct RunServiceData {
    pub heartbeat: RBXScriptSignal,
    pub stepped: RBXScriptSignal,
    pub is_server: bool,
    pub is_client: bool,
}

impl RunServiceData {
    pub fn new(is_server: bool) -> Self {
        Self {
            heartbeat: create_signal("Heartbeat"),
            stepped: create_signal("Stepped"),
            is_server,
            is_client: !is_server,
        }
    }
}

#[derive(Clone)]
pub struct RunService {
    pub data: Arc<Mutex<RunServiceData>>,
}

impl RunService {
    pub fn new(is_server: bool) -> Self {
        Self {
            data: Arc::new(Mutex::new(RunServiceData::new(is_server))),
        }
    }

    pub fn heartbeat(&self) -> RBXScriptSignal {
        self.data.lock().unwrap().heartbeat.clone()
    }

    pub fn stepped(&self) -> RBXScriptSignal {
        self.data.lock().unwrap().stepped.clone()
    }
}

impl UserData for RunService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Heartbeat", |_, this| {
            Ok(this.data.lock().unwrap().heartbeat.clone())
        });
        fields.add_field_method_get("Stepped", |_, this| {
            Ok(this.data.lock().unwrap().stepped.clone())
        });
        fields.add_field_method_get("Name", |_, _| Ok("RunService".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("RunService".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("IsServer", |_, this, ()| Ok(this.data.lock().unwrap().is_server));
        methods.add_method("IsClient", |_, this, ()| Ok(this.data.lock().unwrap().is_client));
    }
}
