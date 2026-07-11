//! Integration tests for the WASM host, driving the public API end to end.
//! Gated on `bun run modules:wasm` having produced the demo guest + bundle;
//! they self-skip otherwise so a bare `cargo test` stays green.

use std::path::PathBuf;

use luma_module_wasm::{HttpReq, WasmHost, WasmModule};

fn hello_wasm() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../../wasm-modules/hello-wasm/server/target/wasm32-unknown-unknown/release/luma_module_hello_wasm.wasm");
    p
}

fn ping() -> HttpReq {
    HttpReq { method: "GET".into(), path: "/ping".into(), query: String::new(), body: String::new() }
}

#[test]
fn wasm_module_loads_and_serves_http() {
    let wasm = hello_wasm();
    if !wasm.exists() {
        eprintln!("skipping: demo wasm not built ({}); run `bun run modules:wasm`", wasm.display());
        return;
    }
    // Assemble a minimal install dir (module.json + module.wasm).
    let dir = std::env::temp_dir().join("luma-wasm-hello-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&wasm, dir.join("module.wasm")).unwrap();
    std::fs::write(
        dir.join("module.json"),
        br#"{"id":"dev.luma.hellowasm","name":"Hello WASM","version":"0.1.0"}"#,
    )
    .unwrap();

    let module = WasmModule::load(&dir).expect("load runtime module");
    assert_eq!(module.id(), "dev.luma.hellowasm");
    assert!(module.serves_http());
    let resp = module.handle_http(&ping()).expect("handle_http");
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("hello"), "body was {:?}", resp.body);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wasm_host_installs_and_uninstalls_a_bundle() {
    let mut tar = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    tar.push("../../../dist/wasm-modules/dev.luma.hellowasm.tar");
    if !tar.exists() {
        eprintln!("skipping: demo bundle not built ({}); run `bun run modules:wasm`", tar.display());
        return;
    }
    let bytes = std::fs::read(&tar).unwrap();

    let root = std::env::temp_dir().join("luma-wasm-host-test");
    let _ = std::fs::remove_dir_all(&root);
    let mut host = WasmHost::load_all(&root);
    assert!(host.manifests().is_empty());

    let manifest = host.install(&bytes).expect("install bundle");
    assert_eq!(manifest.id, "dev.luma.hellowasm");
    assert_eq!(host.manifests().len(), 1);
    assert!(host.icon("dev.luma.hellowasm").is_some());
    let resp = host.handle_http("dev.luma.hellowasm", &ping()).expect("proxy ping");
    assert_eq!(resp.status, 200);

    // Reloading from disk finds it (survives restart).
    let reloaded = WasmHost::load_all(&root);
    assert_eq!(reloaded.manifests().len(), 1);

    host.uninstall("dev.luma.hellowasm").expect("uninstall");
    assert!(host.manifests().is_empty());
    assert!(!root.join("dev.luma.hellowasm").exists());

    let _ = std::fs::remove_dir_all(&root);
}
