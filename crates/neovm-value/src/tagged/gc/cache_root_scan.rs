//! Which kind of scan reads the per-owner cache roots, kept apart from the
//! `tagged/gc.rs` facade (see `gc_root_stays_a_facade_after_the_domain_split`).

/// Only a scan whose roots will be seeded can certify cache coverage of the
/// running collection. Diagnostic snapshots must not advance that stamp.
#[derive(Clone, Copy)]
pub(crate) enum CacheRootScan {
    Collection,
    /// Copy the current owner's cache roots without certifying coverage of a
    /// future collection. Green's held facade epoch keeps the collector idle.
    Parked,
    #[cfg(test)]
    Snapshot {
        collection_in_progress: bool,
    },
}
