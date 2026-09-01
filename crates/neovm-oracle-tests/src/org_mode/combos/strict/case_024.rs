//! combo_strict_24.rs — final fringe APIs
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_babel_ob_shell_script_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ob-shell-loaded nil :ob-sh-loaded nil :ob-bash ob-shell :ob-eshell-loaded ob-eshell)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :ob-shell-loaded (featurep 'ob-shell) :ob-sh-loaded (featurep 'ob-sh) :ob-bash (condition-case nil (require 'ob-shell) (error (featurep 'ob-shell))) :ob-eshell-loaded (condition-case nil (require 'ob-eshell) (error (featurep 'ob-eshell)))))"##,
        expect,
    );
}
#[test]
fn strict_ob_python_R_octave() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:python ob-python :R ob-R :octave ob-octave)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :python (condition-case nil (require 'ob-python) (error (featurep 'ob-python)))
 :R (condition-case nil (require 'ob-R) (error (featurep 'ob-R)))
 :octave (condition-case nil (require 'ob-octave) (error (featurep 'ob-octave)))))"##,
        expect,
    );
}
#[test]
fn strict_org_table_calc_formula_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:to-lisp ((#(\"a\" 0 1 (face org-table)) #(\"a / 0\" 0 5 (face org-table))) (#(\"1\" 0 1 (face org-table)) #(\"1/0\" 0 3 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"3/0\" 0 3 (face org-table))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a | b |\n| 1 | 2 |\n| 3 | 0 |\n") (insert "#+TBLFM: $2=$1/0\n")
 (let ((r '())) (goto-char (point-min))
  (condition-case e (org-table-recalculate t) (error (push (list :err (car e)) r)))
  (push (list :to-lisp (org-table-to-lisp)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_element_context_deep_nested_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ctx-S strike-through) (:ctx-I italic))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "*H* /I/ _U_ +S+\n") (goto-char (point-min))
 (let ((r '())) (search-forward "+S+") (backward-char 2)
  (push (list :ctx-S (org-element-type (org-element-context))) r)
  (search-backward "/I/") (forward-char 1) (push (list :ctx-I (org-element-type (org-element-context))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_agenda_follow_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:follow-fbound t :sticky-bound t :auto-exclude-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :follow-fbound (fboundp 'org-agenda-follow-mode) :sticky-bound (boundp 'org-agenda-sticky)
 :auto-exclude-fbound (boundp 'org-agenda-auto-exclude-function)))"##,
        expect,
    );
}
#[test]
fn strict_org_babel_hash_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:cache-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list :cache-p (boundp 'org-babel-cache-seen)))"##,
        expect,
    );
}
#[test]
fn strict_org_entity_latex_math_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:forall \"\\\\forall\" :exists \"\\\\exists\" :neg \"\\\\neg{}\" :in \"\\\\in\" :ni \"\\\\ni\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (list
 :forall (nth 1 (org-entity-get "forall")) :exists (nth 1 (org-entity-get "exists"))
 :neg (nth 1 (org-entity-get "neg")) :in (nth 1 (org-entity-get "in")) :ni (nth 1 (org-entity-get "ni"))))"##,
        expect,
    );
}
#[test]
fn strict_org_comment_block_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:at-comment t) (:at-fixed nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "# This is a comment line\n: This is fixed width\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :at-comment (org-at-comment-p)) r) (forward-line) (push (list :at-fixed (org-at-block-p)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_timestamp_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (let ((ts (org-timestamp-from-string "[2024-05-15 Wed]"))) (list :type (org-element-property :type ts)
  :year (org-element-property :year-start ts) :month (org-element-property :month-start ts) :day (org-element-property :day-start ts))))"##,
        expect,
    );
}
#[test]
fn strict_org_element_properties_for_all_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'org-element) (with-temp-buffer (org-mode)
 (insert "* TODO H :t:\nSCHEDULED: <2024-01-01>\n:PROPERTIES:\n:ID: x\n:END:\nBody\n")
 (let* ((t (org-element-parse-buffer)) (hl (car (org-element-map t 'headline #'identity))) (r '()))
  (dolist (prop '(:level :todo-keyword :priority :tags :title :raw-value :begin :end :post-blank
    :contents-begin :contents-end :pre-blank :archived-p :commentedp :footnote-section-p))
   (push (list prop (org-element-property prop hl)) r))
  (nreverse r)))))"##,
        expect,
    );
}
