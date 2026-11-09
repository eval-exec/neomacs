//! system_font_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::display_host::{
    FrameFontRequest, SystemFontName, SystemFontRole, SystemFonts,
};
use crate::emacs_core::font::*;
use neovm_display::test_utils::system_font_test_upper_layer_helpers::desktop_context;
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
    assert_eq!(
        eval.eval_str("(font-get-system-font)")
            .unwrap()
            .as_utf8_str(),
        Some("Ubuntu Mono 13")
    );
}
