//! Verifies the vendored WIT used by the upstream compatibility guest.

use std::path::Path;
use std::process::Command;

const FILES: &[&str] = &["deps/clocks.wit", "deps/filesystem.wit"];

#[test]
fn vendored_files_match_the_locked_wasmtime_wasi_package() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "wasmtime-wasi")
        .unwrap();
    let version = package["version"].as_str().unwrap();
    let manifest = Path::new(package["manifest_path"].as_str().unwrap());
    let source = manifest.parent().unwrap().join("src/p3/wit");
    let vendored = workspace
        .join("wit/upstream")
        .join(format!("wasmtime-wasi-{version}"));

    for relative in FILES {
        assert_eq!(
            std::fs::read(vendored.join(relative)).unwrap(),
            std::fs::read(source.join(relative)).unwrap(),
            "vendored {relative} differs from {}",
            source.display()
        );
    }
}
