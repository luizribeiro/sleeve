//! Reads two notes and combines them into a one-line summary.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{format, string::String};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "wit"],
        world: "example:note-summary/plugin@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn summarize(first: String, second: String) -> String {
        let first = bindings::example::notes::notes::read(first).await;
        let second = bindings::example::notes::notes::read(second).await;
        format!("{first}; {second}")
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};

    bindings::export!(Component with_types_in bindings);
}
