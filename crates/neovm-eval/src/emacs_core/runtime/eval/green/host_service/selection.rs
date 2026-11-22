//! Main-owner selection effects.
//!
//! This is the hosted service's half of the five selection primitives: main
//! holds the display host while the requesting owner is parked. It receives
//! only the typed request (the caller already did arity, conversion and the
//! Lisp-side result) and only `&mut dyn DisplayHost`: no `Context`, so it
//! cannot run Lisp, allocate through the evaluator or touch owner-carried
//! state while main is parked. The host calls are exactly the serial direct
//! body's (`lisp/native/builtins/stubs.rs`), and a host error becomes the
//! direct body's `(error MESSAGE)` on the requester.

use crate::emacs_core::eval::protocol::DisplayHost;
use crate::host_effect::contracts::OwnedNativeData;
use crate::host_effect::native_outcome::{NativeOutcome, NativeReply};
use crate::host_effect::selection::{SelectionOwnerReply, SelectionReply, SelectionRequest};
use neomacs_display_protocol::SelectionOwner;

pub(super) fn perform_selection(
    host: &mut dyn DisplayHost,
    request: SelectionRequest,
) -> NativeOutcome {
    let reply = match request {
        SelectionRequest::ClipboardSet(text) => host
            .set_clipboard_text(text.as_deref())
            .map(|()| SelectionReply::Set),
        SelectionRequest::ClipboardGet => host.clipboard_text().map(SelectionReply::Clipboard),
        SelectionRequest::PrimarySet(text) => host
            .set_primary_selection_text(text.as_deref())
            .map(|()| SelectionReply::Set),
        SelectionRequest::PrimaryGet => host.primary_selection_text().map(SelectionReply::Primary),
        SelectionRequest::PrimaryOwner => host
            .primary_selection_owner()
            .map(|owner| SelectionReply::Owner(owner_reply(owner))),
    };
    match reply {
        Ok(reply) => NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::SelectionReply(
            reply,
        ))),
        Err(message) => NativeOutcome::Reply(NativeReply::LispError(
            OwnedNativeData::SelectionReply(SelectionReply::HostError(message)),
        )),
    }
}

fn owner_reply(owner: SelectionOwner) -> SelectionOwnerReply {
    match owner {
        SelectionOwner::ThisProcess => SelectionOwnerReply::ThisProcess,
        SelectionOwner::OtherProcess => SelectionOwnerReply::OtherProcess,
        SelectionOwner::None => SelectionOwnerReply::None,
        SelectionOwner::Unknown => SelectionOwnerReply::Unknown,
    }
}
