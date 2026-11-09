use crate::emacs_core::display_host::FontResolveRequest;
use crate::face::{FaceHeight, FontSlant, FontWeight, FontWidth};
use neovm_eval::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFontMatch,
};
use neovm_value::value::Value;

/// Harness-bound wrapper over the shared upper-layer helper (one home:
/// display test_utils); display tests keep their unit-runtime Context.
fn context() -> Context {
    crate::test_utils::xfaces_upper_layer_helpers::font_remapping_context(|| {
        Context::new(crate::test_utils::unit_runtime())
    })
}

/// Deterministic font-system adapter: point sizes at 96 DPI, no font cache.
/// Face definitions, remapping, and Lisp queries all use the real evaluator.
fn default_font_size(eval: &mut Context) -> i64 {
    eval.eval_str("(font-get (font-spec :name (face-font 'default)) :size)")
        .unwrap()
        .as_int()
        .unwrap()
}

#[test]
fn face_font_relative_remapping_takes_precedence_over_its_base_face() {
    let mut eval = context();
    assert_eq!(default_font_size(&mut eval), 20);
    // GNU face-remap-add-relative emits this shape. The leftmost entry wins;
    // the trailing self-reference supplies the unremapped base attributes.
    eval.eval_str("(setq face-remapping-alist '((default (:height 1.2) default)))")
        .unwrap();
    assert_eq!(default_font_size(&mut eval), 24);
    eval.eval_str("(setq face-remapping-alist nil)").unwrap();
    assert_eq!(default_font_size(&mut eval), 20);
}

#[test]
fn face_font_remapping_preserves_gnu_relative_and_absolute_precedence() {
    let mut eval = context();
    // GNU xfaces.c merges the tail before the head. Relative heights compose,
    // whereas an absolute height replaces the lower-priority contribution.
    for (specification, expected) in [
        ("((default (:height 1.2) (:height 1.5) default))", 36),
        ("((default (:height 150) (:height 2.0) default))", 20),
        ("((default (:height 2.0) (:height 150) default))", 40),
    ] {
        eval.eval_str(&format!("(setq face-remapping-alist '{specification})"))
            .unwrap();
        assert_eq!(default_font_size(&mut eval), expected, "{specification}");
    }
}
