//! Divergence tests: stress — 1000 overlay + regex + undo + marker combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_mass_overlay_insert_undo_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil t \"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\" nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 100 ?A))
  (let ((ovs nil)
        (mks nil))
    (dotimes (i 20)
      (let ((ov (make-overlay (+ 1 (* i 5)) (+ 4 (* i 5)))))
        (overlay-put ov 'idx i)
        (push ov ovs))
      (push (copy-marker (+ 2 (* i 5))) mks))
    (undo-boundary)
    (goto-char 50)
    (insert "XXXX")
    (let ((marker-positions (mapcar 'marker-position mks))
          (ov-positions (mapcar (lambda (ov)
                                  (list (overlay-start ov) (overlay-end ov)))
                                (nreverse ovs))))
      (primitive-undo 1 buffer-undo-list)
      (let ((marker-after (mapcar 'marker-position mks))
            (ov-after (mapcar (lambda (ov)
                                (list (overlay-start ov) (overlay-end ov)))
                              (nreverse ovs))))
        (list (= (length ovs) 20)
              (= (length mks) 20)
              (buffer-string)
              (equal marker-positions marker-after)
              (equal ov-positions ov-after)))))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_replace_preserves_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aa-bb-cc-dd-ee-ff-gg-hh-ii-jj")
  (let ((ovs nil))
    (dotimes (i 10)
      (let ((start (+ 1 (* i 3)))
            (end (+ 2 (* i 3))))
        (when (<= end (point-max))
          (let ((ov (make-overlay start end)))
            (overlay-put ov 'idx i)
            (push ov ovs)))))
    (goto-char 1)
    (undo-boundary)
    (while (re-search-forward "[a-z][a-z]" nil t)
      (replace-match "XX"))
    (let ((ov-props (mapcar (lambda (ov)
                              (list (overlay-get ov 'idx)
                                    (overlay-start ov)
                                    (overlay-end ov)))
                            (nreverse ovs))))
      (list (buffer-string)
            (length ov-props)
            (= (length ov-props) 10)
            (every (lambda (p) (numberp (cadr p))) ov-props))))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_ring_regex_jump() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LINE1\nLINE2\nLINE3\nLINE4\nLINE5\n")
  (let ((ring nil))
    (dotimes (i 5)
      (push (copy-marker (+ 1 (* i 6))) ring))
    (setq ring (nreverse ring))
    (goto-char 1)
    (let ((matches nil))
      (while (re-search-forward "LINE[0-9]" nil t)
        (push (list (match-string 0)
                    (marker-position (nth (1- (string-to-number
                                                 (substring (match-string 0) 4)))
                                           ring)))
              matches))
      (list (= (length matches) 5)
            (nreverse matches)
            (buffer-string)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_priority_sort_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t (0 1 2 3 4) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (let ((ovs nil))
    (dotimes (i 5)
      (let ((ov (make-overlay (+ 1 (* i 5)) (+ 5 (* i 5)))))
        (overlay-put ov 'priority (* (1+ i) 10))
        (overlay-put ov 'idx i)
        (push ov ovs)))
    (setq ovs (sort ovs
                    (lambda (a b)
                      (< (or (overlay-get a 'priority) 0)
                         (or (overlay-get b 'priority) 0)))))
    (goto-char 1)
    (re-search-forward "CDE" nil t)
    (let ((at-point (overlays-at (match-beginning 0)))
          (in-range (overlays-in 1 26)))
      (list (= (length at-point) 1)
            (<= (length in-range) 5)
            (mapcar (lambda (ov) (overlay-get ov 'idx)) ovs)
            (= (buffer-size) 26))))) "#,
        expect,
    );
}

#[test]
fn divergence_500_overlays_delete_region_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 500 ?X))
  (let ((ovs nil))
    (dotimes (i 50)
      (let ((start (+ 1 (* i 10)))
            (end (+ 5 (* i 10))))
        (when (<= end 500)
          (let ((ov (make-overlay start end)))
            (overlay-put ov 'idx i)
            (push ov ovs)))))
    (undo-boundary)
    (delete-region 100 200)
    (let ((buf-size (buffer-size))
          (ov-count (length (overlays-in 1 (point-max)))))
      (primitive-undo 1 buffer-undo-list)
      (list buf-size
            (= buf-size 400)
            ov-count
            (= (buffer-size) 500)
            (every (lambda (ov) (numberp (overlay-start ov)))
                   (nreverse ovs)))))) "#,
        expect,
    );
}

#[test]
fn divergence_replace_match_preserves_marker_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 3 5 7 9 \"X X X X X\") \"\" 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha bravo charlie delta echo")
  (let ((m1 (copy-marker 1))
        (m2 (copy-marker 7))
        (m3 (copy-marker 13))
        (m4 (copy-marker 21))
        (m5 (copy-marker 27)))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "[a-z]+" nil t)
      (replace-match "X"))
    (let ((result (list (marker-position m1)
                        (marker-position m2)
                        (marker-position m3)
                        (marker-position m4)
                        (marker-position m5)
                        (buffer-string))))
      (primitive-undo 5 buffer-undo-list)
      (list result
            (buffer-string)
            (marker-position m1)
            (marker-position m5))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_evaporate_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"---DDDD-EEEE\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD-EEEE")
  (let ((ov1 (make-overlay 1 3))
        (ov2 (make-overlay 5 9))
        (ov3 (make-overlay 10 14)))
    (overlay-put ov1 'evaporate t)
    (overlay-put ov2 'evaporate t)
    (overlay-put ov3 'evaporate t)
    (overlay-put ov1 'idx 1)
    (overlay-put ov2 'idx 2)
    (overlay-put ov3 'idx 3)
    (goto-char 1)
    (undo-boundary)
    (while (re-search-forward "AAA\\|BBBB\\|CCCC" nil t)
      (replace-match ""))
    (let ((remaining (delq nil (mapcar (lambda (ov)
                                         (when (overlay-start ov)
                                           (overlay-get ov 'idx)))
                                       (list ov1 ov2 ov3)))))
      (list (buffer-string)
            remaining
            (= (length remaining) 1))))) "#,
        expect,
    );
}

#[test]
fn divergence_mass_marker_insert_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil \"ABBAAA\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA")
  (let ((m-insert (copy-marker 2 t))
        (m-noinst (copy-marker 3 nil)))
    (let ((p1 (marker-position m-insert))
          (p2 (marker-position m-noinst)))
      (goto-char 2)
      (insert "BB")
      (let ((p3 (marker-position m-insert))
            (p4 (marker-position m-noinst)))
        (list (= p1 2) (= p2 3)
              (= p3 4) (= p4 3)
              (buffer-string)
              (= (buffer-size) 6)))))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_face_overlay_face_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (italic t italic t bold t #(\"ABCDEFGHIJ\" 2 6 (face italic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (put-text-property 3 7 'face 'italic)
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'face 'bold)
    (let ((faces-at-4 (get-text-property 4 'face))
          (faces-at-6 (get-text-property 6 'face))
          (ov-faces (overlay-get ov 'face)))
      (list faces-at-4
            (eq faces-at-4 'italic)
            faces-at-6
            (eq faces-at-6 'italic)
            ov-faces
            (eq ov-faces 'bold)
            (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_after_multiple_overlays_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"STXXARYYT\" \"STXXART\" \"STXXART\" (1 10) (1 8) (1 8) main)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "START")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'tag 'main)
    (undo-boundary)
    (goto-char 3)
    (insert "XX")
    (undo-boundary)
    (goto-char 7)
    (insert "YY")
    (let ((s1 (buffer-string))
          (ov1 (list (overlay-start ov) (overlay-end ov))))
      (primitive-undo 1 buffer-undo-list)
      (let ((s2 (buffer-string))
            (ov2 (list (overlay-start ov) (overlay-end ov))))
        (primitive-undo 1 buffer-undo-list)
        (list s1 s2 (buffer-string)
              ov1 ov2
              (list (overlay-start ov) (overlay-end ov))
              (overlay-get ov 'tag)))))) "#,
        expect,
    );
}
