//! Produces channels before and after an imported asynchronous call returns.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{string::ToString, vec};
use core::sync::atomic::{AtomicBool, Ordering};

use bindings::example::stream_relay::relay;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/platform", "../../../wit/stream-relay"],
        world: "example:stream-relay/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

static RELEASE: AtomicBool = AtomicBool::new(false);

struct Component;

impl bindings::Guest for Component {
    async fn run(mode: u8, size: u32) -> u32 {
        RELEASE.store(false, Ordering::Relaxed);
        let (mut body, body_reader) = bindings::wit_stream::new();
        let (trailers, trailers_reader) = bindings::wit_future::new(|| 7);

        if mode == 2 {
            core::mem::forget(body);
            core::mem::forget(trailers);
        } else {
            wit_bindgen::spawn_local(async move {
                if mode == 0 {
                    while !RELEASE.load(Ordering::Relaxed) {
                        wit_bindgen::yield_async().await;
                    }
                }
                let remaining = body.write_all(vec![b'x'; size as usize]).await;
                drop(remaining);
                drop(body);
                drop(trailers);
            });
        }

        let accepted = relay::send(mode, body_reader, trailers_reader).await;
        relay::mark("plugin-after-send".to_string()).await;
        RELEASE.store(true, Ordering::Relaxed);
        accepted
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
