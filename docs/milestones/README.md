# Aegis-SSR — Learning Milestones Index

Welcome to **Aegis-SSR** (🛡️). This curriculum is designed to teach you **`ssr-platform`** by building it module-for-module from scratch. 

From **Milestone 1**, code is organized into the exact same files and folders used in `ssr-platform`.

---

## 🗺️ Milestone Roadmap & Module Mapping

| Milestone | Title | Exact Files Introduced | Role in `ssr-platform` |
| :---: | :--- | :--- | :--- |
| [**01**](./01-hello-v8.md) | **Hello V8 & Isolate Harness** | `src/lib.rs`<br>`src/isolate_runner.rs`<br>`src/main.rs` | V8 platform init & isolate execution harness |
| [**02**](./02-host-shims.md) | **Host Shims & Native Console** | `src/shims.rs`<br>`src/isolate_runner.rs` (`bind_console`) | Polyfills prelude & native FFI console proxy |
| [**03**](./03-first-react-ssr.md) | **React SSR & Bundle Loader** | `web-bundle/`<br>`src/bundle.rs` | React 19 esbuild bundle & script assembly |
| [**04**](./04-guardrails-and-limits.md) | **Guardrails & Limits** | `src/isolate_runner.rs` (`TenantSpec`) | Watchdog timeouts & heap memory guards |
| [**05**](./05-isolate-pooling.md) | **Isolate Worker Pooling** | `src/isolate_pool.rs` | Multi-threaded MPMC worker pool |
| [**06**](./06-http-gateway.md) | **Axum HTTP Gateway** | `src/bin/gateway.rs` | Production Axum server (`/health`, `/render`) |
| [**07**](./07-multi-tenancy.md) | **Multi-Tenancy & Manifest** | `tenants.json`<br>`src/tenant_registry.rs`<br>`src/deployment.rs` | Tenant manifest & multi-pool orchestrator |
| [**08**](./08-elastic-scaling-and-canaries.md) | **Elastic Scaling & Canaries** | `src/cohort.rs` | Sticky canary cookies & scale-to-zero |

---

## 📁 The Target File Tree (Matching `ssr-platform`)

```text
aegis-ssr/
├── tenants.json                  <-- Declarative tenant manifest
├── docs/
│   └── milestones/               <-- Curriculum & guides
├── web-bundle/                   <-- React applications
│   ├── package.json
│   ├── src/entry.jsx
│   └── dist/bundle.js            <-- Compiled JS bundle
├── src/
│   ├── lib.rs                    <-- Library root (exports all modules)
│   ├── shims.rs                  <-- Web API polyfills (MessageChannel, TextEncoder)
│   ├── bundle.rs                 <-- Script assembler (combines shims + bundle + entry)
│   ├── isolate_runner.rs         <-- V8 execution harness, native console, watchdog
│   ├── isolate_pool.rs           <-- Multi-threaded MPMC isolate worker pool
│   ├── tenant_registry.rs        <-- Manifest loader (tenants.json)
│   ├── deployment.rs             <-- DeploymentManager managing active & canary pools
│   ├── cohort.rs                 <-- Sticky cookie canary routing (v8i_cohort)
│   ├── main.rs                   <-- Standalone CLI test harness (`cargo run`)
│   └── bin/
│       └── gateway.rs            <-- Production HTTP server (`cargo run --bin gateway`)
├── Cargo.lock
└── Cargo.toml
```

---

## 📌 How to Follow This Guide
1. Follow each milestone sequentially.
2. Read the explanation of **why** the module exists in `ssr-platform`.
3. Add the files to your repository.
4. Run `cargo run` (or `cargo run --bin gateway`) to verify.
