//! Advanced comprehensive oracle parity tests for regexp operations.
//!
//! Covers: character classes `[:alpha:]` `[:digit:]` etc., shy groups
//! `\\(?:...\\)`, numbered groups with `match-string`, backreferences
//! `\\1` `\\2`, alternation `\\|` with groups, repetition `*` `+` `?`
//! `\\{n,m\\}`, anchors `^` `$` `\\b` `\\'` `\\``, `string-match` vs
//! `string-match-p`, `replace-regexp-in-string` with all params,
//! `match-data`/`set-match-data`/`save-match-data` lifecycle.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Character classes: [:alpha:] [:digit:] [:alnum:] [:space:] etc.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_char_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; [:alpha:] matches letters
      (string-match "[[:alpha:]]+" "hello123")
      (match-string 0 "hello123")
      ;; [:digit:] matches digits
      (string-match "[[:digit:]]+" "hello123")
      (match-string 0 "hello123")
      ;; [:alnum:] matches alphanumeric
      (string-match "[[:alnum:]]+" "  abc123  ")
      (match-string 0 "  abc123  ")
      ;; [:space:] matches whitespace
      (string-match "[[:space:]]+" "hello   world")
      (match-string 0 "hello   world")
      ;; [:upper:] and [:lower:]
      (string-match "[[:upper:]]+" "helloWORLDfoo")
      (match-string 0 "helloWORLDfoo")
      (string-match "[[:lower:]]+" "HELLOworldFOO")
      (match-string 0 "HELLOworldFOO")
      ;; [:punct:] matches punctuation
      (string-match "[[:punct:]]+" "hello!@#world")
      (match-string 0 "hello!@#world")
      ;; Negated character classes
      (string-match "[^[:digit:]]+" "123abc456")
      (match-string 0 "123abc456")
      ;; Combined: alpha or digit
      (string-match "[[:alpha:][:digit:]]+" "---abc123---")
      (match-string 0 "---abc123---")
      ;; [:blank:] matches space and tab only
      (progn (string-match "[[:blank:]]+" "a \t b")
             (match-string 0 "a \t b")))"#;
    let expect = expect_test::expect![[
        r#""OK (0 \"hello\" 5 \"123\" 2 \"abc123\" 5 \"   \" 0 \"helloWORLDfoo\" 0 \"HELLOworldFOO\" 5 \"!@#\" 3 \"abc\" 3 \"abc123\" \" \t \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shy groups \\(?:...\\) vs numbered groups \\(...\\)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_shy_groups_vs_numbered() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s "foobar-bazquux"))
      (list
        ;; Numbered groups: each \\(...\\) gets a number
        (progn
          (string-match "\\(foo\\)\\(bar\\)-\\(baz\\)\\(quux\\)" s)
          (list (match-string 0 s)
                (match-string 1 s)
                (match-string 2 s)
                (match-string 3 s)
                (match-string 4 s)))
        ;; Shy groups: \\(?:...\\) does NOT capture
        (progn
          (string-match "\\(?:foo\\)\\(bar\\)-\\(?:baz\\)\\(quux\\)" s)
          (list (match-string 0 s)
                (match-string 1 s)   ;; "bar" (first capturing group)
                (match-string 2 s))) ;; "quux" (second capturing group)
        ;; Mix of shy and numbered
        (progn
          (string-match "\\(?:a\\(b\\)c\\)" "abc")
          (list (match-string 0 "abc")
                (match-string 1 "abc")))
        ;; Shy group for grouping without capture
        (progn
          (string-match "\\(?:ab\\)+" "ababab")
          (list (match-string 0 "ababab")))
        ;; Numbered inside shy
        (progn
          (string-match "\\(?:\\(foo\\)\\|\\(bar\\)\\)" "bar")
          (list (match-string 0 "bar")
                (match-string 1 "bar")
                (match-string 2 "bar")))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"foobar-bazquux\" \"foo\" \"bar\" \"baz\" \"quux\") (\"foobar-bazquux\" \"bar\" \"quux\") (\"abc\" \"b\") (\"ababab\") (\"bar\" nil \"bar\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backreferences \\1 \\2
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_backreferences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; \\1 refers to first group
      (string-match "\\(..\\)\\1" "abab")
      (match-string 0 "abab")
      (match-string 1 "abab")
      ;; No match when backreference doesn't match
      (string-match "\\(..\\)\\1" "abcd")
      ;; Multiple backreferences
      (progn
        (string-match "\\(a+\\)b\\(c+\\)d\\1e\\2" "aabccdaaeccc")
        ;; This should NOT match because \1 should be "aa" and \2 should be "cc"
        ;; "aabccdaaeccc" -> group1="aa", then need "aa" at pos after "d" = "aa" yes, then "e", then "cc"
        nil)
      (let ((s "aabccaacc"))
        (string-match "\\(a+\\)\\(b\\)\\(c+\\)\\1\\3" s)
        (when (match-string 0 s)
          (list (match-string 0 s)
                (match-string 1 s)
                (match-string 2 s)
                (match-string 3 s))))
      ;; Backreference for repeated words
      (let ((s "the the cat"))
        (string-match "\\b\\(\\w+\\) \\1\\b" s)
        (when (match-string 0 s)
          (list (match-string 0 s)
                (match-string 1 s))))
      ;; Backreference with single char group
      (string-match "\\(x\\)\\1\\1" "xxx")
      (match-string 0 "xxx"))"#;
    let expect = expect_test::expect![[
        r#""OK (0 \"abab\" \"ab\" nil nil (\"aabccaacc\" \"aa\" \"b\" \"cc\") (\"the the\" \"the\") 0 \"xxx\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alternation \\| with groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_alternation_with_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Simple alternation
      (string-match "cat\\|dog" "I have a dog")
      (match-string 0 "I have a dog")
      (string-match "cat\\|dog" "I have a cat")
      (match-string 0 "I have a cat")
      ;; Alternation with groups
      (progn
        (string-match "\\(cat\\)\\|\\(dog\\)" "the dog")
        (list (match-string 0 "the dog")
              (match-string 1 "the dog")
              (match-string 2 "the dog")))
      ;; Alternation inside a group
      (progn
        (string-match "\\(cat\\|dog\\)s" "cats")
        (list (match-string 0 "cats")
              (match-string 1 "cats")))
      (progn
        (string-match "\\(cat\\|dog\\)s" "dogs")
        (list (match-string 0 "dogs")
              (match-string 1 "dogs")))
      ;; Multiple alternations
      (string-match "a\\|b\\|c\\|d" "xyzd")
      (match-string 0 "xyzd")
      ;; Alternation with empty branch
      (string-match "foo\\|" "anything")
      ;; Alternation at different levels
      (progn
        (string-match "\\(a\\(b\\|c\\)d\\)" "acd")
        (list (match-string 0 "acd")
              (match-string 1 "acd")
              (match-string 2 "acd")))
      ;; Alternation doesn't match
      (string-match "cat\\|dog\\|fish" "bird")
      ;; Three-way alternation with capture
      (progn
        (string-match "\\(red\\|green\\|blue\\)" "the green one")
        (list (match-string 0 "the green one")
              (match-string 1 "the green one"))))"#;
    let expect = expect_test::expect![[
        r#""OK (9 \"dog\" 9 \"cat\" (\"dog\" nil \"dog\") (\"cats\" \"cat\") (\"dogs\" \"dog\") 3 \"d\" 0 (\"acd\" \"acd\" \"c\") nil (\"green\" \"green\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Repetition operators: * + ? \\{n,m\\}
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_repetition_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; * (zero or more) is greedy
      (progn (string-match "a*" "aaa") (match-string 0 "aaa"))
      (progn (string-match "a*" "bbb") (match-string 0 "bbb"))
      (progn (string-match "a*" "") (match-string 0 ""))
      ;; + (one or more)
      (progn (string-match "a+" "aaa") (match-string 0 "aaa"))
      (string-match "a+" "bbb")  ;; nil
      ;; ? (zero or one)
      (progn (string-match "ab?" "a") (match-string 0 "a"))
      (progn (string-match "ab?" "ab") (match-string 0 "ab"))
      (progn (string-match "ab?" "abb") (match-string 0 "abb"))
      ;; \\{n\\} exactly n
      (progn (string-match "a\\{3\\}" "aaaa") (match-string 0 "aaaa"))
      (string-match "a\\{3\\}" "aa")  ;; nil
      ;; \\{n,\\} n or more
      (progn (string-match "a\\{2,\\}" "aaaa") (match-string 0 "aaaa"))
      (string-match "a\\{2,\\}" "a")  ;; nil
      ;; \\{n,m\\} between n and m
      (progn (string-match "a\\{2,4\\}" "aaaaaa") (match-string 0 "aaaaaa"))
      (progn (string-match "a\\{2,4\\}" "aaa") (match-string 0 "aaa"))
      (progn (string-match "a\\{2,4\\}" "aa") (match-string 0 "aa"))
      (string-match "a\\{2,4\\}" "a")  ;; nil
      ;; \\{0,1\\} same as ?
      (progn (string-match "ab\\{0,1\\}" "a") (match-string 0 "a"))
      (progn (string-match "ab\\{0,1\\}" "ab") (match-string 0 "ab"))
      ;; Greedy matching: * matches as much as possible
      (progn (string-match "a.*b" "aXXbYYb") (match-string 0 "aXXbYYb"))
      ;; Non-greedy not supported in Emacs basic regex (no *?)
      ;; Repetition with groups
      (progn (string-match "\\(ab\\)\\{2,3\\}" "ababab")
             (list (match-string 0 "ababab") (match-string 1 "ababab"))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"aaa\" \"\" \"\" \"aaa\" nil \"a\" \"ab\" \"ab\" \"aaa\" nil \"aaaa\" nil \"aaaa\" \"aaa\" \"aa\" nil \"a\" \"ab\" \"aXXbYYb\" (\"ababab\" \"ab\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Anchors: ^ $ \\b \\B \\` \\'
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_anchors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; ^ matches beginning of string (or line in buffer)
      (string-match "^hello" "hello world")
      (string-match "^world" "hello world")
      ;; $ matches end of string
      (string-match "world$" "hello world")
      (string-match "hello$" "hello world")
      ;; \\` matches beginning of string (not line)
      (string-match "\\`hello" "hello world")
      (string-match "\\`world" "hello world")
      ;; \\' matches end of string (not line)
      (string-match "world\\'" "hello world")
      (string-match "hello\\'" "hello world")
      ;; \\b word boundary
      (string-match "\\bword\\b" "a word here")
      (string-match "\\bword\\b" "awordhere")
      (string-match "\\bcat\\b" "concatenate")
      (string-match "\\bcat\\b" "the cat sat")
      ;; \\B non-word-boundary
      (string-match "\\Bat\\B" "concatenate")
      (string-match "\\Bat\\B" "at")
      ;; ^ and $ with multiline content in string-match
      ;; In string-match, ^ matches start of string, not start of line
      (string-match "^line2" "line1\nline2")
      ;; But in buffer with looking-at, ^ matches line start
      (with-temp-buffer
        (insert "line1\nline2\nline3")
        (goto-char (point-min))
        (list
          (looking-at "^line1")
          (progn (forward-line 1) (looking-at "^line2"))
          (progn (forward-line 1) (looking-at "^line3")))))"#;
    let expect =
        expect_test::expect![[r#""OK (0 nil 6 nil 0 nil 6 nil 2 nil nil 4 4 nil 6 (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-regexp-in-string with all parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Basic replacement
      (replace-regexp-in-string "foo" "bar" "foo baz foo")
      ;; With group reference in replacement
      (replace-regexp-in-string "\\(\\w+\\)=\\(\\w+\\)" "\\2=\\1"
                                "a=1 b=2 c=3")
      ;; FIXEDCASE: nil means adjust case of replacement
      (replace-regexp-in-string "hello" "world" "Hello HELLO hello"
                                nil nil)
      ;; FIXEDCASE: t means use replacement as-is
      (replace-regexp-in-string "hello" "world" "Hello HELLO hello"
                                t nil)
      ;; LITERAL: t means treat replacement literally (no \\1 etc.)
      (replace-regexp-in-string "\\(foo\\)" "\\1bar" "foo"
                                nil t)
      ;; START parameter: begin replacement at position
      (replace-regexp-in-string "x" "Y" "xxxxx" nil nil 2)
      (replace-regexp-in-string "a" "b" "aaaa" nil nil 0)
      (replace-regexp-in-string "a" "b" "aaaa" nil nil 3)
      ;; Function replacement
      (replace-regexp-in-string "[0-9]+"
                                (lambda (m) (number-to-string (* 2 (string-to-number m))))
                                "a1 b2 c3")
      ;; Function replacement with groups
      (replace-regexp-in-string "\\([a-z]+\\)\\([0-9]+\\)"
                                (lambda (m)
                                  (concat (upcase (match-string 1 m))
                                          (match-string 2 m)))
                                "abc123 def456")
      ;; Empty replacement
      (replace-regexp-in-string "[aeiou]" "" "hello world")
      ;; Replace nothing (no match)
      (replace-regexp-in-string "xyz" "abc" "hello world")
      ;; Replace with empty match (zero-width)
      ;; Edge: replace at every position
      (replace-regexp-in-string "^" ">> " "hello"))"#;
    let expect =
        expect_test::expect![[r#""ERR (error \"replace-match subexpression does not exist\" 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-data / set-match-data / save-match-data lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_match_data_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; match-data after string-match
      (progn
        (string-match "\\(foo\\)\\(bar\\)" "foobar")
        (match-data))
      ;; match-beginning and match-end
      (progn
        (string-match "\\(hello\\) \\(world\\)" "hello world")
        (list (match-beginning 0) (match-end 0)
              (match-beginning 1) (match-end 1)
              (match-beginning 2) (match-end 2)))
      ;; set-match-data restores match state
      (progn
        (string-match "\\(aaa\\)" "aaa")
        (let ((saved (match-data)))
          (string-match "\\(bbb\\)\\(ccc\\)" "bbbccc")
          (let ((new-md (match-data)))
            (set-match-data saved)
            (list 'restored (match-data)
                  'was new-md
                  'equal-to-saved (equal (match-data) saved)))))
      ;; save-match-data macro
      (progn
        (string-match "\\(outer\\)" "outer")
        (let ((outer-md (match-data)))
          (save-match-data
            (string-match "\\(inner\\)" "inner")
            (match-data))
          (list 'after-save (match-data)
                'preserved (equal (match-data) outer-md))))
      ;; Nested save-match-data
      (progn
        (string-match "\\(level0\\)" "level0")
        (let ((md0 (match-data)))
          (save-match-data
            (string-match "\\(level1\\)" "level1")
            (let ((md1 (match-data)))
              (save-match-data
                (string-match "\\(level2\\)" "level2"))
              ;; After inner save-match-data, level1 match is restored
              (equal (match-data) md1)))
          ;; After outer save-match-data, level0 match is restored
          (equal (match-data) md0)))
      ;; match-data returns nil before any match
      ;; Actually after (set-match-data nil) it returns nil
      (progn
        (set-match-data nil)
        (match-data)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 6 0 3 3 6) (0 11 0 5 6 11) (restored (0 3 0 3) was (0 6 0 3 3 6) equal-to-saved t) (after-save (0 5 0 5) preserved t) t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Word and symbol regexp constructs: \\w \\W \\sw \\Sw \\< \\>
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_word_symbol_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; \\w matches word-constituent chars
      (progn (string-match "\\w+" "  hello  ") (match-string 0 "  hello  "))
      (progn (string-match "\\w+" "123abc") (match-string 0 "123abc"))
      ;; \\W matches non-word chars
      (progn (string-match "\\W+" "hello   world") (match-string 0 "hello   world"))
      ;; \\< word beginning, \\> word end
      (string-match "\\<cat\\>" "the cat sat")
      (string-match "\\<cat\\>" "concatenate")
      (string-match "\\<cat\\>" "scat")
      (match-string 0 "the cat sat")
      ;; \\sw is same as \\w in default syntax table
      (progn (string-match "\\sw+" "hello world") (match-string 0 "hello world"))
      ;; Multiple words
      (let ((s "one two three four") (result nil) (pos 0))
        (while (string-match "\\<\\(\\w+\\)\\>" s pos)
          (setq result (cons (match-string 1 s) result))
          (setq pos (match-end 0)))
        (nreverse result))
      ;; Word boundaries with digits
      (let ((s "var1 = var2 + 3") (result nil) (pos 0))
        (while (string-match "\\<\\([a-z]+[0-9]*\\)\\>" s pos)
          (setq result (cons (match-string 1 s) result))
          (setq pos (match-end 0)))
        (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"123abc\" \"   \" 4 nil nil \"cat\" \"hello\" (\"one\" \"two\" \"three\" \"four\") (\"var1\" \"var2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex regex patterns: email, URL-like, structured data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_complex_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Simple email-like pattern
      (let ((email-re "\\([a-zA-Z0-9.]+\\)@\\([a-zA-Z0-9.]+\\)"))
        (progn
          (string-match email-re "user@example.com")
          (list (match-string 0 "user@example.com")
                (match-string 1 "user@example.com")
                (match-string 2 "user@example.com"))))
      ;; Key-value pairs
      (let ((s "name=Alice age=30 city=NYC") (result nil) (pos 0))
        (while (string-match "\\([a-z]+\\)=\\([^ ]+\\)" s pos)
          (setq result (cons (cons (match-string 1 s) (match-string 2 s)) result))
          (setq pos (match-end 0)))
        (nreverse result))
      ;; CSV-like parsing (simple: no quotes)
      (let ((line "one,two,three,four") (result nil) (pos 0))
        (while (string-match "\\([^,]+\\)" line pos)
          (setq result (cons (match-string 1 line) result))
          (setq pos (match-end 0))
          ;; Skip the comma
          (when (and (< pos (length line)) (= (aref line pos) ?,))
            (setq pos (1+ pos))))
        (nreverse result))
      ;; Nested parens counting (not full match, just first level)
      (progn
        (string-match "(\\([^()]*\\))" "before (inner content) after")
        (match-string 1 "before (inner content) after"))
      ;; IP-like pattern (simplified)
      (let ((ip-re "\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)"))
        (progn
          (string-match ip-re "IP: 192.168.1.100 end")
          (list (match-string 0 "IP: 192.168.1.100 end")
                (match-string 1 "IP: 192.168.1.100 end")
                (match-string 2 "IP: 192.168.1.100 end")
                (match-string 3 "IP: 192.168.1.100 end")
                (match-string 4 "IP: 192.168.1.100 end"))))
      ;; Date-like pattern
      (let ((date-re "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)"))
        (progn
          (string-match date-re "Today is 2026-03-02 and tomorrow")
          (list (match-string 0 "Today is 2026-03-02 and tomorrow")
                (match-string 1 "Today is 2026-03-02 and tomorrow")
                (match-string 2 "Today is 2026-03-02 and tomorrow")
                (match-string 3 "Today is 2026-03-02 and tomorrow")))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"user@example.com\" \"user\" \"example.com\") ((\"name\" . \"Alice\") (\"age\" . \"30\") (\"city\" . \"NYC\")) (\"one\" \"two\" \"three\" \"four\") \"inner content\" (\"192.168.1.100\" \"192\" \"168\" \"1\" \"100\") (\"2026-03-02\" \"2026\" \"03\" \"02\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-forward and re-search-backward in buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_buffer_search_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "alpha beta gamma\nalpha delta epsilon\nalpha zeta eta\n")
      (goto-char (point-min))
      (list
        ;; re-search-forward basic
        (re-search-forward "alpha" nil t)
        (point)
        ;; Search again finds next
        (re-search-forward "alpha" nil t)
        (point)
        ;; With bound
        (progn
          (goto-char (point-min))
          (re-search-forward "alpha" 10 t))
        ;; NOERROR = t returns nil on fail
        (progn
          (goto-char (point-min))
          (re-search-forward "nonexistent" nil t))
        ;; re-search-backward
        (progn
          (goto-char (point-max))
          (re-search-backward "alpha" nil t))
        (point)
        ;; re-search-backward with bound
        (progn
          (goto-char (point-max))
          (let ((found (re-search-backward "alpha" 20 t)))
            (list found (point))))
        ;; COUNT parameter
        (progn
          (goto-char (point-min))
          (let ((found (re-search-forward "alpha" nil t 2)))
            (list found (point))))
        ;; Regex search with groups in buffer
        (progn
          (goto-char (point-min))
          (re-search-forward "\\(alpha\\) \\(\\w+\\)" nil t)
          (list (match-string 0)
                (match-string 1)
                (match-string 2)
                (match-beginning 1)
                (match-end 1)))))"#;
    let expect = expect_test::expect![[
        r#""OK (6 6 23 23 6 nil 38 38 (38 38) (23 23) (\"alpha beta\" \"alpha\" \"beta\" 1 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// regexp-quote: escaping special characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Simple string, no special chars
      (regexp-quote "hello")
      ;; String with special regexp chars
      (regexp-quote "hello.world")
      (regexp-quote "a*b+c?d")
      (regexp-quote "[foo]")
      (regexp-quote "(bar)")
      (regexp-quote "a\\b")
      (regexp-quote "x|y")
      (regexp-quote "^start$end")
      (regexp-quote "{1,3}")
      ;; All specials together
      (regexp-quote ".*+?[](){}|\\^$")
      ;; Using regexp-quote to do literal match
      (let ((literal "foo.bar"))
        (string-match (regexp-quote literal) "foo.bar"))
      (let ((literal "a+b"))
        (string-match (regexp-quote literal) "a+b"))
      ;; Without regexp-quote, . matches any char
      (string-match "foo.bar" "fooXbar")
      ;; With regexp-quote, . is literal
      (string-match (regexp-quote "foo.bar") "fooXbar")
      (string-match (regexp-quote "foo.bar") "foo.bar")
      ;; Empty string
      (regexp-quote "")
      ;; String with only special chars
      (regexp-quote "...")
      (regexp-quote "***"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello\\\\.world\" \"a\\\\*b\\\\+c\\\\?d\" \"\\\\[foo]\" \"(bar)\" \"a\\\\\\\\b\" \"x|y\" \"\\\\^start\\\\$end\" \"{1,3}\" \"\\\\.\\\\*\\\\+\\\\?\\\\[](){}|\\\\\\\\\\\\^\\\\$\" 0 0 0 nil 0 \"\" \"\\\\.\\\\.\\\\.\" \"\\\\*\\\\*\\\\*\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Iterative matching: collecting all matches in a string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_iterative_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Collect all words
      (let ((s "the quick brown fox") (result nil) (pos 0))
        (while (string-match "\\b\\w+\\b" s pos)
          (setq result (cons (match-string 0 s) result))
          (setq pos (match-end 0)))
        (nreverse result))
      ;; Collect all numbers
      (let ((s "x=10 y=20 z=30") (result nil) (pos 0))
        (while (string-match "[0-9]+" s pos)
          (setq result (cons (string-to-number (match-string 0 s)) result))
          (setq pos (match-end 0)))
        (nreverse result))
      ;; Collect matched groups
      (let ((s "func(a, b, c)") (result nil) (pos 0))
        (while (string-match "\\b\\([a-z]+\\)\\b" s pos)
          (setq result (cons (match-string 1 s) result))
          (setq pos (match-end 0)))
        (nreverse result))
      ;; Count matches
      (let ((s "banana") (count 0) (pos 0))
        (while (string-match "an" s pos)
          (setq count (1+ count))
          (setq pos (match-end 0)))
        count)
      ;; Iterative match in buffer
      (with-temp-buffer
        (insert "line one\nline two\nline three\n")
        (goto-char (point-min))
        (let ((result nil))
          (while (re-search-forward "^line \\(\\w+\\)" nil t)
            (setq result (cons (match-string 1) result)))
          (nreverse result)))
      ;; Replace all using iterative approach
      (let ((s "aaa bbb aaa ccc aaa") (result "") (pos 0))
        (while (string-match "aaa" s pos)
          (setq result (concat result (substring s pos (match-beginning 0)) "XXX"))
          (setq pos (match-end 0)))
        (setq result (concat result (substring s pos)))
        result))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"the\" \"quick\" \"brown\" \"fox\") (10 20 30) (\"func\" \"a\" \"b\" \"c\") 2 (\"one\" \"two\" \"three\") \"XXX bbb XXX ccc XXX\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case-insensitive matching via case-fold-search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_case_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Default: case-fold-search is t for string-match
      (let ((case-fold-search t))
        (list
          (string-match "hello" "HELLO")
          (string-match "HELLO" "hello")
          (string-match "HeLLo" "hEllO")))
      ;; With case-fold-search nil: case-sensitive
      (let ((case-fold-search nil))
        (list
          (string-match "hello" "HELLO")
          (string-match "hello" "hello")
          (string-match "HELLO" "HELLO")))
      ;; In buffer search
      (with-temp-buffer
        (insert "Hello World HELLO world")
        (let ((case-fold-search t))
          (goto-char (point-min))
          (let ((count 0))
            (while (re-search-forward "hello" nil t)
              (setq count (1+ count)))
            count)))
      (with-temp-buffer
        (insert "Hello World HELLO world hello")
        (let ((case-fold-search nil))
          (goto-char (point-min))
          (let ((count 0))
            (while (re-search-forward "hello" nil t)
              (setq count (1+ count)))
            count)))
      ;; case-fold-search with character classes
      (let ((case-fold-search t))
        (string-match "[a-z]+" "ABCDEF"))
      (let ((case-fold-search nil))
        (string-match "[a-z]+" "ABCDEF")))"#;
    let expect = expect_test::expect![[r#""OK ((0 0 0) (nil 0 0) 2 1 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-string with subexpressions and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_match_string_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; match-string 0 is whole match
      (progn
        (string-match "quick \\(brown\\) fox" "the quick brown fox")
        (match-string 0 "the quick brown fox"))
      ;; match-string for non-participating group returns nil
      (progn
        (string-match "\\(a\\)\\|\\(b\\)" "b")
        (list (match-string 0 "b")
              (match-string 1 "b")   ;; nil - didn't participate
              (match-string 2 "b"))) ;; "b"
      ;; match-string on no match (match-data from previous)
      (progn
        (string-match "xxx" "yyy")  ;; no match
        nil)  ;; can't call match-string safely after no match
      ;; match-string in buffer
      (with-temp-buffer
        (insert "key: value")
        (goto-char (point-min))
        (re-search-forward "\\(\\w+\\): \\(\\w+\\)" nil t)
        (list (match-string 0) (match-string 1) (match-string 2)))
      ;; Many groups
      (progn
        (string-match "\\(a\\)\\(b\\)\\(c\\)\\(d\\)\\(e\\)\\(f\\)\\(g\\)\\(h\\)\\(i\\)"
                      "abcdefghi")
        (list (match-string 1 "abcdefghi")
              (match-string 5 "abcdefghi")
              (match-string 9 "abcdefghi")))
      ;; match-string-no-properties (strips text properties)
      (with-temp-buffer
        (insert (propertize "hello" 'face 'bold))
        (insert " world")
        (goto-char (point-min))
        (re-search-forward "\\(hello\\)" nil t)
        (list (match-string 1)
              (match-string-no-properties 1))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"quick brown fox\" (\"b\" nil \"b\") nil (\"key: value\" \"key\" \"value\") (\"a\" \"e\" \"i\") (#(\"hello\" 0 5 (face bold)) \"hello\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
