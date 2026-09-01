use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo108_org_publish_get_project() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:get-project-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ox-publish)(list:get-project-fbound(fboundp'org-publish-get-project-from-filename):files-fbound(fboundp'org-publish-get-base-files)))"##,
        expect,
    );
}
#[test]
fn combo108_org_babel_result_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:hash-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-core)(list:hash-fbound(fboundp'org-babel-result-hash):hash-to-params-fbound(fboundp'org-babel-hash-at-point)))"##,
        expect,
    );
}
#[test]
fn combo108_org_capture_template_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-capture)(let((org-capture-templates'(("t""Todo"entry(file+headline"""Tasks")"* TODO %?"))))(list:keys(mapcar'car org-capture-templates):descs(mapcar'cadr org-capture-templates))))"##,
        expect,
    );
}
#[test]
fn combo108_org_table_sort_descending() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:sorted)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| Name |\n| Zebra |\n| Apple |\n| Mango |\n")(goto-char(point-min))(forward-line 1)(condition-case nil(org-table-sort-lines nil ?a)(error nil))(list:sorted(org-table-to-lisp)))"##,
        expect,
    );
}
#[test]
fn combo108_org_timestamp_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(let((ts(org-timestamp-from-string"<2024-07-04 Thu -5d>")))(list:type(org-element-property:type ts):warning-type(org-element-property:warning-type ts):warning-value(org-element-property:warning-value ts):warning-unit(org-element-property:warning-unit ts))))"##,
        expect,
    );
}
#[test]
fn combo108_org_footnote_definition_begin_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"A[fn:1]\n[fn:1]Definition here.\n")(goto-char(point-max))(search-backward"[fn:1]Definition")(beginning-of-line)(let*((t(org-element-parse-buffer))(fd(car(org-element-map t'footnote-definition'identity))))(list:has-fd(and fd t):begin(when fd(org-element-property:begin fd)):end(when fd(org-element-property:end fd)))))"##,
        expect,
    );
}
#[test]
fn combo108_org_shifttab_overview_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:ov-invis)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* A\n** B\nBody.\n** C\nBody.\n* D\n")(let((r()))(goto-char(point-min))(org-shifttab 1)(push(list:ov-invis(get-char-property(point)'invisible))r)(org-shifttab 1)(push(list:cont-invis(get-char-property(point)'invisible))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo108_org_babel_expand_lob() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:lob-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-lob)(list:lob-fbound(fboundp'org-babel-lob-execute):lob-get-fbound(fboundp'org-babel-lob-get-info)))"##,
        expect,
    );
}
#[test]
fn combo108_org_element_put_delete_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-element)(insert"* H\nBody.\n")(let*((t(org-element-parse-buffer))(h(car(org-element-map t'headline'identity))))(org-element-put-property h :custom-attr "value")(org-element-put-property h :another 42)(list:custom(org-element-property:custom-attr h):another(org-element-property:another h):raw(substring-no-properties(org-element-property:raw-value h)))))"##,
        expect,
    );
}
#[test]
fn combo108_org_export_data_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox-ascii)(let((org-export-show-temporary-export-buffer nil))(insert"| a | b |\n| 1 | 2 |\n")(let*((t(org-element-parse-buffer))(info(org-export-get-environment))(tbl(car(org-element-map t'table'identity))))(list:table-ok(and tbl t):export-data(when(and tbl(fboundp'org-export-data))(condition-case nil(stringp(org-export-data tbl info))(error:err)))))))"##,
        expect,
    );
}
