//! Regex match-data parity: explicit numbered groups (\(?N:...\)), shy
//! groups, match-data (integers/markers/reuse), set/save-match-data,
//! repetition bounds \{n,m\}, anchors \`/\'/\_</\_>; while-let; plus the
//! replace-region-contents function-argument divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn md_explicit_numbered_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"123\" \"abc\" 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "\\(?2:[a-z]+\\)\\(?1:[0-9]+\\)" "abc123")
  (list (match-string 1 "abc123") (match-string 2 "abc123")
        (match-beginning 1) (match-end 2)))"##,
        expect,
    );
}

#[test]
fn md_match_data_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 3 1 2 2 3) 6 (1 3 1 2 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "\\(a\\)\\(b\\)" "xaby")
  (let ((md (match-data)))
    (list md (length md) (match-data t))))"##,
        expect,
    );
}

#[test]
fn md_match_data_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"wor\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (goto-char (point-min))
  (re-search-forward "\\(wor\\)ld" nil t)
  (let ((md (match-data t)))
    (list (integerp (nth 0 md)) (match-string 1)
          (let ((mm (match-data nil (list nil nil)))) (markerp (nth 0 (progn (goto-char 1) (re-search-forward "hello") (match-data t t))))))))"##,
        expect,
    );
}

#[test]
fn md_regex_alternation_anchors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 3 4 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-match "\\`foo" "foobar")
        (string-match "bar\\'" "foobar")
        (progn (string-match "\\(?:cat\\|dog\\)s?" "cats") (match-end 0))
        (string-match "\\_<word\\_>" "a word b"))"##,
        expect,
    );
}

#[test]
fn md_regex_repetition_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 4 2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (progn (string-match "a\\{2,3\\}" "aaaa") (match-end 0))
        (progn (string-match "a\\{2,\\}" "aaaa") (match-end 0))
        (progn (string-match "a\\{,2\\}" "aaaa") (match-end 0))
        (string-match "a\\{0\\}b" "b"))"##,
        expect,
    );
}

#[test]
fn divergence_replace_region_contents_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"REPLACED\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((src (generate-new-buffer " neo-rrc-xxx")))
  (with-current-buffer src (insert "REPLACED"))
  (prog1 (with-temp-buffer (insert "original text")
           (replace-region-contents (point-min) (point-max) (lambda () src))
           (buffer-string))
    (kill-buffer src)))"##,
        expect,
    );
}

#[test]
fn md_save_match_data_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"foo\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "foo" "foobar")
  (save-match-data (string-match "bar" "bar"))
  (list (match-beginning 0) (match-string 0 "foobar")))"##,
        expect,
    );
}

#[test]
fn md_set_match_data_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "abc" "xabcy")
  (let ((saved (match-data)))
    (string-match "z" "z")
    (set-match-data saved)
    (list (match-beginning 0) (match-end 0))))"##,
        expect,
    );
}

#[test]
fn md_shy_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ababc\" \"c\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (string-match "\\(?:ab\\)+\\(c\\)" "ababc")
  (list (match-string 0 "ababc") (match-string 1 "ababc") (match-beginning 1)))"##,
        expect,
    );
}

#[test]
fn md_while_let_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((data '(1 2 3 nil 5)) (acc nil))
  (while-let ((x (pop data)) ((numberp x)))
    (push (* x x) acc))
  (nreverse acc))"##,
        expect,
    );
}
