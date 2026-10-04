# Milestone 07: Multi-Tenancy & Manifest Registry (`src/tenant_registry.rs` & `src/deployment.rs`)

## 🎯 Goal
Implement enterprise multi-tenancy matching `ssr-platform`'s architecture:
1. Load a declarative manifest from **`tenants.json`** via **`src/tenant_registry.rs`**.
2. Manage per-tenant isolated pools via **`src/deployment.rs`** (`DeploymentManager`).
3. Route incoming HTTP traffic in **`src/bin/gateway.rs`** dynamically by tenant ID (`/render/{tenant_id}`).

---

## 📁 File Structure Introduced
```text
aegis-ssr/
├── tenants.json            <-- 🌟 NEW: Multi-tenant configuration manifest
├── src/
│   ├── lib.rs              <-- Library root (exports tenant_registry, deployment)
│   ├── shims.rs            <-- Browser polyfills
│   ├── isolate_runner.rs   <-- Execution engine & guardrails
│   ├── isolate_pool.rs     <-- Worker pool
│   ├── tenant_registry.rs  <-- 🌟 NEW: Manifest loader & tenant configs
│   ├── deployment.rs       <-- 🌟 NEW: DeploymentManager orchestrating tenant pools
│   ├── main.rs             <-- CLI runner
│   └── bin/
│       └── gateway.rs      <-- Production HTTP Gateway routing /render/{tenant_id}
├── web-bundle/
│   └── dist/bundle.js
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. Multi-Tenant Separation of Concerns
In `ssr-platform`, tenant management is strictly split into two layers:
- **`TenantRegistry` (`src/tenant_registry.rs`)**: Reads `tenants.json`. Contains static configuration (resource limits, timeout, initial bundle path).
- **`DeploymentManager` (`src/deployment.rs`)**: Owns the runtime state (which isolate pool is warm, active bundle, memory allocation per tenant).

### 2. Failure Isolation
Each tenant has:
- Its own isolate pool.
- Its own `max_heap_mb`.
- Its own worker threads.
If the `checkout` bundle crashes or hits an infinite loop, `marketplace` traffic continues serving normally with zero degradation!

---

## 📝 Implementation

### 1. Update `Cargo.toml`
Add `serde` and `serde_json`:

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
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

### 2. Create `tenants.json`
Create `tenants.json` in the project root:

```json
{
  "marketplace": {
    "bundle_path": "web-bundle/dist/bundle.js",
    "max_heap_mb": 64,
    "pool_size": 2,
    "timeout_secs": 5
  },
  "checkout": {
    "bundle_path": "web-bundle/dist/bundle.js",
    "max_heap_mb": 32,
    "pool_size": 2,
    "timeout_secs": 3
  }
}
```

---

### 3. Create `src/tenant_registry.rs`
Create `src/tenant_registry.rs` matching `ssr-platform/src/tenant_registry.rs`:

```rust
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct TenantConfig {
    pub bundle_path: String,
    #[serde(default = "default_heap_mb")]
    pub max_heap_mb: usize,
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_heap_mb() -> usize { 64 }
fn default_pool_size() -> usize { 2 }
fn default_timeout_secs() -> u64 { 5 }

pub struct TenantRegistry {
    pub tenants: HashMap<String, TenantConfig>,
}

impl TenantRegistry {
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let tenants: HashMap<String, TenantConfig> = serde_json::from_str(&content)
            .map_err(|e| e.to_string())?;
        Ok(Self { tenants })
    }

    pub fn get(&self, tenant_id: &str) -> Option<&TenantConfig> {
        self.tenants.get(tenant_id)
    }
}
```

---

### 4. Create `src/deployment.rs`
Create `src/deployment.rs` matching `ssr-platform/src/deployment.rs`:

```rust
use std::collections::HashMap;
use std::fs;
use crate::isolate_pool::IsolatePool;
use crate::tenant_registry::TenantRegistry;

