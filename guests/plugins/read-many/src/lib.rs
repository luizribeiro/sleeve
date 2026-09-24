//! Repeats one notes call inside a single exported invocation.

#![no_std]
#![deny(unsafe_code)]

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "wit"],
        world: "example:read-many/plugin@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn run(count: u32) -> u64 {
        let mut total = 0_u64;
        for _ in 0..count {
            let note = bindings::example::notes::notes::read("bench".into()).await;
            total += note.len() as u64;
        }
        total
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
