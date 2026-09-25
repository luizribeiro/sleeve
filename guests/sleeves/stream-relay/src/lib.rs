//! Relays queued guest channels from an invocation-long host task.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::{format, string::ToString, vec::Vec};
use core::future::{Future, IntoFuture, poll_fn};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::task::Poll;

use bindings::exports::example::stream_relay::relay::Guest;
use sleeve_core::{
    Call, Chain, ChannelKind, Decision, Denied, Metadata, Policy, PolicyState, Returned, Sleeve,
};
use spin::Mutex;

const CHUNK_SIZE: usize = 64 * 1024;
const BUFFERED_MODE: u8 = 1;
const CALL_SCOPED_MODE: u8 = 3;

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
static STOP: AtomicBool = AtomicBool::new(false);
static RELAYS: Mutex<VecDeque<Relay>> = Mutex::new(VecDeque::new());

struct Relay {
    source: wit_bindgen::StreamReader<u8>,
    trailers: wit_bindgen::FutureReader<u8>,
    destination: wit_bindgen::StreamWriter<u8>,
    destination_trailers: wit_bindgen::FutureWriter<u8>,
    body_id: u64,
    trailers_id: u64,
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
            body_id: open(ChannelKind::Stream),
            trailers_id: open(ChannelKind::Future),
        };
        let Some(mut queue) = RELAYS.try_lock() else {
            core::arch::wasm32::unreachable();
        };
        queue.push_back(relay);
        drop(queue);

        let accepted =
            bindings::example::stream_relay::sink::accept(0, body, forwarded_trailers).await;
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

impl bindings::exports::sleeve::platform::anchor::Guest for Component {
    async fn run() -> u32 {
        let mut cancelled = 0_u32;
        loop {
            let relay = RELAYS.try_lock().and_then(|mut queue| queue.pop_front());
            if let Some(relay) = relay {
                cancelled += u32::from(relay_one(relay).await);
            } else if STOP.load(Ordering::Relaxed) {
                return cancelled;
            } else {
                wit_bindgen::yield_async().await;
            }
        }
    }

    async fn stop() {
        STOP.store(true, Ordering::Relaxed);
    }
}

async fn relay_one(mut relay: Relay) -> bool {
    loop {
        let Some((status, bytes)) =
            until_stop(relay.source.read(Vec::with_capacity(CHUNK_SIZE))).await
        else {
            cancel(relay).await;
            return true;
        };
        if !bytes.is_empty() && !relay.destination.write_all(bytes).await.is_empty() {
            break;
        }
        if !matches!(status, wit_bindgen::StreamResult::Complete(_)) {
            break;
        }
    }
    drop(relay.destination);
    close(relay.body_id, "body").await;

    let Some(trailers) = until_stop(relay.trailers.into_future()).await else {
        close(relay.trailers_id, "trailers-cancelled").await;
        return true;
    };
    let _write = relay.destination_trailers.write(trailers).await;
    close(relay.trailers_id, "trailers").await;
    false
}

async fn until_stop<F: Future>(future: F) -> Option<F::Output> {
    let mut future = core::pin::pin!(future);
    poll_fn(|context| {
        if STOP.load(Ordering::Relaxed) {
            Poll::Ready(None)
        } else {
            context.waker().wake_by_ref();
            future.as_mut().poll(context).map(Some)
        }
    })
    .await
}

async fn cancel(relay: Relay) {
    drop(relay.destination);
    drop(relay.destination_trailers);
    close(relay.body_id, "body-cancelled").await;
    close(relay.trailers_id, "trailers-cancelled").await;
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
        STOP.store(false, Ordering::Relaxed);
        if SLEEVE
            .start(Chain::new().with(QueryPolicy), invocation)
            .is_err()
        {
            core::arch::wasm32::unreachable();
        }
    }

    fn end(invocation: alloc::string::String, trapped: bool) {
        STOP.store(true, Ordering::Relaxed);
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
