//! AsyncThreadManager: Manages pending AsyncThreads for true yielding behavior.
//!
//! When a Lua script calls an async operation like DataStore:GetAsync(), the function
//! is implemented using `create_async_function`. This creates an AsyncThread that can
//! be polled using Rust's Future interface.
//!
//! This manager tracks all pending AsyncThreads and polls them each game tick using
//! `Waker::noop()`. When an operation completes, the result is automatically returned
//! to the Lua script.
//!
//! Flow:
//! 1. Async function is called, returns an AsyncThread
//! 2. AsyncThread is registered with this manager
//! 3. Each tick, poll_all() is called to check for completed operations
//! 4. When Poll::Ready, the operation completed and Lua resumes automatically

use mlua::{Lua, RegistryKey, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

/// Manages pending AsyncThreads that are waiting for async operations to complete.
pub struct AsyncThreadManager {
    /// Pending async threads, keyed by unique ID
    pending: HashMap<u64, PendingAsyncThread>,
    /// Counter for generating unique IDs
    next_id: u64,
}

/// A pending async thread waiting for an operation to complete.
struct PendingAsyncThread {
    /// The boxed future stored for polling
    /// We store the future directly since AsyncThread doesn't implement Clone
    future: Pin<Box<dyn Future<Output = mlua::Result<Value>> + Send>>,
    /// Registry key for cleanup tracking (optional, for the thread itself)
    #[allow(dead_code)]
    thread_key: Option<RegistryKey>,
}

impl AsyncThreadManager {
    /// Creates a new AsyncThreadManager.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            next_id: 1,
        }
    }

    /// Registers an async future for polling.
    ///
    /// The future should be created from an AsyncThread using `into_future()` or similar.
    /// Returns a unique ID that can be used to track this operation.
    pub fn register<F>(&mut self, future: F) -> u64
    where
        F: Future<Output = mlua::Result<Value>> + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;

        self.pending.insert(
            id,
            PendingAsyncThread {
                future: Box::pin(future),
                thread_key: None,
            },
        );

        id
    }

    /// Registers an async future with an associated registry key for cleanup.
    pub fn register_with_key<F>(&mut self, lua: &Lua, future: F) -> mlua::Result<u64>
    where
        F: Future<Output = mlua::Result<Value>> + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;

        // Create a dummy registry value for tracking (can be removed if not needed)
        let key = lua.create_registry_value(Value::Nil)?;

        self.pending.insert(
            id,
            PendingAsyncThread {
                future: Box::pin(future),
                thread_key: Some(key),
            },
        );

        Ok(id)
    }

    /// Polls all pending async threads (called each game tick).
    ///
    /// Uses `Waker::noop()` since we poll every tick and don't need wake notifications.
    /// Returns the number of completed operations.
    pub fn poll_all(&mut self, lua: &Lua) -> mlua::Result<usize> {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(&waker);

        let mut completed = Vec::new();

        for (&id, pending) in &mut self.pending {
            match pending.future.as_mut().poll(&mut cx) {
                Poll::Ready(result) => {
                    completed.push(id);
                    // Log errors but don't fail the whole poll operation
                    if let Err(e) = result {
                        eprintln!("[AsyncThreadManager] Async operation {} failed: {}", id, e);
                    }
                }
                Poll::Pending => {
                    // Still waiting, will poll again next tick
                }
            }
        }

        // Clean up completed operations
        for id in &completed {
            if let Some(pending) = self.pending.remove(id) {
                // Clean up registry key if present
                if let Some(key) = pending.thread_key {
                    let _ = lua.remove_registry_value(key);
                }
            }
        }

        Ok(completed.len())
    }

    /// Returns the number of pending async operations.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for AsyncThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_thread_manager_new() {
        let manager = AsyncThreadManager::new();
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_register_increments_id() {
        let mut manager = AsyncThreadManager::new();

        let id1 = manager.register(async { Ok(Value::Nil) });
        let id2 = manager.register(async { Ok(Value::Nil) });

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(manager.pending_count(), 2);
    }

    #[test]
    fn test_poll_ready_future() {
        let lua = Lua::new();
        let mut manager = AsyncThreadManager::new();

        // Register an immediately-ready future
        manager.register(async { Ok(Value::Nil) });

        assert_eq!(manager.pending_count(), 1);

        // Poll should complete it
        let completed = manager.poll_all(&lua).unwrap();
        assert_eq!(completed, 1);
        assert_eq!(manager.pending_count(), 0);
    }
}
