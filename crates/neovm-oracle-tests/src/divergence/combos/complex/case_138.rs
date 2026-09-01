//! Complex combo batch 138 — `ediff` / `smerge-mode` / `diff-mode` /
//! `patience` / `merge` parsing and conflict resolution.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx138_ediff_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ediff)
      (list (fboundp 'ediff)
            (fboundp 'ediff-files)
            (fboundp 'ediff-buffers)
            (boundp 'ediff-window-setup-function)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_smerge_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'smerge-mode)
      (list (fboundp 'smerge-mode)
            (fboundp 'smerge-resolve)
            (fboundp 'smerge-ediff)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_diff_mode_parse_hunks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t diff-header diff-removed 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,4 @@\n context\n-removed\n+added\n+new line\n")
      (diff-mode)
      (font-lock-fontify-buffer)
      (list (eq major-mode 'diff-mode)
            (get-text-property 1 'face)
            (get-text-property 60 'face)
            (next-single-property-change 1 'face)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_smerge_parse_conflict() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "<<<<<<< HEAD\nour change\n=======\ntheir change\n>>>>>>> branch\n")
      (smerge-mode 1)
      (list (eq major-mode 'smerge-mode)
            (get-text-property 1 'face)
            (next-single-property-change 1 'face)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_ediff_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'ediff-other-buffer)
          (fboundp 'ediff-merge-files)
          (fboundp 'ediff-merge-buffers)
          (boundp 'ediff-split-window-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_diff_hunk_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "--- a/foo.txt\n+++ b/foo.txt\n@@ -1,3 +1,4 @@\n unchanged\n-old line\n+new line\n+added line\n")
      (diff-mode)
      (goto-char 1)
      (let ((hunk-beg (condition-case err
                          (diff-hunk-next) (error :err))))
        (list hunk-beg (point))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_smerge_resolve_keep_mine() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"mine\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> branch\n")
      (smerge-mode 1)
      (goto-char 1)
      (condition-case err
          (smerge-keep-mine)
        (error :err))
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_smerge_resolve_keep_other() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"theirs\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> branch\n")
      (smerge-mode 1)
      (goto-char 1)
      (condition-case err
          (smerge-keep-other)
        (error :err))
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_diff_apply_hunk_to_source() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'diff-test-hunk)
          (fboundp 'diff-apply-hunk)
          (boundp 'diff-update-on-the-fly)
          (boundp 'diff-refine-hunk))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_diff_reversed_direction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-old\n+new\n")
      (diff-mode)
      (condition-case err
          (diff-reverse-direction)
        (error :err))
      (buffer-substring 1 60))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_ediff_get_remote_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'ediff-get-region-from-buffer)
          (fboundp 'ediff-prepare-meta-buffer)
          (boundp 'ediff-meta-buffer))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx138_diff_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "--- a/file\n+++ b/file\n@@ -1,3 +1,4 @@\n unchanged\n-old line\n+new line\n+added line\n")
      (diff-mode)
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 10))
            (ov (make-overlay 4 18)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 25)
        (let ((state (list (eq major-mode 'diff-mode)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
