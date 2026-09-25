//! Attempts to import an HTTP interface the sleeve does not wrap.

#![no_std]
#![deny(unsafe_code)]

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/http", "wit"],
        world: "example:http-bypass/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

impl bindings::Guest for Component {
    fn run() {
        let headers = bindings::wasi::http::types::Fields::new();
        let (trailers, trailers_reader) = bindings::wit_future::new(|| Ok(None));
        drop(trailers);
        let (request, result) =
            bindings::wasi::http::types::Request::new(headers, None, trailers_reader, None);
        drop(result);
        wit_bindgen::spawn_local(async move {
            let _response = bindings::wasi::http::handler::handle(request).await;
        });
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
