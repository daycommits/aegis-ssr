# Milestone 04: Guardrails & Limits (Timeout & Heap Guards)

## 🎯 Goal
Protect the host process from misbehaving JavaScript code by enforcing strict per-render execution timeouts (wall-clock watchdog) and memory ceilings (V8 heap limits).

---

## 💡 Concepts

### 1. The Threat of Untrusted / Faulty JS Code
In a multi-tenant platform:
- Team A might accidentally ship an infinite loop: `while (true) {}`.
- Team B might have a memory leak or allocate huge arrays: `new Array(1e8)`.
Without guards, one team can consume 100% CPU of the pod or trigger an OS Out-Of-Memory (OOM) kill that brings down the entire server for all teams!

### 2. Wall-Clock Watchdog (`terminate_execution`)
V8 allows terminating script execution from another thread via an `IsolateHandle`:
```rust
let handle = isolate.thread_safe_handle();
let watchdog = std::thread::spawn(move || {
    std::thread::sleep(Duration::from_millis(timeout_ms));
    handle.terminate_execution();
});
```
When `terminate_execution()` is called, V8 interrupts the running JavaScript bytecode and aborts execution.

### 3. V8 Heap Limits
We configure heap limits during isolate creation:
```rust
let mut params = v8::CreateParams::default();
params = params.heap_limits(0, max_heap_bytes); // e.g. 32MB
```
Additionally, `isolate.add_near_heap_limit_callback` intercepts V8 before an OOM occurs, allowing graceful isolate termination.

---

## 📝 Implementation

### Code (`src/main.rs`)
```rust
use std::time::Duration;

fn run_with_timeout(code_str: &str, timeout_ms: u64, max_heap_mb: usize) -> Result<String, String> {
    // 1. Configure heap limits
    let heap_limit_bytes = max_heap_mb * 1024 * 1024;
    let params = v8::CreateParams::default().heap_limits(0, heap_limit_bytes);
    let isolate = &mut v8::Isolate::new(params);

    // 2. Set up watchdog thread to enforce wall-clock timeout
    let handle = isolate.thread_safe_handle();
    let (done_tx, done_rx) = std::sync::mpsc::channel();

    let watchdog = std::thread::spawn(move || {
        if done_rx.recv_timeout(Duration::from_millis(timeout_ms)).is_err() {
            // Timed out! Force-kill V8 execution
            handle.terminate_execution();
        }
    });

    let result = {
        v8::scope!(let scope, isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);

        let code = v8::String::new(scope, code_str).unwrap();
        match v8::Script::compile(scope, code, None) {
            Some(script) => match script.run(scope) {
                Some(val) => Ok(val.to_rust_string_lossy(scope)),
                None => Err("Execution terminated (Timeout or Out-Of-Memory)".to_string()),
            },
            None => Err("Failed to compile script".to_string()),
        }
    };

    // Notify watchdog that execution finished
    let _ = done_tx.send(());
    let _ = watchdog.join();

    result
}

fn main() {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    println!("--- Test 1: Normal Code (Should Succeed) ---");
    let res = run_with_timeout("'Calculation: ' + (10 * 20)", 500, 32);
    println!("Result: {:?}", res);

    println!("\n--- Test 2: Infinite Loop (Should be killed by Watchdog) ---");
    let res = run_with_timeout("while(true) {}", 200, 32);
    println!("Result: {:?}", res);

    println!("\n--- Test 3: Memory Hog (Should be killed by Heap Guard) ---");
    let res = run_with_timeout("const arr = []; while(true) { arr.push(new Array(1000000)); }", 1000, 16);
    println!("Result: {:?}", res);
}
```

---

## 🔍 Verification
Run:
```bash
cargo run
```

Expected output:
```text
--- Test 1: Normal Code (Should Succeed) ---
Result: Ok("Calculation: 200")

--- Test 2: Infinite Loop (Should be killed by Watchdog) ---
Result: Err("Execution terminated (Timeout or Out-Of-Memory)")

--- Test 3: Memory Hog (Should be killed by Heap Guard) ---
Result: Err("Execution terminated (Timeout or Out-Of-Memory)")
```

Notice: Neither the infinite loop nor the memory exhaustion crashed the Rust process!

---

## ✅ Checklist
- [ ] Understand why thread-safe handles are necessary for timeouts.
- [ ] Observe V8 safely aborting runaway loops.
- [ ] Verify that V8 heap ceilings prevent host process crashes.
