//! Builds the isolated WebAssembly guest workspace for host-side tests.

use std::io;
use std::path::{Path, PathBuf};

#[path = "src/build_support.rs"]
#[allow(dead_code)]
mod build_support;

const GUESTS: &[(&str, &str)] = &[
    ("BYPASS_COMPONENT", "bypass"),
    ("CONCURRENT_EXPORTS_COMPONENT", "concurrent-exports"),
    ("COUNTING_SLEEVE_COMPONENT", "counting-sleeve"),
    ("DENY_SLEEVE_COMPONENT", "deny-sleeve"),
    ("DIRECT_IMPORT_COMPONENT", "direct-import"),
    ("EXTERNAL_POLICY_SLEEVE_COMPONENT", "external-policy-sleeve"),
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
    ("TRACE_POLICY_COMPONENT", "trace-policy-component"),
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
    let guest_target_dir = build_support::auxiliary_target_dir(&out_dir, "guest-build")?;

    let packages = GUESTS
        .iter()
        .map(|(_, package)| *package)
        .collect::<Vec<_>>();
    let components = build_support::build_components(
        &guest_dir.join("Cargo.toml"),
        &guest_target_dir,
        &packages,
    )?;
    let wasip2_target_dir = guest_target_dir.join("wasip2-experiment");
    build_wasip2_experiments(&guest_dir.join("Cargo.toml"), &wasip2_target_dir)?;

    for ((variable, _), component) in GUESTS.iter().zip(&components) {
        emit_guest_path(variable, component);
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
    build_support::build_packages(
        manifest,
        target_dir,
        "wasm32-wasip2",
        &["note-summary", "passthrough-sleeve"],
    )
}

fn required_var(name: &str) -> io::Result<std::ffi::OsString> {
    std::env::var_os(name).ok_or_else(|| io::Error::other(format!("{name} is not set")))
}

fn emit_guest_path(variable: &str, component: &Path) {
    println!("cargo::rustc-env={variable}={}", component.display());
}
