//! Exercises filesystem information-flow decisions through a sleeve.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{string::String, string::ToString, vec, vec::Vec};
use bindings::wasi::filesystem::types::{
    Descriptor, DescriptorFlags, ErrorCode, OpenFlags, PathFlags,
};
use core::sync::atomic::{AtomicBool, Ordering};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/clocks", "../../../wit/filesystem", "wit"],
        world: "example:file-scenarios/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

static COMPLETION_READY: AtomicBool = AtomicBool::new(false);

impl bindings::Guest for Component {
    async fn run(scenario: u8) -> String {
        let (public, secret) = preopens();
        match scenario {
            0 => open_public_then_read_secret(&public, &secret).await,
            1 => close_public_then_read_secret(&public, &secret).await,
            2 => read_secret_then_write(&secret, &secret, true).await,
            3 => read_secret_then_write(&secret, &public, false).await,
            4 => escape(&public, "../secret/note.txt").await,
            5 => escape(&public, "/etc/passwd").await,
            6 => symlink_escape(&public, "secret-link").await,
            7 => write_intent(&secret, &public, OpenFlags::TRUNCATE, DescriptorFlags::READ).await,
            8 => write_intent(&secret, &public, OpenFlags::empty(), DescriptorFlags::WRITE).await,
            9 => raise_after_writer_close_before_completion(&public, &secret).await,
            10 => write_benchmark(&public).await,
            11 => read_benchmark(&public).await,
            _ => "unknown scenario".into(),
        }
    }
}

async fn write_benchmark(public: &Descriptor) -> String {
    const SIZE: usize = 1024 * 1024;
    let file = create(public, "benchmark.bin").await;
    write(&file, &vec![b'x'; SIZE]).await;
    SIZE.to_string()
}

async fn read_benchmark(public: &Descriptor) -> String {
    read(public, "benchmark.bin").await.len().to_string()
}

fn preopens() -> (Descriptor, Descriptor) {
    let mut public = None;
    let mut secret = None;
    for (descriptor, name) in bindings::wasi::filesystem::preopens::get_directories() {
        match name.as_str() {
            "public" => public = Some(descriptor),
            "secret" => secret = Some(descriptor),
            _ => {}
        }
    }
    match (public, secret) {
        (Some(public), Some(secret)) => (public, secret),
        _ => core::arch::wasm32::unreachable(),
    }
}

async fn open_public_then_read_secret(public: &Descriptor, secret: &Descriptor) -> String {
    let file = create(public, "open.txt").await;
    let (writer, reader) = bindings::wit_stream::new();
    let _completion = file.write_via_stream(reader, 0);
    let result = open_read(secret, "note.txt").await;
    drop(writer);
    if matches!(result, Err(ErrorCode::Access)) {
        "read refused".into()
    } else {
        "read unexpectedly allowed".into()
    }
}

async fn close_public_then_read_secret(public: &Descriptor, secret: &Descriptor) -> String {
    let file = create(public, "closed.txt").await;
    write(&file, b"public report").await;
    read(secret, "note.txt").await
}

async fn read_secret_then_write(
    source: &Descriptor,
    destination: &Descriptor,
    allow: bool,
) -> String {
    let contents = read(source, "note.txt").await;
    match destination
        .open_at(
            PathFlags::empty(),
            "report.txt".to_string(),
            OpenFlags::CREATE | OpenFlags::TRUNCATE,
            DescriptorFlags::WRITE,
        )
        .await
    {
        Ok(file) if allow => {
            write(&file, contents.as_bytes()).await;
            "secret write allowed".into()
        }
        Ok(_) => "write unexpectedly allowed".into(),
        Err(ErrorCode::Access) if !allow => "write refused".into(),
        Err(ErrorCode::Access) => "write unexpectedly refused".into(),
        Err(error) => alloc::format!("unexpected error: {error:?}"),
    }
}

async fn escape(directory: &Descriptor, path: &str) -> String {
    match open_read(directory, path).await {
        Err(ErrorCode::NotPermitted) => "escape refused".into(),
        _ => "escape unexpectedly allowed".into(),
    }
}

async fn symlink_escape(directory: &Descriptor, path: &str) -> String {
    match directory
        .open_at(
            PathFlags::SYMLINK_FOLLOW,
            path.to_string(),
            OpenFlags::empty(),
            DescriptorFlags::READ,
        )
        .await
    {
        Err(_) => "escape refused".into(),
        Ok(_) => "escape unexpectedly allowed".into(),
    }
}

async fn write_intent(
    source: &Descriptor,
    destination: &Descriptor,
    open_flags: OpenFlags,
    descriptor_flags: DescriptorFlags,
) -> String {
    let _contents = read(source, "note.txt").await;
    match destination
        .open_at(
            PathFlags::empty(),
            "existing.txt".to_string(),
            open_flags,
            descriptor_flags,
        )
        .await
    {
        Err(ErrorCode::Access) => "write refused".into(),
        _ => "write unexpectedly allowed".into(),
    }
}

async fn raise_after_writer_close_before_completion(
    public: &Descriptor,
    secret: &Descriptor,
) -> String {
    let file = create(public, "pending.txt").await;
    let (mut writer, reader) = bindings::wit_stream::new();
    let completion = file.write_via_stream(reader, 0);
    COMPLETION_READY.store(false, Ordering::Relaxed);
    wit_bindgen::spawn_local(async move {
        let _completed = completion.await;
        COMPLETION_READY.store(true, Ordering::Relaxed);
    });
    for _ in 0..4 {
        let remaining = writer.write_all(alloc::vec![b'x'; 64 * 1024]).await;
        if !remaining.is_empty() {
            return "write failed".into();
        }
    }
    drop(writer);

    let descriptor = loop {
        match open_read(secret, "note.txt").await {
            Ok(descriptor) => break descriptor,
            Err(ErrorCode::Access) => wit_bindgen::yield_async().await,
            Err(_) => return "read failed".into(),
        }
    };
    drop(descriptor);
    if COMPLETION_READY.load(Ordering::Relaxed) {
        "completion arrived before raise".into()
    } else {
        "raise allowed while completion pending".into()
    }
}

async fn create(directory: &Descriptor, path: &str) -> Descriptor {
    match directory
        .open_at(
            PathFlags::empty(),
            path.to_string(),
            OpenFlags::CREATE | OpenFlags::TRUNCATE,
            DescriptorFlags::WRITE,
        )
        .await
    {
        Ok(file) => file,
        Err(_) => core::arch::wasm32::unreachable(),
    }
}

async fn open_read(directory: &Descriptor, path: &str) -> Result<Descriptor, ErrorCode> {
    directory
        .open_at(
            PathFlags::empty(),
            path.to_string(),
            OpenFlags::empty(),
            DescriptorFlags::READ,
        )
        .await
}

async fn read(directory: &Descriptor, path: &str) -> String {
    let Ok(file) = open_read(directory, path).await else {
        return "read failed".into();
    };
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
    match String::from_utf8(bytes) {
        Ok(contents) => contents,
        Err(_) => "invalid UTF-8".into(),
    }
}

async fn write(file: &Descriptor, bytes: &[u8]) {
    let (mut writer, reader) = bindings::wit_stream::new();
    let completion = file.write_via_stream(reader, 0);
    let _remaining = writer.write_all(bytes.to_vec()).await;
    drop(writer);
    let _completed = completion.await;
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
