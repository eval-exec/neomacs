//! Immutable dependency buffers leased until their certificates retire.
//!
//! Free buffers contain no World handle. A live CollectionReads owns its
//! origin World and returns the buffer there, even after switching Worlds.
use super::{LispCollectionRevision, capture_storage::CaptureReads};
use rustc_hash::FxHashSet;
use std::mem::{MaybeUninit, align_of, size_of};

#[derive(Debug)]
struct CertificateStorage {
    reads: FxHashSet<usize>,
    revision: LispCollectionRevision,
}

#[derive(Default)]
pub(super) struct CertificatePool {
    free: Vec<Box<CertificateStorage>>,
    count: usize,
}

impl CertificatePool {
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
            })
        });
        storage.reads.extend(reads.keys().copied());
        storage.revision = revision;
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
    pub(super) fn dependencies(&self) -> Option<(&FxHashSet<usize>, LispCollectionRevision)> {
        self.storage
            .as_ref()
            .map(|storage| (&storage.reads, storage.revision))
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &usize> {
        self.storage.iter().flat_map(|storage| storage.reads.iter())
    }

    #[cfg(test)]
    pub(super) fn contains(&self, bits: &usize) -> bool {
        self.storage
            .as_ref()
            .is_some_and(|storage| storage.reads.contains(bits))
    }

    #[cfg(test)]
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
        pool.recycle(&mut lease);
        assert_eq!(pool.free.len(), 1, "retirement cannot return a lease twice");
    }
}
