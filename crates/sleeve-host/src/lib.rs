//! Host support for composing a policy sleeve in front of an untrusted plugin.
//!
//! This crate will load composed components in Wasmtime and provide the host
//! side of the sleeve's platform interface.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
