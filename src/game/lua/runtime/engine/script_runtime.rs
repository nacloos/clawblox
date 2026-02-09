use super::super::*;
use crate::game::lua::instance::{ClassName, Instance};

impl LuaRuntime {
    pub(crate) fn register_require(
        lua: &Lua,
        module_cache: Arc<Mutex<HashMap<u64, RegistryKey>>>,
        loading_modules: Arc<Mutex<HashSet<u64>>>,
        pending_coroutines: Arc<Mutex<Vec<RegistryKey>>>,
    ) -> Result<()> {
        let require_fn = lua.create_function(move |lua, module: Instance| -> Result<Value> {
            {
                let data = module.data.lock().unwrap();
                if data.class_name != ClassName::ModuleScript {
                    return Err(mlua::Error::runtime("require expects a ModuleScript"));
                }
            }

            let module_id = module.id().0;
            if let Some(key) = module_cache.lock().unwrap().get(&module_id) {
                return lua.registry_value::<Value>(key);
            }

            {
                let mut loading = loading_modules.lock().unwrap();
                if !loading.insert(module_id) {
                    return Err(mlua::Error::runtime("cyclic module dependency detected"));
                }
            }

            let result = (|| -> Result<Value> {
                let source = {
                    let data = module.data.lock().unwrap();
                    data.script_data
                        .as_ref()
                        .map(|s| s.source.clone())
                        .unwrap_or_default()
                };

                let old_script: Value = lua.globals().get("script").unwrap_or(Value::Nil);
                lua.globals().set("script", module.clone())?;

                let func = lua.load(&source).into_function()?;
                let thread = lua.create_thread(func)?;
                let out = match thread.resume::<Value>(()) {
                    Ok(value) => {
                        if thread.status() == ThreadStatus::Resumable {
                            let key = lua.create_registry_value(thread)?;
                            pending_coroutines.lock().unwrap().push(key);
                            return Err(mlua::Error::runtime(
                                "ModuleScript yielded during require (not yet supported)",
                            ));
                        }
                        value
                    }
                    Err(e) => {
                        if thread.status() == ThreadStatus::Resumable {
                            let key = lua.create_registry_value(thread)?;
                            pending_coroutines.lock().unwrap().push(key);
                            return Err(mlua::Error::runtime(
                                "ModuleScript yielded during require (not yet supported)",
                            ));
                        }
                        return Err(e);
                    }
                };

                lua.globals().set("script", old_script)?;
                Ok(out)
            })();

            loading_modules.lock().unwrap().remove(&module_id);

            match result {
                Ok(value) => {
                    let key = lua.create_registry_value(value.clone())?;
                    module_cache.lock().unwrap().insert(module_id, key);
                    Ok(value)
                }
                Err(e) => Err(e),
            }
        })?;

        lua.globals().set("require", require_fn)?;
        Ok(())
    }

    fn execute_script_instance(&self, script: &Instance) -> Result<()> {
        let (script_id, source, disabled) = {
            let data = script.data.lock().unwrap();
            let source = data
                .script_data
                .as_ref()
                .map(|s| s.source.clone())
                .unwrap_or_default();
            let disabled = data.script_data.as_ref().map(|s| s.disabled).unwrap_or(false);
            (data.id.0, source, disabled)
        };

        if disabled {
            return Ok(());
        }

        if self.executed_scripts.lock().unwrap().contains(&script_id) {
            return Ok(());
        }

        let old_script: Value = self.lua.globals().get("script").unwrap_or(Value::Nil);
        self.lua.globals().set("script", script.clone())?;

        let func = self.lua.load(&source).into_function()?;
        let thread = self.lua.create_thread(func)?;
        match thread.resume::<()>(()) {
            Ok(()) => {
                if thread.status() == ThreadStatus::Resumable {
                    let key = self.lua.create_registry_value(thread)?;
                    self.pending_coroutines.lock().unwrap().push(key);
                }
            }
            Err(e) => {
                if thread.status() == ThreadStatus::Resumable {
                    let key = self.lua.create_registry_value(thread)?;
                    self.pending_coroutines.lock().unwrap().push(key);
                } else {
                    self.lua.globals().set("script", old_script)?;
                    return Err(e);
                }
            }
        }

        self.lua.globals().set("script", old_script)?;
        self.executed_scripts.lock().unwrap().insert(script_id);
        Ok(())
    }

    pub(crate) fn discover_and_run_scripts(&self) -> Result<()> {
        let mut scripts = Vec::new();

        for inst in self.workspace().get_descendants() {
            if inst.class_name() == ClassName::Script {
                scripts.push(inst);
            }
        }

        let sss = self.game.server_script_service();
        for inst in sss.get_descendants() {
            if inst.class_name() == ClassName::Script {
                scripts.push(inst);
            }
        }

        for script in scripts {
            self.execute_script_instance(&script)?;
        }

        Ok(())
    }
}
