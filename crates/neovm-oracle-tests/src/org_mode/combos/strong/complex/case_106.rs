use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo106_org_refile_targets_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* Tasks\n** TODO A\n** DONE B\n* Notes\n")(let((r()))(let((tgts(org-refile-get-targets)))(push(list:count(length tgts))r)(push(list:names(mapcar'car tgts))r))(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo106_org_babel_noweb_prefix_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ob-emacs-lisp)(let((org-confirm-babel-evaluate nil))(insert"#+name: pre\n#+begin_src emacs-lisp\n(setq pre-x 5)\n#+end_src\n\n")(insert"#+begin_src emacs-lisp :results value :noweb yes\n<<pre>>\n(+ pre-x 10)\n#+end_src\n")(let((r()))(goto-char(point-min))(search-forward"#+begin_src emacs-lisp")(org-babel-execute-src-block)(search-forward"#+begin_src emacs-lisp")(push(org-babel-execute-src-block)r)(nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo106_org_export_ignore_headings_noexport_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:has-B)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox-ascii)(let((org-export-show-temporary-export-buffer nil)(org-export-exclude-tags'("noexport")))(insert"* A :noexport:\nBody A.\n* B\nBody B.\n")(let((r()))(let((out(org-export-as'ascii nil nil t)))(push(list:has-B(and out(string-match-p"Body B"out)))r)(push(list:no-A(and out(not(string-match-p"Body A"out))))r))(nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo106_org_move_subtree_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"Cannot move past superior level or buffer limit\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* A\n** B\n* C\n* D\n")(let((r()))(goto-char(point-min))(search-forward"** B")(beginning-of-line)(org-metadown)(org-metadown)(push(list:after-move(mapcar(lambda(h)(substring-no-properties(org-element-property:raw-value h)))(org-element-map(org-element-parse-buffer)'headline'identity)))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo106_org_use_sub_superscripts_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:sub-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org)(list:sub-bound(boundp'org-use-sub-superscripts):default(when(boundp'org-use-sub-superscripts)org-use-sub-superscripts)))"##,
        expect,
    );
}
#[test]
fn combo106_org_timestamp_active_to_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:after)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"<2024-06-15 Sat>\n")(goto-char(point-min))(condition-case nil(org-toggle-timestamp-type)(error nil))(list:after(buffer-string)))"##,
        expect,
    );
}
#[test]
fn combo106_org_dblock_update_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:ok)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-clock)(let((org-clock-persist nil))(insert"* Task\n")(goto-char(point-min))(org-clock-in nil)(org-clock-out nil nil)(goto-char(point-min))(insert"#+BEGIN: clocktable :maxlevel 3 :scope file :block today\n#+END:\n")(goto-char(point-min))(search-forward"#+BEGIN:")(beginning-of-line)(org-dblock-update)(list:ok(>(length(buffer-string))0))))"##,
        expect,
    );
}
#[test]
fn combo106_org_element_find_property_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"* H\n:PROPERTIES:\n:A:1\n:END:\nBody.\n")(goto-char(point-min))(search-forward":PROPERTIES:")(beginning-of-line)(let*((t(org-element-parse-buffer))(h(car(org-element-map t'headline'identity)))(pd(car(org-element-map h'property-drawer'identity))))(list:has-pd(and pd t):pd-begin(when pd(org-element-property:begin pd)):pd-end(when pd(org-element-property:end pd)))))"##,
        expect,
    );
}
#[test]
fn combo106_org_export_html_toc_ranking() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:has-h1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox-html)(let((org-export-show-temporary-export-buffer nil))(insert"* H1\n** H2\nBody.\n")(let((out(org-export-as'html nil nil t)))(list:has-h1(and out(string-match-p"H1"out)):has-h2(and out(string-match-p"H2"out)):has-toc(and out(string-match-p"table-of-contents"out))))))"##,
        expect,
    );
}
#[test]
fn combo106_org_table_decimal_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:to-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"| 1.5 |\n| 20.75 |\n| 300.0 |\n")(goto-char(point-min))(org-table-align)(list:to-lisp(org-table-to-lisp)))"##,
        expect,
    );
}
