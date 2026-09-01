use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_agenda_tags_and_todo_views_file_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t nil \"Headlines with TAGS match: +work\\nPress ‘C-u r’ to search again\\nProbe:  TODO Alpha                                                       :work:\\nProbe:  DONE Gamma                                                       :work:\\n\" \"Global list of TODO items of type: ALL\\nPress ‘N r’ (e.g. ‘0 r’) to search again: (0)[ALL] (1)DONE (2)TODO\\nProbe:  TODO Alpha                                                       :work:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-tags" nil ".org"
                "#+CATEGORY: Probe
* TODO Alpha :work:
SCHEDULED: <2026-05-27 Wed 09:00>
:PROPERTIES:
:Effort: 1:00
:END:
* WAIT Beta :home:
DEADLINE: <2026-05-28 Thu>
* DONE Gamma :work:
CLOSED: [2026-05-26 Tue]
"))
         (org-agenda-files (list file))
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-prefix-format "%?-12t%-8:c% s")
         (org-agenda-start-day "2026-05-27")
         (org-agenda-span 3)
         (org-agenda-start-on-weekday nil))
    (unwind-protect
        (progn
          (org-tags-view nil "+work")
          (let ((tags (with-current-buffer org-agenda-buffer-name
                        (buffer-substring-no-properties
                         (point-min) (point-max)))))
            (kill-buffer org-agenda-buffer-name)
            (org-todo-list nil)
            (let ((todos (with-current-buffer org-agenda-buffer-name
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (list (not (null (string-match-p "Alpha" tags)))
                    (not (null (string-match-p "Gamma" tags)))
                    (not (null (string-match-p "TODO Alpha" todos)))
                    (not (null (string-match-p "WAIT Beta" todos)))
                    tags
                    todos))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_table_recalculate_delete_column_formula_rewrite_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"| Name | Qty | Price | Total |\\n|------+-----+-------+-------|\\n| b    |   2 |     3 |     6 |\\n| a    |   5 |     4 |    20 |\\n#+TBLFM: $4=$2*$3\\n\" \"| Name | Qty | Price | Total |\\n|------+-----+-------+-------|\\n| b    |   2 |     3 |     6 |\\n| a    |   5 |     4 |    20 |\\n#+TBLFM: $4=$2*$3\\n\" \"| Name | Qty | Total |\\n|------+-----+-------|\\n| b    |   2 |     6 |\\n| a    |   5 |    20 |\\n#+TBLFM: $3=$2*$INVALID\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Qty | Price | Total |\n")
    (insert "|------+-----+-------+-------|\n")
    (insert "| b | 2 | 3 | |\n")
    (insert "| a | 5 | 4 | |\n")
    (insert "#+TBLFM: $4=$2*$3\n")
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (let ((after-calc
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "Name")
      (org-table-sort-lines nil ?a)
      (let ((after-sort
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "Price")
        (org-table-delete-column)
        (list after-calc
              after-sort
              (buffer-substring-no-properties (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_table_copy_move_column_row_to_lisp_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"| A | B |\\n|---+---|\\n| 1 | 2 |\\n| 2 | 4 |\\n\" \"| B | A |\\n|---+---|\\n| 2 | 1 |\\n| 4 | 2 |\\n\" \"| B | A |\\n| 2 | 1 |\\n|---+---|\\n| 4 | 2 |\\n\" ((#(\"B\" 0 1 (face org-table)) #(\"A\" 0 1 (face org-table))) (#(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table))) hline (#(\"4\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| A | B |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n")
    (goto-char (point-min))
    (search-forward "1")
    (org-table-copy-down 1)
    (let ((after-copy
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "B")
      (org-table-move-column-left)
      (let ((after-col
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "2")
        (org-table-move-row-up)
        (list after-copy
              after-col
              (buffer-substring-no-properties (point-min) (point-max))
              (org-table-to-lisp))))))"#,
        expect,
    );
}
