//! Divergence tests: overlay before/after strings + insert edges + undo combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_overlay_invisible_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "VISIBLE-HIDDEN-VISIBLE-HIDDEN-VISIBLE")
  (put-text-property 1 7 'vis 'show)
  (put-text-property 9 14 'vis 'hide)
  (put-text-property 16 22 'vis 'show)
  (put-text-property 24 29 'vis 'hide)
  (put-text-property 31 37 'vis 'show)
  (let ((ov1 (make-overlay 9 14)) (ov2 (make-overlay 24 29)))
    (overlay-put ov1 'invisible t)
    (overlay-put ov2 'invisible t)
    (let ((m (copy-marker 9 t))
          (ov-all (make-overlay 1 37)))
      (overlay-put ov-all 'wrap t)
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "VISIBLE" nil t)
      (replace-match "SHOWN")
      (let ((s (buffer-string)))
        (primitive-undo 1 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "VISIBLE-HIDDEN-VISIBLE-HIDDEN-VISIBLE")
              (= (marker-position m) 9)
              (get-text-property 1 'vis) (eq (get-text-property 1 'vis) 'show)
              (get-text-property 9 'vis) (eq (get-text-property 9 'vis) 'hide)
              (overlay-get ov1 'invisible)
              (overlay-get ov2 'invisible)
              (overlay-get ov-all 'wrap))))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_face_priority_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 35 89)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (put-text-property 1 4 'style 'plain)
  (put-text-property 6 9 'style 'plain)
  (put-text-property 11 14 'style 'plain)
  (put-text-property 16 19 'style 'plain)
  (put-text-property 21 24 'style 'plain)
  (let ((ov1 (make-overlay 1 4)) (ov2 (make-overlay 6 9))
        (ov3 (make-overlay 11 14)) (ov4 (make-overlay 16 19))
        (ov5 (make-overlay 21 24)))
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (overlay-put ov3 'face 'underline)
    (overlay-put ov4 'face 'bold-italic)
    (overlay-put ov5 'face 'highlight)
    (let ((m (copy-marker 6 t)))
      (undo-boundary)
      (goto-char 6)
      (insert "XXX")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "CCCC" nil t)
      (replace-match "ZZZZ")
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "AAAA-BBBB-CCCC-DDDD-EEEE")
              (= (marker-position m) 6)
              (overlay-get ov1 'face) (eq (overlay-get ov1 'face) 'bold)
              (overlay-get ov2 'face) (eq (overlay-get ov2 'face) 'italic)
              (overlay-get ov3 'face) (eq (overlay-get ov3 'face) 'underline)
              (overlay-get ov4 'face) (eq (overlay-get ov4 'face) 'bold-italic)
              (overlay-get ov5 'face) (eq (overlay-get ov5 'face) 'highlight)
              (get-text-property 1 'style) (eq (get-text-property 1 'style) 'plain))))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_evaporation_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 17 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "KEEP-DELETE-KEEP-DELETE-KEEP")
  (put-text-property 1 4 'zone 'keep)
  (put-text-property 6 11 'zone 'delete)
  (put-text-property 13 16 'zone 'keep)
  (put-text-property 18 23 'zone 'delete)
  (put-text-property 25 28 'zone 'keep)
  (let ((ov-del1 (make-overlay 6 11)) (ov-del2 (make-overlay 18 23))
        (ov-keep (make-overlay 1 28)))
    (overlay-put ov-del1 'grp 'del)
    (overlay-put ov-del2 'grp 'del)
    (overlay-put ov-keep 'grp 'keep)
    (let ((m (copy-marker 6 t)))
      (undo-boundary)
      (delete-region 6 12)
      (undo-boundary)
      (delete-region 17 24)
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "KEEP-DELETE-KEEP-DELETE-KEEP")
              (= (marker-position m) 6)
              (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'keep)
              (get-text-property 6 'zone) (eq (get-text-property 6 'zone) 'delete)
              (get-text-property 13 'zone) (eq (get-text-property 13 'zone) 'keep)
              (overlay-get ov-keep 'grp) (eq (overlay-get ov-keep 'grp) 'keep))))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure ((mod-count . 1)) (ov beg end pre-len) (setq mod-count (+ mod-count 1))) 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (put-text-property 1 4 'blk 1)
  (put-text-property 6 9 'blk 2)
  (put-text-property 11 14 'blk 3)
  (put-text-property 16 19 'blk 4)
  (let ((mod-count 0)
        (ov (make-overlay 6 14)))
    (overlay-put ov 'modification-hooks
                 (list (lambda (ov beg end pre-len)
                         (setq mod-count (+ mod-count 1)))))
    (let ((m (copy-marker 6 t)))
      (undo-boundary)
      (goto-char 6)
      (insert "QQ")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "BBBB" nil t)
      (replace-match "XXXX")
      (let ((s (buffer-string))
            (cnt mod-count))
        (primitive-undo 2 buffer-undo-list)
        (list s cnt (> cnt 0)
              (buffer-string)
              (string= (buffer-string) "AAAA-BBBB-CCCC-DDDD")
              (= (marker-position m) 6)
              (get-text-property 1 'blk) (= (get-text-property 1 'blk) 1)
              (get-text-property 6 'blk) (= (get-text-property 6 'blk) 2)
              (overlay-get ov 'modification-hooks))))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_insert_behind_front() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 22 82)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAAAAAAA")
  (put-text-property 1 9 'flat t)
  (let ((ov (make-overlay 5 5)))
    (overlay-put ov 'insert-in-front-hooks (list (lambda (&rest _))))
    (overlay-put ov 'point 'boundary)
    (let ((m (copy-marker 5 t)))
      (undo-boundary)
      (goto-char 5)
      (insert "XXX")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "AAAA" nil t)
      (replace-match "BBBB")
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "AAAAAAAAA")
              (= (marker-position m) 5)
              (get-text-property 1 'flat) (eq (get-text-property 1 'flat) t)
              (overlay-get ov 'point) (eq (overlay-get ov 'point) 'boundary))))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_priority_stack_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYZZ\" 48 49 (layer 10)) #(\"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ\" 0 4 (layer 1) 5 8 (layer 2) 8 9 (layer 2) 10 12 (layer 3) 12 14 (layer 3) 15 16 (layer 4) 16 19 (layer 4) 20 24 (layer 5) 25 28 (layer 6) 28 29 (layer 6) 30 32 (layer 7) 32 34 (layer 7) 35 36 (layer 8) 36 39 (layer 8) 40 44 (layer 9) 45 48 (layer 10) 48 49 (layer 10)) t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 50 ?Z))
  (let ((ovs nil))
    (dotimes (i 10)
      (let ((ov (make-overlay (+ 1 (* i 5)) (+ 4 (* i 5)))))
        (overlay-put ov 'priority (+ i 1))
        (overlay-put ov 'layer (+ i 1))
        (put-text-property (+ 1 (* i 5)) (+ 5 (* i 5)) 'layer (+ i 1))
        (push ov ovs)))
    (setq ovs (nreverse ovs))
    (let ((m (copy-marker 6 t)))
      (undo-boundary)
      (goto-char 1)
      (while (re-search-forward "ZZZZ" nil t)
        (replace-match "YYYY"))
      (let ((s (buffer-string)))
        (primitive-undo 1 buffer-undo-list)
        (let ((all-ok t))
          (dotimes (i 10)
            (let ((ov (nth i ovs)))
              (unless (and (= (overlay-get ov 'priority) (+ i 1))
                           (= (overlay-get ov 'layer) (+ i 1))
                           (= (get-text-property (+ 1 (* i 5)) 'layer) (+ i 1)))
                (setq all-ok nil))))
          (list s
                (buffer-string)
                (string= (buffer-string) (make-string 50 ?Z))
                all-ok
                (= (marker-position m) 6))))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_intangible_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 24 47)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BEFORE-INTANGIBLE-AFTER")
  (put-text-property 1 6 'part 'before)
  (put-text-property 8 17 'part 'intangible)
  (put-text-property 19 23 'part 'after)
  (let ((ov (make-overlay 8 17)))
    (overlay-put ov 'intangible t)
    (let ((m (copy-marker 8 t))
          (ov-all (make-overlay 1 23)))
      (overlay-put ov-all 'scope 'all)
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "BEFORE" nil t)
      (replace-match "CHANGED")
      (let ((s (buffer-string)))
        (primitive-undo 1 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "BEFORE-INTANGIBLE-AFTER")
              (= (marker-position m) 8)
              (get-text-property 1 'part) (eq (get-text-property 1 'part) 'before)
              (get-text-property 8 'part) (eq (get-text-property 8 'part) 'intangible)
              (overlay-get ov 'intangible)
              (overlay-get ov-all 'scope))))))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_overlays_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"OUTER-QQINNER-XXXX-INNER-OUTER\" 0 4 (level outer) 8 12 (level inner) 19 23 (level inner) 25 29 (level outer)) #(\"OUTER-INNER-CORE-INNER-OUTER\" 0 4 (level outer) 6 10 (level inner) 12 15 (level core) 17 21 (level inner) 23 27 (level outer)) t t outer t inner t core t 1 t 2 t 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "OUTER-INNER-CORE-INNER-OUTER")
  (put-text-property 1 5 'level 'outer)
  (put-text-property 7 11 'level 'inner)
  (put-text-property 13 16 'level 'core)
  (put-text-property 18 22 'level 'inner)
  (put-text-property 24 28 'level 'outer)
  (let ((ov-outer (make-overlay 1 28))
        (ov-inner (make-overlay 7 22))
        (ov-core (make-overlay 13 16))
        (m (copy-marker 7 t)))
    (overlay-put ov-outer 'depth 1)
    (overlay-put ov-inner 'depth 2)
    (overlay-put ov-core 'depth 3)
    (undo-boundary)
    (goto-char 7)
    (insert "QQ")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "CORE" nil t)
    (replace-match "XXXX")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "OUTER-INNER-CORE-INNER-OUTER")
            (= (marker-position m) 7)
            (get-text-property 1 'level) (eq (get-text-property 1 'level) 'outer)
            (get-text-property 7 'level) (eq (get-text-property 7 'level) 'inner)
            (get-text-property 13 'level) (eq (get-text-property 13 'level) 'core)
            (overlay-get ov-outer 'depth) (= (overlay-get ov-outer 'depth) 1)
            (overlay-get ov-inner 'depth) (= (overlay-get ov-inner 'depth) 2)
            (overlay-get ov-core 'depth) (= (overlay-get ov-core 'depth) 3))))) "#,
        expect,
    );
}

#[test]
fn deficiency_overlay_textprop_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 100 ?A))
  (dotimes (i 10)
    (let ((start (+ 1 (* i 10)))
          (end (+ 10 (* i 10))))
      (put-text-property start end 'group (+ i 1))
      (let ((ov (make-overlay start end)))
        (overlay-put ov 'group (+ i 1)))))
  (let ((all-match t))
    (dotimes (i 10)
      (let ((pos (+ 5 (* i 10))))
        (unless (= (get-text-property pos 'group)
                   (overlay-get (car (overlays-at pos)) 'group))
          (setq all-match nil))))
    (list all-match
          (= (length (overlays-in 1 100)) 10)
          (= (buffer-size) 100)))) "#,
        expect,
    );
}

#[test]
fn deficiency_overlay_merge_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil whole t nil nil t 1 t 2 t 3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 30 ?M))
  (put-text-property 1 30 'section 'whole)
  (let ((ov1 (make-overlay 1 10))
        (ov2 (make-overlay 11 20))
        (ov3 (make-overlay 21 30)))
    (overlay-put ov1 'part 1)
    (overlay-put ov2 'part 2)
    (overlay-put ov3 'part 3)
    (let ((m (copy-marker 11 t)))
      (undo-boundary)
      (goto-char 11)
      (insert "XXX")
      (list (= (overlay-start ov1) 1)
            (>= (overlay-end ov1) 10)
            (>= (overlay-start ov2) 11)
            (= (marker-position m) 11)
            (get-text-property 1 'section) (eq (get-text-property 1 'section) 'whole)
            (get-text-property 11 'section) (eq (get-text-property 11 'section) 'whole)
            (= (buffer-size) 33)
            (overlay-get ov1 'part) (= (overlay-get ov1 'part) 1)
            (overlay-get ov2 'part) (= (overlay-get ov2 'part) 2)
            (overlay-get ov3 'part) (= (overlay-get ov3 'part) 3))))) "#,
        expect,
    );
}
