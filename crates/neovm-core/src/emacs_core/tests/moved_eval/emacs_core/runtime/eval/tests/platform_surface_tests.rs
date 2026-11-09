//! Per-host platform tables (MACOS_PRODUCT_DIVERGENCES rows 87-99): the
//! `DEFVAR_BOOL` and C `DEFVAR` surfaces are keyed by the GNU REFERENCE
//! BUILD, not by the running kernel. The X11 build declares the
//! xterm.c/xfns.c/xsettings.c names and not the nsterm.m/nsfns.m ones; the NS
//! build is the mirror.
//!
//! The scoped override comes from the signed eval test-support authority
//! fence; production derives the surface once from the host cfg and never
//! reads a runtime override.

use crate::emacs_core::eval::Context;
use crate::emacs_core::format_eval_result;

fn boundp(eval: &mut Context, name: &str) -> String {
    format_eval_result(&eval.eval_str(&format!("(boundp '{name})")))
}

fn value(eval: &mut Context, expr: &str) -> String {
    format_eval_result(&eval.eval_str(expr))
}

fn surface_rows() -> usize {
    neovm_eval::test_support::authority::gnu_bool_rows_for_surface_for_test().len()
}

#[test]
fn ns_surface_binds_ns_bools_and_omits_x11_bools() {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::with_platform_surface_for_test("ns", &mut || {
        let mut eval = crate::test_utils::new_context();
        assert_eq!(boundp(&mut eval, "ns-use-native-fullscreen"), "OK t");
        assert_eq!(boundp(&mut eval, "ns-use-srgb-colorspace"), "OK t");
        assert_eq!(boundp(&mut eval, "ns-click-through"), "OK t");
        assert_eq!(boundp(&mut eval, "x-gtk-use-native-input"), "OK nil");
        assert_eq!(boundp(&mut eval, "x-dnd-disable-motif-drag"), "OK nil");
        assert_eq!(boundp(&mut eval, "font-use-system-font"), "OK nil");
        // GNU NS binds 147 - 15 + 7 = 139 DEFVAR_BOOLs (byte-boolean-vars 116
        // - 15 + 7 = 108; the erased rows are not in that list).
        assert_eq!(surface_rows(), 139);
    });
    // The override is scoped: the host surface is back afterwards.
    assert_eq!(
        surface_rows(),
        if cfg!(target_os = "macos") { 139 } else { 147 }
    );
}

#[test]
fn x11_surface_binds_x11_bools_and_omits_ns_bools() {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::with_platform_surface_for_test("x11", &mut || {
        let mut eval = crate::test_utils::new_context();
        assert_eq!(boundp(&mut eval, "x-gtk-use-native-input"), "OK t");
        assert_eq!(boundp(&mut eval, "x-dnd-disable-motif-drag"), "OK t");
        assert_eq!(boundp(&mut eval, "font-use-system-font"), "OK t");
        assert_eq!(boundp(&mut eval, "ns-use-native-fullscreen"), "OK nil");
        assert_eq!(boundp(&mut eval, "ns-click-through"), "OK nil");
        assert_eq!(surface_rows(), 147);
    });
}

