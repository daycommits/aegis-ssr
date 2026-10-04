fn console_log_callback(
    scope: &mut v8::PinScope,              // The pinned scope active during this function call
    args: v8::FunctionCallbackArguments,    // The array of arguments JavaScript passed in
    _rv: v8::ReturnValue<v8::Value>,       // Return slot (console.log returns undefined, so unused)
) {
    let mut parts = Vec::new();

    // Iterate through all arguments passed by JS (e.g. console.log("User:", 42, true))
    for i in 0..args.length() {
        let arg = args.get(i);

        // Convert the V8 JavaScript value (string, number, object, etc.) into a Rust String
        parts.push(arg.to_rust_string_lossy(scope));
    }

    // Print the captured JS arguments through Rust's standard output
    println!("🦀 [Rust console.log proxy] {}", parts.join(" "));
}

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


    let global = context.global(scope);
    let console_obj = v8::Object::new(scope);
    let log_fn = v8::Function::new(scope, console_log_callback).unwrap();
    let log_key = v8::String::new(scope, "log").unwrap();
    console_obj.set(scope, log_key.into(), log_fn.into());
    let console_key = v8::String::new(scope, "console").unwrap();
    global.set(scope, console_key.into(), console_obj.into());

    // 5. Write real JavaScript code with computation, arrays, and template strings
    let js_code = r#"
        const engine = "Aegis";
        const version = 1;
        const features = ["Memory Isolation", "Watchdogs", "Sub-millisecond Renders"];

        // Compute a summary using arrow functions and array methods
        const summary = features.map((f, i) => `${i + 1}. ${f}`).join(" | ");
        console.log("🛡️ Log [JavaScript] Active features:", summary);

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