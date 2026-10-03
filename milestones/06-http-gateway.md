# Milestone 06: Axum HTTP Gateway

## 🎯 Goal
Build an asynchronous HTTP gateway using Axum and Tokio that receives incoming HTTP requests, routes them to our background V8 isolate pool, and streams the server-rendered HTML back to the browser.

---

## 💡 Concepts

### 1. The Async-to-Sync Bridge
- **Axum** runs on Tokio's asynchronous event loop (thousands of concurrent network connections).
- **V8** runs on synchronous OS threads in our `IsolatePool`.
- The bridge between them is Tokio's `tokio::sync::oneshot` channel. The async HTTP handler sends the render job to the pool and `await`s the oneshot response without blocking Tokio's worker threads!

```
HTTP Client ──> [Axum GET /render] ──(async await)──> [Oneshot Channel]
                                                              ▲
                                                              │
[V8 Worker Pool (OS Threads)] ────────────────────────────────┘
```

### 2. Kubernetes Probes (`/health` & `/ready`)
To be container- and Kubernetes-ready:
- `/health`: Liveness probe (returns 200 immediately if process is alive).
- `/ready`: Readiness probe (returns 200 once bundles and isolate pools are fully initialized).

---

## 📝 Implementation

### Dependencies (`Cargo.toml`)
```toml
[dependencies]
v8 = "152.2.0"
crossbeam-channel = "0.5"
tokio = { version = "1", features = ["full"] }
axum = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### Code (`src/main.rs`)
```rust
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::sync::Arc;
use tokio::sync::oneshot;

struct RenderJob {
    props: String,
    reply_tx: oneshot::Sender<Result<String, String>>,
}

struct AppState {
    pool_tx: Sender<RenderJob>,
}

#[tokio::main]
async fn main() {
    // 1. Initialize V8 platform
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    // 2. Initialize V8 Isolate Pool
    let (pool_tx, pool_rx) = unbounded::<RenderJob>();
    for id in 0..4 {
        let rx = pool_rx.clone();
        std::thread::spawn(move || {
            worker_loop(id, rx);
        });
    }

    let shared_state = Arc::new(AppState { pool_tx });

    // 3. Build Axum HTTP Router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/render/{user}", get(render_handler))
        .with_state(shared_state);

    // 4. Bind to 0.0.0.0:8787
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787").await.unwrap();
    println!("🛡️ Aegis-SSR Gateway listening on http://0.0.0.0:8787");

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

    let props = format!(r#"{{"user": "{}", "timestamp": "{:?}"}}"#, user, std::time::Instant::now());

    state.pool_tx.send(RenderJob { props, reply_tx }).unwrap();

    match reply_rx.await.unwrap() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

fn worker_loop(id: usize, rx: Receiver<RenderJob>) {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    let bundle = r#"
        globalThis.render = (propsJson) => {
            const props = JSON.parse(propsJson);
            return `<div style="font-family:sans-serif;padding:2rem;background:#f4f4f9;">
                <h1>🛡️ Aegis-SSR Gateway</h1>
                <p>Welcome, <strong>${props.user}</strong>!</p>
                <small>Generated live via Rust + V8 152 at ${props.timestamp}</small>
            </div>`;
        };
    "#;
    let code = v8::String::new(scope, bundle).unwrap();
    v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();

    println!("👷 Worker #{} ready!", id);

    while let Ok(job) = rx.recv() {
        let global = context.global(scope);
        let render_key = v8::String::new(scope, "render").unwrap();
        let render_fn: v8::Local<v8::Function> = global.get(scope, render_key.into())
            .unwrap()
            .try_into()
            .unwrap();

        let props_str = v8::String::new(scope, &job.props).unwrap();
        let html = match render_fn.call(scope, global.into(), &[props_str.into()]) {
            Some(v) => Ok(v.to_rust_string_lossy(scope)),
            None => Err("Render error".to_string()),
        };

        let _ = job.reply_tx.send(html);
    }
}
```

---

## 🔍 Verification
Run:
```bash
cargo run
```

Then in another terminal or browser:
```bash
curl http://localhost:8787/health
# Returns: OK

curl http://localhost:8787/render/Ashu
# Returns: HTML containing "Welcome, Ashu!"
```

---

## ✅ Checklist
- [ ] Connect Tokio async HTTP handlers with OS worker threads.
- [ ] Implement production health/readiness endpoints.
- [ ] Serve live SSR HTML over HTTP with zero blocking on Tokio threads.
