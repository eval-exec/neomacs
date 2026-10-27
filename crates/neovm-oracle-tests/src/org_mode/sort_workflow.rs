use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_table_sort_region_time_numeric_function_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) nil (let ((fields (org-split-string (org-table-get-field) \"[ \t]*|[ \t]*\"))) (downcase (car fields)))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Task | Time | Score | Owner |\n")
    (insert "|------+-------+-------+-------|\n")
    (insert "| C | 11:30 | 8 | bob |\n")
    (insert "| A | 09:15 | 13 | ada |\n")
    (insert "| B | 10:00 | 5 | zoe |\n")
    (insert "|------+-------+-------+-------|\n")
    (insert "| Z | 12:00 | 1 | tail |\n")
    (goto-char (point-min))
    (search-forward "Time")
    (org-table-sort-lines nil ?t)
    (let ((by-time (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "Score")
      (org-table-sort-lines nil ?N)
      (let ((by-score-desc (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "Owner")
        (org-table-sort-lines
         t ?f
         (lambda ()
           (let ((fields (org-split-string (org-table-get-field) "[ \t]*|[ \t]*")))
             (downcase (car fields))))
         #'string<)
        (list by-time
              by-score-desc
              (buffer-substring-no-properties (point-min) (point-max))
              (org-table-to-lisp))))))"##,
        expect,
    );
}

#[test]
fn org_sort_entries_priority_clock_region_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable events)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO" "NEXT" "WAIT" "|" "DONE")))
          (org-priority-highest ?A)
          (org-priority-default ?B)
          (org-priority-lowest ?D)
          (org-priority-start-cycle-with-default nil)
          (events nil)
          (org-after-sorting-entries-or-items-hook
           (list (lambda ()
                   (push (mapcar (lambda (h) (org-link-display-format h))
                                 (org-map-entries
                                  (lambda () (org-get-heading t t t t))
                                  nil 'tree))
                         events)))))
      (org-mode)
      (insert "* Project\n")
      (insert "** WAIT 20 Gamma [#C]\n")
      (insert "DEADLINE: <2026-06-03 Wed>\n")
      (insert "[2026-05-24 Sun]\n")
      (insert ":LOGBOOK:\n")
      (insert "CLOCK: [2026-05-27 Wed 08:00]--[2026-05-27 Wed 10:30] =>  2:30\n")
      (insert ":END:\n")
      (insert "** TODO 10 Alpha [#A]\n")
      (insert "DEADLINE: <2026-06-01 Mon>\n")
      (insert "[2026-05-22 Fri]\n")
      (insert ":LOGBOOK:\n")
      (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 09:45] =>  0:45\n")
      (insert ":END:\n")
      (insert "** DONE 30 Beta [#D]\n")
      (insert "DEADLINE: <2026-06-02 Tue>\n")
      (insert "[2026-05-23 Sat]\n")
      (insert ":LOGBOOK:\n")
      (insert "CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 12:15] =>  1:15\n")
      (insert ":END:\n")
      (insert "** NEXT 40 Delta\n")
      (insert "DEADLINE: <2026-06-04 Thu>\n")
      (insert "[2026-05-21 Thu]\n")
      (insert ":LOGBOOK:\n")
      (insert "CLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 13:20] =>  0:20\n")
      (insert ":END:\n")
      (goto-char (point-min))
      (search-forward "Delta")
      (beginning-of-line)
      (let ((priority-cycle
             (list
              (progn (org-priority 'up) (org-get-heading t t t t))
              (progn (org-priority 'down) (org-get-heading t t t t))
              (progn (org-priority ?D) (org-get-heading t t t t))
              (org-get-priority (thing-at-point 'line t)))))
        (goto-char (point-min))
        (org-sort-entries nil ?o)
        (let ((by-todo (buffer-substring-no-properties (point-min) (point-max))))
          (goto-char (point-min))
          (org-sort-entries nil ?P)
          (let ((by-priority-desc
                 (buffer-substring-no-properties (point-min) (point-max))))
            (goto-char (point-min))
            (org-sort-entries nil ?k)
            (let ((by-clock (buffer-substring-no-properties (point-min) (point-max))))
              (goto-char (point-min))
              (org-sort-entries nil ?d)
              (let ((by-deadline
                     (buffer-substring-no-properties (point-min) (point-max))))
                (goto-char (point-min))
                (org-sort-entries nil ?C)
                (let ((by-created-desc
                       (buffer-substring-no-properties (point-min) (point-max))))
                  (goto-char (point-min))
                  (search-forward "** TODO")
                  (beginning-of-line)
                  (let ((region-start (point)))
                    (search-forward "** WAIT")
                    (beginning-of-line)
                    (org-end-of-subtree t t)
                    (let ((region-end (point)))
                      (goto-char region-start)
                      (let ((transient-mark-mode t)
                            (mark-active t))
                        (set-mark region-end)
                        (org-sort-entries nil ?n))
                      (list priority-cycle
                            by-todo
                            by-priority-desc
                            by-clock
                            by-deadline
                            by-created-desc
                            (buffer-substring-no-properties
                             (point-min) (point-max))
                            (nreverse events)
                             (org-element-map
                                 (org-element-parse-buffer) 'headline
                               (lambda (h)
                                 (list (org-element-property :todo-keyword h)
                                       (org-element-property :priority h)
                                       (org-element-property :raw-value h))))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_sort_entries_time_tag_property_alpha_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Charlie :work:\n")
    (insert "SCHEDULED: <2026-05-29 Thu>\n")
    (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
    (insert "* WAIT Alpha :home:\n")
    (insert "SCHEDULED: <2026-05-27 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 0:30\n:END:\n")
    (insert "* DONE Bravo :work:urgent:\n")
    (insert "SCHEDULED: <2026-05-28 Thu>\n")
    (insert ":PROPERTIES:\n:Effort: 1:00\n:END:\n")
    (insert "* TODO Delta :home:\n")
    (insert "SCHEDULED: <2026-05-26 Tue>\n")
    (insert ":PROPERTIES:\n:Effort: 1:30\n:END:\n")
    (let ((headlines (lambda ()
                       (org-element-map (org-element-parse-buffer) 'headline
                         (lambda (h)
                           (list (org-element-property :raw-value h)
                                 (substring-no-properties
                                  (or (org-element-property :todo-keyword h) ""))
                                 (org-element-property :tags h)))))))
      ;; Sort by time
      (goto-char (point-min))
      (org-sort-entries nil ?t)
      (let ((by-time (buffer-substring-no-properties (point-min) (point-max)))
            (by-time-headlines (funcall headlines)))
        ;; Sort by tag
        (goto-char (point-min))
        (org-sort-entries nil ?T)
        (let ((by-tag (buffer-substring-no-properties (point-min) (point-max)))
              (by-tag-headlines (funcall headlines)))
          ;; Sort by property
          (goto-char (point-min))
          (org-sort-entries nil ?r)
          (let ((by-prop (buffer-substring-no-properties (point-min) (point-max)))
                (by-prop-headlines (funcall headlines)))
            ;; Sort alphabetically
            (goto-char (point-min))
            (org-sort-entries nil ?a)
            (let ((by-alpha (buffer-substring-no-properties (point-min) (point-max)))
                  (by-alpha-headlines (funcall headlines)))
              (list by-time
                    by-time-headlines
                    by-tag
                    by-tag-headlines
                    by-prop
                    by-prop-headlines
                    by-alpha
                    by-alpha-headlines))))))))"##,
        expect,
    );
}

#[test]
fn org_sort_entries_edit_resort_by_alpha_time_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Charlie :work:\nSCHEDULED: <2026-05-30 Fri>\nBody C.\n\n")
    (insert "* Alpha :home:\nSCHEDULED: <2026-05-28 Wed>\nBody A.\n\n")
    (insert "* Echo :work:\nSCHEDULED: <2026-06-01 Sun>\nBody E.\n\n")
    (insert "* Bravo :home:\nSCHEDULED: <2026-05-29 Thu>\nBody B.\n\n")
    (insert "* Delta :work:\nSCHEDULED: <2026-05-31 Sat>\nBody D.\n\n")
    (let ((headlines (lambda ()
                       (mapcar
                        (lambda (h)
                          (list (org-element-property :raw-value h)
                                (org-element-property :tags h)))
                        (org-element-map (org-element-parse-buffer) 'headline
                          #'identity)))))
      (let ((initial (funcall headlines)))
        ;; Sort by alpha
        (goto-char (point-min))
        (org-sort-entries nil ?a)
        (let ((sorted-alpha (funcall headlines))
              (alpha-buf (buffer-substring-no-properties
                          (point-min) (point-max))))
          ;; Sort by time
          (goto-char (point-min))
          (org-sort-entries nil ?t)
          (let ((sorted-time (funcall headlines))
                (time-buf (buffer-substring-no-properties
                           (point-min) (point-max))))
            ;; Edit: change Charlie to Zulu
            (goto-char (point-min))
            (search-forward "Charlie")
            (replace-match "Zulu")
            ;; Re-sort alpha
            (goto-char (point-min))
            (org-sort-entries nil ?a)
            (let ((resorted (funcall headlines))
                  (resort-buf (buffer-substring-no-properties
                               (point-min) (point-max))))
              (list initial
                    sorted-alpha
                    sorted-time
                    resorted
                    alpha-buf
                    time-buf
                    resort-buf))))))))"##,
        expect,
    );
}
