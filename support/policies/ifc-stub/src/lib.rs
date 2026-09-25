//! A small information-flow policy used by sleeve tests and examples.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use sleeve_core::{
    Call, ChannelOpened, Decision, Denied, Metadata, Policy, PolicyState, Returned, Trap,
};

const NOTES: &str = "example:notes/notes@0.1.0";
const HTTP_CLIENT: &str = "wasi:http/client@0.3.0";
const FILESYSTEM_PREOPENS: &str = "wasi:filesystem/preopens@0.3.0";
const FILESYSTEM_TYPES: &str = "wasi:filesystem/types@0.3.0";

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
    /// The host supplied a preopen label this policy does not recognize.
    UnknownPreopenLabel,
}

#[derive(Clone, Copy)]
#[doc(hidden)]
pub enum FileLabel {
    Known(Label),
    Unknown,
}

/// Enforces a public/secret lattice and a normalized HTTP-origin allowlist.
pub struct Ifc {
    current: Label,
    allowed_origins: BTreeSet<String>,
}

/// Policy-private state retained between call hooks.
#[doc(hidden)]
pub enum Frame {
    /// No returned handle needs a label.
    None,
    /// Labels returned for preopen descriptors in order.
    Preopens(Vec<FileLabel>),
    /// A label inherited from a parent descriptor.
    Derived(Option<FileLabel>),
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

    fn denial(refusal: Refusal) -> Denied {
        let message = match refusal {
            Refusal::CloseChannel(handle) => alloc::format!("close channel {handle} first"),
            Refusal::OriginNotAllowed => "origin is not allowlisted".into(),
            Refusal::SecretToPublic => "secret data cannot flow to a public origin".into(),
            Refusal::ChannelAtSecret => "cannot open a public channel at secret".into(),
            Refusal::UnknownPreopenLabel => "preopen label is not recognized".into(),
        };
        Denied::new(message, refusal)
    }

    fn refuse<F>(refusal: Refusal) -> Decision<F> {
        Decision::Deny(Self::denial(refusal))
    }

    fn raise(&mut self, state: &PolicyState<'_>, label: Label) -> Decision<Frame> {
        if label == Label::Secret && self.current == Label::Public {
            if let Some(channel) = state.open_channels().find(|channel| {
                channel
                    .sink
                    .and_then(|sink| state.metadata::<FileLabel>(sink))
                    .is_none_or(|label| matches!(label, FileLabel::Known(Label::Public)))
            }) {
                return Decision::Deny(Self::denial(Refusal::CloseChannel(channel.handle)));
            }
            self.current = Label::Secret;
        }
        Decision::Allow(Frame::None)
    }

    fn label(designator: &str) -> FileLabel {
        match designator {
            "public" => FileLabel::Known(Label::Public),
            "secret" => FileLabel::Known(Label::Secret),
            _ => FileLabel::Unknown,
        }
    }

    fn handle_label(state: &PolicyState<'_>, call: &Call<'_>) -> Option<FileLabel> {
        call.handles
            .first()
            .and_then(|handle| state.metadata::<FileLabel>(*handle))
            .copied()
    }

    fn write_allowed(&self, label: Label) -> Decision<Frame> {
        if self.current == Label::Secret && label == Label::Public {
            Decision::Deny(Self::denial(Refusal::SecretToPublic))
        } else {
            Decision::Allow(Frame::None)
        }
    }
}

impl Policy for Ifc {
    type Frame = Frame;

