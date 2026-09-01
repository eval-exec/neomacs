//! Oracle parity tests for `upcase-initials`, `capitalize`, `upcase`,
//! and `downcase` with complex patterns.
//!
//! Tests upcase-initials on various string patterns, differences between
//! capitalize and upcase-initials, case conversion on single chars vs
//! strings, non-ASCII characters, title case implementation, and
//! camelCase/snake_case conversion utilities.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// upcase-initials: comprehensive string patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test upcase-initials on a wide variety of string patterns:
    // word separators, consecutive separators, leading/trailing
    // whitespace, digits, mixed case, empty, and single char.
    let form = r#"(list
  ;; Basic: first letter of each word uppercased, rest unchanged
  (upcase-initials "hello world")
  (upcase-initials "foo bar baz")
  (upcase-initials "a b c d e f")

  ;; Already capitalized: no change expected
  (upcase-initials "Hello World")
  (upcase-initials "FOO BAR")

  ;; Mixed case input: only first letter touched, rest preserved
  (upcase-initials "hELLO wORLD")
  (upcase-initials "tHIS iS a tEST")

  ;; Different word separators
  (upcase-initials "hello-world")
  (upcase-initials "hello_world")
  (upcase-initials "hello.world")
  (upcase-initials "hello/world")

  ;; Multiple consecutive separators
  (upcase-initials "hello---world")
  (upcase-initials "hello   world")
  (upcase-initials "a--b--c--d")

  ;; Leading and trailing separators
  (upcase-initials " hello world ")
  (upcase-initials "---hello---")
  (upcase-initials "  leading")
  (upcase-initials "trailing  ")

  ;; Digits in various positions
  (upcase-initials "hello123world")
  (upcase-initials "123hello")
  (upcase-initials "1st 2nd 3rd")
  (upcase-initials "abc 123 def")

  ;; Empty and single character
  (upcase-initials "")
  (upcase-initials "x")
  (upcase-initials "X")
  (upcase-initials " ")

  ;; Punctuation-heavy
  (upcase-initials "hello, world!")
  (upcase-initials "one.two.three"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" \"Foo Bar Baz\" \"A B C D E F\" \"Hello World\" \"FOO BAR\" \"HELLO WORLD\" \"THIS IS A TEST\" \"Hello-World\" \"Hello_World\" \"Hello.World\" \"Hello/World\" \"Hello---World\" \"Hello   World\" \"A--B--C--D\" \" Hello World \" \"---Hello---\" \"  Leading\" \"Trailing  \" \"Hello123world\" \"123hello\" \"1st 2nd 3rd\" \"Abc 123 Def\" \"\" \"X\" \"X\" \" \" \"Hello, World!\" \"One.Two.Three\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// capitalize vs upcase-initials: behavioral differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_vs_capitalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The key difference: capitalize lowercases the rest of each word,
    // while upcase-initials leaves the rest unchanged. Test this
    // systematically with various inputs.
    let form = r#"(progn
  (fset 'neovm--uip-compare
    (lambda (s)
      "Compare capitalize vs upcase-initials on string S."
      (let ((cap (capitalize s))
            (ui (upcase-initials s)))
        (list s cap ui (string= cap ui)))))

  (unwind-protect
      (list
        ;; All lowercase: both produce same result
        (funcall 'neovm--uip-compare "hello world")
        ;; All uppercase: capitalize lowercases rest, upcase-initials does not
        (funcall 'neovm--uip-compare "HELLO WORLD")
        ;; Mixed case: capitalize normalizes, upcase-initials only touches initials
        (funcall 'neovm--uip-compare "hELLO wORLD")
        (funcall 'neovm--uip-compare "already Capitalized")
        ;; camelCase: capitalize breaks it, upcase-initials preserves
        (funcall 'neovm--uip-compare "camelCase test")
        ;; ALL CAPS acronym: capitalize lowercases, upcase-initials keeps
        (funcall 'neovm--uip-compare "the HTTP protocol")
        (funcall 'neovm--uip-compare "use TCP/IP stack")
        ;; Single word
        (funcall 'neovm--uip-compare "SCREAMING")
        (funcall 'neovm--uip-compare "whisper")
        ;; With separators
        (funcall 'neovm--uip-compare "snake_CASE_test")
        (funcall 'neovm--uip-compare "kebab-CASE-test")
        ;; Empty
        (funcall 'neovm--uip-compare "")
        ;; Count how many of these are the same
        (let ((count 0))
          (dolist (s '("hello" "HELLO" "hELLO" "Hello" "a" "A" "" " "))
            (when (string= (capitalize s) (upcase-initials s))
              (setq count (1+ count))))
          count))
    (fmakunbound 'neovm--uip-compare)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello world\" \"Hello World\" \"Hello World\" t) (\"HELLO WORLD\" \"Hello World\" \"HELLO WORLD\" nil) (\"hELLO wORLD\" \"Hello World\" \"HELLO WORLD\" nil) (\"already Capitalized\" \"Already Capitalized\" \"Already Capitalized\" t) (\"camelCase test\" \"Camelcase Test\" \"CamelCase Test\" nil) (\"the HTTP protocol\" \"The Http Protocol\" \"The HTTP Protocol\" nil) (\"use TCP/IP stack\" \"Use Tcp/Ip Stack\" \"Use TCP/IP Stack\" nil) (\"SCREAMING\" \"Screaming\" \"SCREAMING\" nil) (\"whisper\" \"Whisper\" \"Whisper\" t) (\"snake_CASE_test\" \"Snake_Case_Test\" \"Snake_CASE_Test\" nil) (\"kebab-CASE-test\" \"Kebab-Case-Test\" \"Kebab-CASE-Test\" nil) (\"\" \"\" \"\" t) 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion on single chars (integers) vs strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_char_vs_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // upcase, downcase, and capitalize work on both characters (integers)
    // and strings. Verify consistent behavior and that upcase-initials
    // only works on strings.
    let form = r#"(list
  ;; upcase on chars
  (upcase ?a) (upcase ?z) (upcase ?A) (upcase ?Z)
  (upcase ?0) (upcase ?!) (upcase ?\s) (upcase ?\n)

  ;; downcase on chars
  (downcase ?a) (downcase ?z) (downcase ?A) (downcase ?Z)
  (downcase ?0) (downcase ?!) (downcase ?\s)

  ;; upcase on single-char strings matches char upcase
  (= (upcase ?a) (aref (upcase "a") 0))
  (= (upcase ?z) (aref (upcase "z") 0))
  (= (downcase ?A) (aref (downcase "A") 0))

  ;; capitalize on chars: same as upcase for single chars
  (capitalize ?a) (capitalize ?z) (capitalize ?A)

  ;; capitalize on single-char strings
  (capitalize "a") (capitalize "A") (capitalize "z")
  (capitalize "0") (capitalize "!")

  ;; Roundtrips: downcase(upcase(ch)) for alpha
  (= (downcase (upcase ?a)) ?a)
  (= (downcase (upcase ?m)) ?m)
  (= (upcase (downcase ?Z)) ?Z)

  ;; upcase/downcase idempotence on chars
  (= (upcase (upcase ?a)) (upcase ?a))
  (= (downcase (downcase ?A)) (downcase ?A))

  ;; Non-alphabetic chars are unchanged
  (= (upcase ?5) ?5)
  (= (downcase ?5) ?5)
  (= (upcase ?+) ?+)
  (= (downcase ?+) ?+)

  ;; Character range: all lowercase letters convert
  (let ((all-ok t))
    (let ((ch ?a))
      (while (<= ch ?z)
        (unless (and (>= (upcase ch) ?A) (<= (upcase ch) ?Z))
          (setq all-ok nil))
        (setq ch (1+ ch))))
    all-ok))"#;
    let expect = expect_test::expect![[
        r#""OK (65 90 65 90 48 33 32 10 97 122 97 122 48 33 32 t t t 65 90 65 \"A\" \"A\" \"Z\" \"0\" \"!\" t t t t t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion with non-ASCII characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_non_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test case conversion functions with various non-ASCII characters,
    // including Latin extended, and mixed ASCII/non-ASCII strings.
    let form = r#"(list
  ;; Basic ASCII still works
  (upcase "hello") (downcase "HELLO")
  (capitalize "hello world")
  (upcase-initials "hello world")

  ;; Strings with digits and punctuation: only alpha changes
  (upcase "abc123def!@#")
  (downcase "ABC123DEF!@#")

  ;; Mixed content
  (upcase "price: $100")
  (downcase "PRICE: $100")
  (capitalize "the quick brown fox")
  (upcase-initials "the quick brown fox")

  ;; String length preserved after conversion
  (= (length (upcase "hello")) (length "hello"))
  (= (length (downcase "WORLD")) (length "WORLD"))

  ;; capitalize preserves non-alpha
  (capitalize "123 hello 456 world")
  (capitalize "!!! hello ??? world")

  ;; upcase-initials preserves everything except initial letters
  (upcase-initials "the QUICK brown FOX")

  ;; Tab and special whitespace as separators
  (capitalize "hello\tworld")
  (upcase-initials "hello\tworld")

  ;; Newline as separator
  (capitalize "hello\nworld")
  (upcase-initials "hello\nworld"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"HELLO\" \"hello\" \"Hello World\" \"Hello World\" \"ABC123DEF!@#\" \"abc123def!@#\" \"PRICE: $100\" \"price: $100\" \"The Quick Brown Fox\" \"The Quick Brown Fox\" t t \"123 Hello 456 World\" \"!!! Hello ??? World\" \"The QUICK Brown FOX\" \"Hello\tWorld\" \"Hello\tWorld\" \"Hello\\nWorld\" \"Hello\\nWorld\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: title case implementation comparing strategies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_title_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement proper title case: capitalize each word but keep certain
    // small words (a, an, the, of, in, on, at, to, for, and, but, or)
    // lowercase unless they are the first or last word.
    let form = r#"(progn
  (defvar neovm--uip-small-words
    '("a" "an" "the" "of" "in" "on" "at" "to" "for" "and" "but" "or" "nor" "is"))

  (fset 'neovm--uip-title-case
    (lambda (s)
      "Convert S to proper title case."
      (let* ((words (split-string (downcase s) " " t))
             (total (length words))
             (result nil)
             (i 0))
        (dolist (w words)
          (let ((capitalized
                 (if (or (= i 0) (= i (1- total))
                         (not (member w neovm--uip-small-words)))
                     (capitalize w)
                   w)))
            (setq result (cons capitalized result)))
          (setq i (1+ i)))
        (mapconcat 'identity (nreverse result) " "))))

  (fset 'neovm--uip-simple-title
    (lambda (s)
      "Simple title case: just capitalize every word."
      (capitalize s)))

  (unwind-protect
      (list
        ;; Simple title case (capitalize)
        (funcall 'neovm--uip-simple-title "the lord of the rings")
        ;; Proper title case (small words lowercase in middle)
        (funcall 'neovm--uip-title-case "the lord of the rings")
        ;; First/last words always capitalized
        (funcall 'neovm--uip-title-case "a tale of two cities")
        ;; No small words
        (funcall 'neovm--uip-title-case "war peace")
        ;; All small words except first/last
        (funcall 'neovm--uip-title-case "the an of in on at to for")
        ;; Single word
        (funcall 'neovm--uip-title-case "hello")
        ;; Already correct
        (funcall 'neovm--uip-title-case "The Great Gatsby")
        ;; All caps input
        (funcall 'neovm--uip-title-case "THE GREAT GATSBY")
        ;; Compare simple vs proper
        (let ((inputs '("the art of war"
                        "gone with the wind"
                        "pride and prejudice"
                        "to kill a mockingbird")))
          (mapcar (lambda (s)
                    (list (funcall 'neovm--uip-simple-title s)
                          (funcall 'neovm--uip-title-case s)
                          (string= (funcall 'neovm--uip-simple-title s)
                                   (funcall 'neovm--uip-title-case s))))
                  inputs)))
    (fmakunbound 'neovm--uip-title-case)
    (fmakunbound 'neovm--uip-simple-title)
    (makunbound 'neovm--uip-small-words)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"The Lord Of The Rings\" \"The Lord of the Rings\" \"A Tale of Two Cities\" \"War Peace\" \"The an of in on at to For\" \"Hello\" \"The Great Gatsby\" \"The Great Gatsby\" ((\"The Art Of War\" \"The Art of War\" nil) (\"Gone With The Wind\" \"Gone With the Wind\" nil) (\"Pride And Prejudice\" \"Pride and Prejudice\" nil) (\"To Kill A Mockingbird\" \"To Kill a Mockingbird\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: camelCase / snake_case / kebab-case conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_case_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement conversions between camelCase, PascalCase, snake_case,
    // SCREAMING_SNAKE_CASE, and kebab-case using case functions.
    let form = r#"(progn
  (fset 'neovm--uip-split-camel
    (lambda (s)
      "Split camelCase or PascalCase string into word list."
      (let ((parts nil) (current "") (i 0) (len (length s)))
        (while (< i len)
          (let ((ch (aref s i)))
            (cond
              ;; Uppercase after lowercase: word boundary
              ((and (>= ch ?A) (<= ch ?Z)
                    (> (length current) 0)
                    (let ((prev (aref current (1- (length current)))))
                      (and (>= prev ?a) (<= prev ?z))))
               (setq parts (cons current parts))
               (setq current (char-to-string ch)))
              ;; Separator chars: _ - . space
              ((or (= ch ?_) (= ch ?-) (= ch ?.) (= ch ?\s))
               (when (> (length current) 0)
                 (setq parts (cons current parts)))
               (setq current ""))
              (t
               (setq current (concat current (char-to-string ch))))))
          (setq i (1+ i)))
        (when (> (length current) 0)
          (setq parts (cons current parts)))
        (nreverse parts))))

  (fset 'neovm--uip-to-camel
    (lambda (s)
      "Convert to camelCase."
      (let* ((parts (funcall 'neovm--uip-split-camel s))
             (first (downcase (car parts)))
             (rest (mapcar 'capitalize (cdr parts))))
        (apply 'concat first rest))))

  (fset 'neovm--uip-to-pascal
    (lambda (s)
      "Convert to PascalCase."
      (let ((parts (funcall 'neovm--uip-split-camel s)))
        (apply 'concat (mapcar 'capitalize parts)))))

  (fset 'neovm--uip-to-snake
    (lambda (s)
      "Convert to snake_case."
      (let ((parts (funcall 'neovm--uip-split-camel s)))
        (mapconcat 'downcase parts "_"))))

  (fset 'neovm--uip-to-screaming
    (lambda (s)
      "Convert to SCREAMING_SNAKE_CASE."
      (let ((parts (funcall 'neovm--uip-split-camel s)))
        (mapconcat 'upcase parts "_"))))

  (fset 'neovm--uip-to-kebab
    (lambda (s)
      "Convert to kebab-case."
      (let ((parts (funcall 'neovm--uip-split-camel s)))
        (mapconcat 'downcase parts "-"))))

  (unwind-protect
      (let ((inputs '("helloWorld" "HelloWorld" "hello_world"
                       "HELLO_WORLD" "hello-world" "myURLParser"
                       "getHTTPResponse" "simple")))
        (list
          ;; Split results
          (mapcar 'neovm--uip-split-camel inputs)
          ;; To camelCase
          (mapcar 'neovm--uip-to-camel inputs)
          ;; To PascalCase
          (mapcar 'neovm--uip-to-pascal inputs)
          ;; To snake_case
          (mapcar 'neovm--uip-to-snake inputs)
          ;; To SCREAMING_SNAKE_CASE
          (mapcar 'neovm--uip-to-screaming inputs)
          ;; To kebab-case
          (mapcar 'neovm--uip-to-kebab inputs)
          ;; Roundtrip: snake -> camel -> snake
          (let ((s "hello_world_test"))
            (string= (funcall 'neovm--uip-to-snake
                              (funcall 'neovm--uip-to-camel s))
                     s))
          ;; Roundtrip: camel -> snake -> camel
          (let ((s "helloWorldTest"))
            (string= (funcall 'neovm--uip-to-camel
                              (funcall 'neovm--uip-to-snake s))
                     s))))
    (fmakunbound 'neovm--uip-split-camel)
    (fmakunbound 'neovm--uip-to-camel)
    (fmakunbound 'neovm--uip-to-pascal)
    (fmakunbound 'neovm--uip-to-snake)
    (fmakunbound 'neovm--uip-to-screaming)
    (fmakunbound 'neovm--uip-to-kebab)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"hello\" \"World\") (\"Hello\" \"World\") (\"hello\" \"world\") (\"HELLO\" \"WORLD\") (\"hello\" \"world\") (\"my\" \"URLParser\") (\"get\" \"HTTPResponse\") (\"simple\")) (\"helloWorld\" \"helloWorld\" \"helloWorld\" \"helloWorld\" \"helloWorld\" \"myUrlparser\" \"getHttpresponse\" \"simple\") (\"HelloWorld\" \"HelloWorld\" \"HelloWorld\" \"HelloWorld\" \"HelloWorld\" \"MyUrlparser\" \"GetHttpresponse\" \"Simple\") (\"hello_world\" \"hello_world\" \"hello_world\" \"hello_world\" \"hello_world\" \"my_urlparser\" \"get_httpresponse\" \"simple\") (\"HELLO_WORLD\" \"HELLO_WORLD\" \"HELLO_WORLD\" \"HELLO_WORLD\" \"HELLO_WORLD\" \"MY_URLPARSER\" \"GET_HTTPRESPONSE\" \"SIMPLE\") (\"hello-world\" \"hello-world\" \"hello-world\" \"hello-world\" \"hello-world\" \"my-urlparser\" \"get-httpresponse\" \"simple\") t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// All four functions applied to the same inputs: cross-comparison matrix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_patterns_cross_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Apply all four case conversion functions (upcase, downcase,
    // capitalize, upcase-initials) to the same set of inputs and
    // compare results systematically.
    let form = r#"(progn
  (fset 'neovm--uip-all-conversions
    (lambda (s)
      "Apply all 4 case functions and return results + comparisons."
      (let ((up (upcase s))
            (down (downcase s))
            (cap (capitalize s))
            (ui (upcase-initials s)))
        (list s up down cap ui
              ;; Which pairs are equal?
              (string= up down)
              (string= up cap)
              (string= up ui)
              (string= down cap)
              (string= down ui)
              (string= cap ui)))))

  (unwind-protect
      (let ((inputs '("hello" "HELLO" "Hello" "hELLO" ""
                       "hello world" "HELLO WORLD" "Hello World"
                       "a" "A" "123" "hello123"
                       "one two three" "ONE TWO THREE"
                       "hello-world" "HELLO-WORLD"
                       "mixedCASE" "ALL UPPER case MIXED")))
        (list
          (mapcar 'neovm--uip-all-conversions inputs)
          ;; Count how many inputs have all 4 results distinct
          (let ((distinct-count 0))
            (dolist (s inputs)
              (let ((results (list (upcase s) (downcase s)
                                   (capitalize s) (upcase-initials s))))
                (let ((unique (let ((seen nil))
                                (dolist (r results)
                                  (unless (member r seen)
                                    (setq seen (cons r seen))))
                                seen)))
                  (when (= (length unique) 4)
                    (setq distinct-count (1+ distinct-count))))))
            distinct-count)
          ;; Invariants that should always hold:
          ;; upcase(upcase(s)) = upcase(s)
          ;; downcase(downcase(s)) = downcase(s)
          ;; capitalize on all-lower = upcase-initials on all-lower
          (let ((invariants-ok t))
            (dolist (s inputs)
              (unless (string= (upcase (upcase s)) (upcase s))
                (setq invariants-ok nil))
              (unless (string= (downcase (downcase s)) (downcase s))
                (setq invariants-ok nil)))
            invariants-ok)))
    (fmakunbound 'neovm--uip-all-conversions)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"hello\" \"HELLO\" \"hello\" \"Hello\" \"Hello\" nil nil nil nil nil t) (\"HELLO\" \"HELLO\" \"hello\" \"Hello\" \"HELLO\" nil nil t nil nil nil) (\"Hello\" \"HELLO\" \"hello\" \"Hello\" \"Hello\" nil nil nil nil nil t) (\"hELLO\" \"HELLO\" \"hello\" \"Hello\" \"HELLO\" nil nil t nil nil nil) (\"\" \"\" \"\" \"\" \"\" t t t t t t) (\"hello world\" \"HELLO WORLD\" \"hello world\" \"Hello World\" \"Hello World\" nil nil nil nil nil t) (\"HELLO WORLD\" \"HELLO WORLD\" \"hello world\" \"Hello World\" \"HELLO WORLD\" nil nil t nil nil nil) (\"Hello World\" \"HELLO WORLD\" \"hello world\" \"Hello World\" \"Hello World\" nil nil nil nil nil t) (\"a\" \"A\" \"a\" \"A\" \"A\" nil t t nil nil t) (\"A\" \"A\" \"a\" \"A\" \"A\" nil t t nil nil t) (\"123\" \"123\" \"123\" \"123\" \"123\" t t t t t t) (\"hello123\" \"HELLO123\" \"hello123\" \"Hello123\" \"Hello123\" nil nil nil nil nil t) (\"one two three\" \"ONE TWO THREE\" \"one two three\" \"One Two Three\" \"One Two Three\" nil nil nil nil nil t) (\"ONE TWO THREE\" \"ONE TWO THREE\" \"one two three\" \"One Two Three\" \"ONE TWO THREE\" nil nil t nil nil nil) (\"hello-world\" \"HELLO-WORLD\" \"hello-world\" \"Hello-World\" \"Hello-World\" nil nil nil nil nil t) (\"HELLO-WORLD\" \"HELLO-WORLD\" \"hello-world\" \"Hello-World\" \"HELLO-WORLD\" nil nil t nil nil nil) (\"mixedCASE\" \"MIXEDCASE\" \"mixedcase\" \"Mixedcase\" \"MixedCASE\" nil nil nil nil nil nil) (\"ALL UPPER case MIXED\" \"ALL UPPER CASE MIXED\" \"all upper case mixed\" \"All Upper Case Mixed\" \"ALL UPPER Case MIXED\" nil nil nil nil nil nil)) 2 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
