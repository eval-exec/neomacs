//! Divergence tests: simulated editing session + undo + markers + overlays.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_simulated_code_edit_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"fn main() {\\n    println!(\\\"world\\\");\\n}\\n\" 0 7 (syntax function) 24 26 (syntax string)) #(\"fn main()// comment\\n     {\\n    println!(\\\"world\\\");\\n}\\n\" 0 7 (syntax function) 39 41 (syntax string)) 25 32 40 47 35 #(\"fn main() {\\n    println!(\\\"world\\\");\\n}\\n\" 0 7 (syntax function) 24 26 (syntax string)) function t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "fn main() {\n    println!(\"hello\");\n}\n")
  (let ((ov-fn (make-overlay 1 9))
        (ov-str (make-overlay 25 32))
        (m-line2 (copy-marker 20 t)))
    (overlay-put ov-fn 'face 'font-lock-function-name-face)
    (overlay-put ov-str 'face 'font-lock-string-face)
    (put-text-property 1 8 'syntax 'function)
    (put-text-property 25 31 'syntax 'string)
    (undo-boundary)
    (goto-char 25)
    (re-search-forward "hello" nil t)
    (replace-match "world")
    (let ((s1 (buffer-string))
          (str-start (overlay-start ov-str))
          (str-end (overlay-end ov-str)))
      (undo-boundary)
      (goto-char 10)
      (insert "// comment\n    ")
      (let ((s2 (buffer-string))
            (str-start2 (overlay-start ov-str))
            (str-end2 (overlay-end ov-str))
            (m-pos (marker-position m-line2)))
        (primitive-undo 1 buffer-undo-list)
        (list s1 s2
              str-start str-end
              str-start2 str-end2
              m-pos
              (buffer-string)
              (get-text-property 1 'syntax)
              (eq (get-text-property 1 'syntax) 'function)))))) "#,
        expect,
    );
}

