use aegis_ssr::{init_v8_platform, isolate_runner::run_script};

fn main() {
    // 1. Initialize V8 platform once
    init_v8_platform();
    println!("✅ V8 platform initialized.");

    // 2. Real JavaScript computation
    let js_code = r#"
        const engine = "Aegis";
        const version = 1;
        const features = ["Memory Isolation", "Watchdogs", "Sub-millisecond Renders"];

        const summary = features.map((f, i) => `${i + 1}. ${f}`).join(" | ");

        `🛡️ [${engine} v${version}] Active features: ${summary} (Calculation: 10 * 42 = ${10 * 42})`
    "#;

    // 3. Run through isolate_runner harness
    match run_script(js_code) {
        Ok(output) => println!("🎉 JavaScript returned:\n{}", output),
        Err(err) => eprintln!("❌ Error: {}", err),
    }
}