# Learnings

## Hello V8

### Step 1 V8 Platform
The global runtime engine.
1. Create new instance of the default v8::Platform.
    - `let platform = v8::new_default_platform(0, false).make_shared();`
    - arg thread_pool_size: u32 = 0
    - idle_task_support: bool = false
2. Sets the v8::Platform to use. `v8::V8::initialize_platform(platform);`
3. Initializes V8. `v8::V8::initialize();`

### Step 2 Isolate
An entirely self-contained instance of V8
- Create a new Isolate. `let isolate = &mut v8::Isolate::new(v8::CreateParams::default());`
### Scope
Inside the Google V8 engine (written in C++), V8's garbage collector maintains a linked list of HandleScopes on the thread's stack.
1. Create a pinned HandleScope.
    - `v8::scope!(let scope, isolate);`
    - To stop Rust from moving V8's internal C++ GC handles on the stack

### Step 3 Context
A context represents the global JavaScript environment (e.g. globalThis, built-in constructors like Object, Array, Promise). An isolate can have multiple contexts, but each script executes within a specific context.
1. Create a new context. `let context = v8::Context::new(scope, Default::default());`
2. Set the execution context for all operations executed within a local scope.
    `let scope = &mut v8::ContextScope::new(scope, context);`

### Step 4 Compile and execute
1. Create js code string
```
let js_code = r#"
    const engine = "Aegis";
    const version = 1;
    const features = ["Memory Isolation", "Watchdogs", "Sub-millisecond Renders"];

    // Compute a summary using arrow functions and array methods
    const summary = features.map((f, i) => `${i + 1}. ${f}`).join(" | ");

    `🛡️ [${engine} v${version}] Active features: ${summary} (Calculation: 10 * 42 = ${10 * 42})`
"#;
```
2. Create a v8 javascript string. `let code = v8::String::new(scope, js_code).unwrap();`
    - `v8::String::new(...)` returns option - Some(value) or None
    - `.unwrap()` returns value from Some(value) or crash when None
3. Compile the script.
    `let script = v8::Script::compile(scope, code, None).expect("Failed to compile JavaScript");`
4. Execute the script and get the evaluated result
    `let result = script.run(scope).expect("Failed to execute script");`
5. Convert the V8 string value back into a Rust String
    `let result_str = result.to_rust_string_lossy(scope);`

## Miscellaneous
- `to_rust_string_lossy` is a method on `v8::Local<v8::String>` in the Rust v8 crate. It converts an internal V8 JavaScript string into a standard Rust String.
