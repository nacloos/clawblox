use mlua::{Lua, MultiValue, Result, UserData, UserDataFields, UserDataMethods};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::events::{create_signal, RBXScriptSignal};

const DEBUG_ANIMATION_TRACKS: bool = false;

pub fn default_animation_length_seconds(animation_id: &str) -> f32 {
    match animation_id {
        "local://fire_rifle" => 0.16,
        "local://fire_shotgun" => 0.22,
        "local://reload_rifle" => 1.15,
        "local://reload_shotgun" => 1.35,
        "local://idle_default" => 0.9,
        "local://walk_default" => 1.2,
        _ => 0.5,
    }
}

static ANIMATION_TRACK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct AnimationTrack {
    pub data: Arc<Mutex<AnimationTrackData>>,
}

pub struct AnimationTrackData {
    pub track_id: u64,
    pub animation_id: String,
    pub owner_animator_id: u64,
    pub length: f32,
    pub priority: i32,
    pub looped: bool,
    pub speed: f32,
    pub time_position: f32,
    pub is_playing: bool,
    pub is_stopping: bool,
    pub weight_current: f32,
    pub weight_target: f32,
    pub effective_weight: f32,
    pub fade_from: f32,
    pub fade_to: f32,
    pub fade_duration: f32,
    pub fade_elapsed: f32,
    pub stopped: RBXScriptSignal,
    pub ended: RBXScriptSignal,
}

impl AnimationTrack {
    pub fn new(animation_id: String, owner_animator_id: u64, length: f32) -> Self {
        Self {
            data: Arc::new(Mutex::new(AnimationTrackData {
                track_id: ANIMATION_TRACK_ID.fetch_add(1, Ordering::SeqCst),
                animation_id,
                owner_animator_id,
                length: length.max(0.01),
                priority: 0,
                looped: false,
                speed: 1.0,
                time_position: 0.0,
                is_playing: false,
                is_stopping: false,
                weight_current: 0.0,
                weight_target: 1.0,
                effective_weight: 0.0,
                fade_from: 0.0,
                fade_to: 0.0,
                fade_duration: 0.0,
                fade_elapsed: 0.0,
                stopped: create_signal("Stopped"),
                ended: create_signal("Ended"),
            })),
        }
    }

    pub fn tick(&self, lua: &Lua, delta_time: f32) -> Result<()> {
        let mut fire_stopped = None;
        let mut fire_ended = None;
        let dt = delta_time.max(0.0);

        {
            let mut data = self.data.lock().unwrap();
            if !data.is_playing && !data.is_stopping {
                return Ok(());
            }

            if data.fade_duration > 0.0 {
                data.fade_elapsed = (data.fade_elapsed + dt).min(data.fade_duration);
                let alpha = (data.fade_elapsed / data.fade_duration).clamp(0.0, 1.0);
                data.weight_current = data.fade_from + (data.fade_to - data.fade_from) * alpha;
                if (data.fade_duration - data.fade_elapsed).abs() <= f32::EPSILON {
                    data.fade_duration = 0.0;
                    data.fade_elapsed = 0.0;
                    data.weight_current = data.fade_to;
                }
            } else {
                data.weight_current = data.weight_target.max(0.0);
            }
            data.weight_current = data.weight_current.max(0.0);

            if data.is_playing {
                data.time_position += dt * data.speed.max(0.0);
            }

            if data.is_playing && data.time_position >= data.length {
                if data.looped {
                    while data.time_position >= data.length {
                        data.time_position -= data.length;
                    }
                } else {
                    data.time_position = data.length;
                    data.is_playing = false;
                    data.is_stopping = false;
                    data.weight_current = 0.0;
                    data.weight_target = 0.0;
                    data.effective_weight = 0.0;
                    fire_stopped = Some(data.stopped.clone());
                    fire_ended = Some(data.ended.clone());
                }
            }

            if data.is_stopping && data.weight_current <= 0.0001 {
                data.is_stopping = false;
                if data.is_playing {
                    data.is_playing = false;
                    data.weight_current = 0.0;
                    data.weight_target = 0.0;
                    data.effective_weight = 0.0;
                    fire_stopped = Some(data.stopped.clone());
                }
            }
        }

        if let Some(stopped) = fire_stopped {
            let threads = stopped.fire_as_coroutines(lua, MultiValue::new())?;
            crate::game::lua::events::track_yielded_threads(lua, threads)?;
        }
        if let Some(ended) = fire_ended {
            let threads = ended.fire_as_coroutines(lua, MultiValue::new())?;
            crate::game::lua::events::track_yielded_threads(lua, threads)?;
        }
        Ok(())
    }
}

