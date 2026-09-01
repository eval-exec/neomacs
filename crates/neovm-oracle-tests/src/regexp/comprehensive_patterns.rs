//! Comprehensive oracle parity tests for regexp operations:
//! `string-match` vs `string-match-p`, `looking-at` vs `looking-at-p`,
//! `re-search-forward`/`re-search-backward` with BOUND, NOERROR, COUNT,
//! complex regex patterns, `match-string`/`match-beginning`/`match-end`
//! with subgroups, `replace-regexp-in-string` with function replacement,
//! `regexp-quote`, and back-references.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-match vs string-match-p: side effects on match-data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_string_match_vs_match_p_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-match sets match-data, string-match-p does NOT
    let form = r#"(progn
      ;; First set match-data via string-match
      (string-match "\\(foo\\)\\(bar\\)" "foobar")
      (let ((md1 (match-data)))
        ;; string-match-p should NOT alter match-data
        (string-match-p "xyz" "xyzabc")
        (let ((md2 (match-data)))
          ;; md1 and md2 should be equal since string-match-p doesn't change it
          (list 'md1 md1 'md2 md2 'equal (equal md1 md2)))))"#;
    let expect = expect_test::expect![[r#""OK (md1 (0 6 0 3 3 6) md2 (0 6 0 3 3 6) equal t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Verify string-match DOES overwrite match-data
    let form2 = r#"(progn
      (string-match "\\(aaa\\)" "aaa")
      (let ((first-md (match-data)))
        (string-match "\\(bbb\\)\\(ccc\\)" "bbbccc")
        (let ((second-md (match-data)))
          (list 'first first-md 'second second-md
                'different (not (equal first-md second-md))))))"#;
    let expect =
        expect_test::expect![[r#""OK (first (0 3 0 3) second (0 6 0 3 3 6) different t)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // string-match-p returns same match position as string-match
    let form3 = r#"(let ((s "hello world"))
      (list (string-match "world" s)
            (string-match-p "world" s)
            (string-match "o" s)
            (string-match-p "o" s)
            (string-match-p "xyz" s)
            (string-match "xyz" s)))"#;
    let expect = expect_test::expect![[r#""OK (6 6 4 4 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// looking-at vs looking-at-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_looking_at_vs_looking_at_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // looking-at sets match-data, looking-at-p does NOT
    let form = r#"(with-temp-buffer
      (insert "foobar baz quux")
      (goto-char (point-min))
      ;; looking-at sets match-data
      (let ((r1 (looking-at "\\(foo\\)\\(bar\\)")))
        (let ((md1 (match-data)))
          ;; Move to " baz"
          (goto-char 8)
          ;; looking-at-p should NOT change match-data
          (let ((r2 (looking-at-p "baz")))
            (let ((md2 (match-data)))
              (list 'r1 r1 'r2 r2
                    'md1 md1 'md2 md2
                    'match-data-preserved (equal md1 md2)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (r1 t r2 t md1 (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>) md2 (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>) match-data-preserved t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // looking-at at various positions
    let form2 = r#"(with-temp-buffer
      (insert "abcdefghij")
      (goto-char (point-min))
      (list
       (looking-at "abc")
       (looking-at "bcd")
       (progn (goto-char 4) (looking-at "def"))
       (looking-at "^def")
       (progn (goto-char (point-min)) (looking-at "^abc"))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);
}

// ---------------------------------------------------------------------------
// re-search-forward/backward with BOUND, NOERROR, COUNT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_re_search_forward_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BOUND parameter: limit search to specific position
    let form = r#"(with-temp-buffer
      (insert "aaa bbb aaa ccc aaa")
      (goto-char (point-min))
      ;; Search with bound at position 8 - should find first "aaa" only
      (let ((r1 (re-search-forward "aaa" 8 t)))
        (let ((p1 (point)))
          ;; Search again with same bound - should NOT find second "aaa"
          (let ((r2 (re-search-forward "aaa" 8 t)))
            (list r1 p1 r2 (point))))))"#;
    let expect = expect_test::expect![[r#""OK (4 4 nil 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // NOERROR parameter: nil => error, t => return nil, other => move to limit
    let form2 = r#"(with-temp-buffer
      (insert "hello world")
      (goto-char (point-min))
      ;; NOERROR = t: return nil on failure, point unchanged
      (let ((r (re-search-forward "xyz" nil t)))
        (list r (point))))"#;
    let expect = expect_test::expect![[r#""OK (nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // NOERROR = non-nil non-t: move point to limit on failure
    let form3 = r#"(with-temp-buffer
      (insert "hello world")
      (goto-char (point-min))
      (let ((r (re-search-forward "xyz" nil 'move)))
        (list r (point) (= (point) (point-max)))))"#;
    let expect = expect_test::expect![[r#""OK (nil 12 t)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // COUNT parameter: find Nth occurrence
    let form4 = r#"(with-temp-buffer
      (insert "xx yy xx zz xx ww xx")
      (goto-char (point-min))
      (let ((r (re-search-forward "xx" nil t 3)))
        (list r (point))))"#;
    let expect = expect_test::expect![[r#""OK (15 15)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);
}

#[test]
fn oracle_prop_regexp_re_search_backward_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // re-search-backward with BOUND
    let form = r#"(with-temp-buffer
      (insert "aaa bbb aaa ccc aaa")
      (goto-char (point-max))
      ;; Backward search with bound at 10 - should not find first "aaa"
      (let ((r1 (re-search-backward "aaa" 10 t)))
        (let ((p1 (point)))
          ;; Backward search with bound at 1 - should find first "aaa"
          (let ((r2 (re-search-backward "aaa" 1 t)))
            (list r1 p1 r2 (point))))))"#;
    let expect = expect_test::expect![[r#""OK (17 17 9 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // re-search-backward with COUNT
    let form2 = r#"(with-temp-buffer
      (insert "ab cd ab ef ab gh ab")
      (goto-char (point-max))
      (let ((r (re-search-backward "ab" nil t 2)))
        (list r (point))))"#;
    let expect = expect_test::expect![[r#""OK (13 13)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // NOERROR with backward search
    let form3 = r#"(with-temp-buffer
      (insert "hello world")
      (goto-char (point-max))
      (let ((r1 (re-search-backward "xyz" nil t))
            (p1 (point)))
        (goto-char (point-max))
        (let ((r2 (re-search-backward "xyz" nil 'move))
              (p2 (point)))
          (list r1 p1 r2 p2))))"#;
    let expect = expect_test::expect![[r#""OK (nil 12 nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Complex regex patterns: grouping, alternation, repetition, char classes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_complex_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"dog\"""#]];
    // Alternation with grouping
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "\\(cat\\|dog\\|bird\\)" "I have a dog")
          (match-string 1 "I have a dog"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (\"2025-03\" \"2025-03\" \"2025\" \"03\")""#]];
    // Nested groups
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "\\(\\([0-9]+\\)-\\([0-9]+\\)\\)" "date: 2025-03")
          (list (match-string 0 "date: 2025-03")
                (match-string 1 "date: 2025-03")
                (match-string 2 "date: 2025-03")
                (match-string 3 "date: 2025-03")))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    // Character classes: [:alpha:], [:digit:], [:space:]
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "[[:digit:]]+" "abc 42 def")
          (match-string 0 "abc 42 def"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "[[:alpha:]]+" "123 hello 456")
          (match-string 0 "123 hello 456"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (0 0 0 nil 0 0 0 nil)""#]];
    // Repetition: *, +, ?, counted
    crate::common::assert_oracle_parity_expect(
        r#"(list
          (string-match "ab*c" "ac")
          (string-match "ab*c" "abc")
          (string-match "ab*c" "abbbbc")
          (string-match "ab+c" "ac")
          (string-match "ab+c" "abc")
          (string-match "ab?c" "ac")
          (string-match "ab?c" "abc")
          (string-match "ab?c" "abbc"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (\"bar-99\" \"99\")""#]];
    // Shy groups \\(?: ... \\) don't capture
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "\\(?:foo\\|bar\\)-\\([0-9]+\\)" "bar-99")
          (list (match-string 0 "bar-99")
                (match-string 1 "bar-99")))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// match-string, match-beginning, match-end with subgroup numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_match_accessors_subgroups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple groups with match-beginning/match-end
    let form = r#"(progn
      (string-match "\\([a-z]+\\)@\\([a-z]+\\)\\.\\([a-z]+\\)"
                     "user@example.com")
      (list
       ;; Group 0: whole match
       (match-beginning 0) (match-end 0)
       (match-string 0 "user@example.com")
       ;; Group 1: user
       (match-beginning 1) (match-end 1)
       (match-string 1 "user@example.com")
       ;; Group 2: domain
       (match-beginning 2) (match-end 2)
       (match-string 2 "user@example.com")
       ;; Group 3: tld
       (match-beginning 3) (match-end 3)
       (match-string 3 "user@example.com")))"#;
    let expect = expect_test::expect![[
        r#""OK (0 16 \"user@example.com\" 0 4 \"user\" 5 12 \"example\" 13 16 \"com\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Unmatched optional group returns nil
    let form2 = r#"(progn
      (string-match "\\(foo\\)\\(-\\([a-z]+\\)\\)?" "foo")
      (list (match-string 1 "foo")
            (match-string 2 "foo")
            (match-string 3 "foo")
            (match-beginning 2)
            (match-end 2)))"#;
    let expect = expect_test::expect![[r#""OK (\"foo\" nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // match-data as a flat list of integers
    let form3 = r#"(progn
      (string-match "\\(ab\\)\\(cd\\)\\(ef\\)" "xabcdefx")
      (match-data))"#;
    let expect = expect_test::expect![[r#""OK (1 7 1 3 3 5 5 7)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// replace-regexp-in-string with function replacement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_with_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"a3 b66 c999\"""#]];
    // Function replacement: receives matched string
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string
           "[0-9]+"
           (lambda (m) (number-to-string (* 3 (string-to-number m))))
           "a1 b22 c333")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"Hello World Foo\"""#]];
    // Function with upcase
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\b[a-z]" #'upcase "hello world foo")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"FOO:1 BAR:2 BAZ:3\"""#]];
    // Function that accesses match-data to get subgroups
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string
           "\\([a-z]+\\)=\\([0-9]+\\)"
           (lambda (m)
             (format "%s:%s" (upcase (match-string 1 m)) (match-string 2 m)))
           "foo=1 bar=2 baz=3")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"1 and 2 and 3\"""#]];
    // Function replacement with counter (closure)
    crate::common::assert_oracle_parity_expect(
        r#"(let ((n 0))
           (replace-regexp-in-string
            "X"
            (lambda (_m) (setq n (1+ n)) (number-to-string n))
            "X and X and X"))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// regexp-quote with special chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"foo\\\\.bar\" \"a\\\\*b\\\\+c\\\\?\" \"\\\\[abc]\" \"\\\\\\\\(group\\\\\\\\)\" \"\\\\^start\\\\$end\" \"a|b\" \"price: \\\\$5\\\\.00\")""#
    ]];
    // regexp-quote escapes all special regex characters
    crate::common::assert_oracle_parity_expect(
        r#"(list
          (regexp-quote "hello")
          (regexp-quote "foo.bar")
          (regexp-quote "a*b+c?")
          (regexp-quote "[abc]")
          (regexp-quote "\\(group\\)")
          (regexp-quote "^start$end")
          (regexp-quote "a|b")
          (regexp-quote "price: $5.00"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK 5""#]];
    // Use regexp-quote to search for literal special chars
    crate::common::assert_oracle_parity_expect(
        r#"(let ((needle "foo.bar"))
           (string-match (regexp-quote needle) "test foo.bar test"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    // regexp-quote + concat for anchored literal search
    crate::common::assert_oracle_parity_expect(
        r#"(let ((literal "a+b"))
           (list
            (string-match (concat "^" (regexp-quote literal)) "a+b stuff")
            (string-match (concat "^" (regexp-quote literal)) "aab stuff")))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Back-references in patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_back_references() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"the the\" \"the\")""#]];
    // \1 back-reference: match repeated word
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "\\([a-z]+\\) \\1" "the the cat")
          (list (match-string 0 "the the cat")
                (match-string 1 "the the cat")))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"123_foo 456_bar 789_baz\"""#]];
    // Back-reference in replace-regexp-in-string
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string
           "\\([a-z]+\\)-\\([0-9]+\\)"
           "\\2_\\1"
           "foo-123 bar-456 baz-789")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (0 \"abcdab\")""#]];
    // Back-reference: detect palindrome-like pattern (aba)
    crate::common::assert_oracle_parity_expect(
        r#"(list
          (string-match "\\(..\\)..\\1" "abcdab")
          (when (string-match "\\(..\\)..\\1" "abcdab")
            (match-string 0 "abcdab")))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (\"abba\" \"a\" \"b\")""#]];
    // Multiple back-references
    crate::common::assert_oracle_parity_expect(
        r#"(progn
          (string-match "\\([a-z]\\)\\([a-z]\\)\\2\\1" "abba xyzzy")
          (list (match-string 0 "abba xyzzy")
                (match-string 1 "abba xyzzy")
                (match-string 2 "abba xyzzy")))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex: iterative search collecting all matches with positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_iterative_search_collecting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Collect all matches of a pattern with their positions in a buffer
    let form = r#"(with-temp-buffer
      (insert "The quick brown fox jumps over the lazy fox")
      (goto-char (point-min))
      (let ((matches nil))
        (while (re-search-forward "\\b\\([a-z]+\\)\\b" nil t)
          (setq matches
                (cons (list (match-beginning 0)
                            (match-end 0)
                            (match-string 1))
                      matches)))
        (nreverse matches)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 \"The\") (5 10 \"quick\") (11 16 \"brown\") (17 20 \"fox\") (21 26 \"jumps\") (27 31 \"over\") (32 35 \"the\") (36 40 \"lazy\") (41 44 \"fox\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Collect all matches of a pattern in a string using string-match + START
    let form2 = r#"(let ((s "aa123bb456cc789dd")
                         (pos 0)
                         (nums nil))
      (while (string-match "[0-9]+" s pos)
        (setq nums (cons (list (match-beginning 0) (match-string 0 s)) nums))
        (setq pos (match-end 0)))
      (nreverse nums))"#;
    let expect = expect_test::expect![[r#""OK ((2 \"123\") (7 \"456\") (12 \"789\"))""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based tokenizer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a simple tokenizer that classifies tokens by regex
    let form = r#"(let ((input "let x = 42 + y * 3.14")
                       (pos 0)
                       (tokens nil)
                       (patterns '(("[ \t]+" . ws)
                                   ("[a-zA-Z_][a-zA-Z0-9_]*" . ident)
                                   ("[0-9]+\\(?:\\.[0-9]+\\)?" . number)
                                   ("[=+*]" . operator))))
      (while (< pos (length input))
        (let ((matched nil))
          (dolist (pat patterns)
            (unless matched
              (when (and (string-match (concat "\\`" (car pat))
                                       (substring input pos))
                         (= (match-beginning 0) 0))
                (let ((tok (match-string 0 (substring input pos))))
                  (unless (eq (cdr pat) 'ws)
                    (setq tokens (cons (list (cdr pat) tok) tokens)))
                  (setq pos (+ pos (length tok)))
                  (setq matched t)))))
          (unless matched
            (setq pos (1+ pos)))))
      (nreverse tokens))"#;
    let expect = expect_test::expect![[
        r#""OK ((ident \"let\") (ident \"x\") (operator \"=\") (number \"42\") (operator \"+\") (ident \"y\") (operator \"*\") (number \"3.14\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
