pub mod models;

use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::PgPool;
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::game::GameManagerHandle;

pub async fn create_pool() -> Result<PgPool, sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/clawblox".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    Ok(pool)
}

pub async fn create_sqlite_pool(db_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

pub async fn ensure_local_datastore_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS data_stores (
            game_id TEXT NOT NULL,
            store_name TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (game_id, store_name, key)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_data_stores_game_store
        ON data_stores(game_id, store_name)
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Cleans up orphaned instances from DB on startup.
/// Called when server restarts - any instances that were "running" in DB but no longer
/// exist in memory are orphaned.
pub async fn reconcile_instances(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Mark all running/waiting instances as orphaned
    let result = sqlx::query(
        "UPDATE game_instances SET status = 'orphaned' WHERE status IN ('running', 'waiting', 'playing')"
    )
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        eprintln!(
            "[Reconcile] Marked {} orphaned instances from previous session",
            result.rows_affected()
        );
    }

    // Clear instance_id from game_players that reference orphaned instances
    let cleared = sqlx::query(
        "UPDATE game_players SET instance_id = NULL WHERE instance_id IN
         (SELECT id FROM game_instances WHERE status = 'orphaned')",
    )
    .execute(pool)
    .await?;

    if cleared.rows_affected() > 0 {
        eprintln!(
            "[Reconcile] Cleared {} orphaned player-instance associations",
            cleared.rows_affected()
        );
    }

    Ok(())
}

/// Background task that periodically syncs in-memory instance state to the database.
/// This keeps the DB up-to-date for debugging/monitoring purposes.
pub async fn sync_instances_to_db(pool: Arc<PgPool>, state: GameManagerHandle) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        for entry in state.instances.iter() {
            let instance_id = *entry.key();
            let (game_id, player_count, status) = {
                let instance = entry.value().read();
                (
                    instance.game_id,
                    instance.players.len() as i32,
                    match instance.status {
                        crate::game::instance::GameStatus::Waiting => "waiting",
                        crate::game::instance::GameStatus::Playing => "playing",
                        crate::game::instance::GameStatus::Finished => "finished",
                    },
                )
            };

            // Upsert instance record
            let result = sqlx::query(
                "INSERT INTO game_instances (id, game_id, status, player_count)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (id) DO UPDATE SET status = $3, player_count = $4",
            )
            .bind(instance_id)
            .bind(game_id)
            .bind(status)
            .bind(player_count)
            .execute(pool.as_ref())
            .await;

            if let Err(e) = result {
                eprintln!("[Sync] Failed to sync instance {}: {}", instance_id, e);
            }
        }

        // Clean up DB records for destroyed instances
        let active_ids: Vec<Uuid> = state.instances.iter().map(|e| *e.key()).collect();
        if !active_ids.is_empty() {
            // Mark instances not in active_ids as destroyed
            let _ = sqlx::query(
                "UPDATE game_instances SET status = 'destroyed'
                 WHERE status NOT IN ('orphaned', 'destroyed')
                 AND id != ALL($1)",
            )
            .bind(&active_ids)
            .execute(pool.as_ref())
            .await;
        }
    }
}
