//! Immutable dependency buffers leased until their certificates retire.
//!
//! Free buffers contain no World handle. A live CollectionReads owns its
//! origin World and returns the buffer there, even after switching Worlds.
use super::{LispCollectionRevision, capture_storage::CaptureReads};
use std::mem::{MaybeUninit, align_of, size_of};

#[derive(Debug)]
struct CertificateStorage {
    reads: Vec<usize>,
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
                reads: Vec::new(),
                revision,
            })
        });
        storage.reads.extend(reads.keys().copied());
        storage.reads.sort_unstable();
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

/// A completed certificate sees only sorted immutable identities. Empty
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
    pub(super) fn as_slice(&self) -> &[usize] {
        self.storage.as_ref().map_or(&[], |storage| &storage.reads)
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, usize> {
        self.as_slice().iter()
    }

    pub(super) fn revision(&self) -> Option<LispCollectionRevision> {
        self.storage.as_ref().map(|storage| storage.revision)
    }

    #[cfg(test)]
    pub(super) fn contains(&self, bits: &usize) -> bool {
        self.as_slice().binary_search(bits).is_ok()
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_certificate_accessors_are_total() {
        let mut reads = CaptureReads::default();
        let revision = LispCollectionRevision::from_sequence(0);
        assert!(reads.insert_first(7, revision));
        let mut pool = CertificatePool::default();
        let mut lease = pool.acquire(&reads, revision);
        assert_eq!(lease.as_slice(), &[7]);
        pool.recycle(&mut lease);
        assert!(lease.as_slice().is_empty());
        assert_eq!(lease.revision(), None);
        assert_eq!(lease.iter().count(), 0);
        assert!(!lease.contains(&7));
        pool.recycle(&mut lease);
        assert_eq!(pool.free.len(), 1, "retirement cannot return a lease twice");
    }
}
