//! Oracle parity tests for complex string processing patterns:
//! `split-string`, `string-join`, `string-trim`, `string-prefix-p`,
//! `string-suffix-p`, `replace-regexp-in-string` in combination.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// split-string extended usage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_default_sep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\")""#]];
    // Default separator is split-string-default-separators
    crate::common::assert_oracle_parity_expect(r#"(split-string "  hello   world  ")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"no-spaces\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "no-spaces")"#, expect);
}

#[test]
fn oracle_prop_split_string_custom_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\" \"d\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "a,b,c,d" ",")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "a::b::c" "::")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "a.b.c" "\\.")"#, expect);
}

#[test]
fn oracle_prop_split_string_omit_nulls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    // OMIT-NULLS parameter (3rd arg)
    crate::common::assert_oracle_parity_expect(r#"(split-string ",a,,b,c," "," t)"#, expect);
    let expect = expect_test::expect![[r#""OK (\"\" \"a\" \"\" \"b\" \"c\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string ",a,,b,c," ",")"#, expect);
}

#[test]
fn oracle_prop_split_string_trim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    // TRIM parameter (4th arg) — regex to trim from each result
    crate::common::assert_oracle_parity_expect(
        r#"(split-string "  a , b , c  " "," t "[ \t]+")"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// string-trim / string-trim-left / string-trim-right
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "  hello  ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello  \"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left "  hello  ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"  hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "  hello  ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "\n\thello\n\t")"#, expect);
}

#[test]
fn oracle_prop_string_trim_custom_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello---\"""#]];
    // Custom trim characters
    crate::common::assert_oracle_parity_expect(r#"(string-trim "---hello---" "-+")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-trim "***hello***" "[*]+" "[*]+")"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// string-prefix-p / string-suffix-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-prefix-p "hel" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-prefix-p "world" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-prefix-p "" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-prefix-p "hello" "hello")"#, expect);

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-suffix-p "llo" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-suffix-p "world" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-suffix-p "" "hello")"#, expect);
}

#[test]
fn oracle_prop_string_prefix_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    // IGNORE-CASE parameter (3rd arg)
    crate::common::assert_oracle_parity_expect(r#"(string-prefix-p "HEL" "hello" t)"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-prefix-p "HEL" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-suffix-p "LLO" "hello" t)"#, expect);
}

// ---------------------------------------------------------------------------
// string-search / string-replace
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "world" "hello world")"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "xyz" "hello world")"#, expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "o" "hello world")"#, expect);
}

#[test]
fn oracle_prop_string_search_start_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 7""#]];
    // START-POS parameter
    crate::common::assert_oracle_parity_expect(r#"(string-search "o" "hello world" 5)"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "o" "hello world" 8)"#, expect);
}

#[test]
fn oracle_prop_string_replace_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello emacs\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "world" "emacs" "hello world")"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"hell0 w0rld\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-replace "o" "0" "hello world")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "xyz" "abc" "hello world")"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex: text processing pipelines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV-like data into structured form
    let form = r####"(let ((csv "name,age,role\nAlice,30,engineer\nBob,25,designer"))
                    (let ((lines (split-string csv "\n"))
                          (result nil))
                      (let ((headers (split-string (car lines) ",")))
                        (dolist (line (cdr lines))
                          (let ((values (split-string line ","))
                                (record nil)
                                (h headers))
                            (while (and h values)
                              (setq record
                                    (cons (cons (car h) (car values))
                                          record))
                              (setq h (cdr h) values (cdr values)))
                            (setq result (cons (nreverse record) result)))))
                      (nreverse result)))"####;
    let expect = expect_test::expect![[
        r#""OK (((\"name\" . \"Alice\") (\"age\" . \"30\") (\"role\" . \"engineer\")) ((\"name\" . \"Bob\") (\"age\" . \"25\") (\"role\" . \"designer\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_path_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Path manipulation: split, normalize, rebuild
    let form = r####"(let ((path "/home/user/../user/./docs/file.txt"))
                    ;; Split into components
                    (let ((parts (split-string path "/" t)))
                      ;; Normalize: remove "." and handle ".."
                      (let ((stack nil))
                        (dolist (p parts)
                          (cond
                            ((string= p ".") nil)
                            ((string= p "..")
                             (when stack (setq stack (cdr stack))))
                            (t (setq stack (cons p stack)))))
                        ;; Rebuild
                        (concat "/" (mapconcat #'identity
                                              (nreverse stack) "/")))))"####;
    let expect = expect_test::expect![[r#""OK \"/home/user/docs/file.txt\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_word_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Word-wrap text at given width
    let form = r####"(let ((text "the quick brown fox jumps over the lazy dog")
                        (width 15))
                    (let ((words (split-string text))
                          (lines nil)
                          (current-line ""))
                      (dolist (word words)
                        (if (= (length current-line) 0)
                            (setq current-line word)
                          (if (<= (+ (length current-line) 1 (length word))
                                  width)
                              (setq current-line
                                    (concat current-line " " word))
                            (setq lines (cons current-line lines)
                                  current-line word))))
                      (when (> (length current-line) 0)
                        (setq lines (cons current-line lines)))
                      (nreverse lines)))"####;
    let expect =
        expect_test::expect![[r#""OK (\"the quick brown\" \"fox jumps over\" \"the lazy dog\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_camelcase_to_kebab() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert camelCase to kebab-case
    let form = r####"(let ((convert
                     (lambda (s)
                       (let ((result "")
                             (i 0)
                             (len (length s)))
                         (while (< i len)
                           (let ((ch (aref s i)))
                             (if (and (>= ch ?A) (<= ch ?Z))
                                 (setq result
                                       (concat result
                                               (if (> i 0) "-" "")
                                               (char-to-string
                                                (+ ch 32))))
                               (setq result
                                     (concat result
                                             (char-to-string ch)))))
                           (setq i (1+ i)))
                         result))))
                    (list (funcall convert "helloWorld")
                          (funcall convert "camelCaseString")
                          (funcall convert "XMLParser")
                          (funcall convert "simple")))"####;
    let expect = expect_test::expect![[
        r#""OK (\"hello-world\" \"camel-case-string\" \"x-m-l-parser\" \"simple\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_frequency_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Character frequency analysis
    let form = r####"(let ((text "hello world")
                        (freq (make-hash-table)))
                    (dotimes (i (length text))
                      (let ((ch (aref text i)))
                        (puthash ch (1+ (gethash ch freq 0)) freq)))
                    ;; Get sorted frequency list
                    (let ((pairs nil))
                      (maphash (lambda (k v)
                                 (setq pairs
                                       (cons (cons (char-to-string k) v)
                                             pairs)))
                               freq)
                      (sort pairs
                            (lambda (a b)
                              (> (cdr a) (cdr b))))))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"l\" . 3) (\"o\" . 2) (\"d\" . 1) (\"r\" . 1) (\"w\" . 1) (\" \" . 1) (\"e\" . 1) (\"h\" . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
