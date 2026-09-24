//! Minimal asynchronous component used to verify the guest toolchain.

#![deny(unsafe_code)]

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "smoke",
    });
}

use bindings::exports::sleeve::smoke::greeter::Guest;

struct Component;

impl Guest for Component {
    #[allow(clippy::unused_async_trait_impl)]
    async fn greet(name: String) -> String {
        format!("Hello, {name}!")
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};

    bindings::export!(Component with_types_in bindings);
}
