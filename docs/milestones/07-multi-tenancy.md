# Milestone 07: Multi-Tenancy & Manifest Registry

## 🎯 Goal
Implement multi-tenant isolation by reading a declarative `tenants.json` manifest, creating independent isolate pools per tenant with dedicated memory limits, and routing incoming requests dynamically by tenant ID (`/render/:tenant_id`).

---

## 💡 Concepts

### 1. Multi-Tenant Architecture
Instead of one global pool, the gateway manages a map of tenant deployments:
```
HashMap<TenantId, IsolatePool>
```
Each tenant has:
- Its own JavaScript bundle file.
- Its own configured `max_heap_mb`.
- Its own `pool_size` (worker count).
- Complete isolation: If the `checkout-team` bundle crashes or leaks memory, the `marketplace-team` pool is completely unaffected!

### 2. The Tenant Manifest (`tenants.json`)
```json
{
  "marketplace": {
    "bundle_path": "web/dist/marketplace.js",
    "max_heap_mb": 64,
    "pool_size": 2
  },
  "checkout": {
    "bundle_path": "web/dist/checkout.js",
    "max_heap_mb": 32,
    "pool_size": 2
  }
}
```

---

## 📝 Implementation

### 1. Data Structures (`src/main.rs`)
```rust
use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct TenantConfig {
    pub bundle_path: String,
    pub max_heap_mb: usize,
    pub pool_size: usize,
}

pub struct TenantRegistry {
    tenants: HashMap<String, TenantConfig>,
}

impl TenantRegistry {
    pub fn load_from_file(path: &str) -> Self {
        let content = std::fs::read_to_string(path).expect("Could not read tenants.json");
        let tenants: HashMap<String, TenantConfig> = serde_json::from_str(&content)
            .expect("Invalid tenants.json syntax");
        Self { tenants }
    }
}
```

### 2. Multi-Pool Manager
```rust
pub struct DeploymentManager {
    pools: HashMap<String, IsolatePool>,
}

impl DeploymentManager {
    pub fn init(registry: &TenantRegistry) -> Self {
        let mut pools = HashMap::new();

        for (tenant_id, config) in &registry.tenants {
            println!("📦 Initializing pool for tenant: '{}'", tenant_id);
            let bundle_source = std::fs::read_to_string(&config.bundle_path)
                .unwrap_or_else(|_| panic!("Failed to read bundle for {}", tenant_id));

            // Leak to 'static for thread usage in this prototype
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

### 3. Dynamic HTTP Handler
```rust
async fn render_tenant_handler(
    Path(tenant_id): Path<String>,
    State(manager): State<Arc<DeploymentManager>>,
) -> impl IntoResponse {
    match manager.get_pool(&tenant_id) {
        Some(pool) => {
            let (reply_tx, reply_rx) = oneshot::channel();
            let props = r#"{"title": "Multi-Tenant SSR"}"#.to_string();
            pool.job_tx.send(RenderJob { props, reply_tx }).unwrap();

            match reply_rx.await.unwrap() {
                Ok(html) => Html(html).into_response(),
                Err(err) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
            }
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            format!("Tenant '{}' not found", tenant_id),
        ).into_response(),
    }
}
```

---

## 🔍 Verification
Run:
```bash
cargo run
```

Test requests:
```bash
# Valid tenant 1
curl http://localhost:8787/render/marketplace

# Valid tenant 2
curl http://localhost:8787/render/checkout

# Unknown tenant
curl -i http://localhost:8787/render/unknown
# Returns HTTP 404: Tenant 'unknown' not found
```

---

## ✅ Checklist
- [ ] Parse declarative JSON tenant manifests.
- [ ] Spin up dedicated isolate pools per tenant.
- [ ] Ensure strict failure isolation across tenants.
