//! Face-remapping tests that need the core upper layers.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::display_host::FontResolveRequest;
use crate::face::{FaceHeight, FontSlant, FontWeight, FontWidth};
use neovm_display::test_support::authority::xfaces_font_remapping_context_for_test as font_remapping_context;
use neovm_eval::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFontMatch,
};
use neovm_value::value::Value;

#[test]
fn font_at_named_face_remapping_uses_the_same_precedence_as_face_font() {
    let mut eval = font_remapping_context(crate::test_utils::new_context);
    eval.eval_str("(internal-make-lisp-face 'issue360-font)")
        .unwrap();
    eval.eval_str("(internal-set-lisp-face-attribute 'issue360-font :height 150 nil)")
        .unwrap();
    eval.eval_str("(setq face-remapping-alist '((issue360-font (:height 1.2) issue360-font)))")
        .unwrap();
    let size = eval
        .eval_str("(font-get (font-at 0 nil (propertize \"M\" 'face 'issue360-font)) :size)")
        .unwrap();
    assert_eq!(size.as_int(), Some(24));
}
