//! Divergence tests: keymap, syntax table, and category table edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_define_key_and_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (insert-a insert-b nil prefix-a)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-sparse-keymap)))
  (define-key map "a" 'insert-a)
  (define-key map "b" 'insert-b)
  (define-key map (kbd "C-c a") 'prefix-a)
  (list (lookup-key map "a")
        (lookup-key map "b")
        (lookup-key map "c")
        (lookup-key map (kbd "C-c a"))))"#,
        expect,
    );
}

#[test]
fn divergence_keymap_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (parent-a child-b (keymap (97 . parent-a)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((parent (make-sparse-keymap))
        (child (make-sparse-keymap)))
  (define-key parent "a" 'parent-a)
  (define-key child "b" 'child-b)
  (set-keymap-parent child parent)
  (list (lookup-key child "a")
        (lookup-key child "b")
        (keymap-parent child)))"#,
        expect,
    );
}

#[test]
fn divergence_current_active_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((maps (current-active-maps)))
  (list (length maps)
        (> (length maps) 0)
        (keymapp (car maps))))"#,
        expect,
    );
}

#[test]
fn divergence_where_is() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([7])""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(where-is-internal 'keyboard-quit (current-global-map))"#,
        expect,
    );
}

#[test]
fn divergence_syntax_table_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 6 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((st (standard-syntax-table)))
  (list (char-table-p st)
        (aref st ?a)
        (aref st ?0)
        (aref st ? )
        (aref st ?())))"#,
        expect,
    );
}

#[test]
fn divergence_modify_syntax_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (39 119 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (let ((st (copy-syntax-table (standard-syntax-table))))
    (set-syntax-table st)
    (modify-syntax-entry ?$ "'")
    (list (char-syntax ?$)
          (char-syntax ?a)
          (char-syntax ? ))))"#,
        expect,
    );
}

#[test]
fn divergence_scan_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (21 \"(foo (bar baz) quux)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "(foo (bar baz) quux)")
  (goto-char 1)
  (let ((end (scan-sexps (point) 1)))
    (list end (buffer-substring 1 end))))"#,
        expect,
    );
}

#[test]
fn divergence_forward_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "/* comment */ code")
  (goto-char 1)
  (forward-comment 1)
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_parse_partial_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 31 32 nil nil nil 0 nil nil (1 31) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "(defun foo ()\n  \"docstring\"\n  (list 1 2))")
  (parse-partial-sexp 1 35))"#,
        expect,
    );
}

#[test]
fn divergence_category_table_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function category-table-mnemonics)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (category-table)))
  (list (char-table-p ct)
        (category-table-mnemonics ct)
        (char-table-range ct ?a)
        (char-table-range ct ?0)))"#,
        expect,
    );
}

#[test]
fn divergence_define_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil \".LTalr\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (define-category ?T "Test category")
  (modify-category-entry ?a ?T)
  (list (aref (char-category-set ?a) 0)
        (category-set-mnemonics (char-category-set ?a))))"#,
        expect,
    );
}
