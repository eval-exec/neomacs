use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo112_org_agenda_remove_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:remove-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-agenda)(list:remove-fbound(fboundp'org-agenda-remove-restriction-lock):set-fbound(fboundp'org-agenda-set-restriction-lock)))"##,
        expect,
    );
}
#[test]
fn combo112_org_babel_local_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:local-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-core)(list:local-fbound(fboundp'org-babel-temp-file):temp-dir-bound(boundp'org-babel-temporary-directory)))"##,
        expect,
    );
}
#[test]
fn combo112_org_capture_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:clock-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-capture)(list:clock-fbound(fboundp'org-capture-put-clock):get-clock-fbound(fboundp'org-capture-get-clock)))"##,
        expect,
    );
}
#[test]
fn combo112_org_clock_resolve_idle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:resolve-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-clock)(list:resolve-fbound(fboundp'org-resolve-clocks):idle-fbound(fboundp'org-user-idle-seconds)))"##,
        expect,
    );
}
#[test]
fn combo112_org_dblock_update_clocktable_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:ok)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-clock)(let((org-clock-persist nil))(insert"* A\n* B\n")(goto-char(point-min))(org-clock-in nil)(org-clock-out nil nil)(search-forward"* B")(beginning-of-line)(org-clock-in nil)(org-clock-out nil nil)(goto-char(point-min))(insert"#+BEGIN: clocktable :maxlevel 3 :scope file :block thisweek\n#+END:\n")(goto-char(point-min))(search-forward"#+BEGIN:")(beginning-of-line)(org-dblock-update)(list:ok t))))"##,
        expect,
    );
}
#[test]
fn combo112_org_element_interpret_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[[/tmp/test.org]]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-element)(list(org-element-interpret-data(org-element-create'link'(:type"file":path"/tmp/test.org":raw-link"file:/tmp/test.org")))))"##,
        expect,
    );
}
#[test]
fn combo112_org_export_html_headline_anchor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:ok)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'ox-html)(let((org-export-show-temporary-export-buffer nil))(insert"* H1\n** H2\n")(let((out(org-export-as'html nil nil t)))(list:ok(and out(>(length out)0))))))"##,
        expect,
    );
}
#[test]
fn combo112_org_footnote_normalize_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-property:label)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"A[fn:1]B[fn::inline]C\n[fn:1]x\n")(goto-char(point-min))(org-footnote-normalize)(let((r()))(push(mapcar(lambda(fr)(org-element-property:label fr))(org-element-map(org-element-parse-buffer)'footnote-reference'identity))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo112_org_habit_momentum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:build-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-habit)(list:build-fbound(fboundp'org-habit-build-graph):momentum-fbound(fboundp'org-habit-get-priority)))"##,
        expect,
    );
}
#[test]
fn combo112_org_id_update_location() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:add-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-id)(list:add-fbound(fboundp'org-id-add-location):remove-fbound(fboundp'org-id-remove-location)))"##,
        expect,
    );
}
