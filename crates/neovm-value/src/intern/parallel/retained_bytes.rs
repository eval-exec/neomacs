//! Retained Rust payload accounting for sealed initial-obarray publication.

/// A byte charge, distinct from registry IDs, root slots and collection counts.
/// Saturation preserves the existing collector accounting policy. Convert at
/// collection-capacity and legacy bytes_since_gc boundaries only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct RetainedBytes(usize);

impl RetainedBytes {
    pub(crate) fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub(crate) fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl From<usize> for RetainedBytes {
    fn from(bytes: usize) -> Self {
        Self(bytes)
    }
}

impl From<RetainedBytes> for usize {
    fn from(bytes: RetainedBytes) -> Self {
        bytes.0
    }
}

static_assertions::assert_eq_size!(RetainedBytes, usize);
