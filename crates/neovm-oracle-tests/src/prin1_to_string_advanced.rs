//! Advanced oracle parity tests for `prin1-to-string`.
//!
//! Tests printing of various types, nested structures, dotted pairs,
//! hash tables, special character escaping, and roundtrip serialization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Print various primitive types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_primitive_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Integers: zero, positive, negative, large
  (prin1-to-string 0)
  (prin1-to-string 42)
  (prin1-to-string -999)
  (prin1-to-string 1000000000)
  ;; Floats: zero, positive, negative, scientific
  (prin1-to-string 0.0)
  (prin1-to-string 3.14159)
  (prin1-to-string -2.718)
  (prin1-to-string 1.5e10)
  (prin1-to-string -6.022e-23)
  ;; Strings
  (prin1-to-string "")
  (prin1-to-string "hello world")
  ;; Symbols and keywords
  (prin1-to-string 'foo-bar)
  (prin1-to-string 'nil)
  (prin1-to-string 't)
  (prin1-to-string :my-keyword)
  (prin1-to-string :another)
  ;; Characters (printed as integers by prin1)
  (prin1-to-string ?A)
  (prin1-to-string ?z)
  (prin1-to-string ?\n))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"42\" \"-999\" \"1000000000\" \"0.0\" \"3.14159\" \"-2.718\" \"15000000000.0\" \"-6.022e-23\" \"\\\"\\\"\" \"\\\"hello world\\\"\" \"foo-bar\" \"nil\" \"t\" \":my-keyword\" \":another\" \"65\" \"122\" \"10\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested lists and vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_nested_lists_and_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Simple list
  (prin1-to-string '(1 2 3))
  ;; Nested lists
  (prin1-to-string '((a b) (c d) (e f)))
  ;; Deeply nested
  (prin1-to-string '(1 (2 (3 (4 (5))))))
  ;; Mixed types in list
  (prin1-to-string '(1 "two" three 4.0 :five))
  ;; Empty list
  (prin1-to-string '())
  ;; Single-element list
  (prin1-to-string '(only))
  ;; Vectors
  (prin1-to-string [])
  (prin1-to-string [1 2 3])
  ;; Nested vectors
  (prin1-to-string [[1 2] [3 4]])
  ;; Lists inside vectors
  (prin1-to-string [(a b) (c d)])
  ;; Vectors inside lists
  (prin1-to-string '([1 2] [3 4]))
  ;; Complex nesting
  (prin1-to-string '((name . "Alice") (scores . [95 87 92]) (tags . (:math :science)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(1 2 3)\" \"((a b) (c d) (e f))\" \"(1 (2 (3 (4 (5)))))\" \"(1 \\\"two\\\" three 4.0 :five)\" \"nil\" \"(only)\" \"[]\" \"[1 2 3]\" \"[[1 2] [3 4]]\" \"[(a b) (c d)]\" \"([1 2] [3 4])\" \"((name . \\\"Alice\\\") (scores . [95 87 92]) (tags :math :science))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cons cells / dotted pairs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_dotted_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Simple dotted pair
  (prin1-to-string '(a . b))
  ;; Integer dotted pair
  (prin1-to-string '(1 . 2))
  ;; String in dotted pair
  (prin1-to-string '("key" . "value"))
  ;; Improper list (last cdr is not nil)
  (prin1-to-string '(1 2 . 3))
  (prin1-to-string '(a b c . d))
  ;; Nested dotted pairs
  (prin1-to-string '((a . 1) . (b . 2)))
  ;; Mixed: alist with dotted pairs
  (prin1-to-string '((x . 10) (y . 20) (z . 30)))
  ;; Dotted pair with nil
  (prin1-to-string '(a . nil))
  ;; Dotted pair with vector
  (prin1-to-string '(key . [1 2 3])))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(a . b)\" \"(1 . 2)\" \"(\\\"key\\\" . \\\"value\\\")\" \"(1 2 . 3)\" \"(a b c . d)\" \"((a . 1) b . 2)\" \"((x . 10) (y . 20) (z . 30))\" \"(a)\" \"(key . [1 2 3])\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_hash_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Hash table printing format: verify that prin1-to-string produces
    // readable output and that key-value contents are correct
    let form = r#"(let ((h (make-hash-table :test 'equal)))
  (puthash "name" "Alice" h)
  (puthash "age" 30 h)
  (let ((printed (prin1-to-string h)))
    (list
      ;; It should start with #s(hash-table
      (string-match-p "^#s(hash-table" printed)
      ;; It should contain the test type
      (string-match-p "equal" printed)
      ;; Verify the hash table has expected count
      (hash-table-count h)
      ;; Read it back and compare
      (let ((restored (car (read-from-string printed))))
        (list
          (hash-table-p restored)
          (gethash "name" restored)
          (gethash "age" restored))))))"#;
    let expect = expect_test::expect![[r#""OK (0 19 2 (t \"Alice\" 30))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Special characters in strings (escaping)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_special_char_escaping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Newline
  (prin1-to-string "line1\nline2")
  ;; Tab
  (prin1-to-string "col1\tcol2")
  ;; Double quote inside string
  (prin1-to-string "she said \"hello\"")
  ;; Backslash
  (prin1-to-string "path\\to\\file")
  ;; Carriage return
  (prin1-to-string "before\rafter")
  ;; Mixed special chars
  (prin1-to-string "a\nb\tc\\d\"e")
  ;; Consecutive special chars
  (prin1-to-string "\n\n\n")
  (prin1-to-string "\\\\\\\\")
  ;; String with only special chars
  (prin1-to-string "\t\n\"\\")
  ;; Unicode characters
  (prin1-to-string "hello")
  ;; Empty string
  (prin1-to-string ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\\\"line1\\nline2\\\"\" \"\\\"col1\tcol2\\\"\" \"\\\"she said \\\\\\\"hello\\\\\\\"\\\"\" \"\\\"path\\\\\\\\to\\\\\\\\file\\\"\" \"\\\"before\\rafter\\\"\" \"\\\"a\\nb\tc\\\\\\\\d\\\\\\\"e\\\"\" \"\\\"\\n\\n\\n\\\"\" \"\\\"\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"\" \"\\\"\t\\n\\\\\\\"\\\\\\\\\\\"\" \"\\\"hello\\\"\" \"\\\"\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// prin1-to-string with NOESCAPE argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_noescape_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // prin1-to-string takes optional NOESCAPE: when non-nil, acts like princ
    let form = r#"(list
  ;; With NOESCAPE=nil (default): readable output
  (prin1-to-string "hello" nil)
  ;; With NOESCAPE=t: human-readable, no quotes/escaping
  (prin1-to-string "hello" t)
  ;; Compare on string with special chars
  (prin1-to-string "say \"hi\"" nil)
  (prin1-to-string "say \"hi\"" t)
  ;; Symbols
  (prin1-to-string 'foo nil)
  (prin1-to-string 'foo t)
  ;; Lists
  (prin1-to-string '(1 "two" three) nil)
  (prin1-to-string '(1 "two" three) t)
  ;; Numbers are the same either way
  (string= (prin1-to-string 42 nil) (prin1-to-string 42 t))
  ;; nil and t
  (prin1-to-string nil nil)
  (prin1-to-string nil t))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\\\"hello\\\"\" \"hello\" \"\\\"say \\\\\\\"hi\\\\\\\"\\\"\" \"say \\\"hi\\\"\" \"foo\" \"foo\" \"(1 \\\"two\\\" three)\" \"(1 two three)\" t \"nil\" \"nil\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Roundtrip serialization: prin1-to-string -> read-from-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_roundtrip_complex_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build complex structures, serialize, deserialize, verify equality
    let form = r#"(let ((structures
         (list
           ;; Flat list of mixed types
           '(1 2.5 "three" four :five)
           ;; Nested alist
           '((a . ((b . 1) (c . 2))) (d . ((e . 3) (f . 4))))
           ;; Vector of lists
           ['(1 2 3) '(4 5 6) '(7 8 9)]
           ;; Deeply nested
           '(level1 (level2 (level3 (level4 "bottom"))))
           ;; Improper list
           '(1 2 3 . end)
           ;; Alist with string keys and vector values
           '(("row1" . [1 2 3]) ("row2" . [4 5 6]))
           ;; Quoted forms
           '(quote hello)
           ;; Boolean-like
           '(t nil t nil))))
  (mapcar
    (lambda (orig)
      (let* ((printed (prin1-to-string orig))
             (restored (car (read-from-string printed)))
             (match (equal orig restored)))
        (list match printed)))
    structures))"#;
    let expect = expect_test::expect![[
        r#""OK ((t \"(1 2.5 \\\"three\\\" four :five)\") (t \"((a (b . 1) (c . 2)) (d (e . 3) (f . 4)))\") (t \"['(1 2 3) '(4 5 6) '(7 8 9)]\") (t \"(level1 (level2 (level3 (level4 \\\"bottom\\\"))))\") (t \"(1 2 3 . end)\") (t \"((\\\"row1\\\" . [1 2 3]) (\\\"row2\\\" . [4 5 6]))\") (t \"'hello\") (t \"(t nil t nil)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build and serialize a record-like structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_adv_serialize_record_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a record system, serialize all records, deserialize, verify
    let form = r#"(progn
  (fset 'neovm--test-make-record
    (lambda (type &rest fields)
      (let ((rec (list type)))
        (while fields
          (setq rec (append rec (list (cons (car fields) (cadr fields)))))
          (setq fields (cddr fields)))
        rec)))

  (fset 'neovm--test-record-get
    (lambda (rec field)
      (cdr (assq field (cdr rec)))))

  (fset 'neovm--test-serialize-db
    (lambda (records)
      (prin1-to-string records)))

  (fset 'neovm--test-deserialize-db
    (lambda (str)
      (car (read-from-string str))))

  (unwind-protect
      (let* ((db (list
                   (funcall 'neovm--test-make-record 'person
                            'name "Alice" 'age 30 'score 95)
                   (funcall 'neovm--test-make-record 'person
                            'name "Bob" 'age 25 'score 87)
                   (funcall 'neovm--test-make-record 'person
                            'name "Carol" 'age 35 'score 92)))
             (serialized (funcall 'neovm--test-serialize-db db))
             (restored (funcall 'neovm--test-deserialize-db serialized)))
        (list
          ;; Roundtrip preserves structure
          (equal db restored)
          ;; Can query restored records
          (funcall 'neovm--test-record-get (nth 0 restored) 'name)
          (funcall 'neovm--test-record-get (nth 1 restored) 'age)
          (funcall 'neovm--test-record-get (nth 2 restored) 'score)
          ;; Compute aggregate from restored data
          (let ((total 0))
            (dolist (r restored)
              (setq total (+ total (funcall 'neovm--test-record-get r 'score))))
            total)
          ;; Number of records
          (length restored)))
    (fmakunbound 'neovm--test-make-record)
    (fmakunbound 'neovm--test-record-get)
    (fmakunbound 'neovm--test-serialize-db)
    (fmakunbound 'neovm--test-deserialize-db)))"#;
    let expect = expect_test::expect![[r#""OK (t \"Alice\" 25 92 274 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
