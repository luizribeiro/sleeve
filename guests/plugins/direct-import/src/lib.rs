//! Attempts to reach an unrelated host interface directly.

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
        bindings::example::direct_import::secrets::reveal().await
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
