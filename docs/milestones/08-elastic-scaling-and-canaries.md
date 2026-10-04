# Milestone 08: Elastic Scaling & Canaries

## 🎯 Goal
Implement the two crowning features of the SSR platform:
1. **Elastic Scaling & Cold Pools (`pool_size: 0`)**: Dynamic worker spawning under backlog with idle worker eviction back to zero.
2. **Sticky Canary Traffic Splitting**: Routing a percentage of users to experimental bundle versions using sticky HTTP cookies with instant rollback capability.

---

## 💡 Concepts

### 1. The Cost of Idle Tenants
In an enterprise with 50+ micro-frontend teams, many internal or low-traffic teams receive few requests per hour. If every tenant maintains a minimum floor of 1–2 warm isolates, memory usage scales linearly with the number of *teams* rather than actual *traffic*.

### 2. Cold Pools (`pool_size: 0`) & Idle Eviction
- A tenant with `pool_size: 0` starts with **zero OS threads** and **zero bytes of V8 heap**.
- When the first request arrives:
  ```rust
  if live_workers == 0 {
      spawn_elastic_worker();
  }
  ```
- Elastic workers use `rx.recv_timeout(Duration::from_secs(30))` instead of an indefinite blocking `recv()`. If 30 seconds elapse without a request, the worker thread cleanly exits and drops its V8 isolate.

### 3. Canary Cohort Routing
When rolling out a new bundle version (`v2`):
- Both `v1` (Active pool) and `v2` (Canary pool) run concurrently.
- Incoming requests check for a `v8i_cohort` cookie:
  - If cookie is `canary` → route to Canary pool.
  - If cookie is missing → roll a random percentage (e.g. 10% canary, 90% stable) and set the `Set-Cookie` header to pin the client to that version.

---

## 📝 Implementation

### 1. Elastic Worker Loop with Idle Eviction
```rust
fn elastic_worker_loop(
    id: usize,
    rx: Receiver<RenderJob>,
    bundle: &str,
    is_elastic: bool,
    idle_timeout: Duration,
) {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // Compile bundle
    let code = v8::String::new(scope, bundle).unwrap();
    v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();

    loop {
        let job = if is_elastic {
            // Elastic workers timeout and exit when idle!
            match rx.recv_timeout(idle_timeout) {
                Ok(j) => j,
                Err(_) => {
                    println!("💤 Elastic worker #{} idle timed out. Evicting isolate.", id);
                    break; // Thread exits, isolate memory is reclaimed!
                }
            }
        } else {
            // Baseline workers wait forever
            match rx.recv() {
                Ok(j) => j,
                Err(_) => break,
            }
        };

        // Execute render
        let global = context.global(scope);
        let render_key = v8::String::new(scope, "render").unwrap();
        let render_fn: v8::Local<v8::Function> = global.get(scope, render_key.into())
            .unwrap()
            .try_into()
            .unwrap();

        let props_str = v8::String::new(scope, &job.props).unwrap();
        let html = render_fn.call(scope, global.into(), &[props_str.into()])
            .map(|v| v.to_rust_string_lossy(scope))
            .ok_or_else(|| "Render error".to_string());

        let _ = job.reply_tx.send(html);
    }
}
```

### 2. Canary Cohort Decision Logic
```rust
pub enum Cohort {
    Stable,
    Canary,
}

pub fn resolve_cohort(cookie_header: Option<&str>, canary_weight_percent: u8) -> (Cohort, bool) {
    // 1. Honor existing cookie
    if let Some(cookies) = cookie_header {
        if cookies.contains("v8i_cohort=canary") {
            return (Cohort::Canary, false);
        }
        if cookies.contains("v8i_cohort=stable") {
            return (Cohort::Stable, false);
        }
    }

    // 2. Roll random number for new visitors
    let roll: u8 = rand::random::<u8>() % 100;
    if roll < canary_weight_percent {
        (Cohort::Canary, true) // true = set cookie header
    } else {
        (Cohort::Stable, true)
    }
}
```

---

## 🔍 Verification & Graduation
Once you reach this milestone:
1. You understand scale-to-zero economics for serverless/multi-tenant platforms.
2. You have built the exact primitives that Walmart's Aurora platform and Cloudflare Workers use for sandboxed JavaScript execution.
3. You can read, debug, and contribute to the parent `ssr-platform` repository with total clarity! 🎓
