use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo110_org_agenda_deadline_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:warn-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-agenda)(list:warn-bound(boundp'org-agenda-skip-deadline-prewarning-if-scheduled):days-bound(boundp'org-deadline-warning-days)))"##,
        expect,
    );
}
#[test]
fn combo110_org_babel_ob_awk_script() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'ob-awk)(error nil))(list:loaded(featurep'ob-awk):exec-fbound(fboundp'org-babel-execute:awk)))"##,
        expect,
    );
}
#[test]
fn combo110_org_timestamp_with_time_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(let((ts(org-timestamp-from-string"<2024-06-15 Sat 10:00-11:00>")))(list:type(org-element-property:type ts):h-start(org-element-property:hour-start ts):h-end(org-element-property:hour-end ts))))"##,
        expect,
    );
}
#[test]
fn combo110_org_table_hide_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:at-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| a | b |\n| 1 | 2 |\n")(goto-char(point-min))(forward-line 1)(list:at-table(org-at-table-p):cell-contents(org-table-get nil nil)))"##,
        expect,
    );
}
#[test]
fn combo110_org_export_inline_special_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:ok)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox-ascii)(let((org-export-show-temporary-export-buffer nil))(insert"#+BEGIN_ABSTRACT\nAbstract.\n#+END_ABSTRACT\n")(let((out(org-export-as'ascii nil nil t)))(list:ok(and out(>(length out)0))))))"##,
        expect,
    );
}
#[test]
fn combo110_org_id_get_create_with_location() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-id)(let((org-id-track-globally t))(insert"* H\n")(goto-char(point-min))(let((id(org-id-get-create)))(list:id-created(and id(stringp id)):id-added(and(fboundp'org-id-add-location)t)))))"##,
        expect,
    );
}
#[test]
fn combo110_org_check_list_off() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:checkboxes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- [ ] todo\n  - [ ] sub\n")(goto-char(point-min))(search-forward"todo")(beginning-of-line)(org-toggle-checkbox)(goto-char(point-min))(search-forward"sub")(beginning-of-line)(org-toggle-checkbox)(list:checkboxes(mapcar(lambda(i)(org-element-property:checkbox i))(org-element-map(org-element-parse-buffer)'item'identity))))"##,
        expect,
    );
}
#[test]
fn combo110_org_backward_forward_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:at-para2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"Para1.\n\nPara2.\n\nPara3.\n")(goto-char(point-max))(condition-case nil(backward-paragraph)(error nil))(list:at-para2(>(point)1)))"##,
        expect,
    );
}
#[test]
fn combo110_org_element_set_get_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-element)(insert"* A\n** B\n** C\n")(let*((t(org-element-parse-buffer))(h(car(org-element-map t'headline(lambda(h)(when(equal"A"(org-element-property:raw-value h))h))))))(list:children-count(length(org-element-contents h)):first-child-type(org-element-type(car(org-element-contents h))))))"##,
        expect,
    );
}
#[test]
fn combo110_org_table_sort_numeric_desc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:sorted)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| Score |\n|    95 |\n|    82 |\n|    99 |\n")(goto-char(point-min))(forward-line 1)(condition-case nil(org-table-sort-lines nil ?N)(error nil))(list:sorted(org-table-to-lisp)))"##,
        expect,
    );
}
