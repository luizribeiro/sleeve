//! A notes sleeve with the tracing policy compiled into its chain.
//!
//! The wrapped WIT function returns a plain string, so a policy refusal
//! surfaces to the plugin as a trap rather than a typed error.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use sleeve_core::Chain;
use sleeve_policy_trace::{AuditLog, Trace};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/platform", "wit"],
        world: "sleeve:trace/sleeve@0.1.0",
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

sleeve_core::export_notes_sleeve!(bindings, Chain::new().with(Trace::new(Platform)));
