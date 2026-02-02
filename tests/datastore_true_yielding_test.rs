//! Integration test for DataStore with true yielding behavior.
//!
//! This test verifies that DataStore operations properly yield and allow
//! other code to run while waiting for the database.

use mlua::{Lua, ThreadStatus, Value};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Simulates the DataStore pattern: async function that awaits a channel.
/// This is what DataStore:GetAsync does internally.
fn create_mock_get_async(lua: &Lua, rx: Arc<std::sync::Mutex<Option<oneshot::Receiver<i32>>>>) -> mlua::Function {
    let rx_clone = rx.clone();
    lua.create_async_function(move |_lua, _key: String| {
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
    .expect("Failed to create mock GetAsync")
}

/// Test that DataStore-style async methods yield properly when called from a coroutine.
#[test]
fn test_datastore_yields_in_heartbeat_callback() {
    let lua = Lua::new();

    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));

    // Create mock DataStore-like GetAsync
    let get_async = create_mock_get_async(&lua, rx);
    lua.globals().set("MockGetAsync", get_async).expect("Set global");

    // Track what happened
    lua.load("_G.events = {}").exec().expect("Init");

    // Create a callback that simulates a Heartbeat callback calling DataStore:GetAsync
    let heartbeat_callback = lua
        .load(
            r#"
        function()
            table.insert(_G.events, "callback_start")
            local data = MockGetAsync("player_data")
            table.insert(_G.events, "got_data:" .. tostring(data))
            return data
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("Create callback");

    // Simulate: fire_as_coroutines creates a thread for the callback
    let thread = lua.create_thread(heartbeat_callback).expect("Thread");

    // First tick: start the callback
    let _ = thread.resume::<()>(());

    // Callback should have started but yielded at MockGetAsync
    assert_eq!(
        thread.status(),
        ThreadStatus::Resumable,
        "Callback should yield while waiting for DataStore"
    );

    let events: Vec<String> = lua
        .load("return _G.events")
        .eval::<mlua::Table>()
        .unwrap()
        .sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect();

    println!("Events after first tick: {:?}", events);
    assert!(events.contains(&"callback_start".to_string()));
    assert!(!events.iter().any(|e| e.starts_with("got_data")));

    // Simulate: database returns data
    tx.send(42).expect("DB response");

    // Second tick: resume the callback
    let _ = thread.resume::<()>(());

    // Callback should have completed
    assert_eq!(thread.status(), ThreadStatus::Finished);

    let events: Vec<String> = lua
        .load("return _G.events")
        .eval::<mlua::Table>()
        .unwrap()
        .sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect();

    println!("Events after second tick: {:?}", events);
    assert!(events.contains(&"got_data:42".to_string()));

    println!("DataStore-style callback yielded and resumed correctly!");
}

/// Test that multiple Heartbeat callbacks can run while one is waiting for DataStore.
#[test]
fn test_multiple_callbacks_with_datastore() {
    let lua = Lua::new();

    let (tx1, rx1) = oneshot::channel::<i32>();
    let (tx2, rx2) = oneshot::channel::<i32>();

    let rx1 = Arc::new(std::sync::Mutex::new(Some(rx1)));
    let rx2 = Arc::new(std::sync::Mutex::new(Some(rx2)));

    // Create two mock DataStore functions (simulating two different DataStore calls)
    let get_async1 = create_mock_get_async(&lua, rx1);
    let get_async2 = create_mock_get_async(&lua, rx2);

    lua.globals().set("GetData1", get_async1).expect("Set");
    lua.globals().set("GetData2", get_async2).expect("Set");

    lua.load("_G.order = {}").exec().expect("Init");

    // Two callbacks - both call async functions
    let callback1 = lua
        .load(
            r#"
        function()
            table.insert(_G.order, "cb1_start")
            local data = GetData1("key1")
            table.insert(_G.order, "cb1_done:" .. data)
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("CB1");

    let callback2 = lua
        .load(
            r#"
        function()
            table.insert(_G.order, "cb2_start")
            local data = GetData2("key2")
            table.insert(_G.order, "cb2_done:" .. data)
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("CB2");

    // Simulate game loop: fire both callbacks as coroutines
    let thread1 = lua.create_thread(callback1).expect("T1");
    let thread2 = lua.create_thread(callback2).expect("T2");

    // First tick: both start
    let _ = thread1.resume::<()>(());
    let _ = thread2.resume::<()>(());

    // Both should be yielded
    assert_eq!(thread1.status(), ThreadStatus::Resumable);
    assert_eq!(thread2.status(), ThreadStatus::Resumable);

    let order: Vec<String> = lua
        .load("return _G.order")
        .eval::<mlua::Table>()
        .unwrap()
        .sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect();

    println!("Order after first tick: {:?}", order);
    assert!(order.contains(&"cb1_start".to_string()));
    assert!(order.contains(&"cb2_start".to_string()));

    // Complete callback 2 first (out of order)
    tx2.send(200).expect("Send 2");

    // Second tick: poll both
    // Only thread2 should complete
    let _ = thread1.resume::<()>(());  // Still waiting
    let _ = thread2.resume::<()>(());  // Completes

    assert_eq!(thread1.status(), ThreadStatus::Resumable, "Thread 1 still waiting");
    assert_eq!(thread2.status(), ThreadStatus::Finished, "Thread 2 done");

    // Complete callback 1
    tx1.send(100).expect("Send 1");
    let _ = thread1.resume::<()>(());

    assert_eq!(thread1.status(), ThreadStatus::Finished, "Thread 1 done");

    let order: Vec<String> = lua
        .load("return _G.order")
        .eval::<mlua::Table>()
        .unwrap()
        .sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect();

    println!("Final order: {:?}", order);

    // Verify out-of-order completion
    let cb2_done_pos = order.iter().position(|e| e == "cb2_done:200").unwrap();
    let cb1_done_pos = order.iter().position(|e| e == "cb1_done:100").unwrap();
    assert!(cb2_done_pos < cb1_done_pos, "Callback 2 should complete before callback 1");

    println!("Multiple callbacks with DataStore work correctly!");
}

/// Test that sync code runs while DataStore callback is yielded (simulating game physics, etc.)
#[test]
fn test_sync_physics_while_datastore_waits() {
    let lua = Lua::new();

    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));

    let get_async = create_mock_get_async(&lua, rx);
    lua.globals().set("LoadPlayerData", get_async).expect("Set");

    lua.load("_G.physics_ticks = 0; _G.player_data = nil").exec().expect("Init");

    // Callback that loads player data
    let load_callback = lua
        .load(
            r#"
        function()
            _G.player_data = LoadPlayerData("player1")
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("CB");

    // Sync function that simulates physics tick
    let physics_tick = lua
        .load(
            r#"
        function()
            _G.physics_ticks = _G.physics_ticks + 1
        end
    "#,
        )
        .eval::<mlua::Function>()
        .expect("Physics");

    // Start the async load
    let load_thread = lua.create_thread(load_callback).expect("Thread");
    let _ = load_thread.resume::<()>(());

    assert_eq!(load_thread.status(), ThreadStatus::Resumable);

    // Simulate several game ticks - physics should continue
    for _ in 0..10 {
        physics_tick.call::<()>(()).expect("Physics tick");
    }

    // Check physics ran while datastore was waiting
    let physics_ticks: i32 = lua.load("return _G.physics_ticks").eval().expect("Get");
    let player_data: Value = lua.load("return _G.player_data").eval().expect("Get");

    assert_eq!(physics_ticks, 10, "Physics should run while DataStore waits");
    assert!(matches!(player_data, Value::Nil), "Player data shouldn't be loaded yet");

    // Complete the DataStore operation
    tx.send(9999).expect("DB");
    let _ = load_thread.resume::<()>(());

    // Now data should be loaded
    let player_data: i64 = lua.load("return _G.player_data").eval().expect("Get");
    assert_eq!(player_data, 9999);

    println!("Physics continued while DataStore waited - true async behavior!");
}
