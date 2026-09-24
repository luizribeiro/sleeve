//! An empty-chain sleeve used to measure composition overhead.
//!
//! The wrapped WIT function returns a plain string, so any future policy
//! refusal must surface to the plugin as a trap rather than a typed error.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use sleeve_core::Chain;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/notes", "../../../wit/platform", "wit"],
        world: "sleeve:passthrough/sleeve@0.1.0",
        generate_all,
        ownership: Borrowing { duplicate_if_necessary: true },
    });
}

sleeve_core::export_notes_sleeve!(bindings, Chain::new());
