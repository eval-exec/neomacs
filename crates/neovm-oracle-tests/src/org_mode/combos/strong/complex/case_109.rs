use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo109_org_protocol_client() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:loaded)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(condition-case nil(require'org-protocol)(error nil))(list:loaded(featurep'org-protocol):capture-fbound(fboundp'org-protocol-capture):open-fbound(fboundp'org-protocol-open-source)))"##,
        expect,
    );
}
#[test]
fn combo109_org_face_levels_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:l1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-faces)(list:l1(facep'org-level-1):l2(facep'org-level-2):l3(facep'org-level-3):l4(facep'org-level-4)))"##,
        expect,
    );
}
#[test]
fn combo109_org_bookmark_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:bm-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* Task\n")(goto-char(point-min))(condition-case nil(bookmark-set"test-bm")(error nil))(list:bm-fbound(fboundp'bookmark-set)))"##,
        expect,
    );
}
#[test]
fn combo109_org_element_normalize_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph nil \"s1\\n\" (bold nil \"b\") \"\\ns2\") (paragraph nil \"s1\\n\\ns2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-element)(list(org-element-normalize-contents'(paragraph nil"  s1\n" (bold nil"b")"\n  s2"))(org-element-normalize-contents'(paragraph nil"  s1\n\n  s2"))))"##,
        expect,
    );
}
#[test]
fn combo109_org_babel_shell_stdin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:stdin-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-core)(list:stdin-fbound(fboundp'org-babel-eval):shell-fbound(fboundp'org-babel-sh-command)))"##,
        expect,
    );
}
#[test]
fn combo109_org_todo_archive_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:done-heads)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* TODO A\n* DONE B\n* TODO C\n")(let((r()))(org-map-entries(lambda()(org-todo"DONE"))"TODO=\"TODO\"")(push(list:done-heads(org-map-entries(lambda()(org-get-heading t t t t))"TODO=\"DONE\""))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo109_org_agenda_file_list_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:files-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-agenda)(list:files-bound(boundp'org-agenda-files):files-count(when(boundp'org-agenda-files)(length org-agenda-files))))"##,
        expect,
    );
}
#[test]
fn combo109_org_babel_var_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ob-emacs-lisp)(let((org-confirm-babel-evaluate nil))(insert"#+begin_src emacs-lisp :results value :var x=99\nx\n#+end_src\n")(goto-char(point-min))(search-forward"#+begin_src")(push(org-babel-execute-src-block)nil)(list:ok t)))"##,
        expect,
    );
}
#[test]
fn combo109_org_align_tags_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:tags)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(let((org-tags-column -80))(insert"* H :t1:t2:\n")(goto-char(point-min))(list:tags(org-get-tags))))"##,
        expect,
    );
}
#[test]
fn combo109_org_count_items_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- a\n  - a1\n  - a2\n- b\n- c\n  - c1\n")(let*((t(org-element-parse-buffer))(items(org-element-map t'item'identity)))(list:count(length items):levels(mapcar(lambda(i)(org-element-property:level i))items))))"##,
        expect,
    );
}
