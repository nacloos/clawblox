use mlua::{Function, Lua, MultiValue, RegistryKey, Result, Thread, UserData, UserDataFields, UserDataMethods};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::game::instance::ErrorMode;

static CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct RBXScriptConnection {
    id: u64,
    connected: Arc<Mutex<bool>>,
    signal: Arc<Mutex<SignalInner>>,
}

impl RBXScriptConnection {
    pub fn new(signal: Arc<Mutex<SignalInner>>) -> Self {
        Self {
            id: CONNECTION_ID.fetch_add(1, Ordering::SeqCst),
            connected: Arc::new(Mutex::new(true)),
            signal,
        }
    }

    pub fn disconnect(&self) {
        *self.connected.lock().unwrap() = false;
        self.signal.lock().unwrap().remove_connection(self.id);
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl UserData for RBXScriptConnection {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Connected", |_, this| Ok(this.is_connected()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Disconnect", |_, this, ()| {
            this.disconnect();
            Ok(())
        });
    }
}

struct ConnectionEntry {
    id: u64,
    callback: RegistryKey,
    once: bool,
}

pub struct SignalInner {
    connections: Vec<ConnectionEntry>,
}

impl SignalInner {
    fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    fn remove_connection(&mut self, id: u64) {
        self.connections.retain(|c| c.id != id);
    }
}

#[derive(Clone)]
pub struct RBXScriptSignal {
    inner: Arc<Mutex<SignalInner>>,
    name: String,
}

pub fn track_yielded_threads(lua: &Lua, threads: Vec<Thread>) -> Result<()> {
    if threads.is_empty() {
        return Ok(());
    }

    let tracker: Function = match lua.globals().get("__clawblox_track_thread") {
        Ok(f) => f,
        Err(_) => {
            eprintln!("[LuaRuntime] Missing coroutine tracker (__clawblox_track_thread)");
            return Ok(());
        }
    };

    for thread in threads {
        if thread.status() == mlua::ThreadStatus::Resumable {
            tracker.call::<()>(thread)?;
        }
    }

    Ok(())
}

impl fmt::Debug for RBXScriptSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RBXScriptSignal")
            .field("name", &self.name)
            .finish()
    }
}

impl RBXScriptSignal {
    pub fn new(name: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SignalInner::new())),
            name: name.to_string(),
        }
    }

    pub fn connect(&self, lua: &Lua, callback: Function) -> Result<RBXScriptConnection> {
        let key = lua.create_registry_value(callback)?;
        let connection = RBXScriptConnection::new(Arc::clone(&self.inner));
        let id = connection.id();

        self.inner.lock().unwrap().connections.push(ConnectionEntry {
            id,
            callback: key,
            once: false,
        });

        Ok(connection)
    }

    pub fn once(&self, lua: &Lua, callback: Function) -> Result<RBXScriptConnection> {
        let key = lua.create_registry_value(callback)?;
        let connection = RBXScriptConnection::new(Arc::clone(&self.inner));
        let id = connection.id();

        self.inner.lock().unwrap().connections.push(ConnectionEntry {
            id,
            callback: key,
            once: true,
        });

        Ok(connection)
    }

    pub fn fire(&self, lua: &Lua, args: MultiValue) -> Result<()> {
        let mut to_remove = Vec::new();

        let connections: Vec<_> = {
            let inner = self.inner.lock().unwrap();
            inner.connections.iter().map(|c| (c.id, c.once)).collect()
        };

        for (id, once) in connections {
            let callback_key = {
                let inner = self.inner.lock().unwrap();
                inner
                    .connections
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| &c.callback)
                    .and_then(|k| lua.registry_value::<Function>(k).ok())
            };

            if let Some(callback) = callback_key {
                if let Err(e) = callback.call::<()>(args.clone()) {
                    let error_mode = lua
                        .app_data_ref::<ErrorMode>()
                        .map(|m| *m)
                        .unwrap_or(ErrorMode::Continue);
                    if error_mode == ErrorMode::Halt {
                        return Err(e);
                    }
                    eprintln!("[Lua Error] Callback error in signal '{}': {}", self.name, e);
                }
                if once {
                    to_remove.push(id);
                }
            }
        }

        for id in to_remove {
            self.inner.lock().unwrap().remove_connection(id);
        }

        Ok(())
    }

    /// Fires the signal with each callback running in its own coroutine.
    ///
    /// This allows callbacks to yield (e.g., for async operations like DataStore:GetAsync)
    /// without blocking other callbacks. Returns the coroutine threads for tracking.
    ///
    /// Yielded threads should be tracked by the CoroutineManager if they call async functions.
    pub fn fire_as_coroutines(&self, lua: &Lua, args: MultiValue) -> Result<Vec<Thread>> {
        let mut to_remove = Vec::new();
        let mut threads = Vec::new();

        let connections: Vec<_> = {
            let inner = self.inner.lock().unwrap();
            inner.connections.iter().map(|c| (c.id, c.once)).collect()
        };

        for (id, once) in connections {
            let callback_key = {
                let inner = self.inner.lock().unwrap();
                inner
                    .connections
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| &c.callback)
                    .and_then(|k| lua.registry_value::<Function>(k).ok())
            };

            if let Some(callback) = callback_key {
                // Create a new coroutine for this callback
                let thread = lua.create_thread(callback)?;

                // Resume the coroutine with the arguments
                match thread.resume::<()>(args.clone()) {
                    Ok(()) => {
                        // Callback completed without yielding
                        // Check if thread yielded after completion (async functions do this)
                        if thread.status() == mlua::ThreadStatus::Resumable {
                            threads.push(thread);
                        }
                    }
                    Err(e) => {
                        // Check if the thread yielded (this is normal for async operations)
                        if thread.status() == mlua::ThreadStatus::Resumable {
                            // Thread yielded - track it for later resumption
                            threads.push(thread);
                        } else {
                            // Actual error — in Halt mode, propagate immediately
                            let error_mode = lua
                                .app_data_ref::<ErrorMode>()
                                .map(|m| *m)
                                .unwrap_or(ErrorMode::Continue);
                            if error_mode == ErrorMode::Halt {
                                return Err(e);
                            }
                            eprintln!(
                                "[Lua Error] Callback error in signal '{}': {}",
                                self.name, e
                            );
                        }
                    }
                }

                if once {
                    to_remove.push(id);
                }
            }
        }

        for id in to_remove {
            self.inner.lock().unwrap().remove_connection(id);
        }

        Ok(threads)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl UserData for RBXScriptSignal {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Connect", |lua, this, callback: Function| {
            this.connect(lua, callback)
        });

        methods.add_method("Once", |lua, this, callback: Function| {
            this.once(lua, callback)
        });

        methods.add_method("Wait", |_, this, ()| {
            Ok(format!(
                "[Signal:Wait() not fully implemented for {}]",
                this.name
            ))
        });
    }
}

pub fn create_signal(name: &str) -> RBXScriptSignal {
    RBXScriptSignal::new(name)
}
