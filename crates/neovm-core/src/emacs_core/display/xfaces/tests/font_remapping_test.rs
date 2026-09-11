use crate::emacs_core::display_host::FontResolveRequest;
use crate::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFontMatch,
};
use crate::emacs_core::value::Value;
use crate::face::{FaceHeight, FontSlant, FontWeight, FontWidth};

/// Deterministic font-system adapter: point sizes at 96 DPI, no font cache.
/// Face definitions, remapping, and Lisp queries all use the real evaluator.
struct PointSizeFontHost;

impl DisplayHost for PointSizeFontHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        let height = match request.faces.ascii_face.height {
            Some(FaceHeight::Absolute(height)) => f64::from(height),
            Some(FaceHeight::Relative(scale)) => 150.0 * scale,
            None => 150.0,
        };
        let pixel_size = (height * 96.0 / 720.0).round() as i32;
        Ok(Some(ResolvedFontMatch {
            font: crate::emacs_core::eval::test_resolved_opened_font(
                "Test Mono",
                None,
                None,
                FontWeight::NORMAL,
                FontSlant::Normal,
                FontWidth::Normal,
                Some("TestMono"),
                FontPxProbeResult {
                    pixel_size: pixel_size.try_into().unwrap(),
                    height: pixel_size + 4,
                    ascent: pixel_size,
                    descent: 4,
                    max_width: pixel_size / 2,
                    space_width: pixel_size / 2,
                    average_width: pixel_size / 2,
                },
                None,
            ),
            glyph_code: Some(request.character.code()),
        }))
    }
}

fn context() -> Context {
    let mut eval = Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    eval.eval_str("(internal-set-lisp-face-attribute 'default :height 150 nil)")
        .unwrap();
    eval.set_display_host(Box::new(PointSizeFontHost));
    eval
}

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

#[test]
fn font_at_named_face_remapping_uses_the_same_precedence_as_face_font() {
    let mut eval = context();
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
