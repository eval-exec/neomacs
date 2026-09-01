//! Advanced oracle parity tests for upcase/downcase patterns.
//!
//! Tests upcase/downcase on chars and strings, capitalize, upcase-initials,
//! upcase-region/downcase-region in buffers, case conversion with multibyte
//! characters, case-fold-search interaction, and combined case + string
//! operations for real-world text processing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Comprehensive upcase/downcase on chars and strings: edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_downcase_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test upcase/downcase on chars (integers), empty strings, single-char
    // strings, strings with only non-alpha characters, and verify
    // idempotence: upcase(upcase(x)) == upcase(x).
    let form = r#"(list
      ;; char (integer) conversions
      (upcase ?a) (downcase ?A)
      (upcase ?z) (downcase ?Z)
      (upcase ?0) (downcase ?0)  ;; digits unchanged
      (upcase ?!) (downcase ?!)  ;; punctuation unchanged
      (upcase ?\s) (downcase ?\s) ;; space unchanged

      ;; string conversions
      (upcase "") (downcase "")
      (upcase "a") (downcase "A")
      (upcase "!@#$%^&*()") (downcase "!@#$%^&*()")
      (upcase "123") (downcase "123")

      ;; idempotence
      (string= (upcase (upcase "Hello World"))
               (upcase "Hello World"))
      (string= (downcase (downcase "Hello World"))
               (downcase "Hello World"))

      ;; roundtrip: downcase(upcase(s)) for pure alpha
      (string= (downcase (upcase "abcxyz")) "abcxyz")

      ;; Mixed: only alpha chars change
      (upcase "a1b2c3!d4")
      (downcase "A1B2C3!D4")

      ;; Long string
      (upcase "the quick brown fox jumps over the lazy dog")
      (downcase "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG"))"#;
    let expect = expect_test::expect![[
        r#""OK (65 97 90 122 48 48 33 33 32 32 \"\" \"\" \"A\" \"a\" \"!@#$%^&*()\" \"!@#$%^&*()\" \"123\" \"123\" t t t \"A1B2C3!D4\" \"a1b2c3!d4\" \"THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG\" \"the quick brown fox jumps over the lazy dog\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// capitalize with complex word boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_capitalize_complex_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // capitalize treats various non-alpha chars as word separators.
    // Test with multiple consecutive separators, leading/trailing
    // separators, digits at word start, and all-uppercase input.
    let form = r#"(list
      ;; Basic word boundaries
      (capitalize "hello-world")
      (capitalize "hello_world")
      (capitalize "hello.world")
      (capitalize "hello world")

      ;; Multiple consecutive separators
      (capitalize "hello---world")
      (capitalize "hello___world")
      (capitalize "hello...world")
      (capitalize "hello   world")

      ;; Leading/trailing separators
      (capitalize "---hello---")
      (capitalize "  hello  world  ")

      ;; Digits at word boundaries
      (capitalize "hello123world")
      (capitalize "123hello")
      (capitalize "hello 123 world")

      ;; All uppercase input
      (capitalize "HELLO WORLD FOO BAR")
      (capitalize "ALL-CAPS-HERE")

      ;; Mixed case input
      (capitalize "hELLO wORLD fOO bAR")
      (capitalize "already Capitalized")

      ;; Single char words
      (capitalize "a b c d e")
      (capitalize "i-am-a-test")

      ;; Empty and single char
      (capitalize "")
      (capitalize "x")
      (capitalize "X")

      ;; Complex mixed separators
      (capitalize "one.two-three_four five/six"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello-World\" \"Hello_World\" \"Hello.World\" \"Hello World\" \"Hello---World\" \"Hello___World\" \"Hello...World\" \"Hello   World\" \"---Hello---\" \"  Hello  World  \" \"Hello123world\" \"123hello\" \"Hello 123 World\" \"Hello World Foo Bar\" \"All-Caps-Here\" \"Hello World Foo Bar\" \"Already Capitalized\" \"A B C D E\" \"I-Am-A-Test\" \"\" \"X\" \"X\" \"One.Two-Three_Four Five/Six\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// upcase-initials: first letter of each word, rest unchanged
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_initials_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // upcase-initials only uppercases the first letter of each word,
    // leaving all other characters exactly as they are (unlike capitalize
    // which lowercases the rest).
    let form = r#"(list
      ;; Basic: first letter up, rest unchanged
      (upcase-initials "hello world")
      (upcase-initials "HELLO WORLD")
      (upcase-initials "hELLO wORLD")

      ;; Difference from capitalize: rest is NOT lowercased
      (let ((s "hELLO wORLD"))
        (list (upcase-initials s)
              (capitalize s)
              (string= (upcase-initials s) (capitalize s))))

      ;; Various separators
      (upcase-initials "hello-world")
      (upcase-initials "hello_world")
      (upcase-initials "hello.world")

      ;; Already capitalized: no change
      (upcase-initials "Hello World")
      (string= (upcase-initials "Hello World") "Hello World")

      ;; Multiple separators
      (upcase-initials "a--b--c")
      (upcase-initials "x  y  z")

      ;; Empty and single
      (upcase-initials "")
      (upcase-initials "a")
      (upcase-initials "A")

      ;; With digits
      (upcase-initials "123abc")
      (upcase-initials "abc123def")
      (upcase-initials "1st 2nd 3rd")

      ;; camelCase detection: initials on camelCase input
      (upcase-initials "camelCase test")
      (upcase-initials "already PascalCase"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" \"HELLO WORLD\" \"HELLO WORLD\" (\"HELLO WORLD\" \"Hello World\" nil) \"Hello-World\" \"Hello_World\" \"Hello.World\" \"Hello World\" t \"A--B--C\" \"X  Y  Z\" \"\" \"A\" \"A\" \"123abc\" \"Abc123def\" \"1st 2nd 3rd\" \"CamelCase Test\" \"Already PascalCase\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// upcase-region / downcase-region in buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_upcase_downcase_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test upcase-region and downcase-region on buffer regions:
    // partial region, whole buffer, overlapping with point, region
    // at buffer boundaries, and empty region.
    let form = r#"(with-temp-buffer
      (insert "Hello World Foo Bar Baz")
      (let ((results nil))
        ;; upcase-region on middle portion "World Foo"
        (upcase-region 7 16)
        (push (buffer-string) results)

        ;; downcase-region on the upcased portion
        (downcase-region 7 16)
        (push (buffer-string) results)

        ;; upcase-region on entire buffer
        (upcase-region (point-min) (point-max))
        (push (buffer-string) results)

        ;; downcase-region on entire buffer
        (downcase-region (point-min) (point-max))
        (push (buffer-string) results)

        ;; upcase just first char
        (upcase-region 1 2)
        (push (buffer-string) results)

        ;; empty region (no change)
        (let ((before (buffer-string)))
          (upcase-region 5 5)
          (push (string= before (buffer-string)) results))

        ;; Verify point is not changed by region operations
        (goto-char 10)
        (let ((p (point)))
          (upcase-region 1 5)
          (push (= (point) p) results))

        (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello WORLD FOO Bar Baz\" \"Hello world foo Bar Baz\" \"HELLO WORLD FOO BAR BAZ\" \"hello world foo bar baz\" \"Hello world foo bar baz\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_multibyte_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test case conversion on Latin extended, accented characters,
    // and strings mixing ASCII with multibyte characters.
    let form = r#"(list
      ;; Latin accented characters
      (upcase "cafe")
      (downcase "CAFE")
      (capitalize "cafe au lait")

      ;; Char-level conversion on accented
      (upcase ?a) (downcase ?A)

      ;; Mixed ASCII and non-ASCII
      (upcase "hello world 123")
      (downcase "HELLO WORLD 123")

      ;; capitalize with accented chars
      (capitalize "hello world")

      ;; upcase-initials with accented
      (upcase-initials "hello world test")

      ;; String operations combined with case
      (let* ((s "Hello World")
             (up (upcase s))
             (down (downcase s))
             (cap (capitalize s)))
        (list up down cap
              (length up) (length down) (length cap)
              (= (length up) (length s))
              (= (length down) (length s))))

      ;; Case conversion preserves non-letter multibyte chars
      (let ((s "price: 100"))
        (list (upcase s) (downcase s))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"CAFE\" \"cafe\" \"Cafe Au Lait\" 65 97 \"HELLO WORLD 123\" \"hello world 123\" \"Hello World\" \"Hello World Test\" (\"HELLO WORLD\" \"hello world\" \"Hello World\" 11 11 11 t t) (\"PRICE: 100\" \"price: 100\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion combined with string operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_combined_string_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a text normalizer that combines case conversion with
    // string splitting, trimming, joining, and comparison.
    let form = r#"(progn
  (fset 'neovm--test-normalize-identifier
    (lambda (name)
      "Normalize an identifier: split on separators, downcase, rejoin with underscores."
      (let ((parts nil)
            (current "")
            (i 0)
            (len (length name)))
        (while (< i len)
          (let ((ch (aref name i)))
            (cond
              ;; Separator characters
              ((or (= ch ?-) (= ch ?_) (= ch ?.) (= ch ?/)
                   (= ch ?\s))
               (when (> (length current) 0)
                 (setq parts (cons (downcase current) parts)))
               (setq current ""))
              ;; Uppercase after lowercase: camelCase boundary
              ((and (>= ch ?A) (<= ch ?Z)
                    (> (length current) 0)
                    (let ((prev (aref current (1- (length current)))))
                      (and (>= prev ?a) (<= prev ?z))))
               (setq parts (cons (downcase current) parts))
               (setq current (downcase (char-to-string ch))))
              (t
               (setq current (concat current (char-to-string ch))))))
          (setq i (1+ i)))
        (when (> (length current) 0)
          (setq parts (cons (downcase current) parts)))
        (mapconcat 'identity (nreverse parts) "_"))))

  (fset 'neovm--test-to-title
    (lambda (s)
      "Convert to title case: capitalize each word."
      (let ((parts nil) (current "") (i 0) (len (length s)))
        (while (< i len)
          (let ((ch (aref s i)))
            (if (= ch ?\s)
                (progn
                  (when (> (length current) 0)
                    (setq parts (cons (capitalize current) parts)))
                  (setq current ""))
              (setq current (concat current (char-to-string ch)))))
          (setq i (1+ i)))
        (when (> (length current) 0)
          (setq parts (cons (capitalize current) parts)))
        (mapconcat 'identity (nreverse parts) " "))))

  (fset 'neovm--test-case-insensitive-sort
    (lambda (lst)
      "Sort strings case-insensitively."
      (sort (copy-sequence lst)
            (lambda (a b) (string-lessp (downcase a) (downcase b))))))

  (unwind-protect
      (list
        ;; normalize-identifier
        (funcall 'neovm--test-normalize-identifier "helloWorld")
        (funcall 'neovm--test-normalize-identifier "HelloWorld")
        (funcall 'neovm--test-normalize-identifier "hello-world")
        (funcall 'neovm--test-normalize-identifier "hello_world")
        (funcall 'neovm--test-normalize-identifier "hello.world.test")
        (funcall 'neovm--test-normalize-identifier "SCREAMING_SNAKE")
        (funcall 'neovm--test-normalize-identifier "myURLParser")

        ;; to-title
        (funcall 'neovm--test-to-title "hello world")
        (funcall 'neovm--test-to-title "THE QUICK BROWN FOX")
        (funcall 'neovm--test-to-title "already Title Case")
        (funcall 'neovm--test-to-title "single")

        ;; case-insensitive-sort
        (funcall 'neovm--test-case-insensitive-sort
                 '("banana" "Apple" "cherry" "date" "APRICOT"))
        (funcall 'neovm--test-case-insensitive-sort
                 '("Zebra" "alpha" "BETA" "gamma"))

        ;; Roundtrip: normalize -> to-title
        (funcall 'neovm--test-to-title
          (mapconcat 'identity
            (split-string
              (funcall 'neovm--test-normalize-identifier "myVariableName")
              "_")
            " ")))
    (fmakunbound 'neovm--test-normalize-identifier)
    (fmakunbound 'neovm--test-to-title)
    (fmakunbound 'neovm--test-case-insensitive-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello_world\" \"hello_world\" \"hello_world\" \"hello_world\" \"hello_world_test\" \"screaming_snake\" \"my_u_r_l_parser\" \"Hello World\" \"The Quick Brown Fox\" \"Already Title Case\" \"Single\" (\"Apple\" \"APRICOT\" \"banana\" \"cherry\" \"date\") (\"alpha\" \"BETA\" \"gamma\" \"Zebra\") \"My Variable Name\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// case-fold-search interaction with string-match and search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_fold_search_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test case-fold-search with string-match, re-search-forward, and
    // compare operations, toggling the variable.
    let form = r#"(with-temp-buffer
      (insert "Hello WORLD foo BAR baz QUX")
      (let ((results nil))
        ;; With case-fold-search = t (default): case-insensitive
        (let ((case-fold-search t))
          ;; string-match should find regardless of case
          (push (list 'fold-t
                      (string-match "hello" "Hello World")
                      (string-match "HELLO" "Hello World")
                      (string-match "hElLo" "Hello World"))
                results)
          ;; re-search-forward case-insensitive
          (save-excursion
            (goto-char (point-min))
            (let ((found nil))
              (while (re-search-forward "hello\\|foo\\|baz" nil t)
                (push (match-string 0) found))
              (push (list 'fold-t-search (nreverse found)) results))))

        ;; With case-fold-search = nil: case-sensitive
        (let ((case-fold-search nil))
          (push (list 'fold-nil
                      (string-match "hello" "Hello World")
                      (string-match "Hello" "Hello World")
                      (string-match "HELLO" "Hello World"))
                results)
          ;; re-search-forward case-sensitive
          (save-excursion
            (goto-char (point-min))
            (let ((found nil))
              (while (re-search-forward "Hello\\|foo\\|baz" nil t)
                (push (match-string 0) found))
              (push (list 'fold-nil-search (nreverse found)) results))))

        ;; Build case-insensitive lookup table using downcase
        (let ((table (list (cons "apple" 1) (cons "banana" 2) (cons "cherry" 3))))
          (push (list 'ci-lookup
                      (assoc (downcase "APPLE") table)
                      (assoc (downcase "Banana") table)
                      (assoc (downcase "GRAPE") table))
                results))

        (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((fold-t 0 0 0) (fold-t-search (\"Hello\" \"foo\" \"baz\")) (fold-nil nil 0 nil) (fold-nil-search (\"Hello\" \"foo\" \"baz\")) (ci-lookup (\"apple\" . 1) (\"banana\" . 2) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion pipeline: build a slug generator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_case_slug_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A URL slug generator: downcase, replace non-alnum with hyphens,
    // collapse consecutive hyphens, trim leading/trailing hyphens.
    let form = r#"(progn
  (fset 'neovm--test-slugify
    (lambda (title)
      "Convert title to URL slug."
      (let* ((lower (downcase title))
             (len (length lower))
             (chars nil)
             (i 0))
        ;; Replace non-alnum with hyphen
        (while (< i len)
          (let ((ch (aref lower i)))
            (if (or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?0) (<= ch ?9)))
                (setq chars (cons ch chars))
              (setq chars (cons ?- chars))))
          (setq i (1+ i)))
        (let ((slug (concat (nreverse chars))))
          ;; Collapse consecutive hyphens
          (while (string-match "--" slug)
            (setq slug (replace-match "-" nil nil slug)))
          ;; Trim leading/trailing hyphens
          (when (and (> (length slug) 0)
                     (= (aref slug 0) ?-))
            (setq slug (substring slug 1)))
          (when (and (> (length slug) 0)
                     (= (aref slug (1- (length slug))) ?-))
            (setq slug (substring slug 0 (1- (length slug)))))
          slug))))

  (unwind-protect
      (list
        (funcall 'neovm--test-slugify "Hello World")
        (funcall 'neovm--test-slugify "  Multiple   Spaces  ")
        (funcall 'neovm--test-slugify "Emacs Lisp: A Tutorial!")
        (funcall 'neovm--test-slugify "CamelCase And PascalCase")
        (funcall 'neovm--test-slugify "100% Pure & Simple")
        (funcall 'neovm--test-slugify "What's New in 2026?")
        (funcall 'neovm--test-slugify "---leading-and-trailing---")
        (funcall 'neovm--test-slugify "ALLCAPS")
        (funcall 'neovm--test-slugify "a")
        (funcall 'neovm--test-slugify ""))
    (fmakunbound 'neovm--test-slugify)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello-world\" \"multiple-spaces\" \"emacs-lisp-a-tutorial\" \"camelcase-and-pascalcase\" \"100-pure-simple\" \"what-s-new-in-2026\" \"leading-and-trailing\" \"allcaps\" \"a\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
