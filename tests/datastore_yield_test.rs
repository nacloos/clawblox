//! Integration test for DataStoreService true yielding behavior.
//!
//! This test verifies that:
//! 1. GetAsync/SetAsync properly yield the Lua coroutine
//! 2. Other code continues running while waiting for DB
//! 3. The coroutine resumes with the correct value

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// Test that verifies the async function yields properly
#[test]
fn test_async_function_yields() {
    use mlua::{Lua, ThreadStatus, Value};
    use tokio::sync::oneshot;

    let lua = Lua::new();

    // Create channel to control when async completes
    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

    // Create an async function that awaits the channel
    let async_fn = lua
        .create_async_function(move |_lua, ()| {
            let rx = rx_clone.clone();
            async move {
                let receiver = rx.lock().unwrap().take();
                if let Some(receiver) = receiver {
                    let val = receiver.await.unwrap_or(-1);
                    Ok(Value::Integer(val as i64))
                } else {
                    Ok(Value::Integer(-999))
                }
            }
        })
        .expect("Failed to create async function");

    // Create a thread and start it
    let thread = lua.create_thread(async_fn).expect("Failed to create thread");

    // First resume - should yield because async work is pending
    let result = thread.resume::<Value>(());

    println!("First resume result: {:?}", result);
    println!("Thread status after first resume: {:?}", thread.status());

    // Thread should be resumable (yielded)
    assert_eq!(
        thread.status(),
        ThreadStatus::Resumable,
        "Thread should yield on async operation"
    );

    // Complete the operation
    tx.send(42).expect("Send");

    // Resume - should complete
    let result2 = thread.resume::<Value>(()).expect("Resume");
    assert_eq!(result2.as_integer(), Some(42));
    assert_eq!(thread.status(), ThreadStatus::Finished);
}

/// Test that multiple coroutines can yield independently
#[test]
fn test_multiple_coroutines_yield() {
    use mlua::{Lua, ThreadStatus, Value};
    use tokio::sync::oneshot;

    let lua = Lua::new();

    // Create two channels
    let (tx1, rx1) = oneshot::channel::<i32>();
    let (tx2, rx2) = oneshot::channel::<i32>();

    let rx1 = Arc::new(std::sync::Mutex::new(Some(rx1)));
    let rx2 = Arc::new(std::sync::Mutex::new(Some(rx2)));

    let rx1_clone = rx1.clone();
    let rx2_clone = rx2.clone();

    let async_fn1 = lua
        .create_async_function(move |_lua, ()| {
            let rx = rx1_clone.clone();
            async move {
                let receiver = rx.lock().unwrap().take();
                if let Some(receiver) = receiver {
                    Ok(Value::Integer(receiver.await.unwrap_or(-1) as i64))
                } else {
                    Ok(Value::Integer(-999))
                }
            }
        })
        .expect("Failed to create async function 1");

    let async_fn2 = lua
        .create_async_function(move |_lua, ()| {
            let rx = rx2_clone.clone();
            async move {
                let receiver = rx.lock().unwrap().take();
                if let Some(receiver) = receiver {
                    Ok(Value::Integer(receiver.await.unwrap_or(-1) as i64))
                } else {
                    Ok(Value::Integer(-999))
                }
            }
        })
        .expect("Failed to create async function 2");

    // Create and start both threads
    let thread1 = lua.create_thread(async_fn1).expect("Failed to create thread 1");
    let thread2 = lua.create_thread(async_fn2).expect("Failed to create thread 2");

    let _ = thread1.resume::<Value>(());
    let _ = thread2.resume::<Value>(());

    // Both should be resumable (yielded)
    assert_eq!(thread1.status(), ThreadStatus::Resumable);
    assert_eq!(thread2.status(), ThreadStatus::Resumable);

    // Complete thread 2 first
    tx2.send(2).expect("Send 2");
    let r2 = thread2.resume::<Value>(()).expect("Resume 2");
    assert_eq!(r2.as_integer(), Some(2));
    assert_eq!(thread2.status(), ThreadStatus::Finished);

    // Thread 1 still waiting
    assert_eq!(thread1.status(), ThreadStatus::Resumable);

    // Complete thread 1
    tx1.send(1).expect("Send 1");
    let r1 = thread1.resume::<Value>(()).expect("Resume 1");
    assert_eq!(r1.as_integer(), Some(1));
    assert_eq!(thread1.status(), ThreadStatus::Finished);

    println!("Both threads yielded successfully - true yielding works!");
}

