use std::fs;

fn console_log_callback(
    scope: &mut v8::PinScope,   // The pinned scope active during this function call
    args: v8::FunctionCallbackArguments,    // The array of arguments JavaScript passed in
    _rv: v8::ReturnValue<v8::Value>,    // Return slot (console.log returns undefined, so unused)
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

const SHIM_PRELUDE: &str = r#"
if (typeof queueMicrotask === "undefined") {
    globalThis.queueMicrotask = function (fn) { Promise.resolve().then(fn); };
}
if (typeof setTimeout === "undefined") {
    globalThis.setTimeout = function (fn) { queueMicrotask(fn); };
    globalThis.clearTimeout = function () {};
}
if (typeof MessageChannel === "undefined") {
    class MessagePort {
        constructor() { this.onmessage = null; this._other = null; }
        postMessage(data) {
            const other = this._other;
            queueMicrotask(() => { if (other && other.onmessage) other.onmessage({ data }); });
        }
    }
    globalThis.MessageChannel = class MessageChannel {
        constructor() {
            this.port1 = new MessagePort();
            this.port2 = new MessagePort();
            this.port1._other = this.port2;
            this.port2._other = this.port1;
        }
    };
}
"#;

fn main() {
    // 1. Initialize V8
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    // 2. Read the bundled JavaScript file
    let bundle_source = fs::read_to_string("web/dist/bundle.js")
        .expect("Please build bundle first: (cd web && npm run build)");

    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    let shim_code = v8::String::new(scope, SHIM_PRELUDE).unwrap();
    let shim_script = v8::Script::compile(scope, shim_code, None).unwrap();
    shim_script.run(scope).expect("Shims failed");

    // 3. Compile and execute the React bundle
    let code = v8::String::new(scope, &bundle_source).unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    script.run(scope).expect("Bundle evaluation failed");

    // 4. Retrieve `globalThis.render`
    let global = context.global(scope);
    let render_key = v8::String::new(scope, "render").unwrap();
    let render_val = global.get(scope, render_key.into()).unwrap();
    let render_fn: v8::Local<v8::Function> = render_val.try_into().unwrap();

    let console_obj = v8::Object::new(scope);
    let log_fn = v8::Function::new(scope, console_log_callback).unwrap();
    let log_key = v8::String::new(scope, "log").unwrap();
    console_obj.set(scope, log_key.into(), log_fn.into());
    let console_key = v8::String::new(scope, "console").unwrap();
    global.set(scope, console_key.into(), console_obj.into());

    // 5. Call `render(propsJson)`
    let props = r#"{"title": "Aegis Storefront", "user": "Engineer"}"#;
    let props_str = v8::String::new(scope, props).unwrap();

    let html_val = render_fn.call(scope, global.into(), &[props_str.into()])
        .expect("render() call failed");

    let html = html_val.to_rust_string_lossy(scope);

    println!("🎨 Server-Side Rendered HTML:\n\n{}", html);
}