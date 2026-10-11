//! Public Lisp glyph coverage across the platform display-host boundary.
use neovm_core::emacs_core::{
    display_host::{FontCoverage, FontCoverageRequest, FontCoverageTarget, FontEntityHandle},
    eval::{DisplayHost, FontSpecResolveRequest, GuiFrameHostRequest, ResolvedFontSpecMatch},
    load::create_bootstrap_evaluator_cached,
};
use neovm_core::heap_types::LispString;

struct CoverageHost;
impl DisplayHost for CoverageHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_font_for_spec(
        &mut self,
        _request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        Ok(Some(ResolvedFontSpecMatch {
            coverage_handle: FontEntityHandle::new(1),
            family: LispString::from_utf8("Coverage Fixture"),
            foundry: None,
            registry: None,
            file: None,
            weight: None,
            slant: None,
            width: None,
            spacing: None,
            postscript_name: None,
        }))
    }

    fn font_character_coverage(
        &mut self,
        request: FontCoverageRequest,
    ) -> Result<FontCoverage, String> {
        // This is a platform boundary fixture, not a replacement for font
        // parsing. Actual native coverage is exercised by the GUI harness.
        Ok(match request.target {
            FontCoverageTarget::Entity(handle) if handle.get() == 1 => {
                match request.character.code() {
                    65 | 0x2588 => FontCoverage::Present,
                    67 => FontCoverage::NeedsOpening,
                    _ => FontCoverage::Absent,
                }
            }
            _ => FontCoverage::NeedsOpening,
        })
    }
}

#[test]
fn lisp_reports_known_coverage_and_deferred_entity_checks() {
    let mut eval = create_bootstrap_evaluator_cached().unwrap();
    eval.set_display_host(Box::new(CoverageHost));
    let result = eval
        .eval_str(
            r#"(let ((font (find-font (font-spec :family "Coverage Fixture"))))
        (list (font-has-char-p font ?A)
              (font-has-char-p font ?█ (selected-frame))
              (font-has-char-p font ?B)
              (font-has-char-p font ?C)))"#,
        )
        .unwrap();
    assert_eq!(format!("{result}"), "(t t nil nil)");
}

#[test]
fn font_coverage_validates_full_emacs_character_and_frame_domains() {
    let mut eval = create_bootstrap_evaluator_cached().unwrap();
    eval.set_display_host(Box::new(CoverageHost));
    let result = eval
        .eval_str(
            r#"(let ((font (find-font (font-spec))))
      (list (mapcar (lambda (ch) (font-has-char-p font ch)) '(#xd800 #x110000 #x3fffff))
            (mapcar (lambda (ch) (condition-case err (font-has-char-p font ch)
                                  (wrong-type-argument (cdr err))))
                    '(-1 #x400000 4294967361 65.0 nil))
            (condition-case err (font-has-char-p font ?A 1)
              (wrong-type-argument (cdr err)))))"#,
        )
        .unwrap();
    assert_eq!(
        format!("{result}"),
        "((nil nil nil) ((characterp -1) (characterp 4194304) (characterp 4294967361) (characterp 65.0) (characterp nil)) (framep 1))"
    );
}
