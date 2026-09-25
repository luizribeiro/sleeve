//! A filesystem sleeve with tracing before the IFC policy.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use sleeve_core::Chain;
use sleeve_policy_ifc_stub::Ifc;
use sleeve_policy_trace::{AuditLog, Trace};

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/clocks", "../../../wit/filesystem", "../../../wit/platform", "wit"],
        world: "sleeve:trace-file-ifc/sleeve@0.1.0",
        generate_all,
        ownership: Owning,
        with: {
            "wasi:clocks/system-clock@0.3.0/instant": sleeve_core::filesystem::Instant,
            "wasi:filesystem/types@0.3.0/descriptor-type": sleeve_core::filesystem::DescriptorType,
            "wasi:filesystem/types@0.3.0/descriptor-stat": sleeve_core::filesystem::DescriptorStat,
            "wasi:filesystem/types@0.3.0/path-flags": sleeve_core::filesystem::PathFlags,
            "wasi:filesystem/types@0.3.0/open-flags": sleeve_core::filesystem::OpenFlags,
            "wasi:filesystem/types@0.3.0/descriptor-flags": sleeve_core::filesystem::DescriptorFlags,
            "wasi:filesystem/types@0.3.0/error-code": sleeve_core::filesystem::ErrorCode,
        },
    });
}

struct Platform;

impl AuditLog for Platform {
    fn log(&mut self, event: &str) {
        bindings::sleeve::platform::audit::log(event);
    }
}

sleeve_core::export_file_sleeve!(
    bindings,
    Chain::new().with(Trace::new(Platform)).with(Ifc::new([]))
);
