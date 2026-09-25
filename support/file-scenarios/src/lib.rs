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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Case {
    /// Human-readable scenario identifier.
    pub name: &'static str,
    /// Plugin behavior to invoke.
    pub scenario: Scenario,
    /// Expected plugin return value.
    pub expected: &'static str,
    /// Whether the trace must contain a policy denial.
    pub policy_denial: bool,
}

/// Scenarios exercised under every IFC sleeve variant.
pub const CASES: &[Case] = &[
    Case {
        name: "open-public-then-read-secret",
        scenario: Scenario::OpenPublicThenReadSecret,
        expected: "read refused",
        policy_denial: true,
    },
    Case {
        name: "close-public-then-read-secret",
        scenario: Scenario::ClosePublicThenReadSecret,
        expected: "classified",
        policy_denial: false,
    },
    Case {
        name: "read-secret-then-write-secret",
        scenario: Scenario::ReadSecretThenWriteSecret,
        expected: "secret write allowed",
        policy_denial: false,
    },
    Case {
        name: "read-secret-then-write-public",
        scenario: Scenario::ReadSecretThenWritePublic,
        expected: "write refused",
        policy_denial: true,
    },
    Case {
        name: "parent-escape",
        scenario: Scenario::ParentEscape,
        expected: "escape refused",
        policy_denial: false,
    },
    Case {
        name: "absolute-escape",
        scenario: Scenario::AbsoluteEscape,
        expected: "escape refused",
        policy_denial: false,
    },
    Case {
        name: "cross-preopen-symlink",
        scenario: Scenario::CrossPreopenSymlink,
        expected: "escape refused",
        policy_denial: false,
    },
    Case {
        name: "truncate-only-after-secret",
        scenario: Scenario::TruncateOnlyAfterSecret,
        expected: "write refused",
        policy_denial: true,
    },
    Case {
        name: "write-only-after-secret",
        scenario: Scenario::WriteOnlyAfterSecret,
        expected: "write refused",
        policy_denial: true,
    },
];
