# Milestone 05: Isolate Worker Pooling (`src/isolate_pool.rs`)

## 🎯 Goal
Build a dedicated worker pool module in **`src/isolate_pool.rs`** (matching `ssr-platform`), where OS worker threads maintain warm V8 isolates with compiled JavaScript bundles, processing jobs concurrently over MPMC `crossbeam-channel` queues.

---

## 📁 File Structure Introduced
```text
aegis-ssr/
├── src/
│   ├── lib.rs              <-- Library root (exports isolate_pool)
│   ├── shims.rs            <-- Browser polyfills
│   ├── isolate_runner.rs   <-- Execution engine & guardrails
│   ├── isolate_pool.rs     <-- 🌟 NEW: Multi-threaded isolate pool
│   └── main.rs             <-- Test harness submitting concurrent jobs
├── web-bundle/
│   └── dist/bundle.js
├── Cargo.lock
└── Cargo.toml
```

---

## 💡 Concepts

### 1. Why Isolate Pooling?
Creating a fresh V8 isolate, parsing shims, and compiling a React bundle takes ~5–15ms per request. Under high traffic, this adds unacceptable latency.
By maintaining a pool of worker threads, each thread keeps a **warm isolate** ready in memory. Render requests take **< 1ms** because the bundle is already compiled and waiting.

### 2. Why OS Threads instead of Tokio Async Tasks?
V8 isolates are **not `Send`** and their execution is completely **CPU-bound** (blocking). If you run V8 inside a standard `tokio::spawn` task, it blocks Tokio's async worker threads and starves other I/O operations. Therefore, dedicated OS worker threads (`std::thread::spawn`) are the industry-standard architecture for embedded V8 engines.

### 3. Multi-Producer Multi-Consumer (MPMC) Queue
We use `crossbeam-channel` so multiple requests can be queued, and whichever worker thread becomes free immediately pulls the next render job.

```
Request 1 ──┐
Request 2 ──┼──> [crossbeam MPMC channel] ──> Worker Thread 1 (Warm Isolate)
Request 3 ──┘                             ──> Worker Thread 2 (Warm Isolate)
```

---

## 📝 Implementation

### 1. Update `Cargo.toml`
Add `crossbeam-channel`:

```toml
[package]
name = "aegis-ssr"
version = "0.1.0"
edition = "2024"

[dependencies]
v8 = "152.2.0"
crossbeam-channel = "0.5"
```

---

### 2. Update `src/lib.rs`
Export the new module:

```rust
pub mod bundle;
pub mod isolate_pool;
pub mod isolate_runner;
pub mod shims;
```

---

### 3. Create `src/isolate_pool.rs`
Create `src/isolate_pool.rs` matching `ssr-platform/src/isolate_pool.rs`:

```rust
use std::sync::mpsc::{channel as oneshot_channel, Sender as OneshotSender};
use std::thread;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crate::shims::HOST_API_SHIM_PRELUDE;

pub struct RenderJob {
    pub props: String,
    pub reply_tx: OneshotSender<Result<String, String>>,
}

pub struct IsolatePool {
    job_tx: Sender<RenderJob>,
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

    pub fn render(&self, props: String) -> Result<String, String> {
        let (reply_tx, reply_rx) = oneshot_channel();
        self.job_tx.send(RenderJob { props, reply_tx }).map_err(|e| e.to_string())?;
        reply_rx.recv().map_err(|e| e.to_string())?
    }
}

fn worker_loop(id: usize, job_rx: Receiver<RenderJob>, bundle_code: &str) {
    // 1. Initialize warm isolate for this thread
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 2. Pre-evaluate shims and bundle ONCE at worker startup
    let shim_code = v8::String::new(scope, HOST_API_SHIM_PRELUDE).unwrap();
    v8::Script::compile(scope, shim_code, None).unwrap().run(scope).unwrap();

    let code = v8::String::new(scope, bundle_code).unwrap();
    v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();

    println!("👷 Worker #{} ready and warm!", id);

    // 3. Keep serving requests from the crossbeam channel
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

### 4. Update `src/main.rs`
Demonstrate concurrent rendering using the pool:

```rust
use aegis_ssr::{isolate_pool::IsolatePool, isolate_runner::init_v8_once};
use std::fs;
use std::thread;

fn main() {
    init_v8_once();

    let bundle_source = fs::read_to_string("web-bundle/dist/bundle.js")
        .expect("Please build bundle first: (cd web-bundle && npm run build)");

    // Leak to static lifetime so worker threads can read it
    let bundle_static: &'static str = Box::leak(bundle_source.into_boxed_str());

    println!("🚀 Initializing IsolatePool (4 warm workers)...");
    let pool = IsolatePool::new(4, bundle_static);

    thread::sleep(std::time::Duration::from_millis(50));

    println!("\n⚡ Submitting concurrent render jobs across worker pool:");
    let handles: Vec<_> = (1..=6).map(|i| {
        let pool_ref = &pool;
        thread::spawn(move || {
            let props = format!(r#"{{"title": "Store #{}", "user": "Customer_{}"}}"#, i, i);
            let html = pool_ref.render(props).unwrap();
            println!("  [Job #{}] -> Rendered HTML length: {} bytes", i, html.len());
        })
    }).collect();

    for h in handles {
        h.join().unwrap();
    }

    println!("\n✅ All concurrent render jobs completed successfully!");
}
```

---

## 🔍 Verification
Run:
```bash
cargo run
```

Expected output:
```text
🚀 Initializing IsolatePool (4 warm workers)...
👷 Worker #0 ready and warm!
👷 Worker #1 ready and warm!
👷 Worker #2 ready and warm!
👷 Worker #3 ready and warm!

⚡ Submitting concurrent render jobs across worker pool:
  [Job #1] -> Rendered HTML length: 228 bytes
  [Job #2] -> Rendered HTML length: 228 bytes
  ...
✅ All concurrent render jobs completed successfully!
```

---

## ✅ Checklist
- [ ] Created `src/isolate_pool.rs` matching `ssr-platform`.
- [ ] Preserved warm isolates on dedicated OS threads.
- [ ] Dispatched concurrent jobs across threads via `crossbeam-channel`.
