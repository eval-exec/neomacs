use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_org_entities_help_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:help-fbound t :total 436)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-entities)(list :help-fbound(fboundp'org-entities-help):total(length org-entities)))"##,
        expect,
    );
}
#[test]
fn strict_org_protocol_uri() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-protocol)(error nil))(list:loaded(featurep'org-protocol):capture-fbound(fboundp'org-protocol-capture)))"##,
        expect,
    );
}
#[test]
fn strict_org_mobile_files_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:files-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-mobile)(error nil))(list:files-bound(boundp'org-mobile-files):push-fbound(fboundp'org-mobile-push)))"##,
        expect,
    );
}
#[test]
fn strict_org_babel_ob_ref_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:resolve-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-ref)(list:resolve-fbound(fboundp'org-babel-ref-resolve):parse-fbound(fboundp'org-babel-ref-parse)))"##,
        expect,
    );
}
#[test]
fn strict_org_plot_script_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:script-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-plot)(list:script-fbound(fboundp'org-plot/gnuplot-script):script-to-data-fbound(fboundp'org-plot/gnuplot-to-data)))"##,
        expect,
    );
}
#[test]
fn strict_org_table_tab_first_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:tab-first-hook-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list:tab-first-hook-bound(boundp'org-tab-first-hook):cycle-fbound(fboundp'org-cycle)))"##,
        expect,
    );
}
#[test]
fn strict_org_list_search_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:at-item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- apple\n- banana\n- cherry\n")(goto-char(point-min))(search-forward"banana")(beginning-of-line)(list:at-item(org-at-item-p):item-bullet(org-list-bullet-string 1))))"##,
        expect,
    );
}
#[test]
fn strict_org_export_title_date_author() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:title)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox)(insert"#+TITLE: Test\n#+AUTHOR: X\n#+DATE: 2024-01\n")(let((info(org-export-get-environment)))(list:title(plist-get info:title):author(stringp(plist-get info:author)):date(stringp(plist-get info:date)))))"##,
        expect,
    );
}
#[test]
fn strict_org_babel_execute_results_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:result-count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ob-emacs-lisp)(let((org-confirm-babel-evaluate nil))(insert"#+begin_src emacs-lisp :results value wrap\n'(a b c)\n#+end_src\n")(goto-char(point-min))(search-forward"#+begin_src")(org-babel-execute-src-block)(list:result-count(length(org-element-map(org-element-parse-buffer)'result'identity)))))"##,
        expect,
    );
}
#[test]
fn strict_org_block_indent_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:indent-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list:indent-bound(boundp'org-adapt-indentation):edit-src-bound(boundp'org-src-preserve-indentation):content-bound(boundp'org-edit-src-content-indentation)))"##,
        expect,
    );
}