/// Class (b): the C DEFVAR families are gated by the reference surface, not
/// by the running kernel.  GNU's NS build binds the `syms_of_nsterm` names
/// (`src/nsterm.m:11568-11657`) and leaves the `xterm.c`/`xfns.c`/`xselect.c`/
/// `xsettings.c` family void; the X11 build is the mirror.  Two names are
/// declared in both backends: `x-max-tooltip-size` (`xfns.c:10436`,
/// `nsfns.m:4245`) and `x-toolkit-scroll-bars` (`xterm.c:32699`,
/// `nsterm.m:11700`).
#[test]
fn ns_surface_binds_the_ns_c_family_and_omits_the_x11_c_family() {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::with_platform_surface_for_test("ns", &mut || {
        let mut eval = crate::test_utils::new_context();
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-alternate-modifier)"),
            "OK meta"
        );
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-command-modifier)"),
            "OK super"
        );
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-control-modifier)"),
            "OK control"
        );
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-function-modifier)"),
            "OK none"
        );
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-antialias-text)"),
            "OK t"
        );
        assert_eq!(boundp(&mut eval, "ns-confirm-quit"), "OK t");
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-confirm-quit)"),
            "OK nil"
        );
        assert_eq!(boundp(&mut eval, "ns-auto-hide-menu-bar"), "OK t");
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-auto-hide-menu-bar)"),
            "OK nil"
        );
        // The right-hand modifier twins default to `left' -- "use the value
        // of ns-<x>-modifier instead" (nsterm.m:11587/11612/11633) -- and
        // ns-scroll-event-delta-factor is make_float (1.0) (nsterm.m:11736).
        for name in [
            "ns-right-alternate-modifier",
            "ns-right-command-modifier",
            "ns-right-control-modifier",
        ] {
            assert_eq!(
                value(&mut eval, &format!("(symbol-value '{name})")),
                "OK left",
                "{name}"
            );
        }
        assert_eq!(
            value(&mut eval, "(symbol-value 'ns-scroll-event-delta-factor)"),
            "OK 1.0"
        );
        for name in [
            "x-keysym-table",
            "x-selection-timeout",
            "x-pointer-shape",
            "x-sensitive-text-pointer-shape",
            "x-scroll-event-delta-factor",
            "x-no-window-manager",
            "x-allow-focus-stealing",
            "x-dnd-movement-function",
            "x-session-id",
            "x-wait-for-event-timeout",
            "xft-settings",
            "x-gtk-resize-child-frames",
            "x-auto-preserve-selections",
            "x-ctrl-keysym",
        ] {
            assert_eq!(
                boundp(&mut eval, name),
                "OK nil",
                "{name} is X11-only and must be void on the NS surface"
            );
        }
        assert_eq!(boundp(&mut eval, "x-max-tooltip-size"), "OK t");
        assert_eq!(boundp(&mut eval, "x-toolkit-scroll-bars"), "OK t");
        // GNU's two per-surface initializers: nsterm.m:11700 sets
        // x-toolkit-scroll-bars to t and nsterm.m:11704 starts
        // x-use-underline-position-properties at 0.
        assert_eq!(
            value(&mut eval, "(symbol-value 'x-toolkit-scroll-bars)"),
            "OK t"
        );
        assert_eq!(
            value(
                &mut eval,
                "(symbol-value 'x-use-underline-position-properties)"
            ),
            "OK nil"
        );
    });
}

#[test]
fn x11_surface_binds_the_x11_c_family_and_omits_the_ns_c_family() {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::with_platform_surface_for_test("x11", &mut || {
        let mut eval = crate::test_utils::new_context();
        for name in [
            "x-keysym-table",
            "x-selection-timeout",
            "x-pointer-shape",
            "x-sensitive-text-pointer-shape",
            "x-scroll-event-delta-factor",
            "x-no-window-manager",
            "x-allow-focus-stealing",
            "x-dnd-movement-function",
            "x-session-id",
            "x-wait-for-event-timeout",
            "xft-settings",
            "x-gtk-resize-child-frames",
            "x-auto-preserve-selections",
            "x-ctrl-keysym",
        ] {
            assert_eq!(
                boundp(&mut eval, name),
                "OK t",
                "{name} belongs to the X11 C surface"
            );
        }
        for name in [
            "ns-alternate-modifier",
            "ns-command-modifier",
            "ns-control-modifier",
            "ns-function-modifier",
            "ns-right-alternate-modifier",
            "ns-right-command-modifier",
            "ns-right-control-modifier",
            "ns-scroll-event-delta-factor",
            "ns-antialias-text",
            "ns-confirm-quit",
            "ns-auto-hide-menu-bar",
        ] {
            assert_eq!(
                boundp(&mut eval, name),
                "OK nil",
                "{name} is NS-only and must be void on the X11 surface"
            );
        }
        assert_eq!(boundp(&mut eval, "x-max-tooltip-size"), "OK t");
        assert_eq!(boundp(&mut eval, "x-toolkit-scroll-bars"), "OK t");
        // The X11 values: xterm.c:32699 names the toolkit, and
        // xterm.c:32667 starts x-use-underline-position-properties true.
        assert_eq!(
            value(&mut eval, "(symbol-value 'x-toolkit-scroll-bars)"),
            "OK gtk"
        );
        assert_eq!(
            value(
                &mut eval,
                "(symbol-value 'x-use-underline-position-properties)"
            ),
            "OK t"
        );
    });
}
