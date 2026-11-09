//! Which kind of scan reads the per-owner cache roots, kept apart from the
//! `tagged/gc.rs` facade (see `gc_root_stays_a_facade_after_the_domain_split`).

/// Zero-sized proof carried by [`CacheRootScan::Snapshot`].
///
/// The type exists in every build so the variant and every match over it
/// compile unconditionally (a cfg-gated variant made cross-crate matches
/// depend on two crates' feature agreement and produced E0004 under feature
/// unification).  Its only constructor is compiled under
/// `cfg(any(test, feature = "test-support"))`, so release code can bind and
/// forward a token but cannot build one - and therefore cannot construct a
/// diagnostic snapshot.  It stays crate-private: no consumer ever names it
/// (they match the variant with `..` and construct through
/// [`CacheRootScan::snapshot`]), so a crate-private type keeps it out of the
/// shrink-only public-hidden promotion ledger.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotToken {
    _private: (),
}

#[cfg(any(test, feature = "test-support"))]
impl SnapshotToken {
    pub(crate) const fn new() -> Self {
        Self { _private: () }
    }
}

/// Only a scan whose roots will be seeded can certify cache coverage of the
/// running collection. Diagnostic snapshots must not advance that stamp.
///
/// Every variant exists in every build.  A test-support-gated variant whose
/// existence depended on this crate's feature made consumers in other crates
/// (which gate their own match arms on their own features) non-exhaustive
/// whenever feature unification enabled this crate's feature without theirs.
/// The variant itself is unconditional so cross-crate matches never depend
/// on two crates' cfg agreement; `Snapshot` construction is fenced by the
/// `SnapshotToken` payload instead.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub enum CacheRootScan {
    Collection,
    /// Copy the current owner's cache roots without certifying coverage of a
    /// future collection. Green's held facade epoch keeps the collector idle.
    Parked,
    Snapshot {
        /// Construction proof; no release-build constructor exists.
        #[allow(private_interfaces)] // deliberate sealed proof; consumers match with `..`
        token: SnapshotToken,
        collection_in_progress: bool,
    },
}

#[cfg(any(test, feature = "test-support"))]
impl CacheRootScan {
    /// Diagnostic snapshot scan (test-support/test builds only): the token
    /// constructor behind it exists only under the same gate, so release
    /// code cannot construct this variant.
    pub fn snapshot(collection_in_progress: bool) -> Self {
        Self::Snapshot {
            token: SnapshotToken::new(),
            collection_in_progress,
        }
    }
}
