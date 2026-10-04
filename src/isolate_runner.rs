/// The V8 execution harness (matching ssr-platform/src/isolate_runner.rs)
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