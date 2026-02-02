//! Test to understand mlua async function behavior in different contexts

use mlua::{Lua, ThreadStatus, Value};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Test what happens when we call an async function directly (not via resume)
#[test]
fn test_direct_async_call() {
    let lua = Lua::new();

    // Create a simple async function
    let async_fn = lua
        .create_async_function(|_lua, ()| async {
            // This should yield
            Ok(Value::Integer(42))
        })
        .expect("Failed to create async function");

    // Call it directly as a function
    let result = async_fn.call::<Value>(());
    println!("Direct call result: {:?}", result);
}

/// Test what resume returns when async function yields
#[test]
fn test_resume_async_function() {
    let lua = Lua::new();

    // Create channel to control when the async completes
    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

    let async_fn = lua
        .create_async_function(move |_lua, ()| {
            let rx = rx_clone.clone();
            async move {
                // Take the receiver (only works once)
                let receiver = {
                    let mut guard = rx.lock().unwrap();
                    guard.take()
                };

                if let Some(receiver) = receiver {
                    let val = receiver.await.unwrap_or(-1);
                    Ok(Value::Integer(val as i64))
                } else {
                    Ok(Value::Integer(-999))
                }
            }
        })
        .expect("Failed to create async function");

    // Create a thread and resume it
    let thread = lua.create_thread(async_fn).expect("Failed to create thread");

    println!("Thread status before resume: {:?}", thread.status());

    // First resume - should start the coroutine
    let result1 = thread.resume::<Value>(());
    println!("First resume result: {:?}", result1);
    println!("Thread status after first resume: {:?}", thread.status());

    // Send a value on the channel
    let _ = tx.send(42);

    // Try to resume again
    let result2 = thread.resume::<Value>(());
    println!("Second resume result: {:?}", result2);
    println!("Thread status after second resume: {:?}", thread.status());
}

/// Test with immediate completion (no actual async work)
#[test]
fn test_resume_immediate_async() {
    let lua = Lua::new();

    let async_fn = lua
        .create_async_function(|_lua, ()| async {
            // No await, completes immediately
            Ok(Value::Integer(42))
        })
        .expect("Failed to create async function");

    let thread = lua.create_thread(async_fn).expect("Failed to create thread");

    println!("Thread status before: {:?}", thread.status());

    let result = thread.resume::<Value>(());
    println!("Resume result: {:?}", result);
    println!("Thread status after: {:?}", thread.status());

    // Should be finished since no async work
    assert_eq!(thread.status(), ThreadStatus::Finished);
}

/// Test calling async function from within a Lua coroutine
#[test]
fn test_async_in_lua_coroutine() {
    let lua = Lua::new();

    let async_fn = lua
        .create_async_function(|_lua, ()| async {
            Ok(Value::Integer(42))
        })
        .expect("Failed to create async function");

    lua.globals().set("async_fn", async_fn).expect("Failed to set global");

    // Create a Lua coroutine that calls our async function
    let lua_code = r#"
        local co = coroutine.create(function()
            local result = async_fn()
            return result
        end)

        local status, result = coroutine.resume(co)
        return status, result, coroutine.status(co)
    "#;

    let result: (bool, Value, String) = lua.load(lua_code).eval().expect("Failed to run Lua");
    println!("Lua coroutine result: status={}, result={:?}, co_status={}", result.0, result.1, result.2);
}

/// Test the nested coroutine pattern (what our DataStore does)
#[test]
fn test_nested_thread_pattern() {
    let lua = Lua::new();

    // This mimics what DataStore:GetAsync does
    let get_async_impl = lua
        .create_function(|lua, ()| {
            // Create an async function internally
            let async_fn = lua.create_async_function(|_lua, ()| async {
                Ok(Value::Integer(42))
            })?;

            // Create a thread and resume it
            let thread = lua.create_thread(async_fn)?;
            let result = thread.resume::<Value>(())?;

            // Return whatever resume gave us
            Ok(result)
        })
        .expect("Failed to create function");

    lua.globals().set("get_async", get_async_impl).expect("Failed to set global");

    // Call it from Lua
    let result: Value = lua.load("return get_async()").eval().expect("Failed to run");
    println!("Nested pattern result: {:?}", result);
}

/// Test: What does the PENDING value look like? Can we detect it?
#[test]
fn test_detect_pending_value() {
    let lua = Lua::new();

    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

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

    let thread = lua.create_thread(async_fn).expect("Failed to create thread");

    // First resume - should return PENDING
    let result = thread.resume::<Value>(()).expect("Resume failed");

    println!("PENDING value type: {:?}", result);

    // Check if it's LightUserData (that's what PENDING is)
    let is_pending = matches!(result, Value::LightUserData(_));
    println!("Is LightUserData (PENDING): {}", is_pending);

    // The thread should still be resumable
    assert_eq!(thread.status(), ThreadStatus::Resumable);

    // Now send the value
    let _ = tx.send(42);

    // Resume again - should get the actual value
    let result2 = thread.resume::<Value>(()).expect("Second resume failed");
    println!("After send, result: {:?}", result2);

    assert_eq!(result2.as_integer(), Some(42));
    assert_eq!(thread.status(), ThreadStatus::Finished);
}

/// Test: Can we poll a thread repeatedly until it completes?
#[test]
fn test_polling_until_complete() {
    use std::thread::sleep;
    use std::time::Duration;

    let lua = Lua::new();

    let (tx, rx) = oneshot::channel::<i32>();
    let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
    let rx_clone = rx.clone();

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

    let thread = lua.create_thread(async_fn).expect("Failed to create thread");

    // Start the async operation
    let _ = thread.resume::<Value>(());

    // Spawn a thread to send the value after a delay
    std::thread::spawn(move || {
        sleep(Duration::from_millis(50));
        let _ = tx.send(123);
    });

    // Poll until complete (simulating game loop)
    let mut polls = 0;
    let mut final_result = None;

    while thread.status() == ThreadStatus::Resumable && polls < 100 {
        sleep(Duration::from_millis(10));

        // Try to resume
        match thread.resume::<Value>(()) {
            Ok(value) => {
                if !matches!(value, Value::LightUserData(_)) {
                    // Got actual result
                    final_result = Some(value);
                    break;
                }
            }
            Err(e) => {
                println!("Resume error: {:?}", e);
            }
        }
        polls += 1;
    }

    println!("Completed after {} polls", polls);
    println!("Final result: {:?}", final_result);
    println!("Thread status: {:?}", thread.status());

    assert!(final_result.is_some());
    assert_eq!(final_result.unwrap().as_integer(), Some(123));
}
