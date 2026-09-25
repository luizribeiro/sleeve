//! Checks sleeve composition against Wasmtime's upstream filesystem WIT.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        inline: "package example:filesystem-upstream@0.1.0; world plugin { import wasi:filesystem/preopens@0.3.0; import wasi:filesystem/types@0.3.0; export run: func(); }",
        path: [
            "../../../wit/upstream/wasmtime-wasi-49.0.1/deps/clocks.wit",
            "../../../wit/upstream/wasmtime-wasi-49.0.1/deps/filesystem.wit",
        ],
        world: "example:filesystem-upstream/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

impl bindings::Guest for Component {
    fn run() {
        let Some((descriptor, _)) = bindings::wasi::filesystem::preopens::get_directories()
            .into_iter()
            .next()
        else {
            return;
        };
        let _read = descriptor.read_via_stream(0);
        let (_writer, reader) = bindings::wit_stream::new();
        let _write = descriptor.write_via_stream(reader, 0);
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
