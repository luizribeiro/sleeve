use alloc::{boxed::Box, string::String};
use core::any::Any;

use crate::{Call, ChannelOpened, Event, Returned};

/// Read-only invocation state available while a policy makes a decision.
pub struct PolicyState<'a> {
    open_channels: &'a [ChannelOpened],
}

impl<'a> PolicyState<'a> {
    /// Creates a view over the core's current open-channel set.
    #[must_use]
    pub const fn new(open_channels: &'a [ChannelOpened]) -> Self {
        Self { open_channels }
    }

    /// Iterates over writable channels that can still carry later writes.
    #[must_use]
    pub fn open_channels(&self) -> impl ExactSizeIterator<Item = &ChannelOpened> {
        self.open_channels.iter()
    }
}

/// An opaque, typed refusal that a caller can downcast at its WIT boundary.
pub struct Denied {
    message: String,
    value: Box<dyn Any + Send>,
}

impl Denied {
    /// Creates a refusal carrying a concrete error value.
    pub fn new<T: Any + Send>(message: impl Into<String>, value: T) -> Self {
        Self {
            message: message.into(),
            value: Box::new(value),
        }
    }

    /// Returns the diagnostic form used when the WIT function has no error case.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Recovers the typed error when the caller recognizes it.
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }
}

/// A request to stop execution without returning a WIT value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trap {
    message: String,
}

impl Trap {
    /// Creates a trap with a diagnostic message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the trap's diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// A policy's decision before an effect or state transition.
#[non_exhaustive]
pub enum Decision<F = ()> {
    /// Permit the operation and retain a policy-private frame.
    Allow(F),
    /// Refuse with a typed error.
    Deny(Denied),
    /// Stop the component with a trap.
    Trap(Trap),
}

/// Opaque metadata to attach to a produced handle.
pub struct Metadata {
    pub(crate) handle: u64,
    pub(crate) value: Box<dyn Any + Send>,
}

impl Metadata {
    /// Attaches a typed value to a handle produced by the observed return.
    pub fn new<T: Any + Send>(handle: u64, value: T) -> Self {
        Self {
            handle,
            value: Box::new(value),
        }
    }
}

/// Applies one policy to neutral events emitted by a sleeve.
///
/// ```
/// use sleeve_core::{Call, Decision, Metadata, Policy, PolicyState, Returned};
///
/// struct Allow;
///
/// impl Policy for Allow {
///     type Frame = ();
///
///     fn before(&mut self, _: &PolicyState<'_>, _: &Call<'_>) -> Decision<Self::Frame> {
///         Decision::Allow(())
///     }
///
///     fn after(&mut self, _: &Call<'_>, (): (), _: &Returned) -> Vec<Metadata> {
///         Vec::new()
///     }
/// }
/// ```
pub trait Policy: Send + 'static {
    /// Per-call state returned by [`Policy::before`] and consumed by [`Policy::after`].
    type Frame: Send + 'static;

    /// Observes lifecycle, handle-drop, and channel-close events.
    fn observe(&mut self, _event: &Event<'_>) {}

    /// Allows, denies, or traps a wrapped call before its effect.
    fn before(&mut self, state: &PolicyState<'_>, call: &Call<'_>) -> Decision<Self::Frame>;

    /// Observes the return in reverse chain order and may label produced handles.
    fn after(
        &mut self,
        call: &Call<'_>,
        frame: Self::Frame,
        returned: &Returned,
    ) -> alloc::vec::Vec<Metadata>;

    /// Allows, denies, or traps a channel opening before the core records it.
    fn before_state_change(
        &mut self,
        _state: &PolicyState<'_>,
        _opened: &ChannelOpened,
    ) -> Decision {
        Decision::Allow(())
    }
}
