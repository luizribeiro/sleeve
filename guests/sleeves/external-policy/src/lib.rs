//! A notes sleeve that delegates policy decisions to another component.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{string::ToString, vec::Vec};
use sleeve_core::{
    Call, ChannelKind, ChannelOpened, Decision, Denied, Event, InvocationOutcome, Metadata, Policy,
    PolicyState, Provenance, ReturnStatus, Returned, Trap,
};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/platform", "../../../wit/policy", "wit"],
        world: "sleeve:external-policy/sleeve@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

use bindings::sleeve::policy::hooks as wit;

struct External;

impl Policy for External {
    type Frame = u64;

    fn observe(&mut self, event: &Event<'_>) {
        wit::observe(&to_event(event));
    }

    fn before(&mut self, state: &PolicyState<'_>, call: &Call<'_>) -> Decision<Self::Frame> {
        decide(wit::before(
            &state.open_channels().map(to_channel).collect::<Vec<_>>(),
            &wit::Event::Call(to_call(call)),
        ))
    }

    fn after(&mut self, call: &Call<'_>, frame: Self::Frame, returned: &Returned) -> Vec<Metadata> {
        let _metadata = wit::after(&to_call(call), frame, &to_returned(returned));
        Vec::new()
    }

    fn before_state_change(&mut self, state: &PolicyState<'_>, opened: &ChannelOpened) -> Decision {
        match decide(wit::before(
            &state.open_channels().map(to_channel).collect::<Vec<_>>(),
            &wit::Event::ChannelOpened(to_channel(opened)),
        )) {
            Decision::Allow(_) => Decision::Allow(()),
            Decision::Deny(error) => Decision::Deny(error),
            Decision::Trap(error) => Decision::Trap(error),
            _ => Decision::Trap(Trap::new("unknown external policy decision")),
        }
    }
}

fn decide(decision: wit::Decision) -> Decision<u64> {
    match decision {
        wit::Decision::Permit(frame) => Decision::Allow(frame),
        wit::Decision::Deny(message) => Decision::Deny(Denied::new(message, ())),
        wit::Decision::Trap(message) => Decision::Trap(Trap::new(message)),
    }
}

fn to_event(event: &Event<'_>) -> wit::Event {
    match event {
        Event::InvocationStarted(event) => {
            wit::Event::InvocationStarted((event.invocation.clone(),))
        }
        Event::InvocationEnded(event) => wit::Event::InvocationEnded((
            event.invocation.clone(),
            match event.outcome {
                InvocationOutcome::Returned => wit::InvocationOutcome::Returned,
                _ => wit::InvocationOutcome::Trapped,
            },
        )),
        Event::Call(call) => wit::Event::Call(to_call(call)),
        Event::Returned(returned) => wit::Event::Returned(to_returned(returned)),
        Event::HandleDropped(event) => wit::Event::HandleDropped(event.handle),
        Event::ChannelOpened(event) => wit::Event::ChannelOpened(to_channel(event)),
        Event::ChannelClosed(event) => wit::Event::ChannelClosed(event.handle),
        _ => wit::Event::InvocationEnded(("unknown".to_string(), wit::InvocationOutcome::Trapped)),
    }
}

fn to_call(call: &Call<'_>) -> wit::Call {
    wit::Call {
        id: call.id,
        interface_name: call.interface.to_string(),
        function: call.function.to_string(),
        designators: call
            .designators
            .iter()
            .map(|item| wit::Designator {
                key: item.key.to_string(),
                value: item.value.to_string(),
            })
            .collect(),
        handles: call.handles.clone(),
    }
}

fn to_returned(returned: &Returned) -> wit::Returned {
    wit::Returned {
        call_id: returned.call_id,
        status: match &returned.status {
            ReturnStatus::Ok => wit::ReturnStatus::Ok,
            ReturnStatus::Error(message) => wit::ReturnStatus::Error(message.clone()),
            ReturnStatus::Denied(message) => wit::ReturnStatus::Denied(message.clone()),
            ReturnStatus::Trapped(message) => wit::ReturnStatus::Trapped(message.clone()),
            _ => wit::ReturnStatus::Trapped("unknown".to_string()),
        },
        handles: returned
            .handles
            .iter()
            .map(|handle| wit::ProducedHandle {
                id: handle.id,
                resource_type: handle.resource_type.clone(),
                provenance: match handle.provenance {
                    Provenance::Call(id) => wit::Provenance::Call(id),
                    Provenance::Parent(id) => wit::Provenance::Parent(id),
                    _ => wit::Provenance::Call(0),
                },
            })
            .collect(),
    }
}

fn to_channel(channel: &ChannelOpened) -> wit::ChannelOpened {
    wit::ChannelOpened {
        handle: channel.handle,
        kind: match channel.kind {
            ChannelKind::Stream => wit::ChannelKind::ByteStream,
            ChannelKind::Future => wit::ChannelKind::FutureValue,
            _ => wit::ChannelKind::Other,
        },
        call_id: channel.call_id,
        sink: channel.sink,
    }
}

sleeve_core::export_notes_sleeve!(bindings, sleeve_core::Chain::new().with(External));
