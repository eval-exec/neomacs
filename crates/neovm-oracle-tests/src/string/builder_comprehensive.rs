//! Oracle parity tests for comprehensive string building patterns.
//!
//! Tests `concat` with many args, `make-string` with init char, `string`
//! from char codes, `format` as string builder, `mapconcat` for joining,
//! `substring` extraction, `string-join` (from subr-x), building strings
//! in loops via push+nreverse+mapconcat, `with-output-to-string`+`princ`/`prin1`,
//! and char-by-char construction patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Test 1: concat with many arguments, mixed types, edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_concat_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; No arguments
  (concat)
  ;; Single string
  (concat "hello")
  ;; Two strings
  (concat "foo" "bar")
  ;; Many strings
  (concat "a" "b" "c" "d" "e" "f" "g" "h")
  ;; Empty strings interspersed
  (concat "" "x" "" "y" "" "" "z" "")
  ;; All empty
  (concat "" "" "")
  ;; With vectors (char sequences)
  (concat [65 66 67])
  ;; Mixed string and list of chars
  (concat "hello " '(119 111 114 108 100))
  ;; Concat with nil (treated as empty sequence)
  (concat "abc" nil "def")
  ;; Large concatenation
  (length (concat "abcdefghij" "klmnopqrst" "uvwxyz"))
  ;; Unicode strings
  (concat "\u00e9" "l\u00e8ve"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"hello\" \"foobar\" \"abcdefgh\" \"xyz\" \"\" \"ABC\" \"hello world\" \"abcdef\" 26 \"élève\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 2: make-string with various lengths and init characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_make_string_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Zero length
  (make-string 0 ?x)
  ;; Single char
  (make-string 1 ?A)
  ;; Multiple identical chars
  (make-string 5 ?*)
  ;; Space padding
  (make-string 10 ?\s)
  ;; Null character
  (make-string 3 0)
  ;; Numeric character
  (make-string 4 ?0)
  ;; Length verification
  (length (make-string 20 ?z))
  ;; Content verification: all chars equal
  (let ((s (make-string 6 ?Q)))
    (and (= (aref s 0) ?Q)
         (= (aref s 3) ?Q)
         (= (aref s 5) ?Q)))
  ;; make-string + concat for padding
  (concat (make-string 3 ?.) "hello" (make-string 3 ?.))
  ;; Right-pad a short string to fixed width
  (let* ((s "hi")
         (pad (- 10 (length s))))
    (concat s (make-string pad ?\s))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"A\" \"*****\" \"          \" \"\\0\\0\\0\" \"0000\" 20 t \"...hello...\" \"hi        \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 3: string from char codes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_string_from_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Single char
  (string ?A)
  ;; Multiple chars
  (string ?h ?e ?l ?l ?o)
  ;; Numeric chars for digits
  (string ?1 ?2 ?3)
  ;; Special chars
  (string ?( ?) ?[ ?])
  ;; Build from char codes via mapconcat
  (mapconcat #'string '(?w ?o ?r ?l ?d) "")
  ;; Char arithmetic: build "ABC" from ?A base
  (string ?A (1+ ?A) (+ ?A 2))
  ;; Build lowercase alphabet prefix
  (mapconcat (lambda (i) (string (+ ?a i))) (number-sequence 0 4) "")
  ;; Char identity roundtrip
  (= (string-to-char (string ?Z)) ?Z)
  ;; string-to-char of multi-char string returns first char
  (= (string-to-char "hello") ?h)
  ;; Empty string from no chars yields ""
  (string))"#;
    let expect = expect_test::expect![[
        r#""OK (\"A\" \"hello\" \"123\" \"()[]\" \"world\" \"ABC\" \"abcde\" t t \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 4: format as a string builder — all format directives
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_format_directives() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; %d integer
  (format "%d" 42)
  ;; %s string
  (format "%s" "hello")
  ;; %S prin1 representation
  (format "%S" "hello")
  ;; %f float
  (format "%.2f" 3.14159)
  ;; %x hexadecimal
  (format "%x" 255)
  ;; %o octal
  (format "%o" 8)
  ;; %c character
  (format "%c" 65)
  ;; %% literal percent
  (format "100%%")
  ;; Multiple format args
  (format "Name: %s, Age: %d, Score: %.1f" "Alice" 30 95.5)
  ;; Width specifiers
  (format "[%10d]" 42)
  (format "[%-10d]" 42)
  ;; Zero padding
  (format "%05d" 42)
  ;; Combining format calls to build complex strings
  (let ((header (format "=== %s ===" "Report"))
        (line1 (format "  %-12s %6d" "Apples" 42))
        (line2 (format "  %-12s %6d" "Oranges" 107)))
    (mapconcat #'identity (list header line1 line2) "\n")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"hello\" \"\\\"hello\\\"\" \"3.14\" \"ff\" \"10\" \"A\" \"100%\" \"Name: Alice, Age: 30, Score: 95.5\" \"[        42]\" \"[42        ]\" \"00042\" \"=== Report ===\\n  Apples           42\\n  Oranges         107\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 5: mapconcat for joining sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_mapconcat_joining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic join with separator
  (mapconcat #'identity '("a" "b" "c") ", ")
  ;; Join with empty separator
  (mapconcat #'identity '("x" "y" "z") "")
  ;; Join with newline
  (mapconcat #'identity '("line1" "line2" "line3") "\n")
  ;; Transform + join: numbers to strings
  (mapconcat #'number-to-string '(1 2 3 4 5) "-")
  ;; Symbols to strings
  (mapconcat #'symbol-name '(foo bar baz) "::")
  ;; Empty list
  (mapconcat #'identity nil ",")
  ;; Single element
  (mapconcat #'identity '("only") ",")
  ;; Join with multi-char separator
  (mapconcat #'identity '("a" "b" "c") " | ")
  ;; Complex transformation
  (mapconcat (lambda (n) (format "(%d=%s)" n (make-string n ?#)))
             '(1 2 3 4) " ")
  ;; Join chars from a vector
  (mapconcat (lambda (c) (string c)) [?H ?i ?!] " "))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a, b, c\" \"xyz\" \"line1\\nline2\\nline3\" \"1-2-3-4-5\" \"foo::bar::baz\" \"\" \"only\" \"a | b | c\" \"(1=#) (2=##) (3=###) (4=####)\" \"H i !\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 6: substring extraction — all parameter forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_substring_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s "Hello, World!"))
  (list
    ;; Full string
    (substring s)
    ;; From start index
    (substring s 7)
    ;; From start to end
    (substring s 0 5)
    ;; Middle slice
    (substring s 7 12)
    ;; Negative indices (from end)
    (substring s -6)
    (substring s -6 -1)
    ;; Zero-length substring
    (substring s 3 3)
    ;; Single character
    (substring s 0 1)
    ;; Last character
    (substring s -1)
    ;; Combine: extract, transform, rejoin
    (let* ((first (substring s 0 5))
           (rest (substring s 7))
           (upper (upcase first)))
      (concat upper " " rest))
    ;; substring-no-properties equivalent (no text props to strip here)
    (substring-no-properties "plain text" 0 5)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, World!\" \"World!\" \"Hello\" \"World\" \"World!\" \"World\" \"\" \"H\" \"!\" \"HELLO World!\" \"plain\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_builder_substring_bignum_index_errors_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c validates substring indexes through validate_subarray, which
    // uses CHECK_FIXNUM rather than CHECK_INTEGER.  Bignum FROM/TO arguments
    // therefore signal wrong-type-argument fixnump before range clipping.
    let form = r#"(let ((big (ash 1 100))
      (s "abcdef"))
  (list (condition-case err
            (substring s big)
          (error (list (car err) (cadr err))))
        (condition-case err
            (substring s 0 big)
          (error (list (car err) (cadr err))))
        (condition-case err
            (substring [a b c] big)
          (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument integerp) (wrong-type-argument integerp) (wrong-type-argument integerp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 7: string-join from subr-x
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_string_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; Basic join
    (string-join '("foo" "bar" "baz") ", ")
    ;; Single string
    (string-join '("alone") "-")
    ;; Empty list
    (string-join nil "/")
    ;; No separator (defaults to "")
    (string-join '("a" "b" "c"))
    ;; Newline separator
    (string-join '("alpha" "beta" "gamma") "\n")
    ;; Empty strings in list
    (string-join '("" "x" "" "y" "") ".")
    ;; Tab separator
    (string-join '("col1" "col2" "col3") "\t")
    ;; Build CSV-like row
    (string-join (mapcar #'number-to-string '(1 2 3 4 5)) ",")
    ;; Build path-like string
    (string-join '("usr" "local" "bin") "/")
    ;; Nested join for 2D table
    (mapconcat
     (lambda (row) (string-join (mapcar #'number-to-string row) "\t"))
     '((1 2 3) (4 5 6) (7 8 9))
     "\n")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"foo, bar, baz\" \"alone\" \"\" \"abc\" \"alpha\\nbeta\\ngamma\" \".x..y.\" \"col1\tcol2\tcol3\" \"1,2,3,4,5\" \"usr/local/bin\" \"1\t2\t3\\n4\t5\t6\\n7\t8\t9\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 8: Building strings in loops via push+nreverse+mapconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_loop_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Collect formatted strings then join
  (let ((parts nil))
    (dotimes (i 6)
      (push (format "item-%d" i) parts))
    (mapconcat #'identity (nreverse parts) ", "))
  ;; Build with while loop
  (let ((parts nil) (n 1))
    (while (<= n 5)
      (push (number-to-string (* n n)) parts)
      (setq n (1+ n)))
    (mapconcat #'identity (nreverse parts) " + "))
  ;; Accumulate via dolist
  (let ((words '("the" "quick" "brown" "fox"))
        (result nil))
    (dolist (w words)
      (push (concat "[" (upcase w) "]") result))
    (mapconcat #'identity (nreverse result) " "))
  ;; FizzBuzz via loop + push
  (let ((parts nil))
    (dotimes (i 15)
      (let ((n (1+ i)))
        (push (cond ((= (mod n 15) 0) "FizzBuzz")
                    ((= (mod n 3) 0) "Fizz")
                    ((= (mod n 5) 0) "Buzz")
                    (t (number-to-string n)))
              parts)))
    (mapconcat #'identity (nreverse parts) " "))
  ;; Recursive string building
  (let ((repeat-str nil))
    (setq repeat-str
          (lambda (s n)
            (if (<= n 0) ""
              (concat s (funcall repeat-str s (1- n))))))
    (funcall repeat-str "ab" 4)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"item-0, item-1, item-2, item-3, item-4, item-5\" \"1 + 4 + 9 + 16 + 25\" \"[THE] [QUICK] [BROWN] [FOX]\" \"1 2 Fizz 4 Buzz Fizz 7 8 Fizz Buzz 11 Fizz 13 14 FizzBuzz\" \"abababab\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 9: with-output-to-string + princ/prin1
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_with_output_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Simple princ
  (with-output-to-string (princ "hello"))
  ;; princ of number
  (with-output-to-string (princ 42))
  ;; Multiple princ calls
  (with-output-to-string
    (princ "Name: ")
    (princ "Alice")
    (princ ", Age: ")
    (princ 30))
  ;; prin1 adds quotes for strings
  (with-output-to-string (prin1 "quoted"))
  ;; prin1 for symbols
  (with-output-to-string (prin1 'my-symbol))
  ;; prin1 for lists
  (with-output-to-string (prin1 '(1 2 3)))
  ;; Mix princ and prin1
  (with-output-to-string
    (princ "data: ")
    (prin1 '(a b c))
    (princ " end"))
  ;; prin1-to-string as alternative
  (prin1-to-string '(1 "two" three))
  ;; Loop inside with-output-to-string
  (with-output-to-string
    (let ((i 0))
      (while (< i 5)
        (when (> i 0) (princ ", "))
        (princ (* i i))
        (setq i (1+ i))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"42\" \"Name: Alice, Age: 30\" \"\\\"quoted\\\"\" \"my-symbol\" \"(1 2 3)\" \"data: (a b c) end\" \"(1 \\\"two\\\" three)\" \"0, 1, 4, 9, 16\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 10: Char-by-char construction and advanced patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_char_by_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Build string char-by-char via aset on make-string
  (let ((s (make-string 5 ?_)))
    (aset s 0 ?H)
    (aset s 1 ?e)
    (aset s 2 ?l)
    (aset s 3 ?l)
    (aset s 4 ?o)
    s)
  ;; Caesar cipher via char manipulation
  (let ((plaintext "hello")
        (shift 3))
    (mapconcat
     (lambda (c)
       (if (and (>= c ?a) (<= c ?z))
           (string (+ ?a (mod (+ (- c ?a) shift) 26)))
         (string c)))
     (append plaintext nil) ""))
  ;; ROT13
  (let ((input "Hello World"))
    (mapconcat
     (lambda (c)
       (cond ((and (>= c ?a) (<= c ?z))
              (string (+ ?a (mod (+ (- c ?a) 13) 26))))
             ((and (>= c ?A) (<= c ?Z))
              (string (+ ?A (mod (+ (- c ?A) 13) 26))))
             (t (string c))))
     (append input nil) ""))
  ;; Reverse a string via char list
  (concat (nreverse (append "abcdef" nil)))
  ;; Interleave two strings char by char
  (let ((a "abc") (b "123"))
    (let ((result nil) (i 0))
      (while (< i (min (length a) (length b)))
        (push (aref a i) result)
        (push (aref b i) result)
        (setq i (1+ i)))
      (concat (nreverse result))))
  ;; String to list of chars and back
  (let* ((s "testing")
         (chars (append s nil))
         (back (concat chars)))
    (string= s back))
  ;; Remove vowels char by char
  (mapconcat
   (lambda (c)
     (if (memq c '(?a ?e ?i ?o ?u ?A ?E ?I ?O ?U))
         ""
       (string c)))
   (append "Hello World" nil) ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello\" \"khoor\" \"Uryyb Jbeyq\" \"fedcba\" \"a1b2c3\" t \"Hll Wrld\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 11: Complex combined string building patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_combined_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Build an ASCII table section
  (mapconcat
   (lambda (code)
     (format "%3d: %c" code code))
   (number-sequence 65 72) "\n")
  ;; Template string substitution via format + alist
  (let ((vars '(("name" . "World") ("lang" . "Elisp"))))
    (let ((template "Hello, %s! Welcome to %s."))
      (format template
              (cdr (assoc "name" vars))
              (cdr (assoc "lang" vars)))))
  ;; Repeat each char n times
  (mapconcat
   (lambda (c) (make-string 3 c))
   (append "abc" nil) "")
  ;; String builder with conditional formatting
  (let ((items '(("apple" 3 1.5) ("banana" 12 0.75) ("cherry" 1 4.0))))
    (mapconcat
     (lambda (item)
       (let ((name (nth 0 item))
             (qty (nth 1 item))
             (price (nth 2 item)))
         (format "  %-10s x%d  $%.2f" name qty (* qty price))))
     items "\n"))
  ;; Number to binary string
  (let ((to-binary nil))
    (setq to-binary
          (lambda (n)
            (if (= n 0) "0"
              (let ((bits nil))
                (while (> n 0)
                  (push (if (= (mod n 2) 1) ?1 ?0) bits)
                  (setq n (/ n 2)))
                (concat bits)))))
    (mapconcat (lambda (n) (format "%3d = %s" n (funcall to-binary n)))
               '(0 1 5 10 42 255) ", ")))"#;
    let expect = expect_test::expect![[
        r#""OK (\" 65: A\\n 66: B\\n 67: C\\n 68: D\\n 69: E\\n 70: F\\n 71: G\\n 72: H\" \"Hello, World! Welcome to Elisp.\" \"aaabbbccc\" \"  apple      x3  $4.50\\n  banana     x12  $9.00\\n  cherry     x1  $4.00\" \"  0 = 0,   1 = 1,   5 = 101,  10 = 1010,  42 = 101010, 255 = 11111111\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