impl UserData for AnimationTrack {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("TrackId", |_, this| Ok(this.data.lock().unwrap().track_id));
        fields.add_field_method_get("Length", |_, this| Ok(this.data.lock().unwrap().length));
        fields.add_field_method_get("Priority", |_, this| Ok(this.data.lock().unwrap().priority));
        fields.add_field_method_set("Priority", |_, this, priority: i32| {
            this.data.lock().unwrap().priority = priority;
            Ok(())
        });
        fields.add_field_method_get("Looped", |_, this| Ok(this.data.lock().unwrap().looped));
        fields.add_field_method_set("Looped", |_, this, looped: bool| {
            this.data.lock().unwrap().looped = looped;
            Ok(())
        });
        fields.add_field_method_get("Speed", |_, this| Ok(this.data.lock().unwrap().speed));
        fields.add_field_method_set("Speed", |_, this, speed: f32| {
            this.data.lock().unwrap().speed = speed.max(0.0);
            Ok(())
        });
        fields.add_field_method_get("TimePosition", |_, this| {
            Ok(this.data.lock().unwrap().time_position)
        });
        fields.add_field_method_set("TimePosition", |_, this, time_position: f32| {
            let mut data = this.data.lock().unwrap();
            data.time_position = time_position.clamp(0.0, data.length);
            Ok(())
        });
        fields.add_field_method_get("IsPlaying", |_, this| {
            Ok(this.data.lock().unwrap().is_playing)
        });
        fields.add_field_method_get("WeightCurrent", |_, this| {
            Ok(this.data.lock().unwrap().weight_current)
        });
        fields.add_field_method_get("WeightTarget", |_, this| {
            Ok(this.data.lock().unwrap().weight_target)
        });
        fields.add_field_method_get("Stopped", |_, this| {
            Ok(this.data.lock().unwrap().stopped.clone())
        });
        fields.add_field_method_get("Ended", |_, this| Ok(this.data.lock().unwrap().ended.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Play", |_, this, (fade_time, weight, speed): (Option<f32>, Option<f32>, Option<f32>)| {
            let mut data = this.data.lock().unwrap();
            let fade = fade_time.unwrap_or(0.0).max(0.0);
            let target_weight = weight.unwrap_or(1.0).max(0.0);
            let was_playing = data.is_playing;
            let prev_time = data.time_position;
            data.time_position = 0.0;
            data.is_playing = true;
            data.is_stopping = false;
            data.weight_target = target_weight;
            if let Some(s) = speed {
                data.speed = s.max(0.0);
            }
            if fade > 0.0 {
                data.fade_from = data.weight_current.max(0.0);
                data.fade_to = target_weight;
                data.fade_duration = fade;
                data.fade_elapsed = 0.0;
            } else {
                data.fade_duration = 0.0;
                data.fade_elapsed = 0.0;
                data.fade_from = target_weight;
                data.fade_to = target_weight;
                data.weight_current = target_weight;
            }
            if DEBUG_ANIMATION_TRACKS && (data.animation_id == "local://walk_default" || data.animation_id == "local://idle_default") {
                eprintln!(
                    "[AnimTrack] Play track={} anim={} owner={} len={:.3} prev_playing={} prev_time={:.3} fade={:.3} weight={:.3} speed={:.3}",
                    data.track_id,
                    data.animation_id,
                    data.owner_animator_id,
                    data.length,
                    was_playing,
                    prev_time,
                    fade,
                    target_weight,
                    data.speed,
                );
            }
            Ok(())
        });
        methods.add_method("Stop", |lua, this, fade_time: Option<f32>| {
            let fade = fade_time.unwrap_or(0.0).max(0.0);
            let stopped = {
                let mut data = this.data.lock().unwrap();
                let should_log = data.animation_id == "local://walk_default"
                    || data.animation_id == "local://idle_default";
                if DEBUG_ANIMATION_TRACKS && should_log {
                    eprintln!(
                        "[AnimTrack] Stop track={} anim={} owner={} time={:.3} fade={:.3} is_playing={} is_stopping={}",
                        data.track_id,
                        data.animation_id,
                        data.owner_animator_id,
                        data.time_position,
                        fade,
                        data.is_playing,
                        data.is_stopping,
                    );
                }
                if !data.is_playing && !data.is_stopping {
                    None
                } else if fade > 0.0 {
                    data.is_stopping = true;
                    data.weight_target = 0.0;
                    data.fade_from = data.weight_current.max(0.0);
                    data.fade_to = 0.0;
                    data.fade_duration = fade;
                    data.fade_elapsed = 0.0;
                    None
                } else {
                    data.is_playing = false;
                    data.is_stopping = false;
                    data.weight_current = 0.0;
                    data.weight_target = 0.0;
                    data.effective_weight = 0.0;
                    Some(data.stopped.clone())
                }
            };
            if let Some(stopped) = stopped {
                let threads = stopped.fire_as_coroutines(lua, MultiValue::new())?;
                crate::game::lua::events::track_yielded_threads(lua, threads)?;
            }
            Ok(())
        });
        methods.add_method("AdjustSpeed", |_, this, speed: f32| {
            this.data.lock().unwrap().speed = speed.max(0.0);
            Ok(())
        });
        methods.add_method("AdjustWeight", |_, this, (weight, fade_time): (f32, Option<f32>)| {
            let mut data = this.data.lock().unwrap();
            let fade = fade_time.unwrap_or(0.0).max(0.0);
            let target = weight.max(0.0);
            data.weight_target = target;
            if fade > 0.0 {
                data.fade_from = data.weight_current.max(0.0);
                data.fade_to = target;
                data.fade_duration = fade;
                data.fade_elapsed = 0.0;
            } else {
                data.fade_duration = 0.0;
                data.fade_elapsed = 0.0;
                data.fade_from = target;
                data.fade_to = target;
                data.weight_current = target;
            }
            Ok(())
        });
    }
}

