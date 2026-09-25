use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locates a named auxiliary target directory beside Cargo's main target tree.
///
/// # Errors
///
/// Returns an error when `out_dir` is not inside Cargo's target directory.
pub fn auxiliary_target_dir(out_dir: &Path, name: &str) -> io::Result<PathBuf> {
    out_dir
        .ancestors()
        .nth(4)
        .map(|target| target.join(name))
        .ok_or_else(|| io::Error::other("OUT_DIR is not inside Cargo's target directory"))
}

/// Builds named `wasm32-unknown-unknown` packages and componentizes each output.
///
/// # Errors
///
/// Returns an error when Cargo, filesystem setup, or componentization fails.
pub fn build_components(
    manifest: &Path,
    target_dir: &Path,
    packages: &[&str],
) -> io::Result<Vec<PathBuf>> {
    build_packages(manifest, target_dir, "wasm32-unknown-unknown", packages)?;
    let release = target_dir.join("wasm32-unknown-unknown/release");
    let components = target_dir.join("components");
    std::fs::create_dir_all(&components)?;
    packages
        .iter()
        .map(|package| {
            let module = release.join(package.replace('-', "_") + ".wasm");
            let component = components.join(format!("{package}.wasm"));
            componentize(&module, &component)?;
            Ok(component)
        })
        .collect()
}

/// Builds and componentizes one package beside Cargo's main target tree.
///
/// # Errors
///
/// Returns an error when `OUT_DIR` is missing or the guest cannot be built.
pub fn build_component(manifest: &Path, target_name: &str, package: &str) -> io::Result<PathBuf> {
    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("OUT_DIR is not set"))?;
    let target = auxiliary_target_dir(&out_dir, target_name)?;
    let mut components = build_components(manifest, &target, &[package])?;
    components
        .pop()
        .ok_or_else(|| io::Error::other("guest component was not built"))
}

/// Builds selected packages for a WebAssembly target in an isolated target tree.
///
/// # Errors
///
/// Returns an error when Cargo cannot start or the build fails.
pub fn build_packages(
    manifest: &Path,
    target_dir: &Path,
    target: &str,
    packages: &[&str],
) -> io::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args([
        OsStr::new("build"),
        OsStr::new("--release"),
        OsStr::new("--target"),
        OsStr::new(target),
        OsStr::new("--manifest-path"),
        manifest.as_os_str(),
        OsStr::new("--target-dir"),
        target_dir.as_os_str(),
        OsStr::new("--locked"),
    ]);
    for package in packages {
        command.arg("-p").arg(package);
    }
    clear_cargo_environment(&mut command);
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "guest build failed with {status}"
        )))
    }
}

fn clear_cargo_environment(command: &mut Command) {
    for (key, _) in env::vars_os() {
        if key.to_string_lossy().starts_with("CARGO_") || key == "RUSTFLAGS" {
            command.env_remove(key);
        }
    }
}

fn componentize(module: &Path, component: &Path) -> io::Result<()> {
    let status = Command::new("wasm-tools")
        .args([OsStr::new("component"), OsStr::new("new")])
        .arg(module)
        .arg("-o")
        .arg(component)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "component creation failed with {status}"
        )))
    }
}
