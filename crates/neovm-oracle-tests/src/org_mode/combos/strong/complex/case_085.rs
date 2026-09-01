use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo85_call_org_links_delete_glue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ol)
 (insert "[[https://a.com][A]]\n") (let ((r '())) (goto-char (point-min))
  (let* ((t (org-element-parse-buffer)) (ls (org-element-map t 'link #'identity)))
   (push (list :link-type (mapcar (lambda (l) (org-element-property :type l)) ls)) r)
   (push (list :link-count (length ls)) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_insert_date() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ts-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((r '())) (condition-case nil (org-insert-time-stamp nil nil nil) (error nil))
  (push (list :ts-count (length (org-element-map (org-element-parse-buffer) 'timestamp #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_babel_go_to_header_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:goto-fbound t :previous-fbound t :next-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :goto-fbound (fboundp 'org-babel-goto-named-src-block) :previous-fbound (fboundp 'org-babel-previous-src-block)
 :next-fbound (fboundp 'org-babel-next-src-block)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_property_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:A \"10\") (:B \"20\") (:sum 30))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n:PROPERTIES:\n:A: 10\n:B: 20\n:END:\n")
 (let ((r '())) (goto-char (point-min)) (push (list :A (org-entry-get nil "A")) r)
  (push (list :B (org-entry-get nil "B")) r) (push (list :sum (+ (string-to-number (org-entry-get nil "A")) (string-to-number (org-entry-get nil "B")))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_table_sum_all_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n|   |   |   |\n")
 (insert "#+TBLFM: @>$1=vsum(@2..@-1)::@>$2=vmax(@2..@-1)::@>$3=vmin(@2..@-1)\n")
 (let ((r '())) (goto-char (point-min)) (org-table-recalculate t) (org-table-align) (goto-char (point-min))
  (forward-line 2) (push (list :sum (org-table-get "a" nil)) r) (push (list :max (org-table-get "b" nil)) r)
  (push (list :min (org-table-get "c" nil)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_sort_entries_by_todo_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* DONE Z\n* TODO A\n* TODO C\n* DONE B\n")
 (let ((r '())) (goto-char (point-min)) (org-sort-entries nil ?o)
  (push (list :sorted (mapcar (lambda (h) (list (substring-no-properties (org-element-property :raw-value h))
   (org-element-property :todo-keyword h))) (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_self_insert_command_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after \"aXbc\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "abc\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "b") (backward-char)
  (condition-case nil (self-insert-command 1 ?X) (error nil))
  (push (list :after (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_fill_paragraph_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:filled \"- aaaa bbbb cccc\\n  dddd eeee ffff\\n  gggg hhhh\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (setq fill-column 20)
 (insert "- aaaa bbbb cccc dddd eeee ffff gggg hhhh\n")
 (let ((r '())) (goto-char (point-min)) (org-fill-paragraph)
  (push (list :filled (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_increase_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after ((1 \"A\") (3 \"B\"))))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A\n** B\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "** B") (beginning-of-line)
  (condition-case nil (org-do-demote) (error nil))
  (push (list :after (mapcar (lambda (h) (list (org-element-property :level h)
   (substring-no-properties (org-element-property :raw-value h))))
   (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo85_call_org_renumber_ordered_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:after \"1. first\\n3. third\\n2. second\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "1. first\n3. third\n2. second\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (org-renumber-ordered-list) (error nil))
  (push (list :after (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
