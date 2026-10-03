# Aegis-SSR — Learning Milestones Index

Welcome to **Aegis-SSR** (🛡️), an educational, from-scratch implementation of a multi-tenant Server-Side Rendering (SSR) platform using Rust and Google's V8 JavaScript engine (V8 152+).

This folder contains the complete step-by-step curriculum for building the engine from zero.

---

## 🗺️ Milestone Roadmap

| Milestone | Title | Status | Core Concept Learned |
| :---: | :--- | :---: | :--- |
| [**01**](./01-hello-v8.md) | **Hello V8** | ✅ Completed | V8 Platform initialization, `Isolate`, `PinnedRef`, `v8::scope!`, `ContextScope` |
| [**02**](./02-host-shims.md) | **Host Shims & JS Globals** | 🟡 Next Up | Exposing Rust callbacks (`FunctionCallbackArguments`), injecting `console.log` into `globalThis` |
| [**03**](./03-first-react-ssr.md) | **First React SSR** | ⏳ Planned | Bundling React 19 with `esbuild`, loading bundles into V8, executing `renderToString` |
| [**04**](./04-guardrails-and-limits.md) | **Guardrails & Limits** | ⏳ Planned | Wall-clock Watchdog threads, V8 near-heap-limit callbacks, crash isolation via `catch_unwind` |
| [**05**](./05-isolate-pooling.md) | **Isolate Worker Pooling** | ⏳ Planned | Dedicated OS threads, crossbeam MPMC job queues, warm isolate reuse, isolate retirement |
| [**06**](./06-http-gateway.md) | **Axum HTTP Gateway** | ⏳ Planned | Asynchronous HTTP endpoints (`GET /render/:tenant`), request tracing headers, queue admission |
| [**07**](./07-multi-tenancy.md) | **Multi-Tenancy & Manifest** | ⏳ Planned | Dynamic tenant registry (`tenants.json`), per-tenant resource ceilings, bundle hot-reloading |
| [**08**](./08-elastic-scaling-and-canaries.md) | **Elastic Scaling & Canaries** | ⏳ Planned | Scale-to-zero cold pools (`pool_size: 0`), queue-driven elasticity, sticky cookie canary routing |

---

## 🏗️ Architectural Progression

Each milestone adds a distinct system layer:

```
[Milestone 1: V8 Core]
        │
[Milestone 2: Host Shims] (console, fetch shims)
        │
[Milestone 3: React SSR] (esbuild + ReactDOMServer)
        │
[Milestone 4: Guardrails] (Heap limit + Watchdog thread)
        │
[Milestone 5: Worker Pool] (OS Threads + Crossbeam MPMC)
        │
[Milestone 6: HTTP Gateway] (Axum + Tokio)
        │
[Milestone 7: Multi-Tenancy] (tenants.json + dynamic loading)
        │
[Milestone 8: Production Polish] (Scale-to-zero + Canaries)
```

---

## 📌 How to Follow This Guide
1. Follow each milestone in order.
2. Read the explanation of **why** the feature is needed.
3. Review the code changes.
4. Copy the code into your `src/` files and run `cargo run` or `cargo test`.
5. Check off the milestone verification checklist before moving to the next.
