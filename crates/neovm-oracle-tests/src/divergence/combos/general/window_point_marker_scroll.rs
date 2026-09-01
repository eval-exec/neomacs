//! Divergence tests: window + point + marker + overlay + textprop + scroll sim.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_point_marker_after_large_insert_scroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\" 0 49 (half first) 250 299 (half second)) 1 250 300 first nil #(\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\" 0 49 (half first) 50 99 (half second)) t 1 t 50 t 100 t first t nil t second t whole t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 100 ?A))
  (put-text-property 1 50 'half 'first)
  (put-text-property 51 100 'half 'second)
  (let ((ov (make-overlay 1 100))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 50 t))
        (m3 (copy-marker 100)))
    (overlay-put ov 'section 'whole)
    (undo-boundary)
    (goto-char 50)
    (insert (make-string 200 ?B))
    (let ((s1 (buffer-string))
          (m1p (marker-position m1))
          (m2p (marker-position m2))
          (m3p (marker-position m3))
          (p1 (get-text-property 1 'half))
          (p2 (get-text-property 50 'half)))
      (primitive-undo 1 buffer-undo-list)
      (list s1 m1p m2p m3p p1 p2
            (buffer-string)
            (= (buffer-size) 100)
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 50)
            (marker-position m3) (= (marker-position m3) 100)
            (get-text-property 1 'half) (eq (get-text-property 1 'half) 'first)
            (get-text-property 50 'half) (null (get-text-property 50 'half))
            (get-text-property 51 'half) (eq (get-text-property 51 'half) 'second)
            (overlay-get ov 'section) (eq (overlay-get ov 'section) 'whole))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_recenter_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable wp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 500 ?X))
  (put-text-property 1 100 'block 'a)
  (put-text-property 101 200 'block 'b)
  (put-text-property 201 300 'block 'c)
  (put-text-property 301 400 'block 'd)
  (put-text-property 401 500 'block 'e)
  (let ((ov (make-overlay 101 200))
        (m (copy-marker 101 t)))
    (overlay-put ov 'visible 'yes)
    (goto-char 101)
    (let ((w (selected-window)))
      (set-window-point w 250)
      (let ((wp (window-point w))
            (ov-s (overlay-start ov))
            (ov-e (overlay-end ov))
            (block-at-wp (get-text-property wp 'block)))
        (list (>= wp 1)
              (<= wp 500)
              (memq block-at-wp '(a b c d e))
              ov-s (= ov-s 101)
              ov-e (= ov-e 200)
              (overlay-get ov 'visible) (eq (overlay-get ov 'visible) 'yes)
              (marker-position m) (= (marker-position m) 101)))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_start_end_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t 5 t 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 1000 ?Z))
  (dotimes (i 10)
    (let ((start (+ 1 (* i 100)))
          (end (min (+ 1 (* (+ i 1) 100)) 1001)))
      (put-text-property start end 'chunk (+ i 1))
      (let ((ov (make-overlay start end)))
        (overlay-put ov 'chunk-num (+ i 1)))))
  (let ((ovs (overlays-in 1 1001))
        (c1 (get-text-property 1 'chunk))
        (c5 (get-text-property 401 'chunk))
        (c10 (get-text-property 901 'chunk)))
    (list (>= (length ovs) 10)
          (= c1 1)
          (= c5 5)
          (= c10 10)
          (= (buffer-size) 1000)
          (get-text-property 500 'chunk) (= (get-text-property 500 'chunk) 5)
          (get-text-property 999 'chunk) (= (get-text-property 999 'chunk) 10)))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_to_visible_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"QQWWWWWW-EEE-FFF-GGG-HHH-II\" 9 11 (seg e) 13 15 (seg f) 17 19 (seg g) 21 23 (seg h) 25 27 (seg i)) #(\"AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ-KKK-LLL\" 0 2 (seg a) 4 6 (seg b) 8 10 (seg c) 12 14 (seg d) 16 18 (seg e) 20 22 (seg f) 24 26 (seg g) 28 30 (seg h) 32 34 (seg i) 36 38 (seg j) 40 42 (seg k) 44 46 (seg l)) t 1 t 16 nil 25 t 37 t a t b t c t d t e t f t g t h t i t j t k t l t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ-KKK-LLL")
  (let ((ov1 (make-overlay 1 3)) (ov2 (make-overlay 5 7))
        (ov3 (make-overlay 9 11)) (ov4 (make-overlay 13 15))
        (ov5 (make-overlay 17 19)) (ov6 (make-overlay 21 23))
        (ov7 (make-overlay 25 27)) (ov8 (make-overlay 29 31))
        (ov9 (make-overlay 33 35)) (ov10 (make-overlay 37 39))
        (ov11 (make-overlay 41 43)) (ov12 (make-overlay 45 47)))
    (dolist (ov (list ov1 ov2 ov3 ov4 ov5 ov6 ov7 ov8 ov9 ov10 ov11 ov12))
      (overlay-put ov 'type 'segment))
    (put-text-property 1 3 'seg 'a) (put-text-property 5 7 'seg 'b)
    (put-text-property 9 11 'seg 'c) (put-text-property 13 15 'seg 'd)
    (put-text-property 17 19 'seg 'e) (put-text-property 21 23 'seg 'f)
    (put-text-property 25 27 'seg 'g) (put-text-property 29 31 'seg 'h)
    (put-text-property 33 35 'seg 'i) (put-text-property 37 39 'seg 'j)
    (put-text-property 41 43 'seg 'k) (put-text-property 45 47 'seg 'l)
    (let ((m1 (copy-marker 1 t)) (m2 (copy-marker 13 t))
          (m3 (copy-marker 25 t)) (m4 (copy-marker 37 t)))
      (undo-boundary)
      (narrow-to-region 13 35)
      (goto-char (point-min))
      (insert "QQ")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "DDD" nil t)
      (replace-match "WWWWWW")
      (let ((ns (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (widen)
        (list ns
              (buffer-string)
              (string= (buffer-string)
                       "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ-KKK-LLL")
              (marker-position m1) (= (marker-position m1) 1)
              (marker-position m2) (= (marker-position m2) 13)
              (marker-position m3) (= (marker-position m3) 25)
              (marker-position m4) (= (marker-position m4) 37)
              (get-text-property 1 'seg) (eq (get-text-property 1 'seg) 'a)
              (get-text-property 5 'seg) (eq (get-text-property 5 'seg) 'b)
              (get-text-property 9 'seg) (eq (get-text-property 9 'seg) 'c)
              (get-text-property 13 'seg) (eq (get-text-property 13 'seg) 'd)
              (get-text-property 17 'seg) (eq (get-text-property 17 'seg) 'e)
              (get-text-property 21 'seg) (eq (get-text-property 21 'seg) 'f)
              (get-text-property 25 'seg) (eq (get-text-property 25 'seg) 'g)
              (get-text-property 29 'seg) (eq (get-text-property 29 'seg) 'h)
              (get-text-property 33 'seg) (eq (get-text-property 33 'seg) 'i)
              (get-text-property 37 'seg) (eq (get-text-property 37 'seg) 'j)
              (get-text-property 41 'seg) (eq (get-text-property 41 'seg) 'k)
              (get-text-property 45 'seg) (eq (get-text-property 45 'seg) 'l)))))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_insertion_type_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (8 4 t nil t t t t nil t 4 t 4 t #(\"ABCDEFGH\" 0 3 (part left) 4 7 (part right)) t left t right t boundary t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (put-text-property 1 4 'part 'left)
  (put-text-property 5 8 'part 'right)
  (let ((m-advance (copy-marker 4 t))
        (m-stay (copy-marker 4 nil))
        (ov (make-overlay 4 4)))
    (overlay-put ov 'point 'boundary)
    (undo-boundary)
    (goto-char 4)
    (insert "XXXX")
    (let ((ma-pos (marker-position m-advance))
          (ms-pos (marker-position m-stay))
          (ma-type (marker-insertion-type m-advance))
          (ms-type (marker-insertion-type m-stay)))
      (primitive-undo 1 buffer-undo-list)
      (list ma-pos ms-pos ma-type ms-type
            (= ma-pos 8) (= ms-pos 4)
            ma-type (eq ma-type t)
            ms-type (null ms-type)
            (marker-position m-advance) (= (marker-position m-advance) 4)
            (marker-position m-stay) (= (marker-position m-stay) 4)
            (buffer-string) (string= (buffer-string) "ABCDEFGH")
            (get-text-property 1 'part) (eq (get-text-property 1 'part) 'left)
            (get-text-property 5 'part) (eq (get-text-property 5 'part) 'right)
            (overlay-get ov 'point) (eq (overlay-get ov 'point) 'boundary))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_chain_move_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"111-PPPPREPLACED-333-REPLACED-555-REPLACED-777\" 0 2 (val 1) 17 19 (val 3) 30 32 (val 5) 43 45 (val 7)) #(\"111-222-333-444-555-666-777\" 0 2 (val 1) 4 6 (val 2) 8 10 (val 3) 12 14 (val 4) 16 18 (val 5) 20 22 (val 6) 24 26 (val 7)) t 8 nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "111-222-333-444-555-666-777")
  (let ((ovs (list (make-overlay 1 3) (make-overlay 5 7) (make-overlay 9 11)
                   (make-overlay 13 15) (make-overlay 17 19) (make-overlay 21 23)
                   (make-overlay 25 27))))
    (dotimes (i 7)
      (overlay-put (nth i ovs) 'idx (+ i 1)))
    (put-text-property 1 3 'val 1) (put-text-property 5 7 'val 2)
    (put-text-property 9 11 'val 3) (put-text-property 13 15 'val 4)
    (put-text-property 17 19 'val 5) (put-text-property 21 23 'val 6)
    (put-text-property 25 27 'val 7)
    (let ((m (copy-marker 5 t)))
      (undo-boundary)
      (goto-char 5)
      (insert "PPPP")
      (undo-boundary)
      (goto-char 1)
      (while (re-search-forward "222\\|444\\|666" nil t)
        (replace-match "REPLACED"))
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "111-222-333-444-555-666-777")
              (marker-position m) (= (marker-position m) 5)
              (dotimes (i 7 t)
                (and (= (overlay-get (nth i ovs) 'idx) (+ i 1))
                     (= (get-text-property (+ 1 (* i 4)) 'val) (+ i 1))))))))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_boundary_after_kill_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAXBBBBCCCC\" 0 2 (zone a) 4 7 (zone b) 8 11 (zone c)) a nil nil #(\"AAAXBBBXXXBCCCC\" 0 2 (zone a) 4 7 (zone b) 7 9 (zone x) 9 10 (rear-nonsticky t zone x) 11 14 (zone c)) #(\"AAAXXXXBBBBCCCC\" 0 2 (zone a) 3 6 (zone x) 7 10 (zone b) 11 14 (zone c)) t a t x t b t c t 4 t all t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAXXXXBBBBCCCC")
  (put-text-property 1 3 'zone 'a)
  (put-text-property 4 7 'zone 'x)
  (put-text-property 8 11 'zone 'b)
  (put-text-property 12 15 'zone 'c)
  (let ((ov (make-overlay 1 15))
        (m (copy-marker 4 t)))
    (overlay-put ov 'scope 'all)
    (undo-boundary)
    (kill-region 4 7)
    (let ((s1 (buffer-string))
          (p-a (get-text-property 1 'zone))
          (p-b (get-text-property 4 'zone))
          (p-c (get-text-property 8 'zone)))
      (undo-boundary)
      (goto-char 8)
      (yank)
      (let ((s2 (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s1 p-a p-b p-c s2
              (buffer-string)
              (string= (buffer-string) "AAAXXXXBBBBCCCC")
              (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'a)
              (get-text-property 4 'zone) (eq (get-text-property 4 'zone) 'x)
              (get-text-property 8 'zone) (eq (get-text-property 8 'zone) 'b)
              (get-text-property 12 'zone) (eq (get-text-property 12 'zone) 'c)
              (marker-position m) (= (marker-position m) 4)
              (overlay-get ov 'scope) (eq (overlay-get ov 'scope) 'all)))))) "#,
        expect,
    );
}

#[test]
fn divergence_100_overlay_sweep_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 500 ?T))
  (let ((ovs nil)
        (mids nil))
    (dotimes (i 100)
      (let* ((start (+ 1 (* i 5)))
             (end (min (+ start 4) 501))
             (ov (make-overlay start end)))
        (overlay-put ov 'idx (+ i 1))
        (push ov ovs)
        (put-text-property start end 'idx (+ i 1))
        (push (copy-marker start t) mids)))
    (setq ovs (nreverse ovs))
    (setq mids (nreverse mids))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "TTTTT" nil t)
      (replace-match "ABCDE"))
    (let ((s (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (let ((all-ok t))
        (dotimes (i 100)
          (let ((ov (nth i ovs)))
            (when (or (not (= (overlay-get ov 'idx) (+ i 1)))
                      (not (= (get-text-property (+ 1 (* i 5)) 'idx) (+ i 1))))
              (setq all-ok nil))))
        (list (string= (buffer-string) (make-string 500 ?T))
              all-ok
              (= (buffer-size) 500)
              (= (length ovs) 100)
              (= (length mids) 100)))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_face_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"NORMAL-XXHEAVY-ITALIC-UNDERLINE-NORMAL\" 0 5 (style plain) 15 20 (style italic) 22 30 (style underline) 32 37 (style plain)) #(\"NORMAL-BOLD-ITALIC-UNDERLINE-NORMAL\" 0 5 (style plain) 7 10 (style bold) 12 17 (style italic) 19 27 (style underline) 29 34 (style plain)) t 12 nil 13 t 20 t bold t italic t underline t plain t bold t italic t underline t plain t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "NORMAL-BOLD-ITALIC-UNDERLINE-NORMAL")
  (let ((ov-bold (make-overlay 8 11))
        (ov-italic (make-overlay 13 18))
        (ov-under (make-overlay 20 28))
        (m1 (copy-marker 8 t))
        (m2 (copy-marker 13 t))
        (m3 (copy-marker 20 t)))
    (overlay-put ov-bold 'face 'bold)
    (overlay-put ov-italic 'face 'italic)
    (overlay-put ov-under 'face 'underline)
    (put-text-property 1 6 'style 'plain)
    (put-text-property 8 11 'style 'bold)
    (put-text-property 13 18 'style 'italic)
    (put-text-property 20 28 'style 'underline)
    (put-text-property 30 35 'style 'plain)
    (undo-boundary)
    (goto-char 8)
    (insert "XX")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "BOLD" nil t)
    (replace-match "HEAVY")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "NORMAL-BOLD-ITALIC-UNDERLINE-NORMAL")
            (marker-position m1) (= (marker-position m1) 8)
            (marker-position m2) (= (marker-position m2) 13)
            (marker-position m3) (= (marker-position m3) 20)
            (overlay-get ov-bold 'face) (eq (overlay-get ov-bold 'face) 'bold)
            (overlay-get ov-italic 'face) (eq (overlay-get ov-italic 'face) 'italic)
            (overlay-get ov-under 'face) (eq (overlay-get ov-under 'face) 'underline)
            (get-text-property 1 'style) (eq (get-text-property 1 'style) 'plain)
            (get-text-property 8 'style) (eq (get-text-property 8 'style) 'bold)
            (get-text-property 13 'style) (eq (get-text-property 13 'style) 'italic)
            (get-text-property 20 'style) (eq (get-text-property 20 'style) 'underline)
            (get-text-property 30 'style) (eq (get-text-property 30 'style) 'plain))))) "#,
        expect,
    );
}

#[test]
fn divergence_region_active_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"XXXX-C-DDDD-EEEE\" 7 10 (zone d) 12 15 (zone e)) #(\"AAAA-BBBB-CCCC-DDDD-EEEE\" 0 3 (zone a) 5 8 (zone b) 10 13 (zone c) 15 18 (zone d) 20 23 (zone e)) t 6 t selected t 6 t 6 nil a t b t c t d t e t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (put-text-property 1 4 'zone 'a)
  (put-text-property 6 9 'zone 'b)
  (put-text-property 11 14 'zone 'c)
  (put-text-property 16 19 'zone 'd)
  (put-text-property 21 24 'zone 'e)
  (let ((ov (make-overlay 6 14))
        (m (copy-marker 6 t)))
    (overlay-put ov 'region 'selected)
    (undo-boundary)
    (delete-region 6 14)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "AAAA" nil t)
    (replace-match "XXXX")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "AAAA-BBBB-CCCC-DDDD-EEEE")
            (marker-position m) (= (marker-position m) 6)
            (overlay-get ov 'region) (eq (overlay-get ov 'region) 'selected)
            (overlay-start ov) (= (overlay-start ov) 6)
            (overlay-end ov) (= (overlay-end ov) 14)
            (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'a)
            (get-text-property 6 'zone) (eq (get-text-property 6 'zone) 'b)
            (get-text-property 11 'zone) (eq (get-text-property 11 'zone) 'c)
            (get-text-property 16 'zone) (eq (get-text-property 16 'zone) 'd)
            (get-text-property 21 'zone) (eq (get-text-property 21 'zone) 'e))))) "#,
        expect,
    );
}
