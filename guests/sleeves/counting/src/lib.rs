//! A test sleeve whose policy counts calls within one invocation.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{format, vec::Vec};
use sleeve_core::{Call, Chain, Decision, Metadata, Policy, PolicyState, Returned};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/platform", "wit"],
        world: "sleeve:counting/sleeve@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

struct Count {
    calls: usize,
}

impl Policy for Count {
    type Frame = ();

    fn before(&mut self, _: &PolicyState<'_>, _: &Call<'_>) -> Decision<Self::Frame> {
        self.calls += 1;
        bindings::sleeve::platform::audit::log(&format!("count {}", self.calls));
        Decision::Allow(())
    }

    fn after(&mut self, _: &Call<'_>, (): Self::Frame, _: &Returned) -> Vec<Metadata> {
        Vec::new()
    }
}

sleeve_core::export_notes_sleeve!(bindings, Chain::new().with(Count { calls: 0 }));
