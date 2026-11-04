//! Owned native outcomes shared by serial Green host service and released
//! native adapters. Registered Lisp values and errors remain in their owner.

use crate::host_effect::contracts::OwnedNativeData;

/// Why the requesting owner no longer wants ordinary native completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelReason {
    TargetSignal,
    TargetQuit,
    OwnerEnded,
    SessionClosing,
}

/// Native-only payloads. Registered Lisp errors remain in the root registry.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NativeReply {
    Returned(OwnedNativeData),
    LispError(OwnedNativeData),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NativeOutcome {
    Reply(NativeReply),
    Interrupted(CancelReason),
    HostClosed,
}
