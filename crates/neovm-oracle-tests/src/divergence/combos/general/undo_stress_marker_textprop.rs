//! Divergence tests: undo stress + marker + textprop + overlay deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_chain_50_edits_markers_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (60 t t 5 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 50 ?A))
  (let ((ovs nil) (mks nil))
    (dotimes (i 5)
      (let ((s (+ 1 (* i 10))) (e (+ 5 (* i 10))))
        (push (make-overlay s e) ovs)
        (push (copy-marker s t) mks)
        (put-text-property s e 'idx i)))
    (dotimes (i 10)
      (undo-boundary)
      (goto-char (+ 1 (% (* i 7) 40)))
      (insert "X"))
    (let ((buf-sz (buffer-size))
          (ov-cnt (length (overlays-in 1 (point-max)))))
      (condition-case nil
          (dotimes (_ 10) (primitive-undo 1 buffer-undo-list))
        (error nil))
      (list buf-sz
            (= buf-sz 60)
            (<= (buffer-size) 60)
            ov-cnt (>= ov-cnt 3)
            (get-text-property 1 'idx)
            (= (or (get-text-property 1 'idx) 0) 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_after_textprop_overlay_rearrange() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (let ((ov1 (make-overlay 1 3))
        (ov2 (make-overlay 5 9))
        (ov3 (make-overlay 10 14)))
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (overlay-put ov3 'face 'underline)
    (put-text-property 1 30 'group 'original)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "AAA\\|CCCC" nil t)
      (replace-match "XXX"))
    (let ((s1 (buffer-string))
          (o1s (overlay-start ov1)) (o1e (overlay-end ov1))
          (o3s (overlay-start ov3)) (o3e (overlay-end ov3)))
      (primitive-undo 2 buffer-undo-list)
      (list s1
            (buffer-string)
            (string= (buffer-string) "AAA-BBBB-CCCC-DDDD-EEEE-FFFF")
            o1s o1e o3s o3e
            (overlay-start ov1) (overlay-end ov1)
            (overlay-start ov3) (overlay-end ov3)
            (overlay-get ov1 'face)
            (get-text-property 1 'group))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_narrow_marker_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (7 17 #(\"ZZ-BBBB-CCCC-DDDD-EEEE\" 2 7 (tag first) 8 17 (tag second)) 5 15 #(\"AAAA-BBBB-CCCC-DDDD-EEEE-FFFF\" 0 4 (tag first) 4 9 (tag first) 10 19 (tag second)) first t second t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (let ((m1 (copy-marker 5 t))
        (m2 (copy-marker 15 nil)))
    (put-text-property 1 10 'tag 'first)
    (put-text-property 11 20 'tag 'second)
    (narrow-to-region 5 25)
    (undo-boundary)
    (goto-char (point-min))
    (insert "ZZ")
    (let ((n1 (marker-position m1))
          (n2 (marker-position m2))
          (buf-str (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (widen)
      (list n1 n2 buf-str
            (marker-position m1)
            (marker-position m2)
            (buffer-string)
            (get-text-property 1 'tag)
            (eq (get-text-property 1 'tag) 'first)
            (get-text-property 15 'tag)
            (eq (get-text-property 15 'tag) 'second))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_overlay_evaporate_and_reinsert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((nil t \"ABGHIJ\" t) #(\"ABCDEFGHIJ\" 2 6 (color blue)) t blue t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'data 'important)
    (put-text-property 3 7 'color 'blue)
    (undo-boundary)
    (delete-region 3 7)
    (let ((before-undo (list (overlay-start ov)
                             (null (overlay-start ov))
                             (buffer-string)
                             (= (buffer-size) 6))))
      (undo-boundary)
      (insert "XXXX")
      (primitive-undo 2 buffer-undo-list)
      (list before-undo
            (buffer-string)
            (= (buffer-size) 10)
            (get-text-property 3 'color)
            (eq (get-text-property 3 'color) 'blue))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_after_kill_rectangle_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1-AAA\nline2-BBB\nline3-CCC\nline4-DDD\n")
  (let ((m1 (copy-marker 1))
        (m2 (copy-marker 25)))
    (put-text-property 1 10 'line 1)
    (put-text-property 12 21 'line 2)
    (undo-boundary)
    (goto-char 7)
    (delete-region 7 10)
    (goto-char 16)
    (delete-region 16 19)
    (goto-char 24)
    (delete-region 24 27)
    (let ((s1 (buffer-string))
          (mp1 (marker-position m1))
          (mp2 (marker-position m2)))
      (primitive-undo 3 buffer-undo-list)
      (list s1 mp1 mp2
            (buffer-string)
            (string= (buffer-string) "line1-AAA\nline2-BBB\nline3-CCC\nline4-DDD\n")
            (marker-position m1)
            (marker-position m2)
            (get-text-property 1 'line)
            (= (get-text-property 1 'line) 1))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_preserves_overlay_priority_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((1 2 3) (1 2 3) t \"AAAA-BBBB-CCCC-DDDD-EEEE\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((ov1 (make-overlay 1 25))
        (ov2 (make-overlay 5 20))
        (ov3 (make-overlay 10 15)))
    (overlay-put ov1 'priority 1)
    (overlay-put ov2 'priority 2)
    (overlay-put ov3 'priority 3)
    (overlay-put ov1 'tag 'outer)
    (overlay-put ov2 'tag 'middle)
    (overlay-put ov3 'tag 'inner)
    (undo-boundary)
    (goto-char 10)
    (insert "XXXX")
    (let ((sorted-before (sort (overlays-at 12)
                               (lambda (a b)
                                 (< (overlay-get a 'priority)
                                    (overlay-get b 'priority)))))
          (priorities-before (mapcar (lambda (ov) (overlay-get ov 'priority))
                                     (overlays-at 12))))
      (primitive-undo 1 buffer-undo-list)
      (let ((sorted-after (sort (overlays-at 12)
                                (lambda (a b)
                                  (< (overlay-get a 'priority)
                                     (overlay-get b 'priority)))))
            (priorities-after (mapcar (lambda (ov) (overlay-get ov 'priority))
                                      (overlays-at 12))))
        (list priorities-before
              priorities-after
              (equal priorities-before priorities-after)
              (buffer-string)
              (= (buffer-size) 25)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_insert_behind_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (13 6 #(\"STARTMIDDLE+-END\" 0 4 (part start) 12 15 (part end)) 6 6 #(\"START-END\" 0 4 (part start) 5 8 (part end)) t t start t end t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "START-END")
  (let ((m-insert (copy-marker 6 t))
        (m-noinst (copy-marker 6 nil)))
    (put-text-property 1 5 'part 'start)
    (put-text-property 6 9 'part 'end)
    (undo-boundary)
    (goto-char 6)
    (insert "MIDDLE+")
    (let ((mi-pos (marker-position m-insert))
          (mn-pos (marker-position m-noinst))
          (buf (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list mi-pos mn-pos buf
            (marker-position m-insert)
            (marker-position m-noinst)
            (buffer-string)
            (= (marker-position m-insert) 6)
            (= (marker-position m-noinst) 6)
            (get-text-property 1 'part)
            (eq (get-text-property 1 'part) 'start)
            (get-text-property 6 'part)
            (eq (get-text-property 6 'part) 'end))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_textprop_intervention() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAAXXXX-BBBB-CCCC-DDDD\" 0 3 (type override) 3 4 (type override) 8 12 (type override) 12 13 (type override) 13 17 (type override) 17 20 (type override)) override nil #(\"AAAA-BBBB-CCCC-DDDD\" 0 3 (type override) 3 4 (type override) 4 8 (type override) 8 9 (type override) 9 13 (type override) 13 16 (type override)) override override #(\"AAAA-BBBB-CCCC-DDDD\" 0 3 (type override) 3 4 (type override) 4 8 (type override) 8 9 (type override) 9 13 (type override) 13 16 (type override)) override nil override nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'face 'bold)
    (put-text-property 1 4 'type 'alpha)
    (put-text-property 5 9 'type 'beta)
    (put-text-property 10 14 'type 'gamma)
    (undo-boundary)
    (put-text-property 1 17 'type 'override)
    (undo-boundary)
    (goto-char 5)
    (insert "XXXX")
    (let ((s1 (buffer-string))
          (t1 (get-text-property 1 'type))
          (t5 (get-text-property 5 'type)))
      (primitive-undo 1 buffer-undo-list)
      (let ((s2 (buffer-string))
            (t1b (get-text-property 1 'type))
            (t5b (get-text-property 5 'type)))
        (primitive-undo 1 buffer-undo-list)
        (list s1 t1 t5 s2 t1b t5b
              (buffer-string)
              (get-text-property 1 'type)
              (eq (get-text-property 1 'type) 'alpha)
              (get-text-property 5 'type)
              (eq (get-text-property 5 'type) 'beta)))))) "#,
        expect,
    );
}

#[test]
fn divergence_multiple_undo_boundaries_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ORIGINAL")
  (let ((ov (make-overlay 1 8))
        (m (copy-marker 4 t)))
    (overlay-put ov 'val 'initial)
    (put-text-property 1 8 'status 'clean)
    (undo-boundary)
    (goto-char 4) (insert "AA")
    (undo-boundary)
    (goto-char (point-max)) (insert "BB")
    (undo-boundary)
    (put-text-property 1 12 'status 'dirty)
    (overlay-put ov 'val 'modified)
    (let ((s1 (buffer-string))
          (v1 (overlay-get ov 'val))
          (st1 (get-text-property 1 'status)))
      (primitive-undo 1 buffer-undo-list)
      (let ((s2 (buffer-string))
            (v2 (overlay-get ov 'val))
            (st2 (get-text-property 1 'status)))
        (primitive-undo 1 buffer-undo-list)
        (list s1 v1 st1 s2 v2 st2
              (buffer-string)
              (overlay-get ov 'val)
              (get-text-property 1 'status)
              (marker-position m))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_prop_change_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"ABCDEFGH\" 0 3 (state changed) 3 4 (state initial) 4 7 (state also-changed)) after changed also-changed #(\"ABCDEFGH\" 0 3 (state changed) 3 4 (state initial) 4 7 (state initial)) changed initial after)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (let ((ov (make-overlay 1 8)))
    (overlay-put ov 'tag 'before)
    (put-text-property 1 8 'state 'initial)
    (undo-boundary)
    (put-text-property 1 4 'state 'changed)
    (overlay-put ov 'tag 'after)
    (undo-boundary)
    (put-text-property 5 8 'state 'also-changed)
    (let ((s1 (buffer-string))
          (ov-tag (overlay-get ov 'tag))
          (p1 (get-text-property 1 'state))
          (p5 (get-text-property 5 'state)))
      (primitive-undo 1 buffer-undo-list)
      (list s1 ov-tag p1 p5
            (buffer-string)
            (get-text-property 1 'state)
            (get-text-property 5 'state)
            (overlay-get ov 'tag))))) "#,
        expect,
    );
}
