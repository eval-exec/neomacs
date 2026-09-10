//! Existing identities shared by input-method messages and observations.

/// Host-issued identity, never reused during an editor session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSessionId(pub u64);

/// VM-issued identity of one surrounding-text observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSnapshotId(pub u64);
