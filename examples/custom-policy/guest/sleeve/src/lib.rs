//! The example's sleeve variant: one custom policy in the standard HTTP core.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use example_count_policy::{CountCalls, SummaryLog};
use sleeve_core::Chain;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../../wit/notes", "../../../../wit/http", "../../../../wit/platform", "wit"],
        world: "example:custom-policy-sleeve/sleeve@0.1.0",
        generate_all,
        ownership: Owning,
        with: {
            "wasi:http/types@0.3.0/DNS-error-payload": sleeve_core::http::DnsErrorPayload,
            "wasi:http/types@0.3.0/TLS-alert-received-payload": sleeve_core::http::TlsAlertReceivedPayload,
            "wasi:http/types@0.3.0/field-size-payload": sleeve_core::http::FieldSizePayload,
            "wasi:http/types@0.3.0/error-code": sleeve_core::http::ErrorCode,
        },
    });
}

struct Platform;

impl SummaryLog for Platform {
    fn log(&mut self, summary: &str) {
        bindings::sleeve::platform::audit::log(summary);
    }
}

sleeve_core::export_http_sleeve!(bindings, Chain::new().with(CountCalls::new(Platform)));
