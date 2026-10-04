# Milestone 01: Hello V8

## 🎯 Goal
Initialize the Google V8 engine platform inside Rust, create an independent Isolate, evaluate real JavaScript computation (variables, arrays, arrow functions, and template literals), and read the computed result back into Rust.

---

## 💡 Concepts

### 1. V8 Platform vs. Isolate
- **Platform (`v8::V8::initialize_platform`)**: The global runtime engine. Handles background thread pools, CPU feature discovery, and platform timers. Initialized **once** per process.
- **Isolate (`v8::Isolate`)**: An entirely self-contained instance of V8. It has its own private heap memory, call stack, and garbage collector. Different isolates can run in parallel on different OS threads without lock contention.

### 2. Pinned Scopes (`v8::scope!`)
In modern V8 (v140+ / v152+), scopes cannot simply be moved or aliased across the stack because the V8 garbage collector tracks handle addresses. V8 uses Rust's `Pin` mechanism via the `v8::scope!(let scope, isolate);` macro to guarantee memory safety.

### 3. Context (`v8::Context`)
A context represents the global JavaScript environment (e.g. `globalThis`, built-in constructors like `Object`, `Array`, `Promise`). An isolate can have multiple contexts, but each script executes within a specific context.

---

## 📝 Implementation

### Dependencies (`Cargo.toml`)
```toml
[package]
name = "aegis-ssr"
version = "0.1.0"
edition = "2024"

[dependencies]
v8 = "152.2.0"
```

### Code (`src/main.rs`)
```rust
fn main() {
    // 1. Initialize the global V8 platform once per process
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    println!("✅ V8 platform initialized.");

    // 2. Create a new Isolate
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());

    // 3. Create a pinned HandleScope using the modern v8::scope! macro
    v8::scope!(let scope, isolate);

    // 4. Create a Context (global execution environment)
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 5. Write real JavaScript code with computation, arrays, and template strings
    let js_code = r#"
        const engine = "Aegis";
        const version = 1;
        const features = ["Memory Isolation", "Watchdogs", "Sub-millisecond Renders"];

        // Compute a summary using arrow functions and array methods
        const summary = features.map((f, i) => `${i + 1}. ${f}`).join(" | ");

        `🛡️ [${engine} v${version}] Active features: ${summary} (Calculation: 10 * 42 = ${10 * 42})`
    "#;

    let code = v8::String::new(scope, js_code).unwrap();

    // 6. Compile the script
    let script = v8::Script::compile(scope, code, None)
        .expect("Failed to compile JavaScript");

    // 7. Execute the script and get the evaluated result
    let result = script.run(scope).expect("Failed to execute script");

    // 8. Convert the V8 string value back into a Rust String
    let result_str = result.to_rust_string_lossy(scope);

    println!("🎉 JavaScript returned:\n{}", result_str);
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
- [x] V8 platform initializes without panic.
- [x] Isolate creates its own private heap.
- [x] `v8::scope!` safely pins the scope to the stack.
- [x] Real JavaScript computation (mapping, array manipulation, template strings, math) executes and returns to Rust.
