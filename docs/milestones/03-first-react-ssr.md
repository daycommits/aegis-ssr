# Milestone 03: First React SSR

## 🎯 Goal
Bundle a real React 19 component using `esbuild`, provide browser API shims via a dedicated `src/shims.rs` module (matching `ssr-platform`), load the bundle into V8 from Rust, pass JSON props, and capture server-rendered HTML via `ReactDOMServer.renderToString()`.

---

## 💡 Concepts

### 1. Bundling for a Bare V8 Environment
In a bare V8 isolate, there are no Node.js built-ins. Therefore:
1. We import from **`react-dom/server.browser`** (the standalone build).
2. We compile using `esbuild` with `--platform=neutral`.
3. We define `process.env.NODE_ENV = "production"` so React doesn't search for Node's `process` global.

```bash
esbuild src/entry.jsx --bundle --format=iife --platform=neutral --define:process.env.NODE_ENV='"production"' --outfile=dist/bundle.js
```

### 2. Why `src/shims.rs`?
React's browser build expects certain Web APIs (`queueMicrotask`, `setTimeout`, `TextEncoder`, `TextDecoder`, `MessageChannel`). 

Following the architecture of `ssr-platform`, we isolate all runtime polyfills in **`src/shims.rs`**. We evaluate this prelude in V8 **before** running the React bundle so React finds all necessary globals on `globalThis`.

### 3. The Global Render Contract
The React bundle exposes a function on `globalThis`:
```javascript
globalThis.render = function(propsJson) {
    const props = JSON.parse(propsJson);
    return ReactDOMServer.renderToString(React.createElement(App, props));
};
```

---

## 📝 Implementation

### 1. React Web Bundle Setup
In `aegis-ssr/web/`:
```bash
mkdir -p web/src web/dist
```

`web/package.json`:
```json
{
  "name": "aegis-web",
  "version": "1.0.0",
  "scripts": {
    "build": "esbuild src/entry.jsx --bundle --format=iife --platform=neutral --define:process.env.NODE_ENV='\"production\"' --outfile=dist/bundle.js"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "esbuild": "^0.25.0"
  }
}
```

`web/src/entry.jsx`:
```jsx
import React from 'react';
import ReactDOMServer from 'react-dom/server.browser';

function App({ title, user }) {
  return (
    <div className="container" style={{ fontFamily: 'system-ui, sans-serif' }}>
      <h1>{title}</h1>
      <p>Welcome back, <strong>{user}</strong>!</p>
      <div className="badge">Rendered server-side with Aegis V8 🛡️</div>
    </div>
  );
}

// Register global render function
globalThis.render = function(propsJson) {
  const props = JSON.parse(propsJson);
  return ReactDOMServer.renderToString(React.createElement(App, props));
};
```

Build the bundle:
```bash
cd web && npm install && npm run build && cd ..
```

---

### 2. Browser Shims Module (`src/shims.rs`)
Create `src/shims.rs` to hold the Web API polyfills:

```rust
pub const HOST_API_SHIM_PRELUDE: &str = r#"
if (typeof queueMicrotask === "undefined") {
    globalThis.queueMicrotask = function (fn) { Promise.resolve().then(fn); };
}
if (typeof setTimeout === "undefined") {
    globalThis.setTimeout = function (fn) { queueMicrotask(fn); };
    globalThis.clearTimeout = function () {};
}
if (typeof TextEncoder === "undefined") {
    globalThis.TextEncoder = class TextEncoder {
        encode(str) {
            const bytes = [];
            for (let i = 0; i < str.length; i++) {
                let code = str.codePointAt(i);
                if (code > 0xFFFF) i++;
                if (code < 0x80) bytes.push(code);
                else if (code < 0x800) bytes.push(0xC0 | (code >> 6), 0x80 | (code & 0x3F));
                else if (code < 0x10000) bytes.push(0xE0 | (code >> 12), 0x80 | ((code >> 6) & 0x3F), 0x80 | (code & 0x3F));
                else bytes.push(0xF0 | (code >> 18), 0x80 | ((code >> 12) & 0x3F), 0x80 | ((code >> 6) & 0x3F), 0x80 | (code & 0x3F));
            }
            return new Uint8Array(bytes);
        }
    };
}
if (typeof TextDecoder === "undefined") {
    globalThis.TextDecoder = class TextDecoder {
        decode(bytes) {
            let out = "";
            for (let i = 0; i < bytes.length; i++) out += String.fromCharCode(bytes[i]);
            return out;
        }
    };
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
```

---

### 3. Rust Host Execution (`src/main.rs`)
```rust
mod shims;

use std::fs;
use shims::HOST_API_SHIM_PRELUDE;

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

    // 3. Evaluate the shims prelude FIRST so React finds Web APIs
    let shim_code = v8::String::new(scope, HOST_API_SHIM_PRELUDE).unwrap();
    let shim_script = v8::Script::compile(scope, shim_code, None).unwrap();
    shim_script.run(scope).expect("Shims prelude evaluation failed");

    // 4. Compile and execute the React bundle
    let code = v8::String::new(scope, &bundle_source).unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    script.run(scope).expect("Bundle evaluation failed");

    // 5. Retrieve `globalThis.render`
    let global = context.global(scope);
    let render_key = v8::String::new(scope, "render").unwrap();
    let render_val = global.get(scope, render_key.into()).unwrap();
    let render_fn: v8::Local<v8::Function> = render_val.try_into().unwrap();

    // 6. Call `render(propsJson)`
    let props = r#"{"title": "Aegis Storefront", "user": "Engineer"}"#;
    let props_str = v8::String::new(scope, props).unwrap();

    let html_val = render_fn.call(scope, global.into(), &[props_str.into()])
        .expect("render() call failed");

    let html = html_val.to_rust_string_lossy(scope);

    println!("🎨 Server-Side Rendered HTML:\n\n{}", html);
}
```

---

## 🔍 Verification
Run:
```bash
cargo run
```

Expected output:
```html
🎨 Server-Side Rendered HTML:

<div class="container" style="font-family:system-ui, sans-serif"><h1>Aegis Storefront</h1><p>Welcome back, <strong>Engineer</strong>!</p><div class="badge">Rendered server-side with Aegis V8 🛡️</div></div>
```

---

## ✅ Checklist
- [ ] Understand why `src/shims.rs` isolates browser polyfills from core logic.
- [ ] Successfully load and evaluate `HOST_API_SHIM_PRELUDE` before bundle execution.
- [ ] Pass dynamic JSON props from Rust into JavaScript and receive full React-rendered HTML.
