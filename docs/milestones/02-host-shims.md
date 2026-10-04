# Milestone 02: Host Shims & Native Console (`src/shims.rs` & `src/isolate_runner.rs`)

## 🎯 Goal
Implement the host shims architecture matching `ssr-platform`:
1. **`src/lib.rs`**: Only module declarations (`pub mod isolate_runner; pub mod shims;`).
2. **`src/shims.rs`**: Define `HOST_API_SHIM_PRELUDE` for JavaScript-side polyfills.
3. **`src/isolate_runner.rs`**: Implement native `bind_console` and `console_log_callback` directly inside the isolate runner.
4. **`src/main.rs`**: Execute JavaScript that logs via `console.log` and verify the output bridges into Rust.

---

## 📁 File Structure (`ssr-platform` layout)
```text
aegis-ssr/
├── src/
│   ├── lib.rs              <-- 🌟 Only module exports (pub mod shims, isolate_runner)
│   ├── shims.rs            <-- 🌟 JS polyfills prelude
│   ├── isolate_runner.rs   <-- 🌟 All V8 logic: init_v8_once, bind_console, run_script
│   └── main.rs             <-- CLI test runner
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. `src/lib.rs` Stays Minimal
In `ssr-platform`, `src/lib.rs` does not contain logic—it simply exposes the modules:
```rust
pub mod isolate_runner;
pub mod shims;
```

### 2. All V8 Setup & Native Callbacks Live in `src/isolate_runner.rs`
Before executing any script, `isolate_runner` sets up the context, calls `bind_console(scope, context)`, and runs the `HOST_API_SHIM_PRELUDE`. `main.rs` never needs to know the internal V8 handle details.

---

## 📝 Implementation

### 1. Update `src/lib.rs`
Only declare the modules:

```rust
pub mod isolate_runner;
pub mod shims;
```

---

### 2. Create `src/shims.rs`
```rust
pub const HOST_API_SHIM_PRELUDE: &str = r#"
if (typeof queueMicrotask === "undefined") {
    globalThis.queueMicrotask = function (fn) { Promise.resolve().then(fn); };
}
if (typeof setTimeout === "undefined") {
    globalThis.setTimeout = function (fn) { queueMicrotask(fn); };
    globalThis.clearTimeout = function () {};
}
"#;
```

---

### 3. Update `src/isolate_runner.rs`
Add `bind_console` and `console_log_callback` (matching `ssr-platform/src/isolate_runner.rs#L662-L680`):

```rust
use std::sync::Once;
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

/// Native Rust callback invoked whenever JS runs `console.log(...)`
fn console_log_callback(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue<v8::Value>,
) {
    let parts: Vec<String> = (0..args.length())
        .map(|i| args.get(i).to_rust_string_lossy(scope))
        .collect();

    println!("🦀 [Rust console.log proxy] {}", parts.join(" "));
}

/// Injects `console.log` onto JavaScript's globalThis
fn bind_console(scope: &mut v8::PinScope, context: v8::Local<v8::Context>) {
    let global = context.global(scope);
    let console = v8::Object::new(scope);

    let log_key = v8::String::new(scope, "log").unwrap();
    let log_fn = v8::Function::new(scope, console_log_callback).unwrap();
    console.set(scope, log_key.into(), log_fn.into());

    let console_key = v8::String::new(scope, "console").unwrap();
    global.set(scope, console_key.into(), console.into());
}

pub fn run_script(js_code: &str) -> Result<String, String> {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 1. Install native Rust host bindings
    bind_console(scope, context);

    // 2. Run pure JavaScript shims prelude
    let shim_code = v8::String::new(scope, HOST_API_SHIM_PRELUDE).unwrap();
    v8::Script::compile(scope, shim_code, None).unwrap().run(scope).unwrap();

    // 3. Compile and execute user script
    let code = match v8::String::new(scope, js_code) {
        Some(c) => c,
        None => return Err("Failed to allocate JS string".to_string()),
    };

    let script = match v8::Script::compile(scope, code, None) {
        Some(s) => s,
        None => return Err("Failed to compile JavaScript".to_string()),
    };

    match script.run(scope) {
        Some(result) => Ok(result.to_rust_string_lossy(scope)),
        None => Err("Script execution failed".to_string()),
    }
}
```

---

### 4. Update `src/main.rs`
```rust
use aegis_ssr::isolate_runner::{init_v8_once, run_script};

fn main() {
    init_v8_once();

    let js_code = r#"
        console.log("Hello from inside JavaScript!", 42, true);
        console.log("Structured log:", { user: "Ashu", status: "online" });

        const engine = "Aegis";
        `Execution finished by ${engine}`
    "#;

    match run_script(js_code) {
        Ok(output) => println!("🎉 Script return value: {}", output),
        Err(err) => eprintln!("❌ Error: {}", err),
    }
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
🦀 [Rust console.log proxy] Hello from inside JavaScript! 42 true
🦀 [Rust console.log proxy] Structured log: [object Object]
🎉 Script return value: Execution finished by Aegis
```

---

## ✅ Checklist
- [ ] `src/lib.rs` contains only module declarations (`pub mod shims; pub mod isolate_runner;`).
- [ ] `src/isolate_runner.rs` contains all V8 code: `init_v8_once`, `bind_console`, `console_log_callback`, and `run_script`.
- [ ] `src/main.rs` runs without touching low-level V8 scopes directly.
