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
/// itself. `plist` joined with the stage-3 split (C10): the Lisp-command
/// plist layer sits below the evaluator next to its `symbol::plist_core`
/// primitives, and core re-exports it as `emacs_core::plist`.
/// `string_pos_cache` joined with the stage-3 split (C12): the
/// character<->byte position cache is value-shaped and moved down beside
/// the substring/concat cores; core re-exports it as
/// `emacs_core::string_pos_cache`.
/// `quit` joined with the stage-3b T4.1 move: the cross-thread quit request
/// and the asynchronous attention word are std-only and name no evaluator
/// item; core re-exports them through `emacs_core::eval`.
/// `heap_registry` joined with the stage-3b T3 move: the Context-owned
/// semantic-registry substrate binds std plus
/// `tagged::gc::current_tagged_heap_identity` only, and names no evaluator
/// item; core re-exports it through the `emacs_core::heap_registry` facade.
/// `system_vocab` joined with the stage-4 W1/E3 TYPE-DOWN wave: errno,
/// platform facts, the C-feature table, path_exec, the wait timeout and the
/// startup constants are pure std data/helpers naming no evaluator item;
/// core keeps every historical module path as a re-export.
pub(crate) const BOTTOM_PEER_MODULES: [&str; 9] = [
    "tagged",
    "heap_types",
    "gc_trace",
    "tls_fingerprint",
    "plist",
    "string_pos_cache",
    "quit",
    "heap_registry",
    "system_vocab",
];

/// The first lower crate of the extraction. Its modules are already inside
/// the moving set; the coherence ratchet scans it under this path prefix.
pub(crate) const MOVING_CRATE: &str = "neovm_value";

/// The lower crates whose modules are already inside the moving set.  N4
/// moved the display-identity vocabulary into a second leaf, so a moving file
/// that reaches either crate through a core-root re-export stays internal
/// (main c1933304ad's two-crate set).
/// Stage-4 W5/E13 extends the B3 scope to every crate extracted so far:
/// the evaluator is the fourth lower crate of the moving set, and the
/// edit/text leaves are scanned with it. Stage 5 adds the display crate and
/// stage 6 the host-system crate. A moving file that reaches any of them
/// through a core-root re-export stays internal.
pub(crate) const MOVING_CRATES: [&str; 7] = [
    "neovm_value",
    "neovm_display_identity",
    "neovm_edit",
    "neovm_text",
    "neovm_eval",
    "neovm_display",
    "neovm_system",
];

pub(crate) fn is_g0_member(name: &str) -> bool {
    G0_MEMBERS.contains(&name)
}

pub(crate) fn is_bottom_peer_name(name: &str) -> bool {
    BOTTOM_PEER_MODULES.contains(&name)
}

pub(crate) fn is_moving_crate(name: &str) -> bool {
    MOVING_CRATES.contains(&name)
}
