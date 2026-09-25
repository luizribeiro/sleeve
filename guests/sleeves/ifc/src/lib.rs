//! A notes and HTTP sleeve with the IFC policy compiled into its chain.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use sleeve_core::Chain;
use sleeve_policy_ifc_stub::Ifc;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/http", "../../../wit/platform", "wit"],
        world: "sleeve:ifc/sleeve@0.1.0",
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

sleeve_core::export_http_sleeve!(
    bindings,
    Chain::new().with(Ifc::new(
        bindings::sleeve::platform::settings::allowed_origins()
    ))
);
