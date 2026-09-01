use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo100_org_full_circle_create_parse_edit_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-ok t) (:headline-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil))
  (insert "* TODO Task :work:\nSCHEDULED: <2024-01-15 Mon>\n:PROPERTIES:\n:EFFORT:   1:00\n:END:\nBody *bold*.\n")
  (let ((r '())) (goto-char (point-min)) (org-todo "DONE") (org-priority ?A)
   (org-entry-put nil "REVIEWER" "alice")
   (let ((out (org-export-as 'ascii nil nil t)))
    (push (list :export-ok (> (length out) 0)) r)
    (push (list :headline-count (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r))
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo100_org_babel_full_pipeline_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AB\" \"AB\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: base\n#+begin_src emacs-lisp :results output\n(princ \"A\")(princ \"B\")\n#+end_src\n\n")
  (insert "#+begin_src emacs-lisp :results output :var in=base\n(princ in)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results output")
   (push (org-babel-execute-src-block) r)
   (search-forward "#+begin_src emacs-lisp :results output :var in=base")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo100_org_agenda_complete_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todos 2) (:dones 2) (:work 2) (:urgent 1) (:scheduled 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-agenda) (require 'org-clock)
 (let ((org-clock-persist nil))
  (insert "* TODO A :work:\nSCHEDULED: <2024-06-01 Sat>\n** DONE A1 :work:\n")
  (insert "* TODO B :urgent:\nDEADLINE: <2024-06-15 Sat>\n* DONE C :home:\n")
  (let ((r '())) (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
   (push (list :todos (length (org-map-entries (lambda () t) "TODO=\"TODO\""))) r)
   (push (list :dones (length (org-map-entries (lambda () t) "TODO=\"DONE\""))) r)
   (push (list :work (length (org-map-entries (lambda () t) "work"))) r)
   (push (list :urgent (length (org-map-entries (lambda () t) "urgent"))) r)
   (push (list :scheduled (length (org-map-entries (lambda () t) "SCHEDULED<>\"\""))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo100_org_element_create_document_from_scratch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:has-TODO 16) (:has-star nil) (:has-table 107) (:stable t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-element)
 (let* ((data (org-element-create 'org-data nil
        (org-element-create 'keyword '(:key "TITLE" :value "Test"))
        (org-element-create 'headline '(:level 1 :raw-value "H1" :todo-keyword "TODO" :priority ?A :tags ("t"))
          (org-element-create 'section nil
            (org-element-create 'paragraph nil
              (org-element-create 'bold nil "B") " and " (org-element-create 'italic nil "I"))))
        (org-element-create 'headline '(:level 1 :raw-value "H2")
          (org-element-create 'section nil
            (org-element-create 'table '(:type org)
              (org-element-create 'table-row '(:type standard)
                (org-element-create 'table-cell nil "X")))))))
        (str (substring-no-properties (org-element-interpret-data data)))
        (r '()))
  (push (list :has-TODO (string-match-p "TODO" str)) r)
  (push (list :has-star (string-match-p "\\`\\*" str)) r)
  (push (list :has-table (string-match-p "|" str)) r)
  ;; reparse stability
  (let* ((data2 (with-temp-buffer (org-mode) (insert str) (goto-char (point-min))
                  (org-element-parse-buffer)))
         (h2 (length (org-element-map data2 'headline #'identity)))
         (t2 (length (org-element-map data2 'table #'identity))))
    (push (list :stable (= h2 (length (org-element-map data 'headline #'identity)))) r))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo100_org_export_all_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((ascii t) (html t) (latex t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* H\nBody *bold*.\n")
  (let ((r '())) (dolist (b '(ascii html latex md))
    (condition-case nil (let ((out (org-export-string-as (buffer-string) b t)))
     (push (list b (and out (> (length out) 0))) r))
    (error nil)))
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo100_org_table_eval_and_export_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "| a | b | c |\n| 1 | 2 |   |\n")
  (insert "#+TBLFM: $3=$1+$2\n") (goto-char (point-min)) (org-table-recalculate t)
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t))) (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo100_org_clock_effort_export_totals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock) (require 'ox-ascii)
 (let ((org-clock-persist nil) (org-export-show-temporary-export-buffer nil))
  (insert "* Task 1\n:PROPERTIES:\n:EFFORT:   0:30\n:END:\n")
  (insert "* Task 2\n:PROPERTIES:\n:EFFORT:   1:15\n:END:\n")
  (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (search-forward "* Task 2") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
  (goto-char (point-min)) (insert "#+BEGIN: clocktable :maxlevel 2 :scope file\n#+END:\n")
  (goto-char (point-min)) (search-forward "#+BEGIN:") (beginning-of-line) (org-dblock-update)
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t))) (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo100_org_mixed_all_elements_soup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+TITLE: Soup\n\n* TODO H :tag:\nSCHEDULED: <2024-01-01>\n:PROPERTIES:\n:KEY: val\n:END:\n")
 (insert "Body *bold* /italic/ _under_ +strike+ =code= ~verb~ [[link][desc]].\n")
 (insert "- item [X]\n  - sub [ ]\n| a | b |\n| 1 | 2 |\n[fn:1]\n[fn:1] Def.\n")
 (let* ((t (org-element-parse-buffer)) (types (delete-dups (mapcar #'org-element-type
   (org-element-map t t #'identity)))) (r '()))
  (dolist (type types) (push (list type (length (org-element-map t type #'identity))) r))
  (push (list :unique-types (length types)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo100_org_property_with_inheritance_deep_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:color-direct nil) (:color-inherit \"red\") (:color-select nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* Root\n:PROPERTIES:\n:COLOR: red\n:END:\n** Mid\n*** Leaf\n")
 (let ((r '())) (goto-char (point-min))
  (search-forward "*** Leaf") (beginning-of-line)
  (push (list :color-direct (org-entry-get nil "COLOR")) r)
  (push (list :color-inherit (org-entry-get nil "COLOR" t)) r)
  (push (list :color-select (org-entry-get nil "COLOR" 'selective)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo100_org_sort_all_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* B\n:PROPERTIES:\n:PRIO: 2\n:END:\n* A\n:PROPERTIES:\n:PRIO: 1\n:END:\n* C\n:PROPERTIES:\n:PRIO: 3\n:END:\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :init (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (goto-char (point-min)) (org-sort-entries nil ?a)
  (push (list :alpha (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  ;; sort by property numeric ascending
  (goto-char (point-min)) (org-sort-entries nil ?r ?p "PRIO" nil #'<)
  (push (list :prop-num (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
