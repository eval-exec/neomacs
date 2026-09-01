//! Syntax-table (modify-syntax-entry, syntax-ppss string class, char-syntax,
//! skip-chars/syntax) and char-table (range, parent, extra slots, map) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn syntax_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 119 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((tbl (make-syntax-table)))
    (modify-syntax-entry ?_ "w" tbl)
    (modify-syntax-entry ?- "." tbl)
    (set-syntax-table tbl)
    (insert "foo_bar-baz")
    (goto-char 1) (forward-word)
    (list (point) (char-syntax ?_) (char-syntax ?-))))"##,
        expect,
    );
}

#[test]
fn syntax_skip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 4 10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "   abc123  ")
  (goto-char 1)
  (let ((n (skip-chars-forward " ")))
    (list n (point) (progn (skip-syntax-forward "w") (point))
          (progn (skip-chars-forward "0-9") (point)))))"##,
        expect,
    );
}

#[test]
fn syntax_string_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(foo \"a string\" bar)")
  (goto-char 6)
  (list (nth 3 (syntax-ppss)) (progn (forward-sexp) (point))))"##,
        expect,
    );
}

#[test]
fn syntax_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (40 41 60 39 34 \"_\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (lisp-mode)
  (list (char-syntax ?\() (char-syntax ?\)) (char-syntax ?\;)
        (char-syntax ?') (char-syntax ?\")  (string (char-syntax ?-))))"##,
        expect,
    );
}

#[test]
fn chartable_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (aval default-val range-val range-val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'test 'default-val)))
  (aset ct ?a 'aval) (aset ct ?b 'bval)
  (set-char-table-range ct '(?x . ?z) 'range-val)
  (list (aref ct ?a) (aref ct ?c) (aref ct ?y) (char-table-range ct ?y)))"##,
        expect,
    );
}

#[test]
fn chartable_extra_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil test nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'test)))
  (set-char-table-extra-slot ct 0 'extra0)
  (list (char-table-extra-slot ct 0) (char-table-p ct) (type-of ct)))"##,
        expect,
    );
}

#[test]
fn chartable_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((97 . 1) (98 . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'test)) (acc nil))
  (aset ct ?a 1) (aset ct ?b 2)
  (map-char-table (lambda (k v) (push (cons (if (consp k) 'range k) v) acc)) ct)
  (sort acc (lambda (x y) (< (cdr x) (cdr y)))))"##,
        expect,
    );
}

#[test]
fn chartable_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (from-parent t test)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((parent (make-char-table 'test)) (child (make-char-table 'test)))
  (aset parent ?a 'from-parent)
  (set-char-table-parent child parent)
  (list (aref child ?a) (eq (char-table-parent child) parent)
        (char-table-subtype child)))"##,
        expect,
    );
}
