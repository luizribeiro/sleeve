//! Attempts to import a lookalike notes interface.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::string::String;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({ path: "wit", world: "plugin" });
}

struct Component;

impl bindings::Guest for Component {
    async fn run() -> String {
        bindings::example::bypass::notes::read("private".into()).await
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
