//! Immutable dependency buffers leased until their certificates retire.
//!
//! Free buffers contain no World handle. A live CollectionReads owns its
//! origin World and returns the buffer there, even after switching Worlds.
use super::{LispCollectionRevision, capture_storage::CaptureReads};
use rustc_hash::FxHashSet;
use std::cell::Cell;
use std::mem::{MaybeUninit, align_of, size_of};

/// Monotonic incremental validation state for one immutable certificate.
///
/// A certificate's `storage.revision` is fixed at capture completion and its
/// `storage.reads` set never changes while leased. The mutator's mutation
/// journal only appends at strictly increasing sequence numbers, so:
/// - Any prefix `(storage.revision, sequence]` already checked and found clean
///   stays clean for all future validations while
///   `store.sequence() - storage.revision <= JOURNAL_SIZE`.
/// - Once invalidated (a dependency mutated or the `JOURNAL_SIZE` horizon from
///   `storage.revision` was exceeded), the certificate can never become valid
///   again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidationState {
    Unchecked,
    VerifiedThrough { sequence: u64 },
    Invalid,
}

#[derive(Debug)]
struct CertificateStorage {
    reads: FxHashSet<usize>,
    revision: LispCollectionRevision,
    validation: Cell<ValidationState>,
    #[cfg(any(test, feature = "test-support"))]
    full_validation_invocations: Cell<usize>,
    #[cfg(any(test, feature = "test-support"))]
    journal_slots_scanned: Cell<usize>,
}

#[derive(Default)]
pub(super) struct CertificatePool {
    free: Vec<Box<CertificateStorage>>,
    count: usize,
}

/// Cold handoff census; no additional counter is maintained by acquisition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CertificateAvailability {
    Idle,
    Leased,
}

impl CertificatePool {
    #[cold]
    pub(super) fn availability(&self) -> CertificateAvailability {
        if self.count == self.free.len() {
            CertificateAvailability::Idle
        } else {
            CertificateAvailability::Leased
        }
    }

    pub(super) fn acquire(
        &mut self,
        reads: &CaptureReads,
        revision: LispCollectionRevision,
    ) -> CertificateLease {
        let mut storage = self.free.pop().unwrap_or_else(|| {
            self.count += 1;
            // All outstanding leases may retire together. Reserve their free
            // slots on acquisition so retirement itself never has to allocate.
            self.free.reserve(self.count - self.free.len());
            Box::new(CertificateStorage {
                reads: FxHashSet::default(),
                revision,
                validation: Cell::new(ValidationState::Unchecked),
                #[cfg(any(test, feature = "test-support"))]
                full_validation_invocations: Cell::new(0),
                #[cfg(any(test, feature = "test-support"))]
                journal_slots_scanned: Cell::new(0),
            })
        });
        storage.reads.extend(reads.keys().copied());
        storage.revision = revision;
        storage.validation.set(ValidationState::Unchecked);
        #[cfg(any(test, feature = "test-support"))]
        storage.full_validation_invocations.set(0);
        #[cfg(any(test, feature = "test-support"))]
        storage.journal_slots_scanned.set(0);
        CertificateLease {
            storage: Some(storage),
            _reserve: [MaybeUninit::uninit(); LEASE_RESERVE],
        }
    }

    pub(super) fn recycle(&mut self, lease: &mut CertificateLease) {
        if let Some(mut storage) = lease.storage.take() {
            storage.reads.clear();
            self.free.push(storage);
        }
    }
}

type LegacySet = rustc_hash::FxHashSet<usize>;
const LEASE_RESERVE: usize = size_of::<LegacySet>() - size_of::<usize>();

/// A completed certificate sees only immutable identities. Empty
/// storage occurs only during retirement; all accessors remain total there.
/// Preserve the old hash set's footprint in existing redisplay TLS caches.
#[derive(Debug)]
#[repr(C)]
pub(super) struct CertificateLease {
    storage: Option<Box<CertificateStorage>>,
    _reserve: [MaybeUninit<u8>; LEASE_RESERVE],
}

const _: () = {
    assert!(size_of::<CertificateLease>() == size_of::<LegacySet>());
    assert!(align_of::<CertificateLease>() == align_of::<LegacySet>());
};

