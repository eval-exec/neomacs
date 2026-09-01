use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_babel_ob_calc_comint() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:calc ob-calc :comint ob-comint)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(list :calc(condition-case nil(require'ob-calc)(error(featurep'ob-calc))):comint(condition-case nil(require'ob-comint)(error(featurep'ob-comint)))))"##,
        expect,
    );
}
#[test]
fn strict_toggle_fixed_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:fw-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert": fixed\n: width\n")(goto-char(point-min))(list:fw-bound(fboundp'org-toggle-fixed-width):fw-lines(length(org-element-map(org-element-parse-buffer)'fixed-width'identity))))"##,
        expect,
    );
}
#[test]
fn strict_heading_tag_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"*  :tag1:tag2:tag3:\n")(let*((t(org-element-parse-buffer))(h(car(org-element-map t'headline'identity))))(list:tags(org-element-property:tags h):raw(substring-no-properties(or(org-element-property:raw-value h)"")):level(org-element-property:level h))))"##,
        expect,
    );
}
#[test]
fn strict_property_empty_value_and_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:empty)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* H\n")(goto-char(point-min))(org-entry-put nil"EMPTY" "")(org-entry-put nil"SPACE" " ")(list:empty(org-entry-get nil"EMPTY"):space(org-entry-get nil"SPACE")))"##,
        expect,
    );
}
#[test]
fn strict_timestamp_end_of_month() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list :jan31(let((ts(org-timestamp-from-string"<2024-01-31 Wed>")))(org-element-property:day-start ts)):mar31(let((ts(org-timestamp-from-string"<2024-03-31 Sun>")))(org-element-property:day-start ts)):apr30(let((ts(org-timestamp-from-string"<2024-04-30 Tue>")))(org-element-property:day-start ts))))"##,
        expect,
    );
}
#[test]
fn strict_link_raw_vs_path_discrepancy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"[[https://example.com/path?q=1][desc]]\n")(let*((t(org-element-parse-buffer))(l(car(org-element-map t'link'identity))))(list:raw(org-element-property:raw-link l):path(org-element-property:path l):type(org-element-property:type l))))"##,
        expect,
    );
}
#[test]
fn strict_sort_list_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:sorted)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- zebra\n- apple\n- mango\n")(goto-char(point-min))(condition-case nil(org-sort-list nil ?a)(error nil))(list:sorted(mapcar(lambda(i)(substring-no-properties(or(org-element-property:raw-value i)"")))(org-element-map(org-element-parse-buffer)'item'identity))))"##,
        expect,
    );
}
#[test]
fn strict_occur_by_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:visible)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* TODO A\n* DONE B\n* TODO C\n* DONE D\n")(goto-char(point-min))(condition-case nil(org-occur"TODO")(error nil))(list:visible(length(org-element-map(org-element-parse-buffer nil t)'headline'identity)))(org-remove-occur-highlights))"##,
        expect,
    );
}
#[test]
fn strict_export_latex_packages_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:packages-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ox-latex)(list:packages-bound(boundp'org-latex-packages-alist):count(when(boundp'org-latex-packages-alist)(length org-latex-packages-alist))))"##,
        expect,
    );
}
#[test]
fn strict_element_map_null_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* H\n")(let*((t(org-element-parse-buffer)))(list:with-nil-type(length(org-element-map t nil'identity)):with-t(length(org-element-map t t'identity)))))"##,
        expect,
    );
}
