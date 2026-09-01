//! Divergence tests: undo boundary + marker + overlay + narrow + regex stress.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_chain_with_overlays_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"XXBLOCK2-XXXX BLOCK3-CCCC\" 2 9 (region b) 14 25 (region c)) 13 nil #(\"BLOCK1-AAAA BLOCK2-BBBB BLOCK3-CCCC BLOCK4-DDDD\" 0 11 (region a) 12 19 (region b) 19 23 (region b) 24 35 (region c) 36 47 (region d)) t 1 t a t b t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BLOCK1-AAAA BLOCK2-BBBB BLOCK3-CCCC BLOCK4-DDDD")
  (let ((ov1 (make-overlay 1 12))
        (ov2 (make-overlay 13 24))
        (ov3 (make-overlay 25 36))
        (ov4 (make-overlay 37 48)))
    (overlay-put ov1 'block 1)
    (overlay-put ov2 'block 2)
    (overlay-put ov3 'block 3)
    (overlay-put ov4 'block 4)
    (put-text-property 1 12 'region 'a)
    (put-text-property 13 24 'region 'b)
    (put-text-property 25 36 'region 'c)
    (put-text-property 37 48 'region 'd)
    (narrow-to-region 13 36)
    (undo-boundary)
    (goto-char (point-min))
    (insert "XX")
    (undo-boundary)
    (re-search-forward "BBBB" nil t)
    (replace-match "XXXX")
    (let ((s1 (buffer-string))
          (ov-s (overlay-start ov2))
          (reg (get-text-property 14 'region)))
      (primitive-undo 2 buffer-undo-list)
      (widen)
      (list s1 ov-s reg
            (buffer-string)
            (string= (buffer-string)
                     "BLOCK1-AAAA BLOCK2-BBBB BLOCK3-CCCC BLOCK4-DDDD")
            (overlay-get ov1 'block)
            (= (overlay-get ov1 'block) 1)
            (get-text-property 1 'region)
            (eq (get-text-property 1 'region) 'a)
            (get-text-property 13 'region)
            (eq (get-text-property 13 'region) 'b))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_after_delete_insert_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"MODIFIEDREPLACEDT-HERE\" 17 22 (section footer)) 17 #(\"ORIGINAL-CONTENT-HERE\" 0 7 (section header) 8 15 (section body) 16 21 (section footer)) t 10 t main t header t body t footer t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ORIGINAL-CONTENT-HERE")
  (let ((ov (make-overlay 1 22))
        (m (copy-marker 10 t)))
    (overlay-put ov 'tag 'main)
    (put-text-property 1 8 'section 'header)
    (put-text-property 9 16 'section 'body)
    (put-text-property 17 22 'section 'footer)
    (undo-boundary)
    (delete-region 9 16)
    (undo-boundary)
    (goto-char 9)
    (insert "REPLACED")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "ORIGINAL" nil t)
    (replace-match "MODIFIED")
    (let ((s (buffer-string))
          (m-pos (marker-position m)))
      (primitive-undo 3 buffer-undo-list)
      (list s m-pos
            (buffer-string)
            (string= (buffer-string) "ORIGINAL-CONTENT-HERE")
            (marker-position m)
            (= (marker-position m) 10)
            (overlay-get ov 'tag)
            (eq (overlay-get ov 'tag) 'main)
            (get-text-property 1 'section)
            (eq (get-text-property 1 'section) 'header)
            (get-text-property 9 'section)
            (eq (get-text-property 9 'section) 'body)
            (get-text-property 17 'section)
            (eq (get-text-property 17 'section) 'footer))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_invisible_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"SHOW1-GONE-SHOW2-GONE-SHOW3\" 0 4 (vis first) 10 14 (vis second) 21 24 (vis third)) 6 16 \"\" 1 1 1 1 t t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "SHOW1-HIDE-SHOW2-HIDE-SHOW3")
  (let ((ov1 (make-overlay 6 10))
        (ov2 (make-overlay 16 20)))
    (overlay-put ov1 'invisible t)
    (overlay-put ov2 'invisible t)
    (put-text-property 1 5 'vis 'first)
    (put-text-property 11 15 'vis 'second)
    (put-text-property 21 25 'vis 'third)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "HIDE" nil t)
      (replace-match "GONE"))
    (let ((s (buffer-string))
          (ov1-s (overlay-start ov1))
          (ov2-s (overlay-start ov2)))
      (primitive-undo 2 buffer-undo-list)
      (list s ov1-s ov2-s
            (buffer-string)
            (overlay-start ov1) (overlay-end ov1)
            (overlay-start ov2) (overlay-end ov2)
            (overlay-get ov1 'invisible)
            (overlay-get ov2 'invisible)
            (get-text-property 1 'vis)
            (eq (get-text-property 1 'vis) 'first))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_preserves_overlay_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 2) nil outer t middle t inner t \"AAAA-BBBB-CCCC-DDDD-EEEE\" nil)""#
    ]];
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
    (goto-char 5)
    (insert "XXXX")
    (let ((priorities (mapcar (lambda (ov) (overlay-get ov 'priority))
                              (sort (overlays-at 12)
                                    (lambda (a b)
                                      (< (overlay-get a 'priority)
                                         (overlay-get b 'priority)))))))
      (primitive-undo 1 buffer-undo-list)
      (list priorities
            (equal priorities '(1 2 3))
            (overlay-get ov1 'tag)
            (eq (overlay-get ov1 'tag) 'outer)
            (overlay-get ov2 'tag)
            (eq (overlay-get ov2 'tag) 'middle)
            (overlay-get ov3 'tag)
            (eq (overlay-get ov3 'tag) 'inner)
            (buffer-string)
            (= (buffer-size) 25))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_marker_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (6 4 t nil t t t t nil t 4 t 4 t first t second t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (let ((m-insert (copy-marker 4 t))
        (m-noinst (copy-marker 4 nil)))
    (put-text-property 1 4 'half 'first)
    (put-text-property 5 8 'half 'second)
    (undo-boundary)
    (goto-char 4)
    (insert "XX")
    (let ((mi (marker-position m-insert))
          (mn (marker-position m-noinst))
          (mi-type (marker-insertion-type m-insert))
          (mn-type (marker-insertion-type m-noinst)))
      (primitive-undo 1 buffer-undo-list)
      (list mi mn mi-type mn-type
            (= mi 6) (= mn 4)
            mi-type (eq mi-type t)
            mn-type (null mn-type)
            (marker-position m-insert)
            (= (marker-position m-insert) 4)
            (marker-position m-noinst)
            (= (marker-position m-noinst) 4)
            (get-text-property 1 'half)
            (eq (get-text-property 1 'half) 'first)
            (get-text-property 5 'half)
            (eq (get-text-property 5 'half) 'second))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_text_property_changes_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (dirty modified dirty dirty t t t t dirty nil dirty nil #(\"AAAA-BBBB-CCCC-DDDD\" 0 3 (status dirty) 3 4 (status dirty) 4 8 (status dirty) 8 9 (status dirty) 9 13 (status dirty) 13 14 (status dirty) 14 16 (status dirty)) monitor t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (let ((ov (make-overlay 1 17)))
    (overlay-put ov 'tag 'monitor)
    (put-text-property 1 4 'status 'clean)
    (put-text-property 5 9 'status 'clean)
    (put-text-property 10 14 'status 'clean)
    (put-text-property 15 17 'status 'clean)
    (undo-boundary)
    (put-text-property 1 17 'status 'dirty)
    (undo-boundary)
    (put-text-property 5 9 'status 'modified)
    (let ((s1 (get-text-property 1 'status))
          (s2 (get-text-property 5 'status))
          (s3 (get-text-property 10 'status)))
      (primitive-undo 1 buffer-undo-list)
      (let ((s2b (get-text-property 5 'status)))
        (primitive-undo 1 buffer-undo-list)
        (list s1 s2 s3 s2b
              (eq s1 'dirty)
              (eq s2 'modified)
              (eq s3 'dirty)
              (eq s2b 'dirty)
              (get-text-property 1 'status)
              (eq (get-text-property 1 'status) 'clean)
              (get-text-property 5 'status)
              (eq (get-text-property 5 'status) 'clean)
              (buffer-string)
              (overlay-get ov 'tag)
              (eq (overlay-get ov 'tag) 'monitor)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_multiple_markers_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 7 12 19 24) (1 5 10 15 20) t \"AAAA-BBBB-CCCC-DDDD-EEEE\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 5 t))
        (m3 (copy-marker 10 t))
        (m4 (copy-marker 15 t))
        (m5 (copy-marker 20)))
    (undo-boundary)
    (goto-char 5)
    (insert "XX")
    (undo-boundary)
    (goto-char 15)
    (insert "YY")
    (let ((positions (mapcar 'marker-position (list m1 m2 m3 m4 m5))))
      (primitive-undo 2 buffer-undo-list)
      (list positions
            (mapcar 'marker-position (list m1 m2 m3 m4 m5))
            (equal (mapcar 'marker-position (list m1 m2 m3 m4 m5))
                   '(1 5 10 15 20))
            (buffer-string)
            (= (buffer-size) 24))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_overlay_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 1 t t 1 nil \"AAAA-BBBB-CCCC-DDDD\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (let ((initial-count (length (overlays-in 1 17))))
    (undo-boundary)
    (let ((ov (make-overlay 5 9)))
      (overlay-put ov 'created 'yes)
      (let ((with-count (length (overlays-in 1 17))))
        (primitive-undo 1 buffer-undo-list)
        (list initial-count with-count
              (= initial-count 0)
              (= with-count 1)
              (length (overlays-in 1 17))
              (= (length (overlays-in 1 17)) initial-count)
              (buffer-string)
              (= (buffer-size) 17)))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_kill_ring_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 22 55)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "KEEP-REMOVE-KEEP-REMOVE-KEEP")
  (put-text-property 1 5 'zone 'keep)
  (put-text-property 6 11 'zone 'remove)
  (put-text-property 12 15 'zone 'keep)
  (put-text-property 16 21 'zone 'remove)
  (put-text-property 22 26 'zone 'keep)
  (undo-boundary)
  (kill-region 6 11)
  (undo-boundary)
  (kill-region 11 16)
  (let ((s (buffer-string))
        (kr (current-kill 0)))
    (primitive-undo 2 buffer-undo-list)
    (list s kr
          (buffer-string)
          (string= (buffer-string)
                   "KEEP-REMOVE-KEEP-REMOVE-KEEP")
          (get-text-property 1 'zone)
          (eq (get-text-property 1 'zone) 'keep)
          (get-text-property 6 'zone)
          (eq (get-text-property 6 'zone) 'remove)))) #"#,
        expect,
    );
}

#[test]
fn divergence_undo_with_overlay_before_after_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"CONXXTENT\" 0 3 (wrapped t) 5 8 (wrapped t)) \"[\" \"]\" t t #(\"CONTENT\" 0 3 (wrapped t) 3 6 (wrapped t)) t \"[\" \"]\" bold t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "CONTENT")
  (let ((ov (make-overlay 1 7)))
    (overlay-put ov 'before-string "[")
    (overlay-put ov 'after-string "]")
    (overlay-put ov 'face 'bold)
    (put-text-property 1 7 'wrapped t)
    (undo-boundary)
    (goto-char 4)
    (insert "XX")
    (let ((s (buffer-string))
          (before (overlay-get ov 'before-string))
          (after (overlay-get ov 'after-string)))
      (primitive-undo 1 buffer-undo-list)
      (list s before after
            (string= before "[")
            (string= after "]")
            (buffer-string)
            (string= (buffer-string) "CONTENT")
            (overlay-get ov 'before-string)
            (overlay-get ov 'after-string)
            (overlay-get ov 'face)
            (eq (overlay-get ov 'face) 'bold)
            (get-text-property 1 'wrapped)
            (eq (get-text-property 1 'wrapped) t))))) "#,
        expect,
    );
}
