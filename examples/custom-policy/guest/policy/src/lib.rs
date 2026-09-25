//! A policy written entirely inside the custom-policy example.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use sleeve_core::{
    Call, Decision, Denied, Event, Metadata, Policy, PolicyState, Returned, http::ErrorCode,
};

/// Receives the final per-interface call counts.
pub trait SummaryLog: Send {
    /// Persists one summary line.
    fn log(&mut self, summary: &str);
}

/// Counts calls by interface and refuses the fourth HTTP send.
pub struct CountCalls<L> {
    counts: BTreeMap<String, u32>,
    log: L,
}

impl<L> CountCalls<L> {
    /// Creates a fresh per-invocation counter.
    pub const fn new(log: L) -> Self {
        Self {
            counts: BTreeMap::new(),
            log,
        }
    }
}

impl<L: SummaryLog + 'static> Policy for CountCalls<L> {
    type Frame = ();

    fn observe(&mut self, event: &Event<'_>) {
        match event {
            Event::InvocationStarted(_) => self.counts.clear(),
            Event::InvocationEnded(_) => {
                for (interface, count) in &self.counts {
                    self.log.log(&format!("summary {interface} calls={count}"));
                }
            }
            _ => {}
        }
    }

    fn before(&mut self, _: &PolicyState<'_>, call: &Call<'_>) -> Decision<Self::Frame> {
        let count = self.counts.entry(call.interface.to_string()).or_default();
        *count += 1;
        if call.interface == "wasi:http/client@0.3.0" && call.function == "send" && *count == 4 {
            Decision::Deny(Denied::new(
                "fourth HTTP request refused",
                ErrorCode::HttpRequestDenied,
            ))
        } else {
            Decision::Allow(())
        }
    }

    fn after(&mut self, _: &Call<'_>, (): (), _: &Returned) -> Vec<Metadata> {
        Vec::new()
    }
}
