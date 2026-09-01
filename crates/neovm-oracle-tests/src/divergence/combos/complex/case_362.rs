//! Complex combo batch 362 — `outline`/`reveal-mode`/`hi-lock` ultimate:
//! outline hide/show with custom regexp, reveal-mode auto-unhide,
//! hi-lock interactive highlighting with highlight-regexp.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx362_outline_mode_hide_show_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t \"* Top\\n** Sub1\\n** Sub2\\n*** SubSub1\\n* Second\\nbody\\n\" \"* Top\\n** Sub1\\n** Sub2\\n*** SubSub1\\n* Second\\nbody\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (with-temp-buffer
        (outline-mode)
        (insert "* Top\n** Sub1\n** Sub2\n*** SubSub1\n* Second\nbody\n")
        (goto-char 1)
        (outline-hide-subtree)
        (let ((after-hide (buffer-string)))
          (outline-show-subtree)
          (let ((after-show (buffer-string)))
            (list (eq major-mode 'outline-mode)
                  after-hide after-show)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_outline_hide_body_show_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Top\\nhidden body 1\\n* Second\\nhidden body 2\\n\" \"* Top\\nhidden body 1\\n* Second\\nhidden body 2\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (outline-mode)
      (insert "* Top\nhidden body 1\n* Second\nhidden body 2\n")
      (goto-char 1)
      (outline-hide-body)
      (let ((visible (buffer-string)))
        (outline-show-all)
        (list visible (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_outline_minor_mode_custom_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (with-temp-buffer
        (insert "SECTION Alpha\nbody\nSUB Beta\nbody\nSECTION Gamma\nbody\n")
        (outline-minor-mode 1)
        (setq-local outline-regexp "^SECTION\\|SUB")
        (goto-char 1)
        (list (eq minor-mode 'outline-minor-mode)
              (outline-on-heading-p)
              (forward-line 1) (forward-line 1)
              (outline-on-heading-p))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_outline_promote_demote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (outline-mode)
      (insert "* Heading\n** Sub\n*** SubSub\nbody\n")
      (goto-char 1)
      (outline-demote)
      (let ((after-demote (buffer-string)))
        (goto-char 1)
        (outline-promote)
        (let ((after-promote (buffer-string)))
          (list after-demote after-promote))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_outline_next_prev_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (14 28 14 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (outline-mode)
      (insert "* First\nbody\n* Second\nbody\n* Third\nbody\n")
      (goto-char 1)
      (outline-next-heading)
      (let ((h2 (point)))
        (outline-next-heading)
        (let ((h3 (point)))
          (outline-previous-heading)
          (list h2 h3 (point) (line-beginning-position)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_reveal_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'reveal)
      (list (fboundp 'reveal-mode)
            (boundp 'reveal-auto-hide)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_hi_lock_highlight_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'hi-lock)
      (list (fboundp 'highlight-regexp)
            (fboundp 'highlight-phrase)
            (fboundp 'highlight-lines-matching-regexp)
            (fboundp 'unhighlight-regexp)
            (boundp 'hi-lock-file-patterns)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_outline_level_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (with-temp-buffer
        (outline-mode)
        (insert "* L1\n** L2\n*** L3\n* L1-again\n")
        (goto-char 1)
        (list (outline-current-level)
              (forward-line 1) (outline-current-level)
              (forward-line 1) (outline-current-level)
              (forward-line 1) (outline-current-level))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_allout_foldout_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'allout)
          (fboundp 'allout-mode)
          (featurep 'foldout)
          (fboundp 'foldout-zoom-subtree))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx362_outline_reveal_hilock_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (require 'reveal)
      (require 'hi-lock)
      (with-temp-buffer
        (buffer-enable-undo)
        (outline-mode)
        (insert "* Heading one\nbody content\n* Heading two\nmore content\n")
        (put-text-property 1 9 'face 'bold)
        (let ((m (set-marker (make-marker) 15))
              (ov (make-overlay 5 22)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 35)
          (goto-char 1)
          (let ((state (list (eq major-mode 'outline-mode)
                             (fboundp 'highlight-regexp)
                             (fboundp 'reveal-mode)
                             (outline-on-heading-p)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen()
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
