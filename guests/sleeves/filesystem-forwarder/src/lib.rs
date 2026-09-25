//! Forwards filesystem resources through one component boundary.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

#[allow(unsafe_code, missing_docs, clippy::same_length_and_capacity)]
mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "example:filesystem-forwarder/forwarder@0.1.0",
        generate_all,
        ownership: Owning,
        with: {
            "wasi:filesystem/types@0.3.0/path-flags": crate::types::PathFlags,
            "wasi:filesystem/types@0.3.0/open-flags": crate::types::OpenFlags,
            "wasi:filesystem/types@0.3.0/descriptor-flags": crate::types::DescriptorFlags,
            "wasi:filesystem/types@0.3.0/error-code": crate::types::ErrorCode,
        },
    });
}

use bindings::exports::wasi::filesystem::types::{
    Descriptor as ExportedDescriptor, GuestDescriptor,
};
use bindings::wasi::filesystem::types::Descriptor as ImportedDescriptor;
use types::{DescriptorFlags, ErrorCode, OpenFlags, PathFlags};

struct Component;

struct Descriptor(ImportedDescriptor);

impl bindings::exports::wasi::filesystem::types::Guest for Component {
    type Descriptor = Descriptor;
}

impl GuestDescriptor for Descriptor {
    fn read_via_stream(
        &self,
        offset: u64,
    ) -> (
        wit_bindgen::StreamReader<u8>,
        wit_bindgen::FutureReader<Result<(), ErrorCode>>,
    ) {
        self.0.read_via_stream(offset)
    }

    async fn open_at(
        &self,
        path_flags: PathFlags,
        path: alloc::string::String,
        open_flags: OpenFlags,
        descriptor_flags: DescriptorFlags,
    ) -> Result<ExportedDescriptor, ErrorCode> {
        self.0
            .open_at(path_flags, path, open_flags, descriptor_flags)
            .await
            .map(|descriptor| ExportedDescriptor::new(Descriptor(descriptor)))
    }
}

#[allow(missing_docs)]
mod types {
    macro_rules! flags {
        ($name:ident) => {
            #[derive(Clone, Copy)]
            pub struct $name(u8);

            impl $name {
                pub const fn empty() -> Self {
                    Self(0)
                }

                pub const fn from_bits_retain(bits: u8) -> Self {
                    Self(bits)
                }

                pub const fn bits(self) -> u8 {
                    self.0
                }
            }

            impl core::ops::BitOr for $name {
                type Output = Self;

                fn bitor(self, other: Self) -> Self {
                    Self(self.0 | other.0)
                }
            }
        };
    }

    flags!(PathFlags);
    flags!(OpenFlags);
    flags!(DescriptorFlags);

    #[derive(Clone)]
    pub enum ErrorCode {
        Access,
        Already,
        BadDescriptor,
        Busy,
        Deadlock,
        Quota,
        Exist,
        FileTooLarge,
        IllegalByteSequence,
        InProgress,
        Interrupted,
        Invalid,
        Io,
        IsDirectory,
        Loop,
        TooManyLinks,
        MessageSize,
        NameTooLong,
        NoDevice,
        NoEntry,
        NoLock,
        InsufficientMemory,
        InsufficientSpace,
        NotDirectory,
        NotEmpty,
        NotRecoverable,
        Unsupported,
        NoTty,
        NoSuchDevice,
        Overflow,
        NotPermitted,
        Pipe,
        ReadOnly,
        InvalidSeek,
        TextFileBusy,
        CrossDevice,
        Other(Option<alloc::string::String>),
    }
}

impl bindings::exports::wasi::filesystem::preopens::Guest for Component {
    fn get_directories() -> alloc::vec::Vec<(ExportedDescriptor, alloc::string::String)> {
        bindings::wasi::filesystem::preopens::get_directories()
            .into_iter()
            .map(|(descriptor, name)| (ExportedDescriptor::new(Descriptor(descriptor)), name))
            .collect()
    }
}

#[allow(unsafe_code)]
mod component_export {
    use super::{Component, bindings};
    bindings::export!(Component with_types_in bindings);
}
