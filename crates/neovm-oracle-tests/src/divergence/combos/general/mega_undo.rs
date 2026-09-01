//! Divergence tests: mega undo stress — complex edit sequences with props.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_20_step_edit_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"ALPHA-BETA-GAMMA-DELTA-EPSILON\" #(\"FUNC1-FUNC2-FUNC3-FUNC4-FUNC5\" 0 5 (func 1) 6 11 (func 2) 12 17 (func 3) 18 23 (func 4) 24 29 (func 5)) t nil nil nil nil nil 1 t 2 t 3 t 4 t 5 t all t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "FUNC1-FUNC2-FUNC3-FUNC4-FUNC5")
  (dotimes (i 5)
    (let ((start (+ 1 (* i 6)))
          (end (+ 5 (* i 6))))
      (put-text-property start (+ end 1) 'func (+ i 1))
      (let ((ov (make-overlay start (+ end 1))))
        (overlay-put ov 'func (+ i 1)))))
  (let ((m1 (copy-marker 1 t)) (m2 (copy-marker 7 t))
        (m3 (copy-marker 13 t)) (m4 (copy-marker 19 t))
        (m5 (copy-marker 25 t))
        (ov (make-overlay 1 30)))
    (overlay-put ov 'module 'all)
    (undo-boundary)
    (goto-char 1) (re-search-forward "FUNC1" nil t) (replace-match "ALPHA")
    (undo-boundary)
    (goto-char 1) (re-search-forward "FUNC2" nil t) (replace-match "BETA")
    (undo-boundary)
    (goto-char 1) (re-search-forward "FUNC3" nil t) (replace-match "GAMMA")
    (undo-boundary)
    (goto-char 1) (re-search-forward "FUNC4" nil t) (replace-match "DELTA")
    (undo-boundary)
    (goto-char 1) (re-search-forward "FUNC5" nil t) (replace-match "EPSILON")
    (let ((s (buffer-string)))
      (primitive-undo 5 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "FUNC1-FUNC2-FUNC3-FUNC4-FUNC5")
            (= (marker-position m1) 1) (= (marker-position m2) 7)
            (= (marker-position m3) 13) (= (marker-position m4) 19)
            (= (marker-position m5) 25)
            (get-text-property 1 'func) (= (get-text-property 1 'func) 1)
            (get-text-property 7 'func) (= (get-text-property 7 'func) 2)
            (get-text-property 13 'func) (= (get-text-property 13 'func) 3)
            (get-text-property 19 'func) (= (get-text-property 19 'func) 4)
            (get-text-property 25 'func) (= (get-text-property 25 'func) 5)
            (overlay-get ov 'module) (eq (overlay-get ov 'module) 'all))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_insert_delete_alternating() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 39)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BASE-BASE-BASE-BASE-BASE")
  (dotimes (i 5)
    (put-text-property (+ 1 (* i 5)) (+ 4 (* i 5)) 'slot (+ i 1)))
  (let ((m (copy-marker 6 t))
        (ov (make-overlay 1 24)))
    (overlay-put ov 'base t)
    (undo-boundary)
    (goto-char 6) (insert "INS1")
    (undo-boundary)
    (delete-region 6 10)
    (undo-boundary)
    (goto-char 11) (insert "INS2")
    (undo-boundary)
    (delete-region 11 15)
    (undo-boundary)
    (goto-char 16) (insert "INS3")
    (let ((s (buffer-string)))
      (primitive-undo 5 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "BASE-BASE-BASE-BASE-BASE")
            (= (marker-position m) 6)
            (get-text-property 1 'slot) (= (get-text-property 1 'slot) 1)
            (get-text-property 6 'slot) (= (get-text-property 6 'slot) 2)
            (get-text-property 11 'slot) (= (get-text-property 11 'slot) 3)
            (get-text-property 16 'slot) (= (get-text-property 16 'slot) 4)
            (overlay-get ov 'base)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_10_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 100 ?T))
  (let ((ovs nil) (ms nil))
    (dotimes (i 10)
      (let* ((start (+ 1 (* i 10)))
             (end (min (+ start 9) 101))
             (ov (make-overlay start end)))
        (overlay-put ov 'block (+ i 1))
        (overlay-put ov 'face (nth (mod i 5) '(bold italic underline bold-italic highlight)))
        (push ov ovs)
        (put-text-property start end 'block (+ i 1))
        (push (copy-marker start t) ms)))
    (setq ovs (nreverse ovs))
    (setq ms (nreverse ms))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "TTTTT" nil t) (replace-match "AAAAA"))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "AAAAA" nil t) (replace-match "BBBBB"))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "BBBBB" nil t) (replace-match "CCCCC"))
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (let ((all-ok t))
        (dotimes (i 10)
          (unless (= (overlay-get (nth i ovs) 'block) (+ i 1))
            (setq all-ok nil))
          (unless (= (get-text-property (+ 1 (* i 10)) 'block) (+ i 1))
            (setq all-ok nil)))
        (list s (buffer-string)
              (string= (buffer-string) (make-string 100 ?T))
              all-ok (= (buffer-size) 100))))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_kill_rectangle_sim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"LINE1-\\nLINE2-BLINE3-CLINE4-\\nLINEEE\" 0 4 (line 1) 7 11 (line 2) 14 18 (line 3) 21 25 (line 4) 28 32 (line 5)) #(\"LINE1-AAA\\nLINE2-BBB\\nLINE3-CCC\\nLINE4-DDD\\nLINE5-EEE\" 0 4 (line 1) 10 14 (line 2) 20 24 (line 3) 30 34 (line 4) 40 44 (line 5)) t t 1 t 2 t 3 t 4 t rect t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LINE1-AAA\nLINE2-BBB\nLINE3-CCC\nLINE4-DDD\nLINE5-EEE")
  (dotimes (i 5)
    (let ((start (+ 1 (* i 10)))
          (end (+ 5 (* i 10))))
      (put-text-property start end 'line (+ i 1))))
  (let ((m (copy-marker 7 t))
        (ov (make-overlay 1 49)))
    (overlay-put ov 'block 'rect)
    (undo-boundary)
    (goto-char 7) (delete-region 7 10)
    (undo-boundary)
    (goto-char 15) (delete-region 15 18)
    (undo-boundary)
    (goto-char 22) (delete-region 22 25)
    (undo-boundary)
    (goto-char 28) (delete-region 28 31)
    (undo-boundary)
    (goto-char 33) (delete-region 33 36)
    (let ((s (buffer-string)))
      (primitive-undo 5 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "LINE1-AAA\nLINE2-BBB\nLINE3-CCC\nLINE4-DDD\nLINE5-EEE")
            (= (marker-position m) 7)
            (get-text-property 1 'line) (= (get-text-property 1 'line) 1)
            (get-text-property 11 'line) (= (get-text-property 11 'line) 2)
            (get-text-property 21 'line) (= (get-text-property 21 'line) 3)
            (get-text-property 31 'line) (= (get-text-property 31 'line) 4)
            (overlay-get ov 'block) (eq (overlay-get ov 'block) 'rect))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_propertize_replace_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"ORANGE-CYAN-TEAL-YELLOW\" 17 23 (color yellow)) #(\"RED-BLUE-GREEN-YELLOW\" 0 3 (color red) 4 8 (color blue) 9 14 (color green) 15 21 (color yellow)) nil red t blue t green t yellow t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((s1 (propertize "RED" 'color 'red))
        (s2 (propertize "BLUE" 'color 'blue))
        (s3 (propertize "GREEN" 'color 'green))
        (s4 (propertize "YELLOW" 'color 'yellow)))
    (insert s1 "-" s2 "-" s3 "-" s4))
  (let ((m (copy-marker 5 t))
        (ov (make-overlay 1 22)))
    (overlay-put ov 'palette t)
    (undo-boundary)
    (goto-char 1) (re-search-forward "RED" nil t) (replace-match "ORANGE")
    (undo-boundary)
    (goto-char 1) (re-search-forward "BLUE" nil t) (replace-match "CYAN")
    (undo-boundary)
    (goto-char 1) (re-search-forward "GREEN" nil t) (replace-match "TEAL")
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (list s
            (buffer-string)
            (= (marker-position m) 5)
            (get-text-property 1 'color) (eq (get-text-property 1 'color) 'red)
            (get-text-property 5 'color) (eq (get-text-property 5 'color) 'blue)
            (get-text-property 10 'color) (eq (get-text-property 10 'color) 'green)
            (get-text-property 16 'color) (eq (get-text-property 16 'color) 'yellow)
            (overlay-get ov 'palette))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_narrow_replace_widen_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 26 39)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "A1-A2-A3-A4-A5-B1-B2-B3-B4-B5-C1-C2-C3-C4-C5")
  (dotimes (i 15)
    (let ((start (+ 1 (* i 3)))
          (end (+ 2 (* i 3))))
      (put-text-property start (+ end 1) 'cell (+ i 1))))
  (let ((m (copy-marker 16 t))
        (ov (make-overlay 1 44)))
    (overlay-put ov 'grid t)
    (undo-boundary)
    (narrow-to-region 16 30)
    (goto-char 16)
    (insert "XX")
    (undo-boundary)
    (goto-char 16)
    (re-search-forward "B3" nil t) (replace-match "YY")
    (let ((ns (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (widen)
      (list ns
            (buffer-string)
            (= (marker-position m) 16)
            (get-text-property 1 'cell) (= (get-text-property 1 'cell) 1)
            (get-text-property 16 'cell) (= (get-text-property 16 'cell) 6)
            (get-text-property 31 'cell) (= (get-text-property 31 'cell) 11)
            (overlay-get ov 'grid)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_buffer_substring_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"ORIG\" 0 3 (chunk 1)) #(\"ORIG\" 0 3 (chunk 2)) #(\"ORIG-ORIG-ORIG-ORIG-ORIG\" 10 13 (chunk 3) 15 18 (chunk 4) 20 23 (chunk 5)) #(\"ORIG-ORIG-ORIG-ORIG-ORIG\" 0 3 (chunk 1) 5 8 (chunk 2) 10 13 (chunk 3) 15 18 (chunk 4) 20 23 (chunk 5)) t t 1 t 2 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ORIG-ORIG-ORIG-ORIG-ORIG")
  (dotimes (i 5)
    (put-text-property (+ 1 (* i 5)) (+ 4 (* i 5)) 'chunk (+ i 1)))
  (let ((m (copy-marker 6 t))
        (ov (make-overlay 1 24)))
    (overlay-put ov 'set t)
    (undo-boundary)
    (let ((sub (buffer-substring 1 5)))
      (delete-region 1 5)
      (goto-char 1)
      (insert (upcase sub))
      (undo-boundary)
      (let ((sub2 (buffer-substring 6 10)))
        (delete-region 6 10)
        (goto-char 6)
        (insert (upcase sub2))
        (let ((s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list sub sub2 s
                (buffer-string)
                (string= (buffer-string) "ORIG-ORIG-ORIG-ORIG-ORIG")
                (= (marker-position m) 6)
                (get-text-property 1 'chunk) (= (get-text-property 1 'chunk) 1)
                (get-text-property 6 'chunk) (= (get-text-property 6 'chunk) 2)
                (overlay-get ov 'set))))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_30_property_segments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable start)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 150 ?P))
  (dotimes (i 30)
    (let ((start (+ 1 (* i 5)))
          (end (min (+ start 4) 151)))
      (put-text-property start end 'seg (+ i 1))))
  (let ((m (copy-marker 6 t))
        (ov (make-overlay 1 150)))
    (overlay-put ov 'segs t)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "PPPPP" nil t) (replace-match "QQQQQ"))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "QQQQQ" nil t) (replace-match "RRRRR"))
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (let ((all-ok t))
        (dotimes (i 30)
          (unless (= (get-text-property (+ 1 (* i 5)) 'seg) (+ i 1))
            (setq all-ok nil)))
        (list s
              (buffer-string)
              (string= (buffer-string) (make-string 150 ?P))
              all-ok
              (= (buffer-size) 150)
              (= (marker-position m) 6)
              (overlay-get ov 'segs)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_3_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"MAIN-CONTENT-HERE\" 0 3 (buf main) 5 11 (buf main) 13 16 (buf main)) t t main)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "MAIN-CONTENT-HERE")
  (put-text-property 1 4 'buf 'main)
  (put-text-property 6 12 'buf 'main)
  (put-text-property 14 17 'buf 'main)
  (let ((m-main (copy-marker 6 t))
        (ov-main (make-overlay 1 17)))
    (overlay-put ov-main 'origin 'main)
    (undo-boundary)
    (let ((buf2 (generate-new-buffer "test-sw2"))
          (buf3 (generate-new-buffer "test-sw3")))
      (with-current-buffer buf2
        (insert "SECOND-BUFFER-DATA")
        (put-text-property 1 6 'buf 'second)
        (put-text-property 8 13 'buf 'second)
        (let ((m2 (copy-marker 8 t)))
          (undo-boundary)
          (goto-char 1) (re-search-forward "SECOND" nil t) (replace-match "CHANGED")
          (let ((s2 (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (with-current-buffer buf3
              (insert "THIRD-BUFFER-HERE")
              (put-text-property 1 5 'buf 'third)
              (undo-boundary)
              (goto-char 1) (re-search-forward "THIRD" nil t) (replace-match "ALTERED")
              (let ((s3 (buffer-string)))
                (primitive-undo 1 buffer-undo-list)
                (list s2 s3
                      (with-current-buffer buf2 (buffer-string))
                      (string= (with-current-buffer buf2 (buffer-string)) "SECOND-BUFFER-DATA")
                      (with-current-buffer buf3 (buffer-string))
                      (string= (with-current-buffer buf3 (buffer-string)) "THIRD-BUFFER-HERE")))))))
      (kill-buffer buf2)
      (kill-buffer buf3)
      (list (buffer-string)
            (string= (buffer-string) "MAIN-CONTENT-HERE")
            (= (marker-position m-main) 6)
            (overlay-get ov-main 'origin))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_replace_within_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"XXAAYBBYZCCZ\" 0 2 (zone x) 4 5 (zone y) 8 9 (zone z)) #(\"XXXXYYYYZZZZ\" 0 2 (zone x) 2 3 (zone x) 4 5 (zone y) 5 7 (zone y) 8 9 (zone z) 9 11 (zone z)) t t x t y t z t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "XXXXYYYYZZZZ")
  (put-text-property 1 4 'zone 'x)
  (put-text-property 5 8 'zone 'y)
  (put-text-property 9 12 'zone 'z)
  (let ((m (copy-marker 5 t))
        (ov (make-overlay 1 12)))
    (overlay-put ov 'all t)
    (undo-boundary)
    (goto-char 3)
    (re-search-forward "XX" nil t) (replace-match "AA")
    (undo-boundary)
    (goto-char 6)
    (re-search-forward "YY" nil t) (replace-match "BB")
    (undo-boundary)
    (goto-char 10)
    (re-search-forward "ZZ" nil t) (replace-match "CC")
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "XXXXYYYYZZZZ")
            (= (marker-position m) 5)
            (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'x)
            (get-text-property 5 'zone) (eq (get-text-property 5 'zone) 'y)
            (get-text-property 9 'zone) (eq (get-text-property 9 'zone) 'z)
            (overlay-get ov 'all))))) "#,
        expect,
    );
}
