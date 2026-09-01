//! Divergence tests: mega undo stress — marker + textprop + overlay + narrow + regex.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_replace_with_props_and_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AA-XXXXXX-CC-DD-YYYYYY-FF-GG-HH-II-JJ\" 0 2 (section a) 10 12 (section c) 13 15 (section d) 23 25 (section f) 26 28 (section g) 29 31 (section h) 32 34 (section i) 35 36 (section j)) 4 25 a nil #(\"AA-BB-CC-DD-EE-FF-GG-HH-II-JJ\" 0 2 (section a) 3 5 (section b) 6 8 (section c) 9 11 (section d) 12 14 (section e) 15 17 (section f) 18 20 (section g) 21 23 (section h) 24 26 (section i) 27 28 (section j)) t 1 t 6 nil 10 t 17 t 24 t active t a t b t c t d t e t f t g t h t i t j t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AA-BB-CC-DD-EE-FF-GG-HH-II-JJ")
  (let ((ov (make-overlay 4 17))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 4 t))
        (m3 (copy-marker 10 t))
        (m4 (copy-marker 17))
        (m5 (copy-marker 24)))
    (overlay-put ov 'zone 'active)
    (put-text-property 1 3 'section 'a)
    (put-text-property 4 6 'section 'b)
    (put-text-property 7 9 'section 'c)
    (put-text-property 10 12 'section 'd)
    (put-text-property 13 15 'section 'e)
    (put-text-property 16 18 'section 'f)
    (put-text-property 19 21 'section 'g)
    (put-text-property 22 24 'section 'h)
    (put-text-property 25 27 'section 'i)
    (put-text-property 28 29 'section 'j)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "BB" nil t)
    (replace-match "XXXXXX")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "EE" nil t)
    (replace-match "YYYYYY")
    (let ((s (buffer-string))
          (ov-start (overlay-start ov))
          (ov-end (overlay-end ov))
          (p-a (get-text-property 1 'section))
          (p-d (get-text-property 10 'section)))
      (primitive-undo 2 buffer-undo-list)
      (list s ov-start ov-end p-a p-d
            (buffer-string)
            (string= (buffer-string) "AA-BB-CC-DD-EE-FF-GG-HH-II-JJ")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 4)
            (marker-position m3) (= (marker-position m3) 10)
            (marker-position m4) (= (marker-position m4) 17)
            (marker-position m5) (= (marker-position m5) 24)
            (overlay-get ov 'zone) (eq (overlay-get ov 'zone) 'active)
            (get-text-property 1 'section) (eq (get-text-property 1 'section) 'a)
            (get-text-property 4 'section) (eq (get-text-property 4 'section) 'b)
            (get-text-property 7 'section) (eq (get-text-property 7 'section) 'c)
            (get-text-property 10 'section) (eq (get-text-property 10 'section) 'd)
            (get-text-property 13 'section) (eq (get-text-property 13 'section) 'e)
            (get-text-property 16 'section) (eq (get-text-property 16 'section) 'f)
            (get-text-property 19 'section) (eq (get-text-property 19 'section) 'g)
            (get-text-property 22 'section) (eq (get-text-property 22 'section) 'h)
            (get-text-property 25 'section) (eq (get-text-property 25 'section) 'i)
            (get-text-property 28 'section) (eq (get-text-property 28 'section) 'j))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_narrow_insert_replace_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"ZZCC-WWWW-EE-FF-GG-HH\" 2 5 (region a) 10 21 (region b)) #(\"AA-BB-CC-DD-EE-FF-GG-HH-II-JJ-KK-LL\" 0 6 (region a) 6 9 (region a) 9 11 (region a) 12 23 (region b) 24 34 (region c)) t 1 t 7 t 13 t 19 t 25 t first t second t third t a t b t c t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AA-BB-CC-DD-EE-FF-GG-HH-II-JJ-KK-LL")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 7 t))
        (m3 (copy-marker 13 t))
        (m4 (copy-marker 19 t))
        (m5 (copy-marker 25))
        (ov1 (make-overlay 1 12))
        (ov2 (make-overlay 13 24))
        (ov3 (make-overlay 25 35)))
    (overlay-put ov1 'group 'first)
    (overlay-put ov2 'group 'second)
    (overlay-put ov3 'group 'third)
    (put-text-property 1 12 'region 'a)
    (put-text-property 13 24 'region 'b)
    (put-text-property 25 35 'region 'c)
    (undo-boundary)
    (narrow-to-region 7 24)
    (goto-char (point-min))
    (insert "ZZ")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "DD" nil t)
    (replace-match "WWWW")
    (let ((narrow-s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (widen)
      (list narrow-s
            (buffer-string)
            (string= (buffer-string) "AA-BB-CC-DD-EE-FF-GG-HH-II-JJ-KK-LL")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 7)
            (marker-position m3) (= (marker-position m3) 13)
            (marker-position m4) (= (marker-position m4) 19)
            (marker-position m5) (= (marker-position m5) 25)
            (overlay-get ov1 'group) (eq (overlay-get ov1 'group) 'first)
            (overlay-get ov2 'group) (eq (overlay-get ov2 'group) 'second)
            (overlay-get ov3 'group) (eq (overlay-get ov3 'group) 'third)
            (get-text-property 1 'region) (eq (get-text-property 1 'region) 'a)
            (get-text-property 13 'region) (eq (get-text-property 13 'region) 'b)
            (get-text-property 25 'region) (eq (get-text-property 25 'region) 'c))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_kill_yank_with_overlay_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 34 39)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BLOCK1---BLOCK2---BLOCK3---BLOCK4")
  (let ((ov1 (make-overlay 1 6))
        (ov2 (make-overlay 12 17))
        (ov3 (make-overlay 23 28))
        (ov4 (make-overlay 34 39))
        (m (copy-marker 12 t)))
    (overlay-put ov1 'idx 1)
    (overlay-put ov2 'idx 2)
    (overlay-put ov3 'idx 3)
    (overlay-put ov4 'idx 4)
    (put-text-property 1 6 'block 'one)
    (put-text-property 12 17 'block 'two)
    (put-text-property 23 28 'block 'three)
    (put-text-property 34 39 'block 'four)
    (undo-boundary)
    (kill-region 7 11)
    (undo-boundary)
    (goto-char (marker-position m))
    (insert "INSERTED")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "BLOCK2" nil t)
    (replace-match "CHANGED")
    (let ((s (buffer-string))
          (m-pos (marker-position m)))
      (primitive-undo 3 buffer-undo-list)
      (list s m-pos
            (buffer-string)
            (string= (buffer-string) "BLOCK1---BLOCK2---BLOCK3---BLOCK4")
            (marker-position m) (= (marker-position m) 12)
            (overlay-get ov1 'idx) (= (overlay-get ov1 'idx) 1)
            (overlay-get ov2 'idx) (= (overlay-get ov2 'idx) 2)
            (overlay-get ov3 'idx) (= (overlay-get ov3 'idx) 3)
            (overlay-get ov4 'idx) (= (overlay-get ov4 'idx) 4)
            (get-text-property 1 'block) (eq (get-text-property 1 'block) 'one)
            (get-text-property 12 'block) (eq (get-text-property 12 'block) 'two)
            (get-text-property 23 'block) (eq (get-text-property 23 'block) 'three)
            (get-text-property 34 'block) (eq (get-text-property 34 'block) 'four))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_multiple_overlays_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 41 77)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LINE1\nLINE2\nLINE3\nLINE4\nLINE5")
  (let ((ovs (list (make-overlay 1 5) (make-overlay 7 11)
                   (make-overlay 13 17) (make-overlay 19 23)
                   (make-overlay 25 29))))
    (dotimes (i 5)
      (overlay-put (nth i ovs) 'line-num (+ i 1))
      (overlay-put (nth i ovs) 'face (list 'bold 'italic)))
    (put-text-property 1 5 'line 1)
    (put-text-property 7 11 'line 2)
    (put-text-property 13 17 'line 3)
    (put-text-property 19 23 'line 4)
    (put-text-property 25 29 'line 5)
    (undo-boundary)
    (goto-char 7)
    (kill-line)
    (undo-boundary)
    (goto-char 13)
    (insert "REPLACED-LINE")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "LINE4" nil t)
    (replace-match "MODIFIED")
    (primitive-undo 3 buffer-undo-list)
    (list (buffer-string)
          (string= (buffer-string) "LINE1\nLINE2\nLINE3\nLINE4\nLINE5")
          (overlay-get (nth 0 ovs) 'line-num)
          (= (overlay-get (nth 0 ovs) 'line-num) 1)
          (overlay-get (nth 1 ovs) 'line-num)
          (= (overlay-get (nth 1 ovs) 'line-num) 2)
          (overlay-get (nth 2 ovs) 'line-num)
          (= (overlay-get (nth 2 ovs) 'line-num) 3)
          (overlay-get (nth 3 ovs) 'line-num)
          (= (overlay-get (nth 3 ovs) 'line-num) 4)
          (overlay-get (nth 4 ovs) 'line-num)
          (= (overlay-get (nth 4 ovs) 'line-num) 5)
          (get-text-property 1 'line) (= (get-text-property 1 'line) 1)
          (get-text-property 7 'line) (= (get-text-property 7 'line) 2)
          (get-text-property 13 'line) (= (get-text-property 13 'line) 3)
          (get-text-property 19 'line) (= (get-text-property 19 'line) 4)
          (get-text-property 25 'line) (= (get-text-property 25 'line) 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "KEEP-REMOVE-KEEP-REMOVE-KEEP")
  (let ((ov1 (make-overlay 6 11))
        (ov2 (make-overlay 18 23))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 6 t))
        (m3 (copy-marker 12 t)))
    (overlay-put ov1 'evaporate t)
    (overlay-put ov2 'evaporate t)
    (overlay-put ov1 'tag 'first)
    (overlay-put ov2 'tag 'second)
    (put-text-property 1 5 'zone 'keep1)
    (put-text-property 6 11 'zone 'remove1)
    (put-text-property 12 16 'zone 'keep2)
    (put-text-property 18 23 'zone 'remove2)
    (put-text-property 25 29 'zone 'keep3)
    (undo-boundary)
    (delete-region 6 11)
    (undo-boundary)
    (delete-region 12 17)
    (let ((s (buffer-string))
          (ov1-live (overlay-start ov1))
          (ov2-live (overlay-start ov2))
          (m1-pos (marker-position m1))
          (m2-pos (marker-position m2))
          (m3-pos (marker-position m3)))
      (primitive-undo 2 buffer-undo-list)
      (list s ov1-live ov2-live m1-pos m2-pos m3-pos
            (buffer-string)
            (string= (buffer-string) "KEEP-REMOVE-KEEP-REMOVE-KEEP")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 6)
            (marker-position m3) (= (marker-position m3) 12)
            (overlay-start ov1) (= (overlay-start ov1) 6)
            (overlay-start ov2) (= (overlay-start ov2) 18)
            (overlay-get ov1 'tag) (eq (overlay-get ov1 'tag) 'first)
            (overlay-get ov2 'tag) (eq (overlay-get ov2 'tag) 'second)
            (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'keep1)
            (get-text-property 6 'zone) (eq (get-text-property 6 'zone) 'remove1)
            (get-text-property 12 'zone) (eq (get-text-property 12 'zone) 'keep2))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_regex_replace_preserve_intervals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 41 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "tok1-sep-tok2-sep-tok3-sep-tok4-sep-tok5")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 5 t))
        (m3 (copy-marker 9 t))
        (m4 (copy-marker 13 t))
        (m5 (copy-marker 17 t))
        (m6 (copy-marker 21 t)))
    (put-text-property 1 4 'tok 1)
    (put-text-property 5 8 'tok 2)
    (put-text-property 9 12 'tok 3)
    (put-text-property 13 16 'tok 4)
    (put-text-property 17 20 'tok 5)
    (put-text-property 21 24 'sep 's1)
    (put-text-property 25 28 'sep 's2)
    (put-text-property 29 32 'sep 's3)
    (put-text-property 33 36 'sep 's4)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "sep" nil t)
      (replace-match "SEPARATOR"))
    (let ((s (buffer-string))
          (tok1 (get-text-property 1 'tok))
          (sep1 (get-text-property 25 'sep)))
      (primitive-undo 1 buffer-undo-list)
      (list s tok1 sep1
            (buffer-string)
            (string= (buffer-string) "tok1-sep-tok2-sep-tok3-sep-tok4-sep-tok5")
            (get-text-property 1 'tok) (= (get-text-property 1 'tok) 1)
            (get-text-property 5 'tok) (= (get-text-property 5 'tok) 2)
            (get-text-property 9 'tok) (= (get-text-property 9 'tok) 3)
            (get-text-property 13 'tok) (= (get-text-property 13 'tok) 4)
            (get-text-property 17 'tok) (= (get-text-property 17 'tok) 5)
            (get-text-property 21 'sep) (eq (get-text-property 21 'sep) 's1)
            (get-text-property 25 'sep) (eq (get-text-property 25 'sep) 's2)
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 5)
            (marker-position m3) (= (marker-position m3) 9)
            (marker-position m4) (= (marker-position m4) 13)
            (marker-position m5) (= (marker-position m5) 17)
            (marker-position m6) (= (marker-position m6) 21)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_insert_delete_with_prop_transitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAABBBBCCCCDDDDEEEE\" 0 3 (color red) 4 7 (color blue) 8 11 (color green) 12 15 (color yellow) 16 19 (color purple)) t 5 t all t red t blue t green t yellow t purple t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAABBBBCCCCDDDDEEEE")
  (let ((m (copy-marker 5 t))
        (ov (make-overlay 1 20)))
    (overlay-put ov 'wrap 'all)
    (put-text-property 1 4 'color 'red)
    (put-text-property 5 8 'color 'blue)
    (put-text-property 9 12 'color 'green)
    (put-text-property 13 16 'color 'yellow)
    (put-text-property 17 20 'color 'purple)
    (undo-boundary)
    (goto-char 4)
    (insert "XXXX")
    (undo-boundary)
    (delete-region 4 8)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "AAAA" nil t)
    (replace-match "AAAA" nil t)
    (primitive-undo 3 buffer-undo-list)
    (list (buffer-string)
          (string= (buffer-string) "AAAABBBBCCCCDDDDEEEE")
          (marker-position m) (= (marker-position m) 5)
          (overlay-get ov 'wrap) (eq (overlay-get ov 'wrap) 'all)
          (get-text-property 1 'color) (eq (get-text-property 1 'color) 'red)
          (get-text-property 5 'color) (eq (get-text-property 5 'color) 'blue)
          (get-text-property 9 'color) (eq (get-text-property 9 'color) 'green)
          (get-text-property 13 'color) (eq (get-text-property 13 'color) 'yellow)
          (get-text-property 17 'color) (eq (get-text-property 17 'color) 'purple)))) "#,
        expect,
    );
}
