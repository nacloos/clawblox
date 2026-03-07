use crate::game::instance::{SpectatorEntity, SpectatorObservation, SpectatorPlayerInfo};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use uuid::Uuid;

const CHECKPOINT_INTERVAL: u64 = 60;
const WAL_CHECKPOINT_SECS: u64 = 300;

// --- Delta format ---

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum DeltaEntry {
    Add {
        id: u32,
        add: SpectatorEntity,
    },
    Remove {
        id: u32,
        rm: bool,
    },
    Update {
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        p: Option<[f32; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        r: Option<[[f32; 3]; 3]>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayerDelta {
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    p: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FrameDelta {
    entities: Vec<DeltaEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    players: Vec<PlayerDelta>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    sounds: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    recent_events: Vec<serde_json::Value>,
}

// --- Writer ---

pub struct RecordingWriter {
    pool: SqlitePool,
    checkpoint_interval: u64,
    last_checkpoint_obs: Option<SpectatorObservation>,
    last_checkpoint_tick: u64,
    last_wal_checkpoint: Instant,
    latest_tick: u64,
    latest_time_ms: u64,
    _tick_rate: u32,
}

impl RecordingWriter {
    pub async fn create(
        path: &Path,
        world_name: &str,
        world_dir: &str,
        tick_rate: u32,
    ) -> Result<Self, sqlx::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT)",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS checkpoints (
                tick INTEGER PRIMARY KEY,
                time_ms INTEGER NOT NULL,
                data BLOB NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS deltas (
                tick INTEGER NOT NULL,
                time_ms INTEGER NOT NULL,
                data BLOB NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_deltas_tick ON deltas(tick)")
            .execute(&pool)
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS inputs (
                tick INTEGER NOT NULL,
                time_ms INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                input_type TEXT NOT NULL,
                data TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_inputs_tick ON inputs(tick)")
            .execute(&pool)
            .await?;

        let now = chrono::Utc::now().to_rfc3339();
        for (k, v) in [
            ("world_name", world_name),
            ("world_dir", world_dir),
            ("created_at", &now),
            ("tick_rate", &tick_rate.to_string()),
            ("version", "1"),
        ] {
            sqlx::query("INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)")
                .bind(k)
                .bind(v)
                .execute(&pool)
                .await?;
        }

        Ok(Self {
            pool,
            checkpoint_interval: CHECKPOINT_INTERVAL,
            last_checkpoint_obs: None,
            last_checkpoint_tick: 0,
            last_wal_checkpoint: Instant::now(),
            latest_tick: 0,
            latest_time_ms: 0,
            _tick_rate: tick_rate,
        })
    }

    pub async fn record_tick(&mut self, tick: u64, time_ms: u64, obs: &SpectatorObservation) {
        let is_checkpoint = tick == 0 || tick % self.checkpoint_interval == 0;

        if is_checkpoint {
            if let Err(e) = self.write_checkpoint(tick, time_ms, obs).await {
                eprintln!("Warning: failed to write checkpoint at tick {}: {}", tick, e);
            }
            self.last_checkpoint_obs = Some(obs.clone());
            self.last_checkpoint_tick = tick;
        } else if let Some(ref base) = self.last_checkpoint_obs {
            let delta = compute_delta(base, obs);
            if let Err(e) = self.write_delta(tick, time_ms, &delta).await {
                eprintln!("Warning: failed to write delta at tick {}: {}", tick, e);
            }
        }

        // Periodic WAL checkpoint
        if self.last_wal_checkpoint.elapsed().as_secs() >= WAL_CHECKPOINT_SECS {
            let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(&self.pool)
                .await;
            self.last_wal_checkpoint = Instant::now();
        }

        self.latest_tick = self.latest_tick.max(tick);
        self.latest_time_ms = self.latest_time_ms.max(time_ms);
    }

    pub async fn record_input(
        &self,
        tick: u64,
        time_ms: u64,
        agent_id: &Uuid,
        input_type: &str,
        data: &serde_json::Value,
    ) {
        let data_str = serde_json::to_string(data).unwrap_or_default();
        if let Err(e) = sqlx::query(
            "INSERT INTO inputs (tick, time_ms, agent_id, input_type, data) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(tick as i64)
        .bind(time_ms as i64)
        .bind(agent_id.to_string())
        .bind(input_type)
        .bind(&data_str)
        .execute(&self.pool)
        .await
        {
            eprintln!("Warning: failed to record input at tick {}: {}", tick, e);
        }
    }

    pub async fn close(&mut self, total_ticks: u64, duration_ms: u64) {
        let final_ticks = total_ticks.max(self.latest_tick);
        let final_duration_ms = duration_ms.max(self.latest_time_ms);
        for (k, v) in [
            ("total_ticks", final_ticks.to_string()),
            ("duration_ms", final_duration_ms.to_string()),
        ] {
            let _ = sqlx::query("INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)")
                .bind(k)
                .bind(&v)
                .execute(&self.pool)
                .await;
        }
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await;
        self.pool.close().await;
    }

    async fn write_checkpoint(
        &self,
        tick: u64,
        time_ms: u64,
        obs: &SpectatorObservation,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_vec(obs)?;
        let compressed = zstd::encode_all(json.as_slice(), 3)?;
        sqlx::query("INSERT INTO checkpoints (tick, time_ms, data) VALUES (?, ?, ?)")
            .bind(tick as i64)
            .bind(time_ms as i64)
            .bind(&compressed)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn write_delta(
        &self,
        tick: u64,
        time_ms: u64,
        delta: &FrameDelta,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_vec(delta)?;
        let compressed = zstd::encode_all(json.as_slice(), 3)?;
        sqlx::query("INSERT INTO deltas (tick, time_ms, data) VALUES (?, ?, ?)")
            .bind(tick as i64)
            .bind(time_ms as i64)
            .bind(&compressed)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn compute_delta(base: &SpectatorObservation, current: &SpectatorObservation) -> FrameDelta {
    let mut entries = Vec::new();

    // Build lookup of base entities by id
    let base_entities: HashMap<u32, &SpectatorEntity> =
        base.entities.iter().map(|e| (e.id, e)).collect();
    let current_entities: HashMap<u32, &SpectatorEntity> =
        current.entities.iter().map(|e| (e.id, e)).collect();

    // Additions and changes
    for entity in &current.entities {
        match base_entities.get(&entity.id) {
            None => {
                entries.push(DeltaEntry::Add {
                    id: entity.id,
                    add: entity.clone(),
                });
            }
            Some(base_entity) => {
                let pos_changed = entity.position != base_entity.position;
                let rot_changed = entity.rotation != base_entity.rotation;
                if pos_changed || rot_changed {
                    entries.push(DeltaEntry::Update {
                        id: entity.id,
                        p: if pos_changed {
                            Some(entity.position)
                        } else {
                            None
                        },
                        r: if rot_changed { entity.rotation } else { None },
                    });
                }
            }
        }
    }

    // Removals
    for id in base_entities.keys() {
        if !current_entities.contains_key(id) {
            entries.push(DeltaEntry::Remove { id: *id, rm: true });
        }
    }

    // Player deltas
    let base_players: HashMap<Uuid, &SpectatorPlayerInfo> =
        base.players.iter().map(|p| (p.id, p)).collect();
    let mut player_deltas = Vec::new();
    for player in &current.players {
        if let Some(base_player) = base_players.get(&player.id) {
            let pos_changed = player.position != base_player.position;
            let health_changed = player.health != base_player.health;
            if pos_changed || health_changed {
                player_deltas.push(PlayerDelta {
                    id: player.id,
                    p: if pos_changed {
                        Some(player.position)
                    } else {
                        None
                    },
                    health: if health_changed {
                        Some(player.health)
                    } else {
                        None
                    },
                });
            }
        }
    }

    // Sounds and events: include full arrays if non-empty (they're small and stateful)
    let sounds: Vec<serde_json::Value> = if current.sounds != base.sounds {
        current
            .sounds
            .iter()
            .filter_map(|s| serde_json::to_value(s).ok())
            .collect()
    } else {
        Vec::new()
    };

    let recent_events: Vec<serde_json::Value> = if !current.recent_events.is_empty() {
        current
            .recent_events
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect()
    } else {
        Vec::new()
    };

    FrameDelta {
        entities: entries,
        players: player_deltas,
        sounds,
        recent_events,
    }
}

// --- Reader ---

pub struct RecordingMeta {
    pub world_name: String,
    pub world_dir: String,
    pub created_at: String,
    pub tick_rate: u32,
    pub total_ticks: u64,
    pub duration_ms: u64,
}

pub struct RecordingReader {
    pool: SqlitePool,
    checkpoint_ticks: Vec<u64>,
}

impl RecordingReader {
    pub async fn open(path: &Path) -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        let rows = sqlx::query("SELECT tick FROM checkpoints ORDER BY tick")
            .fetch_all(&pool)
            .await?;
        let checkpoint_ticks: Vec<u64> = rows.iter().map(|r| r.get::<i64, _>("tick") as u64).collect();

        Ok(Self {
            pool,
            checkpoint_ticks,
        })
    }

    pub async fn meta(&self) -> Result<RecordingMeta, sqlx::Error> {
        let rows = sqlx::query("SELECT key, value FROM meta")
            .fetch_all(&self.pool)
            .await?;
        let map: HashMap<String, String> = rows
            .iter()
            .map(|r| (r.get::<String, _>("key"), r.get::<String, _>("value")))
            .collect();

        Ok(RecordingMeta {
            world_name: map.get("world_name").cloned().unwrap_or_default(),
            world_dir: map.get("world_dir").cloned().unwrap_or_default(),
            created_at: map.get("created_at").cloned().unwrap_or_default(),
            tick_rate: map
                .get("tick_rate")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            total_ticks: map
                .get("total_ticks")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            duration_ms: map
                .get("duration_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        })
    }

    pub fn total_ticks(&self) -> u64 {
        self.checkpoint_ticks.last().copied().unwrap_or(0)
    }

    pub fn first_tick(&self) -> Option<u64> {
        self.checkpoint_ticks.first().copied()
    }

    pub async fn get_state_at(
        &self,
        tick: u64,
    ) -> Result<SpectatorObservation, Box<dyn std::error::Error + Send + Sync>> {
        // Find nearest checkpoint <= tick
        let cp_tick = match self.checkpoint_ticks.binary_search(&tick) {
            Ok(i) => self.checkpoint_ticks[i],
            Err(0) => {
                return Err("tick before first checkpoint".into());
            }
            Err(i) => self.checkpoint_ticks[i - 1],
        };

        // Load checkpoint
        let row = sqlx::query("SELECT data FROM checkpoints WHERE tick = ?")
            .bind(cp_tick as i64)
            .fetch_one(&self.pool)
            .await?;
        let compressed: Vec<u8> = row.get("data");
        let json = zstd::decode_all(compressed.as_slice())?;
        let mut obs: SpectatorObservation = serde_json::from_slice(&json)?;

        if cp_tick == tick {
            return Ok(obs);
        }

        // Apply deltas from cp_tick+1 to tick
        let delta_rows = sqlx::query(
            "SELECT data FROM deltas WHERE tick > ? AND tick <= ? ORDER BY tick",
        )
        .bind(cp_tick as i64)
        .bind(tick as i64)
        .fetch_all(&self.pool)
        .await?;

        for delta_row in &delta_rows {
            let compressed: Vec<u8> = delta_row.get("data");
            let json = zstd::decode_all(compressed.as_slice())?;
            let delta: FrameDelta = serde_json::from_slice(&json)?;
            apply_delta(&mut obs, &delta);
        }

        obs.tick = tick;
        Ok(obs)
    }
}

fn apply_delta(obs: &mut SpectatorObservation, delta: &FrameDelta) {
    // Apply entity deltas
    for entry in &delta.entities {
        match entry {
            DeltaEntry::Add { id: _, add } => {
                obs.entities.push(add.clone());
            }
            DeltaEntry::Remove { id, rm: _ } => {
                obs.entities.retain(|e| e.id != *id);
            }
            DeltaEntry::Update { id, p, r } => {
                if let Some(entity) = obs.entities.iter_mut().find(|e| e.id == *id) {
                    if let Some(pos) = p {
                        entity.position = *pos;
                    }
                    if let Some(rot) = r {
                        entity.rotation = Some(*rot);
                    }
                }
            }
        }
    }

    // Apply player deltas
    for pd in &delta.players {
        if let Some(player) = obs.players.iter_mut().find(|p| p.id == pd.id) {
            if let Some(pos) = pd.p {
                player.position = pos;
            }
            if let Some(health) = pd.health {
                player.health = health;
            }
        }
    }

    // Replace sounds if delta has any
    if !delta.sounds.is_empty() {
        if let Ok(sounds) = serde_json::from_value(serde_json::Value::Array(
            delta.sounds.clone(),
        )) {
            obs.sounds = sounds;
        }
    }

    // Replace recent_events
    if !delta.recent_events.is_empty() {
        if let Ok(events) = serde_json::from_value(serde_json::Value::Array(
            delta.recent_events.clone(),
        )) {
            obs.recent_events = events;
        }
    } else {
        obs.recent_events.clear();
    }
}

// Need PartialEq for SpectatorSound comparison in compute_delta
impl PartialEq for crate::game::instance::SpectatorSound {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.stream_id == other.stream_id
            && self.volume == other.volume
            && self.is_playing == other.is_playing
    }
}
