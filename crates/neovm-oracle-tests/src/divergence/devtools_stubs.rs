//! Divergence tests: undo-bridge, tree-sitter, project, xref stubs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_bridge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'undo-amalgamate-change-group)
  (fboundp 'undo-start-change-group)
  (fboundp 'undo-end-change-group))"#,
        expect,
    );
}

#[test]
fn divergence_undo_redo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'undo-only)
  (fboundp 'undo-redo)
  (fboundp 'undo-in-region)
  (fboundp 'undo-redo-in-region))"#,
        expect,
    );
}

#[test]
fn divergence_tree_sitter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (featurep 'treesit)
  (fboundp 'treesit-language-available-p)
  (fboundp 'treesit-parser-create)
  (fboundp 'treesit-node-type))"#,
        expect,
    );
}

#[test]
fn divergence_project_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'project-current)
  (fboundp 'project-root)
  (fboundp 'project-files)
  (fboundp 'project-buffers)
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
fn divergence_eglot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'eglot)
  (fboundp 'eglot-connect)
  (featurep 'eglot))"#,
        expect,
    );
}

#[test]
fn divergence_flymake() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'flymake-mode)
  (fboundp 'flymake-start)
  (fboundp 'flymake-diagnostic-functions)
  (featurep 'flymake))"#,
        expect,
    );
}

#[test]
fn divergence_vcs_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'vc-backend)
  (fboundp 'vc-dir)
  (fboundp 'vc-diff)
  (fboundp 'vc-log)
  (featurep 'vc))"#,
        expect,
    );
}

#[test]
fn divergence_magit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'magit-status)
  (featurep 'magit))"#,
        expect,
    );
}

#[test]
fn divergence_compilation_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'compile)
  (fboundp 'recompile)
  (fboundp 'compilation-start)
  (featurep 'compile))"#,
        expect,
    );
}