pub struct DeploymentManager {
    pools: HashMap<String, IsolatePool>,
}

impl DeploymentManager {
    pub fn init(registry: &TenantRegistry) -> Self {
        let mut pools = HashMap::new();

        for (tenant_id, config) in &registry.tenants {
            println!("📦 Initializing pool for tenant: '{}'", tenant_id);

            let bundle_source = fs::read_to_string(&config.bundle_path)
                .unwrap_or_else(|_| panic!("Failed to read bundle at {} for tenant {}", config.bundle_path, tenant_id));
            let bundle_static: &'static str = Box::leak(bundle_source.into_boxed_str());

            let pool = IsolatePool::new(config.pool_size, bundle_static);
            pools.insert(tenant_id.clone(), pool);
        }

        Self { pools }
    }

    pub fn get_pool(&self, tenant_id: &str) -> Option<&IsolatePool> {
        self.pools.get(tenant_id)
    }
}
```

---

### 5. Update `src/lib.rs`
`src/lib.rs` only exports the modules:

```rust
pub mod bundle;
pub mod deployment;
pub mod isolate_pool;
pub mod isolate_runner;
pub mod shims;
pub mod tenant_registry;
```

---

### 6. Update `src/bin/gateway.rs`
Update the gateway to route by tenant ID:

```rust
use aegis_ssr::{
    deployment::DeploymentManager,
    isolate_pool::RenderJob,
    isolate_runner::init_v8_once,
    tenant_registry::TenantRegistry,
};
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
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
    // 1. Initialize V8 platform
    init_v8_once();

    // 2. Load tenants.json and initialize tenant pools
    let tenants_file = std::env::var("TENANTS_FILE").unwrap_or_else(|_| "tenants.json".to_string());
    println!("📋 Loading manifest: {}", tenants_file);
    let registry = TenantRegistry::load_from_file(&tenants_file).expect("Failed to load tenants.json");

    println!("🚀 Starting DeploymentManager...");
    let deployment_manager = DeploymentManager::init(&registry);
    let state = Arc::new(AppState { deployment_manager });

    // 3. Build router routing /render/{tenant_id}
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ready", get(|| async { "READY" }))
        .route("/render/{tenant_id}", get(render_tenant_handler))
        .with_state(state);

    let host = std::env::var("GATEWAY_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("GATEWAY_PORT").unwrap_or_else(|_| "8787".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("🛡️ Aegis Multi-Tenant Gateway serving on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}

async fn render_tenant_handler(
    Path(tenant_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.deployment_manager.get_pool(&tenant_id) {
        Some(pool) => {
            let (reply_tx, reply_rx) = oneshot::channel();
            let props = format!(r#"{{"title": "{} Portal", "user": "Engineer"}}"#, tenant_id);

            if let Err(_) = pool.job_tx.send(RenderJob { props, reply_tx }) {
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Pool channel error").into_response();
            }

            match reply_rx.await {
                Ok(Ok(html)) => Html(html).into_response(),
                Ok(Err(err)) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
                Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Job dropped").into_response(),
            }
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            format!("Tenant '{}' not found in registry", tenant_id),
        ).into_response(),
    }
}
```

---

## 🔍 Verification
Run the multi-tenant gateway:
```bash
cargo run --bin gateway
```

Test requests across tenants:
```bash
# Valid Tenant 1
curl http://localhost:8787/render/marketplace
# Returns: HTML for Marketplace Portal

# Valid Tenant 2
curl http://localhost:8787/render/checkout
# Returns: HTML for Checkout Portal

# Non-existent Tenant
curl -i http://localhost:8787/render/unknown
# Returns HTTP 404: Tenant 'unknown' not found in registry
```

---

## ✅ Checklist
- [ ] Created `tenants.json` manifest at project root.
- [ ] Implemented `src/tenant_registry.rs` and `src/deployment.rs` matching `ssr-platform`.
- [ ] Verified per-tenant routing and graceful 404 handling.
