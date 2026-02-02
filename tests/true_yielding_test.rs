//! Test true yielding behavior for DataStore operations.
//!
//! This test verifies that:
//! 1. When a Lua coroutine calls an async operation, it yields
//! 2. Other coroutines can run while the first is waiting
//! 3. The yielded coroutine resumes when the operation completes

use mlua::{Lua, ThreadStatus, Value};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Test that an async function properly yields and can be polled to completion.
/// This mimics what DataStore:GetAsync should do.
#[test]
fn test_async_yields_and_resumes() {
    let lua = Lua::new();

    // Create channel to simulate DB operation
    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

    // Create async function that awaits the channel
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

    // Create thread
    let thread = lua
        .create_thread(async_fn)
        .expect("Failed to create thread");

    // First resume - should yield (return PENDING)
    let result1 = thread.resume::<Value>(()).expect("First resume failed");
    println!("First resume: {:?}", result1);

    // Should be PENDING (LightUserData)
    assert!(
        matches!(result1, Value::LightUserData(_)),
        "Expected PENDING, got {:?}",
        result1
    );
    assert_eq!(
        thread.status(),
        ThreadStatus::Resumable,
        "Thread should be resumable"
    );

    // Simulate DB completion
    tx.send(42).expect("Failed to send");

    // Resume again - should complete
    let result2 = thread.resume::<Value>(()).expect("Second resume failed");
    println!("Second resume: {:?}", result2);

    assert_eq!(result2.as_integer(), Some(42));
    assert_eq!(thread.status(), ThreadStatus::Finished);
}

/// Test that multiple coroutines can yield independently.
/// This is the core of true yielding - coroutine A yields, coroutine B runs.
#[test]
fn test_multiple_coroutines_yield_independently() {
    let lua = Lua::new();

    // Create two channels for two async operations
    let (tx1, rx1) = oneshot::channel::<i32>();
    let (tx2, rx2) = oneshot::channel::<i32>();

    let rx1 = Arc::new(std::sync::Mutex::new(Some(rx1)));
    let rx2 = Arc::new(std::sync::Mutex::new(Some(rx2)));

    let rx1_clone = rx1.clone();
    let rx2_clone = rx2.clone();

    // Async function 1
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

    // Async function 2
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

    // Create both threads
    let thread1 = lua.create_thread(async_fn1).expect("Thread 1");
    let thread2 = lua.create_thread(async_fn2).expect("Thread 2");

    // Start both - both should yield
    let r1 = thread1.resume::<Value>(()).expect("Resume 1");
    let r2 = thread2.resume::<Value>(()).expect("Resume 2");

    println!("Thread 1 status: {:?}", thread1.status());
    println!("Thread 2 status: {:?}", thread2.status());

    assert!(matches!(r1, Value::LightUserData(_)), "Thread 1 should yield");
    assert!(matches!(r2, Value::LightUserData(_)), "Thread 2 should yield");
    assert_eq!(thread1.status(), ThreadStatus::Resumable);
    assert_eq!(thread2.status(), ThreadStatus::Resumable);

    // Complete thread 2 first (out of order)
    tx2.send(200).expect("Send to thread 2");

    // Poll thread 2 - should complete
    let r2_final = thread2.resume::<Value>(()).expect("Resume 2 final");
    assert_eq!(r2_final.as_integer(), Some(200));
    assert_eq!(thread2.status(), ThreadStatus::Finished);

    // Thread 1 should still be waiting
    assert_eq!(thread1.status(), ThreadStatus::Resumable);

    // Now complete thread 1
    tx1.send(100).expect("Send to thread 1");
    let r1_final = thread1.resume::<Value>(()).expect("Resume 1 final");
    assert_eq!(r1_final.as_integer(), Some(100));
    assert_eq!(thread1.status(), ThreadStatus::Finished);

    println!("Both coroutines completed independently - true yielding works!");
}

