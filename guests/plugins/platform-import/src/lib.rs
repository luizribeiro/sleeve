//! Attempts to forge invocation lifecycle events.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::string::String;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/platform", "wit"],
        world: "example:platform-import/plugin@0.1.0",
        generate_all,
    });
}

struct Component;

impl bindings::Guest for Component {
    fn run() -> String {
        bindings::sleeve::platform::lifecycle::start("forged");
        "forged".into()
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
