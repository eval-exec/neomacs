use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo107_org_babel_multiple_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ob-emacs-lisp)(let((org-confirm-babel-evaluate nil))(insert"#+begin_src emacs-lisp :results value\n1\n#+end_src\n\n#+begin_src emacs-lisp :results value\n2\n#+end_src\n")(let((r()))(goto-char(point-min))(search-forward"#+begin_src")(push(org-babel-execute-src-block)r)(search-forward"#+begin_src")(push(org-babel-execute-src-block)r)(push(list:count(length(org-element-map(org-element-parse-buffer)'result'identity)))r)(nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo107_org_link_descriptive_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"[[https://a.com]]\n")(let*((t(org-element-parse-buffer))(l(car(org-element-map t'link'identity))))(list:type(org-element-property:type l):path(org-element-property:path l):raw-link(org-element-property:raw-link l))))"##,
        expect,
    );
}
#[test]
fn combo107_org_entities_replace_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:alpha-utf8 \"alpha\" :beta-utf8 \"beta\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-entities)(list :alpha-utf8(nth 5(org-entity-get"alpha")):beta-utf8(nth 5(org-entity-get"beta"))))"##,
        expect,
    );
}
#[test]
fn combo107_org_todo_cycle_with_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:all-done)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* TODO A\n* TODO B\n* DONE C\n")(let((r()))(goto-char(point-min))(org-map-entries(lambda()(org-todo"DONE"))"TODO=\"TODO\"")(push(list:all-done(org-map-entries(lambda()(org-get-todo-state))))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo107_org_agenda_batch_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:todos)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-agenda)(insert"* TODO A :work:\nSCHEDULED:<2024-01-15>\n* TODO B :home:\nDEADLINE:<2024-01-20>\n")(let((r()))(push(list:todos(length(org-map-entries(lambda()t)"TODO=\"TODO\"")))r)(push(list:scheduled(length(org-map-entries(lambda()t)"SCHEDULED<>\"\"")))r)(push(list:deadlines(length(org-map-entries(lambda()t)"DEADLINE<>\"\"")))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo107_org_set_tags_via_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer(org-mode)(insert"* H :old:\n")(goto-char(point-min))(condition-case nil(org-set-tags-command)(error nil))(list:after-tags(org-get-tags)))"##,
    );
}
#[test]
fn combo107_org_sparse_by_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:matched)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* alpha\n* beta\n* gamma\n")(goto-char(point-min))(condition-case nil(org-occur"[ab]") (error nil))(list:matched(length(org-element-map(org-element-parse-buffer nil t)'headline'identity)))(org-remove-occur-highlights))"##,
        expect,
    );
}
#[test]
fn combo107_org_enum_list_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:item-2-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"1. one\n2. two\n3. three\n")(let((r()))(goto-char(point-min))(search-forward"2. two")(beginning-of-line)(push(list:item-2-type(org-element-property:counternil(org-element-at-point)))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo107_org_indent_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:col)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(let((org-adapt-indentation t))(insert"* H\nBody.")(goto-char(point-max))(condition-case nil(org-indent-line)(error nil))(list:col(current-column))))"##,
        expect,
    );
}
#[test]
fn combo107_org_footnote_normalize_flat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:refs)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"A[fn:3]B[fn:1]C[fn:2]\n[fn:1]a\n[fn:2]b\n[fn:3]c\n")(goto-char(point-min))(org-footnote-normalize)(let((r()))(push(list:refs(mapcar(lambda(fr)(org-element-property:label fr))(org-element-map(org-element-parse-buffer)'footnote-reference'identity)))r)(nreverse r)))"##,
        expect,
    );
}
