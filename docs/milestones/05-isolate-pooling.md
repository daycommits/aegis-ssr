# Milestone 05: Isolate Worker Pooling

## 🎯 Goal
Build a multi-threaded worker pool where dedicated OS threads hold pre-warmed V8 isolates with compiled React bundles, processing incoming render jobs via concurrent MPMC channels without isolate recreation overhead.

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
Request 2 ──┼──> [crossbeam channel] ──> Worker Thread 1 (Warm Isolate)
Request 3 ──┘                       ──> Worker Thread 2 (Warm Isolate)
```

---

## 📝 Implementation

### Dependencies (`Cargo.toml`)
```toml
[dependencies]
v8 = "152.2.0"
crossbeam-channel = "0.5"
```

### Code (`src/main.rs`)
```rust
use std::sync::mpsc::{channel as oneshot_channel, Sender as OneshotSender};
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::thread;

struct RenderJob {
    props: String,
    reply_tx: OneshotSender<Result<String, String>>,
}

struct IsolatePool {
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
        self.job_tx.send(RenderJob { props, reply_tx }).unwrap();
        reply_rx.recv().unwrap()
    }
}

fn worker_loop(id: usize, job_rx: Receiver<RenderJob>, bundle_code: &str) {
    // 1. Each worker owns one warm Isolate
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, isolate);

    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    // 2. Pre-compile the bundle ONCE at startup
    let code = v8::String::new(scope, bundle_code).unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    script.run(scope).unwrap();

    println!("👷 Worker #{} ready and warm!", id);

    // 3. Process jobs in a loop
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

fn main() {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    // Mock bundle exposing globalThis.render
    let bundle = r#"
        globalThis.render = (propsJson) => {
            const props = JSON.parse(propsJson);
            return `<div><h1>Hello ${props.user}</h1><p>Items in cart: ${props.cart}</p></div>`;
        };
    "#;

    println!("🚀 Spawning isolate pool of 4 workers...");
    let pool = IsolatePool::new(4, bundle);

    // Give workers a moment to initialize
    thread::sleep(std::time::Duration::from_millis(50));

    println!("\n⚡ Submitting concurrent render jobs:");
    let handles: Vec<_> = (1..=8).map(|i| {
        let pool_ref = pool.job_tx.clone();
        thread::spawn(move || {
            let (tx, rx) = oneshot_channel();
            pool_ref.send(RenderJob {
                props: format!(r#"{{"user": "Customer_{}", "cart": {}}}"#, i, i * 2),
                reply_tx: tx,
            }).unwrap();
            let html = rx.recv().unwrap().unwrap();
            println!("  [Job #{}] -> {}", i, html);
        })
    }).collect();

    for h in handles {
        h.join().unwrap();
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
```text
🚀 Spawning isolate pool of 4 workers...
👷 Worker #0 ready and warm!
👷 Worker #1 ready and warm!
👷 Worker #2 ready and warm!
👷 Worker #3 ready and warm!

⚡ Submitting concurrent render jobs:
  [Job #1] -> <div><h1>Hello Customer_1</h1><p>Items in cart: 2</p></div>
  [Job #2] -> <div><h1>Hello Customer_2</h1><p>Items in cart: 4</p></div>
  ...
```

Notice: Requests are served in parallel across workers with near-zero latency!

---

## ✅ Checklist
- [ ] Understand why thread-per-isolate is the right model for V8.
- [ ] Master the MPMC job queue pattern with crossbeam channels.
- [ ] Verify that pre-warmed isolates reuse their compiled state across requests.
