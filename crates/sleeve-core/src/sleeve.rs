use alloc::{boxed::Box, collections::VecDeque, string::String, vec::Vec};
use core::fmt::Debug;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll, Waker};

use spin::{Mutex, MutexGuard};

use crate::{
    Chain, ChannelClosed, ChannelKind, ChannelOpened, Decision, Designator, Event, HandleDropped,
    HandleTable, InvocationEnded, InvocationOutcome, InvocationStarted, PolicyState, ReturnStatus,
    Returned, Start,
};

struct State {
    chain: Chain,
    handles: HandleTable,
    open_channels: Vec<ChannelOpened>,
    relays: VecDeque<RelayFuture>,
    relay_waker: Option<Waker>,
    stopping_relays: bool,
}

/// A channel relay owned by the invocation anchor.
#[doc(hidden)]
pub type RelayFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Persistent policy and handle state for one component instance.
pub struct Sleeve {
    state: Mutex<Option<State>>,
    next_call: AtomicU64,
    next_handle: AtomicU64,
}

impl Sleeve {
    /// Creates an unstarted sleeve instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(None),
            next_call: AtomicU64::new(1),
            next_handle: AtomicU64::new(1),
        }
    }

    /// Starts an invocation with a newly constructed policy chain.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::StateBorrowed`] if policy code re-enters the
    /// sleeve while its state is being updated.
    pub fn start(&self, mut chain: Chain, invocation: String) -> Result<(), DispatchError> {
        self.next_call.store(1, Ordering::Relaxed);
        self.next_handle.store(1, Ordering::Relaxed);
        chain.observe(&Event::InvocationStarted(InvocationStarted::new(
            invocation,
        )));
        *self.try_state()? = Some(State {
            chain,
            handles: HandleTable::new(),
            open_channels: Vec::new(),
            relays: VecDeque::new(),
            relay_waker: None,
            stopping_relays: false,
        });
        Ok(())
    }

    /// Observes the end of the current invocation when one has started.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::StateBorrowed`] if policy code re-enters the
    /// sleeve while its state is being updated.
    pub fn end(&self, invocation: String, trapped: bool) -> Result<(), DispatchError> {
        let outcome = if trapped {
            InvocationOutcome::Trapped
        } else {
            InvocationOutcome::Returned
        };
        if let Some(state) = self.try_state()?.as_mut() {
            state
                .chain
                .observe(&Event::InvocationEnded(InvocationEnded::new(
                    invocation, outcome,
                )));
        }
        Ok(())
    }

    /// Runs a forwarded operation through the persistent policy chain.
    ///
    /// The state lock is released before polling `forward`, so concurrent async
    /// tasks cannot retain a mutable borrow of policy state across a suspension.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when no invocation has started or a policy
    /// denies or traps the call.
    pub async fn dispatch<T>(
        &self,
        interface: &'static str,
        function: &'static str,
        designators: Vec<Designator<'_>>,
        forward: impl Future<Output = T>,
    ) -> Result<T, DispatchError> {
        let (call, active) = self.begin_call(interface, function, designators, Vec::new())?;
        let value = forward.await;
        self.finish_call(&call, active, ReturnStatus::Ok, Vec::new())?;
        Ok(value)
    }

    /// Allocates an invocation-local identifier for a wrapped handle.
    #[must_use]
    pub fn next_handle(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::Relaxed)
    }

    /// Registers a handle produced outside a synchronous call return.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::StateBorrowed`] on synchronous re-entry.
    pub fn register_handle(&self, handle: crate::ProducedHandle) -> Result<(), DispatchError> {
        let mut guard = self.try_state()?;
        let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
        state.handles.insert(handle);
        Ok(())
    }

    /// Runs a synchronous forwarded operation through the policy chain.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when the invocation is unavailable or policy
    /// refuses the operation.
    pub fn dispatch_sync<T>(
        &self,
        interface: &'static str,
        function: &'static str,
        designators: Vec<Designator<'_>>,
        handles: Vec<u64>,
        produced: Vec<(u64, &'static str)>,
        forward: impl FnOnce(u64) -> T,
    ) -> Result<T, DispatchError> {
        let (call, active) = self.begin_call(interface, function, designators, handles)?;
        let value = forward(call.id);
        self.finish_call(&call, active, ReturnStatus::Ok, produced)?;
        Ok(value)
    }

    /// Runs a synchronous fallible operation through the policy chain.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when the invocation is unavailable or policy
    /// refuses the operation. The forwarded error remains inside the outer
    /// result and is recorded as a normal error return.
    pub fn dispatch_sync_result<T, E>(
        &self,
        interface: &'static str,
        function: &'static str,
        designators: Vec<Designator<'_>>,
        handles: Vec<u64>,
        produced: Vec<(u64, &'static str)>,
        forward: impl FnOnce(u64) -> Result<T, E>,
    ) -> Result<Result<T, E>, DispatchError>
    where
        E: Debug,
    {
        let (call, active) = self.begin_call(interface, function, designators, handles)?;
        let result = forward(call.id);
        let status = match &result {
            Ok(_) => ReturnStatus::Ok,
            Err(error) => ReturnStatus::Error(alloc::format!("{error:?}")),
        };
        let returned_handles = if result.is_ok() { produced } else { Vec::new() };
        self.finish_call(&call, active, status, returned_handles)?;
        Ok(result)
    }

    /// Runs an asynchronous fallible operation through the policy chain.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when the invocation is unavailable or policy
    /// refuses the operation. The forwarded error remains inside the outer
    /// result and is recorded as a normal error return.
    pub async fn dispatch_result<T, E, F>(
        &self,
        interface: &'static str,
        function: &'static str,
        designators: Vec<Designator<'_>>,
        handles: Vec<u64>,
        produced: Vec<(u64, &'static str)>,
        forward: impl FnOnce(u64) -> F,
    ) -> Result<Result<T, E>, DispatchError>
    where
        F: Future<Output = Result<T, E>>,
        E: Debug,
    {
        let (call, active) = self.begin_call(interface, function, designators, handles)?;
        let result = forward(call.id).await;
        let status = match &result {
            Ok(_) => ReturnStatus::Ok,
            Err(error) => ReturnStatus::Error(alloc::format!("{error:?}")),
        };
        let returned_handles = if result.is_ok() { produced } else { Vec::new() };
        self.finish_call(&call, active, status, returned_handles)?;
        Ok(result)
    }

    fn begin_call<'a>(
        &self,
        interface: &'static str,
        function: &'static str,
        designators: Vec<Designator<'a>>,
        handles: Vec<u64>,
    ) -> Result<(crate::Call<'a>, crate::ActiveCall), DispatchError> {
        let call = crate::Call::new(
            self.next_call.fetch_add(1, Ordering::Relaxed),
            interface,
            function,
        )
        .with_designators(designators)
        .with_handles(handles);
        let active = {
            let mut guard = self.try_state()?;
            let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
            let policy_state = PolicyState::new(&state.open_channels);
            match state.chain.start_call(&policy_state, &call) {
                Start::Allowed(active) => active,
                Start::Denied(denied) => return Err(DispatchError::Denied(denied)),
                Start::Trap(trap) => return Err(DispatchError::Trap(trap)),
            }
        };
        Ok((call, active))
    }

    fn finish_call(
        &self,
        call: &crate::Call<'_>,
        active: crate::ActiveCall,
        status: ReturnStatus,
        produced: Vec<(u64, &'static str)>,
    ) -> Result<(), DispatchError> {
        let handles = produced
            .into_iter()
            .map(|(id, resource_type)| crate::ProducedHandle::from_call(id, resource_type, call.id))
            .collect();
        let mut guard = self.try_state()?;
        let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
        state.chain.finish_call(
            call,
            active,
            &Returned::new(call.id, status).with_handles(handles),
            &mut state.handles,
        );
        Ok(())
    }

    /// Asks policies to approve and then records a writable channel opening.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when no invocation has started or a policy
    /// refuses the state transition.
    pub fn open_channel(
        &self,
        handle: u64,
        kind: ChannelKind,
        call_id: u64,
    ) -> Result<(), DispatchError> {
        let opened = ChannelOpened::new(handle, kind, call_id);
        let mut guard = self.try_state()?;
        let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
        let policy_state = PolicyState::new(&state.open_channels);
        match state.chain.before_state_change(&policy_state, &opened) {
            Decision::Allow(()) => {
                state.open_channels.push(opened);
                Ok(())
            }
            Decision::Deny(denied) => {
                state.chain.observe(&Event::Returned(Returned::new(
                    call_id,
                    ReturnStatus::Denied(denied.message().into()),
                )));
                Err(DispatchError::Denied(denied))
            }
            Decision::Trap(trap) => {
                state.chain.observe(&Event::Returned(Returned::new(
                    call_id,
                    ReturnStatus::Trapped(trap.message().into()),
                )));
                Err(DispatchError::Trap(trap))
            }
        }
    }

    /// Records that a writable channel can no longer carry later writes.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::StateBorrowed`] on synchronous re-entry.
    pub fn close_channel(&self, handle: u64) -> Result<(), DispatchError> {
        let mut guard = self.try_state()?;
        let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
        if let Some(index) = state
            .open_channels
            .iter()
            .position(|channel| channel.handle == handle)
        {
            state.open_channels.swap_remove(index);
            state
                .chain
                .observe(&Event::ChannelClosed(ChannelClosed::new(handle)));
        }
        Ok(())
    }

    /// Hands a relay to the invocation-long anchor task.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when no invocation is active, shutdown has
    /// started, or policy state is synchronously borrowed.
    pub fn enqueue_relay(
        &self,
        relay: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), DispatchError> {
        let mut guard = self.try_state()?;
        let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
        if state.stopping_relays {
            return Err(DispatchError::RelaysStopped);
        }
        state.relays.push_back(Box::pin(relay));
        if let Some(waker) = state.relay_waker.take() {
            waker.wake();
        }
        Ok(())
    }

    /// Waits without spinning until a relay is ready or shutdown begins.
    pub async fn next_relay(&self) -> Option<RelayFuture> {
        core::future::poll_fn(|context| self.poll_relay(context)).await
    }

    /// Stops accepting relays and wakes the idle anchor.
    ///
    /// Queued relays are dropped immediately; running relay tasks are cancelled
    /// when the anchor export returns.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when no invocation is active or policy state
    /// is synchronously borrowed.
    pub fn stop_relays(&self) -> Result<(), DispatchError> {
        let queued = {
            let mut guard = self.try_state()?;
            let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
            state.stopping_relays = true;
            if let Some(waker) = state.relay_waker.take() {
                waker.wake();
            }
            core::mem::take(&mut state.relays)
        };
        drop(queued);
        Ok(())
    }

    fn poll_relay(&self, context: &mut Context<'_>) -> Poll<Option<RelayFuture>> {
        let Ok(mut guard) = self.try_state() else {
            context.waker().wake_by_ref();
            return Poll::Pending;
        };
        let Some(state) = guard.as_mut() else {
            return Poll::Ready(None);
        };
        if let Some(relay) = state.relays.pop_front() {
            Poll::Ready(Some(relay))
        } else if state.stopping_relays {
            Poll::Ready(None)
        } else {
            state.relay_waker = Some(context.waker().clone());
            Poll::Pending
        }
    }

    /// Removes a wrapped handle and emits its close and drop observations.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::StateBorrowed`] on synchronous re-entry.
    pub fn drop_handle(&self, handle: u64) -> Result<(), DispatchError> {
        self.close_channel(handle)?;
        let mut guard = self.try_state()?;
        let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
        state.handles.remove(handle);
        state
            .chain
            .observe(&Event::HandleDropped(HandleDropped::new(handle)));
        Ok(())
    }

    fn try_state(&self) -> Result<MutexGuard<'_, Option<State>>, DispatchError> {
        self.state.try_lock().ok_or(DispatchError::StateBorrowed)
    }
}

impl Default for Sleeve {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapped call that cannot produce its WIT return value.
pub enum DispatchError {
    /// The host called an export before starting an invocation.
    NotStarted,
    /// A policy refused the operation.
    Denied(crate::Denied),
    /// A policy requested a trap.
    Trap(crate::Trap),
    /// Policy code re-entered the sleeve while its state was borrowed.
    StateBorrowed,
    /// The invocation anchor is already stopping.
    RelaysStopped,
}

impl DispatchError {
    /// Maps a policy refusal at an HTTP boundary to `HTTP-request-denied`.
    ///
    /// Traps and lifecycle errors remain errors so the WIT wrapper can trap.
    ///
    /// # Errors
    ///
    /// Returns itself when the failure must trap rather than become a typed
    /// HTTP refusal.
    pub fn into_http_denial(self) -> Result<crate::http::ErrorCode, Self> {
        match self {
            Self::Denied(_) => Ok(crate::http::ErrorCode::HttpRequestDenied),
            other => Err(other),
        }
    }

    /// Traps the current WebAssembly component call.
    #[cfg(target_arch = "wasm32")]
    pub fn trap(self) -> ! {
        core::arch::wasm32::unreachable()
    }
}

/// Implements the shared notes wrapper and lifecycle exports for a sleeve.
#[macro_export]
macro_rules! export_notes_sleeve {
    ($bindings:ident, $chain:expr) => {
        static SLEEVE: $crate::Sleeve = $crate::Sleeve::new();

        struct Component;

        impl $bindings::exports::example::notes::notes::Guest for Component {
            async fn read(name: ::alloc::string::String) -> ::alloc::string::String {
                let result = SLEEVE
                    .dispatch(
                        "example:notes/notes@0.1.0",
                        "read",
                        ::alloc::vec![$crate::Designator::new("name", name.as_str())],
                        $bindings::example::notes::notes::read(name.clone()),
                    )
                    .await;
                match result {
                    Ok(value) => value,
                    Err(error) => error.trap(),
                }
            }
        }

        impl $bindings::exports::sleeve::platform::lifecycle::Guest for Component {
            fn start(invocation: ::alloc::string::String) {
                if let Err(error) = SLEEVE.start($chain, invocation) {
                    error.trap();
                }
            }

            fn end(invocation: ::alloc::string::String, trapped: bool) {
                if let Err(error) = SLEEVE.end(invocation, trapped) {
                    error.trap();
                }
            }
        }

        #[allow(unsafe_code)]
        mod component_export {
            use super::{bindings, Component};
            bindings::export!(Component with_types_in bindings);
        }
    };
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec::Vec;
    use core::future::Future;
    use core::task::{Context, Poll, Waker};
    use std::sync::{Arc, Mutex};

    use crate::{
        Call, Chain, ChannelKind, ChannelOpened, Decision, Denied, Event, Metadata, Policy,
        PolicyState, Provenance, ReturnStatus, Returned,
    };

    use super::{DispatchError, Sleeve};

    struct RefuseWithOpenChannel;

    struct ObserveOpenRefusal(Arc<Mutex<Vec<ReturnStatus>>>);

    impl Policy for ObserveOpenRefusal {
        type Frame = ();

        fn observe(&mut self, event: &Event<'_>) {
            if let Event::Returned(returned) = event {
                self.0.lock().unwrap().push(returned.status.clone());
            }
        }

        fn before(&mut self, _: &PolicyState<'_>, _: &Call<'_>) -> Decision<Self::Frame> {
            Decision::Allow(())
        }

        fn after(&mut self, _: &Call<'_>, (): Self::Frame, _: &Returned) -> Vec<Metadata> {
            Vec::new()
        }

        fn before_state_change(&mut self, _: &PolicyState<'_>, _: &ChannelOpened) -> Decision {
            Decision::Deny(Denied::new("closed", ()))
        }
    }

    impl Policy for RefuseWithOpenChannel {
        type Frame = ();

        fn before(&mut self, state: &PolicyState<'_>, _: &Call<'_>) -> Decision<Self::Frame> {
            match state.open_channels().next() {
                Some(channel) => Decision::Deny(Denied::new("close channel", channel.handle)),
                None => Decision::Allow(()),
            }
        }

        fn after(&mut self, _: &Call<'_>, (): Self::Frame, _: &Returned) -> Vec<Metadata> {
            Vec::new()
        }
    }

    fn poll_ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = core::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => unreachable!(),
        }
    }

    #[test]
    fn nested_state_access_returns_an_error() {
        let sleeve = Sleeve::new();
        let Some(_borrow) = sleeve.state.try_lock() else {
            panic!("fresh sleeve state should be available");
        };

        assert!(matches!(
            sleeve.try_state(),
            Err(DispatchError::StateBorrowed)
        ));
    }

    #[test]
    fn policy_state_tracks_channels_until_the_core_closes_them() {
        let sleeve = Sleeve::new();
        assert!(
            sleeve
                .start(Chain::new().with(RefuseWithOpenChannel), "test".into())
                .is_ok()
        );
        assert!(sleeve.open_channel(7, ChannelKind::Stream, 1).is_ok());

        let denied =
            poll_ready(sleeve.dispatch("test:api/run", "run", Vec::new(), async {})).unwrap_err();
        let DispatchError::Denied(denied) = denied else {
            unreachable!()
        };
        assert_eq!(denied.downcast_ref::<u64>(), Some(&7));

        assert!(sleeve.close_channel(7).is_ok());
        assert!(poll_ready(sleeve.dispatch("test:api/run", "run", Vec::new(), async {})).is_ok());
    }

    #[test]
    fn channel_open_refusals_are_observable_return_events() {
        let records = Arc::new(Mutex::new(Vec::new()));
        let sleeve = Sleeve::new();
        assert!(
            sleeve
                .start(
                    Chain::new().with(ObserveOpenRefusal(Arc::clone(&records))),
                    "test".into(),
                )
                .is_ok()
        );

        assert!(matches!(
            sleeve.open_channel(7, ChannelKind::Stream, 4),
            Err(DispatchError::Denied(_))
        ));
        assert_eq!(
            &*records.lock().unwrap(),
            &[ReturnStatus::Denied("closed".into())]
        );
    }

    #[test]
    fn synchronous_dispatch_records_produced_handle_provenance() {
        let sleeve = Sleeve::new();
        assert!(
            sleeve
                .start(Chain::new().with(RefuseWithOpenChannel), "test".into())
                .is_ok()
        );
        let id = sleeve.next_handle();
        assert!(
            sleeve
                .dispatch_sync(
                    "test:api/run",
                    "make",
                    Vec::new(),
                    Vec::new(),
                    alloc::vec![(id, "test:api/item")],
                    |_| (),
                )
                .is_ok()
        );
        let state = sleeve.state.try_lock().unwrap();
        assert_eq!(
            state
                .as_ref()
                .and_then(|state| state.handles.provenance(id)),
            Some(Provenance::Call(1))
        );
    }
}
