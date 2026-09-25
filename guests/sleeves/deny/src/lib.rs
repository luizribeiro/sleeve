//! A test sleeve that traces a policy refusal.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use sleeve_core::{Call, Chain, Decision, Denied, Metadata, Policy, PolicyState, Returned};
use sleeve_policy_trace::{AuditLog, Trace};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/platform", "wit"],
        world: "sleeve:deny/sleeve@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

struct Platform;

impl AuditLog for Platform {
    fn log(&mut self, event: &str) {
        bindings::sleeve::platform::audit::log(event);
    }
}

struct Deny;

impl Policy for Deny {
    type Frame = ();

    fn before(&mut self, _: &PolicyState<'_>, _: &Call<'_>) -> Decision<Self::Frame> {
        Decision::Deny(Denied::new("notes access denied", ()))
    }

    fn after(&mut self, _: &Call<'_>, (): Self::Frame, _: &Returned) -> Vec<Metadata> {
        Vec::new()
    }
}

sleeve_core::export_notes_sleeve!(bindings, Chain::new().with(Trace::new(Platform)).with(Deny));
