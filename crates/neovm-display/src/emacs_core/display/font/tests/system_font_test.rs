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
    // GNU defines font-get-system-font and font-get-system-normal-font only
    // in xsettings.c (X11, configure.ac:7227; PGTK, configure.ac:3911) and
    // haikufont.c (Haiku, haikufont.c:1369); an NS (macOS) or w32 build has
    // neither object.  neovm-display registers both under the same platform
    // gate, so this query surface exists only where GNU has it.
    #[cfg(not(any(windows, target_os = "macos")))]
    {
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
    // Where GNU's NS/w32 builds have no such subr, neovm must not register
    // one either: both names are unbound, and GNU's own
    // dynamic-setting.el:46 guards its call with fboundp.
    #[cfg(any(windows, target_os = "macos"))]
    {
        assert!(
            eval.eval_str("(fboundp 'font-get-system-font)")
                .unwrap()
                .is_nil()
        );
        assert!(
            eval.eval_str("(fboundp 'font-get-system-normal-font)")
                .unwrap()
                .is_nil()
        );
    }
}

#[test]
fn system_font_queries_without_a_native_display_remain_nil() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        assert!(eval.eval_str("(font-get-system-font)").unwrap().is_nil());
        assert!(
            eval.eval_str("(font-get-system-normal-font)")
                .unwrap()
                .is_nil()
        );
    }
    // On NS/w32 GNU has no such subrs at all (xsettings.c is X11/PGTK only,
    // haikufont.c is Haiku only), so the accurate assertion is that neither
    // name is bound rather than that the query answers nil.
    #[cfg(any(windows, target_os = "macos"))]
    {
        assert!(
            eval.eval_str("(fboundp 'font-get-system-font)")
                .unwrap()
                .is_nil()
        );
        assert!(
            eval.eval_str("(fboundp 'font-get-system-normal-font)")
                .unwrap()
                .is_nil()
        );
    }
}
