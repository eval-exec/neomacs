use std::num::NonZeroU64;

/// Correlation shim only. This is not production session/generation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct HostRequestKey(NonZeroU64);

impl HostRequestKey {
    fn for_test(request: NonZeroU64) -> Self {
        Self(request)
    }
}

// Keep the real module hierarchy: production pub(in ...green) restrictions
// apply unchanged. This path imports the source and its existing unit tests.
#[path = "../../../../../../src/emacs_core/runtime/eval/green/host_service/transport.rs"]
mod transport;

mod actor_tests;