impl CertificateLease {
    pub(super) fn validate(&self, store: &super::StoreState) -> bool {
        let Some(storage) = self.storage.as_ref() else {
            return false;
        };
        let sequence = store.sequence();
        let scan_from = match storage.validation.get() {
            ValidationState::Invalid => return false,
            ValidationState::VerifiedThrough { sequence: verified } if verified == sequence => {
                return true;
            }
            ValidationState::VerifiedThrough { sequence: verified } => {
                // An observation that read zero mutable collections cannot be
                // invalidated by unrelated writes even after the bounded
                // journal wraps.
                if storage.reads.is_empty() {
                    storage
                        .validation
                        .set(ValidationState::VerifiedThrough { sequence });
                    return true;
                }
                verified
            }
            ValidationState::Unchecked => storage.revision.sequence(),
        };
        #[cfg(any(test, feature = "test-support"))]
        storage
            .full_validation_invocations
            .set(storage.full_validation_invocations.get() + 1);
        // The owner's journal cannot change without advancing its sequence;
        // this numeric validation runs no callbacks or safepoints. Keep the
        // ORIGINAL capture revision: incremental checks never rebase or extend
        // its bounded `JOURNAL_SIZE` history. World admission, lifetime and
        // transitive observation remain the caller's responsibility even when
        // this predicate is memoized.
        if !storage.reads.is_empty()
            && sequence.wrapping_sub(storage.revision.sequence()) > super::JOURNAL_SIZE as u64
        {
            storage.validation.set(ValidationState::Invalid);
            return false;
        }
        let (unchanged, scanned) =
            super::unchanged_in_range(&storage.reads, scan_from, sequence, store);
        #[cfg(any(test, feature = "test-support"))]
        storage
            .journal_slots_scanned
            .set(storage.journal_slots_scanned.get() + scanned);
        let _ = scanned;
        if unchanged {
            storage
                .validation
                .set(ValidationState::VerifiedThrough { sequence });
            true
        } else {
            storage.validation.set(ValidationState::Invalid);
            false
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn full_validation_invocations_for_test(&self) -> usize {
        self.storage
            .as_ref()
            .map_or(0, |storage| storage.full_validation_invocations.get())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn journal_slots_scanned_for_test(&self) -> usize {
        self.storage
            .as_ref()
            .map_or(0, |storage| storage.journal_slots_scanned.get())
    }

    #[cfg(test)]
    pub(super) fn dependencies(&self) -> Option<(&FxHashSet<usize>, LispCollectionRevision)> {
        self.storage
            .as_ref()
            .map(|storage| (&storage.reads, storage.revision))
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &usize> {
        self.storage.iter().flat_map(|storage| storage.reads.iter())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn contains(&self, bits: &usize) -> bool {
        self.storage
            .as_ref()
            .is_some_and(|storage| storage.reads.contains(bits))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn is_empty(&self) -> bool {
        self.storage
            .as_ref()
            .is_none_or(|storage| storage.reads.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pooled_certificate_does_not_retain_prior_dependencies() {
        let mut pool = CertificatePool::default();
        let mut reads = CaptureReads::default();
        for (generation, count) in [24, 512, 0, 1].into_iter().enumerate() {
            let revision = LispCollectionRevision::from_sequence(generation as u64);
            let expected: std::collections::BTreeSet<_> =
                (1..=count).map(|index| (index << 8) | generation).collect();
            for &bits in &expected {
                assert!(reads.insert_first(bits, revision));
            }
            let mut lease = pool.acquire(&reads, revision);
            assert_eq!(lease.dependencies().unwrap().1, revision);
            assert_eq!(lease.iter().count(), count);
            for bits in 0..=((512 << 8) | generation) {
                assert_eq!(lease.contains(&bits), expected.contains(&bits));
            }
            pool.recycle(&mut lease);
            reads.clear();
        }
        assert_eq!(pool.count, 1, "all sizes reuse the same certificate lease");
    }

    #[test]
    fn retired_certificate_accessors_are_total() {
        let mut reads = CaptureReads::default();
        let revision = LispCollectionRevision::from_sequence(0);
        assert!(reads.insert_first(7, revision));
        let mut pool = CertificatePool::default();
        let mut lease = pool.acquire(&reads, revision);
        assert!(lease.contains(&7));
        assert_eq!(lease.dependencies().unwrap().1, revision);
        pool.recycle(&mut lease);
        assert!(lease.is_empty());
        assert!(lease.dependencies().is_none());
        assert_eq!(lease.iter().count(), 0);
        assert!(!lease.contains(&7));
        assert_eq!(lease.full_validation_invocations_for_test(), 0);
        pool.recycle(&mut lease);
        assert_eq!(pool.free.len(), 1, "retirement cannot return a lease twice");
    }

    #[test]
    fn reacquired_certificate_resets_validation_at_the_same_sequence() {
        let mut pool = CertificatePool::default();
        let mut reads = CaptureReads::default();
        let original = LispCollectionRevision::current();
        assert!(reads.insert_first(8, original));
        let mut lease = pool.acquire(&reads, original);
        super::super::with_store(|store| {
            let revision = store.advance();
            store.record(8, revision);
            assert!(!lease.validate(store));
            assert_eq!(lease.full_validation_invocations_for_test(), 1);
            let sequence = store.sequence();
            pool.recycle(&mut lease);

            reads.clear();
            let current = LispCollectionRevision::from_sequence(sequence);
            assert!(reads.insert_first(16, current));
            let mut next = pool.acquire(&reads, current);
            assert_eq!(next.full_validation_invocations_for_test(), 0);
            assert_eq!(store.sequence(), sequence);
            assert!(
                next.validate(store),
                "a prior rejection at this sequence belongs to the retired lease"
            );
            assert_eq!(next.full_validation_invocations_for_test(), 1);
            assert_eq!(pool.count, 1, "the same backing allocation must be reused");
            pool.recycle(&mut next);
        });
    }
}
