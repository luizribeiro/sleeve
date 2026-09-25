use alloc::{boxed::Box, string::ToString, vec::Vec};
use core::any::Any;

use crate::{
    Call, ChannelOpened, Decision, Denied, Event, HandleTable, Metadata, Policy, PolicyState,
    ReturnStatus, Returned, Trap,
};

trait ErasedPolicy {
    fn observe(&mut self, event: &Event<'_>);
    fn before(&mut self, state: &PolicyState<'_>, call: &Call<'_>)
    -> Decision<Box<dyn Any + Send>>;
    fn after(
        &mut self,
        call: &Call<'_>,
        frame: Box<dyn Any + Send>,
        returned: &Returned,
    ) -> Vec<Metadata>;
    fn before_state_change(&mut self, state: &PolicyState<'_>, opened: &ChannelOpened) -> Decision;
}

impl<P: Policy> ErasedPolicy for P {
    fn observe(&mut self, event: &Event<'_>) {
        Policy::observe(self, event);
    }

    fn before(
        &mut self,
        state: &PolicyState<'_>,
        call: &Call<'_>,
    ) -> Decision<Box<dyn Any + Send>> {
        match Policy::before(self, state, call) {
            Decision::Allow(frame) => Decision::Allow(Box::new(frame)),
            Decision::Deny(denied) => Decision::Deny(denied),
            Decision::Trap(trap) => Decision::Trap(trap),
        }
    }

    fn after(
        &mut self,
        call: &Call<'_>,
        frame: Box<dyn Any + Send>,
        returned: &Returned,
    ) -> Vec<Metadata> {
        let Ok(frame) = frame.downcast::<P::Frame>() else {
            return Vec::new();
        };
        Policy::after(self, call, *frame, returned)
    }

    fn before_state_change(&mut self, state: &PolicyState<'_>, opened: &ChannelOpened) -> Decision {
        Policy::before_state_change(self, state, opened)
    }
}

/// Frames retained for policies that allowed a call.
pub struct ActiveCall {
    frames: Vec<(usize, Box<dyn Any + Send>)>,
}

/// Result of running the chain before a call.
#[non_exhaustive]
pub enum Start {
    /// Every policy allowed the call.
    Allowed(ActiveCall),
    /// A policy returned a typed refusal.
    Denied(Denied),
    /// A policy requested a trap.
    Trap(Trap),
}

/// Ordered policies compiled into one sleeve variant.
///
/// ```
/// use sleeve_core::{Call, Chain, Decision, Metadata, Policy, PolicyState, Returned};
///
/// struct Allow;
/// impl Policy for Allow {
///     type Frame = ();
///     fn before(&mut self, _: &PolicyState<'_>, _: &Call<'_>) -> Decision<()> {
///         Decision::Allow(())
///     }
///     fn after(&mut self, _: &Call<'_>, (): (), _: &Returned) -> Vec<Metadata> {
///         Vec::new()
///     }
/// }
///
/// let chain = Chain::new().with(Allow);
/// # let _ = chain;
/// ```
#[derive(Default)]
pub struct Chain {
    policies: Vec<Box<dyn ErasedPolicy + Send>>,
}

impl Chain {
    /// Creates an empty chain without allocating.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            policies: Vec::new(),
        }
    }

    /// Appends a policy in before-hook order.
    #[must_use]
    pub fn with<P: Policy>(mut self, policy: P) -> Self {
        self.policies.push(Box::new(policy));
        self
    }

    /// Delivers an event that is not part of call or open-channel dispatch.
    pub fn observe(&mut self, event: &Event<'_>) {
        for policy in &mut self.policies {
            policy.observe(event);
        }
    }

    /// Runs before hooks in order and unwinds allowed policies on refusal.
    pub fn start_call(&mut self, state: &PolicyState<'_>, call: &Call<'_>) -> Start {
        let mut frames = Vec::with_capacity(self.policies.len());
        for index in 0..self.policies.len() {
            match self.policies[index].before(state, call) {
                Decision::Allow(frame) => frames.push((index, frame)),
                Decision::Deny(denied) => {
                    let returned = Returned {
                        call_id: call.id,
                        status: ReturnStatus::Denied(denied.message().to_string()),
                        handles: Vec::new(),
                    };
                    self.finish_frames(call, frames, &returned, &mut HandleTable::new());
                    return Start::Denied(denied);
                }
                Decision::Trap(trap) => {
                    let returned = Returned {
                        call_id: call.id,
                        status: ReturnStatus::Trapped(trap.message().to_string()),
                        handles: Vec::new(),
                    };
                    self.finish_frames(call, frames, &returned, &mut HandleTable::new());
                    return Start::Trap(trap);
                }
            }
        }
        Start::Allowed(ActiveCall { frames })
    }

    /// Runs after hooks in reverse order and records returned-handle metadata.
    pub fn finish_call(
        &mut self,
        call: &Call<'_>,
        active: ActiveCall,
        returned: &Returned,
        handles: &mut HandleTable,
    ) {
        for handle in &returned.handles {
            handles.insert(handle.clone());
        }
        self.finish_frames(call, active.frames, returned, handles);
    }

