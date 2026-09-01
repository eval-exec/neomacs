//! Oracle parity tests for `string-replace` with complex patterns:
//! basic replacement, multiple occurrences, no match, empty FROMSTRING,
//! deletion via empty TOSTRING, special regex chars as literals,
//! cascaded replacements, and comparison with replace-regexp-in-string.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic replacement: FROMSTRING, TOSTRING, INSTRING
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Simple single-occurrence replacement
  (string-replace "cat" "dog" "I have a cat")
  ;; Replacement at the beginning
  (string-replace "Hello" "Goodbye" "Hello, world!")
  ;; Replacement at the end
  (string-replace "end" "finish" "This is the end")
  ;; FROMSTRING equals entire INSTRING
  (string-replace "exact" "replaced" "exact")
  ;; Single character replacement
  (string-replace "x" "y" "foxbox")
  ;; Multi-word replacement
  (string-replace "old phrase" "new phrase" "This is an old phrase here")
  ;; Replacement where TOSTRING is longer than FROMSTRING
  (string-replace "a" "xyz" "banana")
  ;; Replacement where TOSTRING is shorter than FROMSTRING
  (string-replace "abc" "z" "abcdefabcghi"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"I have a dog\" \"Goodbye, world!\" \"This is the finish\" \"replaced\" \"foyboy\" \"This is an new phrase here\" \"bxyznxyznxyz\" \"zdefzghi\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple occurrences replaced
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_multiple_occurrences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; All occurrences replaced (not just first)
  (string-replace "o" "0" "foo boo moo zoo")
  ;; Overlapping-like patterns: "aa" in "aaaa" -> left-to-right, non-overlapping
  (string-replace "aa" "X" "aaaa")
  ;; "aa" in "aaaaa" (odd length)
  (string-replace "aa" "X" "aaaaa")
  ;; Repeated single char
  (string-replace "l" "L" "hello llama")
  ;; Adjacent matches
  (string-replace "ab" "CD" "ababab")
  ;; Pattern appears many times
  (string-replace "the" "THE" "the cat and the dog and the bird")
  ;; Replacement that creates what looks like a new match (but shouldn't re-match)
  (string-replace "ab" "a" "aabb"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"f00 b00 m00 z00\" \"XX\" \"XXa\" \"heLLo LLama\" \"CDCDCD\" \"THE cat and THE dog and THE bird\" \"aab\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// No match (returns original string unchanged)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; FROMSTRING not found
  (string-replace "xyz" "abc" "hello world")
  ;; Case mismatch (string-replace is case-sensitive)
  (string-replace "HELLO" "bye" "hello world")
  ;; FROMSTRING is substring but not at right boundary
  (string-replace "hell" "heaven" "shell")
  ;; Empty INSTRING with non-empty FROMSTRING
  (string-replace "anything" "something" "")
  ;; FROMSTRING longer than INSTRING
  (string-replace "toolong" "short" "too")
  ;; Result is string-equal to input when no match
  (let ((orig "unchanged"))
    (string-equal orig (string-replace "zzz" "aaa" orig))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"hello world\" \"sheaven\" \"\" \"too\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty FROMSTRING behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_empty_fromstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When FROMSTRING is "", inserts TOSTRING between every character and at boundaries
    let form = r#"(list
  ;; Empty FROM on "abc" -> inserts between every char
  (string-replace "" "-" "abc")
  ;; Empty FROM on single char
  (string-replace "" "X" "a")
  ;; Empty FROM on empty string
  (string-replace "" "Y" "")
  ;; Empty FROM with multi-char TOSTRING
  (string-replace "" "<>" "hi")
  ;; Both FROM and TO empty
  (string-replace "" "" "hello")
  ;; Empty FROM on longer string
  (string-replace "" "|" "abcde")
  ;; Length verification: empty-replace on "ab" with "-" should give "-a-b-"
  (length (string-replace "" "-" "ab")))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Replacement with empty TOSTRING (deletion)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_deletion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Delete all occurrences of a character
  (string-replace "l" "" "hello world")
  ;; Delete all spaces
  (string-replace " " "" "hello beautiful world")
  ;; Delete multi-char pattern
  (string-replace "ab" "" "abcabcabc")
  ;; Delete from beginning
  (string-replace "pre" "" "prefix")
  ;; Delete from end
  (string-replace "fix" "" "suffix")
  ;; Delete entire string
  (string-replace "gone" "" "gone")
  ;; Delete pattern that appears in result of deletion (no re-scan)
  (string-replace "bc" "" "abcbc")
  ;; Cascaded deletion
  (let ((s "a--b--c--d"))
    (string-replace "--" "" s)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"heo word\" \"hellobeautifulworld\" \"ccc\" \"fix\" \"suf\" \"\" \"a\" \"abcd\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// FROMSTRING containing special regex chars (literal matching)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-replace does literal matching, NOT regex. So regex-special chars
    // are treated as plain characters.
    let form = r#"(list
  ;; Dot is literal, not "any char"
  (string-replace "." "!" "a.b.c")
  ;; Asterisk is literal
  (string-replace "*" "x" "a*b*c")
  ;; Plus is literal
  (string-replace "+" "-" "a+b+c")
  ;; Brackets are literal
  (string-replace "[" "(" "array[0]")
  (string-replace "]" ")" "array[0]")
  ;; Backslash is literal
  (string-replace "\\" "/" "path\\to\\file")
  ;; Caret and dollar are literal
  (string-replace "^" "UP" "x^2")
  (string-replace "$" "USD" "price: $100")
  ;; Parentheses are literal
  (string-replace "(" "{" "fn(x)")
  (string-replace ")" "}" "fn(x)")
  ;; Question mark is literal
  (string-replace "?" "!" "really?")
  ;; Pipe is literal
  (string-replace "|" " or " "a|b|c")
  ;; Combined: replace a regex-like pattern literally
  (string-replace ".*" "STAR" "match .* everything"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a!b!c\" \"axbxc\" \"a-b-c\" \"array(0]\" \"array[0)\" \"path/to/file\" \"xUP2\" \"price: USD100\" \"fn{x)\" \"fn(x}\" \"really!\" \"a or b or c\" \"match STAR everything\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: cascaded replacements (replace result of replace)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_cascaded() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Chain of replacements
  (let ((s "Hello, World!"))
    (setq s (string-replace "Hello" "Hi" s))
    (setq s (string-replace "World" "Emacs" s))
    (setq s (string-replace "!" "." s))
    s)
  ;; Replace then replace the replacement
  (let ((s "aaa"))
    (setq s (string-replace "a" "bb" s))   ;; -> "bbbbbb"
    (setq s (string-replace "bb" "c" s))   ;; -> "ccc"
    s)
  ;; Build a slug from a title
  (let ((slug "Hello World: A Test!"))
    (setq slug (string-replace " " "-" slug))
    (setq slug (string-replace ":" "" slug))
    (setq slug (string-replace "!" "" slug))
    (downcase slug))
  ;; Escape HTML entities
  (let ((html "<p>Hello & \"World\"</p>"))
    (setq html (string-replace "&" "&amp;" html))
    (setq html (string-replace "<" "&lt;" html))
    (setq html (string-replace ">" "&gt;" html))
    (setq html (string-replace "\"" "&quot;" html))
    html)
  ;; Iterative replacement over a list of pairs
  (let ((s "The quick brown fox")
        (replacements '(("quick" . "slow")
                        ("brown" . "white")
                        ("fox" . "rabbit"))))
    (dolist (pair replacements)
      (setq s (string-replace (car pair) (cdr pair) s)))
    s)
  ;; Replacement creating new instances of original pattern does NOT re-match
  (string-replace "ab" "aab" "ab"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hi, Emacs.\" \"ccc\" \"hello-world-a-test\" \"&lt;p&gt;Hello &amp; &quot;World&quot;&lt;/p&gt;\" \"The slow white rabbit\" \"aab\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string-replace vs replace-regexp-in-string comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_vs_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare string-replace (literal) with replace-regexp-in-string (regex)
    // to demonstrate the difference in behavior.
    let form = r#"(list
  ;; For literal patterns, both should agree
  (let ((s "hello world hello"))
    (list
      (string-replace "hello" "hi" s)
      (replace-regexp-in-string "hello" "hi" s)
      (string-equal
        (string-replace "hello" "hi" s)
        (replace-regexp-in-string "hello" "hi" s))))

  ;; Dot: string-replace literal vs regexp any-char
  (let ((s "a.b.c"))
    (list
      (string-replace "." "X" s)
      (replace-regexp-in-string "\\." "X" s)
      ;; Both should produce "aXbXc" (regexp needs escaping)
      (string-equal
        (string-replace "." "X" s)
        (replace-regexp-in-string "\\." "X" s))))

  ;; Asterisk: literal vs regexp quantifier (must escape in regexp)
  (let ((s "a*b*c"))
    (list
      (string-replace "*" "+" s)
      (replace-regexp-in-string "\\*" "+" s)
      (string-equal
        (string-replace "*" "+" s)
        (replace-regexp-in-string "\\*" "+" s))))

  ;; Newline handling
  (let ((s "line1\nline2\nline3"))
    (list
      (string-replace "\n" "; " s)
      (replace-regexp-in-string "\n" "; " s)
      (string-equal
        (string-replace "\n" "; " s)
        (replace-regexp-in-string "\n" "; " s))))

  ;; Multi-char pattern: both should work the same for literals
  (let ((s "foobarfoo"))
    (list
      (string-replace "foo" "baz" s)
      (replace-regexp-in-string "foo" "baz" s)
      (string-equal
        (string-replace "foo" "baz" s)
        (replace-regexp-in-string "foo" "baz" s)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hi world hi\" \"hi world hi\" t) (\"aXbXc\" \"aXbXc\" t) (\"a+b+c\" \"a+b+c\" t) (\"line1; line2; line3\" \"line1; line2; line3\" t) (\"bazbarbaz\" \"bazbarbaz\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: unicode, whitespace, and boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_patterns_unicode_and_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Unicode replacement
  (string-replace "world" "mundo" "hello world")
  ;; Replace with unicode chars
  (string-replace "arrow" "->" "left arrow right")
  ;; Tab and newline
  (string-replace "\t" "  " "col1\tcol2\tcol3")
  ;; Multiple spaces
  (string-replace "  " " " "too  many  spaces  here")
  ;; Repeated application to collapse multiple spaces
  (let ((s "a    b"))
    (setq s (string-replace "  " " " s))  ;; "a  b"
    (setq s (string-replace "  " " " s))  ;; "a b"
    s)
  ;; Very long replacement
  (length (string-replace "x" "abcdefghij" "xxxx"))
  ;; Verify the actual long replacement
  (string-replace "x" "ab" "xxx"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello mundo\" \"left -> right\" \"col1  col2  col3\" \"too many spaces here\" \"a b\" 40 \"ababab\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
