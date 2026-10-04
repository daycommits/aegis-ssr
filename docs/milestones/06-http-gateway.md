# Milestone 06: Axum HTTP Gateway (`src/bin/gateway.rs`)

## 🎯 Goal
Build the production HTTP gateway as a dedicated binary target in **`src/bin/gateway.rs`** (matching `ssr-platform/src/bin/gateway.rs`). It receives incoming HTTP requests over Axum, routes them asynchronously to our background V8 `IsolatePool`, and returns server-rendered HTML over HTTP.

---

## 📁 File Structure Introduced
```text
aegis-ssr/
├── src/
│   ├── lib.rs              <-- Library root exposing core modules
│   ├── shims.rs            <-- Browser polyfills
│   ├── isolate_runner.rs   <-- Execution engine & guardrails
│   ├── isolate_pool.rs     <-- Worker pool
│   ├── main.rs             <-- CLI runner
│   └── bin/
│       └── gateway.rs      <-- 🌟 NEW: Production HTTP Gateway binary
├── web-bundle/
│   └── dist/bundle.js
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. Dedicated Binary Targets (`src/bin/`)
In Cargo projects, putting a file in `src/bin/<name>.rs` automatically creates a separate binary target.
- `src/main.rs`: Local CLI testing and benchmarks (`cargo run`).
- `src/bin/gateway.rs`: The production HTTP server (`cargo run --bin gateway`).
Both consume the shared library modules defined in `src/lib.rs`.

### 2. The Async-to-Sync Bridge
- **Axum** runs on Tokio's asynchronous event loop (thousands of concurrent network connections).
- **V8** runs on synchronous OS threads in our `IsolatePool`.
- The bridge between them is Tokio's `tokio::sync::oneshot` channel. The async HTTP handler sends the render job to the pool and `await`s the oneshot response without blocking Tokio's worker threads!

```
HTTP Client ──> [Axum GET /render] ──(async await)──> [Tokio Oneshot]
                                                              ▲
                                                              │
[V8 Worker Pool (OS Threads)] ────────────────────────────────┘
```

---

## 📝 Implementation

### 1. Update `Cargo.toml`
Add `tokio` and `axum`:

```toml
[package]
name = "aegis-ssr"
version = "0.1.0"
edition = "2024"

[dependencies]
v8 = "152.2.0"
crossbeam-channel = "0.5"
tokio = { version = "1", features = ["full"] }
axum = "0.8"
```

---

### 2. Update `src/isolate_pool.rs`
Update `RenderJob` to use `tokio::sync::oneshot::Sender` so it integrates smoothly with async Axum handlers:

```rust
use std::thread;
use crossbeam_channel::{unbounded, Receiver, Sender};
use tokio::sync::oneshot;
use crate::shims::HOST_API_SHIM_PRELUDE;

pub struct RenderJob {
    pub props: String,
    pub reply_tx: oneshot::Sender<Result<String, String>>,
}

pub struct IsolatePool {
    pub job_tx: Sender<RenderJob>,
}

impl IsolatePool {
    pub fn new(worker_count: usize, bundle_code: &'static str) -> Self {
        let (job_tx, job_rx): (Sender<RenderJob>, Receiver<RenderJob>) = unbounded();

        for worker_id in 0..worker_count {
            let rx = job_rx.clone();
            thread::spawn(move || {
                worker_loop(worker_id, rx, bundle_code);
            });
        }

        Self { job_tx }
    }
}

fn worker_loop(id: usize, job_rx: Receiver<RenderJob>, bundle_code: &str) {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // Run shims and bundle once at thread start
    let shim_code = v8::String::new(scope, HOST_API_SHIM_PRELUDE).unwrap();
    v8::Script::compile(scope, shim_code, None).unwrap().run(scope).unwrap();

    let code = v8::String::new(scope, bundle_code).unwrap();
    v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();

    println!("👷 Worker #{} ready!", id);

    while let Ok(job) = job_rx.recv() {
        let global = context.global(scope);
        let render_key = v8::String::new(scope, "render").unwrap();
        let render_fn: v8::Local<v8::Function> = global.get(scope, render_key.into())
            .unwrap()
            .try_into()
            .unwrap();

        let props_str = v8::String::new(scope, &job.props).unwrap();
        let result = match render_fn.call(scope, global.into(), &[props_str.into()]) {
            Some(val) => Ok(val.to_rust_string_lossy(scope)),
            None => Err("Render failed".to_string()),
        };

        let _ = job.reply_tx.send(result);
    }
}
```

---

### 3. Create `src/bin/gateway.rs`
Create `src/bin/gateway.rs` matching `ssr-platform/src/bin/gateway.rs`:

```rust
use aegis_ssr::{isolate_pool::{IsolatePool, RenderJob}, isolate_runner::init_v8_once};
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::fs;
use std::sync::Arc;
use tokio::sync::oneshot;

struct AppState {
    pool: IsolatePool,
}

#[tokio::main]
async fn main() {
    // 1. Initialize V8 platform via isolate_runner
    init_v8_once();

    // 2. Load React bundle and spin up pool
    let bundle_source = fs::read_to_string("web-bundle/dist/bundle.js")
        .expect("Please build bundle first: (cd web-bundle && npm run build)");
    let bundle_static: &'static str = Box::leak(bundle_source.into_boxed_str());

    println!("🚀 Starting IsolatePool with 4 warm workers...");
    let pool = IsolatePool::new(4, bundle_static);

    let state = Arc::new(AppState { pool });

    // 3. Build HTTP router with health and render endpoints
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/render/{user}", get(render_handler))
        .with_state(state);

    let host = std::env::var("GATEWAY_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("GATEWAY_PORT").unwrap_or_else(|_| "8787".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("🛡️ Aegis Gateway serving on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}

async fn health_handler() -> &'static str {
    "OK"
}

async fn ready_handler() -> &'static str {
    "READY"
}

async fn render_handler(
    Path(user): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let (reply_tx, reply_rx) = oneshot::channel();

    let props = format!(r#"{{"title": "Live Aegis Gateway", "user": "{}"}}"#, user);

    if let Err(_) = state.pool.job_tx.send(RenderJob { props, reply_tx }) {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Worker pool offline").into_response();
    }

    match reply_rx.await {
        Ok(Ok(html)) => Html(html).into_response(),
        Ok(Err(err)) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Render job dropped").into_response(),
    }
}
```

---

## 🔍 Verification
Run the gateway binary:
```bash
cargo run --bin gateway
```

In another terminal, test the endpoints:
```bash
curl http://localhost:8787/health
# Returns: OK

curl http://localhost:8787/render/Ashu
# Returns: Full React HTML containing "Welcome back, Ashu!"
```

---

## ✅ Checklist
- [ ] Created `src/bin/gateway.rs` as a dedicated binary target matching `ssr-platform`.
- [ ] Bridged Axum's async Tokio tasks to synchronous V8 OS threads using `tokio::sync::oneshot`.
- [ ] Tested live HTTP SSR serving on `http://0.0.0.0:8787`.
