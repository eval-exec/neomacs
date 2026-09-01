//! Complex combo batch 60 — text-property / overlay interval engine edges.
//!
//! Targets likely divergence surface around interval splits & merges,
//! char-property search (overlay vs text-property priority), font-lock
//! face append/prepend/keep merging, sticky front/rear insertion types,
//! and `with-silent-modifications` / `combine-change-calls` hook cadence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx60_char_property_search_overlay_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx60-cps*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "0123456789")
    (put-text-property 2 6 'face 'bold)
    (let ((ov (make-overlay 4 8)))
      (overlay-put ov 'face 'italic)
      (let* ((a (get-char-property 2 'face))
             (b (get-char-property 4 'face))
             (c (get-char-property 6 'face))
             (d (get-char-property 8 'face))
             (e (next-single-char-property-change 1 'face))
             (f (next-single-char-property-change 4 'face)))
        (prog1 (list a b c d e f)
          (delete-overlay ov)))))
  (kill-buffer buf))
"##,
        expect,
    );
}

#[test]
fn div_cx60_text_property_not_all_and_any_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 10 20 1 nil 6 10 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (put-text-property 1 6 'p 'one)
  (put-text-property 10 15 'p 'two)
  (put-text-property 20 26 'p 'three)
  (list (text-property-any 1 27 'p 'one)
        (text-property-any 7 27 'p 'two)
        (text-property-any 17 27 'p 'three)
        (text-property-not-all 1 27 'p nil)
        (text-property-not-all 7 9 'p nil)
        (next-single-property-change 1 'p)
        (next-single-property-change 6 'p)
        (previous-single-property-change 26 'p)))
"##,
        expect,
    );
}

#[test]
fn div_cx60_font_lock_append_prepend_keep_face_merger() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument buffer-or-string-p t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "The quick brown fox")
  (put-text-property 1 19 'face 'bold)
  (put-text-property 5 10 'face '(italic))
  (put-text-property 4 16 'face '(:foreground "red") t)   ; append
  (put-text-property 4 16 'face '(:height 2.0) t)
  (list (get-text-property 1 'face)
        (get-text-property 5 'face)
        (get-text-property 7 'face)
        (get-text-property 15 'face)
        (get-text-property 17 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx60_sticky_front_rear_insertion_split_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abcdXefYghij\" 2 4 (p core rear-nonsticky t front-sticky nil) 5 7 (p core rear-nonsticky t front-sticky nil)) core nil core nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 3 7 'front-sticky nil)
  (put-text-property 3 7 'rear-nonsticky t)
  (put-text-property 3 7 'p 'core)
  (goto-char 5)
  (insert "X")
  (goto-char 8)
  (insert "Y")
  (list (buffer-string)
        (get-text-property 3 'p)
        (get-text-property 5 'p)
        (get-text-property 6 'p)
        (get-text-property 8 'p)
        (get-text-property 9 'p)))
"##,
        expect,
    );
}

#[test]
fn div_cx60_remove_add_text_props_interval_merge_after_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (#(\"0126789\" 0 3 (q b p a) 3 6 (q d p c)) 7 a b a b 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 4 '(p a q b))
  (add-text-properties 7 10 '(p c q d))
  (delete-region 4 7)
  (list (buffer-string)
        (length (buffer-string))
        (get-text-property 1 'p)
        (get-text-property 1 'q)
        (get-text-property 3 'p)
        (get-text-property 3 'q)
        (next-property-change 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx60_with_silent_modifications_undo_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 (3) \"beforeMUTEDafter\" (:change))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (hooks undo-records)
  (setq hooks nil)
  (with-temp-buffer
    (buffer-enable-undo)
    (add-hook 'after-change-functions
              (lambda (&rest _) (push :change hooks)) nil t)
    (insert "before")
    (setq hooks nil)
    (with-silent-modifications
      (insert "MUTED"))
    (let ((silent-count (length hooks)))
      (insert "after")
      (let ((loud-count (length hooks)))
        (setq undo-records (list (length (if (boundp 'buffer-undo-list) buffer-undo-list 'none))))
        (list silent-count loud-count undo-records
              (buffer-string) hooks)))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_combine_change_calls_hook_cadence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((3 8 4)) \"ABXYYEFGHIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (with-temp-buffer
    (buffer-enable-undo)
    (add-hook 'after-change-functions
              (lambda (beg end len) (push (list beg end len) calls)) nil t)
    (insert "ABCDEFGHIJ")
    (setq calls nil)
    (combine-change-calls 3 7
      (goto-char 3)
      (insert "X")
      (delete-region 4 6)
      (insert "YY"))
    (list (nreverse calls) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_overlay_priority_overlap_face_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (bold italic italic (#<overlay in no buffer> #<overlay in no buffer>) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((lo (make-overlay 2 8))
        (hi (make-overlay 4 12)))
    (overlay-put lo 'priority 0)
    (overlay-put hi 'priority 10)
    (overlay-put lo 'face 'bold)
    (overlay-put hi 'face 'italic)
    (overlay-put lo 'window (selected-window))
    (let ((a (get-char-property 3 'face))
          (b (get-char-property 5 'face))
          (c (get-char-property 9 'face)))
      (prog1 (list a b c
                   (overlays-at 5)
                   (length (overlays-in 1 16)))
        (delete-overlay lo)
        (delete-overlay hi)))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_char_property_overlay_rear_advance_on_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 6 3 7 \"ZabQRcdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 4 nil t nil)))   ; front-advance=t, rear-advance=nil
    (overlay-put ov 'face 'region)
    (goto-char 1)
    (insert "Z")            ; before buffer → front-advance pushes start
    (goto-char 4)
    (insert "Q")            ; at overlay start, rear-advance nil → start stays
    (let ((s1 (overlay-start ov))
          (e1 (overlay-end ov)))
      (goto-char (1- e1))
      (insert "R")          ; at overlay end, rear-advance nil → end grows
      (list s1 e1 (overlay-start ov) (overlay-end ov) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_next_property_change_across_text_only_with_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8 10 13 15 27)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (put-text-property 3 8 'face 'bold)
  (put-text-property 15 20 'face 'bold)
  (let ((ov (make-overlay 10 13)))
    (overlay-put ov 'face 'bold)
    (list (next-single-char-property-change 1 'face)
          (next-single-char-property-change 3 'face)
          (next-single-char-property-change 8 'face)
          (next-single-char-property-change 10 'face)
          (next-single-char-property-change 13 'face)
          (next-single-char-property-change 20 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_set_text_properties_empty_value_vs_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold nil nil italic italic 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 10 'face 'italic)
  (set-text-properties 3 7 nil)
  (list (get-text-property 1 'face)
        (get-text-property 3 'face)
        (get-text-property 5 'face)
        (get-text-property 7 'face)
        (get-text-property 8 'face)
        (next-single-property-change 1 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx60_property_search_forward_backward_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil 13 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha beta gamma delta epsilon")
      (put-text-property 1 5 'cat 'greek)
      (put-text-property 7 10 'cat 'greek)
      (put-text-property 13 17 'cat 'greek)
      (let* ((fwd (text-property-search-forward 'cat 'greek t))
             (beg (if fwd (prop-match-beginning fwd)))
             (end (if fwd (prop-match-end fwd)))
             (val (if fwd (prop-match-value fwd))))
        (goto-char (point-max))
        (let ((bwd (text-property-search-backward 'cat 'greek t)))
          (list beg end val
                (if bwd (prop-match-beginning bwd))
                (if bwd (prop-match-end bwd))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_text_property_overlay_undo_replace_marker_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "The quick brown fox jumps over the lazy dog")
  (put-text-property 1 9 'face 'bold)
  (put-text-property 11 19 'face 'italic)
  (let ((m (set-marker (make-marker) 11))
        (ov (make-overlay 5 15)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 4 35)
    (undo-boundary)
    (goto-char 8)
    (insert "VERY ")
    (delete-region 20 25)
    (replace-string "brown" "RED" nil 5 30)
    (let ((state (list (buffer-string) (marker-position m)
                       (overlayp ov) (overlay-start ov) (overlay-end ov)
                       (get-text-property 1 'face)
                       (get-text-property 7 'face)
                       (point-min) (point-max))))
      (undo) (undo) (undo)
      (widen)
      (list state
            (buffer-string)
            (marker-position m)
            (overlayp ov) (overlay-start ov) (overlay-end ov)
            (get-text-property 1 'face)
            (point-min) (point-max)))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_buffer_text_properties_via_buffer_substring_less_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"bcdefghijklmno\" 1 6 (face bold) 8 13 (face italic)) \"bcdefghijklmno\" 14 14 nil bold nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "abcdefghijklmnopqrstuvwxyz")
      (put-text-property 3 8 'face 'bold)
      (put-text-property 10 15 'face 'italic)
      (list (buffer-substring 2 16)
            (buffer-substring-no-properties 2 16)
            (length (buffer-substring 2 16))
            (length (buffer-substring-no-properties 2 16))
            (get-text-property 2 'face)
            (get-text-property 4 'face)
            (get-text-property 9 'face)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx60_field_property_and_constrain_to_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (20 10 10 1 10 #(\"field-tw\" 0 8 (field b)) a b c)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "field-one\tfield-two\tfield-three")
  (put-text-property 1 10 'field 'a)
  (put-text-property 11 19 'field 'b)
  (put-text-property 20 31 'field 'c)
  (list (constrain-to-field 5 31)
        (constrain-to-field 15 1)
        (constrain-to-field 25 5)
        (field-beginning 5)
        (field-end 5)
        (field-string 15)
        (get-text-property 1 'field)
        (get-text-property 11 'field)
        (get-text-property 20 'field)))
"##,
        expect,
    );
}
