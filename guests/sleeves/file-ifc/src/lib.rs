//! A filesystem sleeve with the IFC policy compiled into its chain.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use sleeve_core::Chain;
use sleeve_policy_ifc_stub::Ifc;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/clocks", "../../../wit/filesystem", "../../../wit/platform", "wit"],
        world: "sleeve:file-ifc/sleeve@0.1.0",
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

sleeve_core::export_file_sleeve!(bindings, Chain::new().with(Ifc::new([])));
