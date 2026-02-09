use super::*;

impl LuaRuntime {
    /// Resumes all pending coroutines and removes completed ones.
    /// Checks cancelled threads, scheduled times, and thread args before resuming.
    pub(super) fn resume_pending_coroutines(&self) -> Result<()> {
        let now = self.start_time.elapsed().as_secs_f64();

        // Get internal tables
        let schedule: mlua::Table = self.lua.globals().get("__clawblox_thread_schedule")?;
        let args_table: mlua::Table = self.lua.globals().get("__clawblox_thread_args")?;
        let cancelled: mlua::Table = self.lua.globals().get("__clawblox_cancelled_threads")?;

        // Drain all keys upfront so we don't hold a mutable borrow during iteration
        let keys: Vec<RegistryKey> = {
            let mut pending = self.pending_coroutines.lock().unwrap();
            pending.drain(..).collect()
        };

        let mut still_pending = Vec::new();

        for key in keys {
            // Get the thread from the registry
            let thread: Thread = match self.lua.registry_value(&key) {
                Ok(t) => t,
                Err(_) => {
                    // Thread was garbage collected or invalid, clean up
                    let _ = self.lua.remove_registry_value(key);
                    continue;
                }
            };

            // 1. Check if cancelled
            let is_cancelled: bool = cancelled.get(thread.clone()).unwrap_or(false);
            if is_cancelled {
                cancelled.set(thread.clone(), Value::Nil)?;
                schedule.set(thread.clone(), Value::Nil)?;
                args_table.set(thread.clone(), Value::Nil)?;
                let _ = self.lua.remove_registry_value(key);
                continue;
            }

            // Check if thread is still resumable
            if thread.status() != ThreadStatus::Resumable {
                schedule.set(thread.clone(), Value::Nil)?;
                args_table.set(thread.clone(), Value::Nil)?;
                let _ = self.lua.remove_registry_value(key);
                continue;
            }

            // 2. Check schedule — if time hasn't elapsed, keep pending
            let resume_at: Option<f64> = schedule.get(thread.clone())?;
            if let Some(resume_at) = resume_at {
                if now < resume_at {
                    // Not ready yet, keep pending
                    still_pending.push(key);
                    continue;
                }
                // Time satisfied, clear schedule entry
                schedule.set(thread.clone(), Value::Nil)?;
            }

            // 3. Determine resume args
            let resume_args: MultiValue = {
                let stored_args: Value = args_table.get(thread.clone())?;
                match stored_args {
                    Value::Table(tbl) => {
                        args_table.set(thread.clone(), Value::Nil)?;
                        let n: i64 = tbl.get("n").unwrap_or(0);
                        let mut args = Vec::new();
                        for i in 1..=n {
                            let v: Value = tbl.get(i)?;
                            args.push(v);
                        }
                        MultiValue::from_iter(args)
                    }
                    _ => {
                        // For task.wait threads, pass elapsed time
                        if resume_at.is_some() {
                            MultiValue::from_iter([Value::Number(now)])
                        } else {
                            MultiValue::new()
                        }
                    }
                }
            };

            // Try to resume the thread
            match thread.resume::<()>(resume_args) {
                Ok(()) => {
                    // Check if still yielded
                    if thread.status() == ThreadStatus::Resumable {
                        still_pending.push(key);
                    } else {
                        // Thread finished, clean up
                        let _ = self.lua.remove_registry_value(key);
                    }
                }
                Err(e) => {
                    // Async methods may still be pending and report via an error path while
                    // keeping the coroutine resumable. Keep polling these threads.
                    if thread.status() == ThreadStatus::Resumable {
                        still_pending.push(key);
                        continue;
                    }

                    // Thread errored — in Halt mode, propagate immediately
                    let error_mode = self
                        .lua
                        .app_data_ref::<ErrorMode>()
                        .map(|m| *m)
                        .unwrap_or(ErrorMode::Continue);
                    let _ = self.lua.remove_registry_value(key);
                    if error_mode == ErrorMode::Halt {
                        // Put remaining keys back before returning
                        let mut pending = self.pending_coroutines.lock().unwrap();
                        *pending = still_pending;
                        return Err(e);
                    }
                    eprintln!("[LuaRuntime] Coroutine error: {}", e);
                }
            }
        }

        let mut pending = self.pending_coroutines.lock().unwrap();
        *pending = still_pending;
        Ok(())
    }

    /// Tracks yielded threads for resumption on the next tick.
    pub(super) fn track_yielded_threads(&self, threads: Vec<Thread>) -> Result<()> {
        let mut pending = self.pending_coroutines.lock().unwrap();

        for thread in threads {
            if thread.status() == ThreadStatus::Resumable {
                // Store in registry to prevent garbage collection
                match self.lua.create_registry_value(thread) {
                    Ok(key) => pending.push(key),
                    Err(e) => {
                        eprintln!("[LuaRuntime] Failed to store yielded thread: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
