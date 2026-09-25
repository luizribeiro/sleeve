//! Builds the example-owned sleeve component.

use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"));
    let component = guest_build::build_component(
        &example.join("guest/Cargo.toml"),
        "custom-policy-guest",
        "custom-policy-sleeve",
    )?;
    println!(
        "cargo::rustc-env=CUSTOM_POLICY_SLEEVE={}",
        component.display()
    );
    println!(
        "cargo::rerun-if-changed={}",
        example.join("guest").display()
    );
    Ok(())
}
