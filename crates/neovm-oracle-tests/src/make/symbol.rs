//! Oracle parity tests for `make-symbol` (uninterned symbols).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-symbol basics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_symbol_creates_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(symbolp (make-symbol "test"))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_make_symbol_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"my-sym\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(symbol-name (make-symbol "my-sym"))"#,
        expect,
    );
    assert_ok_eq(r#""my-sym""#, &o, &n);
}

#[test]
fn oracle_prop_make_symbol_reuses_name_string_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
      (let* ((name (copy-sequence "abc"))
             (sym (make-symbol name)))
        (aset name 1 ?Z)
        (list (eq name (symbol-name sym))
              name
              (symbol-name sym)))"#;
    let expect = expect_test::expect![[r#""OK (t \"aZc\" \"aZc\")""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#"(t "aZc" "aZc")"#, &o, &n);
}

#[test]
fn oracle_prop_make_symbol_not_interned() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-symbol creates uninterned symbols - not eq to interned ones
    let form = r####"(let ((s (make-symbol "hello")))
                    (list (symbolp s)
                          (eq s 'hello)
                          (equal (symbol-name s) "hello")))"####;
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_symbol_each_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two calls with same name produce different symbols
    let form = r####"(let ((a (make-symbol "test"))
                        (b (make-symbol "test")))
                    (list (eq a b)
                          (equal (symbol-name a) (symbol-name b))))"####;
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_symbol_set_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Can set value on uninterned symbol
    let form = r####"(let ((s (make-symbol "counter")))
                    (set s 0)
                    (set s (1+ (symbol-value s)))
                    (set s (1+ (symbol-value s)))
                    (symbol-value s))"####;
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_symbol_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Uninterned symbols can have plists
    let form = r####"(let ((s (make-symbol "tagged")))
                    (put s 'type 'integer)
                    (put s 'range '(0 100))
                    (list (get s 'type)
                          (get s 'range)))"####;
    let expect = expect_test::expect![[r#""OK (integer (0 100))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: gensym-like pattern with make-symbol
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_symbol_gensym_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement gensym-like counter with uninterned symbols
    let form = r####"(let ((counter 0))
                    (let ((gen (lambda (prefix)
                                 (setq counter (1+ counter))
                                 (make-symbol
                                  (concat prefix
                                          (number-to-string counter))))))
                      (let ((s1 (funcall gen "g"))
                            (s2 (funcall gen "g"))
                            (s3 (funcall gen "tmp")))
                        (list (symbol-name s1)
                              (symbol-name s2)
                              (symbol-name s3)
                              (eq s1 s2)))))"####;
    let expect = expect_test::expect![[r#""OK (\"g1\" \"g2\" \"tmp3\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gensym_counter_prefix_and_uninterned_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:gensym formats PREFIX with %s, defaults nil to "g", and
    // increments the global gensym-counter after every generated symbol.
    let form = r#"(let ((gensym-counter 7))
  (let ((a (gensym))
        (b (gensym "tmp"))
        (c (gensym nil))
        (d (gensym 42)))
    (list
     (mapcar (lambda (s)
               (list (symbol-name s)
                     (intern-soft (symbol-name s))
                     (symbolp s)))
             (list a b c d))
     gensym-counter
     (eq a (make-symbol (symbol-name a)))
     (equal (symbol-name a)
            (symbol-name (make-symbol (symbol-name a)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"g7\" nil t) (\"tmp8\" nil t) (\"g9\" nil t) (\"4210\" nil t)) 11 nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_symbol_as_unique_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use uninterned symbols as unique keys in an alist
    let form = r####"(let ((k1 (make-symbol "key"))
                        (k2 (make-symbol "key"))
                        (k3 (make-symbol "key")))
                    (let ((table (list (cons k1 "first")
                                       (cons k2 "second")
                                       (cons k3 "third"))))
                      ;; assq finds by identity (eq), not name
                      (list (cdr (assq k1 table))
                            (cdr (assq k2 table))
                            (cdr (assq k3 table))
                            ;; Interned 'key won't match any
                            (assq 'key table))))"####;
    let expect = expect_test::expect![[r#""OK (\"first\" \"second\" \"third\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
