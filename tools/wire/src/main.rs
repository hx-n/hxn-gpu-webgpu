//! Generates the WebGPU wire between the Ash backend and the browser agent, from WebGPU's IDL
//! and the canvas surface xgpu appends, both as xgpu pins them. Run from the repository:
//! `cargo run --manifest-path tools/wire/Cargo.toml -- <webgpu.idl> <canvas.idl>`.

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let [webgpu, canvas] = paths.as_slice() else {
        eprintln!("usage: wire <webgpu.idl> <canvas.idl>");
        std::process::exit(2);
    };
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let idl = format!("{}\n{}", read(webgpu), read(canvas));
    let wire = x_idl::wire::wire(&idl).unwrap_or_else(|e| panic!("generating the wire: {e}"));
    std::fs::create_dir_all("src/agent").unwrap();
    std::fs::create_dir_all("page").unwrap();
    std::fs::write("src/agent/wire.rs", wire.rust).unwrap();
    std::fs::write("page/gpu_webgpu_service.mjs", wire.js).unwrap();
}
