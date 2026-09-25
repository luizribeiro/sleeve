//! Exercises upstream filesystem streams without a sleeve.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{string::String, string::ToString, vec::Vec};
use bindings::wasi::filesystem::types::{Descriptor, DescriptorFlags, OpenFlags, PathFlags};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        inline: "package example:filesystem-upstream@0.1.0; world plugin { import wasi:filesystem/preopens@0.3.0; import wasi:filesystem/types@0.3.0; export read-to-end: async func() -> string; export write-then-read: async func() -> string; }",
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
    async fn read_to_end() -> String {
        read(&preopen("secret"), "note.txt").await
    }

    async fn write_then_read() -> String {
        let file = create(&preopen("public")).await;
        let (mut writer, reader) = bindings::wit_stream::new();
        let completion = file.write_via_stream(reader, 0);
        let _remaining = writer.write_all(b"report".to_vec()).await;
        drop(writer);
        let _completed = completion.await;
        read(&preopen("secret"), "note.txt").await
    }
}

fn preopen(name: &str) -> Descriptor {
    bindings::wasi::filesystem::preopens::get_directories()
        .into_iter()
        .find_map(|(descriptor, candidate)| (candidate == name).then_some(descriptor))
        .unwrap_or_else(|| core::arch::wasm32::unreachable())
}

async fn read(directory: &Descriptor, path: &str) -> String {
    let file = open_read(directory, path).await;
    let (mut stream, completion) = file.read_via_stream(0);
    let bytes = collect(&mut stream).await;
    let _completed = completion.await;
    text(bytes)
}

async fn open_read(directory: &Descriptor, path: &str) -> Descriptor {
    directory
        .open_at(
            PathFlags::empty(),
            path.to_string(),
            OpenFlags::empty(),
            DescriptorFlags::READ,
        )
        .await
        .unwrap_or_else(|_| core::arch::wasm32::unreachable())
}

async fn create(directory: &Descriptor) -> Descriptor {
    directory
        .open_at(
            PathFlags::empty(),
            "report.txt".to_string(),
            OpenFlags::CREATE | OpenFlags::TRUNCATE,
            DescriptorFlags::WRITE,
        )
        .await
        .unwrap_or_else(|_| core::arch::wasm32::unreachable())
}

async fn collect(stream: &mut wit_bindgen::StreamReader<u8>) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let (status, chunk) = stream.read(Vec::with_capacity(64 * 1024)).await;
        bytes.extend(chunk);
        if !matches!(status, wit_bindgen::StreamResult::Complete(_)) {
            break;
        }
    }
    bytes
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|_| core::arch::wasm32::unreachable())
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
