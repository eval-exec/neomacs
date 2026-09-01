//! Complex combo batch 134 — `outline` / `foldout` / `allout` / `hideif` /
//! `page` / `newsticker` / `eww` rendering hooks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx134_outline_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'outline)
      (list (fboundp 'outline-mode)
            (boundp 'outline-regexp)
            (boundp 'outline-level)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_outline_basic_hide_reveal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Top\\n** Sub1\\n** Sub2\\n*** SubSub1\\n* Second\\nbody\\n\" \"* Top\\n** Sub1\\n** Sub2\\n*** SubSub1\\n* Second\\nbody\\n\" ((outline . t) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (outline-mode)
      (insert "* Top\n** Sub1\n** Sub2\n*** SubSub1\n* Second\nbody\n")
      (goto-char 1)
      (outline-hide-subtree)
      (let ((after-hide (buffer-string))
            (invisibility-spec buffer-invisibility-spec))
        (outline-show-subtree)
        (let ((after-show (buffer-string)))
          (list after-hide after-show invisibility-spec))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_outline_visible_only_substring() {
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
    );
}

#[test]
fn div_cx134_allout_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'allout)
      (list (fboundp 'allout-mode)
            (boundp 'allout-command-key)
            (boundp 'allout-auto-activation)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_foldout_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'foldout)
      (list (fboundp 'foldout-zoom-subtree)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_hideif_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'hideif)
      (list (fboundp 'hide-ifdef-mode)
            (boundp 'hide-ifdef-env)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_page_delimiter_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"^\\f\" 17 33 17 112)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "page 1 content\n\x0cpage 2 content\n\x0cpage 3 content")
  (let ((delim page-delimiter))
    (goto-char 1)
    (forward-page 1)
    (let ((after-page-1 (point)))
      (forward-page 1)
      (let ((after-page-2 (point)))
        (backward-page 1)
        (list delim after-page-1 after-page-2 (point) (char-after))))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_outline_promote_demote() {
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
    );
}

#[test]
fn div_cx134_newsticker_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'newsticker)
      (list (fboundp 'newsticker-start)
            (boundp 'newsticker-url-list)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_outline_next_prev_heading() {
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
    );
}

#[test]
fn div_cx134_eww_render_hook_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eww)
      (list (boundp 'eww-header-format-alist)
            (boundp 'eww-after-render-hook)
            (boundp 'eww-history-limit)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx134_outline_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored outline-before-first-heading)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (outline-mode)
      (insert "* Heading 1\nbody 1\n** Sub 1\nsub body\n* Heading 2\nbody 2\n")
      (put-text-property 1 9 'face 'bold)
      (let ((m (set-marker (make-marker) 12))
            (ov (make-overlay 4 22)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 35)
        (goto-char 1)
        (outline-hide-subtree)
        (let ((state (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1)
                           buffer-invisibility-spec)))
          (undo)
          (widen)
          (outline-show-all)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
