use super::*;
use crate::emacs_core::display_host::{
    FrameFontRequest, SystemFontName, SystemFontRole, SystemFonts,
};
use crate::emacs_core::eval::ResolvedFrameFont;

/// Substitute only the native desktop/font system. Lisp face updates, name
/// parsing and default-face queries go through the real public interface.
struct DesktopFontHost(SystemFonts);

impl DisplayHost for DesktopFontHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn system_font(&self, role: SystemFontRole) -> Option<&SystemFontName> {
        self.0.get(role)
    }

    fn resolve_frame_font(
        &mut self,
        _: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        let family = request
            .face()
            .family_runtime_string_owned()
            .unwrap_or("Fallback Mono".into());
        let pixels = match request.size() {
            FrameFontSize::Points(points) => (points.get() * 96.0 / 72.27).round() as u32,
            FrameFontSize::Pixels(pixels) => pixels.get(),
            FrameFontSize::Default => 13,
            FrameFontSize::Relative(scale) => (13.0 * scale.get()).round() as u32,
        };
        Ok(Some(ResolvedFrameFont {
            height_tenths: (f64::from(pixels) * 720.0 / 96.0).round() as i32,
            font: crate::emacs_core::eval::test_resolved_opened_font(
                &family,
                None,
                None,
                request.face().weight.unwrap_or(FontWeight::NORMAL),
                request.face().slant.unwrap_or(FontSlant::Normal),
                request.face().width.unwrap_or(FontWidth::Normal),
                None,
                FontPxProbeResult {
                    pixel_size: pixels,
                    height: pixels as i32 + 1,
                    ascent: pixels as i32,
                    descent: 1,
                    max_width: pixels as i32 / 2,
                    space_width: pixels as i32 / 2,
                    average_width: pixels as i32 / 2,
                },
                None,
            ),
        }))
    }
}

fn desktop_context() -> Context {
    let mut eval = Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(DesktopFontHost(SystemFonts::new(
        SystemFontName::new("Ubuntu Mono 13".into()),
        SystemFontName::new("Ubuntu 10".into()),
    ))));
    eval
}

#[test]
fn system_font_queries_distinguish_monospace_from_application_font() {
    let mut eval = desktop_context();
    assert_eq!(
        eval.eval_str("(font-get-system-font)")
            .unwrap()
            .as_utf8_str(),
        Some("Ubuntu Mono 13")
    );
    assert_eq!(
        eval.eval_str("(font-get-system-normal-font)")
            .unwrap()
            .as_utf8_str(),
        Some("Ubuntu 10")
    );
    // Dynamic opt-in does not control visibility of the current preference.
    eval.eval_str("(setq font-use-system-font nil)").unwrap();
    assert_eq!(
        eval.eval_str("(font-get-system-font)")
            .unwrap()
            .as_utf8_str(),
        Some("Ubuntu Mono 13")
    );
}

#[test]
fn system_font_queries_without_a_native_display_remain_nil() {
    let mut eval = Context::new();
    assert!(eval.eval_str("(font-get-system-font)").unwrap().is_nil());
    assert!(
        eval.eval_str("(font-get-system-normal-font)")
            .unwrap()
            .is_nil()
    );
}

#[test]
fn desktop_style_font_name_applies_family_style_and_point_size_to_default_face() {
    let mut eval = desktop_context();
    eval.eval_str("(internal-set-lisp-face-attribute 'default :font \"Ubuntu Mono Bold 13\" nil)")
        .unwrap();
    let font = eval
        .eval_str("(frame-parameter nil 'font-parameter)")
        .unwrap();
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
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
            .as_symbol_name(),
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
