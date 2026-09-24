use alloc::{borrow::Cow, string::String, vec::Vec};

/// Identifies the call or parent resource that produced a handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Provenance {
    /// The handle was returned by this call.
    Call(u64),
    /// The handle was derived from this parent handle.
    Parent(u64),
}

/// Names a policy-relevant part of a call using strings independent of WIT.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Designator<'a> {
    /// The designator name.
    pub key: Cow<'a, str>,
    /// The designator value.
    pub value: Cow<'a, str>,
}

impl<'a> Designator<'a> {
    /// Creates a string-valued designator.
    pub fn new(key: impl Into<Cow<'a, str>>, value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Describes a call before its effect occurs.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Call<'a> {
    /// Correlates this event with its return.
    pub id: u64,
    /// The versioned WIT interface name.
    pub interface: Cow<'a, str>,
    /// The WIT function name.
    pub function: Cow<'a, str>,
    /// Names extracted from non-resource arguments.
    pub designators: Vec<Designator<'a>>,
    /// Handles passed as arguments.
    pub handles: Vec<u64>,
}

impl<'a> Call<'a> {
    /// Creates a call with no designators or handle arguments.
    pub fn new(
        id: u64,
        interface: impl Into<Cow<'a, str>>,
        function: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            id,
            interface: interface.into(),
            function: function.into(),
            designators: Vec::new(),
            handles: Vec::new(),
        }
    }

    /// Replaces the call's designators.
    #[must_use]
    pub fn with_designators(mut self, designators: Vec<Designator<'a>>) -> Self {
        self.designators = designators;
        self
    }

    /// Replaces the call's handle arguments.
    #[must_use]
    pub fn with_handles(mut self, handles: Vec<u64>) -> Self {
        self.handles = handles;
        self
    }
}

/// Describes a resource produced by a call.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ProducedHandle {
    /// Sleeve-local handle identifier.
    pub id: u64,
    /// Fully qualified WIT resource type.
    pub resource_type: String,
    /// The call or parent handle that produced it.
    pub provenance: Provenance,
}

impl ProducedHandle {
    /// Records a handle returned directly by a call.
    pub fn from_call(id: u64, resource_type: impl Into<String>, call_id: u64) -> Self {
        Self {
            id,
            resource_type: resource_type.into(),
            provenance: Provenance::Call(call_id),
        }
    }

    /// Records a handle derived from another handle.
    pub fn from_parent(id: u64, resource_type: impl Into<String>, parent: u64) -> Self {
        Self {
            id,
            resource_type: resource_type.into(),
            provenance: Provenance::Parent(parent),
        }
    }
}

/// Describes whether a wrapped operation returned normally.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReturnStatus {
    /// The operation returned a value.
    Ok,
    /// The operation returned an error.
    Error(String),
    /// A policy denied the operation.
    Denied(String),
    /// A policy trapped the operation.
    Trapped(String),
}

/// Describes a call after it completes or policy stops it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Returned {
    /// The matching [`Call::id`].
    pub call_id: u64,
    /// How the operation ended.
    pub status: ReturnStatus,
    /// Handles produced by the operation.
    pub handles: Vec<ProducedHandle>,
}

impl Returned {
    /// Creates a return event with no produced handles.
    #[must_use]
    pub const fn new(call_id: u64, status: ReturnStatus) -> Self {
        Self {
            call_id,
            status,
            handles: Vec::new(),
        }
    }

    /// Replaces the handles produced by the call.
    #[must_use]
    pub fn with_handles(mut self, handles: Vec<ProducedHandle>) -> Self {
        self.handles = handles;
        self
    }
}

/// Identifies a writable channel shape without assigning policy meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChannelKind {
    /// A stream that accepts later writes.
    Stream,
    /// A future whose result can reach a sink.
    Future,
    /// A protocol-specific writable channel.
    Other,
}

/// Describes a writable channel before the core records it as open.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChannelOpened {
    /// Handle used for later writes.
    pub handle: u64,
    /// Shape of the channel.
    pub kind: ChannelKind,
    /// Call that produced the channel.
    pub call_id: u64,
}

impl ChannelOpened {
    /// Creates a pending channel-open state change.
    #[must_use]
    pub const fn new(handle: u64, kind: ChannelKind, call_id: u64) -> Self {
        Self {
            handle,
            kind,
            call_id,
        }
    }
}

/// Describes a writable channel that can no longer be used.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChannelClosed {
    /// Handle that ceased to be writable.
    pub handle: u64,
}

impl ChannelClosed {
    /// Creates a channel-close event.
    #[must_use]
    pub const fn new(handle: u64) -> Self {
        Self { handle }
    }
}

/// Describes a resource handle released by the plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct HandleDropped {
    /// Released handle identifier.
    pub handle: u64,
}

impl HandleDropped {
    /// Creates a handle-drop event.
    #[must_use]
    pub const fn new(handle: u64) -> Self {
        Self { handle }
    }
}

/// Marks the beginning of one plugin invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct InvocationStarted {
    /// Embedder-provided invocation identifier.
    pub invocation: String,
}

impl InvocationStarted {
    /// Creates an invocation-start event.
    pub fn new(invocation: impl Into<String>) -> Self {
        Self {
            invocation: invocation.into(),
        }
    }
}

/// Describes the externally visible end state of an invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum InvocationOutcome {
    /// The plugin returned normally.
    Returned,
    /// The plugin trapped.
    Trapped,
}

/// Marks the end of one plugin invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct InvocationEnded {
    /// Embedder-provided invocation identifier.
    pub invocation: String,
    /// How the invocation ended.
    pub outcome: InvocationOutcome,
}

impl InvocationEnded {
    /// Creates an invocation-end event.
    pub fn new(invocation: impl Into<String>, outcome: InvocationOutcome) -> Self {
        Self {
            invocation: invocation.into(),
            outcome,
        }
    }
}

/// A neutral observation delivered to policies.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Event<'a> {
    /// An invocation began.
    InvocationStarted(InvocationStarted),
    /// An invocation ended.
    InvocationEnded(InvocationEnded),
    /// A wrapped call is about to run.
    Call(Call<'a>),
    /// A wrapped call completed or was stopped.
    Returned(Returned),
    /// A resource handle was released.
    HandleDropped(HandleDropped),
    /// A writable channel is about to open.
    ChannelOpened(ChannelOpened),
    /// A writable channel closed.
    ChannelClosed(ChannelClosed),
}
