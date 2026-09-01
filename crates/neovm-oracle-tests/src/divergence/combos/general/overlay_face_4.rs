//! Complex/combo divergence probes (batch 4): read-only enforcement across
//! many modifying operations. Confirmed bug (batch 1/2): Neomacs does not
//! block inserts into a read-only text-property region. This batch probes
//! whether other modifying ops (upcase-region, transpose, replace-string,
//! subst-char, self-insert, delete-region spanning, buffer-read-only, etc.)
//! are also unenforced.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_combo_ro_upcase_region_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (condition-case err (progn (upcase-region 2 4) (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_downcase_region_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEF")
  (put-text-property 2 4 'read-only t)
  (condition-case err (progn (downcase-region 2 4) (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_capitalize_region_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc def")
  (put-text-property 2 4 'read-only t)
  (condition-case err (progn (capitalize-region 2 4) (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_transpose_chars_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (goto-char 3)
  (condition-case err (progn (transpose-chars 1) (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_subst_char_in_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (condition-case err (progn (subst-char-in-region 2 4 ?b ?X nil) (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_replace_string_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (goto-char 1)
  (condition-case err (progn (replace-string "bc" "XY") (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_self_insert_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (goto-char 3)
  (condition-case err (progn (let ((last-command-event ?X)) (self-insert-command 1)) 'ok) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_delete_region_spanning_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (condition-case err (progn (delete-region 1 5) (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_insert_at_boundary_allowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #(\"Xabcdef\" 2 4 (read-only t))""#]];
    // Insert just BEFORE the read-only region (at its start) should be allowed.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (goto-char 1)
  (condition-case err (progn (insert "X") (buffer-string)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_buffer_read_only_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK buffer-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (setq buffer-read-only t)
  (goto-char 2)
  (condition-case err (progn (insert "X") 'inserted) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_store_substring_into_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 90 99 100 101)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (let ((s (buffer-substring 1 6)))
    (condition-case err (progn (store-substring s 1 ?Z) (append s nil)) (error (car err)))))
"##,
        expect,
    );
}

#[test]
fn div_combo_ro_undo_through_readonly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK user-error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (put-text-property 2 4 'read-only t)
  (goto-char 3)
  (let ((r1 (condition-case err (progn (insert "X") 'inserted) (error (car err)))))
    (condition-case err (progn (undo) (buffer-string)) (error (car err)))))
"##,
        expect,
    );
}