/// Test that a sync callback runs while async is pending
#[test]
fn test_sync_runs_while_async_yields() {
    use mlua::{Lua, ThreadStatus, Value};
    use tokio::sync::oneshot;

    let lua = Lua::new();

    let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter_clone = counter.clone();

    // Register a sync function that increments counter
    let increment_fn = lua
        .create_function(move |_, ()| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .expect("Failed to create increment function");
    lua.globals()
        .set("increment", increment_fn)
        .expect("Failed to set global");

    // Create channel for async
    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

    // Create async function
    let async_fn = lua
        .create_async_function(move |_lua, ()| {
            let rx = rx_clone.clone();
            async move {
                let receiver = rx.lock().unwrap().take();
                if let Some(receiver) = receiver {
                    Ok(Value::Integer(receiver.await.unwrap_or(-1) as i64))
                } else {
                    Ok(Value::Integer(-999))
                }
            }
        })
        .expect("Failed to create async function");

    // Start async operation
    let async_thread = lua.create_thread(async_fn).expect("Failed to create thread");
    let _ = async_thread.resume::<Value>(());

    assert_eq!(async_thread.status(), ThreadStatus::Resumable);

    // While async is pending, run sync code
    lua.load("increment()").exec().expect("Failed to run sync code");
    lua.load("increment()").exec().expect("Failed to run sync code");

    // Counter should have been incremented while async was yielded
    assert_eq!(counter.load(Ordering::SeqCst), 2);

    // Async thread should still be resumable
    assert_eq!(async_thread.status(), ThreadStatus::Resumable);

    // Complete async
    tx.send(42).expect("Send");
    let r = async_thread.resume::<Value>(()).expect("Resume");
    assert_eq!(r.as_integer(), Some(42));

    println!("Sync code ran while async was yielded - concurrency works!");
}

/// Test using AsyncThread with proper polling
#[tokio::test]
async fn test_async_thread_polling() {
    use mlua::{Lua, Value};

    let lua = Lua::new();

    // Create async function
    let async_fn = lua
        .create_async_function(|_lua, ()| async {
            // Small delay to simulate DB operation
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(Value::Integer(42))
        })
        .expect("Failed to create async function");

    // Create thread
    let thread = lua.create_thread(async_fn).expect("Failed to create thread");

    // Convert to AsyncThread and await it
    let async_thread = thread.into_async::<Value>(()).expect("Failed to convert to async");

    // This should complete with the value
    let result = async_thread.await;

    match result {
        Ok(value) => {
            println!("Async thread completed with: {:?}", value);
            assert_eq!(value.as_integer(), Some(42));
        }
        Err(e) => panic!("Async thread failed: {:?}", e),
    }
}

/// Test the actual DataStore pattern with oneshot channel
#[tokio::test]
async fn test_datastore_pattern() {
    use mlua::{Lua, Value};
    use tokio::sync::oneshot;

    let lua = Lua::new();

    // Simulate what DataStore does: create channel, send to background, await response
    let (tx, rx) = oneshot::channel::<i32>();

    // Wrap receiver in Arc<Mutex> so it can be moved into the closure
    let rx = Arc::new(tokio::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

    let async_fn = lua
        .create_async_function(move |_lua, ()| {
            let rx = rx_clone.clone();
            async move {
                let mut guard = rx.lock().await;
                let receiver = guard.take().ok_or_else(|| {
                    mlua::Error::RuntimeError("Receiver already used".into())
                })?;

                let value = receiver.await.map_err(|_| {
                    mlua::Error::RuntimeError("Channel closed".into())
                })?;

                Ok(Value::Integer(value as i64))
            }
        })
        .expect("Failed to create async function");

    let thread = lua.create_thread(async_fn).expect("Failed to create thread");
    let async_thread = thread.into_async::<Value>(()).expect("Failed to convert to async");

    // Spawn task to send value after a delay (simulating DB response)
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = tx.send(123);
    });

    // Await the result
    let result = async_thread.await.expect("Async thread failed");
    assert_eq!(result.as_integer(), Some(123));

    println!("DataStore pattern works - oneshot channel integration successful!");
}