#[test]
fn divergence_multi_region_edit_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"REGION1-XXAAAA REGION2-BBBBYY REGION3-CCCC\" 0 8 (zone 1) 10 13 (zone 1) 14 25 (zone 2) 26 27 (zone 3) 29 39 (zone 3)) 1 2 3 t t t #(\"REGION1-XXAAAA REGION2-BBBB REGION3-CCCC\" 0 8 (zone 1) 10 13 (zone 1) 14 25 (zone 2) 26 27 (zone 3) 27 37 (zone 3)) nil 1 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "REGION1-AAAA REGION2-BBBB REGION3-CCCC")
  (put-text-property 1 12 'zone 1)
  (put-text-property 13 24 'zone 2)
  (put-text-property 25 36 'zone 3)
  (let ((ov1 (make-overlay 1 12))
        (ov2 (make-overlay 13 24))
        (ov3 (make-overlay 25 36)))
    (overlay-put ov1 'priority 1)
    (overlay-put ov2 'priority 2)
    (overlay-put ov3 'priority 3)
    (undo-boundary)
    (goto-char 9)
    (insert "XX")
    (undo-boundary)
    (goto-char 28)
    (insert "YY")
    (let ((s (buffer-string))
          (z1 (get-text-property 1 'zone))
          (z2 (get-text-property 15 'zone))
          (z3 (get-text-property 30 'zone)))
      (primitive-undo 1 buffer-undo-list)
      (primitive-undo 1 buffer-undo-list)
      (list s z1 z2 z3
            (= z1 1) (= z2 2) (= z3 3)
            (buffer-string)
            (string= (buffer-string) "REGION1-AAAA REGION2-BBBB REGION3-CCCC")
            (overlay-get ov1 'priority)
            (= (overlay-get ov1 'priority) 1))))) "#,
        expect,
    );
}

#[test]
fn divergence_ediff_style_region_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 61 75)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1 common\nline2 only-A\nline3 common\nline4 only-A\nline5 common\n")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 31 t))
        (m3 (copy-marker 59 t)))
    (put-text-property 1 15 'source 'both)
    (put-text-property 16 30 'source 'a-only)
    (put-text-property 31 45 'source 'both)
    (put-text-property 46 60 'source 'a-only)
    (put-text-property 61 75 'source 'both)
    (let ((ov-diff1 (make-overlay 16 30))
          (ov-diff2 (make-overlay 46 60)))
      (overlay-put ov-diff1 'face 'diff-refine-removed)
      (overlay-put ov-diff2 'face 'diff-refine-removed)
      (undo-boundary)
      (delete-region 16 30)
      (delete-region 31 45)
      (let ((s1 (buffer-string))
            (diff-count (length (overlays-in 1 (point-max)))))
        (primitive-undo 2 buffer-undo-list)
        (list s1
              (buffer-string)
              (string= (buffer-string)
                       "line1 common\nline2 only-A\nline3 common\nline4 only-A\nline5 common\n")
              diff-count
              (marker-position m1)
              (marker-position m2)
              (marker-position m3)))))) "#,
        expect,
    );
}

#[test]
fn divergence_refactor_rename_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "var foo = 1;\nvar foo = 2;\nprint(foo);\nfoo = 3;\n")
  (let ((refs nil))
    (goto-char 1)
    (while (re-search-forward "\\<foo\\>" nil t)
      (push (copy-marker (match-beginning 0)) refs))
    (setq refs (nreverse refs))
    (let ((initial-positions (mapcar 'marker-position refs)))
      (undo-boundary)
      (goto-char 1)
      (while (re-search-forward "\\<foo\\>" nil t)
        (replace-match "bar"))
      (let ((after-positions (mapcar 'marker-position refs))
            (s1 (buffer-string)))
        (list initial-positions
              after-positions
              s1
              (string-match "bar" s1)
              (null (string-match "\\<foo\\>" s1))
              (= (length refs) 4)
              (every (lambda (p) p) initial-positions)))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_chain_delete_reinsert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"AAAA--EEEE-FFFF-GGGG-HHHH\" \"AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH\" t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
  (let ((ovs nil))
    (dotimes (i 8)
      (let ((start (+ 1 (* i 5)))
            (end (+ 4 (* i 5))))
        (let ((ov (make-overlay start end)))
          (overlay-put ov 'idx i)
          (push ov ovs))))
    (setq ovs (nreverse ovs))
    (undo-boundary)
    (delete-region 6 20)
    (let ((s1 (buffer-string))
          (ov-pos (mapcar (lambda (ov)
                            (list (overlay-start ov) (overlay-end ov)))
                          (delq nil (mapcar
                                     (lambda (ov)
                                       (when (overlay-start ov) ov))
                                     ovs)))))
      (primitive-undo 1 buffer-undo-list)
      (list s1
            (buffer-string)
            (string= (buffer-string)
                     "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
            (= (length ov-pos) 6)
            (= (buffer-size) 39))))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_narrow_widen_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (13 42 18 30 #(\"XXE-START INNE\" 2 9 (level middle) 10 14 (level inner)) 22 20 45 outer t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "OUTER-START MIDDLE-START INNER-END MIDDLE-END OUTER-END")
  (let ((m-inner (copy-marker 20 t))
        (m-outer (copy-marker 45)))
    (put-text-property 1 12 'level 'outer)
    (put-text-property 13 25 'level 'middle)
    (put-text-property 26 35 'level 'inner)
    (narrow-to-region 13 42)
    (let ((min1 (point-min)) (max1 (point-max)))
      (narrow-to-region 18 30)
      (let ((min2 (point-min)) (max2 (point-max)))
        (undo-boundary)
        (goto-char (point-min))
        (insert "XX")
        (let ((s (buffer-string))
              (mi (marker-position m-inner)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (widen)
          (list min1 max1 min2 max2 s mi
                (marker-position m-inner)
                (marker-position m-outer)
                (get-text-property 1 'level)
                (eq (get-text-property 1 'level) 'outer))))))) "#,
        expect,
    );
}

#[test]
fn divergence_comment_uncomment_region_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r#"(progn
  (insert "code1\ncode2\ncode3\n")
  (put-text-property 1 6 'type 'code)
  (put-text-property 7 12 'type 'code)
  (put-text-property 13 18 'type 'code)
  (let ((ov (make-overlay 1 18)))
    (overlay-put ov 'tag 'region)
    (condition-case nil
        (comment-region 1 18)
      (error nil))
    (let ((commented (buffer-string)))
      (condition-case nil
          (uncomment-region 1 (point-max))
        (error nil))
      (list commented
            (buffer-string)
            (string-match "code1" (buffer-string))
            (>= (length (buffer-string)) 14)
            (overlay-get ov 'tag)
            (eq (overlay-get ov 'tag) 'region)
            (get-text-property 1 'type)
            (eq (get-text-property 1 'type) 'code))))) "#,
    );
}

#[test]
fn divergence_kill_yank_ring_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"AAAAB-CCCC-DDDD-EEE-BBBE\" \"AAAA-BBBB-CCCC-DDDD-EEE-BBBE\" 1 6 \"\" nil 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 10 t)))
    (undo-boundary)
    (goto-char 5)
    (kill-region 5 9)
    (goto-char 20)
    (yank)
    (let ((s1 (buffer-string))
          (p1 (marker-position m1))
          (p2 (marker-position m2)))
      (undo-boundary)
      (goto-char 5)
      (yank)
      (let ((s2 (buffer-string)))
        (primitive-undo 3 buffer-undo-list)
        (list s1 s2 p1 p2
              (buffer-string)
              (string= (buffer-string) "AAAA-BBBB-CCCC-DDDD-EEEE")
              (marker-position m1)
              (marker-position m2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_text_property_search_replace_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 50 55)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "keep ALPHA replace BETA keep GAMMA replace DELTA keep")
  (put-text-property 6 11 'action 'keep)
  (put-text-property 12 26 'action 'replace)
  (put-text-property 27 32 'action 'keep)
  (put-text-property 33 49 'action 'replace)
  (put-text-property 50 55 'action 'keep)
  (undo-boundary)
  (goto-char 1)
  (while (re-search-forward "replace \\(BETA\\|DELTA\\)" nil t)
    (when (eq (get-text-property (match-beginning 0) 'action) 'replace)
      (replace-match "KEEP")))
  (list (buffer-string)
        (get-text-property 6 'action)
        (eq (get-text-property 6 'action) 'keep)
        (get-text-property 12 'action)
        (eq (get-text-property 12 'action) 'replace)
        (get-text-property 50 'action)
        (eq (get-text-property 50 'action) 'keep)
        (= (buffer-size) 55))) "#,
        expect,
    );
}

#[test]
fn divergence_revert_buffer_with_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 20 42)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ORIGINAL-CONTENT-HERE")
  (let ((ov (make-overlay 1 22))
        (m (copy-marker 10 t)))
    (overlay-put ov 'state 'loaded)
    (put-text-property 1 8 'section 'header)
    (put-text-property 9 16 'section 'body)
    (put-text-property 17 22 'section 'footer)
    (let ((before (list (buffer-string)
                        (overlay-get ov 'state)
                        (get-text-property 1 'section)
                        (marker-position m))))
      (erase-buffer)
      (insert "NEW-CONTENT")
      (list before
            (buffer-string)
            (= (buffer-size) 11)
            (marker-position m)
            (overlay-start ov)
            (null (overlay-start ov))))) #"#,
        expect,
    );
}