    /// Consults policies before a writable channel is recorded as open.
    pub fn before_state_change(
        &mut self,
        state: &PolicyState<'_>,
        opened: &ChannelOpened,
    ) -> Decision {
        for policy in &mut self.policies {
            match policy.before_state_change(state, opened) {
                Decision::Allow(()) => {}
                other => return other,
            }
        }
        Decision::Allow(())
    }

    fn finish_frames(
        &mut self,
        call: &Call<'_>,
        frames: Vec<(usize, Box<dyn Any + Send>)>,
        returned: &Returned,
        handles: &mut HandleTable,
    ) {
        for (index, frame) in frames.into_iter().rev() {
            for metadata in self.policies[index].after(call, frame, returned) {
                handles.attach(metadata);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::{string::String, sync::Arc, vec, vec::Vec};
    use std::sync::Mutex;

    use super::*;
    use crate::{ChannelKind, ProducedHandle, Provenance};

    struct Recorder {
        name: &'static str,
        events: Arc<Mutex<Vec<String>>>,
        decision: fn() -> Decision<&'static str>,
        veto: bool,
    }

    impl Policy for Recorder {
        type Frame = &'static str;

        fn before(&mut self, _: &PolicyState<'_>, _: &Call<'_>) -> Decision<Self::Frame> {
            self.events
                .lock()
                .unwrap()
                .push(alloc::format!("before {}", self.name));
            (self.decision)()
        }

        fn after(
            &mut self,
            _: &Call<'_>,
            frame: Self::Frame,
            returned: &Returned,
        ) -> Vec<Metadata> {
            self.events
                .lock()
                .unwrap()
                .push(alloc::format!("after {frame}"));
            returned
                .handles
                .iter()
                .map(|handle| Metadata::new(handle.id, frame))
                .collect()
        }

        fn before_state_change(&mut self, _: &PolicyState<'_>, _: &ChannelOpened) -> Decision {
            if self.veto {
                Decision::Deny(Denied::new("closed", 7_u8))
            } else {
                Decision::Allow(())
            }
        }
    }

    fn allow() -> Decision<&'static str> {
        Decision::Allow("allowed")
    }
    fn deny() -> Decision<&'static str> {
        Decision::Deny(Denied::new("no", 9_u16))
    }
    fn trap() -> Decision<&'static str> {
        Decision::Trap(Trap::new("stop"))
    }
    fn call() -> Call<'static> {
        Call {
            id: 1,
            interface: "example:notes/notes@0.1.0".into(),
            function: "read".into(),
            designators: Vec::new(),
            handles: Vec::new(),
        }
    }

    #[test]
    fn orders_hooks_and_returns_frames_to_their_policy() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut chain = Chain::new()
            .with(Recorder {
                name: "outer",
                events: Arc::clone(&events),
                decision: allow,
                veto: false,
            })
            .with(Recorder {
                name: "inner",
                events: Arc::clone(&events),
                decision: allow,
                veto: false,
            });
        let Start::Allowed(active) = chain.start_call(&PolicyState::new(&[]), &call()) else {
            unreachable!()
        };
        let returned = Returned {
            call_id: 1,
            status: ReturnStatus::Ok,
            handles: vec![ProducedHandle {
                id: 4,
                resource_type: "example:item".into(),
                provenance: Provenance::Call(1),
            }],
        };
        let mut handles = HandleTable::new();
        chain.finish_call(&call(), active, &returned, &mut handles);
        assert_eq!(
            &*events.lock().unwrap(),
            &[
                "before outer",
                "before inner",
                "after allowed",
                "after allowed"
            ]
        );
        assert_eq!(handles.provenance(4), Some(Provenance::Call(1)));
        assert_eq!(handles.metadata::<&str>(4), Some(&"allowed"));
    }

    #[test]
    fn denial_and_trap_stop_the_chain_and_unwind_allowed_frames() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut denied = Chain::new()
            .with(Recorder {
                name: "outer",
                events: Arc::clone(&events),
                decision: allow,
                veto: false,
            })
            .with(Recorder {
                name: "deny",
                events: Arc::clone(&events),
                decision: deny,
                veto: false,
            })
            .with(Recorder {
                name: "unseen",
                events: Arc::clone(&events),
                decision: allow,
                veto: false,
            });
        let Start::Denied(error) = denied.start_call(&PolicyState::new(&[]), &call()) else {
            unreachable!()
        };
        assert_eq!(error.downcast_ref::<u16>(), Some(&9));
        assert_eq!(
            &*events.lock().unwrap(),
            &["before outer", "before deny", "after allowed"]
        );
        let mut trapped = Chain::new().with(Recorder {
            name: "trap",
            events,
            decision: trap,
            veto: false,
        });
        let Start::Trap(error) = trapped.start_call(&PolicyState::new(&[]), &call()) else {
            unreachable!()
        };
        assert_eq!(error.message(), "stop");
    }

    #[test]
    fn a_policy_can_veto_a_channel_opening() {
        let mut chain = Chain::new().with(Recorder {
            name: "veto",
            events: Arc::new(Mutex::new(Vec::new())),
            decision: allow,
            veto: true,
        });
        let opened = ChannelOpened {
            handle: 3,
            kind: ChannelKind::Stream,
            call_id: 1,
            sink: None,
        };
        let Decision::Deny(error) = chain.before_state_change(&PolicyState::new(&[]), &opened)
        else {
            unreachable!()
        };
        assert_eq!(error.downcast_ref::<u8>(), Some(&7));
    }
}
