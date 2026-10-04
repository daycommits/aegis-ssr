# Milestone 04: Guardrails & Limits (`src/isolate_runner.rs`)

## 🎯 Goal
Structure the execution engine into **`src/isolate_runner.rs`** (matching `ssr-platform`), protect the host process from misbehaving JavaScript code using execution timeouts (wall-clock watchdog) and memory ceilings (V8 heap limits), and expose a library interface in **`src/lib.rs`**.

---

## 📁 File Structure Introduced
```text
aegis-ssr/
├── src/
│   ├── lib.rs              <-- Library root exposing modules
│   ├── shims.rs            <-- Browser polyfills (Milestone 3)
│   ├── isolate_runner.rs   <-- 🌟 NEW: V8 execution harness with guardrails
│   └── main.rs             <-- Test harness running tenants
├── web-bundle/
│   └── dist/bundle.js
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. The Threat of Untrusted / Faulty JS Code
In a multi-tenant platform:
- Team A might accidentally ship an infinite loop: `while (true) {}`.
- Team B might have a memory leak: `new Array(1e8)`.
Without guards, one team can monopolize CPU cores or cause an OS Out-Of-Memory (OOM) kill that crashes the entire server for all tenants.

### 2. Wall-Clock Watchdog (`terminate_execution`)
V8 allows interrupting script execution from another thread using an `IsolateHandle`:
```rust
let handle = isolate.thread_safe_handle();
let watchdog = std::thread::spawn(move || {
    if done_rx.recv_timeout(timeout).is_err() {
        handle.terminate_execution(); // Kills runaway JS execution cleanly!
    }
});
```

### 3. V8 Heap Limits
We configure heap limits during isolate creation:
```rust
let heap_bytes = max_heap_mb * 1024 * 1024;
let params = v8::CreateParams::default().heap_limits(0, heap_bytes);
let isolate = &mut v8::Isolate::new(params);
```

---

## 📝 Implementation

### 1. Update `Cargo.toml`
Ensure `aegis-ssr` is configured as both a library and a binary:

```toml
[package]
name = "aegis-ssr"
version = "0.1.0"
edition = "2024"

[dependencies]
v8 = "152.2.0"
```

---

### 2. Update `src/lib.rs`
`src/lib.rs` only declares the modules:

```rust
pub mod bundle;
pub mod isolate_runner;
pub mod shims;
```

---

### 3. Create `src/isolate_runner.rs`
Create `src/isolate_runner.rs` matching `ssr-platform/src/isolate_runner.rs`:

```rust
use std::sync::{mpsc, Once};
use std::time::Duration;
use crate::shims::HOST_API_SHIM_PRELUDE;

static V8_INIT: Once = Once::new();

/// Global one-time V8 platform initialization
pub fn init_v8_once() {
    V8_INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

#[derive(Clone, Debug)]
pub struct TenantSpec {
    pub name: &'static str,
    pub script: String,
    pub max_heap_mb: usize,
    pub timeout: Duration,
}

#[derive(Debug)]
pub enum TenantOutcome {
    Success(String),
    Timeout,
    HeapLimitExceeded,
    CompileError,
}

pub fn run_tenant(spec: &TenantSpec) -> TenantOutcome {
    // 1. Enforce memory ceiling on isolate creation
    let heap_limit_bytes = spec.max_heap_mb * 1024 * 1024;
    let params = v8::CreateParams::default().heap_limits(0, heap_limit_bytes);
    let isolate = &mut v8::Isolate::new(params);

    // 2. Spawn wall-clock watchdog thread to enforce timeout
    let handle = isolate.thread_safe_handle();
    let (done_tx, done_rx) = mpsc::channel();
    let timeout = spec.timeout;

    let watchdog = std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err() {
            // Execution took too long: interrupt V8!
            handle.terminate_execution();
        }
    });

    let outcome = {
        v8::scope!(let scope, isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);

        // Run shims first
        let shim_code = v8::String::new(scope, HOST_API_SHIM_PRELUDE).unwrap();
        v8::Script::compile(scope, shim_code, None).unwrap().run(scope).unwrap();

        // Compile tenant script
        let code = match v8::String::new(scope, &spec.script) {
            Some(c) => c,
            None => return TenantOutcome::HeapLimitExceeded,
        };

        match v8::Script::compile(scope, code, None) {
            Some(script) => match script.run(scope) {
                Some(val) => TenantOutcome::Success(val.to_rust_string_lossy(scope)),
                None => {
                    // Script was interrupted by watchdog or heap limit!
                    TenantOutcome::Timeout
                }
            },
            None => TenantOutcome::CompileError,
        }
    };

    // Signal watchdog that execution finished
    let _ = done_tx.send(());
    let _ = watchdog.join();

    outcome
}
```

---

### 4. Update `src/main.rs`
Use `src/main.rs` to run test isolates with different behaviors:

```rust
use aegis_ssr::isolate_runner::{init_v8_once, run_tenant, TenantSpec};
use std::time::Duration;

fn main() {
    init_v8_once();

    println!("--- Test 1: Normal Tenant (Should Succeed) ---");
    let good_tenant = TenantSpec {
        name: "good-tenant",
        script: "'Tenant output: ' + (10 * 20)".to_string(),
        max_heap_mb: 32,
        timeout: Duration::from_millis(500),
    };
    println!("Result: {:?}", run_tenant(&good_tenant));

    println!("\n--- Test 2: Infinite Loop (Should be killed by Watchdog) ---");
    let loop_tenant = TenantSpec {
        name: "infinite-loop-tenant",
        script: "while(true) {}".to_string(),
        max_heap_mb: 32,
        timeout: Duration::from_millis(200),
    };
    println!("Result: {:?}", run_tenant(&loop_tenant));

    println!("\n--- Test 3: Memory Hog (Should be killed cleanly) ---");
    let oom_tenant = TenantSpec {
        name: "memory-hog-tenant",
        script: "const a = []; while(true) { a.push(new Array(1000000)); }".to_string(),
        max_heap_mb: 16,
        timeout: Duration::from_secs(1),
    };
    println!("Result: {:?}", run_tenant(&oom_tenant));
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
--- Test 1: Normal Tenant (Should Succeed) ---
Result: Success("Tenant output: 200")

--- Test 2: Infinite Loop (Should be killed by Watchdog) ---
Result: Timeout

--- Test 3: Memory Hog (Should be killed cleanly) ---
Result: Timeout
```

Notice: Neither the infinite loop nor the memory exhaustion crashed the host process!

---

## ✅ Checklist
- [ ] Created `src/lib.rs` and `src/isolate_runner.rs` matching `ssr-platform`.
- [ ] Implemented `TenantSpec` and `TenantOutcome`.
- [ ] Confirmed watchdog threads terminate runaway code without crashing the host process.
