/// Batch 529: buffer-local variables deep, marker with kill buffer, indirect buffer.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx529_buffer_local_set_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"test-val\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq-local cx529-local "test-val")
  cx529-local)
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_local_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable cx529-kv)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((v (make-local-variable 'cx529-kv)))
    (setq cx529-kv 42)
    (kill-local-variable 'cx529-kv)
    (default-value 'cx529-kv)))
"##,
        expect,
    );
}

#[test]
fn div_cx529_marker_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (get-buffer-create " *cx529-mkb*"))
      (m (make-marker)))
  (with-current-buffer buf (insert "test") (set-marker m 3))
  (kill-buffer buf)
  (marker-buffer m))
"##,
        expect,
    );
}

#[test]
fn div_cx529_marker_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (let ((m (set-marker (make-marker) 2)))
    (set-marker-insertion-type m t)
    (insert "XY")
    (marker-position m)))
"##,
        expect,
    );
}

#[test]
fn div_cx529_indirect_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<buffer  *cx529-base*> \"base-text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((base (get-buffer-create " *cx529-base*"))
      (ind (make-indirect-buffer (get-buffer-create " *cx529-base*") " *cx529-ind*")))
  (with-current-buffer base (insert "base-text"))
  (list (buffer-base-buffer ind) (with-current-buffer ind (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx529_indirect_buffer_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"indirect-insert\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((base (get-buffer-create " *cx529-base2*"))
      (ind (make-indirect-buffer (get-buffer-create " *cx529-base2*") " *cx529-ind2*")))
  (with-current-buffer ind (insert "indirect-insert"))
  (with-current-buffer base (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_local_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq-local a 1 b 2 c 3)
  (list a b c))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_local_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (global local)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((v (make-local-variable 'cx529-dv)))
  (setq cx529-dv 'local)
  (set-default 'cx529-dv 'global)
  (list (default-value 'cx529-dv) cx529-dv))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_local_permanent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cx529-pm)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((v (make-local-variable 'cx529-pm)))
    (put 'cx529-pm 'permanent-local t)
    (setq cx529-pm 'permanent))
  (cx529-pm))
"##,
        expect,
    );
}

#[test]
fn div_cx529_marker_before_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (let ((m1 (set-marker (make-marker) 2)))
    (set-marker-insertion-type m1 nil)
    (let ((m2 (set-marker (make-marker) 2)))
      (set-marker-insertion-type m2 t)
      (insert "X")
      (list (marker-position m1) (marker-position m2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (get-buffer-create " *cx529-clone*")))
  (with-current-buffer buf (insert "content"))
  (let ((clone (clone-buffer " *cx529-clone-c*")))
    (with-current-buffer clone (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_name_reuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"*cx529-rn*\" \"*cx529-rn*<2>\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((b1 (get-buffer-create "*cx529-rn*"))
      (b2 (generate-new-buffer "*cx529-rn*")))
  (list (buffer-name b1) (buffer-name b2)))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_local_variable_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (buffer-undo-list buffer-display-time buffer-display-count buffer-invisibility-spec buffer-file-truename point-before-scroll buffer-auto-save-file-format buffer-file-format enable-multibyte-characters mark-active mode-name local-minor-modes major-mode buffer-read-only buffer-auto-save-file-name buffer-saved-size buffer-backed-up default-directory buffer-file-name a b c)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq-local a 1 b 2 c 3)
  (mapcar #'car (buffer-local-variables)))
"##,
        expect,
    );
}

#[test]
fn div_cx529_marker_delete_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((m (set-marker (make-marker) 4)))
    (delete-region 2 6)
    (marker-position m)))
"##,
        expect,
    );
}

#[test]
fn div_cx529_buffer_swap_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (get-buffer-create " *cx529-swap-a*"))
      (b (get-buffer-create " *cx529-swap-b*")))
  (with-current-buffer a (insert "AAAA"))
  (with-current-buffer b (insert "BBBB"))
  (buffer-swap-text a b)
  (prog1 (with-current-buffer a (buffer-string))
    (kill-buffer a) (kill-buffer b)))
"##,
        expect,
    );
}
