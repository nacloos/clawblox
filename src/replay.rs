use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct ReplayInfo {
    pub first_tick: u64,
    pub current_tick: u64,
    pub total_ticks: u64,
    pub tick_rate: u32,
    pub duration_ms: u64,
    pub speed: f32,
    pub paused: bool,
}

pub struct ReplayPlayback {
    first_tick: u64,
    current_tick: u64,
    total_ticks: u64,
    tick_rate: u32,
    duration_ms: u64,
    speed: f32,
    paused: bool,
    fractional_ticks: f64,
}

impl ReplayPlayback {
    pub fn new(
        first_tick: u64,
        total_ticks: u64,
        tick_rate: u32,
        duration_ms: u64,
        speed: f32,
    ) -> Result<Self, String> {
        if tick_rate == 0 {
            return Err("recording tick_rate must be greater than zero".to_string());
        }
        if total_ticks < first_tick {
            return Err(format!(
                "recording total_ticks ({total_ticks}) is before first_tick ({first_tick})"
            ));
        }

        Ok(Self {
            first_tick,
            current_tick: first_tick,
            total_ticks,
            tick_rate,
            duration_ms,
            speed: speed.clamp(0.1, 100.0),
            paused: false,
            fractional_ticks: 0.0,
        })
    }

    pub fn info(&self) -> ReplayInfo {
        ReplayInfo {
            first_tick: self.first_tick,
            current_tick: self.current_tick,
            total_ticks: self.total_ticks,
            tick_rate: self.tick_rate,
            duration_ms: self.duration_ms,
            speed: self.speed,
            paused: self.paused,
        }
    }

    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    pub fn seek(&mut self, tick: u64) {
        self.current_tick = tick.clamp(self.first_tick, self.total_ticks);
        self.fractional_ticks = 0.0;
    }

    pub fn set_speed(&mut self, value: f32) {
        self.speed = value.clamp(0.1, 100.0);
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn advance_frame(&mut self, frame_duration: Duration) {
        if self.paused || self.current_tick >= self.total_ticks {
            return;
        }

        let ticks_per_frame =
            self.speed as f64 * frame_duration.as_secs_f64() * self.tick_rate as f64;
        self.fractional_ticks += ticks_per_frame;
        let advance = self.fractional_ticks.floor() as u64;
        self.fractional_ticks -= advance as f64;
        self.current_tick = (self.current_tick + advance).min(self.total_ticks);
    }
}
