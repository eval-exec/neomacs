//! system_font_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::display_host::{
    FrameFontRequest, SystemFontName, SystemFontRole, SystemFonts,
};
use crate::emacs_core::font::*;
use neovm_display::test_support::authority::system_font_desktop_context_for_test as desktop_context;
use neovm_eval::emacs_core::eval::ResolvedFrameFont;
use neovm_value::value::Value;

#[test]
fn desktop_style_font_name_applies_family_style_and_point_size_to_default_face() {
    let mut eval = desktop_context(crate::test_utils::new_context);
    eval.eval_str("(internal-set-lisp-face-attribute 'default :font \"Ubuntu Mono Bold 13\" nil)")
        .unwrap();
    let font = eval
        .eval_str("(frame-parameter nil 'font-parameter)")
        .unwrap();
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_bare_symbol_name(),
        Some("Ubuntu Mono")
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("size")])
            .unwrap()
            .as_int(),
        Some(17)
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("weight")])
            .unwrap()
            .as_bare_symbol_name(),
        Some("bold")
    );
    // Applying a font must not rewrite the platform preference being queried.
    // GNU defines font-get-system-font only where xsettings.c is built (X11,
    // configure.ac:7227; PGTK, configure.ac:3911) or on Haiku
    // (haikufont.c:1369).  An NS (macOS) or w32 GNU build has no such subr,
    // and neovm-display registers it under the same platform gate; where the
    // subr exists it must still answer the untouched platform preference.
    #[cfg(not(any(windows, target_os = "macos")))]
    assert_eq!(
        eval.eval_str("(font-get-system-font)")
            .unwrap()
            .as_utf8_str(),
        Some("Ubuntu Mono 13")
    );
    // Where GNU has no xsettings/haikufont build (NS, w32), the subr is
    // absent, not merely nil: (fboundp 'font-get-system-font) is nil there,
    // and GNU's own dynamic-setting.el:46 guards its call with fboundp.
    #[cfg(any(windows, target_os = "macos"))]
    assert!(
        eval.eval_str("(fboundp 'font-get-system-font)")
            .unwrap()
            .is_nil(),
        "GNU NS/w32 builds have no font-get-system-font (xsettings.c is X11/PGTK only, haikufont.c is Haiku only)"
    );
}
