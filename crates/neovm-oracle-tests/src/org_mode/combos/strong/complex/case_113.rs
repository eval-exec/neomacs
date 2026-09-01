use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo113_org_latex_preview_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:preview-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list:preview-fbound(fboundp'org-latex-preview):toggle-fbound(fboundp'org-toggle-latex-fragment)))"##,
        expect,
    );
}
#[test]
fn combo113_org_list_indent_generic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:indent-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list:indent-fbound(fboundp'org-list-indent-item-generic):bullets-fbound(fboundp'org-list-bullet-string)))"##,
        expect,
    );
}
#[test]
fn combo113_org_mobile_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-mobile)(error nil))(list:loaded(featurep'org-mobile):edit-fbound(fboundp'org-mobile-edit)))"##,
        expect,
    );
}
#[test]
fn combo113_org_plot_gnuplot_script() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:gnuplot-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-plot)(list:gnuplot-fbound(fboundp'org-plot/gnuplot):script-fbound(fboundp'org-plot/gnuplot-script)))"##,
        expect,
    );
}
#[test]
fn combo113_org_protocol_store_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-protocol)(error nil))(list:loaded(featurep'org-protocol):store-fbound(fboundp'org-protocol-store-link)))"##,
        expect,
    );
}
#[test]
fn combo113_org_refile_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:verify-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-refile)(list:verify-fbound(boundp'org-refile-target-verify-function):use-outline-fbound(boundp'org-refile-use-outline-path)))"##,
        expect,
    );
}
#[test]
fn combo113_org_src_preserve_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:preserve-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-src)(list:preserve-bound(boundp'org-src-preserve-indentation):content-bound(boundp'org-edit-src-content-indentation)))"##,
        expect,
    );
}
#[test]
fn combo113_org_table_calc_formula_with_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:to-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| a | b | c |\n| 1 | 2 |   |\n| 3 | 4 |   |\n")(insert"#+TBLFM: $3='(+ $1 $2);N\n")(goto-char(point-min))(org-table-recalculate t)(org-table-align)(list:to-lisp(org-table-to-lisp)))"##,
        expect,
    );
}
#[test]
fn combo113_org_timer_set_timer_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:set-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-timer)(list:set-fbound(fboundp'org-timer-set-timer):default-bound(boundp'org-timer-default-timer)))"##,
        expect,
    );
}
#[test]
fn combo113_org_todo_sequence_with_fast_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(let((org-todo-keywords'((sequence"TODO(t)""WAIT(w)""|""DONE(d)""CANCELED(c)"))))(insert"* TODO Task\n")(goto-char(point-min))(let((r()))(push(org-get-todo-state)r)(org-todo)(push(org-get-todo-state)r)(nreverse r)))"##,
        expect,
    );
}
