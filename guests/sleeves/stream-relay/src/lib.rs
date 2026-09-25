//! Relays or buffers guest channels before forwarding them to a host sink.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{format, string::ToString, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};

use bindings::exports::example::stream_relay::relay::Guest;
use sleeve_core::{Chain, ChannelKind, Sleeve};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/platform", "../../../wit/stream-relay"],
        world: "example:stream-relay/sleeve@0.1.0",
        generate_all,
        ownership: Owning,
        async: ["-export:drop-future"],
    });
}

static SLEEVE: Sleeve = Sleeve::new();
static NEXT_CHANNEL: AtomicU64 = AtomicU64::new(1);

struct Component;

impl bindings::Guest for Component {
    fn drop_future() {
        let (writer, _reader) = bindings::wit_future::new(|| 0_u8);
        drop(writer);
    }
}

impl Guest for Component {
    async fn send(
        mode: u8,
        body: wit_bindgen::StreamReader<u8>,
        trailers: wit_bindgen::FutureReader<u8>,
    ) -> u32 {
        match mode {
            1 => buffer(body, trailers).await,
            _ => relay(mode, body, trailers).await,
        }
    }

    async fn mark(name: alloc::string::String) {
        bindings::example::stream_relay::sink::log(name).await;
    }
}

async fn relay(
    mode: u8,
    mut source: wit_bindgen::StreamReader<u8>,
    trailers: wit_bindgen::FutureReader<u8>,
) -> u32 {
    let body_id = open(ChannelKind::Stream);
    let trailers_id = open(ChannelKind::Future);
    let (mut destination, body) = bindings::wit_stream::new();
    let (destination_trailers, forwarded_trailers) = bindings::wit_future::new(|| 0);

    wit_bindgen::spawn_local(async move {
        while let Some(byte) = source.next().await {
            if destination.write_one(byte).await.is_some() {
                while source.next().await.is_some() {}
                break;
            }
        }
        drop(destination);
        close(body_id, "body").await;
    });
    wit_bindgen::spawn_local(async move {
        let value = trailers.await;
        let _write = destination_trailers.write(value).await;
        close(trailers_id, "trailers").await;
    });

    let accepted =
        bindings::example::stream_relay::sink::accept(mode, body, forwarded_trailers).await;
    bindings::example::stream_relay::sink::log("send-returned".to_string()).await;
    accepted
}

async fn buffer(
    mut source: wit_bindgen::StreamReader<u8>,
    trailers: wit_bindgen::FutureReader<u8>,
) -> u32 {
    let body_id = open(ChannelKind::Stream);
    let trailers_id = open(ChannelKind::Future);
    let mut bytes = Vec::new();
    while let Some(byte) = source.next().await {
        bytes.push(byte);
    }
    close(body_id, "body").await;
    let trailers = trailers.await;
    close(trailers_id, "trailers").await;

    let (mut destination, body) = bindings::wit_stream::new();
    let (destination_trailers, forwarded_trailers) = bindings::wit_future::new(|| 0);
    wit_bindgen::spawn_local(async move {
        let _remaining = destination.write_all(bytes).await;
        drop(destination);
    });
    wit_bindgen::spawn_local(async move {
        let _write = destination_trailers.write(trailers).await;
    });
    bindings::example::stream_relay::sink::accept(1, body, forwarded_trailers).await
}

fn open(kind: ChannelKind) -> u64 {
    let id = NEXT_CHANNEL.fetch_add(1, Ordering::Relaxed);
    if SLEEVE.open_channel(id, kind, 1).is_err() {
        core::arch::wasm32::unreachable();
    }
    id
}

async fn close(id: u64, name: &str) {
    let event = match SLEEVE.close_channel(id) {
        Ok(()) => format!("{name}-closed-state-ok"),
        Err(_) => format!("{name}-closed-state-error"),
    };
    bindings::example::stream_relay::sink::log(event).await;
}

impl bindings::exports::sleeve::platform::lifecycle::Guest for Component {
    fn start(invocation: alloc::string::String) {
        if SLEEVE.start(Chain::new(), invocation).is_err() {
            core::arch::wasm32::unreachable();
        }
    }

    fn end(invocation: alloc::string::String, trapped: bool) {
        if SLEEVE.end(invocation, trapped).is_err() {
            core::arch::wasm32::unreachable();
        }
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
