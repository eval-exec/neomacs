use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo104_elisp_lambda_in_babel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ob-emacs-lisp)(let((org-confirm-babel-evaluate nil))(insert"#+begin_src emacs-lisp :results value\n((lambda(x)(* x x)) 9)\n#+end_src\n")(goto-char(point-min))(search-forward"#+begin_src")(push(org-babel-execute-src-block)nil)(list:ok t)))"##,
        expect,
    );
}
#[test]
fn combo104_org_link_file_plus_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"[[file:notes.org::*heading][Notes]]\n")(let*((t(org-element-parse-buffer))(ls(org-element-map t'link'identity)))(list:type(when(car ls)(org-element-property:type(car ls))):path(when(car ls)(org-element-property:path(car ls))):search(when(car ls)(org-element-property:search-option(car ls))))))"##,
        expect,
    );
}
#[test]
fn combo104_org_occur_multi_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:matched)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* Apple\n* BANANA\n* cherry\n* Data\n")(goto-char(point-min))(condition-case nil(org-occur"[a-z]") (error nil))(list:matched(length(org-element-map(org-element-parse-buffer nil t)'headline'identity)))(org-remove-occur-highlights))"##,
        expect,
    );
}
#[test]
fn combo104_org_agenda_skip_timestamp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:skip-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-agenda)(list:skip-fbound(boundp'org-agenda-skip-timestamp-if-deadline-is-shown):skip-sched-fbound(boundp'org-agenda-skip-scheduled-if-deadline-is-shown)))"##,
        expect,
    );
}
#[test]
fn combo104_org_babel_insert_result_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:insert-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-core)(list:insert-fbound(fboundp'org-babel-insert-result):remove-fbound(fboundp'org-babel-remove-inline-result):remove-block-fbound(fboundp'org-babel-remove-result-one-or-many)))"##,
        expect,
    );
}
#[test]
fn combo104_org_search_view_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:matched)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* A\n:PROPERTIES:\n:COLOR:red\n:END:\n* B\n:PROPERTIES:\n:COLOR:blue\n:END:\n")(goto-char(point-min))(condition-case nil(org-match-sparse-tree nil"COLOR={red}")(error nil))(list:matched(length(org-element-map(org-element-parse-buffer nil t)'headline'identity)))(org-remove-occur-highlights))"##,
        expect,
    );
}
#[test]
fn combo104_org_list_check_items() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"- [X] done\n- [ ] todo\n- [-] part\n")(let*((t(org-element-parse-buffer))(its(org-element-map t'item'identity)))(list:checkboxes(mapcar(lambda(i)(org-element-property:checkbox i))its):count(length its))))"##,
        expect,
    );
}
#[test]
fn combo104_org_babel_evaluate_lisp_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:result)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-core)(require'ob-emacs-lisp)(with-temp-buffer(org-mode)(let((org-confirm-babel-evaluate nil))(insert"#+begin_src emacs-lisp :results value :var x=1 :var y=2 :var z=3\n(+ x y z)\n#+end_src\n")(goto-char(point-min))(search-forward"#+begin_src")(list:result(org-babel-execute-src-block)))))"##,
        expect,
    );
}
#[test]
fn combo104_org_sort_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* A\n* B\n* C\n")(goto-char(point-min))(org-sort-entries nil ?a)(let((r()))(push(list:sorted(mapcar(lambda(h)(substring-no-properties(org-element-property:raw-value h)))(org-element-map(org-element-parse-buffer)'headline'identity)))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo104_org_timestamp_extract_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(let((ts(org-timestamp-from-string"<2024-12-25 Wed 15:45>")))(list:y(org-element-property:year-start ts):m(org-element-property:month-start ts):d(org-element-property:day-start ts):h(org-element-property:hour-start ts):min(org-element-property:minute-start ts))))"##,
        expect,
    );
}
