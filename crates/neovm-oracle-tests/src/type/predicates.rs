//! Oracle parity tests for type predicates: `booleanp`, `characterp`,
//! `char-or-string-p`,
//! `functionp`, `keywordp`, `nlistp`, `string-or-null-p`,
//! `integer-or-null-p`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// booleanp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_booleanp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(booleanp t)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(booleanp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(booleanp 0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(booleanp 1)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(booleanp 'hello)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(booleanp '())", expect);
}

#[test]
fn oracle_prop_booleanp_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Results of predicates should be booleanp
    let form = "(list (booleanp (= 1 1))
                      (booleanp (null nil))
                      (booleanp (not 42)))";
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// characterp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_characterp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(characterp ?a)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(characterp ?Z)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(characterp ?\\n)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(characterp 65)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(characterp 0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(characterp -1)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(characterp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(characterp "a")"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(characterp 'a)", expect);
}

#[test]
fn oracle_prop_characterp_large_codepoint() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    // Max valid Unicode codepoint
    crate::common::assert_oracle_parity_expect("(characterp #x10ffff)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    // Beyond max
    crate::common::assert_oracle_parity_expect("(characterp #x110000)", expect);
}

#[test]
fn oracle_prop_char_or_string_p_boundaries_and_arity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c:Fchar_or_string_p is true for valid character fixnums and
    // strings, false for every other object.  It uses GNU's full MAX_CHAR
    // range, not just Unicode scalar values.
    let form = r#"
(list
 (char-or-string-p 0)
 (char-or-string-p ?A)
 (char-or-string-p #x10ffff)
 (char-or-string-p (max-char))
 (char-or-string-p (1+ (max-char)))
 (char-or-string-p -1)
 (char-or-string-p "A")
 (char-or-string-p "")
 (char-or-string-p (string-as-unibyte "é"))
 (char-or-string-p nil)
 (char-or-string-p t)
 (char-or-string-p 1.0)
 (char-or-string-p 'A)
 (char-or-string-p '(65))
 (char-or-string-p [65])
 (condition-case err
     (char-or-string-p)
   (error (list (car err) (cdr err))))
 (condition-case err
     (char-or-string-p ?A "A")
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t nil nil t t t nil nil nil nil nil nil (wrong-number-of-arguments (char-or-string-p 0)) (wrong-number-of-arguments (char-or-string-p 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// functionp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functionp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(functionp 'car)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(functionp (lambda (x) x))", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(functionp #'car)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(functionp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(functionp 42)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(functionp '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(functionp "hello")"#, expect);
}

#[test]
fn oracle_prop_functionp_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 10))
                  (let ((f (lambda () x)))
                    (functionp f)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

// ---------------------------------------------------------------------------
// keywordp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keywordp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(keywordp :test)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(keywordp :hello)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(keywordp :)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(keywordp 'test)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(keywordp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(keywordp 42)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(keywordp ":test")"#, expect);
}

// ---------------------------------------------------------------------------
// nlistp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlistp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(nlistp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(nlistp '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(nlistp '(a . b))", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(nlistp 42)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(nlistp "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(nlistp [1 2 3])", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(nlistp t)", expect);
}

// ---------------------------------------------------------------------------
// string-or-null-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_or_null_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-or-null-p "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-or-null-p "")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(string-or-null-p nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(string-or-null-p 42)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(string-or-null-p 'hello)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(string-or-null-p t)", expect);
}

// ---------------------------------------------------------------------------
// list-of-strings-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_of_strings_p_subr_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:list-of-strings-p accepts nil and proper lists whose every
    // element is a string.  It returns nil, without signaling, for non-string
    // elements, dotted string tails, and non-list objects.
    let form = r#"
(list
 (list-of-strings-p nil)
 (list-of-strings-p '())
 (list-of-strings-p '("a" "b" ""))
 (list-of-strings-p '("a" 1 "b"))
 (list-of-strings-p '("a" . "tail"))
 (list-of-strings-p '("a" "b" . nil))
 (list-of-strings-p "not-a-list")
 (list-of-strings-p '(symbol))
 (let ((x (list "loop")))
   (setcdr x (list 1))
   (list-of-strings-p x)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// integer-or-null-p (called integer-or-marker-p in some contexts)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_integer_or_null_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(integer-or-null-p 42)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(integer-or-null-p 0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(integer-or-null-p -7)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(integer-or-null-p nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(integer-or-null-p 3.14)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(integer-or-null-p "42")"#, expect);
}

// ---------------------------------------------------------------------------
// Complex: type dispatch pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_dispatch_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a generic "describe" function using type predicates
    let form = r####"(let ((describe
                     (lambda (val)
                       (cond
                         ((null val) "null")
                         ((booleanp val) "boolean")
                         ((integerp val) (format "int:%d" val))
                         ((floatp val) "float")
                         ((stringp val)
                          (format "str:%d" (length val)))
                         ((keywordp val) "keyword")
                         ((symbolp val) "symbol")
                         ((functionp val) "function")
                         ((vectorp val)
                          (format "vec:%d" (length val)))
                         ((consp val)
                          (format "cons:%d" (length val)))
                         (t "unknown")))))
                    (list (funcall describe nil)
                          (funcall describe t)
                          (funcall describe 42)
                          (funcall describe 3.14)
                          (funcall describe "hello")
                          (funcall describe :test)
                          (funcall describe 'foo)
                          (funcall describe (lambda () nil))
                          (funcall describe [1 2 3])
                          (funcall describe '(a b c))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"null\" \"boolean\" \"int:42\" \"float\" \"str:5\" \"keyword\" \"symbol\" \"function\" \"vec:3\" \"cons:3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_type_coercion_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Type-safe conversion pipeline
    let form = r####"(let ((to-string
                     (lambda (val)
                       (cond
                         ((stringp val) val)
                         ((numberp val) (number-to-string val))
                         ((symbolp val) (symbol-name val))
                         ((null val) "nil")
                         (t (prin1-to-string val))))))
                    (let ((values '(42 3.14 hello nil "already" (1 2) [3 4])))
                      (mapcar to-string values)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"3.14\" \"hello\" \"nil\" \"already\" \"(1 2)\" \"[3 4]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_type_predicate_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Every value should match exactly one primary type
    let form = r####"(let ((classify
                     (lambda (val)
                       (let ((types nil))
                         (when (integerp val) (setq types (cons 'integer types)))
                         (when (floatp val) (setq types (cons 'float types)))
                         (when (stringp val) (setq types (cons 'string types)))
                         (when (symbolp val) (setq types (cons 'symbol types)))
                         (when (consp val) (setq types (cons 'cons types)))
                         (when (vectorp val) (setq types (cons 'vector types)))
                         types))))
                    (list (funcall classify 42)
                          (funcall classify 3.14)
                          (funcall classify "hi")
                          (funcall classify 'foo)
                          (funcall classify '(1 2))
                          (funcall classify [1 2])
                          (funcall classify nil)
                          (funcall classify t)))"####;
    let expect = expect_test::expect![[
        r#""OK ((integer) (float) (string) (symbol) (cons) (vector) (symbol) (symbol))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
