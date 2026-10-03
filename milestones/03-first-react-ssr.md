# Milestone 03: First React SSR

## 🎯 Goal
Bundle a real React 19 component using `esbuild`, load the bundled JavaScript file into V8 from Rust, pass JSON props into JavaScript, execute `ReactDOMServer.renderToString()`, and capture server-rendered HTML.

---

## 💡 Concepts

### 1. Bundling for a Bare V8 Environment
In Node.js, `require("react")` or `import React from 'react'` uses Node module resolution. In raw V8, there is no file system loader or `require`.
Therefore, client application code must be compiled into a single self-contained bundle with all dependencies inlined using `esbuild`:
```bash
esbuild src/entry.jsx --bundle --format=iife --platform=neutral --outfile=dist/bundle.js
```

### 2. The Global Render Contract
The React bundle exposes a function on `globalThis`:
```javascript
globalThis.render = function(props) {
    return ReactDOMServer.renderToString(React.createElement(App, props));
};
```

### 3. Executing a Global Function from Rust
Once the script is evaluated, Rust fetches the function from `context.global(scope)`:
```rust
let global = context.global(scope);
let render_key = v8::String::new(scope, "render").unwrap();
let render_val = global.get(scope, render_key.into()).unwrap();
let render_fn: v8::Local<v8::Function> = render_val.try_into().unwrap();
let html = render_fn.call(scope, global.into(), &[props_arg]).unwrap();
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
    "build": "esbuild src/entry.jsx --bundle --format=iife --platform=neutral --outfile=dist/bundle.js"
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
import ReactDOMServer from 'react-dom/server';

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
  return ReactDOMServer.renderToString(<App {...props} />);
};
```

Build the bundle:
```bash
cd web && npm install && npm run build && cd ..
```

---

### 2. Rust Host Execution (`src/main.rs`)
```rust
use std::fs;

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

    // 3. Compile and execute the React bundle
    let code = v8::String::new(scope, &bundle_source).unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    script.run(scope).expect("Bundle evaluation failed");

    // 4. Retrieve `globalThis.render`
    let global = context.global(scope);
    let render_key = v8::String::new(scope, "render").unwrap();
    let render_val = global.get(scope, render_key.into()).unwrap();
    let render_fn: v8::Local<v8::Function> = render_val.try_into().unwrap();

    // 5. Call `render(propsJson)`
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
- [ ] Understand why self-contained bundle compilation (`esbuild`) is required for V8.
- [ ] Successfully load and evaluate a complete React 19 bundle inside an isolate.
- [ ] Pass dynamic props into JavaScript and receive full HTML strings.
