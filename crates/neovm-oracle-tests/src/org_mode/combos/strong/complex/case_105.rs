use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo105_org_entry_delete_property_recover() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:after-del)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* H\n:PROPERTIES:\n:A:1\n:B:2\n:END:\n")(goto-char(point-min))(org-entry-delete nil"A")(list:after-del(org-entry-get nil"A"):still-there(org-entry-get nil"B")))"##,
        expect,
    );
}
#[test]
fn combo105_org_table_move_row_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:after)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| a |\n| 1 |\n| 2 |\n")(goto-char(point-min))(forward-line 1)(condition-case nil(org-table-move-row)(error nil))(list:after(org-table-to-lisp)))"##,
        expect,
    );
}
#[test]
fn combo105_org_export_selective_tags_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:has-B)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox-ascii)(let((org-export-show-temporary-export-buffer nil)(org-export-select-tags'("keep")))(insert"* A\n* B :keep:\n")(let((r()))(let((out(org-export-as'ascii nil nil t)))(push(list:has-B(and out(string-match-p"B"out)))r))(nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo105_org_id_get_create_5_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-id)(insert"* A\n* B\n* C\n* D\n* E\n")(let((ids()))(while(re-search-forward"^\\* "nil t)(push(org-id-get-create)ids))(list:count(length ids):unique(length(delete-dups ids)))))"##,
        expect,
    );
}
#[test]
fn combo105_org_babel_tangle_comment_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:comment-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-tangle)(list:comment-fbound(fboundp'org-babel-tangle-comment-links):link-fbound(fboundp'org-babel-tangle-collect-blocks)))"##,
        expect,
    );
}
#[test]
fn combo105_org_agenda_file_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:file-p-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-agenda)(list:file-p-fbound(fboundp'org-agenda-file-p):has-buffer(progn(set-buffer(get-buffer-create"*test*"))(with-temp-buffer(org-mode)(condition-case nil(org-agenda-file-p)(error:err)))))))"##,
        expect,
    );
}
#[test]
fn combo105_org_column_view_get_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:format-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-colview)(let((org-columns-default-format"%ITEM %TODO"))(list:format-fbound(fboundp'org-columns-get-format):default(when(fboundp'org-columns-get-format)(condition-case nil(org-columns-get-format)(error:err))))))"##,
        expect,
    );
}
#[test]
fn combo105_org_list_indent_to_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:level)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- apple\n- banana\n")(goto-char(point-min))(forward-line 1)(condition-case nil(progn(org-metaright)(list:level(org-element-property:level(org-element-at-point))))(error:err)))"##,
        expect,
    );
}
#[test]
fn combo105_org_update_statistics_nonexistent_cookie() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* H\n")(goto-char(point-min))(condition-case nil(org-update-statistics-cookies t)(error:no-cookie))(list:buffer(buffer-string)))"##,
        expect,
    );
}
#[test]
fn combo105_org_export_inline_images_setting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:inline-image-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list:inline-image-bound(boundp'org-display-inline-images):toggle-fbound(fboundp'org-toggle-inline-images)))"##,
        expect,
    );
}
