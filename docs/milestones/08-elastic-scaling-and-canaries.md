# Milestone 08: Elastic Scaling & Canaries (`src/cohort.rs`)

## 🎯 Goal
Implement the two crowning features of the SSR platform matching `ssr-platform`:
1. **Canary Cohort Routing (`src/cohort.rs`)**: Route traffic between stable and canary bundle versions using sticky HTTP cookies (`v8i_cohort`).
2. **Elastic Scaling & Cold Pools (`src/isolate_pool.rs`)**: Dynamic worker spawning under backlog with idle worker eviction back to zero (`pool_size: 0`).

---

## 📁 File Structure Introduced
```text
aegis-ssr/
├── tenants.json            <-- Multi-tenant configuration
├── src/
│   ├── lib.rs              <-- Library root (exports cohort, deployment, isolate_pool, etc.)
│   ├── shims.rs            <-- Browser polyfills
│   ├── isolate_runner.rs   <-- Execution engine & guardrails
│   ├── isolate_pool.rs     <-- Enhanced with elastic idle eviction
│   ├── tenant_registry.rs  <-- Manifest loader
│   ├── deployment.rs       <-- Manages stable and canary pools
│   ├── cohort.rs           <-- 🌟 NEW: Sticky cookie canary cohort routing
│   ├── main.rs             <-- CLI runner
│   └── bin/
│       └── gateway.rs      <-- Production gateway routing canary traffic
├── web-bundle/
│   └── dist/bundle.js
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. Sticky Canary Routing (`src/cohort.rs`)
When rolling out a new bundle version (`v2`):
- Both `v1` (Active/Stable pool) and `v2` (Canary pool) run concurrently.
- Incoming requests check for the `v8i_cohort` cookie:
  - If cookie is `canary` → route to Canary pool.
  - If cookie is `stable` → route to Stable pool.
  - If cookie is missing → roll a weighted random percentage (e.g. 20% canary) and return `Set-Cookie` so the user is pinned to that cohort.

### 2. Cold Pools (`pool_size: 0`) & Idle Eviction
In an enterprise with dozens of micro-frontend teams, idle tenants waste memory if they keep workers alive permanently.
- `pool_size: 0` tenants start with **zero OS threads** and **zero bytes of memory**.
- The first request triggers dynamic worker growth:
  ```rust
  if live_workers == 0 {
      spawn_elastic_worker();
  }
  ```
- Elastic workers use `rx.recv_timeout(ELASTIC_IDLE_TIMEOUT)` (30 seconds). If no requests arrive for 30s, the worker thread terminates cleanly and drops its V8 isolate.

---

## 📝 Implementation

### 1. Update `Cargo.toml`
Add `rand`:

```toml
[dependencies]
v8 = "152.2.0"
crossbeam-channel = "0.5"
tokio = { version = "1", features = ["full"] }
axum = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rand = "0.8"
```

---

### 2. Create `src/cohort.rs`
Create `src/cohort.rs` matching `ssr-platform/src/cohort.rs`:

```rust
use axum::http::HeaderMap;
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cohort {
    Stable,
    Canary,
}

impl Cohort {
    pub fn as_str(self) -> &'static str {
        match self {
            Cohort::Stable => "stable",
            Cohort::Canary => "canary",
        }
    }
}

const COOKIE_NAME: &str = "v8i_cohort";
const COOKIE_MAX_AGE_SECS: u64 = 86_400; // 24 hours

/// Reads cohort assignment from the Cookie header
pub fn parse_cohort_hint(headers: &HeaderMap) -> Option<Cohort> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            if k == COOKIE_NAME {
                return match v {
                    "canary" => Some(Cohort::Canary),
                    "stable" => Some(Cohort::Stable),
                    _ => None,
                };
            }
        }
    }
    None
}

/// Rolls random percentage for new visitors without a cookie
pub fn pick_cohort_random(canary_percent: u8) -> Cohort {
    let mut rng = rand::thread_rng();
    if rng.gen_range(0..100) < canary_percent {
        Cohort::Canary
    } else {
        Cohort::Stable
    }
}

/// Generates Set-Cookie header value
pub fn cohort_cookie_value(cohort: Cohort) -> String {
    format!(
        "{}={}; Path=/; Max-Age={}; SameSite=Lax",
        COOKIE_NAME,
        cohort.as_str(),
        COOKIE_MAX_AGE_SECS
    )
}
```

---

### 3. Add Elastic Scaling to `src/isolate_pool.rs`
Update `src/isolate_pool.rs` to support idle timeouts for elastic workers:

```rust
use std::time::Duration;
use crossbeam_channel::{Receiver, Sender};
use tokio::sync::oneshot;

pub const ELASTIC_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct RenderJob {
    pub props: String,
    pub reply_tx: oneshot::Sender<Result<String, String>>,
}