/// Test that a sync callback can run while an async coroutine is yielded.
/// This is what we want in the game loop - Heartbeat callbacks continue
/// while one callback is waiting for DataStore.
#[test]
fn test_sync_runs_while_async_waits() {
    let lua = Lua::new();

    let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter_clone = counter.clone();

    // Sync function that increments counter
    let sync_fn = lua
        .create_function(move |_, ()| {
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .expect("Sync fn");
    lua.globals().set("increment", sync_fn).expect("Set global");

    // Async function that will yield
    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

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
        .expect("Async fn");

    // Start async operation
    let async_thread = lua.create_thread(async_fn).expect("Thread");
    let _ = async_thread.resume::<Value>(());

    // Async should be yielded
    assert_eq!(async_thread.status(), ThreadStatus::Resumable);

    // Run sync code multiple times while async is waiting
    for _ in 0..5 {
        lua.load("increment()").exec().expect("Run sync");
    }

    // Counter should have been incremented
    assert_eq!(
        counter.load(std::sync::atomic::Ordering::SeqCst),
        5,
        "Sync code should run while async yields"
    );

    // Async still waiting
    assert_eq!(async_thread.status(), ThreadStatus::Resumable);

    // Complete async
    tx.send(42).expect("Send");
    let result = async_thread.resume::<Value>(()).expect("Final resume");
    assert_eq!(result.as_integer(), Some(42));

    println!("Sync code ran while async was yielded!");
}

/// Test a Lua callback that calls an async function properly yields the callback's coroutine.
#[test]
fn test_lua_callback_yields_on_async() {
    let lua = Lua::new();

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
        .expect("Async fn");

    lua.globals().set("get_value_async", async_fn).expect("Set");

    // Create a Lua function that calls the async function
    let callback = lua
        .load(
            r#"
        function()
            local result = get_value_async()
            return result + 10
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("Create callback");

    // Run callback as a coroutine
    let thread = lua.create_thread(callback).expect("Thread");

    // First resume - should start callback, which calls async, which yields
    let result1 = thread.resume::<Value>(()).expect("First resume");
    println!("Callback first resume: {:?}, status: {:?}", result1, thread.status());

    // Should be yielded (PENDING)
    assert!(
        matches!(result1, Value::LightUserData(_)),
        "Callback should yield when calling async"
    );
    assert_eq!(thread.status(), ThreadStatus::Resumable);

    // Complete the async operation
    tx.send(100).expect("Send");

    // Resume callback - should complete with result + 10 = 110
    let result2 = thread.resume::<Value>(()).expect("Second resume");
    println!("Callback final result: {:?}", result2);

    assert_eq!(result2.as_integer(), Some(110));
    assert_eq!(thread.status(), ThreadStatus::Finished);

    println!("Lua callback properly yielded on async call!");
}

/// Test simulating the game loop pattern: fire events as coroutines, poll async operations
#[test]
fn test_game_loop_pattern() {
    let lua = Lua::new();

    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

    // Async "GetAsync" function
    let get_async = lua
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
        .expect("Async fn");

    lua.globals().set("GetAsync", get_async).expect("Set");

    // Track results
    lua.load("_G.results = {}").exec().expect("Init");

    // Two callbacks - one calls async, one is sync
    let callback_with_async = lua
        .load(
            r#"
        function()
            local data = GetAsync()
            table.insert(_G.results, "async_done:" .. tostring(data))
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("Async callback");

    let callback_sync = lua
        .load(
            r#"
        function()
            table.insert(_G.results, "sync_ran")
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("Sync callback");

    // Simulate: game loop fires both as coroutines
    let thread1 = lua.create_thread(callback_with_async).expect("T1");
    let thread2 = lua.create_thread(callback_sync).expect("T2");

    // Fire both
    let _ = thread1.resume::<()>(());
    let _ = thread2.resume::<()>(());

    // Thread 1 should be yielded (waiting for async)
    // Thread 2 should be finished (sync completed)
    println!("Thread 1 status: {:?}", thread1.status());
    println!("Thread 2 status: {:?}", thread2.status());

    assert_eq!(
        thread1.status(),
        ThreadStatus::Resumable,
        "Async callback should yield"
    );
    assert_eq!(
        thread2.status(),
        ThreadStatus::Finished,
        "Sync callback should finish"
    );

    // Check results - sync should have run
    let results: Vec<String> = lua
        .load("return _G.results")
        .eval::<mlua::Table>()
        .expect("Get results")
        .sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect();

    println!("Results after first tick: {:?}", results);
    assert!(results.contains(&"sync_ran".to_string()));
    assert!(!results.iter().any(|r| r.starts_with("async_done")));

    // Simulate: async operation completes (e.g., DB returned)
    tx.send(42).expect("DB response");

    // Game loop: poll pending threads
    if thread1.status() == ThreadStatus::Resumable {
        let _ = thread1.resume::<()>(());
    }

    // Now async callback should have completed
    assert_eq!(thread1.status(), ThreadStatus::Finished);

    let results: Vec<String> = lua
        .load("return _G.results")
        .eval::<mlua::Table>()
        .expect("Get results")
        .sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect();

    println!("Results after second tick: {:?}", results);
    assert!(results.contains(&"async_done:42".to_string()));

    println!("Game loop pattern works - true yielding achieved!");
}
