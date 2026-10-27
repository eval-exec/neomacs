//! Expectations captured from GNU Emacs 31.1 through sandbox-run.sh.

/// GNU `internal-set-lisp-face-attribute` runs the QCfont value check on the
/// new-frame-defaults pass (FRAME t, and the first half of FRAME 0) even when
/// the selected frame is a terminal: `nil` and other non-font values signal
/// `Invalid font or font-spec`, a font-name string and `unspecified` pass.
/// See src/xfaces.c:3635-3664.
#[test]
fn tsb_face_font_attribute_rejects_non_font_values() {
    let form = r#"(progn
      (make-face 'tsb-font-nil)
      (mapcar (lambda (v)
                (condition-case e
                    (progn (set-face-attribute 'tsb-font-nil nil :font v) 'ok)
                  (error e)))
              (list nil 42 "Monospace-12" 'unspecified)))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-face-font-validation.expect").trim_end()
    );
}
