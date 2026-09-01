//! Strong uncovered-features-54 oracle tests — org-macs utilities, org-faces, org-check-external.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-trim
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_trim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-trim)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-trim "  hello  ")
        (org-trim "\nhello\n")
        (org-trim "  \n hello \n  "))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-string-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-string-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-string-width "hello")
        (org-string-width "hello world")
        (org-string-width ""))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-remove-indentation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_remove_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-remove-indentation)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-remove-indentation "  hello\n  world")
        (org-remove-indentation "hello\nworld")
        (org-remove-indentation "    hello\n      world"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-do-remove-indentation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_do_remove_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-do-remove-indentation)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "  hello\n  world")
  (org-do-remove-indentation)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-number-sequence
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_number_seq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-number-sequence)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-number-sequence 1 5)
        (org-number-sequence 1 10 2)
        (org-number-sequence 5 1 -1))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-not-nil
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_not_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-not-nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-not-nil "test")
        (org-not-nil nil)
        (org-not-nil "")
        (org-not-nil 0))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-not-empty
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_not_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-not-empty)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-not-empty "test")
        (org-not-empty "")
        (org-not-empty nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-unescape-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_unescape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-unescape-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-unescape-string "hello\\nworld")
        (org-unescape-string "hello\\tworld")
        (org-unescape-string "hello\\\\world"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-replace-escapes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_replace_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-replace-escapes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-replace-escapes "hello\\nworld")
        (org-replace-escapes "hello\\tworld")
        (org-replace-escapes "hello\\\\world"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-faces level
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_faces_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid face\" org-level-1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'org-level-1 :foreground nil t)
        (face-attribute 'org-level-2 :foreground nil t)
        (face-attribute 'org-level-3 :foreground nil t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-faces todo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_faces_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid face\" org-todo)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'org-todo :foreground nil t)
        (face-attribute 'org-done :foreground nil t)
        (face-attribute 'org-priority :foreground nil t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-faces table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_faces_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid face\" org-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'org-table :foreground nil t)
        (face-attribute 'org-table-row :foreground nil t)
        (face-attribute 'org-formula :foreground nil t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-faces link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_faces_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid face\" org-link)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'org-link :foreground nil t)
        (face-attribute 'org-meta-line :foreground nil t)
        (face-attribute 'org-document-info :foreground nil t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-faces block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_faces_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid face\" org-block)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'org-block :foreground nil t)
        (face-attribute 'org-verbatim :foreground nil t)
        (face-attribute 'org-code :foreground nil t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-check-external-command
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_check_external() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-check-external-command "ls" "test")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-open-file
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_open_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-open-file "/tmp/test.org")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-switch-to-buffer-other-window
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-switch-to-buffer-other-window (current-buffer))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-pop-to-buffer-same-window
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-pop-to-buffer-same-window (current-buffer))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-escape-code-in-region
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-escape-code-in-region)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello\nworld\n")
  (org-escape-code-in-region (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-unescape-code-in-region
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf54_unescape_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-unescape-code-in-region)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello\n,world\n")
  (org-unescape-code-in-region (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}
