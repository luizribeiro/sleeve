//! Attempts to export a host-reserved interface.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::string::String;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/platform", "wit"],
        world: "example:platform-export/plugin@0.1.0",
    });
}

struct Component;

impl bindings::exports::sleeve::platform::lifecycle::Guest for Component {
    fn start(_: String) {}
    fn end(_: String, _: bool) {}
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
