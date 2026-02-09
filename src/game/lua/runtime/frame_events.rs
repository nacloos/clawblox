use super::*;

impl LuaRuntime {
    pub fn tick(&self, delta_time: f32) -> Result<()> {
        self.begin_frame(delta_time)?;
        self.end_frame(delta_time)
    }

    /// Runs start-of-frame Lua work before physics (pending coroutines + Stepped event).
    pub fn begin_frame(&self, delta_time: f32) -> Result<()> {
        if !self.script_loaded {
            return Ok(());
        }

        // 1. Resume pending coroutines (callbacks that yielded on DataStore operations, etc.)
        self.resume_pending_coroutines()?;

        // 2. Fire Stepped as coroutines (Roblox pre-physics event)
        let stepped = self.game.run_service().stepped();
        let now = self.start_time.elapsed().as_secs_f64();
        let yielded_threads = stepped.fire_as_coroutines(
            &self.lua,
            MultiValue::from_iter([Value::Number(now), Value::Number(delta_time as f64)]),
        )?;

        // 3. Track any newly yielded coroutines for resumption on next tick
        self.track_yielded_threads(yielded_threads)?;

        Ok(())
    }

    /// Runs end-of-frame Lua work after physics (Heartbeat event).
    pub fn end_frame(&self, delta_time: f32) -> Result<()> {
        if !self.script_loaded {
            return Ok(());
        }

        // Fire Heartbeat as coroutines (allows callbacks to yield)
        let heartbeat = self.game.run_service().heartbeat();
        let yielded_threads = heartbeat.fire_as_coroutines(
            &self.lua,
            MultiValue::from_iter([Value::Number(delta_time as f64)]),
        )?;

        // Track any newly yielded coroutines for resumption on next tick
        self.track_yielded_threads(yielded_threads)?;

        Ok(())
    }
}
