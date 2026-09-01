//! Divergence tests: project, xref, imenu, etags deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_project_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'project-current)
  (fboundp 'project-roots)
  (fboundp 'project-root)
  (featurep 'project))"#,
        expect,
    );
}

#[test]
fn divergence_xref_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'xref-find-definitions)
  (fboundp 'xref-find-references)
  (fboundp 'xref-pop-marker-stack)
  (featurep 'xref))"#,
        expect,
    );
}

#[test]
fn divergence_imenu_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'imenu)
  (fboundp 'imenu-add-to-menubar)
  (boundp 'imenu-auto-rescan)
  (featurep 'imenu))"#,
        expect,
    );
}

#[test]
fn divergence_etags_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'find-tag)
  (fboundp 'visit-tags-table)
  (boundp 'tags-file-name)
  (featurep 'etags))"#,
        expect,
    );
}

#[test]
fn divergence_compile_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'compile)
  (fboundp 'recompile)
  (fboundp 'kill-compilation)
  (featurep 'compile))"#,
        expect,
    );
}

#[test]
fn divergence_grep_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'grep)
  (fboundp 'lgrep)
  (fboundp 'rgrep)
  (featurep 'grep))"#,
        expect,
    );
}

#[test]
fn divergence_occur_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'occur)
  (fboundp 'multi-occur)
  (fboundp 'how-many)
  (fboundp 'keep-lines)
  (fboundp 'flush-lines)
  (fboundp 'delete-matching-lines))"#,
        expect,
    );
}

#[test]
fn divergence_ediff_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ediff-files)
  (fboundp 'ediff-buffers)
  (featurep 'ediff))"#,
        expect,
    );
}

#[test]
fn diff_tool_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'diff)
  (fboundp 'diff-backup)
  (featurep 'diff))"#,
        expect,
    );
}

#[test]
fn divergence_vcs_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'vc-dir)
  (fboundp 'vc-diff)
  (fboundp 'vc-log)
  (featurep 'vc)
  (featurep 'vc-git)
  (featurep 'vc-hg)
  (featurep 'vc-bzr))"#,
        expect,
    );
}
