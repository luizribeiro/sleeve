//! Runs composed sleeve components through the repository's jco host.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, bail};
use serde::Deserialize;

/// One completed or trapped plugin invocation and its retained audit records.
#[derive(Debug, Deserialize)]
pub struct Attempt {
    /// Shared scenario name.
    pub name: String,
    /// Either `returned` or `trapped`.
    pub status: String,
    /// Plugin result for a returned invocation.
    pub value: Option<String>,
    /// Host diagnostic for a trapped invocation.
    pub error: Option<String>,
    /// Audit records emitted before the invocation finished or trapped.
    pub audit: Vec<String>,
}

/// A verified composition transpiled into a temporary JavaScript module.
pub struct Component {
    _temporary: tempfile::TempDir,
    jco: PathBuf,
    module: PathBuf,
}

impl Component {
    /// Composes the selected pair, verifies its routing, and transpiles it.
    ///
    /// # Errors
    ///
    /// Returns an error when composition, file I/O, or jco transpilation fails.
    pub fn transpile(plugin: &[u8], sleeve: &[u8]) -> anyhow::Result<Self> {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let jco = repository.join("hosts/jco");
        let generated = jco.join("generated");
        std::fs::create_dir_all(&generated)?;
        let temporary = tempfile::tempdir_in(generated)?;
        let component = temporary.path().join("component.wasm");
        let output = temporary.path().join("transpiled");
        let module = output.join("component.js");
        let bytes = sleeve_host::compose(plugin, sleeve, sleeve_host::sleeve_sha256(sleeve))?;
        std::fs::write(&component, bytes)?;
        successful(
            Command::new("node")
                .current_dir(&jco)
                .arg("transpile.js")
                .arg(&component)
                .arg(&output)
                .output()?,
            "jco transpile",
        )?;
        Ok(Self {
            _temporary: temporary,
            jco,
            module,
        })
    }

    /// Runs the HTTP scenarios, optionally selecting one by name.
    ///
    /// # Errors
    ///
    /// Returns an error when Node fails or emits an invalid result.
    pub fn run_http(
        &self,
        scenarios: &Path,
        allowed: &str,
        blocked: &str,
        uppercase_allowed: &str,
        origin: &str,
        selected: Option<&str>,
    ) -> anyhow::Result<Vec<Attempt>> {
        let mut command = Command::new("node");
        command
            .current_dir(&self.jco)
            .arg("suite.js")
            .arg(&self.module)
            .arg(scenarios)
            .args([allowed, blocked, uppercase_allowed, origin]);
        if let Some(selected) = selected {
            command.arg(selected);
        }
        let output = successful(command.output()?, "jco HTTP scenarios")?;
        parse(&output)
    }

    /// Runs every filesystem scenario against temporary labeled preopens.
    ///
    /// # Errors
    ///
    /// Returns an error when Node fails or emits an invalid result.
    pub fn run_files(&self, scenarios: &Path) -> anyhow::Result<Vec<Attempt>> {
        let output = successful(
            Command::new("node")
                .current_dir(&self.jco)
                .arg("suite-file.js")
                .arg(&self.module)
                .arg(scenarios)
                .output()?,
            "jco filesystem scenarios",
        )?;
        parse(&output)
    }
}

fn successful(output: Output, operation: &str) -> anyhow::Result<Output> {
    if !output.status.success() {
        bail!(
            "{operation} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output)
}

fn parse(output: &Output) -> anyhow::Result<Vec<Attempt>> {
    serde_json::from_slice(&output.stdout).context("jco host emitted invalid JSON")
}
