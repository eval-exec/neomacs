//! Issue #558: interactive motion uses GNU's shift-selection policy.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn repeated_shifted_motion_extends_from_the_original_anchor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (goto-char (point-min))
  (setq shift-select-mode t)
  (transient-mark-mode 1)
  (let ((this-command-keys-shift-translated t)
        (observations nil))
    (dotimes (_ 4)
      (call-interactively #'forward-char)
      (push (list (point) (mark)
                  (buffer-substring-no-properties (region-beginning) (region-end)))
            observations))
    (nreverse observations)))
"#,
        expect_test::expect![[r#""OK ((2 1 \"a\") (3 1 \"ab\") (4 1 \"abc\") (5 1 \"abcd\"))""#]],
    );
}

#[test]
fn reversing_and_unshifted_motion_respect_the_selection_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(mapcar
  (lambda (mode)
    (with-temp-buffer
      (insert "abcdefgh")
      (goto-char 3)
      (setq shift-select-mode mode)
      (transient-mark-mode 1)
      (let ((observations nil))
        (let ((this-command-keys-shift-translated t))
          (call-interactively #'forward-char)
          (call-interactively #'forward-char)
          (push (list (point) (mark t) mark-active transient-mark-mode) observations)
          (call-interactively #'backward-char)
          (call-interactively #'backward-char)
          (call-interactively #'backward-char)
          (push (list (point) (mark t) mark-active transient-mark-mode) observations))
        (let ((this-command-keys-shift-translated nil))
          (call-interactively #'forward-char)
          (push (list (point) (mark t) mark-active transient-mark-mode
                      (local-variable-p 'transient-mark-mode)) observations))
        (nreverse observations))))
  '(t permanent nil))"#,
        expect_test::expect![[
            r#""OK (((5 3 t (only . t)) (2 3 t (only . t)) (3 3 nil t nil)) ((5 3 t t) (2 3 t t) (3 3 t t nil)) ((5 nil nil t) (2 nil nil t) (3 nil nil t nil)))""#
        ]],
    );
}

#[test]
fn interpreted_and_compiled_callers_preserve_mark_ring_and_hook_delivery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(mapcar
  (lambda (compiled)
    (with-temp-buffer
      (insert "abcdefgh")
      (goto-char 3)
      (set-mark 8)
      (setq mark-active nil shift-select-mode t)
      (transient-mark-mode 0)
      (let* ((activations 0) (deactivations 0)
             (activate-mark-hook (list (lambda () (setq activations (1+ activations)))))
             (deactivate-mark-hook (list (lambda () (setq deactivations (1+ deactivations))))))
        (let ((invoke (if compiled
                          (byte-compile '(lambda () (call-interactively #'forward-char)))
                        (lambda () (call-interactively #'forward-char))))
              (this-command-keys-shift-translated t))
          (funcall invoke)
          (funcall invoke)
          (let ((this-command-keys-shift-translated nil)) (funcall invoke)))
        (list (point) (mark) mark-active transient-mark-mode
              (local-variable-p 'transient-mark-mode)
              (mapcar #'marker-position mark-ring) activations deactivations))))
  '(nil t))"#,
        expect_test::expect![[r#""OK ((6 3 t nil nil (8) 1 0) (6 3 t nil nil (8) 1 0))""#]],
    );
}
