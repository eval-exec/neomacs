use super::*;
use crate::emacs_core::display_host::{
    FrameFontRequest, SystemFontName, SystemFontRole, SystemFonts,
};
use neovm_eval::emacs_core::eval::ResolvedFrameFont;

fn desktop_context() -> Context {
    crate::test_utils::system_font_test_upper_layer_helpers::desktop_context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    })
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    assert!(eval.eval_str("(font-get-system-font)").unwrap().is_nil());
    assert!(
        eval.eval_str("(font-get-system-normal-font)")
            .unwrap()
            .is_nil()
    );
}
