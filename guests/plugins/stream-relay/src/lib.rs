//! Produces channels before and after an imported asynchronous call returns.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{string::ToString, vec};

use bindings::example::stream_relay::relay;

const BUFFERED_MODE: u8 = 1;
const OPEN_MODE: u8 = 2;
const CALL_SCOPED_MODE: u8 = 3;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/platform", "../../../wit/stream-relay"],
        world: "example:stream-relay/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn run(mode: u8, size: u32) -> u32 {
        let (mut body, body_reader) = bindings::wit_stream::new();
        let (trailers, trailers_reader) = bindings::wit_future::new(|| 7);

        if mode == BUFFERED_MODE {
            wit_bindgen::spawn_local(async move {
                let remaining = body.write_all(vec![b'x'; size as usize]).await;
                drop(remaining);
                drop(body);
                drop(trailers);
            });
            return relay::send(mode, body_reader, trailers_reader).await;
        }

        let accepted = relay::send(mode, body_reader, trailers_reader).await;
        relay::mark("plugin-after-send".to_string()).await;
        if mode == OPEN_MODE {
            let open = !relay::query().await;
            core::mem::forget(body);
            core::mem::forget(trailers);
            return accepted + u32::from(open);
        }

        let remaining = body.write_all(vec![b'x'; size as usize]).await;
        drop(remaining);
        drop(body);
        drop(trailers);
        if mode == CALL_SCOPED_MODE {
            return accepted;
        }
        while !relay::query().await {
            wit_bindgen::yield_async().await;
        }
        accepted
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
