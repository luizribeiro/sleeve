//! Relays queued guest channels from an invocation-long host task.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{format, string::ToString, vec::Vec};
use core::future::IntoFuture;
use core::sync::atomic::{AtomicU64, Ordering};

use bindings::exports::example::stream_relay::relay::Guest;
use sleeve_core::{
    Call, Chain, ChannelKind, Decision, Denied, Metadata, Policy, PolicyState, Returned, Sleeve,
};
const CHUNK_SIZE: usize = 64 * 1024;
const BUFFERED_MODE: u8 = 1;
const CALL_SCOPED_MODE: u8 = 3;
const SHUTDOWN_MODE: u8 = 4;
const SHUTDOWN_BYTES: u32 = 2 * 64 * 1024;

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

struct Relay {
    source: wit_bindgen::StreamReader<u8>,
    trailers: wit_bindgen::FutureReader<u8>,
    destination: wit_bindgen::StreamWriter<u8>,
    destination_trailers: wit_bindgen::FutureWriter<u8>,
    channels: RelayChannels,
}

struct RelayChannels {
    body: Option<u64>,
    trailers: Option<u64>,
}

impl Drop for RelayChannels {
    fn drop(&mut self) {
        if let Some(id) = self.body.take() {
            let _closed = SLEEVE.close_channel(id);
        }
        if let Some(id) = self.trailers.take() {
            let _closed = SLEEVE.close_channel(id);
        }
    }
}

struct QueryPolicy;

impl Policy for QueryPolicy {
    type Frame = ();

    fn before(&mut self, state: &PolicyState<'_>, call: &Call<'_>) -> Decision {
        if call.function == "query" && state.open_channels().next().is_some() {
            Decision::Deny(Denied::new("channel open", ()))
        } else {
            Decision::Allow(())
        }
    }

    fn after(&mut self, _: &Call<'_>, (): (), _: &Returned) -> Vec<Metadata> {
        Vec::new()
    }
}

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
        source: wit_bindgen::StreamReader<u8>,
        trailers: wit_bindgen::FutureReader<u8>,
    ) -> u32 {
        if mode == BUFFERED_MODE {
            return buffer(source, trailers).await;
        }
        if mode == CALL_SCOPED_MODE {
            return relay_in_call(source, trailers).await;
        }
        let (destination, body) = bindings::wit_stream::new();
        let (destination_trailers, forwarded_trailers) = bindings::wit_future::new(|| 0);
        let relay = Relay {
            source,
            trailers,
            destination,
            destination_trailers,
            channels: RelayChannels {
                body: Some(open(ChannelKind::Stream)),
                trailers: Some(open(ChannelKind::Future)),
            },
        };
        if SLEEVE.enqueue_relay(relay_one(relay)).is_err() {
            core::arch::wasm32::unreachable();
        }

        let expected = if mode == SHUTDOWN_MODE {
            SHUTDOWN_BYTES
        } else {
            0
        };
        let accepted =
            bindings::example::stream_relay::sink::accept(expected, body, forwarded_trailers).await;
        bindings::example::stream_relay::sink::log("send-returned".to_string()).await;
        accepted
    }

    async fn mark(name: alloc::string::String) {
        bindings::example::stream_relay::sink::log(name).await;
    }

    async fn query() -> bool {
        let closed = SLEEVE
            .dispatch(
                "example:stream-relay/relay@0.1.0",
                "query",
                Vec::new(),
                async {},
            )
            .await
            .is_ok();
        let event = if closed { "query-closed" } else { "query-open" };
        bindings::example::stream_relay::sink::log(event.to_string()).await;
        closed
    }
}

async fn relay_one(mut relay: Relay) {
    loop {
        let (status, bytes) = relay.source.read(Vec::with_capacity(CHUNK_SIZE)).await;
        if !bytes.is_empty() && !relay.destination.write_all(bytes).await.is_empty() {
            break;
        }
        if !matches!(status, wit_bindgen::StreamResult::Complete(_)) {
            break;
        }
    }
    drop(relay.destination);
    if let Some(id) = relay.channels.body.take() {
        close(id, "body").await;
    }

    let trailers = relay.trailers.into_future().await;
    let _write = relay.destination_trailers.write(trailers).await;
    if let Some(id) = relay.channels.trailers.take() {
        close(id, "trailers").await;
    }
}

async fn buffer(
    mut source: wit_bindgen::StreamReader<u8>,
    trailers: wit_bindgen::FutureReader<u8>,
) -> u32 {
    let body_id = open(ChannelKind::Stream);
    let trailers_id = open(ChannelKind::Future);
    let mut chunks = Vec::new();
    loop {
        let (status, bytes) = source.read(Vec::with_capacity(CHUNK_SIZE)).await;
        if !bytes.is_empty() {
            chunks.push(bytes);
        }
        if !matches!(status, wit_bindgen::StreamResult::Complete(_)) {
            break;
        }
    }
    close(body_id, "body").await;
    let trailers = trailers.await;
    close(trailers_id, "trailers").await;

    let expected = chunks
        .iter()
        .map(Vec::len)
        .sum::<usize>()
        .try_into()
        .unwrap_or(u32::MAX);
    let (mut destination, body) = bindings::wit_stream::new();
    let (destination_trailers, forwarded_trailers) = bindings::wit_future::new(|| 0);
    wit_bindgen::spawn_local(async move {
        for bytes in chunks {
            if !destination.write_all(bytes).await.is_empty() {
                break;
            }
        }
        drop(destination);
    });
    wit_bindgen::spawn_local(async move {
        let _write = destination_trailers.write(trailers).await;
    });
    bindings::example::stream_relay::sink::accept(expected, body, forwarded_trailers).await
}

async fn relay_in_call(
    mut source: wit_bindgen::StreamReader<u8>,
    trailers: wit_bindgen::FutureReader<u8>,
) -> u32 {
    let body_id = open(ChannelKind::Stream);
    let trailers_id = open(ChannelKind::Future);
    let (mut destination, body) = bindings::wit_stream::new();
    let (destination_trailers, forwarded_trailers) = bindings::wit_future::new(|| 0);

    wit_bindgen::spawn_local(async move {
        loop {
            let (status, bytes) = source.read(Vec::with_capacity(CHUNK_SIZE)).await;
            if !bytes.is_empty() && !destination.write_all(bytes).await.is_empty() {
                break;
            }
            if !matches!(status, wit_bindgen::StreamResult::Complete(_)) {
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

    let accepted = bindings::example::stream_relay::sink::accept(0, body, forwarded_trailers).await;
    bindings::example::stream_relay::sink::log("send-returned".to_string()).await;
    accepted
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
        if SLEEVE
            .start(Chain::new().with(QueryPolicy), invocation)
            .is_err()
        {
            core::arch::wasm32::unreachable();
        }
    }

    fn end(invocation: alloc::string::String, trapped: bool) {
        if SLEEVE.end(invocation, trapped).is_err() {
            core::arch::wasm32::unreachable();
        }
    }
}

sleeve_core::export_anchor!(bindings);

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
