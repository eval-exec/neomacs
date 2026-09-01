//! Complex combo batch 74 — narrowing / region / mark / excursion / point
//! edge cases: `save-excursion` interaction with text changes, `pop-to-mark`,
//! `exchange-point-and-mark`, transposing across narrowing boundaries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx74_save_excursion_with_text_changes_does_not_restore_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0123AAA456789\" 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx74-se*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "0123456789"))
  (with-current-buffer buf
    (save-excursion
      (goto-char 5)
      (insert "AAA")))
  (prog1 (list (with-current-buffer buf (buffer-string))
               (with-current-buffer buf (point)))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx74_save_restriction_with_text_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 18 \"X0123456789ABCDEF\") 6 11 \"45678\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (narrow-to-region 5 10)
  (let ((before (list (point-min) (point-max) (buffer-string))))
    (save-restriction
      (widen)
      (goto-char 1)
      (insert "X")
      (let ((inside (list (point-min) (point-max) (buffer-string))))
        (setq before inside)))
    (list before (point-min) (point-max) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_exchange_point_and_mark_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 3 3 7 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (push-mark 3)
  (goto-char 7)
  (let ((p-before (point))
        (m-before (mark)))
    (exchange-point-and-mark)
    (let ((p-after (point))
          (m-after (mark)))
      (list p-before m-before p-after m-after
            (= p-before m-after)
            (= m-before p-after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_save_excursion_after_kill_buffer_does_not_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((other (get-buffer-create " *neo-cx74-other*")))
  (with-current-buffer other (insert "x"))
  (let ((origin-buffer (current-buffer)))
    (save-excursion
      (set-buffer other)
      (goto-char 1)
      (kill-buffer other))
    (list (eq (current-buffer) origin-buffer)
          (buffer-live-p other))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_transpose_chars_words_lines_at_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"acbdef\" \"world hello\" \"line1\\nline2\\n\\nline3\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "abcdef")
   (goto-char 3)
   (transpose-chars 1)
   (buffer-string))
 (with-temp-buffer
   (insert "hello world")
   (goto-char 6)
   (transpose-words 1)
   (buffer-string))
 (with-temp-buffer
   (insert "line1\nline2\nline3\n")
   (forward-line 1)
   (transpose-lines 1)
   (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx74_narrow_widen_marker_position_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((5 20 15) (5 18 13) 1 25 13)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF0123456789")
  (let ((m (set-marker (make-marker) 15)))
    (narrow-to-region 5 20)
    (let ((inside (list (point-min) (point-max) (marker-position m))))
      (delete-region 10 12)
      (let ((after-del (list (point-min) (point-max) (marker-position m))))
        (widen)
        (list inside after-del (point-min) (point-max) (marker-position m))))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_set_marker_with_different_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx74-a*"))
      (buf-b (get-buffer-create " *neo-cx74-b*")))
  (with-current-buffer buf-a (insert "AAAA"))
  (with-current-buffer buf-b (insert "BBBB"))
  (let ((m (set-marker (make-marker) 2 buf-a)))
    (let ((pos-in-a (marker-position m))
          (buf-of-m (marker-buffer m)))
      (set-marker m 3 buf-b)
      (let ((pos-in-b (marker-position m))
            (buf-of-m-2 (marker-buffer m)))
        (kill-buffer buf-a)
        (kill-buffer buf-b)
        (list pos-in-a (eq buf-of-m buf-a)
              pos-in-b (eq buf-of-m-2 buf-b)
              (marker-buffer m)))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_save_excursion_persists_point_across_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (goto-char 5)
  (let ((p-before (point)))
    (save-excursion
      (goto-char 1)
      (insert "X")
      (undo))
    (list p-before (point) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_region_active_p_and_use_region_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 2 7 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (push-mark 2)
  (goto-char 7)
  (let ((mark-active-state (region-active-p))
        (use-region-state (use-region-p))
        (reg-beg (region-beginning))
        (reg-end (region-end)))
    (deactivate-mark)
    (list mark-active-state use-region-state reg-beg reg-end
          (region-active-p) (use-region-p))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_buffer_narrowing_with_text_props_undo_marker_overlay_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx74-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "0123456789ABCDEF0123456789ABCDEF")
    (put-text-property 1 5 'face 'bold)
    (put-text-property 8 12 'display "XX")
    (let ((m (set-marker (make-marker) 18))
          (ov (make-overlay 5 25)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (save-restriction
        (narrow-to-region 5 28)
        (let ((state-1 (list (point-min) (point-max)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (buffer-string)
                             (text-properties-at 1))))
          (delete-region 10 15)
          (let ((state-2 (list (point-min) (point-max)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (buffer-string))))
            (widen)
            (undo)
            (prog1 (list state-1 state-2
                         (point-min) (point-max)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (buffer-string))
              (kill-buffer buf)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx74_field_property_constraints_at_motion_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 1 t t 21 #(\"field-tw\" 0 8 (field b)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "field-one | field-two | field-three")
  (put-text-property 1 9 'field 'a)
  (put-text-property 13 21 'field 'b)
  (put-text-property 25 35 'field 'c)
  (goto-char 5)
  (let ((forward-end (condition-case e (field-end (point)) (error :err)))
        (backward-end (condition-case e (field-beginning (point)) (error :err)))
        (in-field-a (eq (get-text-property (point) 'field) 'a)))
    (goto-char 15)
    (let ((in-field-b (eq (get-text-property (point) 'field) 'b))
          (forward-end-b (field-end (point))))
      (list forward-end backward-end in-field-a
            in-field-b forward-end-b
            (field-string 15)))))
"##,
        expect,
    );
}
