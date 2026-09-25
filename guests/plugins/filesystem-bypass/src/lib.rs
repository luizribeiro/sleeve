//! Imports a filesystem method absent from the wrapped subset.

#![no_std]
#![deny(unsafe_code)]

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "example:filesystem-bypass/plugin@0.1.0",
        generate_all,
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn run() {
        if let Some((descriptor, _)) = bindings::wasi::filesystem::preopens::get_directories()
            .into_iter()
            .next()
        {
            let _type = descriptor.get_type().await;
        }
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