pub struct IsolatePool {
    pub job_tx: Sender<RenderJob>,
}

pub fn elastic_worker_loop(
    id: usize,
    job_rx: Receiver<RenderJob>,
    bundle_code: &'static str,
    is_elastic: bool,
) {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // Evaluate shims and bundle
    let shim_code = v8::String::new(scope, crate::shims::HOST_API_SHIM_PRELUDE).unwrap();
    v8::Script::compile(scope, shim_code, None).unwrap().run(scope).unwrap();

    let code = v8::String::new(scope, bundle_code).unwrap();
    v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();

    loop {
        let job = if is_elastic {
            // Elastic workers timeout and exit if idle!
            match job_rx.recv_timeout(ELASTIC_IDLE_TIMEOUT) {
                Ok(j) => j,
                Err(_) => {
                    println!("💤 Elastic worker #{} idle timed out. Evicting isolate.", id);
                    break; // Thread exits, freeing memory!
                }
            }
        } else {
            // Baseline workers wait indefinitely
            match job_rx.recv() {
                Ok(j) => j,
                Err(_) => break,
            }
        };

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

### 4. Update `src/lib.rs`
`src/lib.rs` only exports the modules:

```rust
pub mod bundle;
pub mod cohort;
pub mod deployment;
pub mod isolate_pool;
pub mod isolate_runner;
pub mod shims;
pub mod tenant_registry;
```

---

### 5. Update `src/bin/gateway.rs`
Integrate sticky cohort routing into the request pipeline:

```rust
use aegis_ssr::{
    cohort::{cohort_cookie_value, parse_cohort_hint, pick_cohort_random, Cohort},
    deployment::DeploymentManager,
    isolate_pool::RenderJob,
    isolate_runner::init_v8_once,
    tenant_registry::TenantRegistry,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::oneshot;

struct AppState {
    deployment_manager: DeploymentManager,
}

#[tokio::main]
async fn main() {
    init_v8_once();

    let registry = TenantRegistry::load_from_file("tenants.json").expect("Failed to load tenants.json");
    let deployment_manager = DeploymentManager::init(&registry);
    let state = Arc::new(AppState { deployment_manager });

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/render/{tenant_id}", get(render_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787").await.unwrap();
    println!("🛡️ Aegis Gateway (Canary-Enabled) listening on http://0.0.0.0:8787");

    axum::serve(listener, app).await.unwrap();
}

async fn render_handler(
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    // 1. Resolve cohort (sticky cookie or random 20% canary split)
    let (cohort, set_cookie) = match parse_cohort_hint(&headers) {
        Some(existing) => (existing, None),
        None => {
            let picked = pick_cohort_random(20); // 20% canary weight
            let cookie = cohort_cookie_value(picked);
            (picked, Some(cookie))
        }
    };

    println!("🎯 Request for tenant '{}' routed to cohort: {:?}", tenant_id, cohort);

    // 2. Fetch pool
    let pool = match state.deployment_manager.get_pool(&tenant_id) {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, format!("Tenant '{}' not found", tenant_id)).into_response(),
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    let props = format!(r#"{{"title": "{} [Cohort: {:?}]", "user": "Engineer"}}"#, tenant_id, cohort);

    pool.job_tx.send(RenderJob { props, reply_tx }).unwrap();

    let mut response = match reply_rx.await {
        Ok(Ok(html)) => Html(html).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Render failed").into_response(),
    };

    // 3. Set sticky cookie header if newly assigned
    if let Some(cookie_val) = set_cookie {
        response.headers_mut().insert(header::SET_COOKIE, cookie_val.parse().unwrap());
    }

    response
}
```

---

## 🔍 Verification
Run the gateway:
```bash
cargo run --bin gateway
```

Test sticky canary behavior:
```bash
# First request: receives a Set-Cookie header pinning to stable or canary
curl -i http://localhost:8787/render/marketplace

# Simulate a client pinned to canary via cookie
curl -i -H "Cookie: v8i_cohort=canary" http://localhost:8787/render/marketplace
# Server logs: "routed to cohort: Canary"

# Simulate a client pinned to stable via cookie
curl -i -H "Cookie: v8i_cohort=stable" http://localhost:8787/render/marketplace
# Server logs: "routed to cohort: Stable"
```

---

## 🎓 Graduation Checklist
- [ ] Created `src/cohort.rs` matching `ssr-platform/src/cohort.rs`.
- [ ] Implemented sticky cookie routing (`v8i_cohort`) with weighted percentage roll.
- [ ] Understand `pool_size: 0` and idle worker eviction with `ELASTIC_IDLE_TIMEOUT`.
- [ ] Your codebase structure now mirrors `ssr-platform` module-for-module! 🛡️
