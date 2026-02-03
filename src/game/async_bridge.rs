//! AsyncBridge: Bridges sync game thread with tokio runtime for async DB operations.
//!
//! The game loop runs on a sync thread (not tokio). When Lua scripts need to perform
//! async operations (like database calls), they send requests through AsyncBridge.
//!
//! Flow:
//! 1. Game thread sends AsyncRequest via crossbeam channel (non-blocking)
//! 2. Tokio task receives request, performs async DB operation
//! 3. Result sent back via oneshot channel
//! 4. Game thread polls oneshot (non-blocking) and resumes coroutine when ready

use crossbeam_channel::{Receiver, Sender};
use sqlx::PgPool;
use std::sync::Arc;
use std::thread;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Request types that can be sent from sync game thread to async tokio runtime
#[derive(Debug)]
pub enum AsyncRequest {
    DataStoreGet {
        game_id: Uuid,
        store_name: String,
        key: String,
        response_tx: oneshot::Sender<Result<Option<serde_json::Value>, String>>,
    },
    DataStoreSet {
        game_id: Uuid,
        store_name: String,
        key: String,
        value: serde_json::Value,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    /// Get sorted entries from an OrderedDataStore (for leaderboards)
    DataStoreGetSorted {
        game_id: Uuid,
        store_name: String,
        ascending: bool,
        limit: i32,
        response_tx: oneshot::Sender<Result<Vec<(String, serde_json::Value)>, String>>,
    },
}

/// Bridges sync game thread with tokio runtime
pub struct AsyncBridge {
    request_tx: Sender<AsyncRequest>,
}

impl AsyncBridge {
    /// Creates a new AsyncBridge and spawns the background tokio processor thread.
    ///
    /// The processor thread runs its own tokio runtime to handle async database operations.
    pub fn new(pool: Arc<PgPool>) -> Self {
        let (request_tx, request_rx) = crossbeam_channel::unbounded::<AsyncRequest>();

        // Spawn a dedicated thread with its own tokio runtime to process async requests
        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create tokio runtime for AsyncBridge");
            rt.block_on(async move {
                Self::process_requests(request_rx, pool).await;
            });
        });

        Self { request_tx }
    }

    /// Sends an async request (non-blocking from sync thread)
    pub fn send(&self, request: AsyncRequest) -> Result<(), String> {
        self.request_tx
            .send(request)
            .map_err(|e| format!("Failed to send async request: {}", e))
    }

    /// Background task that processes incoming requests
    async fn process_requests(request_rx: Receiver<AsyncRequest>, pool: Arc<PgPool>) {
        loop {
            // Block on receiving the next request
            let request = match request_rx.recv() {
                Ok(req) => req,
                Err(_) => {
                    // Channel closed, exit the loop
                    break;
                }
            };

            // Clone pool for the spawned task
            let pool = Arc::clone(&pool);

            // Spawn a task for each request to allow concurrent processing
            tokio::spawn(async move {
                match request {
                    AsyncRequest::DataStoreGet {
                        game_id,
                        store_name,
                        key,
                        response_tx,
                    } => {
                        let result = Self::db_get(&pool, game_id, &store_name, &key).await;
                        // Ignore send error - receiver may have been dropped (coroutine cancelled)
                        let _ = response_tx.send(result);
                    }
                    AsyncRequest::DataStoreSet {
                        game_id,
                        store_name,
                        key,
                        value,
                        response_tx,
                    } => {
                        let result = Self::db_set(&pool, game_id, &store_name, &key, &value).await;
                        // Ignore send error - receiver may have been dropped (coroutine cancelled)
                        let _ = response_tx.send(result);
                    }
                    AsyncRequest::DataStoreGetSorted {
                        game_id,
                        store_name,
                        ascending,
                        limit,
                        response_tx,
                    } => {
                        let result =
                            Self::db_get_sorted(&pool, game_id, &store_name, ascending, limit)
                                .await;
                        // Ignore send error - receiver may have been dropped (coroutine cancelled)
                        let _ = response_tx.send(result);
                    }
                }
            });
        }
    }

    /// Performs the actual database GET operation
    async fn db_get(
        pool: &PgPool,
        game_id: Uuid,
        store_name: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        let result: Option<(serde_json::Value,)> = sqlx::query_as(
            r#"
            SELECT value
            FROM data_stores
            WHERE game_id = $1 AND store_name = $2 AND key = $3
            "#,
        )
        .bind(game_id)
        .bind(store_name)
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Database error in GetAsync: {}", e))?;

        Ok(result.map(|(v,)| v))
    }

    /// Performs the actual database SET operation (upsert)
    async fn db_set(
        pool: &PgPool,
        game_id: Uuid,
        store_name: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO data_stores (game_id, store_name, key, value)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (game_id, store_name, key)
            DO UPDATE SET value = $4, updated_at = NOW()
            "#,
        )
        .bind(game_id)
        .bind(store_name)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map_err(|e| format!("Database error in SetAsync: {}", e))?;

        Ok(())
    }

    /// Performs the actual database GET SORTED operation (for OrderedDataStore leaderboards)
    async fn db_get_sorted(
        pool: &PgPool,
        game_id: Uuid,
        store_name: &str,
        ascending: bool,
        limit: i32,
    ) -> Result<Vec<(String, serde_json::Value)>, String> {
        // Use different queries based on sort order
        // The index idx_data_stores_score optimizes these queries
        let results: Vec<(String, serde_json::Value)> = if ascending {
            sqlx::query_as(
                r#"
                SELECT key, value
                FROM data_stores
                WHERE game_id = $1 AND store_name = $2 AND value ? 'score'
                ORDER BY (value->>'score')::numeric ASC NULLS LAST
                LIMIT $3
                "#,
            )
            .bind(game_id)
            .bind(store_name)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Database error in GetSortedAsync: {}", e))?
        } else {
            sqlx::query_as(
                r#"
                SELECT key, value
                FROM data_stores
                WHERE game_id = $1 AND store_name = $2 AND value ? 'score'
                ORDER BY (value->>'score')::numeric DESC NULLS LAST
                LIMIT $3
                "#,
            )
            .bind(game_id)
            .bind(store_name)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Database error in GetSortedAsync: {}", e))?
        };

        Ok(results)
    }
}

impl Clone for AsyncBridge {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would require a test database setup
    // Unit tests for the channel communication:

    #[test]
    fn test_async_bridge_clone() {
        // Create a minimal test - we can't actually test the full bridge
        // without a database, but we can verify cloning works
        let (tx, _rx) = crossbeam_channel::unbounded::<AsyncRequest>();
        let bridge = AsyncBridge { request_tx: tx };
        let _cloned = bridge.clone();
    }
}
