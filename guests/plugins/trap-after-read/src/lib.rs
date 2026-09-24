//! Traps after completing one host call.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::string::String;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "wit"],
        world: "example:trap-after-read/plugin@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn summarize(first: String, _: String) -> String {
        let _ = bindings::example::notes::notes::read(first).await;
        unreachable!()
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
