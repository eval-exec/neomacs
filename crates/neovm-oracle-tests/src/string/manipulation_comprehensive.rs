//! Comprehensive oracle parity tests for string manipulation functions:
//! substring with all parameter combinations, concat with mixed types,
//! string-replace, replace-regexp-in-string with all params,
//! split-string with SEPARATORS/OMIT-NULLS/TRIM, string-join,
//! string-trim variants with custom TRIM-CHARS, string-chop-newline.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// substring: exhaustive parameter combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_comprehensive_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"abcdefgh\"""#]];
    // Basic: START only
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 0)"#, expect);
    let expect = expect_test::expect![[r#""OK \"defgh\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 3)"#, expect);
    let expect = expect_test::expect![[r#""OK \"h\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 7)"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 8)"#, expect);

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    // START and END both positive
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 0 0)"#, expect);
    let expect = expect_test::expect![[r#""OK \"abcdefgh\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 0 8)"#, expect);
    let expect = expect_test::expect![[r#""OK \"cde\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 2 5)"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 4 4)"#, expect);

    let expect = expect_test::expect![[r#""OK \"h\"""#]];
    // Negative START (counts from end)
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" -1)"#, expect);
    let expect = expect_test::expect![[r#""OK \"abcdefgh\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" -8)"#, expect);
    let expect = expect_test::expect![[r#""OK \"fgh\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" -3)"#, expect);

    let expect = expect_test::expect![[r#""OK \"abcdefg\"""#]];
    // Negative END
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 0 -1)"#, expect);
    let expect = expect_test::expect![[r#""OK \"a\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 0 -7)"#, expect);
    let expect = expect_test::expect![[r#""OK \"cdef\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 2 -2)"#, expect);

    let expect = expect_test::expect![[r#""OK \"cdef\"""#]];
    // Both negative
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" -6 -2)"#, expect);
    let expect = expect_test::expect![[r#""OK \"efg\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" -4 -1)"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" -8 -0)"#, expect);

    let expect = expect_test::expect![[r#""OK \"bcdefg\"""#]];
    // Positive START, negative END
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 1 -1)"#, expect);
    let expect = expect_test::expect![[r#""OK \"def\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "abcdefgh" 3 -2)"#, expect);

    let expect = expect_test::expect![[r#""OK \"x\"""#]];
    // Single character string
    crate::common::assert_oracle_parity_expect(r#"(substring "x" 0)"#, expect);
    let expect = expect_test::expect![[r#""OK \"x\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "x" 0 1)"#, expect);
    let expect = expect_test::expect![[r#""OK \"x\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "x" -1)"#, expect);

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    // Empty result
    crate::common::assert_oracle_parity_expect(r#"(substring "hello" 3 3)"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(substring "" 0)"#, expect);

    // Combined with let to test programmatic usage
    let form = r#"(let ((s "The quick brown fox jumps"))
                    (list (substring s 4 9)
                          (substring s -5)
                          (substring s 10 -5)
                          (substring s -15 -10)))"#;
    let expect = expect_test::expect![[r#""OK (\"quick\" \"jumps\" \"brown fox \" \"brown\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// concat: 0, 1, 2, 3+ args, mixed string/char-list/vector types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concat_comprehensive_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    // Zero args
    crate::common::assert_oracle_parity_expect(r#"(concat)"#, expect);

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // Single arg of each type
    crate::common::assert_oracle_parity_expect(r#"(concat "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat '(104 101 108 108 111))"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat [104 101 108 108 111])"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat nil)"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat "")"#, expect);

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // Two args, all type combinations
    crate::common::assert_oracle_parity_expect(r#"(concat "hel" "lo")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat "hel" '(108 111))"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat '(104 101) "llo")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat [104 101] [108 108 111])"#, expect);
    let expect = expect_test::expect![[r#""OK \"abcdef\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat "abc" [100 101 102])"#, expect);

    let expect = expect_test::expect![[r#""OK \"abcde\"""#]];
    // Three+ args mixed
    crate::common::assert_oracle_parity_expect(r#"(concat "a" '(98) [99] "d" nil "e")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat "" "" "" "" "")"#, expect);
    let expect = expect_test::expect![[r#""OK \"xy\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(concat "x" nil nil nil "y")"#, expect);

    // Concat building up a string in a loop
    let form = r#"(let ((result "")
                        (words '("the" "quick" "brown" "fox")))
                    (dolist (w words)
                      (setq result (concat result (if (string= result "") "" " ") w)))
                    result)"#;
    let expect = expect_test::expect![[r#""OK \"the quick brown fox\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Many arguments via apply
    let form2 = r#"(apply #'concat (mapcar #'number-to-string '(1 2 3 4 5 6 7 8 9 0)))"#;
    let expect = expect_test::expect![[r#""OK \"1234567890\"""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    let expect = expect_test::expect![[r#""OK \"λαμβδα\"""#]];
    // Concat with multibyte characters
    crate::common::assert_oracle_parity_expect(r#"(concat '(955 945 956 946 948 945))"#, expect);
}

// ---------------------------------------------------------------------------
// string-replace: comprehensive usage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"bar baz bar\"""#]];
    // Basic replacement
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "foo" "bar" "foo baz foo")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    // No match
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "xyz" "abc" "hello world")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"heo\"""#]];
    // Replace with empty string (deletion)
    crate::common::assert_oracle_parity_expect(r#"(string-replace "l" "" "hello")"#, expect);

    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    // Replace empty with something (inserts between every char)
    crate::common::assert_oracle_parity_expect(r#"(string-replace "" "-" "abc")"#, expect);

    let expect = expect_test::expect![[r#""OK \"ba\"""#]];
    // Overlapping potential matches (non-regex, literal)
    crate::common::assert_oracle_parity_expect(r#"(string-replace "aa" "b" "aaa")"#, expect);
    let expect = expect_test::expect![[r#""OK \"bb\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-replace "aa" "b" "aaaa")"#, expect);

    let expect = expect_test::expect![[r#""OK \"bxyznxyznxyz\"""#]];
    // Replacement longer than original
    crate::common::assert_oracle_parity_expect(r#"(string-replace "a" "xyz" "banana")"#, expect);

    let expect = expect_test::expect![[r#""OK \"a cat in a hat\"""#]];
    // Multi-character FROMSTRING
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "the" "a" "the cat in the hat")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    // Replace in empty string
    crate::common::assert_oracle_parity_expect(r#"(string-replace "a" "b" "")"#, expect);

    let expect = expect_test::expect![[r#""OK \"foo bar foo\"""#]];
    // Self-replacement (idempotent)
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "foo" "foo" "foo bar foo")"#,
        expect,
    );

    // Chained replacements
    let form = r#"(let ((s "hello world"))
                    (setq s (string-replace "hello" "goodbye" s))
                    (setq s (string-replace "world" "planet" s))
                    s)"#;
    let expect = expect_test::expect![[r#""OK \"goodbye planet\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-regexp-in-string: FIXEDCASE, LITERAL, SUBEXP, START
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_comprehensive_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"abcNUMdefNUM\"""#]];
    // Basic regex replacement
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "[0-9]+" "NUM" "abc123def456")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"[hello] [world] [foo]\"""#]];
    // With backreference in replacement
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\([a-z]+\\)" "[\\1]" "hello world foo")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"goodbye goodbye goodbye\"""#]];
    // FIXEDCASE = t (preserve case of original)
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "hello" "goodbye" "Hello HELLO hello" t)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"Goodbye GOODBYE goodbye\"""#]];
    // FIXEDCASE = nil (default)
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "hello" "goodbye" "Hello HELLO hello" nil)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"\\\\1 \\\\1\"""#]];
    // LITERAL = t (treat replacement as literal, no backslash processing)
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\([a-z]+\\)" "\\1" "hello world" nil t)"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\([a-z]+\\)" "\\1" "hello world" nil nil)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"oobar baz BAZ\"""#]];
    // SUBEXP parameter: replace only specific subexpression
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\(foo\\)\\(bar\\)" "BAZ" "foobar baz foobar" nil nil nil 1)"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"obar baz BAZ\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\(foo\\)\\(bar\\)" "BAZ" "foobar baz foobar" nil nil nil 2)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 7) 8)""#]];
    // START parameter: begin matching from offset
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "[0-9]+" "N" "a1b2c3d4" nil nil nil nil 4)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"helloworld\"""#]];
    // Replace with empty
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "[[:space:]]+" "" "  hello   world  ")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"Xxxxx Xxxxx XXX\"""#]];
    // Replace character classes
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "[[:upper:]]" "x" "Hello World FOO")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"X X a\\nb X\"""#]];
    // Dot matches
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "a.b" "X" "aXb a1b a\nb acb")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"goodbye world\"""#]];
    // Anchored replacements
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "^hello" "goodbye" "hello world")"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"hello planet\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "world$" "planet" "hello world")"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// split-string: SEPARATORS, OMIT-NULLS, TRIM params -- comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_comprehensive_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\")""#]];
    // Default separator (whitespace), default OMIT-NULLS (t)
    crate::common::assert_oracle_parity_expect(r#"(split-string "  hello   world  ")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(split-string "\t\nhello\t\nworld\t\n")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\" \"d\")""#]];
    // Custom separator, OMIT-NULLS default
    crate::common::assert_oracle_parity_expect(r#"(split-string "a:b:c:d" ":")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"a\" \"\" \"b\" \"\" \"\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "a::b:::c" ":")"#, expect);

    let expect = expect_test::expect![[r#""OK (\"a\" \"\" \"b\" \"\" \"c\")""#]];
    // OMIT-NULLS = nil (keep empty strings)
    crate::common::assert_oracle_parity_expect(r#"(split-string "a,,b,,c" "," nil)"#, expect);
    let expect = expect_test::expect![[r#""OK (\"\" \"a\" \"b\" \"c\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string ",a,b,c," "," nil)"#, expect);
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string ",,," "," nil)"#, expect);

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    // OMIT-NULLS = t
    crate::common::assert_oracle_parity_expect(r#"(split-string "a,,b,,c" "," t)"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string ",,," "," t)"#, expect);

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    // TRIM parameter (regex to trim from each resulting piece)
    crate::common::assert_oracle_parity_expect(r#"(split-string " a , b , c " "," t " ")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"x\" \"y\" \"z\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(split-string "  x  |  y  |  z  " "|" t "[ \t]+")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (\"one\" \"two\" \"three\")""#]];
    // Multi-char regex separator
    crate::common::assert_oracle_parity_expect(
        r#"(split-string "one-->two-->three" "-->")"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "a123b456c" "[0-9]+")"#, expect);

    let expect = expect_test::expect![[r#""OK (\"\")""#]];
    // Edge cases
    crate::common::assert_oracle_parity_expect(r#"(split-string "" ",")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"nosep\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "nosep" ",")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "," ",")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "," "," nil)"#, expect);

    // Complex: split CSV-like, trim whitespace
    let form = r#"(split-string "  alpha = 1 , beta = 2 , gamma = 3  " "," t "[ \t]+")"#;
    let expect = expect_test::expect![[r#""OK (\"alpha = 1\" \"beta = 2\" \"gamma = 3\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Roundtrip: split then join back
    let form2 = r#"(let ((parts (split-string "a/b/c/d" "/")))
                      (string-join parts "/"))"#;
    let expect = expect_test::expect![[r#""OK \"a/b/c/d\"""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);
}

// ---------------------------------------------------------------------------
// string-join: various separators and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"a, b, c\"""#]];
    // Normal separators
    crate::common::assert_oracle_parity_expect(r#"(string-join '("a" "b" "c") ", ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"x | y | z\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join '("x" "y" "z") " | ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"123\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join '("1" "2" "3") "")"#, expect);
    let expect = expect_test::expect![[r#""OK \"one and two and three\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-join '("one" "two" "three") " and ")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"only\"""#]];
    // Single element list
    crate::common::assert_oracle_parity_expect(r#"(string-join '("only") ":::")"#, expect);

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    // Empty list
    crate::common::assert_oracle_parity_expect(r#"(string-join nil ",")"#, expect);

    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    // Default separator (no second arg)
    crate::common::assert_oracle_parity_expect(r#"(string-join '("a" "b" "c"))"#, expect);

    let expect = expect_test::expect![[r#""OK \"line1\\nline2\\nline3\"""#]];
    // Join with newline
    crate::common::assert_oracle_parity_expect(
        r#"(string-join '("line1" "line2" "line3") "\n")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \",,\"""#]];
    // Join empty strings
    crate::common::assert_oracle_parity_expect(r#"(string-join '("" "" "") ",")"#, expect);
    let expect = expect_test::expect![[r#""OK \",mid,\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join '("" "mid" "") ",")"#, expect);

    // Join from computed list
    let form = r#"(string-join
                    (mapcar (lambda (n) (format "item-%d" n))
                            '(1 2 3 4 5))
                    " -> ")"#;
    let expect =
        expect_test::expect![[r#""OK \"item-1 -> item-2 -> item-3 -> item-4 -> item-5\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Nested join/split roundtrip
    let form2 = r#"(let ((csv "name,age,city"))
                      (equal csv (string-join (split-string csv ",") ",")))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);
}

// ---------------------------------------------------------------------------
// string-trim, string-trim-left, string-trim-right with custom TRIM-CHARS
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // Default whitespace trimming
    crate::common::assert_oracle_parity_expect(r#"(string-trim "   hello   ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "\t\n hello \n\t")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "   ")"#, expect);

    let expect = expect_test::expect![[r#""OK \"--hello---\"""#]];
    // Custom TRIM-CHARS (character alternatives for regex)
    crate::common::assert_oracle_parity_expect(r#"(string-trim "---hello---" "-")"#, expect);
    let expect = expect_test::expect![[r#""OK \"**wrap***\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "***wrap***" "*")"#, expect);
    let expect = expect_test::expect![[r##""OK \"#title##\"""##]];
    crate::common::assert_oracle_parity_expect(r###"(string-trim "##title##" "#")"###, expect);
    let expect = expect_test::expect![[r#""OK \"-=val=-+\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "+-=val=-+" "[+=\\-]")"#, expect);

    let expect = expect_test::expect![[r#""OK \">>hello<<<\"""#]];
    // Left trim only
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left ">>>hello<<<" ">")"#, expect);
    let expect = expect_test::expect![[r#""OK \"00123\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left "000123" "0")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello   \"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left "   hello   ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left "hello" "x")"#, expect);

    let expect = expect_test::expect![[r#""OK \"hello..\"""#]];
    // Right trim only
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "hello..." ".")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello<<\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "hello<<<" "<")"#, expect);
    let expect = expect_test::expect![[r#""OK \"   hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "   hello   ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "hello" "x")"#, expect);

    let expect = expect_test::expect![[r#""OK \"--\"""#]];
    // Trim entire string
    crate::common::assert_oracle_parity_expect(r#"(string-trim "---" "-")"#, expect);
    let expect = expect_test::expect![[r#""OK \"--\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left "---" "-")"#, expect);
    let expect = expect_test::expect![[r#""OK \"--\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "---" "-")"#, expect);

    let expect = expect_test::expect![[r#""OK \"23hello456\"""#]];
    // Trim with regex character classes
    crate::common::assert_oracle_parity_expect(r#"(string-trim "123hello456" "[0-9]")"#, expect);
    let expect = expect_test::expect![[r#""OK \"BChelloXYZ\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "ABChelloXYZ" "[A-Z]")"#, expect);

    // Combine trim operations
    let form = r#"(let ((s "  ### TITLE ###  "))
                    (list (string-trim s)
                          (string-trim (string-trim s) "[# ]")))"#;
    let expect = expect_test::expect![[r####""OK (\"### TITLE ###\" \"## TITLE ###\")""####]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-chop-newline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_chop_newline_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "hello\n")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "\n")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\\n\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "hello\n\n")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\\nhello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "\nhello\n")"#, expect);

    let expect = expect_test::expect![[r#""OK \"hello\\r\"""#]];
    // Only removes trailing newline, not carriage return
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "hello\r\n")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\\r\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-chop-newline "hello\r")"#, expect);
}

// ---------------------------------------------------------------------------
// Complex: string pipeline combining multiple operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_manipulation_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a slug generator: trim, downcase, replace spaces/special with hyphens
    let form = r#"(let ((title "  Hello, World! This is a TEST.  "))
                    (let* ((trimmed (string-trim title))
                           (lowered (downcase trimmed))
                           (no-special (replace-regexp-in-string "[^a-z0-9 ]" "" lowered))
                           (hyphenated (replace-regexp-in-string " +" "-" no-special)))
                      (list trimmed lowered no-special hyphenated)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, World! This is a TEST.\" \"hello, world! this is a test.\" \"hello world this is a test\" \"hello-world-this-is-a-test\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // CSV parser: split lines, split fields, trim each
    let form2 = r#"(let ((csv "name, age, city\nAlice, 30, NYC\nBob, 25, LA"))
                      (let ((lines (split-string csv "\n" t)))
                        (mapcar
                          (lambda (line)
                            (mapcar
                              (lambda (field) (string-trim field))
                              (split-string line "," nil)))
                          lines)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" \"age\" \"city\") (\"Alice\" \"30\" \"NYC\") (\"Bob\" \"25\" \"LA\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Word frequency counter using string operations
    let form3 = r#"(let* ((text "the cat sat on the mat the cat")
                          (words (split-string (downcase text) " " t))
                          (counts nil))
                     (dolist (w words)
                       (let ((entry (assoc w counts)))
                         (if entry
                             (setcdr entry (1+ (cdr entry)))
                           (setq counts (cons (cons w 1) counts)))))
                     (sort counts (lambda (a b) (> (cdr a) (cdr b)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"the\" . 3) (\"cat\" . 2) (\"mat\" . 1) (\"on\" . 1) (\"sat\" . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Complex: string-replace vs replace-regexp-in-string interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_vs_regexp_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"a!b!c\"""#]];
    // string-replace is literal (not regex)
    crate::common::assert_oracle_parity_expect(r#"(string-replace "." "!" "a.b.c")"#, expect);
    let expect = expect_test::expect![[r#""OK \"a!b!c\"""#]];
    // replace-regexp-in-string treats . as any char
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\." "!" "a.b.c")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"aYbYc\"""#]];
    // Bracket differences
    crate::common::assert_oracle_parity_expect(r#"(string-replace "[x]" "Y" "a[x]b[x]c")"#, expect);
    let expect = expect_test::expect![[r#""OK \"aYbYc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\[x\\]" "Y" "a[x]b[x]c")"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"1plus2plus3\"""#]];
    // Backslash handling
    crate::common::assert_oracle_parity_expect(r#"(string-replace "+" "plus" "1+2+3")"#, expect);
    let expect = expect_test::expect![[r#""OK \"1plus2plus3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "\\+" "plus" "1+2+3")"#,
        expect,
    );

    // Multi-step transformation
    let form = r#"(let ((s "Hello World 123 Foo"))
                    (list
                      ;; Literal replacements
                      (string-replace "Hello" "Hi" s)
                      ;; Regex: remove all digits
                      (replace-regexp-in-string "[0-9]" "" s)
                      ;; Regex: wrap words in brackets
                      (replace-regexp-in-string "\\([A-Za-z]+\\)" "[\\1]" s)
                      ;; Regex with START offset
                      (replace-regexp-in-string "[A-Z]" "x" s nil nil nil nil 6)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 7) 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
