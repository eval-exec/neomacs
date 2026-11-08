//! Evaluator and Lisp runtime layer shared with `neovm-core`.
//!
//! Extracted from `neovm-core`; the core crate keeps its stable module
//! paths as re-exports.

pub mod emacs_core;
/// One host-cfg classification shared by the registration plan, the H3b
/// bootstrap snapshot and the T6 order gate.
pub mod host_cfg;
/// The build-script-probed activity predicate for [`host_cfg::HostCfgClass`].
///
/// Kept outside `host_cfg` because the source-only generators include that
/// file in `cargo xtask`, which has no build-script probe.
mod host_cfg_active;
pub mod host_effect;
/// The host's GNU reference build surface (X11/NS/W32), derived once and
/// carried by every per-host platform table.
pub(crate) mod platform_surface;

/// Test support shared by this crate's own unit tests and, behind
/// `test-support`, by core's test build (a dependency crate never has
/// `cfg(test)` on). See TEST-SUPPORT-DESIGN.md rev 2.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(any(test, feature = "test-support"))]
pub mod test_utils;

/// Source-only H3/H3b snapshot generator, shared with
/// `cargo xtask upper-snapshots` and used by the unit-runtime drift test.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../build_support/upper_snapshots.rs"]
pub(crate) mod upper_snapshots_build_support;

// The canonical production registration plan generator is shared with
// `cargo xtask canonical-subr-plan`; eval's cfg(test) plan-consistency test
// calls it so the checked-in TSVs cannot drift from the live sources.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../build_support/canonical_subr_plan.rs"]
pub(crate) mod canonical_subr_plan_build_support;

// The H3 upper-subr source generator is shared with
// `cargo xtask upper-subrs`; eval's cfg(test) drift test renders the tracked
// source from the H3 ledger so a hand edit or a stale ledger cannot land.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../build_support/upper_subrs_generated.rs"]
pub(crate) mod upper_subrs_generated_build_support;

// GNU seeds emacs-version from configure.ac's PACKAGE_VERSION; version.el
// derives its numeric components from that same value. The evaluator seeds
// the bare variable, so the constants live here (stage-4 W5/E13); the core
// crate keeps the historical crate-root paths as re-exports.
macro_rules! gnu_emacs_version {
    ($major:literal, $minor:literal $(, $micro:literal)?) => {
        /// The GNU Emacs version whose Lisp tree and compatibility surface we track.
        pub const GNU_EMACS_VERSION: &str =
            concat!(stringify!($major), ".", stringify!($minor) $(, ".", stringify!($micro))?);
        #[doc(hidden)]
        pub const GNU_EMACS_MAJOR_VERSION: u32 = $major;
        #[doc(hidden)]
        pub const GNU_EMACS_MINOR_VERSION: u32 = $minor;
        $(const _: u32 = $micro;)?
    };
}

gnu_emacs_version!(31, 1);
