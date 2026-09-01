//! Complex combo batch 92 — clipboard / selection / x-select, ring
//! operations (kill-ring, yank), mark ring traversal, and `kill-whole-line`
//! with edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx92_kill_ring_push_yank_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"gamma\" \"beta\" \"alpha\" nil) \"gamma\" \"gamma\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring nil))
  (push "alpha" kill-ring)
  (push "beta" kill-ring)
  (push "gamma" kill-ring)
  (let ((idx 0)
        (collected nil))
    (dolist (_ (number-sequence 1 4))
      (push (nth idx kill-ring) collected)
      (setq idx (1+ idx)))
    (list (nreverse collected)
          (current-kill 0 t)
          (car kill-ring))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_kill_line_then_yank_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\nline2\\nline3\\n\" \"line1\" \"line1\\nline2\\nline3\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring nil))
  (with-temp-buffer
    (insert "line1\nline2\nline3\n")
    (goto-char 1)
    (kill-line)
    (let ((after-kill (buffer-string))
          (killed (current-kill 0 t)))
      (yank)
      (let ((after-yank (buffer-string)))
        (list after-kill killed after-yank)))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_kill_whole_line_with_multiple_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"line1\\nline3\\n\" \"line2\\n\" 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring nil))
  (with-temp-buffer
    (insert "line1\nline2\nline3\n")
    (goto-char 7)
    (let ((kill-whole-line t))
      (kill-whole-line))
    (list (buffer-string) (current-kill 0 t) (point))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_kill_region_rectangle_yank_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kill-ring nil))
      (with-temp-buffer
        (insert "AAA111\nBBB222\nCCC333\n")
        (push-mark 1)
        (goto-char 7)
        (let ((kill-read-only-ok t))
          (copy-rectangle-as-kill 1 7))
        (list (current-kill 0 t)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_mark_ring_push_pop_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (25 4 25 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "012345678901234567890123456789")
  (let ((mark-ring nil))
    (dotimes (i 5)
      (push-mark (+ 1 (* i 6)))
      (goto-char (+ 3 (* i 6))))
    (let ((current-mark (mark t))
          (ring-length (length mark-ring)))
      (set-mark-command 4)   ; pop mark ring
      (let ((after-pop (point))
            (ring-after (length mark-ring)))
        (list current-mark ring-length after-pop ring-after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_append_next_kill_combines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\" third\" 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((kill-ring nil))
      (with-temp-buffer
        (insert "first second third")
        (goto-char 1)
        (kill-word 1)
        (setq this-command 'kill-region)
        (forward-word 1)
        (kill-word 1))
      (list (current-kill 0 t)
            (length kill-ring)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_yank_with_arg_multiple_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"AINSERTEDAA\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring (list "INSERTED")))
  (with-temp-buffer
    (insert "AAA")
    (goto-char 2)
    (let ((this-command 'yank))
      (yank 3))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx92_kill_ring_max_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"kill-9\" \"kill-8\" \"kill-7\") 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring nil)
      (kill-ring-max 3))
  (dotimes (i 10)
    (push (format "kill-%d" i) kill-ring)
    (when (> (length kill-ring) kill-ring-max)
      (setcdr (nthcdr (1- kill-ring-max) kill-ring) nil)))
  (list kill-ring (length kill-ring)))
"##,
        expect,
    );
}

#[test]
fn div_cx92_kill_append_to_previous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"first second third\") \"first second third\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring (list "first")))
  (kill-append " second" nil)
  (kill-append " third" nil)
  (list kill-ring (current-kill 0 t)))
"##,
        expect,
    );
}

#[test]
fn div_cx92_kill_ring_save_no_modify_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AAA BBB CCC\" \"BBB\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring nil))
  (with-temp-buffer
    (insert "AAA BBB CCC")
    (kill-ring-save 5 8)
    (let ((after (buffer-string))
          (killed (current-kill 0 t))
          (modified (buffer-modified-p)))
      (list after killed modified))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_clipboard_interaction_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'gui-set-selection)
          (fboundp 'gui-get-selection)
          (fboundp 'x-set-selection)
          (eq selection-coding-system selection-coding-system))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx92_yank_rectangle_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Kill ring is empty\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((kill-ring nil))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "AAA111\nBBB222\nCCC333\n")
    (put-text-property 1 4 'face 'bold)
    (let ((m (set-marker (make-marker) 5))
          (ov (make-overlay 2 8)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 1 22)
      (copy-rectangle-as-kill 1 7)
      (delete-rectangle 1 7)
      (let ((state (list (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1)
                         (current-kill 0 t))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
