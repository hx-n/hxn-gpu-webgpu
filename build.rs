// A threaded Emscripten build uses the browser agent, which the web page starts from C.
fn main() {
    println!("cargo:rerun-if-changed=c/agent.c");
    let emscripten = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("emscripten");
    let atomics = std::env::var("CARGO_CFG_TARGET_FEATURE")
        .map(|features| features.split(',').any(|feature| feature == "atomics"))
        .unwrap_or(false);
    if emscripten && atomics {
        cc::Build::new()
            .file("c/agent.c")
            .flag("-pthread")
            .compile("gpu_webgpu_agent");
    }
}
