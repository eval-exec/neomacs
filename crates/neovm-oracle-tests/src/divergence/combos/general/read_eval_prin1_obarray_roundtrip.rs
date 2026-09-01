//! Deep combo: read + eval + prin1 + print + obarray + intern roundtrips.
//! Tests reader/printer consistency with symbols and objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_prin1_to_string_read_roundtrip_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data '((a . 1) (b . 2) (c . (3 4 5)))))\n\
         (let ((printed (prin1-to-string data)))\n\
         (let ((re-read (read-from-string printed)))\n\
         (list printed (equal data (car re-read))))))",
        expect,
    );
}

#[test]
fn deficiency_print_with_newline_vs_prin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"(hello \\\"world\\\" 42 (nested list))\" \"\\n(hello \\\"world\\\" 42 (nested list))\\n\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((obj '(hello \"world\" 42 (nested list))))\n\
         (let ((p1 (prin1-to-string obj))\n\
         (p2 (with-output-to-string (print obj))))\n\
         (list p1 p2\n\
         (equal (read p1) (read p2))))))",
        expect,
    );
}

#[test]
fn deficiency_read_from_string_with_multiple_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b c) 42 \"hello\" (d . e))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((s \"(a b c) 42 \\\"hello\\\" (d . e)\")\n\
         (pos 0)\n\
         (objects nil))\n\
         (while (< pos (length s))\n\
         (let ((r (read-from-string s pos)))\n\
         (push (car r) objects)\n\
         (setq pos (cdr r))))\n\
         (nreverse objects)))",
        expect,
    );
}

#[test]
fn deficiency_prin1_special_strings_with_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((strings '(\"hello\\\"world\" \"back\\\\slash\" \"tab\\there\"\n\
         \"newline\\nhere\" \"\")))\n\
         (cl-loop for s in strings\n\
         collect (let ((printed (prin1-to-string s)))\n\
         (list s printed (equal s (read printed)))))))",
        expect,
    );
}

#[test]
fn deficiency_intern_soft_after_prin1_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((sym 'my-test-symbol))\n\
         (let ((printed (prin1-to-string sym)))\n\
         (let ((re-read (read printed)))\n\
         (list printed\n\
         (symbol-name re-read)\n\
         (eq sym re-read)\n\
         (eq sym (intern-soft printed))))))",
        expect,
    );
}

#[test]
fn deficiency_read_vector_and_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"([1 2 3] ((a) (b) (c)) [(x y) z])\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data (list [1 2 3] '((a) (b) (c))\n\
         (vector (list 'x 'y) 'z))))\n\
         (let ((printed (prin1-to-string data)))\n\
         (let ((re-read (read printed)))\n\
         (list printed (equal data re-read))))))",
        expect,
    );
}

#[test]
fn deficiency_format_vs_prin1_for_different_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"42\" \"\\\"hello\\\"\" \"symbol\" \"(1 2 3)\" \"[1 2 3]\" \"nil\" \"t\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (prin1-to-string 42)\n\
         (prin1-to-string \"hello\")\n\
         (prin1-to-string 'symbol)\n\
         (prin1-to-string '(1 2 3))\n\
         (prin1-to-string [1 2 3])\n\
         (prin1-to-string nil)\n\
         (prin1-to-string t)))",
        expect,
    );
}

#[test]
fn deficiency_read_with_intern_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (found missing new-sym new-sym)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0)))\n\
         (intern \"existing-sym\" ob)\n\
         (let ((s1 (intern-soft \"existing-sym\" ob))\n\
         (s2 (intern-soft \"new-sym\" ob)))\n\
         (list (if s1 'found 'missing)\n\
         (if s2 'found 'missing)\n\
         (intern \"new-sym\" ob)\n\
         (intern-soft \"new-sym\" ob)))))",
        expect,
    );
}

#[test]
fn deficiency_prin1_with_print_length_and_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((deep '(a (b (c (d (e (f))))))))\n\
         (let ((p1 (let ((print-level 3)) (prin1-to-string deep))))\n\
         (let ((long (cl-loop for i from 1 to 20 collect i)))\n\
         (let ((p2 (let ((print-length 5)) (prin1-to-string long))))\n\
         (list p1 p2)))))",
        expect,
    );
}

#[test]
fn deficiency_read_syntax_for_cons_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((cells '((a . b) (a b . c) (a b c . nil))))\n\
         (cl-loop for cell in cells\n\
         collect (let ((printed (prin1-to-string cell)))\n\
         (list cell printed (equal cell (read printed)))))))",
        expect,
    );
}
