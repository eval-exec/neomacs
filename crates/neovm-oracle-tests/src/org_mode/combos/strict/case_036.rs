use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_org_indent_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:indent-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-indent)(list:indent-fbound(fboundp'org-indent-mode):initialized-fbound(boundp'org-indent-indentation-per-level)))"##,
        expect,
    );
}
#[test]
fn strict_org_key_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:keys-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-keys)(list:keys-fbound(fboundp'org-speed-command-help):speed-fbound(boundp'org-speed-commands)))"##,
        expect,
    );
}
#[test]
fn strict_org_latex_to_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:latex-fragment-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ox-html)(list:latex-fragment-fbound(fboundp'org-html-latex-fragment):latex-env-fbound(fboundp'org-html-latex-environment)))"##,
        expect,
    );
}
#[test]
fn strict_org_list_to_descriptive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- a ::")(goto-char(point-min))(let*((t(org-element-parse-buffer))(its(org-element-map t'item'identity)))(list:tags(mapcar(lambda(i)(org-element-property:tag i))its):count(length its))))"##,
        expect,
    );
}
#[test]
fn strict_org_mobile_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-mobile)(error nil))(list:loaded(featurep'org-mobile):pull-fbound(fboundp'org-mobile-pull):push-fbound(fboundp'org-mobile-push)))"##,
        expect,
    );
}
#[test]
fn strict_org_plot_table_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-plot)(insert"#+PLOT: title:\"Test\" ind:1 type:2d with:lines\n| X | Y |\n| 1 | 2 |\n| 3 | 4 |\n")(goto-char(point-min))(let*((t(org-element-parse-buffer))(keywords(org-element-map t'keyword(lambda(k)(when(equal"PLOT"(org-element-property:key k))k)))))(list:plot-keywords(length keywords):to-lisp(org-table-to-lisp))))"##,
        expect,
    );
}
#[test]
fn strict_org_protocol_open_source() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-protocol)(error nil))(list:loaded(featurep'org-protocol):open-fbound(fboundp'org-protocol-open-source)))"##,
        expect,
    );
}
#[test]
fn strict_org_refile_goto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:goto-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-refile)(list:goto-fbound(fboundp'org-refile-goto-last-stored):targets-fbound(fboundp'org-refile-get-targets)))"##,
        expect,
    );
}
#[test]
fn strict_org_src_get_lang_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:lang-mode-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-src)(list:lang-mode-fbound(fboundp'org-src-get-lang-mode):edit-fbound(fboundp'org-edit-src-code)))"##,
        expect,
    );
}
#[test]
fn strict_org_table_blank_after_recalc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:to-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n|   |   |   |\n")(insert"#+TBLFM: @>$1=vsum(@2..@-1)::$3=$1+$2\n")(goto-char(point-min))(org-table-recalculate t)(org-table-align)(list:to-lisp(org-table-to-lisp)))"##,
        expect,
    );
}
