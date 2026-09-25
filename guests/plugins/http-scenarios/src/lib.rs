//! Exercises the HTTP and note ordering decisions made by a sleeve.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::{format, string::String, string::ToString, vec};
use bindings::wasi::http::types::{ErrorCode, Fields, Request, Scheme};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/http", "wit"],
        world: "example:http-scenarios/plugin@0.1.0",
        generate_all,
        ownership: Owning,
    });
}

struct Component;

impl bindings::Guest for Component {
    async fn run(scenario: u8, authority: String, body_size: u32) -> String {
        match scenario {
            0 => {
                let _note = bindings::example::notes::notes::read("secret".into()).await;
                fetch_empty(authority).await
            }
            1 => {
                let status = fetch_empty(authority).await;
                let note = bindings::example::notes::notes::read("secret".into()).await;
                format!("{status} {note}")
            }
            2 => {
                let headers = Fields::new();
                let (_body, body_reader) = bindings::wit_stream::new();
                let (_trailers, trailers_reader) = bindings::wit_future::new(|| Ok(None));
                let (request, result) =
                    Request::new(headers, Some(body_reader), trailers_reader, None);
                drop(result);
                configure(&request, &authority);
                bindings::example::notes::notes::read("secret".into()).await
            }
            3 => {
                let status = fetch_body(authority, body_size).await;
                let note = bindings::example::notes::notes::read("secret".into()).await;
                format!("{status} {note}")
            }
            4 => fetch_body(authority, body_size).await,
            5 | 6 => fetch_empty(authority).await,
            7 => {
                let status = fetch_without_authority().await;
                let note = bindings::example::notes::notes::read("secret".into()).await;
                format!("{status} {note}")
            }
            _ => "unknown scenario".into(),
        }
    }
}

async fn fetch_empty(authority: String) -> String {
    let headers = Fields::new();
    let (trailers, trailers_reader) = bindings::wit_future::new(|| Ok(None));
    drop(trailers);
    let (request, result) = Request::new(headers, None, trailers_reader, None);
    drop(result);
    configure(&request, &authority);
    map_response(bindings::wasi::http::client::send(request).await)
}

async fn fetch_body(authority: String, body_size: u32) -> String {
    let headers = Fields::new();
    let (mut body, body_reader) = bindings::wit_stream::new();
    let (trailers, trailers_reader) = bindings::wit_future::new(|| Ok(None));
    let (request, result) = Request::new(headers, Some(body_reader), trailers_reader, None);
    drop(result);
    configure(&request, &authority);
    wit_bindgen::spawn_local(async move {
        let remaining = body.write_all(vec![b'x'; body_size as usize]).await;
        drop(remaining);
        drop(body);
        drop(trailers);
    });
    map_response(bindings::wasi::http::client::send(request).await)
}

async fn fetch_without_authority() -> String {
    let headers = Fields::new();
    let (trailers, trailers_reader) = bindings::wit_future::new(|| Ok(None));
    drop(trailers);
    let (request, result) = Request::new(headers, None, trailers_reader, None);
    drop(result);
    let _scheme = request.set_scheme(Some(&Scheme::Http));
    map_response(bindings::wasi::http::client::send(request).await)
}

fn configure(request: &Request, authority: &str) {
    let _method = request.set_method(&bindings::wasi::http::types::Method::Post);
    let _scheme = request.set_scheme(Some(&Scheme::Http));
    let _authority = request.set_authority(Some(authority));
    let _path = request.set_path_with_query(Some("/report"));
}

fn map_response(response: Result<bindings::wasi::http::types::Response, ErrorCode>) -> String {
    match response {
        Ok(response) => response.get_status_code().to_string(),
        Err(ErrorCode::HttpRequestBodySize(_)) => "body too large".into(),
        Err(ErrorCode::HttpRequestDenied) => "request denied".into(),
        Err(ErrorCode::HttpRequestUriInvalid) => "invalid request".into(),
        Err(error) => format!("HTTP error: {error:?}"),
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
