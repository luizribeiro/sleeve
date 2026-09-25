//! Reusable HTTP policy scenarios for native and JavaScript hosts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Selects the behavior exercised by the shared plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Scenario {
    /// Reads a secret note before attempting an allowed fetch.
    ReadThenFetch,
    /// Fetches an allowed origin before reading a secret note.
    FetchThenRead,
    /// Reads a secret note while a request body writer remains open.
    ReadWithOpenBody,
    /// Sends and closes a body before reading a secret note.
    CloseBodyThenRead,
    /// Sends a request body larger than the configured limit.
    BodyOverLimit,
    /// Fetches an origin absent from the allowlist.
    FetchBlockedOrigin,
    /// Fetches the allowed origin with an uppercase host spelling.
    FetchNormalizedOrigin,
    /// Sends an invalid request, then reads a secret note.
    InvalidRequestThenRead,
}

/// Identifies the rule expected to decide a scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rule {
    /// Opening a public writable channel at secret is refused.
    ChannelOpenVeto,
    /// An open writable channel prevents a secret-label raise.
    OpenChannelRaise,
    /// The normalized origin must be allowlisted.
    OriginAllowlist,
    /// The configured request-body byte limit is enforced.
    BodyLimit,
    /// Request validation rejects an incomplete origin and cleans up channels.
    OriginValidation,
    /// No refusal applies to the operation ordering.
    Allowed,
}

/// Chooses which authority spelling the host passes to the plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Authority {
    /// The exact allowlisted authority.
    Allowed,
    /// An authority absent from the allowlist.
    Blocked,
    /// The allowed authority with its hostname uppercased.
    UppercaseAllowed,
}

/// Expected result of a scenario invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expected {
    /// The plugin returns this value normally.
    Returned(&'static str),
    /// Policy denial traps the plugin export.
    Trapped,
}

/// One reusable plugin scenario and its expected decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Case {
    /// Human-readable scenario identifier.
    pub name: &'static str,
    /// Plugin behavior to invoke.
    pub scenario: Scenario,
    /// Authority spelling to pass.
    pub authority: Authority,
    /// Request body size in bytes.
    pub body_size: u32,
    /// Expected plugin outcome.
    pub expected: Expected,
    /// Policy or validation rule that decides the case.
    pub rule: Rule,
}

/// Scenarios exercised under every IFC sleeve variant.
pub const CASES: &[Case] = &[
    Case {
        name: "read-then-fetch",
        scenario: Scenario::ReadThenFetch,
        authority: Authority::Allowed,
        body_size: 0,
        expected: Expected::Trapped,
        rule: Rule::ChannelOpenVeto,
    },
    Case {
        name: "fetch-then-read",
        scenario: Scenario::FetchThenRead,
        authority: Authority::Allowed,
        body_size: 0,
        expected: Expected::Returned("200 classified"),
        rule: Rule::Allowed,
    },
    Case {
        name: "read-with-open-body",
        scenario: Scenario::ReadWithOpenBody,
        authority: Authority::Allowed,
        body_size: 4,
        expected: Expected::Trapped,
        rule: Rule::OpenChannelRaise,
    },
    Case {
        name: "close-body-then-read",
        scenario: Scenario::CloseBodyThenRead,
        authority: Authority::Allowed,
        body_size: 4,
        expected: Expected::Returned("200 classified"),
        rule: Rule::Allowed,
    },
    Case {
        name: "body-over-limit",
        scenario: Scenario::BodyOverLimit,
        authority: Authority::Allowed,
        body_size: 9,
        expected: Expected::Returned("body too large"),
        rule: Rule::BodyLimit,
    },
    Case {
        name: "blocked-origin",
        scenario: Scenario::FetchBlockedOrigin,
        authority: Authority::Blocked,
        body_size: 0,
        expected: Expected::Returned("request denied"),
        rule: Rule::OriginAllowlist,
    },
    Case {
        name: "normalized-origin",
        scenario: Scenario::FetchNormalizedOrigin,
        authority: Authority::UppercaseAllowed,
        body_size: 0,
        expected: Expected::Returned("200"),
        rule: Rule::Allowed,
    },
    Case {
        name: "invalid-request-then-read",
        scenario: Scenario::InvalidRequestThenRead,
        authority: Authority::Allowed,
        body_size: 0,
        expected: Expected::Returned("invalid request classified"),
        rule: Rule::OriginValidation,
    },
];
