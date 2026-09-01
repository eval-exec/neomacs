/// Batch 479: completion-table-dynamic, completion-table-merge, read-buffer deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx479_completion_table_in_turn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 '("hello" "help" "helicopter"))
      (t2 '("world" "worm" "worry")))
  (let ((t (completion-table-in-turn t1 t2)))
    (list (try-completion "hel" t) (all-completions "hel" t))))
"##,
        expect,
    );
}

#[test]
fn div_cx479_completion_table_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 '("hello" "world"))
      (t2 '("hello" "there")))
  (let ((t (completion-table-merge t1 t2)))
    (list (try-completion "hel" t) (all-completions "hel" t))))
"##,
        expect,
    );
}

#[test]
fn div_cx479_completion_table_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ap\" (\"apple\" \"apply\" \"apt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((table (completion-table-dynamic (lambda (s) '("apple" "apply" "apt")))))
  (list (try-completion "ap" table) (all-completions "ap" table)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_completion_boundaries_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tab '("hello" "help" "helicopter")))
  (completion-boundaries "hel" tab nil "world"))
"##,
        expect,
    );
}

#[test]
fn div_cx479_test_completion_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tab '("hello" "help" "helicopter")))
  (list (test-completion "hello" tab)
        (test-completion "hel" tab)
        (test-completion "nonexistent" tab)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_completion_metadata_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tab '("hello" "help" "helicopter")))
  (let ((md (completion-metadata "hel" tab nil)))
    (list (completion-metadata-get md 'category)
          (completion-metadata-get md 'cycling)
          (completion-metadata-get md 'flushable))))
"##,
        expect,
    );
}

#[test]
fn div_cx479_completion_all_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"hello\" 0 1 (face (completions-common-part completions-first-difference)) 1 3 (face completions-common-part)) #(\"help\" 0 1 (face (completions-common-part completions-first-difference)) 1 3 (face completions-common-part)) #(\"helicopter\" 0 1 (face (completions-common-part completions-first-difference)) 1 3 (face completions-common-part)) . 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tab '("hello" "help" "helicopter" "world")))
  (completion-all-completions "hel" tab nil 0))
"##,
        expect,
    );
}

#[test]
fn div_cx479_read_abbrev_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (fboundp 'read-abbrev))
"##,
        expect,
    );
}

#[test]
fn div_cx479_add_abbrev_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (list (fboundp 'add-mode-abbrev) (fboundp 'inverse-add-mode-abbrev)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_tempo_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'tempo)
  (list (boundp 'tempo-keywords) (fboundp 'tempo-define-template)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_skeleton_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'skeleton)
  (list (boundp 'skeleton-autowrap) (fboundp 'skeleton-insert)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_autoinsert_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'autoinsert)
  (list (boundp 'auto-insert-alist) (fboundp 'auto-insert)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_autoload_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'autoload)
  (list (boundp 'autoload-modified-buffers) (fboundp 'update-file-autoloads)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_copyright_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'copyright)
  (list (boundp 'copyright-query) (fboundp 'copyright-update)))
"##,
        expect,
    );
}

#[test]
fn div_cx479_revprop_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"revprop\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'revprop)
  (list (fboundp 'revprop-get) (fboundp 'revprop-put)))
"##,
        expect,
    );
}
