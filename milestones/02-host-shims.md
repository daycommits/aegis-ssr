# Milestone 02: Host Shims & JS Globals

## 🎯 Goal
Understand why V8 has no built-in I/O APIs, implement a Rust callback function (`console_log_callback`), and expose it to JavaScript on `globalThis.console.log`.

---

## 💡 Concepts

### 1. V8 is a Pure Language Sandbox
Unlike Node.js or browsers, the raw V8 engine does not provide:
- `console`
- `fetch` / `XMLHttpRequest`
- `setTimeout` / `setInterval`
- `process` / `fs`

If a JavaScript bundle runs `console.log("hello")` without host bindings, V8 crashes immediately with:
`ReferenceError: console is not defined`

### 2. Rust Function Callbacks in V8
To provide APIs to JavaScript, the host (Rust) registers callback functions with signature:
```rust
fn callback(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue<v8::Value>,
) {
    // Read JS arguments via args.get(i)
    // Return values to JS via rv.set(...)
}
```

### 3. V8 Object Hierarchy
To build `console.log`:
1. Retrieve the context's root global: `context.global(scope)` (equivalent to `globalThis`).
2. Create an empty JavaScript object: `v8::Object::new(scope)`.
3. Create a callable function: `v8::Function::new(scope, console_log_callback)`.
4. Attach `log` property to `console_obj`.
5. Attach `console` property to `global`.

---

## 📝 Implementation

### Code (`src/main.rs`)
```rust
// Rust callback invoked whenever JavaScript calls `console.log(...)`
fn console_log_callback(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue<v8::Value>,
) {
    let mut parts = Vec::new();
    for i in 0..args.length() {
        let arg = args.get(i);
        parts.push(arg.to_rust_string_lossy(scope));
    }
    println!("🦀 [Rust console.log] {}", parts.join(" "));
}

fn main() {
    // 1. Initialize the global V8 platform
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    // 2. Create Isolate and enter Context
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 3. Inject console.log into the global scope
    let global = context.global(scope);

    let console_obj = v8::Object::new(scope);
    let log_fn = v8::Function::new(scope, console_log_callback).unwrap();

    let log_key = v8::String::new(scope, "log").unwrap();
    console_obj.set(scope, log_key.into(), log_fn.into());

    let console_key = v8::String::new(scope, "console").unwrap();
    global.set(scope, console_key.into(), console_obj.into());

    // 4. Execute JavaScript calling our new console.log API
    let js_code = r#"
        console.log("Hello from inside JavaScript!", 42, true);
        console.log("Calling host shims from JS is seamless.");

        const engine = "Aegis";
        `Rendered by ${engine} Engine`
    "#;

    let code = v8::String::new(scope, js_code).unwrap();
    let script = v8::Script::compile(scope, code, None)
        .expect("Failed to compile JavaScript");

    let result = script.run(scope).expect("Failed to execute script");
    let result_str = result.to_rust_string_lossy(scope);

    println!("🎉 Script return value: {}", result_str);
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
🦀 [Rust console.log] Hello from inside JavaScript! 42 true
🦀 [Rust console.log] Calling host shims from JS is seamless.
🎉 Script return value: Rendered by Aegis Engine
```

---

## ✅ Checklist
- [ ] Understand why bare V8 lacks standard Web / Node.js APIs.
- [ ] Successfully bridge JavaScript calls to a native Rust function.
- [ ] Understand how arguments pass across the JS-Rust boundary.
