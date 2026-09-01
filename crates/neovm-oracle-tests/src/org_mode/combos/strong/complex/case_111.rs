use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo111_org_archive_subtree_confirm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:archive-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-archive)(list:archive-fbound(fboundp'org-archive-subtree):default-fbound(fboundp'org-archive-subtree-default)))"##,
        expect,
    );
}
#[test]
fn combo111_org_babel_strip_blank() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:trimmed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ob-core)(with-temp-buffer(org-mode)(insert"  hello  \n")(goto-char(point-min))(list:trimmed(org-babel-trim(org-babel-chomp(buffer-string)"[\n\r]")))))"##,
        expect,
    );
}
#[test]
fn combo111_org_capture_template_annotation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:annot-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-capture)(list:annot-fbound(fboundp'org-capture-fill-template):store-fbound(fboundp'org-store-link)))"##,
        expect,
    );
}
#[test]
fn combo111_org_clock_table_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:ok)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-clock)(let((org-clock-persist nil))(insert"* Task\n:PROPERTIES:\n:CATEGORY:dev\n:END:\n")(goto-char(point-min))(org-clock-in nil)(org-clock-out nil nil)(goto-char(point-min))(insert"#+BEGIN: clocktable :maxlevel 2 :scope file :properties(\"CATEGORY\")\n#+END:\n")(goto-char(point-min))(search-forward"#+BEGIN:")(beginning-of-line)(org-dblock-update)(list:ok(>(length(buffer-string))0))))"##,
        expect,
    );
}
#[test]
fn combo111_org_dblock_clocktable_compact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:dblock-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-clock)(list:dblock-fbound(fboundp'org-dblock-write:clocktable):update-fbound(fboundp'org-dblock-update)))"##,
        expect,
    );
}
#[test]
fn combo111_org_element_adopt_same_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(require'org-element)(insert"* P\n** C1\n** C2\n")(let*((t(org-element-parse-buffer))(P(car(org-element-map t'headline(lambda(h)(when(equal"P"(org-element-property:raw-value h))h)))))(kids(org-element-map P'headline'identity))(r()))(push(list:init-count(length kids))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo111_org_export_include_file_minlevel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:include-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'ox)(list:include-fbound(boundp'org-export-include-keyword):include-minlevel-fbound(fboundp'org-export-insert-default-template)))"##,
        expect,
    );
}
#[test]
fn combo111_org_footnote_definition_normalize_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer(org-mode)(insert"Z[fn:z]A[fn:a]M[fn:m]\n[fn:z]z\n[fn:a]a\n[fn:m]m\n")(goto-char(point-min))(org-footnote-normalize'sort)(let((r()))(push(mapcar(lambda(fr)(org-element-property:label fr))(org-element-map(org-element-parse-buffer)'footnote-reference'identity))r)(nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo111_org_habit_consistency_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:graph-fbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-habit)(list:graph-fbound(fboundp'org-habit-insert-consistency-graphs):parse-fbound(fboundp'org-habit-parse-todo)))"##,
        expect,
    );
}
#[test]
fn combo111_org_id_locations_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function list:file-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn(require'org-id)(list:file-bound(boundp'org-id-locations-file):file-value(when(boundp'org-id-locations-file)org-id-locations-file)))"##,
        expect,
    );
}
