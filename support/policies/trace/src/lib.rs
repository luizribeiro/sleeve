//! A policy that writes each neutral event to an audit sink immediately.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use sleeve_core::{
    Call, ChannelOpened, Decision, Event, InvocationOutcome, Metadata, Policy, PolicyState,
    ReturnStatus, Returned,
};

/// Receives one already-formatted audit record at a time.
pub trait AuditLog: Send {
    /// Persists a record before control returns to untrusted code.
    fn log(&mut self, event: &str);
}

/// Emits one audit record for every event observed by the policy chain.
pub struct Trace<L> {
    log: L,
}

impl<L> Trace<L> {
    /// Creates a tracing policy backed by `log`.
    pub const fn new(log: L) -> Self {
        Self { log }
    }

    /// Returns the underlying sink after policy execution.
    pub fn into_inner(self) -> L {
        self.log
    }
}

impl<L: AuditLog + 'static> Policy for Trace<L> {
    type Frame = ();

    fn observe(&mut self, event: &Event<'_>) {
        if let Some(record) = format_observation(event) {
            self.log.log(&record);
        }
    }

    fn before(&mut self, _: &PolicyState<'_>, call: &Call<'_>) -> Decision<Self::Frame> {
        self.log.log(&format_call(call));
        Decision::Allow(())
    }

    fn after(&mut self, _: &Call<'_>, (): Self::Frame, returned: &Returned) -> Vec<Metadata> {
        self.log.log(&format_return(returned));
        Vec::new()
    }

    fn before_state_change(&mut self, _: &PolicyState<'_>, opened: &ChannelOpened) -> Decision {
        self.log.log(&format!(
            "channel opened {} {:?} via call {}",
            opened.handle, opened.kind, opened.call_id
        ));
        Decision::Allow(())
    }
}

fn format_observation(event: &Event<'_>) -> Option<String> {
    match event {
        Event::InvocationStarted(event) => Some(format!("invocation start {}", event.invocation)),
        Event::InvocationEnded(event) => {
            let outcome = match event.outcome {
                InvocationOutcome::Returned => "returned",
                InvocationOutcome::Trapped => "trapped",
                _ => "unknown",
            };
            Some(format!("invocation end {} {outcome}", event.invocation))
        }
        Event::HandleDropped(event) => Some(format!("handle dropped {}", event.handle)),
        Event::ChannelClosed(event) => Some(format!("channel closed {}", event.handle)),
        Event::Returned(event) => Some(format_return(event)),
        _ => None,
    }
}

fn format_call(call: &Call<'_>) -> String {
    let mut record = format!("call {} {}.{}", call.id, call.interface, call.function);
    for designator in &call.designators {
        record.push(' ');
        record.push_str(&designator.key);
        record.push('=');
        record.push_str(&designator.value);
    }
    record
}

fn format_return(returned: &Returned) -> String {
    let status = match &returned.status {
        ReturnStatus::Ok => "ok",
        ReturnStatus::Error(_) => "error",
        ReturnStatus::Denied(_) => "denied",
        ReturnStatus::Trapped(_) => "trapped",
        _ => "unknown",
    };
    format!("return {} {status}", returned.call_id)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::{sync::Arc, vec};
    use sleeve_core::{
        Chain, Designator, HandleTable, InvocationEnded, InvocationStarted, PolicyState, Start,
    };
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone)]
    struct MemoryLog(Arc<Mutex<Vec<String>>>);

    impl AuditLog for MemoryLog {
        fn log(&mut self, event: &str) {
            self.0.lock().unwrap().push(event.into());
        }
    }

    #[test]
    fn logs_each_event_when_it_is_observed() {
        let records = Arc::new(Mutex::new(Vec::new()));
        let mut chain = Chain::new().with(Trace::new(MemoryLog(Arc::clone(&records))));
        chain.observe(&Event::InvocationStarted(InvocationStarted::new("summary")));
        let call = Call::new(1, "example:notes/notes@0.1.0", "read")
            .with_designators(vec![Designator::new("name", "first")]);
        let Start::Allowed(active) = chain.start_call(&PolicyState::new(&[]), &call) else {
            unreachable!()
        };
        chain.finish_call(
            &call,
            active,
            &Returned::new(1, ReturnStatus::Ok),
            &mut HandleTable::new(),
        );
        chain.observe(&Event::InvocationEnded(InvocationEnded::new(
            "summary",
            InvocationOutcome::Returned,
        )));
        assert_eq!(
            &*records.lock().unwrap(),
            &[
                "invocation start summary",
                "call 1 example:notes/notes@0.1.0.read name=first",
                "return 1 ok",
                "invocation end summary returned"
            ]
        );
    }
}
