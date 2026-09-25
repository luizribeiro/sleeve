//! Policy-neutral events and policy dispatch for a composed sleeve.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod chain;
mod event;
pub mod filesystem;
mod filesystem_wrapper;
mod handles;
pub mod http;
mod http_wrapper;
mod policy;
mod sleeve;

pub use chain::{ActiveCall, Chain, Start};
pub use event::{
    Call, ChannelClosed, ChannelKind, ChannelOpened, Designator, Event, HandleDropped,
    InvocationEnded, InvocationOutcome, InvocationStarted, ProducedHandle, Provenance,
    ReturnStatus, Returned,
};
pub use handles::HandleTable;
pub use policy::{Decision, Denied, Metadata, Policy, PolicyState, Trap};
pub use sleeve::{DispatchError, Sleeve};

/// Converts a policy refusal into an interface error and traps on other dispatch failures.
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub fn dispatch_domain<T, E>(
    result: Result<Result<T, E>, DispatchError>,
    denial: E,
) -> Result<T, E> {
    match result {
        Ok(result) => result,
        Err(DispatchError::Denied(_)) => Err(denial),
        Err(error) => error.trap(),
    }
}
