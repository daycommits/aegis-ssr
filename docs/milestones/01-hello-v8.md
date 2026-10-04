# Milestone 01: Hello V8 & Isolate Harness

## 🎯 Goal
Set up the foundational architecture matching `ssr-platform`:
1. **`src/lib.rs`**: Only module declarations (`pub mod isolate_runner;`).
2. **`src/isolate_runner.rs`**: All V8 logic — platform initialization (`init_v8_once`), isolate creation, and script execution (`run_script`).
3. **`src/main.rs`**: The CLI runner that invokes `isolate_runner` and prints the output.

---

## 📁 File Structure (`ssr-platform` layout)
```text
aegis-ssr/
├── src/
│   ├── lib.rs              <-- 🌟 Only module exports (pub mod ...)
│   ├── isolate_runner.rs   <-- 🌟 All V8 code: init_v8_once & run_script
│   └── main.rs             <-- 🌟 CLI test runner
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. `src/lib.rs` is Only Module Declarations
In `ssr-platform`, **`src/lib.rs` contains zero logic**—it only exports modules:
```rust
pub mod isolate_runner;
```
This keeps the root clean and decouples module discovery from implementation details.

### 2. All V8 Logic Lives in `src/isolate_runner.rs`
In `ssr-platform`, V8 platform initialization (`init_v8_once`) and isolate management live together inside **`src/isolate_runner.rs`**.

---

## 📝 Implementation

### 1. Dependencies (`Cargo.toml`)
```toml
[package]
name = "aegis-ssr"
version = "0.1.0"
edition = "2024"

[dependencies]
v8 = "152.2.0"
```

---

### 2. Create `src/lib.rs`
`src/lib.rs` only declares public modules:

```rust
pub mod isolate_runner;
```

---

### 3. Create `src/isolate_runner.rs`
All V8 engine initialization and execution code lives here:

```rust
use std::sync::Once;

static V8_INIT: Once = Once::new();

/// Global one-time V8 platform initialization (matching ssr-platform)
pub fn init_v8_once() {
    V8_INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

/// The V8 execution harness
pub fn run_script(js_code: &str) -> Result<String, String> {
    // 1. Create a fresh Isolate
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());

    // 2. Pin the handle scope using the modern v8::scope! macro
    v8::scope!(let scope, isolate);

    // 3. Create execution context
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 4. Allocate JS string in V8's heap
    let code = match v8::String::new(scope, js_code) {
        Some(c) => c,
        None => return Err("Failed to allocate JS string: heap exhausted".to_string()),
    };

    // 5. Compile and run
    let script = match v8::Script::compile(scope, code, None) {
        Some(s) => s,
        None => return Err("Failed to compile JavaScript".to_string()),
    };

    match script.run(scope) {
        Some(result) => Ok(result.to_rust_string_lossy(scope)),
        None => Err("Script execution failed or terminated".to_string()),
    }
}
```

---

### 4. Create `src/main.rs`
```rust
use aegis_ssr::isolate_runner::{init_v8_once, run_script};

fn main() {
    // 1. Initialize V8 platform via isolate_runner
    init_v8_once();
    println!("✅ V8 platform initialized.");

    // 2. Real JavaScript computation
    let js_code = r#"
        const engine = "Aegis";
        const version = 1;
        const features = ["Memory Isolation", "Watchdogs", "Sub-millisecond Renders"];

        const summary = features.map((f, i) => `${i + 1}. ${f}`).join(" | ");

        `🛡️ [${engine} v${version}] Active features: ${summary} (Calculation: 10 * 42 = ${10 * 42})`
    "#;

    // 3. Run through isolate_runner harness
    match run_script(js_code) {
        Ok(output) => println!("🎉 JavaScript returned:\n{}", output),
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
✅ V8 platform initialized.
🎉 JavaScript returned:
🛡️ [Aegis v1] Active features: 1. Memory Isolation | 2. Watchdogs | 3. Sub-millisecond Renders (Calculation: 10 * 42 = 420)
```

---

## ✅ Checklist
- [ ] `src/lib.rs` contains only module declarations (`pub mod isolate_runner;`).
- [ ] `src/isolate_runner.rs` contains `init_v8_once()` and `run_script()`.
- [ ] Executed JavaScript from `src/main.rs` via `isolate_runner`.