#[derive(Clone)]
pub struct AnimationScheduler {
    pub tracks: Arc<Mutex<Vec<AnimationTrack>>>,
}

impl Default for AnimationScheduler {
    fn default() -> Self {
        Self {
            tracks: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnimationTrackSnapshot {
    pub track_id: u64,
    pub animation_id: String,
    pub owner_animator_id: u64,
    pub length: f32,
    pub priority: i32,
    pub time_position: f32,
    pub speed: f32,
    pub looped: bool,
    pub is_playing: bool,
    pub is_stopping: bool,
    pub weight_current: f32,
    pub weight_target: f32,
    pub effective_weight: f32,
}

impl AnimationScheduler {
    fn apply_priority_mask_for_animator(tracks: Vec<AnimationTrack>) {
        let mut ranked = Vec::new();
        for track in tracks {
            let data = track.data.lock().unwrap();
            ranked.push((track.clone(), data.priority, data.track_id, data.weight_current.max(0.0)));
        }

        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));

        let mut remaining = 1.0_f32;
        for (track, _priority, _track_id, requested_weight) in ranked {
            let mut data = track.data.lock().unwrap();
            let applied = requested_weight.min(remaining).max(0.0);
            data.effective_weight = applied;
            remaining = (remaining - applied).max(0.0);
        }
    }

    pub fn register(&self, track: AnimationTrack) {
        self.tracks.lock().unwrap().push(track);
    }

    pub fn tick(&self, lua: &Lua, delta_time: f32) -> Result<()> {
        let tracks = self.tracks.lock().unwrap().clone();
        for track in &tracks {
            track.tick(lua, delta_time)?;
        }

        let mut by_animator: HashMap<u64, Vec<AnimationTrack>> = HashMap::new();
        for track in &tracks {
            let data = track.data.lock().unwrap();
            if data.is_playing || data.is_stopping || data.weight_current > 0.0 {
                by_animator
                    .entry(data.owner_animator_id)
                    .or_default()
                    .push(track.clone());
            } else if data.effective_weight != 0.0 {
                drop(data);
                let mut data = track.data.lock().unwrap();
                data.effective_weight = 0.0;
            }
        }

        for (_, animator_tracks) in by_animator {
            Self::apply_priority_mask_for_animator(animator_tracks);
        }
        Ok(())
    }

    pub fn playing_tracks_for_animator(&self, animator_id: u64) -> Vec<AnimationTrack> {
        let tracks = self.tracks.lock().unwrap().clone();
        tracks
            .into_iter()
            .filter(|track| {
                let Ok(data) = track.data.lock() else {
                    return false;
                };
                data.owner_animator_id == animator_id && (data.is_playing || data.is_stopping)
            })
            .collect()
    }

    pub fn active_tracks_for_animator(&self, animator_id: u64) -> Vec<AnimationTrackSnapshot> {
        let tracks = self.tracks.lock().unwrap().clone();
        tracks
            .into_iter()
            .filter_map(|track| {
                let data = track.data.lock().ok()?;
                if data.owner_animator_id != animator_id {
                    return None;
                }
                if !data.is_playing && !data.is_stopping && data.effective_weight <= 0.0 {
                    return None;
                }
                Some(AnimationTrackSnapshot {
                    track_id: data.track_id,
                    animation_id: data.animation_id.clone(),
                    owner_animator_id: data.owner_animator_id,
                    length: data.length,
                    priority: data.priority,
                    time_position: data.time_position,
                    speed: data.speed,
                    looped: data.looped,
                    is_playing: data.is_playing,
                    is_stopping: data.is_stopping,
                    weight_current: data.weight_current,
                    weight_target: data.weight_target,
                    effective_weight: data.effective_weight,
                })
            })
            .collect()
    }
}
