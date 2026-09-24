use alloc::{string::String, vec::Vec};
use core::future::Future;
use core::sync::atomic::{AtomicU64, Ordering};

use spin::{Mutex, MutexGuard};

use crate::{
    Chain, Designator, Event, HandleTable, InvocationEnded, InvocationOutcome, InvocationStarted,
    ReturnStatus, Returned, Start,
};

struct State {
    chain: Chain,
    handles: HandleTable,
}

/// Persistent policy and handle state for one component instance.
pub struct Sleeve {
    state: Mutex<Option<State>>,
    next_call: AtomicU64,
}

impl Sleeve {
    /// Creates an unstarted sleeve instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(None),
            next_call: AtomicU64::new(1),
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
        chain.observe(&Event::InvocationStarted(InvocationStarted::new(
            invocation,
        )));
        *self.try_state()? = Some(State {
            chain,
            handles: HandleTable::new(),
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
        let call = crate::Call::new(
            self.next_call.fetch_add(1, Ordering::Relaxed),
            interface,
            function,
        )
        .with_designators(designators);
        let active = {
            let mut guard = self.try_state()?;
            let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
            match state.chain.start_call(&call) {
                Start::Allowed(active) => active,
                Start::Denied(denied) => return Err(DispatchError::Denied(denied)),
                Start::Trap(trap) => return Err(DispatchError::Trap(trap)),
            }
        };
        let value = forward.await;
        {
            let mut guard = self.try_state()?;
            let state = guard.as_mut().ok_or(DispatchError::NotStarted)?;
            state.chain.finish_call(
                &call,
                active,
                &Returned::new(call.id, ReturnStatus::Ok),
                &mut state.handles,
            );
        }
        Ok(value)
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
}

impl DispatchError {
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
    use super::{DispatchError, Sleeve};

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
}
