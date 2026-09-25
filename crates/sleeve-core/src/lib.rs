//! Policy-neutral events and policy dispatch for a composed sleeve.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod chain;
mod event;
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
