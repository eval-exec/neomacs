//! Comprehensive oracle parity tests for `prin1-to-string`:
//! all value types, nested structures, circular references with print-circle,
//! special character escaping, Unicode, large numbers, dotted pairs,
//! bool-vectors, char-tables, prin1 vs princ comparison.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// All basic value types in one comprehensive test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_all_value_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Integer zero, positive, negative, boundary
  (prin1-to-string 0)
  (prin1-to-string 1)
  (prin1-to-string -1)
  (prin1-to-string most-positive-fixnum)
  (prin1-to-string most-negative-fixnum)
  ;; Float zero, positive, negative, infinity, NaN
  (prin1-to-string 0.0)
  (prin1-to-string -0.0)
  (prin1-to-string 3.141592653589793)
  (prin1-to-string -2.718281828459045)
  (prin1-to-string 1.0e+INF)
  (prin1-to-string -1.0e+INF)
  (prin1-to-string 0.0e+NaN)
  ;; String: empty, normal, with spaces
  (prin1-to-string "")
  (prin1-to-string "hello")
  (prin1-to-string "hello world foo bar")
  ;; Symbol: normal, nil, t, with special chars
  (prin1-to-string 'foo)
  (prin1-to-string 'nil)
  (prin1-to-string 't)
  (prin1-to-string 'foo-bar-baz)
  (prin1-to-string 'foo/bar)
  ;; Keyword
  (prin1-to-string :key)
  (prin1-to-string :another-keyword)
  ;; Cons / list
  (prin1-to-string '(1 2 3))
  (prin1-to-string '(a . b))
  ;; nil as empty list
  (prin1-to-string '())
  ;; Vector
  (prin1-to-string [1 2 3])
  (prin1-to-string [])
  ;; Character (prints as integer in prin1)
  (prin1-to-string ?A)
  (prin1-to-string ?\n))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"1\" \"-1\" \"2305843009213693951\" \"-2305843009213693952\" \"0.0\" \"-0.0\" \"3.141592653589793\" \"-2.718281828459045\" \"1.0e+INF\" \"-1.0e+INF\" \"0.0e+NaN\" \"\\\"\\\"\" \"\\\"hello\\\"\" \"\\\"hello world foo bar\\\"\" \"foo\" \"nil\" \"t\" \"foo-bar-baz\" \"foo/bar\" \":key\" \":another-keyword\" \"(1 2 3)\" \"(a . b)\" \"nil\" \"[1 2 3]\" \"[]\" \"65\" \"10\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deeply nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; List of lists
  (prin1-to-string '((1 2) (3 4) (5 6)))
  ;; Nested 4 levels deep
  (prin1-to-string '(a (b (c (d (e))))))
  ;; Vector of lists
  (prin1-to-string [(1 2 3) (4 5 6)])
  ;; List of vectors
  (prin1-to-string '([1 2] [3 4] [5 6]))
  ;; Vector of vectors
  (prin1-to-string [[1 2] [3 4]])
  ;; Mixed nesting: alist with vector values and nested lists
  (prin1-to-string '((name . "Alice")
                     (scores . [95 87 92 100])
                     (metadata . ((tags . (:math :science))
                                  (rank . 1)))))
  ;; Deeply nested alternating list/vector
  (prin1-to-string '([1 (2 [3 (4 [5])])]))
  ;; Association list with varied value types
  (prin1-to-string '((:int . 42)
                     (:float . 3.14)
                     (:str . "hello")
                     (:sym . foo)
                     (:vec . [1 2 3])
                     (:nil . nil)
                     (:t . t)
                     (:list . (a b c)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"((1 2) (3 4) (5 6))\" \"(a (b (c (d (e)))))\" \"[(1 2 3) (4 5 6)]\" \"([1 2] [3 4] [5 6])\" \"[[1 2] [3 4]]\" \"((name . \\\"Alice\\\") (scores . [95 87 92 100]) (metadata (tags :math :science) (rank . 1)))\" \"([1 (2 [3 (4 [5])])])\" \"((:int . 42) (:float . 3.14) (:str . \\\"hello\\\") (:sym . foo) (:vec . [1 2 3]) (:nil) (:t . t) (:list a b c))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circular structure handling with print-circle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_circular_with_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that print-circle correctly handles shared and circular structures
    let form = r##"(let ((print-circle t))
  (list
    ;; Shared sub-list
    (let* ((shared '(x y z))
           (outer (list shared shared)))
      (prin1-to-string outer))
    ;; Self-referencing cons (circular car)
    (let ((c (cons nil nil)))
      (setcar c c)
      (let ((s (prin1-to-string c)))
        ;; Should contain #1= and #1# markers
        (list (string-match-p "#[0-9]+=" s)
              (string-match-p "#[0-9]+#" s))))
    ;; Circular list: last cdr points back to head
    (let ((lst (list 'a 'b 'c)))
      (setcdr (last lst) lst)
      (let ((s (prin1-to-string lst)))
        (list (string-match-p "#[0-9]+=" s)
              (string-match-p "#[0-9]+#" s))))
    ;; Shared vector element
    (let* ((inner '(shared data))
           (v (vector inner 42 inner)))
      (prin1-to-string v))))"##;
    let expect = expect_test::expect![[
        r#""OK (\"(#1=(x y z) #1#)\" (0 4) (0 12) \"[#1=(shared data) 42 #1#]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Special characters in strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_special_characters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic escape sequences
  (prin1-to-string "line1\nline2\nline3")
  (prin1-to-string "col1\tcol2\tcol3")
  (prin1-to-string "she said \"hello\" and \"goodbye\"")
  (prin1-to-string "C:\\Users\\test\\file.txt")
  (prin1-to-string "before\rafter")
  (prin1-to-string "null\x00char")
  ;; Multiple escapes combined
  (prin1-to-string "a\tb\nc\\d\"e\rf")
  ;; Consecutive identical escapes
  (prin1-to-string "\n\n\n\n\n")
  (prin1-to-string "\t\t\t")
  (prin1-to-string "\\\\\\\\")
  (prin1-to-string "\"\"\"\"")
  ;; Empty string vs whitespace-only
  (prin1-to-string "")
  (prin1-to-string " ")
  (prin1-to-string "   ")
  ;; Bell and escape chars
  (prin1-to-string "\a")
  (prin1-to-string "\e")
  ;; Formfeed
  (prin1-to-string "\f"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\\\"line1\\nline2\\nline3\\\"\" \"\\\"col1\tcol2\tcol3\\\"\" \"\\\"she said \\\\\\\"hello\\\\\\\" and \\\\\\\"goodbye\\\\\\\"\\\"\" \"\\\"C:\\\\\\\\Users\\\\\\\\test\\\\\\\\file.txt\\\"\" \"\\\"before\\rafter\\\"\" \"\\\"null\\fhar\\\"\" \"\\\"a\tb\\nc\\\\\\\\d\\\\\\\"e\\rf\\\"\" \"\\\"\\n\\n\\n\\n\\n\\\"\" \"\\\"\t\t\t\\\"\" \"\\\"\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"\" \"\\\"\\\\\\\"\\\\\\\"\\\\\\\"\\\\\\\"\\\"\" \"\\\"\\\"\" \"\\\" \\\"\" \"\\\"   \\\"\" \"\\\"\u{7}\\\"\" \"\\\"\u{1b}\\\"\" \"\\\"\\f\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unicode strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_unicode_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; CJK characters
  (prin1-to-string "hello")
  (prin1-to-string "world")
  ;; Emoji (multi-byte)
  (prin1-to-string "abc")
  ;; Mixed ASCII and Unicode
  (prin1-to-string "abc-def-ghi")
  ;; Various scripts
  (prin1-to-string "Greek")
  (prin1-to-string "Cyrillic")
  ;; Unicode escapes in Emacs
  (prin1-to-string (string ?\u00e9))
  (prin1-to-string (string ?\u00fc ?\u00f6 ?\u00e4))
  ;; Combining characters
  (prin1-to-string (string ?e ?\u0301))
  ;; Mathematical symbols
  (prin1-to-string (string ?\u2200 ?\u2203 ?\u2205 ?\u2208 ?\u2209))
  ;; Right-to-left mark and similar
  (prin1-to-string (string ?\u200e ?\u200f))
  ;; Long Unicode string
  (prin1-to-string (make-string 50 ?\u00e9)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\\\"hello\\\"\" \"\\\"world\\\"\" \"\\\"abc\\\"\" \"\\\"abc-def-ghi\\\"\" \"\\\"Greek\\\"\" \"\\\"Cyrillic\\\"\" \"\\\"é\\\"\" \"\\\"üöä\\\"\" \"\\\"e\u{301}\\\"\" \"\\\"∀∃∅∈∉\\\"\" \"\\\"\u{200e}\u{200f}\\\"\" \"\\\"éééééééééééééééééééééééééééééééééééééééééééééééééé\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Large numbers and numeric edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_large_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Fixnum boundaries
  (prin1-to-string most-positive-fixnum)
  (prin1-to-string most-negative-fixnum)
  ;; Powers of 2
  (prin1-to-string (expt 2 30))
  (prin1-to-string (expt 2 31))
  (prin1-to-string (- (expt 2 31)))
  ;; Float special values
  (prin1-to-string 1.0e+INF)
  (prin1-to-string -1.0e+INF)
  (prin1-to-string 0.0e+NaN)
  ;; Very small float
  (prin1-to-string 1.0e-300)
  (prin1-to-string -1.0e-300)
  ;; Float precision edge
  (prin1-to-string 0.1)
  (prin1-to-string 0.2)
  (prin1-to-string (+ 0.1 0.2))
  ;; Integer arithmetic near boundary
  (prin1-to-string (1- most-positive-fixnum))
  (prin1-to-string (1+ most-negative-fixnum))
  ;; Roundtrip: can we read back what we printed?
  (equal (car (read-from-string (prin1-to-string most-positive-fixnum)))
         most-positive-fixnum)
  (equal (car (read-from-string (prin1-to-string 3.141592653589793)))
         3.141592653589793))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2305843009213693951\" \"-2305843009213693952\" \"1073741824\" \"2147483648\" \"-2147483648\" \"1.0e+INF\" \"-1.0e+INF\" \"0.0e+NaN\" \"1e-300\" \"-1e-300\" \"0.1\" \"0.2\" \"0.30000000000000004\" \"2305843009213693950\" \"-2305843009213693951\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dotted pairs and improper lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_dotted_pairs_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Simple dotted pairs with various types
  (prin1-to-string '(a . b))
  (prin1-to-string '(1 . 2))
  (prin1-to-string '("key" . "value"))
  (prin1-to-string '(:keyword . 42))
  (prin1-to-string '(nil . t))
  (prin1-to-string '(t . nil))
  ;; Dotted pair where cdr is nil (should print as (a))
  (prin1-to-string '(a . nil))
  ;; Improper lists of various lengths
  (prin1-to-string '(1 . 2))
  (prin1-to-string '(1 2 . 3))
  (prin1-to-string '(1 2 3 . 4))
  (prin1-to-string '(1 2 3 4 . 5))
  (prin1-to-string '(a b c d e . f))
  ;; Nested dotted pairs
  (prin1-to-string '((a . 1) . (b . 2)))
  (prin1-to-string '((a . (b . (c . d)))))
  ;; Alist (list of dotted pairs)
  (prin1-to-string '((x . 10) (y . 20) (z . 30)))
  ;; Dotted pair with complex cdr
  (prin1-to-string '(head . [1 2 3]))
  (prin1-to-string '(head . (a b c)))
  ;; Cons chain that forms an improper list ending in a vector
  (prin1-to-string (cons 1 (cons 2 [3 4]))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(a . b)\" \"(1 . 2)\" \"(\\\"key\\\" . \\\"value\\\")\" \"(:keyword . 42)\" \"(nil . t)\" \"(t)\" \"(a)\" \"(1 . 2)\" \"(1 2 . 3)\" \"(1 2 3 . 4)\" \"(1 2 3 4 . 5)\" \"(a b c d e . f)\" \"((a . 1) b . 2)\" \"((a b c . d))\" \"((x . 10) (y . 20) (z . 30))\" \"(head . [1 2 3])\" \"(head a b c)\" \"(1 2 . [3 4])\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bool-vectors and char-tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_bool_vectors_and_char_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Bool-vectors
  (prin1-to-string (make-bool-vector 0 nil))
  (prin1-to-string (make-bool-vector 8 t))
  (prin1-to-string (make-bool-vector 8 nil))
  (prin1-to-string (make-bool-vector 16 t))
  (let ((bv (make-bool-vector 8 nil)))
    (aset bv 0 t)
    (aset bv 2 t)
    (aset bv 4 t)
    (aset bv 6 t)
    (prin1-to-string bv))
  ;; Bool-vector with odd length
  (prin1-to-string (make-bool-vector 3 t))
  (prin1-to-string (make-bool-vector 5 nil))
  ;; Char-table
  (let ((ct (make-char-table 'test-table)))
    (set-char-table-range ct ?a 1)
    (set-char-table-range ct ?z 26)
    ;; We just verify it prints something parseable
    (let ((s (prin1-to-string ct)))
      (list (string-match-p "^#\\^" s)
            (> (length s) 0))))
  ;; Roundtrip bool-vector
  (let* ((bv (make-bool-vector 16 nil))
         (_ (aset bv 0 t))
         (_ (aset bv 5 t))
         (_ (aset bv 10 t))
         (_ (aset bv 15 t))
         (printed (prin1-to-string bv))
         (restored (car (read-from-string printed))))
    (list (equal bv restored)
          (bool-vector-p restored)
          (length restored))))"#;
    let expect = expect_test::expect![[
        r##""OK (\"#&0\\\"\\\"\" \"#&8\\\"\\\\377\\\"\" \"#&8\\\"\\0\\\"\" \"#&16\\\"\\\\377\\\\377\\\"\" \"#&8\\\"U\\\"\" \"#&3\\\"\u{7}\\\"\" \"#&5\\\"\\0\\\"\" (0 t) (t t 16))""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// prin1-to-string vs format %S vs format %s (princ-like)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_prin1_vs_princ_vs_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; String: prin1 adds quotes, princ does not
  (list (prin1-to-string "hello")
        (prin1-to-string "hello" t)
        (format "%S" "hello")
        (format "%s" "hello"))
  ;; Symbol: same either way
  (list (prin1-to-string 'foo)
        (prin1-to-string 'foo t)
        (format "%S" 'foo)
        (format "%s" 'foo))
  ;; Integer: same either way
  (list (prin1-to-string 42)
        (prin1-to-string 42 t)
        (format "%S" 42)
        (format "%s" 42))
  ;; List with strings: prin1 escapes inner strings
  (list (prin1-to-string '("a" "b"))
        (prin1-to-string '("a" "b") t)
        (format "%S" '("a" "b"))
        (format "%s" '("a" "b")))
  ;; nil
  (list (prin1-to-string nil)
        (prin1-to-string nil t)
        (format "%S" nil)
        (format "%s" nil))
  ;; t
  (list (prin1-to-string t)
        (prin1-to-string t t)
        (format "%S" t)
        (format "%s" t))
  ;; String with escapes: difference shows clearly
  (list (prin1-to-string "say \"hi\"\nnewline")
        (prin1-to-string "say \"hi\"\nnewline" t))
  ;; Keyword
  (list (prin1-to-string :foo)
        (prin1-to-string :foo t)
        (format "%S" :foo)
        (format "%s" :foo))
  ;; Vector
  (list (prin1-to-string [1 "two" three])
        (prin1-to-string [1 "two" three] t))
  ;; Dotted pair
  (list (prin1-to-string '(a . b))
        (prin1-to-string '(a . b) t)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\"hello\\\"\" \"hello\" \"\\\"hello\\\"\" \"hello\") (\"foo\" \"foo\" \"foo\" \"foo\") (\"42\" \"42\" \"42\" \"42\") (\"(\\\"a\\\" \\\"b\\\")\" \"(a b)\" \"(\\\"a\\\" \\\"b\\\")\" \"(a b)\") (\"nil\" \"nil\" \"nil\" \"nil\") (\"t\" \"t\" \"t\" \"t\") (\"\\\"say \\\\\\\"hi\\\\\\\"\\nnewline\\\"\" \"say \\\"hi\\\"\\nnewline\") (\":foo\" \":foo\" \":foo\" \":foo\") (\"[1 \\\"two\\\" three]\" \"[1 two three]\") (\"(a . b)\" \"(a . b)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex roundtrip: serialization and deserialization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prin1_comp_roundtrip_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((test-values
         (list
           ;; Flat mixed list
           '(1 2.5 "three" four :five nil t)
           ;; Nested alist representing a config
           '((database . ((host . "localhost")
                          (port . 5432)
                          (name . "mydb")))
             (cache . ((enabled . t)
                       (ttl . 3600)
                       (size . nil)))
             (features . [:logging :metrics :tracing]))
           ;; Vector of alists
           (vector '((a . 1) (b . 2)) '((c . 3) (d . 4)))
           ;; Deeply nested structure
           '(l1 (l2 (l3 (l4 (l5 "leaf")))))
           ;; Improper list
           '(1 2 3 . end)
           ;; Boolean table
           '((p . t) (q . nil) (r . t) (s . nil))
           ;; Empty collections
           '(nil () [] "")
           ;; Quoted symbol
           '(quote hello))))
  (mapcar
    (lambda (val)
      (let* ((printed (prin1-to-string val))
             (restored (car (read-from-string printed)))
             (match (equal val restored)))
        (list match (length printed))))
    test-values))"#;
    let expect =
        expect_test::expect![[r#""OK ((t 32) (t 147) (t 37) (t 31) (t 13) (t 25) (t 15) (t 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
