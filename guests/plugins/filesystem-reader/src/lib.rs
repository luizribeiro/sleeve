//! Reads a preopened file through the smallest filesystem interface it needs.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{string::String, string::ToString, vec::Vec};
use bindings::wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        inline: "package example:filesystem-reader@0.1.0; world plugin { import wasi:filesystem/preopens@0.3.0; import wasi:filesystem/types@0.3.0; export run: async func() -> string; }",
        path: "../../sleeves/filesystem-forwarder/wit/deps/filesystem.wit",
        world: "example:filesystem-reader/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn run() -> String {
        let directory = bindings::wasi::filesystem::preopens::get_directories()
            .into_iter()
            .find_map(|(descriptor, name)| (name == "secret").then_some(descriptor))
            .unwrap_or_else(|| core::arch::wasm32::unreachable());
        let file = directory
            .open_at(
                PathFlags::empty(),
                "note.txt".to_string(),
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .await
            .unwrap_or_else(|_| core::arch::wasm32::unreachable());
        let (mut stream, completion) = file.read_via_stream(0);
        let mut bytes = Vec::new();
        loop {
            let (status, chunk) = stream.read(Vec::with_capacity(64 * 1024)).await;
            bytes.extend(chunk);
            if !matches!(status, wit_bindgen::StreamResult::Complete(_)) {
                break;
            }
        }
        let _completed = completion.await;
        String::from_utf8(bytes).unwrap_or_else(|_| core::arch::wasm32::unreachable())
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
