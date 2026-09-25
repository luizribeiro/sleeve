//! Checks whether two asynchronous exports can advance one component instance.

#![no_std]
#![deny(unsafe_code)]

use spin::Mutex;
use wit_bindgen::FutureWriter;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "example:concurrent-exports/probe@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

static SIGNAL: Mutex<Option<FutureWriter<u32>>> = Mutex::new(None);

impl bindings::Guest for Component {
    async fn run() -> u32 {
        let (writer, reader) = bindings::wit_future::new(|| 0_u32);
        *SIGNAL.lock() = Some(writer);
        reader.await
    }

    async fn helper() -> wit_bindgen::FutureReader<u32> {
        let writer = loop {
            if let Some(writer) = SIGNAL.lock().take() {
                break writer;
            }
            wit_bindgen::yield_async().await;
        };
        let _written = writer.write(7).await;
        let (completed, reader) = bindings::wit_future::new(|| 0_u32);
        let _written = completed.write(0).await;
        reader
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
