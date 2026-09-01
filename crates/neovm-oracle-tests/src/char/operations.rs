//! Oracle parity tests for character operations: `char-syntax`,
//! `insert-char`, `char-equal`, `char-width`, `characterp`,
//! `max-char`, `multibyte-char-p`, `unibyte-char-p`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// char-syntax
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_syntax_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Standard syntax table entries
    let form = r#"(list (char-syntax ?a)   ;; word
                        (char-syntax ?1)   ;; word
                        (char-syntax ?\ )  ;; whitespace
                        (char-syntax ?\()  ;; open paren
                        (char-syntax ?\))  ;; close paren
                        (char-syntax ?.)   ;; punctuation
                        (char-syntax ?+))"#;
    let expect = expect_test::expect![r#""OK (119 119 32 40 41 95 95)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_char_syntax_classify_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classify each char of a string by its syntax
    let form = r#"(let ((s "(defun foo (x) (+ x 1))")
                        (result nil))
                    (let ((i 0))
                      (while (< i (length s))
                        (setq result
                              (cons (char-syntax (aref s i))
                                    result))
                        (setq i (1+ i))))
                    (concat (nreverse result)))"#;
    let expect = expect_test::expect![[r#""OK \"(wwwww www (w) (_ w w))\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_char_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert-char ?a 5)
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"aaaaa\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_char_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert-char ?- 20)
                    (insert "\n")
                    (insert-char ?* 10)
                    (insert "\n")
                    (insert-char ?= 15)
                    (buffer-string))"#;
    let expect =
        expect_test::expect![[r#""OK \"--------------------\\n**********\\n===============\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_char_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert-char ?é 3)
                    (insert " ")
                    (insert-char ?日 2)
                    (list (buffer-string)
                          (buffer-size)
                          (length (buffer-string))))"#;
    let expect = expect_test::expect![[r#""OK (\"ééé 日日\" 6 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_char_zero_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "before")
                    (insert-char ?x 0)
                    (insert "after")
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"beforeafter\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-equal (case-sensitive and case-insensitive)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_equal_case_sensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (char-equal ?a ?a)
                        (char-equal ?A ?A)
                        (char-equal ?a ?b)
                        (char-equal ?0 ?0))"#;
    let expect = expect_test::expect![r#""OK (t t nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_char_equal_case_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With case-fold-search bound to t, char-equal is case-insensitive
    let form = r#"(let ((case-fold-search t))
                    (list (char-equal ?a ?A)
                          (char-equal ?Z ?z)
                          (char-equal ?a ?b)
                          (char-equal ?1 ?1)))"#;
    let expect = expect_test::expect![r#""OK (t t nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_char_equal_case_sensitive_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With case-fold-search nil, case matters
    let form = r#"(let ((case-fold-search nil))
                    (list (char-equal ?a ?A)
                          (char-equal ?a ?a)
                          (char-equal ?Z ?z)))"#;
    let expect = expect_test::expect![r#""OK (nil t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (char-width ?a)
                        (char-width ?\ )
                        (char-width ?\t)
                        (char-width ?日)
                        (char-width ?é))"#;
    let expect = expect_test::expect![r#""OK (1 1 8 2 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// characterp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_characterp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (characterp ?a)
                        (characterp 65)
                        (characterp 0)
                        (characterp -1)
                        (characterp nil)
                        (characterp "a")
                        (characterp 'a))"#;
    let expect = expect_test::expect![r#""OK (t t t nil nil nil nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// max-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (integerp (max-char))
                        (> (max-char) 0)
                        (characterp (max-char)))"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: character classifier
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_operations_classifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classify characters in a string by multiple properties
    let form = r#"(let ((s "Hello, World! 123 café")
                        (classes nil))
                    (let ((i 0))
                      (while (< i (length s))
                        (let* ((c (aref s i))
                               (syn (char-syntax c))
                               (cls (cond
                                     ((= syn ?w) 'word)
                                     ((= syn ?\ ) 'space)
                                     ((= syn ?.) 'punct)
                                     ((= syn ?\() 'open)
                                     ((= syn ?\)) 'close)
                                     (t 'other))))
                          (if (and classes
                                   (eq (caar classes) cls))
                              ;; Extend current run
                              (setcdr (car classes)
                                      (1+ (cdar classes)))
                            ;; New run
                            (setq classes
                                  (cons (cons cls 1) classes))))
                        (setq i (1+ i))))
                    (nreverse classes))"#;
    let expect = expect_test::expect![
        r#""OK ((word . 5) (other . 1) (space . 1) (word . 5) (other . 1) (space . 1) (word . 3) (space . 1) (word . 4))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: ROT13 using char operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_operations_rot13() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((input "Hello, World! 42"))
                    (let ((result (make-string (length input) ?\ ))
                          (i 0))
                      (while (< i (length input))
                        (let* ((c (aref input i))
                               (rotated
                                (cond
                                 ((and (>= c ?a) (<= c ?z))
                                  (+ ?a (% (+ (- c ?a) 13) 26)))
                                 ((and (>= c ?A) (<= c ?Z))
                                  (+ ?A (% (+ (- c ?A) 13) 26)))
                                 (t c))))
                          (aset result i rotated))
                        (setq i (1+ i)))
                      ;; ROT13 twice should give back original
                      (let ((double (make-string (length result) ?\ ))
                            (j 0))
                        (while (< j (length result))
                          (let* ((c (aref result j))
                                 (rotated
                                  (cond
                                   ((and (>= c ?a) (<= c ?z))
                                    (+ ?a (% (+ (- c ?a) 13) 26)))
                                   ((and (>= c ?A) (<= c ?Z))
                                    (+ ?A (% (+ (- c ?A) 13) 26)))
                                   (t c))))
                            (aset double j rotated))
                          (setq j (1+ j)))
                        (list result
                              (string= double input)))))"#;
    let expect = expect_test::expect![[r#""OK (\"Uryyb, Jbeyq! 42\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build char-table frequency map
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_operations_frequency_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count char frequencies and return sorted by frequency descending
    let form = r#"(let ((text "the quick brown fox jumps over the lazy dog"))
                    (let ((freq (make-hash-table))
                          (i 0))
                      ;; Count frequencies (skip spaces)
                      (while (< i (length text))
                        (let ((c (aref text i)))
                          (unless (= c ?\ )
                            (puthash c (1+ (gethash c freq 0)) freq)))
                        (setq i (1+ i)))
                      ;; Collect and sort
                      (let ((pairs nil))
                        (maphash (lambda (k v)
                                   (setq pairs (cons (cons k v) pairs)))
                                 freq)
                        (setq pairs
                              (sort pairs
                                    (lambda (a b)
                                      (or (> (cdr a) (cdr b))
                                          (and (= (cdr a) (cdr b))
                                               (< (car a) (car b)))))))
                        ;; Return top 5 as (char . count)
                        (let ((result nil) (n 0))
                          (while (and pairs (< n 5))
                            (let ((p (car pairs)))
                              (setq result
                                    (cons (cons (char-to-string (car p))
                                                (cdr p))
                                          result)))
                            (setq pairs (cdr pairs)
                                  n (1+ n)))
                          (nreverse result)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"o\" . 4) (\"e\" . 3) (\"h\" . 2) (\"r\" . 2) (\"t\" . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
