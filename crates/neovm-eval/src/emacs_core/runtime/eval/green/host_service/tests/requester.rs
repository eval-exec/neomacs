//! H1: requester identity and completion target are separate from the Green
//! scheduler's wait key. Today the only requester is a Green owner; the point
//! is that the registry and the completion path name `HostRequester`, not
//! `ThreadKey`, so a parallel turn can join later without a third
//! `ThreadKey` variant.

use super::*;
use crate::emacs_core::eval::{Context, ThreadingMode};
use std::num::NonZeroU64;

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime())
}

#[test]
fn host_request_requester_names_the_green_owner_and_completion_target() {
    let mut owner = green();
    let baton = TurnRight::from(owner.start_green_runtime().unwrap());
    let worker = ThreadKey::Worker(NonZeroU64::new(1).unwrap());
    owner.with_green_runtime(|runtime| {
        // SAFETY: the fixture passes an empty root slice; no collection or
        // handoff occurs in the registration call.
        let call = unsafe {
            runtime
                .parked
                .borrow_mut()
                .hosts()
                .prepare(&baton, &owner.tagged_heap, worker, &[])
        }
        .unwrap();
        assert_eq!(call.requester(), HostRequester::green(worker));
        assert_eq!(call.completion_target(), CompletionTarget::Green(worker));
        // H1 keeps the Green owner as the registry key; the abstraction is
        // the requester/completion surface.
        assert_eq!(call.owner(), worker);
    });
}

#[test]
fn completion_target_marks_only_main_local() {
    assert!(
        HostRequester::green(ThreadKey::Main)
            .completion_target()
            .is_local()
    );
    let worker = ThreadKey::Worker(NonZeroU64::new(7).unwrap());
    assert!(!HostRequester::green(worker).completion_target().is_local());
}
