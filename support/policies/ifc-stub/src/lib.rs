//! A small information-flow policy used by sleeve tests and examples.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use sleeve_core::{Call, ChannelOpened, Decision, Denied, Metadata, Policy, PolicyState, Returned};

const NOTES: &str = "example:notes/notes@0.1.0";
const HTTP_CLIENT: &str = "wasi:http/client@0.3.0";

/// The two secrecy levels modeled by this test policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Label {
    /// Data that may flow to an allowlisted network origin.
    #[default]
    Public,
    /// Data read from the notes service.
    Secret,
}

/// A typed information-flow refusal returned to a WIT wrapper.
///
/// HTTP send refusals map to `error-code.HTTP-request-denied`: the request is
/// well-formed, but policy deliberately prohibits transmitting it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// A read must wait until the named writable channel is closed.
    CloseChannel(u64),
    /// The normalized request origin is not in the invocation's allowlist.
    OriginNotAllowed,
    /// Secret data cannot flow to a public HTTP origin.
    /// This is defence in depth unless channel opening is deferred past origin selection.
    SecretToPublic,
    /// A public writable channel cannot open after a secret read.
    ChannelAtSecret,
}

/// Enforces a public/secret lattice and a normalized HTTP-origin allowlist.
pub struct Ifc {
    current: Label,
    allowed_origins: BTreeSet<String>,
}

impl Ifc {
    /// Creates a public invocation policy with normalized allowed origins.
    pub fn new(origins: impl IntoIterator<Item = String>) -> Self {
        Self {
            current: Label::Public,
            allowed_origins: origins.into_iter().collect(),
        }
    }

    /// Returns the invocation's current secrecy label.
    #[must_use]
    pub const fn current_label(&self) -> Label {
        self.current
    }

    fn refuse(refusal: Refusal) -> Decision {
        let message = match refusal {
            Refusal::CloseChannel(handle) => alloc::format!("close channel {handle} first"),
            Refusal::OriginNotAllowed => "origin is not allowlisted".into(),
            Refusal::SecretToPublic => "secret data cannot flow to a public origin".into(),
            Refusal::ChannelAtSecret => "cannot open a public channel at secret".into(),
        };
        Decision::Deny(Denied::new(message, refusal))
    }
}

impl Policy for Ifc {
    type Frame = ();

    fn before(&mut self, state: &PolicyState<'_>, call: &Call<'_>) -> Decision<Self::Frame> {
        if call.interface == NOTES && call.function == "read" {
            if let Some(channel) = state.open_channels().next() {
                return Self::refuse(Refusal::CloseChannel(channel.handle));
            }
            self.current = Label::Secret;
        }
        if call.interface == HTTP_CLIENT && call.function == "send" {
            let origin = call
                .designators
                .iter()
                .find(|designator| designator.key == "origin")
                .map(|designator| designator.value.as_ref());
            if origin.is_none_or(|origin| !self.allowed_origins.contains(origin)) {
                return Self::refuse(Refusal::OriginNotAllowed);
            }
            if self.current == Label::Secret {
                return Self::refuse(Refusal::SecretToPublic);
            }
        }
        Decision::Allow(())
    }

    fn after(&mut self, _: &Call<'_>, (): Self::Frame, _: &Returned) -> Vec<Metadata> {
        Vec::new()
    }

    fn before_state_change(&mut self, _: &PolicyState<'_>, _: &ChannelOpened) -> Decision {
        if self.current == Label::Secret {
            Self::refuse(Refusal::ChannelAtSecret)
        } else {
            Decision::Allow(())
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec;
    use sleeve_core::{ChannelKind, Designator};

    use super::*;

    fn notes() -> Call<'static> {
        Call::new(1, NOTES, "read")
    }

    fn send(origin: &'static str) -> Call<'static> {
        Call::new(2, HTTP_CLIENT, "send").with_designators(vec![Designator::new("origin", origin)])
    }

    fn assert_refusal(decision: Decision, expected: &Refusal) {
        let Decision::Deny(denied) = decision else {
            panic!("expected a refusal")
        };
        assert_eq!(denied.downcast_ref::<Refusal>(), Some(expected));
    }

    #[test]
    fn notes_raise_the_label_and_then_http_is_refused() {
        let mut policy = Ifc::new(["http://example.com:80".into()]);
        assert!(matches!(
            policy.before(&PolicyState::new(&[]), &notes()),
            Decision::Allow(())
        ));
        assert_eq!(policy.current_label(), Label::Secret);
        assert_refusal(
            policy.before(&PolicyState::new(&[]), &send("http://example.com:80")),
            &Refusal::SecretToPublic,
        );
    }

    #[test]
    fn http_then_notes_is_allowed() {
        let mut policy = Ifc::new(["http://example.com:80".into()]);
        assert!(matches!(
            policy.before(&PolicyState::new(&[]), &send("http://example.com:80"),),
            Decision::Allow(())
        ));
        assert!(matches!(
            policy.before(&PolicyState::new(&[]), &notes()),
            Decision::Allow(())
        ));
    }

    #[test]
    fn an_open_channel_refuses_a_raise_without_changing_the_label() {
        let mut policy = Ifc::new([]);
        let opened = ChannelOpened::new(9, ChannelKind::Stream, 1);
        assert_refusal(
            policy.before(&PolicyState::new(&[opened]), &notes()),
            &Refusal::CloseChannel(9),
        );
        assert_eq!(policy.current_label(), Label::Public);
    }

    #[test]
    fn secret_invocations_cannot_open_public_channels() {
        let mut policy = Ifc::new([]);
        assert!(matches!(
            policy.before(&PolicyState::new(&[]), &notes()),
            Decision::Allow(())
        ));
        assert_refusal(
            policy.before_state_change(
                &PolicyState::new(&[]),
                &ChannelOpened::new(9, ChannelKind::Future, 2),
            ),
            &Refusal::ChannelAtSecret,
        );
    }

    #[test]
    fn origins_outside_the_allowlist_are_refused() {
        let mut policy = Ifc::new(["http://example.com:80".into()]);
        assert_refusal(
            policy.before(&PolicyState::new(&[]), &send("http://elsewhere.example:80")),
            &Refusal::OriginNotAllowed,
        );
    }
}
