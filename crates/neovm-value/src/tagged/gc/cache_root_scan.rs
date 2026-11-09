//! Which kind of scan reads the per-owner cache roots, kept apart from the
//! `tagged/gc.rs` facade (see `gc_root_stays_a_facade_after_the_domain_split`).

/// Only a scan whose roots will be seeded can certify cache coverage of the
/// running collection. Diagnostic snapshots must not advance that stamp.
///
/// Every variant exists in every build.  A test-support-gated variant whose
/// existence depended on this crate's feature made consumers in other crates
/// (which gate their own match arms on their own features) non-exhaustive
/// whenever feature unification enabled this crate's feature without theirs.
/// Constructors of `Snapshot` remain diagnostic/test-only by convention; the
/// variant itself is unconditional so cross-crate matches never depend on
/// two crates' cfg agreement.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub enum CacheRootScan {
    Collection,
    /// Copy the current owner's cache roots without certifying coverage of a
    /// future collection. Green's held facade epoch keeps the collector idle.
    Parked,
    Snapshot {
        collection_in_progress: bool,
    },
}