    fn before(&mut self, state: &PolicyState<'_>, call: &Call<'_>) -> Decision<Self::Frame> {
        if call.interface == NOTES && call.function == "read" {
            return self.raise(state, Label::Secret);
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
        if call.interface == FILESYSTEM_PREOPENS && call.function == "get-directories" {
            let labels = call
                .designators
                .iter()
                .filter(|designator| designator.key == "label")
                .map(|designator| Self::label(&designator.value))
                .collect();
            return Decision::Allow(Frame::Preopens(labels));
        }
        if call.interface == FILESYSTEM_TYPES {
            let Some(FileLabel::Known(label)) = Self::handle_label(state, call) else {
                return Self::refuse(Refusal::UnknownPreopenLabel);
            };
            return match call.function.as_ref() {
                "[method]descriptor.open-at" => {
                    let writing = call
                        .designators
                        .iter()
                        .any(|designator| designator.key == "write" && designator.value == "true");
                    if writing {
                        match self.write_allowed(label) {
                            Decision::Allow(_) => {
                                Decision::Allow(Frame::Derived(Some(FileLabel::Known(label))))
                            }
                            Decision::Deny(denied) => Decision::Deny(denied),
                            Decision::Trap(trap) => Decision::Trap(trap),
                            _ => Decision::Trap(Trap::new("unsupported policy decision")),
                        }
                    } else {
                        match self.raise(state, label) {
                            Decision::Allow(_) => {
                                Decision::Allow(Frame::Derived(Some(FileLabel::Known(label))))
                            }
                            Decision::Deny(denied) => Decision::Deny(denied),
                            Decision::Trap(trap) => Decision::Trap(trap),
                            _ => Decision::Trap(Trap::new("unsupported policy decision")),
                        }
                    }
                }
                "[method]descriptor.stat" | "[method]descriptor.read-via-stream" => {
                    self.raise(state, label)
                }
                "[method]descriptor.write-via-stream" => self.write_allowed(label),
                _ => Decision::Allow(Frame::None),
            };
        }
        Decision::Allow(Frame::None)
    }

    fn after(&mut self, _: &Call<'_>, frame: Self::Frame, returned: &Returned) -> Vec<Metadata> {
        match frame {
            Frame::None => Vec::new(),
            Frame::Preopens(labels) => returned
                .handles
                .iter()
                .zip(labels)
                .map(|(handle, label)| Metadata::new(handle.id, label))
                .collect(),
            Frame::Derived(label) => returned
                .handles
                .first()
                .zip(label)
                .map(|(handle, label)| alloc::vec![Metadata::new(handle.id, label)])
                .unwrap_or_default(),
        }
    }

    fn before_state_change(&mut self, state: &PolicyState<'_>, opened: &ChannelOpened) -> Decision {
        let sink = opened
            .sink
            .and_then(|handle| state.metadata::<FileLabel>(handle));
        match sink {
            Some(FileLabel::Known(Label::Secret)) => Decision::Allow(()),
            Some(FileLabel::Known(Label::Public)) | None if self.current == Label::Public => {
                Decision::Allow(())
            }
            Some(FileLabel::Known(Label::Public)) | None => Self::refuse(Refusal::ChannelAtSecret),
            Some(FileLabel::Unknown) => Self::refuse(Refusal::UnknownPreopenLabel),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec;
    use core::future::Future;
    use core::task::{Context, Poll, Waker};
    use sleeve_core::{Chain, ChannelKind, Designator, DispatchError, ProducedHandle, Sleeve};

    use super::*;

    fn notes() -> Call<'static> {
        Call::new(1, NOTES, "read")
    }

    fn send(origin: &'static str) -> Call<'static> {
        Call::new(2, HTTP_CLIENT, "send").with_designators(vec![Designator::new("origin", origin)])
    }

    fn assert_refusal<F>(decision: Decision<F>, expected: &Refusal) {
        let Decision::Deny(denied) = decision else {
            panic!("expected a refusal")
        };
        assert_eq!(denied.downcast_ref::<Refusal>(), Some(expected));
    }

    fn poll_ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = core::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test future should be immediately ready"),
        }
    }

    fn filesystem_sleeve_with_first_label(label: &'static str) -> Sleeve {
        let sleeve = Sleeve::new();
        sleeve
            .start(Chain::new().with(Ifc::new([])), "files".into())
            .unwrap();
        sleeve
            .dispatch_sync_handles(
                FILESYSTEM_PREOPENS,
                "get-directories",
                vec![
                    Designator::new("name", "public"),
                    Designator::new("label", label),
                    Designator::new("name", "secret"),
                    Designator::new("label", "secret"),
                ],
                Vec::new(),
                |call_id| {
                    (
                        (),
                        vec![
                            ProducedHandle::from_call(1, "descriptor", call_id),
                            ProducedHandle::from_call(2, "descriptor", call_id),
                        ],
                    )
                },
            )
            .unwrap();
        sleeve
    }

    fn filesystem_sleeve() -> Sleeve {
        filesystem_sleeve_with_first_label("public")
    }

    fn file_call(
        sleeve: &Sleeve,
        function: &'static str,
        handle: u64,
    ) -> Result<Result<(), ()>, DispatchError> {
        poll_ready(sleeve.dispatch_result(
            FILESYSTEM_TYPES,
            function,
            Vec::new(),
            vec![handle],
            Vec::new(),
            |_| async { Ok(()) },
        ))
    }

    fn open_for_write(
        sleeve: &Sleeve,
        parent: u64,
        produced: u64,
    ) -> Result<Result<(), ()>, DispatchError> {
        poll_ready(sleeve.dispatch_result_derived(
            FILESYSTEM_TYPES,
            "[method]descriptor.open-at",
            vec![Designator::new("write", "true")],
            vec![parent],
            (produced, "descriptor", parent),
            |_| async { Ok(()) },
        ))
    }

    #[test]
    fn notes_raise_the_label_and_then_http_is_refused() {
        let mut policy = Ifc::new(["http://example.com:80".into()]);
        assert!(matches!(
            policy.before(&PolicyState::new(&[]), &notes()),
            Decision::Allow(_)
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
            Decision::Allow(_)
        ));
        assert!(matches!(
            policy.before(&PolicyState::new(&[]), &notes()),
            Decision::Allow(_)
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
            Decision::Allow(_)
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

    #[test]
    fn public_writer_refuses_secret_stat_and_stream_read() {
        for function in [
            "[method]descriptor.stat",
            "[method]descriptor.read-via-stream",
        ] {
            let sleeve = filesystem_sleeve();
            sleeve
                .open_channel_to(9, ChannelKind::Stream, 2, 1)
                .unwrap();
            let error = file_call(&sleeve, function, 2).unwrap_err();
            let DispatchError::Denied(denied) = error else {
                panic!("expected a policy refusal")
            };
            assert_eq!(
                denied.downcast_ref::<Refusal>(),
                Some(&Refusal::CloseChannel(9))
            );
        }
    }

    #[test]
    fn secret_reads_allow_secret_writes_and_refuse_public_writes() {
        let sleeve = filesystem_sleeve();
        assert!(file_call(&sleeve, "[method]descriptor.stat", 2).is_ok());
        assert!(open_for_write(&sleeve, 2, 3).is_ok());
        let error = open_for_write(&sleeve, 1, 4).unwrap_err();
        let DispatchError::Denied(denied) = error else {
            panic!("expected a policy refusal")
        };
        assert_eq!(
            denied.downcast_ref::<Refusal>(),
            Some(&Refusal::SecretToPublic)
        );
    }

    #[test]
    fn secret_state_only_opens_writers_to_secret_containers() {
        let sleeve = filesystem_sleeve();
        assert!(file_call(&sleeve, "[method]descriptor.stat", 2).is_ok());
        let error = sleeve
            .open_channel_to(9, ChannelKind::Stream, 2, 1)
            .unwrap_err();
        let DispatchError::Denied(denied) = error else {
            panic!("expected a policy refusal")
        };
        assert_eq!(
            denied.downcast_ref::<Refusal>(),
            Some(&Refusal::ChannelAtSecret)
        );
        assert!(
            sleeve
                .open_channel_to(10, ChannelKind::Stream, 3, 2)
                .is_ok()
        );
    }

    #[test]
    fn unknown_preopen_label_refuses_every_descriptor_call() {
        for function in [
            "[method]descriptor.stat",
            "[method]descriptor.read-via-stream",
            "[method]descriptor.write-via-stream",
        ] {
            let sleeve = filesystem_sleeve_with_first_label("internal");
            let error = file_call(&sleeve, function, 1).unwrap_err();
            let DispatchError::Denied(denied) = error else {
                panic!("expected a policy refusal")
            };
            assert_eq!(
                denied.downcast_ref::<Refusal>(),
                Some(&Refusal::UnknownPreopenLabel)
            );
        }

        let sleeve = filesystem_sleeve_with_first_label("internal");
        let error = open_for_write(&sleeve, 1, 3).unwrap_err();
        let DispatchError::Denied(denied) = error else {
            panic!("expected a policy refusal")
        };
        assert_eq!(
            denied.downcast_ref::<Refusal>(),
            Some(&Refusal::UnknownPreopenLabel)
        );
    }
}
