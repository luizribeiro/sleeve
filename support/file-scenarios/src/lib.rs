//! Reusable filesystem policy scenarios.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Selects the behavior exercised by the filesystem plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Scenario {
    /// Keeps a public writer open while attempting a secret read.
    OpenPublicThenReadSecret,
    /// Closes a public writer before reading a secret file.
    ClosePublicThenReadSecret,
    /// Reads secret data and writes into the secret container.
    ReadSecretThenWriteSecret,
    /// Reads secret data and attempts to write into the public container.
    ReadSecretThenWritePublic,
    /// Attempts parent traversal beneath a held descriptor.
    ParentEscape,
    /// Attempts an absolute path beneath a held descriptor.
    AbsoluteEscape,
    /// Attempts to follow a symlink into a different preopen.
    CrossPreopenSymlink,
    /// Requests truncation without write descriptor rights after a secret read.
    TruncateOnlyAfterSecret,
    /// Requests write descriptor rights without create or truncate after a secret read.
    WriteOnlyAfterSecret,
}

/// Expected result of a filesystem scenario.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
pub struct Case {
    /// Human-readable scenario identifier.
    pub name: String,
    /// Numeric scenario accepted by the plugin export.
    pub input: u8,
    /// Expected plugin return value.
    pub expected: String,
    /// Whether the trace must contain a policy denial.
    pub policy_denial: bool,
}

/// Reads the scenarios exercised under every IFC sleeve variant and host.
///
/// # Errors
///
/// Returns an error if the embedded shared scenario data is invalid.
pub fn cases() -> Result<Vec<Case>, serde_json::Error> {
    serde_json::from_str(include_str!("../scenarios.json"))
}
