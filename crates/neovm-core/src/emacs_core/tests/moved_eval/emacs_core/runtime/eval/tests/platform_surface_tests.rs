//! Per-host platform tables (MACOS_PRODUCT_DIVERGENCES rows 87-99), class
//! (a): the `DEFVAR_BOOL` surface is keyed by the GNU REFERENCE BUILD, not by
//! the running kernel. The X11 build declares xterm.c/xfns.c/xsettings.c
//! bools and not the nsterm.m/nsfns.m ones; the NS build is the mirror.
//!
//! The scoped override comes from the signed eval test-support authority
//! fence; production derives the surface once from the host cfg and never
//! reads a runtime override.

use crate::emacs_core::eval::Context;
use crate::emacs_core::format_eval_result;

fn boundp(eval: &mut Context, name: &str) -> String {
    format_eval_result(&eval.eval_str(&format!("(boundp '{name})")))
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
    assert_eq!(surface_rows(), if cfg!(target_os = "macos") { 139 } else { 147 });
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
