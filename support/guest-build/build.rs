//! Builds the isolated WebAssembly guest workspace for host-side tests.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const GUESTS: &[(&str, &str)] = &[("SMOKE_COMPONENT", "smoke")];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = PathBuf::from(required_var("CARGO_MANIFEST_DIR")?);
    let repository = crate_dir.join("../..");
    let guest_dir = repository.join("guests");
    let out_dir = PathBuf::from(required_var("OUT_DIR")?);
    let main_target_dir = out_dir
        .ancestors()
        .nth(4)
        .ok_or_else(|| io::Error::other("OUT_DIR is not inside Cargo's target directory"))?;
    let guest_target_dir = main_target_dir.join("guest-build");

    build_guest_workspace(&guest_dir.join("Cargo.toml"), &guest_target_dir)?;

    let release_dir = guest_target_dir.join("wasm32-wasip2/release");
    for &(variable, package) in GUESTS {
        emit_guest_path(variable, package, &release_dir);
    }
    println!("cargo::rerun-if-changed={}", guest_dir.display());
    Ok(())
}

fn build_guest_workspace(manifest: &Path, target_dir: &Path) -> io::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args([
        OsStr::new("build"),
        OsStr::new("--release"),
        OsStr::new("--target"),
        OsStr::new("wasm32-wasip2"),
        OsStr::new("--manifest-path"),
        manifest.as_os_str(),
        OsStr::new("--target-dir"),
        target_dir.as_os_str(),
        OsStr::new("--locked"),
    ]);
    for (key, _) in env::vars_os() {
        if key.to_string_lossy().starts_with("CARGO_") || key == "RUSTFLAGS" {
            command.env_remove(key);
        }
    }

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "guest build failed with {status}"
        )));
    }
    Ok(())
}

fn required_var(name: &str) -> io::Result<std::ffi::OsString> {
    env::var_os(name).ok_or_else(|| io::Error::other(format!("{name} is not set")))
}

fn emit_guest_path(variable: &str, package: &str, release_dir: &Path) {
    let artifact = package.replace('-', "_") + ".wasm";
    println!(
        "cargo::rustc-env={variable}={}",
        release_dir.join(artifact).display()
    );
}
