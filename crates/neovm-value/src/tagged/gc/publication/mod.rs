//! Generic publication types.
//!
//! Monomorphized inputs for one retention owner. No Value, Context, World,
//! interner, or GC tracing lives here. Heap-word retention is a sibling of
//! this owner, not a method of [`PublicationPayload`]. Symbol-table
//! publication is the N3 adapter over this owner. It is not a second protocol.

mod ledger;
mod sync;

// The compile-fail fixture names the view. The N3 adapter names the
// transaction types. Both stay crate-private.
#[allow(unused_imports)]
pub(crate) use ledger::{
    Accounted, Admission, BoundWitness, Commit, OpenPublication, PublicationLedger, PublicationView,
};

/// Bytes one owner contributes to the retention quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedBytes(usize);

impl RetainedBytes {
    pub(crate) const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

/// Number of retained predecessors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetiredCount(usize);

impl RetiredCount {
    pub(crate) const fn new(count: usize) -> Self {
        Self(count)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

/// Facade inputs for one owner. These are today's symbol-table defaults, not
/// a new process knob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetentionLimits {
    bytes: RetainedBytes,
    retired: RetiredCount,
}

impl RetentionLimits {
    pub(crate) const DEFAULT_BYTES: RetainedBytes = RetainedBytes::new(64 * 1024 * 1024);
    pub(crate) const DEFAULT_RETIRED: RetiredCount = RetiredCount::new(1024);

    pub(crate) const fn new(bytes: RetainedBytes, retired: RetiredCount) -> Self {
        Self { bytes, retired }
    }

    pub(crate) const fn production() -> Self {
        Self::new(Self::DEFAULT_BYTES, Self::DEFAULT_RETIRED)
    }
}

impl Default for RetentionLimits {
    fn default() -> Self {
        Self::production()
    }
}

/// Which retention cap refused a publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionLimit {
    RetainedBytes,
    Retired,
}

/// Publication refusal. `Payload` is the adapter's own error.
///
/// `Capacity` is a failed predecessor reservation or a byte-quote overflow.
/// `NeedGrace` is only a retiree-count cap or a retained-byte cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationError<E> {
    Payload(E),
    Capacity,
    NeedGrace(RetentionLimit),
    Busy,
    Poisoned,
}

/// Result of consuming a prepared transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationChange<G> {
    Unchanged {
        generation: G,
    },
    Published {
        generation: G,
        charged: RetainedBytes,
    },
}

impl<G: Copy> PublicationChange<G> {
    pub(crate) fn charged(self) -> RetainedBytes {
        match self {
            Self::Unchanged { .. } => RetainedBytes::new(0),
            Self::Published { charged, .. } => charged,
        }
    }
}

/// Nonblocking snapshot of the metadata mutex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublicationStatistics<G> {
    pub(crate) generation: G,
    pub(crate) retired: RetiredCount,
    pub(crate) retained_bytes: RetainedBytes,
}

/// What `prepare` decided. The replacement is already initialized by the
/// payload under its own backing authority.
#[derive(Debug)]
pub(crate) enum PayloadPlan<T> {
    Unchanged,
    Replacement(T),
}

/// Closed, monomorphized payload. Prefix laws stay in the adapter.
///
/// `published_address` is stable when the owner moves into the predecessor
/// vector. `prepare` returns `Unchanged` or a checked replacement; it does
/// not publish.
pub(crate) trait PublicationPayload: Sized {
    type Error;
    type Generation: Copy;

    fn published_address(&self) -> *mut u8;

    fn retained_bytes(&self) -> RetainedBytes;

    /// Bytes of predecessor-vector storage charged for one reserved slot.
    /// Payloads with no retained vector return zero.
    fn storage_overhead() -> usize {
        0
    }

    fn generation(&self) -> Self::Generation;

    fn prepare(current: &Self) -> Result<PayloadPlan<Self>, Self::Error>;
}
