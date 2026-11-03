//! Single source of truth for the neovm-value moving set.
//!
//! A moving-set file belongs to either a bottom-peer top-level module or a G0
//! `emacs_core` member. The move manifest (`tmp/rehearsal/move.py` at the
//! extraction tip) is generated from exactly these roots, and every guard that
//! classifies a source path as moving imports this module, so the coherence
//! ratchet, the layering ratchet and the manifest cannot drift. Test-only; no
//! production code sees these definitions.

/// The `emacs_core` direct submodules that join `tagged`, `heap_types` and
/// `gc_trace` in the bottom `neovm-value` crate (DESIGN.md §4).
pub(crate) const G0_MEMBERS: [&str; 10] = [
    "value",
    "symbol",
    "intern",
    "error",
    "errors",
    "forward",
    "emacs_char",
    "boolvec",
    "alloc",
    "hashtab",
];

/// Top-level modules that move into `neovm-value` with G0. `tls_fingerprint`
/// is a std-only, debug-only hashing helper that the per-module carrier TLS
/// probes in G0 members (error, intern, symbol) call; it names nothing above
/// itself.
pub(crate) const BOTTOM_PEER_MODULES: [&str; 4] =
    ["tagged", "heap_types", "gc_trace", "tls_fingerprint"];

/// The first lower crate of the extraction. Its modules are already inside
/// the moving set; the coherence ratchet scans it under this path prefix.
pub(crate) const MOVING_CRATE: &str = "neovm_value";

/// The lower crates whose modules are already inside the moving set.  N4
/// moved the display-identity vocabulary into a second leaf, so a moving file
/// that reaches either crate through a core-root re-export stays internal
/// (main c1933304ad's two-crate set).
pub(crate) const MOVING_CRATES: [&str; 2] = ["neovm_value", "neovm_display_identity"];

pub(crate) fn is_g0_member(name: &str) -> bool {
    G0_MEMBERS.contains(&name)
}

pub(crate) fn is_bottom_peer_name(name: &str) -> bool {
    BOTTOM_PEER_MODULES.contains(&name)
}

pub(crate) fn is_moving_crate(name: &str) -> bool {
    MOVING_CRATES.contains(&name)
}
