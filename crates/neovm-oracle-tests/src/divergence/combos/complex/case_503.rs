/// Batch 503: overlay-lists characterization — various point and edit patterns.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx503_ol_before_point_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 1 3))) (overlay-put o 'face 'bold))
  (goto-char 4)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_after_point_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 4 6))) (overlay-put o 'face 'bold))
  (goto-char 2)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_both_sides() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdefgh")
  (let ((o1 (make-overlay 1 3)) (o2 (make-overlay 6 8)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic))
  (goto-char 4)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 3 4))) (overlay-put o 'face 'bold))
  (goto-char 3)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_delete_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable o)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 1 6))) (overlay-put o 'face 'bold))
  (delete-overlay o)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_insert_mid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 2 5))) (overlay-put o 'face 'bold))
  (goto-char 3)
  (insert "XXX")
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_delete_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdefgh")
  (let ((o1 (make-overlay 2 4)) (o2 (make-overlay 5 7)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic))
  (delete-region 1 4)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_many_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert (make-string 100 ?x))
  (dotimes (i 20) (let ((o (make-overlay (1+ i) (+ 2 i)))) (overlay-put o 'face 'bold)))
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((o (make-overlay 2 5))) (overlay-put o 'face 'bold))
  (undo)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_move_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable o)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 1 6))) (overlay-put o 'face 'bold))
  (move-overlay o 3 5)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_no_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o1 (make-overlay 1 3)) (o2 (make-overlay 4 6)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic))
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_window_restricted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument bufferp #<window 1 on *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 1 6 (selected-window))))
    (overlay-put o 'face 'bold))
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_evaporate_on_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (let ((o (make-overlay 2 3)))
    (overlay-put o 'evaporate t))
  (delete-region 2 3)
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_shared_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 1 6)))
    (overlay-put o 'face 'bold)
    (overlay-put o 'mouse-face 'highlight)
    (overlay-put o 'help-echo "help"))
  (list (length (car (overlay-lists))) (length (cdr (overlay-lists)))))
"##,
        expect,
    );
}

#[test]
fn div_cx503_ol_end_before_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<overlay from 1 to 1 in *scratch*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (make-overlay 5 3)
  (error (car e)))
"##,
        expect,
    );
}
