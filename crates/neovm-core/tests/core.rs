//! Core integration suites in one test binary.
//!
//! Each former per-file binary linked neovm-core and Cranelift again.
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time. Tests that need a custom harness or a process-wide global
//! allocator stay separate targets in Cargo.toml.

mod common;

#[path = "pentry_inventory_generator.rs"]
mod pentry_inventory_generator;

#[path = "dump_environment.rs"]
mod dump_environment;

#[path = "compat_all.rs"]
mod compat_all;

#[path = "aot_call_bearing.rs"]
mod aot_call_bearing;

#[path = "aot_pgo.rs"]
mod aot_pgo;

#[path = "aot_spec.rs"]
mod aot_spec;

#[path = "fringe_standard_indicators.rs"]
mod fringe_standard_indicators;

#[path = "gdm_casing.rs"]
mod gdm_casing;

#[path = "gdm_columns.rs"]
mod gdm_columns;

#[path = "gdm_region.rs"]
mod gdm_region;

#[path = "gdm_widths.rs"]
mod gdm_widths;

#[path = "pixel_measurement.rs"]
mod pixel_measurement;

#[path = "coding_safety.rs"]
mod coding_safety;

#[path = "coding_detection.rs"]
mod coding_detection;

#[path = "font_coverage.rs"]
mod font_coverage;

#[path = "nmjit_n1.rs"]
mod nmjit_n1;

#[path = "thread_soak.rs"]
mod thread_soak;

// Fast-track regression for the eval-split self-recursion in
// EditHost::sync_window_positions: a buffer edit must refresh the window
// position caches instead of overflowing the stack.
#[path = "sync_window_positions_regression.rs"]
mod sync_window_positions_regression;

// Warn-delta P2 oracle: minibuffer exit restores the previous mini-window
// buffer/start/point under the GNU redisplay-hooks policy (GNU minibuf.c
// minibuffer_unwind / read_minibuf_unwind).
#[path = "warn_delta_minibuffer_restore.rs"]
mod warn_delta_minibuffer_restore;

// Warn-delta P2 row 4 oracles: file-user-uid / file-group-gid are GNU Lisp
// functions (simple.el:4826/:4838), not subrs.
#[path = "warn_delta_identity.rs"]
mod warn_delta_identity;

// Re-homed display host/boot tests (P1 B1): they need system host-input,
// core boot or core display entries.  They are folded here like every other
// core suite.  The moved bodies spell their original display-crate paths
// (`crate::window`, `crate::host_input`, `crate::test_utils`,
// `crate::emacs_core`) against the test-binary root, so the facade modules
// are re-exported at the root to keep those bodies byte-identical.
#[path = "moved_display_host.rs"]
mod moved_display_host;

pub use moved_display_host::{emacs_core, host_input, test_utils, window};
