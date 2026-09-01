//! Advanced oracle parity tests for case conversion functions.
//!
//! Tests upcase/downcase on mixed-case strings with non-alpha chars,
//! capitalize with word boundary variations (hyphens, underscores, dots),
//! upcase-initials on multi-word strings, char (integer) argument cases,
//! case-fold-search simulation, camelCase/snake_case/kebab-case conversion,
//! and title case with exception words.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// upcase/downcase on mixed-case strings with non-alpha characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_mixed_with_non_alpha() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Punctuation, digits, special chars should be preserved
    let form = r#"(list
      (upcase "Hello, World! 123 @#$%")
      (downcase "Hello, World! 123 @#$%")
      (upcase "café-au-lait")
      (downcase "ÜBER-COOL_STUFF")
      (upcase "mixed123CASE_test.value")
      (downcase "mixed123CASE_test.value")
      (upcase "a1b2c3!@#d4e5")
      (downcase "A1B2C3!@#D4E5"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"HELLO, WORLD! 123 @#$%\" \"hello, world! 123 @#$%\" \"CAFÉ-AU-LAIT\" \"über-cool_stuff\" \"MIXED123CASE_TEST.VALUE\" \"mixed123case_test.value\" \"A1B2C3!@#D4E5\" \"a1b2c3!@#d4e5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// capitalize with various word boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_capitalize_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Emacs capitalize treats hyphens, underscores, dots, etc. as word separators
    let form = r#"(list
      (capitalize "hello-world-foo")
      (capitalize "hello_world_foo")
      (capitalize "hello.world.foo")
      (capitalize "hello/world/foo")
      (capitalize "hello world  foo")
      (capitalize "HELLO-WORLD-FOO")
      (capitalize "hELLO_wORLD_fOO")
      (capitalize "one.two.three.four")
      (capitalize "a-b-c-d-e")
      (capitalize "mixed--double__separators..here")
      (capitalize "123hello-456world")
      (capitalize "  leading-spaces  trailing  "))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello-World-Foo\" \"Hello_World_Foo\" \"Hello.World.Foo\" \"Hello/World/Foo\" \"Hello World  Foo\" \"Hello-World-Foo\" \"Hello_World_Foo\" \"One.Two.Three.Four\" \"A-B-C-D-E\" \"Mixed--Double__Separators..Here\" \"123hello-456world\" \"  Leading-Spaces  Trailing  \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// upcase-initials on multi-word strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_multiword() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // upcase-initials capitalizes first letter of each word, keeps rest unchanged
    let form = r#"(list
      (upcase-initials "hello world foo bar")
      (upcase-initials "HELLO WORLD")
      (upcase-initials "already Capitalized Words")
      (upcase-initials "hello-world-test")
      (upcase-initials "hello_world_test")
      (upcase-initials "hello.world.test")
      (upcase-initials "mixedCase camelCase PascalCase")
      (upcase-initials "123abc 456def")
      (upcase-initials "a b c d e f")
      (upcase-initials "")
      (upcase-initials "   spaces   between   ")
      (upcase-initials "one"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World Foo Bar\" \"HELLO WORLD\" \"Already Capitalized Words\" \"Hello-World-Test\" \"Hello_World_Test\" \"Hello.World.Test\" \"MixedCase CamelCase PascalCase\" \"123abc 456def\" \"A B C D E F\" \"\" \"   Spaces   Between   \" \"One\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion on single characters (integer arguments)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_char_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // upcase/downcase on char (integer) arguments return the cased char code
    let form = r#"(list
      (upcase ?a) (upcase ?z)
      (upcase ?A) (upcase ?Z)
      (downcase ?A) (downcase ?Z)
      (downcase ?a) (downcase ?z)
      ;; digits and non-alpha return themselves
      (upcase ?0) (upcase ?9)
      (downcase ?0) (downcase ?9)
      (upcase ?!) (downcase ?!)
      (upcase ? ) (downcase ? )
      ;; roundtrip: downcase(upcase(ch)) for alpha
      (= (downcase (upcase ?m)) ?m)
      (= (upcase (downcase ?M)) ?M)
      ;; non-alpha roundtrip
      (= (upcase ?5) ?5)
      (= (downcase ?5) ?5))"#;
    let expect = expect_test::expect![[
        r#""OK (65 90 65 90 97 122 97 122 48 57 48 57 33 33 32 32 t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion preserving non-alphabetic characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_preserves_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that case conversion only affects alpha chars, preserving
    // all structure, whitespace, punctuation, and string length
    let form = r#"(let* ((original "  Hello, World! [Test_123] {Foo-Bar}  ")
             (up (upcase original))
             (down (downcase original))
             (cap (capitalize original))
             (initials (upcase-initials original)))
        (list
          ;; lengths are preserved
          (= (length up) (length original))
          (= (length down) (length original))
          (= (length cap) (length original))
          (= (length initials) (length original))
          ;; specific results
          up down cap initials
          ;; downcase of upcase equals downcase of original
          (string-equal (downcase up) (downcase original))
          ;; upcase of downcase equals upcase of original
          (string-equal (upcase down) (upcase original))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t \"  HELLO, WORLD! [TEST_123] {FOO-BAR}  \" \"  hello, world! [test_123] {foo-bar}  \" \"  Hello, World! [Test_123] {Foo-Bar}  \" \"  Hello, World! [Test_123] {Foo-Bar}  \" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case-fold-search simulation (case-insensitive comparison)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_fold_search_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate case-insensitive string operations using downcase
    let form = r#"(progn
  (fset 'neovm--test-ci-equal
    (lambda (a b) (string-equal (downcase a) (downcase b))))
  (fset 'neovm--test-ci-member
    (lambda (s lst)
      (let ((ds (downcase s)) (found nil))
        (while (and lst (not found))
          (when (string-equal ds (downcase (car lst)))
            (setq found (car lst)))
          (setq lst (cdr lst)))
        found)))
  (fset 'neovm--test-ci-assoc
    (lambda (key alist)
      (let ((dk (downcase key)) (found nil))
        (while (and alist (not found))
          (when (string-equal dk (downcase (caar alist)))
            (setq found (car alist)))
          (setq alist (cdr alist)))
        found)))
  (fset 'neovm--test-ci-sort
    (lambda (lst)
      (sort (copy-sequence lst)
            (lambda (a b) (string-lessp (downcase a) (downcase b))))))
  (unwind-protect
      (list
        ;; case-insensitive equality
        (funcall 'neovm--test-ci-equal "Hello" "hELLO")
        (funcall 'neovm--test-ci-equal "ABC" "abc")
        (funcall 'neovm--test-ci-equal "test" "TEST")
        (funcall 'neovm--test-ci-equal "foo" "bar")
        ;; case-insensitive member
        (funcall 'neovm--test-ci-member "HELLO" '("world" "Hello" "test"))
        (funcall 'neovm--test-ci-member "FOO" '("bar" "baz"))
        ;; case-insensitive assoc
        (funcall 'neovm--test-ci-assoc "NAME" '(("name" . "Alice") ("age" . 30)))
        (funcall 'neovm--test-ci-assoc "AGE" '(("name" . "Alice") ("age" . 30)))
        ;; case-insensitive sort
        (funcall 'neovm--test-ci-sort '("Banana" "apple" "Cherry" "date")))
    (fmakunbound 'neovm--test-ci-equal)
    (fmakunbound 'neovm--test-ci-member)
    (fmakunbound 'neovm--test-ci-assoc)
    (fmakunbound 'neovm--test-ci-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil \"Hello\" nil (\"name\" . \"Alice\") (\"age\" . 30) (\"apple\" \"Banana\" \"Cherry\" \"date\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// camelCase <-> snake_case <-> kebab-case converter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_style_converter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Split a string into word parts by detecting transitions:
  ;; - underscore or hyphen boundaries
  ;; - lowercase-to-uppercase transitions (camelCase boundaries)
  (fset 'neovm--test-split-identifier
    (lambda (s)
      (let ((parts nil) (current "") (i 0) (len (length s)))
        (while (< i len)
          (let ((ch (aref s i)))
            (cond
              ;; separator: push current word, start new
              ((or (= ch ?_) (= ch ?-))
               (when (> (length current) 0)
                 (setq parts (cons current parts)))
               (setq current ""))
              ;; uppercase after lowercase: camelCase boundary
              ((and (>= ch ?A) (<= ch ?Z)
                    (> (length current) 0)
                    (let ((prev (aref current (1- (length current)))))
                      (and (>= prev ?a) (<= prev ?z))))
               (setq parts (cons current parts))
               (setq current (char-to-string ch)))
              (t
               (setq current (concat current (char-to-string ch))))))
          (setq i (1+ i)))
        (when (> (length current) 0)
          (setq parts (cons current parts)))
        (nreverse parts))))
  (fset 'neovm--test-to-snake
    (lambda (s)
      (mapconcat 'downcase (funcall 'neovm--test-split-identifier s) "_")))
  (fset 'neovm--test-to-kebab
    (lambda (s)
      (mapconcat 'downcase (funcall 'neovm--test-split-identifier s) "-")))
  (fset 'neovm--test-to-camel
    (lambda (s)
      (let* ((parts (funcall 'neovm--test-split-identifier s))
             (first (downcase (car parts)))
             (rest (mapcar 'capitalize (cdr parts))))
        (apply 'concat first rest))))
  (fset 'neovm--test-to-pascal
    (lambda (s)
      (mapconcat 'capitalize (funcall 'neovm--test-split-identifier s) "")))
  (unwind-protect
      (list
        ;; snake_case conversions
        (funcall 'neovm--test-to-snake "helloWorld")
        (funcall 'neovm--test-to-snake "hello-world")
        (funcall 'neovm--test-to-snake "HelloWorld")
        (funcall 'neovm--test-to-snake "hello_world")
        ;; kebab-case conversions
        (funcall 'neovm--test-to-kebab "helloWorld")
        (funcall 'neovm--test-to-kebab "hello_world")
        (funcall 'neovm--test-to-kebab "HelloWorld")
        ;; camelCase conversions
        (funcall 'neovm--test-to-camel "hello_world")
        (funcall 'neovm--test-to-camel "hello-world")
        (funcall 'neovm--test-to-camel "HelloWorld")
        ;; PascalCase conversions
        (funcall 'neovm--test-to-pascal "hello_world")
        (funcall 'neovm--test-to-pascal "helloWorld")
        (funcall 'neovm--test-to-pascal "hello-world-foo")
        ;; Roundtrip: snake -> camel -> snake
        (funcall 'neovm--test-to-snake
          (funcall 'neovm--test-to-camel "my_variable_name"))
        ;; Roundtrip: kebab -> pascal -> kebab
        (funcall 'neovm--test-to-kebab
          (funcall 'neovm--test-to-pascal "my-function-name")))
    (fmakunbound 'neovm--test-split-identifier)
    (fmakunbound 'neovm--test-to-snake)
    (fmakunbound 'neovm--test-to-kebab)
    (fmakunbound 'neovm--test-to-camel)
    (fmakunbound 'neovm--test-to-pascal)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello_world\" \"hello_world\" \"hello_world\" \"hello_world\" \"hello-world\" \"hello-world\" \"hello-world\" \"helloWorld\" \"helloWorld\" \"helloWorld\" \"HelloWorld\" \"HelloWorld\" \"HelloWorldFoo\" \"my_variable_name\" \"my-function-name\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Title case with exception words (a, an, the, of, in, on, at, to, for, etc.)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_title_case_with_exceptions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Title case: capitalize all words except small words (unless first/last)
  (fset 'neovm--test-title-case
    (lambda (s)
      (let* ((small-words '("a" "an" "the" "of" "in" "on" "at" "to" "for"
                            "and" "but" "or" "nor" "is" "by" "as"))
             (words (let ((result nil) (current "") (i 0) (len (length s)))
                      (while (< i len)
                        (let ((ch (aref s i)))
                          (if (= ch ?\s)
                              (progn
                                (when (> (length current) 0)
                                  (setq result (cons current result)))
                                (setq current ""))
                            (setq current (concat current (char-to-string ch)))))
                        (setq i (1+ i)))
                      (when (> (length current) 0)
                        (setq result (cons current result)))
                      (nreverse result)))
             (total (length words))
             (idx 0)
             (result nil))
        (dolist (w words)
          (let ((processed
                 (if (or (= idx 0)          ;; first word always capitalize
                         (= idx (1- total)) ;; last word always capitalize
                         (not (member (downcase w) small-words)))
                     (capitalize w)
                   (downcase w))))
            (setq result (cons processed result))
            (setq idx (1+ idx))))
        (mapconcat 'identity (nreverse result) " "))))
  (unwind-protect
      (list
        (funcall 'neovm--test-title-case "the lord of the rings")
        (funcall 'neovm--test-title-case "a tale of two cities")
        (funcall 'neovm--test-title-case "war and peace")
        (funcall 'neovm--test-title-case "the catcher in the rye")
        (funcall 'neovm--test-title-case "to kill a mockingbird")
        (funcall 'neovm--test-title-case "pride and prejudice")
        (funcall 'neovm--test-title-case "the art of war")
        (funcall 'neovm--test-title-case "on the origin of species")
        (funcall 'neovm--test-title-case "HELLO WORLD")
        (funcall 'neovm--test-title-case "single"))
    (fmakunbound 'neovm--test-title-case)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"The Lord of the Rings\" \"A Tale of Two Cities\" \"War and Peace\" \"The Catcher in the Rye\" \"To Kill a Mockingbird\" \"Pride and Prejudice\" \"The Art of War\" \"On the Origin of Species\" \"Hello World\" \"Single\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
