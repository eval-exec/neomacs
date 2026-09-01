//! Oracle parity tests for advanced `read-from-string` and `prin1-to-string`
//! patterns: START parameter, sequential reads, all data types,
//! special characters, format differences, and roundtrip serialization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// read-from-string with START parameter and sequential reads
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_from_string_start_offsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read from various offsets within a multi-form string
    let form = r####"(let ((s "  42 hello (1 2) \"str\""))
                    (list
                     ;; Read from start (skip leading spaces)
                     (car (read-from-string s))
                     ;; Read from offset 4 (the 'hello' symbol)
                     (car (read-from-string s 4))
                     ;; Read from offset 10 (the list)
                     (car (read-from-string s 10))
                     ;; Read from offset 16 (the string)
                     (car (read-from-string s 16))
                     ;; Verify returned positions
                     (cdr (read-from-string s 0))
                     (cdr (read-from-string s 4))
                     (cdr (read-from-string s 10))))"####;
    let expect = expect_test::expect![[r#""OK (42 hello (1 2) \"str\" 4 10 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_read_from_string_sequential_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use the returned index to sequentially read multiple forms
    let form = r####"(let ((s "(+ 1 2) (* 3 4) (list 'a 'b 'c)")
                        (pos 0)
                        (forms nil))
                    (condition-case nil
                        (while t
                          (let ((result (read-from-string s pos)))
                            (setq forms (cons (car result) forms))
                            (setq pos (cdr result))))
                      (error nil))
                    (list (length forms) (nreverse forms)))"####;
    let expect = expect_test::expect![[r#""OK (3 ((+ 1 2) (* 3 4) (list 'a 'b 'c)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_read_from_string_all_data_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read every major data type from string
    let form = r####"(let ((types (list
                          (car (read-from-string "42"))
                          (car (read-from-string "-17"))
                          (car (read-from-string "3.14"))
                          (car (read-from-string "-0.5e2"))
                          (car (read-from-string "hello"))
                          (car (read-from-string ":keyword"))
                          (car (read-from-string "\"a string\""))
                          (car (read-from-string "?A"))
                          (car (read-from-string "?\\n"))
                          (car (read-from-string "nil"))
                          (car (read-from-string "t"))
                          (car (read-from-string "(1 . 2)"))
                          (car (read-from-string "(a b c)"))
                          (car (read-from-string "[1 2 3]"))
                          (car (read-from-string "'quoted"))
                          (car (read-from-string "#'symbol-function")))))
                    (mapcar #'type-of types))"####;
    let expect = expect_test::expect![[
        r#""OK (integer integer float float symbol symbol string integer integer symbol symbol cons cons vector cons cons)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// prin1-to-string with special characters and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_to_string_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Strings with special characters must be properly escaped
    let form = r####"(list
                    (prin1-to-string "hello\nworld")
                    (prin1-to-string "tab\there")
                    (prin1-to-string "back\\slash")
                    (prin1-to-string "double\"quote")
                    (prin1-to-string "null\x00byte")
                    (prin1-to-string (concat "mixed\n\t\"special\\"))
                    ;; Verify roundtrip: read what we printed
                    (equal "hello\nworld"
                           (car (read-from-string
                                  (prin1-to-string "hello\nworld")))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"\\\"hello\\nworld\\\"\" \"\\\"tab\there\\\"\" \"\\\"back\\\\\\\\slash\\\"\" \"\\\"double\\\\\\\"quote\\\"\" \"\\\"null\u{b}yte\\\"\" \"\\\"mixed\\n\t\\\\\\\"special\\\\\\\\\\\"\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_prin1_vs_format_differences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare prin1-to-string, format %S, format %s
    let form = r####"(let ((vals (list nil t 42 3.14 "hello" 'foo
                                    '(1 2 3) [4 5 6] '(a . b))))
                    (mapcar
                     (lambda (v)
                       (list
                        (prin1-to-string v)        ;; readable, quoted
                        (format "%S" v)            ;; same as prin1
                        (format "%s" v)            ;; human-readable, no quotes
                        ;; prin1 and %S should agree
                        (string= (prin1-to-string v) (format "%S" v))))
                     vals))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"nil\" \"nil\" \"nil\" t) (\"t\" \"t\" \"t\" t) (\"42\" \"42\" \"42\" t) (\"3.14\" \"3.14\" \"3.14\" t) (\"\\\"hello\\\"\" \"\\\"hello\\\"\" \"hello\" t) (\"foo\" \"foo\" \"foo\" t) (\"(1 2 3)\" \"(1 2 3)\" \"(1 2 3)\" t) (\"[4 5 6]\" \"[4 5 6]\" \"[4 5 6]\" t) (\"(a . b)\" \"(a . b)\" \"(a . b)\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_prin1_to_string_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive type coverage for prin1-to-string
    let form = r####"(list
                    ;; nil and t
                    (prin1-to-string nil)
                    (prin1-to-string t)
                    ;; Integers
                    (prin1-to-string 0)
                    (prin1-to-string -1)
                    (prin1-to-string 999999)
                    ;; Floats
                    (prin1-to-string 0.0)
                    (prin1-to-string 1.5)
                    (prin1-to-string -2.5e10)
                    ;; Strings
                    (prin1-to-string "")
                    (prin1-to-string "hello")
                    ;; Symbols
                    (prin1-to-string 'foo)
                    (prin1-to-string :bar)
                    ;; Lists
                    (prin1-to-string '())
                    (prin1-to-string '(1))
                    (prin1-to-string '(1 2 3))
                    (prin1-to-string '(a . b))
                    (prin1-to-string '(1 2 . 3))
                    ;; Vectors
                    (prin1-to-string [])
                    (prin1-to-string [1 2 3])
                    ;; Nested structures
                    (prin1-to-string '((a . 1) (b . (2 3)) (c . [4 5])))
                    ;; Characters
                    (prin1-to-string ?A)
                    (prin1-to-string ?\n))"####;
    let expect = expect_test::expect![[
        r#""OK (\"nil\" \"t\" \"0\" \"-1\" \"999999\" \"0.0\" \"1.5\" \"-25000000000.0\" \"\\\"\\\"\" \"\\\"hello\\\"\" \"foo\" \":bar\" \"nil\" \"(1)\" \"(1 2 3)\" \"(a . b)\" \"(1 2 . 3)\" \"[]\" \"[1 2 3]\" \"((a . 1) (b 2 3) (c . [4 5]))\" \"65\" \"10\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: read-eval-print patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_eval_print_loop_with_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A REPL that handles evaluation errors gracefully
    let form = r####"(let ((input "(+ 1 2) (/ 10 0) (* 3 4) (car 1) (- 8 3)")
                        (pos 0)
                        (results nil))
                    (condition-case nil
                        (while t
                          (let ((parsed (read-from-string input pos)))
                            (setq pos (cdr parsed))
                            (condition-case err
                                (setq results
                                      (cons (list 'ok (eval (car parsed)))
                                            results))
                              (error
                               (setq results
                                     (cons (list 'err (car err))
                                           results))))))
                      (error nil))
                    (nreverse results))"####;
    let expect = expect_test::expect![[
        r#""OK ((ok 3) (err arith-error) (ok 12) (err wrong-type-argument) (ok 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_serialization_roundtrip_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Roundtrip complex nested structures through prin1/read
    let form = r####"(let ((structures
                         (list
                          nil
                          t
                          42
                          "hello world"
                          '(a b c)
                          '(1 . 2)
                          '((a . 1) (b . 2) (c . 3))
                          [1 "two" three]
                          '(nested (list "with" (various . types)) [and vectors])
                          '(1 2 . 3))))
                    (mapcar
                     (lambda (orig)
                       (let* ((printed (prin1-to-string orig))
                              (restored (car (read-from-string printed))))
                         (list (equal orig restored) printed)))
                     structures))"####;
    let expect = expect_test::expect![[
        r#""OK ((t \"nil\") (t \"t\") (t \"42\") (t \"\\\"hello world\\\"\") (t \"(a b c)\") (t \"(1 . 2)\") (t \"((a . 1) (b . 2) (c . 3))\") (t \"[1 \\\"two\\\" three]\") (t \"(nested (list \\\"with\\\" (various . types)) [and vectors])\") (t \"(1 2 . 3)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build data structures by reading from strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_build_alist_from_kv_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a sequence of key-value pairs from a string representation
    let form = r####"(let ((kv-string "(:name \"Alice\" :age 30 :scores (95 87 92) :active t)")
                        (parsed (car (read-from-string
                                       "(:name \"Alice\" :age 30 :scores (95 87 92) :active t)"))))
                    ;; Convert flat plist to alist
                    (let ((result nil)
                          (remaining parsed))
                      (while remaining
                        (let ((key (car remaining))
                              (val (cadr remaining)))
                          (setq result (cons (cons key val) result))
                          (setq remaining (cddr remaining))))
                      (let ((alist (nreverse result)))
                        (list
                         (cdr (assq :name alist))
                         (cdr (assq :age alist))
                         (cdr (assq :scores alist))
                         (cdr (assq :active alist))
                         ;; Roundtrip the alist
                         (equal alist
                                (car (read-from-string
                                       (prin1-to-string alist))))))))"####;
    let expect = expect_test::expect![[r#""OK (\"Alice\" 30 (95 87 92) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
