//! Builds the isolated WebAssembly guest workspace for host-side tests.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const GUESTS: &[(&str, &str)] = &[
    ("BYPASS_COMPONENT", "bypass"),
    ("COUNTING_SLEEVE_COMPONENT", "counting-sleeve"),
    ("DENY_SLEEVE_COMPONENT", "deny-sleeve"),
    ("DIRECT_IMPORT_COMPONENT", "direct-import"),
    ("FILESYSTEM_BYPASS_COMPONENT", "filesystem-bypass"),
    ("FILESYSTEM_FORWARDER_COMPONENT", "filesystem-forwarder"),
    ("FILESYSTEM_READER_COMPONENT", "filesystem-reader"),
    ("FILESYSTEM_UPSTREAM_COMPONENT", "filesystem-upstream"),
    ("FILE_SCENARIOS_COMPONENT", "file-scenarios-plugin"),
    ("FILE_IFC_SLEEVE_COMPONENT", "file-ifc-sleeve"),
    ("HTTP_BYPASS_COMPONENT", "http-bypass"),
    ("HTTP_SCENARIOS_COMPONENT", "http-scenarios-plugin"),
    ("NOTE_SUMMARY_COMPONENT", "note-summary"),
    ("PASSTHROUGH_SLEEVE_COMPONENT", "passthrough-sleeve"),
    ("PLATFORM_EXPORT_COMPONENT", "platform-export"),
    ("PLATFORM_IMPORT_COMPONENT", "platform-import"),
    ("READ_MANY_COMPONENT", "read-many"),
    ("STREAM_RELAY_PLUGIN_COMPONENT", "stream-relay-plugin"),
    ("STREAM_RELAY_SLEEVE_COMPONENT", "stream-relay-sleeve"),
    ("SMOKE_COMPONENT", "smoke"),
    ("TRACE_SLEEVE_COMPONENT", "trace-sleeve"),
    ("IFC_SLEEVE_COMPONENT", "ifc-sleeve"),
    ("TRACE_IFC_SLEEVE_COMPONENT", "trace-ifc-sleeve"),
    ("TRACE_FILE_IFC_SLEEVE_COMPONENT", "trace-file-ifc-sleeve"),
    ("TRAP_AFTER_READ_COMPONENT", "trap-after-read"),
];

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
    let wasip2_target_dir = guest_target_dir.join("wasip2-experiment");
    build_wasip2_experiments(&guest_dir.join("Cargo.toml"), &wasip2_target_dir)?;

    let release_dir = guest_target_dir.join("wasm32-unknown-unknown/release");
    let component_dir = guest_target_dir.join("components");
    std::fs::create_dir_all(&component_dir)?;
    for &(variable, package) in GUESTS {
        let module = release_dir.join(package.replace('-', "_") + ".wasm");
        let component = component_dir.join(package.to_owned() + ".wasm");
        componentize(&module, &component)?;
        emit_guest_path(variable, &component);
    }
    emit_guest_path(
        "WASIP2_NOTE_SUMMARY_COMPONENT",
        &wasip2_target_dir.join("wasm32-wasip2/release/note_summary.wasm"),
    );
    emit_guest_path(
        "WASIP2_PASSTHROUGH_SLEEVE_COMPONENT",
        &wasip2_target_dir.join("wasm32-wasip2/release/passthrough_sleeve.wasm"),
    );
    for path in [
        guest_dir.as_path(),
        &repository.join("crates/sleeve-core"),
        &repository.join("support/policies"),
        &repository.join("wit"),
    ] {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    Ok(())
}

fn build_wasip2_experiments(manifest: &Path, target_dir: &Path) -> io::Result<()> {
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
        OsStr::new("-p"),
        OsStr::new("note-summary"),
        OsStr::new("-p"),
        OsStr::new("passthrough-sleeve"),
    ]);
    clear_cargo_environment(&mut command);
    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "wasip2 experiment build failed with {status}"
        )));
    }
    Ok(())
}

fn build_guest_workspace(manifest: &Path, target_dir: &Path) -> io::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args([
        OsStr::new("build"),
        OsStr::new("--release"),
        OsStr::new("--target"),
        OsStr::new("wasm32-unknown-unknown"),
        OsStr::new("--manifest-path"),
        manifest.as_os_str(),
        OsStr::new("--target-dir"),
        target_dir.as_os_str(),
        OsStr::new("--locked"),
    ]);
    clear_cargo_environment(&mut command);

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "guest build failed with {status}"
        )));
    }
    Ok(())
}

fn clear_cargo_environment(command: &mut Command) {
    for (key, _) in env::vars_os() {
        if key.to_string_lossy().starts_with("CARGO_") || key == "RUSTFLAGS" {
            command.env_remove(key);
        }
    }
}

fn required_var(name: &str) -> io::Result<std::ffi::OsString> {
    env::var_os(name).ok_or_else(|| io::Error::other(format!("{name} is not set")))
}

fn componentize(module: &Path, component: &Path) -> io::Result<()> {
    let status = Command::new("wasm-tools")
        .args([OsStr::new("component"), OsStr::new("new")])
        .arg(module)
        .arg("-o")
        .arg(component)
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "component creation failed with {status}"
        )));
    }
    Ok(())
}

fn emit_guest_path(variable: &str, component: &Path) {
    println!("cargo::rustc-env={variable}={}", component.display());
}
