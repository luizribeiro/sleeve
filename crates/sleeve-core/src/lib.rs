//! Policy-neutral events for a composed sleeve.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod event;
mod handles;
mod policy;

pub use event::{
    Call, ChannelClosed, ChannelKind, ChannelOpened, Designator, Event, HandleDropped,
    InvocationEnded, InvocationOutcome, InvocationStarted, ProducedHandle, Provenance,
    ReturnStatus, Returned,
};
pub use handles::HandleTable;
pub use policy::{Decision, Denied, Metadata, Policy, Trap};
