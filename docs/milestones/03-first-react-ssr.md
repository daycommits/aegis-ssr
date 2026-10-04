# Milestone 03: React SSR & Bundle Loader (`web-bundle/` & `src/bundle.rs`)

## 🎯 Goal
Implement the React SSR bundling and loading pipeline matching `ssr-platform`:
1. **`web-bundle/`**: Set up React 19 and compile a production bundle using `esbuild`.
2. **`src/shims.rs`**: Add `TextEncoder`, `TextDecoder`, and `MessageChannel` polyfills.
3. **`src/bundle.rs`**: Build the script assembler module (matching `ssr-platform/src/bundle.rs`) that reads the compiled bundle and wraps it with shims.
4. **`src/isolate_runner.rs` & `src/main.rs`**: Execute `ReactDOMServer.renderToString()` inside V8 and print server-rendered HTML.

---

## 📁 File Structure (`ssr-platform` layout)
```text
aegis-ssr/
├── web-bundle/             <-- 🌟 Matches ssr-platform/web-bundle/
│   ├── package.json
│   ├── src/entry.jsx
│   └── dist/bundle.js      <-- Built JS bundle
├── src/
│   ├── lib.rs              <-- Exports shims, bundle, isolate_runner
│   ├── shims.rs            <-- Updated with TextEncoder & MessageChannel
│   ├── bundle.rs           <-- 🌟 NEW: Script assembler matching ssr-platform
│   ├── isolate_runner.rs   <-- Executes render(props)
│   └── main.rs             <-- CLI test runner
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. Why `src/bundle.rs`?
In `ssr-platform`, reading bundle files and concatenating shims is handled by **`src/bundle.rs`**:
```rust
pub fn build_script(bundle_path: &str) -> Option<String> {
    let bundle = std::fs::read_to_string(bundle_path).ok()?;
    Some(format!("{}\n{}", HOST_API_SHIM_PRELUDE, bundle))
}
```
This keeps file I/O and script assembly separated from V8 execution.

### 2. Standalone React Browser Build
We import from `react-dom/server.browser` and use `esbuild --platform=neutral --define:process.env.NODE_ENV='"production"'` so React does not look for Node.js built-ins (`util`, `crypto`, `stream`).

---

## 📝 Implementation

### 1. Set Up `web-bundle/`
In the project root, create `web-bundle/`:
```bash
mkdir -p web-bundle/src web-bundle/dist
```

`web-bundle/package.json`:
```json
{
  "name": "aegis-web-bundle",
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

`web-bundle/src/entry.jsx`:
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

// Expose global render function
globalThis.render = function(propsJson) {
  const props = JSON.parse(propsJson);
  return ReactDOMServer.renderToString(React.createElement(App, props));
};
```

Compile the bundle:
```bash
cd web-bundle && npm install && npm run build && cd ..
```

---

### 2. Complete `src/shims.rs`
Update `src/shims.rs` with the complete browser prelude from `ssr-platform`:

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

### 3. Create `src/bundle.rs`
Create `src/bundle.rs` matching `ssr-platform/src/bundle.rs`:

```rust
use crate::shims::HOST_API_SHIM_PRELUDE;
use std::fs;

/// Reads compiled bundle from disk and prepends host API shims
pub fn build_script(bundle_path: &str) -> Option<String> {
    let bundle = fs::read_to_string(bundle_path).ok()?;
    Some(format!("{}\n{}", HOST_API_SHIM_PRELUDE, bundle))
}
```

---

### 4. Update `src/lib.rs`
`src/lib.rs` only exports the modules:

```rust
pub mod bundle;
pub mod isolate_runner;
pub mod shims;
```

---

### 5. Update `src/isolate_runner.rs`
Add `render_bundle` to call `globalThis.render`:

```rust
pub fn render_bundle(assembled_script: &str, props_json: &str) -> Result<String, String> {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 1. Compile and evaluate shims + bundle
    let code = match v8::String::new(scope, assembled_script) {
        Some(c) => c,
        None => return Err("Failed to allocate bundle string".to_string()),
    };

    let script = match v8::Script::compile(scope, code, None) {
        Some(s) => s,
        None => return Err("Failed to compile bundle".to_string()),
    };

    if script.run(scope).is_none() {
        return Err("Bundle initialization failed".to_string());
    }

    // 2. Fetch globalThis.render
    let global = context.global(scope);
    let render_key = v8::String::new(scope, "render").unwrap();
    let render_val = global.get(scope, render_key.into()).unwrap();
    let render_fn: v8::Local<v8::Function> = match render_val.try_into() {
        Ok(f) => f,
        Err(_) => return Err("globalThis.render is not a function".to_string()),
    };

    // 3. Call render(propsJson)
    let props_str = v8::String::new(scope, props_json).unwrap();
    match render_fn.call(scope, global.into(), &[props_str.into()]) {
        Some(val) => Ok(val.to_rust_string_lossy(scope)),
        None => Err("render() invocation failed".to_string()),
    }
}
```

---

### 6. Update `src/main.rs`
```rust
use aegis_ssr::{bundle::build_script, isolate_runner::{init_v8_once, render_bundle}};

fn main() {
    init_v8_once();

    let assembled_script = build_script("web-bundle/dist/bundle.js")
        .expect("Please build bundle: (cd web-bundle && npm run build)");

    let props = r#"{"title": "Aegis Storefront", "user": "Engineer"}"#;

    match render_bundle(&assembled_script, props) {
        Ok(html) => println!("🎨 Server-Side Rendered HTML:\n\n{}", html),
        Err(err) => eprintln!("❌ Render error: {}", err),
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
```html
🎨 Server-Side Rendered HTML:

<div class="container" style="font-family:system-ui, sans-serif"><h1>Aegis Storefront</h1><p>Welcome back, <strong>Engineer</strong>!</p><div class="badge">Rendered server-side with Aegis V8 🛡️</div></div>
```

---

## ✅ Checklist
- [ ] Created `web-bundle/` matching `ssr-platform/web-bundle`.
- [ ] Created `src/bundle.rs` to assemble shims and compiled code.
- [ ] Executed `render_bundle` in `src/isolate_runner.rs` and received valid HTML.
