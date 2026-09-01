//! Divergence tests: narrow + marker + overlay + text-property + regex + undo mega.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_narrow_undo_replace_with_nested_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"QQBBB-ZZZZZZ-DDD-YYYYYY-FFF-GGG-HHH\" 2 4 (group g2) 13 15 (group g4) 24 26 (group g6) 28 30 (group g7) 32 34 (group g8)) #(\"AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ\" 0 2 (group g1) 4 6 (group g2) 8 10 (group g3) 12 14 (group g4) 16 18 (group g5) 20 22 (group g6) 24 26 (group g7) 28 30 (group g8) 32 34 (group g9) 36 38 (group g10)) t 1 t 5 t 13 t 21 t 29 t 37 t 0 t 1 t 2 t g1 t g2 t g3 t g4 t g5 t g6 t g7 t g8 t g9 t g10 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ")
  (let ((ov-outer (make-overlay 1 41))
        (ov-inner1 (make-overlay 5 16))
        (ov-inner2 (make-overlay 21 32))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 5 t))
        (m3 (copy-marker 13 t))
        (m4 (copy-marker 21 t))
        (m5 (copy-marker 29 t))
        (m6 (copy-marker 37)))
    (overlay-put ov-outer 'level 0)
    (overlay-put ov-inner1 'level 1)
    (overlay-put ov-inner2 'level 2)
    (put-text-property 1 3 'group 'g1)
    (put-text-property 5 7 'group 'g2)
    (put-text-property 9 11 'group 'g3)
    (put-text-property 13 15 'group 'g4)
    (put-text-property 17 19 'group 'g5)
    (put-text-property 21 23 'group 'g6)
    (put-text-property 25 27 'group 'g7)
    (put-text-property 29 31 'group 'g8)
    (put-text-property 33 35 'group 'g9)
    (put-text-property 37 39 'group 'g10)
    (undo-boundary)
    (narrow-to-region 5 32)
    (goto-char (point-min))
    (insert "QQ")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "CCC" nil t)
    (replace-match "ZZZZZZ")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "EEE" nil t)
    (replace-match "YYYYYY")
    (let ((narrowed (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (widen)
      (list narrowed
            (buffer-string)
            (string= (buffer-string) "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 5)
            (marker-position m3) (= (marker-position m3) 13)
            (marker-position m4) (= (marker-position m4) 21)
            (marker-position m5) (= (marker-position m5) 29)
            (marker-position m6) (= (marker-position m6) 37)
            (overlay-get ov-outer 'level) (= (overlay-get ov-outer 'level) 0)
            (overlay-get ov-inner1 'level) (= (overlay-get ov-inner1 'level) 1)
            (overlay-get ov-inner2 'level) (= (overlay-get ov-inner2 'level) 2)
            (get-text-property 1 'group) (eq (get-text-property 1 'group) 'g1)
            (get-text-property 5 'group) (eq (get-text-property 5 'group) 'g2)
            (get-text-property 9 'group) (eq (get-text-property 9 'group) 'g3)
            (get-text-property 13 'group) (eq (get-text-property 13 'group) 'g4)
            (get-text-property 17 'group) (eq (get-text-property 17 'group) 'g5)
            (get-text-property 21 'group) (eq (get-text-property 21 'group) 'g6)
            (get-text-property 25 'group) (eq (get-text-property 25 'group) 'g7)
            (get-text-property 29 'group) (eq (get-text-property 29 'group) 'g8)
            (get-text-property 33 'group) (eq (get-text-property 33 'group) 'g9)
            (get-text-property 37 'group) (eq (get-text-property 37 'group) 'g10))))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_yank_preserve_props_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (s1 t s2 t s3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((src (generate-new-buffer " test-kypac-src-xxx"))
        (dst (generate-new-buffer " test-kypac-dst-xxx")))
    (with-current-buffer src
      (insert "AAA-BBB-CCC")
      (put-text-property 1 3 'src 's1)
      (put-text-property 5 7 'src 's2)
      (put-text-property 9 11 'src 's3)
      (let ((ov (make-overlay 1 11)))
        (overlay-put ov 'origin 'src)
        (let ((extracted (buffer-substring 1 12)))
          (with-current-buffer dst
            (insert extracted)
            (let ((p1 (get-text-property 1 'src))
                  (p2 (get-text-property 5 'src))
                  (p3 (get-text-property 9 'src)))
              (kill-buffer src)
              (kill-buffer dst)
              (list p1 (eq p1 's1)
                    p2 (eq p2 's2)
                    p3 (eq p3 's3))))))))) "#,
        expect,
    );
}

#[test]
fn divergence_insert_buffer_substring_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (head t body t tail t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((src (generate-new-buffer " test-ibsp-src-xxx"))
        (dst (generate-new-buffer " test-ibsp-dst-xxx")))
    (with-current-buffer src
      (insert "HEAD-BODY-TAIL")
      (put-text-property 1 4 'part 'head)
      (put-text-property 5 8 'part 'body)
      (put-text-property 9 12 'part 'tail)
      (let ((ov (make-overlay 1 12)))
        (overlay-put ov 'type 'source)))
    (with-current-buffer dst
      (insert-buffer-substring src))
    (let ((p1 (with-current-buffer dst (get-text-property 1 'part)))
          (p2 (with-current-buffer dst (get-text-property 5 'part)))
          (p3 (with-current-buffer dst (get-text-property 9 'part)))
          (s (with-current-buffer dst (buffer-string))))
      (kill-buffer src)
      (kill-buffer dst)
      (list p1 (eq p1 'head)
            p2 (eq p2 'body)
            p3 (eq p3 'tail)
            (string= s "HEAD-BODY-TAIL"))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_after_multiple_narrow_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Changes to be undone are outside visible portion of buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AA-BB-CC-DD-EE-FF-GG-HH")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 4 t))
        (m3 (copy-marker 7 t))
        (m4 (copy-marker 10 t))
        (m5 (copy-marker 13 t))
        (m6 (copy-marker 16 t))
        (m7 (copy-marker 19 t))
        (m8 (copy-marker 22 t))
        (ov (make-overlay 1 23)))
    (overlay-put ov 'wrap 'full)
    (put-text-property 1 2 'seg 'a)
    (put-text-property 4 5 'seg 'b)
    (put-text-property 7 8 'seg 'c)
    (put-text-property 10 11 'seg 'd)
    (put-text-property 13 14 'seg 'e)
    (put-text-property 16 17 'seg 'f)
    (put-text-property 19 20 'seg 'g)
    (put-text-property 22 23 'seg 'h)
    (undo-boundary)
    (narrow-to-region 4 16)
    (goto-char (point-min))
    (insert "XX")
    (undo-boundary)
    (widen)
    (narrow-to-region 10 23)
    (goto-char (point-min))
    (insert "YY")
    (undo-boundary)
    (widen)
    (narrow-to-region 1 8)
    (goto-char (point-min))
    (re-search-forward "BB" nil t)
    (replace-match "ZZZZ")
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (widen)
      (list s
            (buffer-string)
            (string= (buffer-string) "AA-BB-CC-DD-EE-FF-GG-HH")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 4)
            (marker-position m3) (= (marker-position m3) 7)
            (marker-position m4) (= (marker-position m4) 10)
            (marker-position m5) (= (marker-position m5) 13)
            (marker-position m6) (= (marker-position m6) 16)
            (marker-position m7) (= (marker-position m7) 19)
            (marker-position m8) (= (marker-position m8) 22)
            (overlay-get ov 'wrap) (eq (overlay-get ov 'wrap) 'full)
            (get-text-property 1 'seg) (eq (get-text-property 1 'seg) 'a)
            (get-text-property 4 'seg) (eq (get-text-property 4 'seg) 'b)
            (get-text-property 7 'seg) (eq (get-text-property 7 'seg) 'c)
            (get-text-property 10 'seg) (eq (get-text-property 10 'seg) 'd)
            (get-text-property 13 'seg) (eq (get-text-property 13 'seg) 'e)
            (get-text-property 16 'seg) (eq (get-text-property 16 'seg) 'f)
            (get-text-property 19 'seg) (eq (get-text-property 19 'seg) 'g)
            (get-text-property 22 'seg) (eq (get-text-property 22 'seg) 'h))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_priority_ordering_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((10 20 30) t (10 20 30) t #(\"ABCDEFGHIJ\" 0 2 (layer outer) 3 4 (layer middle) 4 6 (layer middle) 7 9 (layer inner)) t outer t middle t inner t outer t middle t inner t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((ov1 (make-overlay 1 10))
        (ov2 (make-overlay 3 8))
        (ov3 (make-overlay 5 6)))
    (overlay-put ov1 'priority 10)
    (overlay-put ov2 'priority 20)
    (overlay-put ov3 'priority 30)
    (overlay-put ov1 'tag 'outer)
    (overlay-put ov2 'tag 'middle)
    (overlay-put ov3 'tag 'inner)
    (put-text-property 1 3 'layer 'outer)
    (put-text-property 4 7 'layer 'middle)
    (put-text-property 8 10 'layer 'inner)
    (undo-boundary)
    (goto-char 5)
    (insert "XXXX")
    (let ((priorities-before (mapcar (lambda (ov)
                                       (overlay-get ov 'priority))
                                     (sort (overlays-at 6)
                                           (lambda (a b)
                                             (< (overlay-get a 'priority)
                                                (overlay-get b 'priority)))))))
      (primitive-undo 1 buffer-undo-list)
      (let ((priorities-after (mapcar (lambda (ov)
                                         (overlay-get ov 'priority))
                                       (sort (overlays-at 5)
                                             (lambda (a b)
                                               (< (overlay-get a 'priority)
                                                  (overlay-get b 'priority)))))))
        (list priorities-before
              (equal priorities-before '(10 20 30))
              priorities-after
              (equal priorities-after '(10 20 30))
              (buffer-string)
              (string= (buffer-string) "ABCDEFGHIJ")
              (overlay-get ov1 'tag) (eq (overlay-get ov1 'tag) 'outer)
              (overlay-get ov2 'tag) (eq (overlay-get ov2 'tag) 'middle)
              (overlay-get ov3 'tag) (eq (overlay-get ov3 'tag) 'inner)
              (get-text-property 1 'layer) (eq (get-text-property 1 'layer) 'outer)
              (get-text-property 4 'layer) (eq (get-text-property 4 'layer) 'middle)
              (get-text-property 8 'layer) (eq (get-text-property 8 'layer) 'inner)))))) "#,
        expect,
    );
}

#[test]
fn divergence_text_property_merge_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (red nil nil blue #(\"XXXX\" 0 1 (color red) 2 3 (color blue)) t red t nil t blue t bold t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "XXXX")
  (put-text-property 1 2 'color 'red)
  (put-text-property 3 4 'color 'blue)
  (let ((ov (make-overlay 1 4)))
    (overlay-put ov 'face 'bold)
    (undo-boundary)
    (goto-char 2)
    (insert "YY")
    (let ((p1 (get-text-property 1 'color))
          (p2 (get-text-property 2 'color))
          (p3 (get-text-property 4 'color))
          (p4 (get-text-property 5 'color)))
      (primitive-undo 1 buffer-undo-list)
      (list p1 p2 p3 p4
            (buffer-string)
            (string= (buffer-string) "XXXX")
            (get-text-property 1 'color) (eq (get-text-property 1 'color) 'red)
            (get-text-property 2 'color) (null (get-text-property 2 'color))
            (get-text-property 3 'color) (eq (get-text-property 3 'color) 'blue)
            (overlay-get ov 'face) (eq (overlay-get ov 'face) 'bold))))) "#,
        expect,
    );
}

#[test]
fn divergence_replace_match_preserve_adjacent_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable s)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LEFT-MIDDLE-RIGHT")
  (put-text-property 1 4 'zone 'left)
  (put-text-property 5 10 'zone 'center)
  (put-text-property 11 15 'zone 'right)
  (let ((ov-l (make-overlay 1 4))
        (ov-c (make-overlay 5 10))
        (ov-r (make-overlay 11 15)))
    (overlay-put ov-l 'side 'left)
    (overlay-put ov-c 'side 'center)
    (overlay-put ov-r 'side 'right)
    (goto-char 1)
    (re-search-forward "MIDDLE" nil t)
    (replace-match "REPLACED")
    (let ((s (buffer-string))
          (pl (get-text-property 1 'zone))
          (pr (get-text-property (+ 1 (length s) -5) 'zone)))
      (list s pl pr
            (eq pl 'left)
            (eq pr 'right)
            (overlay-get ov-l 'side) (eq (overlay-get ov-l 'side) 'left)
            (overlay-get ov-r 'side) (eq (overlay-get ov-r 'side) 'right))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_display_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil t \"(hidden)\" t show t show t display-test t #(\"SHOW-HIDE-SHOW\" 0 3 (vis show) 5 8 (vis hidden display \"(hidden)\") 10 13 (vis show)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "SHOW-HIDE-SHOW")
  (put-text-property 6 9 'display "(hidden)")
  (put-text-property 1 4 'vis 'show)
  (put-text-property 6 9 'vis 'hidden)
  (put-text-property 11 14 'vis 'show)
  (let ((ov (make-overlay 1 14)))
    (overlay-put ov 'before-string
                 (propertize ">" 'display '(left-fringe right-arrow)))
    (overlay-put ov 'tag 'display-test)
    (undo-boundary)
    (put-text-property 6 9 'display nil)
    (let ((d1 (get-text-property 6 'display)))
      (primitive-undo 1 buffer-undo-list)
      (list d1 (null d1)
            (get-text-property 6 'display)
            (equal (get-text-property 6 'display) "(hidden)")
            (get-text-property 1 'vis) (eq (get-text-property 1 'vis) 'show)
            (get-text-property 11 'vis) (eq (get-text-property 11 'vis) 'show)
            (overlay-get ov 'tag) (eq (overlay-get ov 'tag) 'display-test)
            (buffer-string)
            (string= (buffer-string) "SHOW-HIDE-SHOW"))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_before_after_string_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"CONTEN---T-THERE\" 0 6 (half first) 10 11 (half second)) #(\"CONTENT-HERE\" 0 6 (half first) 7 8 (half second) 8 11 (half second)) t 1 t 8 t \"[\" t \"]\" t underline t first t second t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "CONTENT-HERE")
  (let ((ov (make-overlay 1 12)))
    (overlay-put ov 'before-string "[")
    (overlay-put ov 'after-string "]")
    (overlay-put ov 'face 'underline)
    (put-text-property 1 7 'half 'first)
    (put-text-property 8 12 'half 'second)
    (let ((m1 (copy-marker 1 t))
          (m2 (copy-marker 8 t)))
      (undo-boundary)
      (goto-char 7)
      (insert "---")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "HERE" nil t)
      (replace-match "THERE")
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "CONTENT-HERE")
              (marker-position m1) (= (marker-position m1) 1)
              (marker-position m2) (= (marker-position m2) 8)
              (overlay-get ov 'before-string) (string= (overlay-get ov 'before-string) "[")
              (overlay-get ov 'after-string) (string= (overlay-get ov 'after-string) "]")
              (overlay-get ov 'face) (eq (overlay-get ov 'face) 'underline)
              (get-text-property 1 'half) (eq (get-text-property 1 'half) 'first)
              (get-text-property 8 'half) (eq (get-text-property 8 'half) 'second)))))) "#,
        expect,
    );
}

#[test]
fn divergence_multiple_replace_in_sequence_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 53 76)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AA-BB-CC-DD-EE-FF-GG-HH")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 4 t))
        (m3 (copy-marker 7 t))
        (m4 (copy-marker 10 t))
        (m5 (copy-marker 13 t))
        (m6 (copy-marker 16 t))
        (m7 (copy-marker 19 t))
        (m8 (copy-marker 22 t)))
    (put-text-property 1 2 'id 1)
    (put-text-property 4 5 'id 2)
    (put-text-property 7 8 'id 3)
    (put-text-property 10 11 'id 4)
    (put-text-property 13 14 'id 5)
    (put-text-property 16 17 'id 6)
    (put-text-property 19 20 'id 7)
    (put-text-property 22 23 'id 8)
    (let ((ov (make-overlay 1 23)))
      (overlay-put ov 'scope 'all))
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "AA" nil t)
    (replace-match "XX")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "CC" nil t)
    (replace-match "YY")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "EE" nil t)
    (replace-match "ZZ")
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "AA-BB-CC-DD-EE-FF-GG-HH")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 4)
            (marker-position m3) (= (marker-position m3) 7)
            (marker-position m4) (= (marker-position m4) 10)
            (marker-position m5) (= (marker-position m5) 13)
            (marker-position m6) (= (marker-position m6) 16)
            (marker-position m7) (= (marker-position m7) 19)
            (marker-position m8) (= (marker-position m8) 22)
            (get-text-property 1 'id) (= (get-text-property 1 'id) 1)
            (get-text-property 4 'id) (= (get-text-property 4 'id) 2)
            (get-text-property 7 'id) (= (get-text-property 7 'id) 3)
            (get-text-property 10 'id) (= (get-text-property 10 'id) 4)
            (get-text-property 13 'id) (= (get-text-property 13 'id) 5)
            (get-text-property 16 'id) (= (get-text-property 16 'id) 6)
            (get-text-property 19 'id) (= (get-text-property 19 'id) 7)
            (get-text-property 22 'id) (= (get-text-property 22 'id) 8)))))) "#,
        expect,
    );
}
