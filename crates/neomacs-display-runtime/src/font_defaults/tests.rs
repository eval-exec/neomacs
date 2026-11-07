//! The build that links the native GSettings subscription must advertise it.
//!
//! This crate's `neovm-core` dependency enables
//! `neovm-core/desktop-font-settings` unconditionally on Linux
//! (`Cargo.toml`), so this test runs in the ordinary per-package and
//! workspace suites instead of only under a feature flag.  The core unit
//! test in `c_features/surface_test.rs` pins the same seam from the other
//! side: its `cfg(feature)` variant is the enabled build, its
//! `cfg(not(feature))` counterpart is the default core build, and this
//! crate's normal test build is the third leg (the feature is enabled by
//! the display runtime itself, not by the test invocation).
//!
//! `lisp/loadup.el:305` loads `lisp/dynamic-setting.el` only when
//! `(featurep 'dynamic-setting)` is true; GNU provides `dynamic-setting`
//! unconditionally and `system-font-setting` under
//! `(USE_CAIRO||HAVE_XFT) && (HAVE_GCONF||HAVE_GSETTINGS)`
//! (`src/xsettings.c:1406-1420`).  Both rows are gated by the moved
//! `neovm-value` system-vocabulary table, so a lost feature forward makes
//! the native preference update without ever reaching the frame.
#[cfg(target_os = "linux")]
#[test]
fn the_linked_desktop_font_subscription_is_advertised_to_lisp() {
    use neovm_core::emacs_core::c_features::{HereDecision, gnu_c_features};

    for name in ["dynamic-setting", "system-font-setting"] {
        let row = gnu_c_features()
            .into_iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("the GNU C-feature table has no {name} row"));
        assert!(
            matches!(row.here, HereDecision::Implemented { .. }),
            "{name} must be Implemented when the display runtime links the native \
             desktop-font subscription, got {:?}",
            row.here
        );
    }
}
