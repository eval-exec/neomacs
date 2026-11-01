pub(crate) use crate::contracts::HostRequestKey;

// Keep the real module hierarchy: production pub(in ...green) restrictions
// apply unchanged. This path imports the source and its existing unit tests.
#[path = "../../../../../../src/emacs_core/runtime/eval/green/host_service/transport.rs"]
mod transport;

mod actor_tests;

#[path = "../../../../../../src/emacs_core/runtime/eval/green/host_service/pending.rs"]
mod pending;
