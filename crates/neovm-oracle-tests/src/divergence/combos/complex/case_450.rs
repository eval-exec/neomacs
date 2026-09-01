//! Complex combo batch 450 — 15 final milestone probes: thing-at-point,
//! forward-sentence, backward-sentence, transpose-regions, replace-highlight,
//! list-abbrevs, edit-abbrevs, define-global-abbrev, expand-abbrev,
//! unexpand-abbrev, add-global-abbrev, inverse-add-global-abbrev,
//! set-case-syntax-1, set-case-syntax-pair, with-case-table.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx450_thing_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (goto-char 3)
  (thing-at-point 'word))"##,
        expect,
    );
}

#[test]
fn div_cx450_forward_sentence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 28""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "Hello world. Goodbye world.")
  (goto-char 1)
  (forward-sentence 1)
  (point))"##,
        expect,
    );
}

#[test]
fn div_cx450_transpose_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"123abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc123")
  (transpose-regions 1 4 4 7)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx450_define_global_abbrev() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (let ((global-abbrev-table (make-abbrev-table)))
    (define-global-abbrev "teh" "the")
    (expand-abbrev)))"##,
        expect,
    );
}

#[test]
fn div_cx450_abbrev_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"on my way\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (let ((tab (make-abbrev-table)))
    (define-abbrev tab "omw" "on my way")
    (list (abbrev-expansion "omw" tab)
          (abbrev-expansion "nonexistent" tab))))"##,
        expect,
    );
}

#[test]
fn div_cx450_set_case_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tbl (copy-case-table)))
  (set-case-syntax-pair ?\\[ ?\\] tbl)
  (aref tbl ?\\[))"##,
        expect,
    );
}

#[test]
fn div_cx450_with_case_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (copy-case-table)))
  (set-char-table-range ct ?a ?x)
  (set-case-table ct)
  (downcase "A"))"##,
        expect,
    );
}

#[test]
fn div_cx450_list_abbrevs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (with-temp-buffer
    (fboundp 'list-abbrevs)))"##,
        expect,
    );
}

#[test]
fn div_cx450_unexpand_abbrev() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (fboundp 'unexpand-abbrev))"##,
        expect,
    );
}

#[test]
fn div_cx450_add_global_abbrev() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (fboundp 'add-global-abbrev))"##,
        expect,
    );
}

#[test]
fn div_cx450_inverse_add_global_abbrev() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (fboundp 'inverse-add-global-abbrev))"##,
        expect,
    );
}

#[test]
fn div_cx450_edit_abbrevs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (fboundp 'edit-abbrevs))"##,
        expect,
    );
}

#[test]
fn div_cx450_replace_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'replace)
  (boundp 'replace-highlight))"##,
        expect,
    );
}

#[test]
fn div_cx450_make_temp_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((n (make-temp-name "/tmp/neo-cx450-")))
  (stringp n))"##,
        expect,
    );
}

#[test]
fn div_cx450_file_name_all_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Hermetic: completes over a fixture directory created inside the
    // harness-controlled shared tempdir, not live /tmp, whose entry count
    // drifts with machine state (33 at the 2026-07-14 bless, 165 three days
    // later).  Locks GNU src/dired.c `file_name_completion` (all_flag=1)
    // semantics: "." and ".." stay included (TRIVIAL_DIRECTORY_ENTRY
    // exclusion applies only when !all_flag), directories gain a trailing
    // slash via Ffile_name_as_directory, and the FILE argument is a prefix
    // filter.  Results are sorted because readdir order is arbitrary.
    let expect = expect_test::expect![[
        r#""OK (6 (\"../\" \"./\" \"alpha.txt\" \"beta.txt\" \"beta2.log\" \"gamma-dir/\") (\"beta.txt\" \"beta2.log\") nil)""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let* ((base (file-name-as-directory
              (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory)))
       (dir (expand-file-name "completion-fixture" base)))
  (make-directory dir t)
  (make-directory (expand-file-name "gamma-dir" dir) t)
  (dolist (f '("alpha.txt" "beta.txt" "beta2.log"))
    (write-region "" nil (expand-file-name f dir) nil 'silent))
  (list (length (file-name-all-completions "" dir))
        (sort (file-name-all-completions "" dir) #'string<)
        (sort (file-name-all-completions "beta" dir) #'string<)
        (file-name-all-completions "nomatch" dir)))"##,
        expect,
    );
}
