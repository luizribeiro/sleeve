//! A tracing policy exported as an independently composed component.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{format, string::String, vec::Vec};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/platform", "../../../wit/policy", "wit"],
        world: "sleeve:trace-policy/policy@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

use bindings::exports::sleeve::policy::hooks::{
    Call, Decision, Event, Guest, InvocationOutcome, Metadata, ReturnStatus, Returned,
};

struct Component;

impl Guest for Component {
    fn before(
        _open_channels: Vec<bindings::exports::sleeve::policy::hooks::ChannelOpened>,
        event: Event,
    ) -> Decision {
        if let Event::Call(call) = event {
            bindings::sleeve::platform::audit::log(&format_call(&call));
        }
        Decision::Permit(0)
    }

    fn after(_call: Call, _frame: u64, returned: Returned) -> Vec<Metadata> {
        bindings::sleeve::platform::audit::log(&format_return(&returned));
        Vec::new()
    }

    fn observe(event: Event) {
        if let Some(record) = format_event(&event) {
            bindings::sleeve::platform::audit::log(&record);
        }
    }
}

fn format_event(event: &Event) -> Option<String> {
    match event {
        Event::InvocationStarted((invocation,)) => Some(format!("invocation start {invocation}")),
        Event::InvocationEnded((invocation, outcome)) => {
            let outcome = match outcome {
                InvocationOutcome::Returned => "returned",
                InvocationOutcome::Trapped => "trapped",
            };
            Some(format!("invocation end {invocation} {outcome}"))
        }
        Event::HandleDropped(handle) => Some(format!("handle dropped {handle}")),
        Event::ChannelClosed(handle) => Some(format!("channel closed {handle}")),
        Event::Returned(returned) => Some(format_return(returned)),
        _ => None,
    }
}

fn format_call(call: &Call) -> String {
    let mut record = format!("call {} {}.{}", call.id, call.interface_name, call.function);
    for designator in &call.designators {
        record.push(' ');
        record.push_str(&designator.key);
        record.push('=');
        record.push_str(&designator.value);
    }
    record
}

fn format_return(returned: &Returned) -> String {
    let status = match returned.status {
        ReturnStatus::Ok => "ok",
        ReturnStatus::Error(_) => "error",
        ReturnStatus::Denied(_) => "denied",
        ReturnStatus::Trapped(_) => "trapped",
    };
    format!("return {} {status}", returned.call_id)
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
