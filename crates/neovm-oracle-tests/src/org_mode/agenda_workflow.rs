use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_agenda_custom_command_series_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil t t (t t t) \"2 days-agenda (W22):\\nWednesday  27 May 2026\\n 9:00...... Work:   Scheduled:  TODO Alpha                               :work:\\n12:00...... Work:   Scheduled:  TODO Home                                :home:\\nThursday   28 May 2026\\nWork:   Deadline:   WAIT Beta                                            :work:\\n\\n===============================================================================\\nWork tasks\\nWork:   TODO Alpha                                                       :work:\\n\\n===============================================================================\\nWaiting\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-custom" nil ".org"
                "#+CATEGORY: Work
* TODO Alpha :work:
SCHEDULED: <2026-05-27 Wed 09:00>
* WAIT Beta :work:
DEADLINE: <2026-05-28 Thu>
* TODO Home :home:
SCHEDULED: <2026-05-27 Wed 12:00>
"))
         (org-agenda-files (list file))
         (org-agenda-custom-commands
          '(("x" "Oracle combo"
             ((agenda "" ((org-agenda-span 2)
                          (org-agenda-start-day "2026-05-27")
                          (org-agenda-start-on-weekday nil)
                          (org-agenda-show-all-dates nil)
                          (org-agenda-use-time-grid nil)
                          (org-agenda-prefix-format "%?-12t%-8:c% s")))
              (tags-todo "+work" ((org-agenda-overriding-header "Work tasks")
                                  (org-agenda-prefix-format "%-8:c% s")))
              (todo "WAIT" ((org-agenda-overriding-header "Waiting")
                            (org-agenda-prefix-format "%-8:c% s")))))))
         (org-agenda-sorting-strategy
          '((agenda time-up priority-down category-keep)
            (tags todo-state-up priority-down)
            (todo todo-state-up priority-down))))
    (unwind-protect
        (progn
          (org-agenda nil "x")
          (with-current-buffer org-agenda-buffer-name
            (let ((text (buffer-substring-no-properties
                         (point-min) (point-max))))
              (list (not (null (string-match-p "Oracle combo" text)))
                    (not (null (string-match-p "Work tasks" text)))
                    (not (null (string-match-p "Waiting" text)))
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle text))))
                            '("TODO Alpha" "WAIT Beta" "TODO Home"))
                    text))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_modes_filter_mutate_source_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((t t nil) t (nil nil nil) t (nil t t t t) nil (t t nil) \"Day-agenda (W22):\\nWednesday  27 May 2026\\n 9:00...... Modes:   0:45 Scheduled: DONE [#A] Alpha     :work:billable:review:\\nModes:   2:00 Deadline:  WAIT Beta                                                                :work:internal:\\n| File                       | Headline         | Time   |\\n|----------------------------+------------------+--------|\\n|                            | ALL *Total time* | *1:45* |\\n|----------------------------+------------------+--------|\\n| org-agenda-modes<tmp>.org | *File time*      | *1:45* |\\n|                            | Alpha            | 0:30   |\\n|                            | WAIT Beta        | 1:15   |\\n\" \"#+CATEGORY: Modes\\n* DONE [#A] Alpha                                      :work:billable:review:\\nDEADLINE: <2026-05-29 Fri> SCHEDULED: <2026-05-28 Thu 10:15>\\n:PROPERTIES:\\n:Effort: 0:45\\n:END:\\nBody alpha line one.\\nBody alpha line two.\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 09:30] =>  0:30\\n* WAIT Beta :work:internal:\\nDEADLINE: <2026-05-27 Wed>\\n:PROPERTIES:\\n:Effort: 2:00\\n:END:\\nBody beta.\\nCLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 12:15] =>  1:15\\n* DONE Gamma :home:\\nCLOSED: [2026-05-27 Wed 17:00]\\n\")""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-clock)
  (let* ((file (make-temp-file
                "org-agenda-modes" nil ".org"
                "#+CATEGORY: Modes
* TODO Alpha :work:billable:
SCHEDULED: <2026-05-27 Wed 09:00>
:PROPERTIES:
:Effort: 0:45
:END:
Body alpha line one.
Body alpha line two.
CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 09:30] =>  0:30
* WAIT Beta :work:internal:
DEADLINE: <2026-05-27 Wed>
:PROPERTIES:
:Effort: 2:00
:END:
Body beta.
CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 12:15] =>  1:15
* DONE Gamma :home:
CLOSED: [2026-05-27 Wed 17:00]
"))
         (org-agenda-files (list file))
         (org-agenda-start-day "2026-05-27")
         (org-agenda-span 1)
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-entry-text-maxlines 2)
         (org-agenda-clockreport-parameter-plist
          '(:link nil :maxlevel 2 :fileskip0 t))
         (org-agenda-prefix-format "%?-12t%-8:c%5e %s"))
    (unwind-protect
        (progn
          (org-agenda-list nil "2026-05-27" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((initial (buffer-substring-no-properties
                            (point-min) (point-max))))
              (org-agenda-entry-text-mode)
              (let ((entry-text (buffer-substring-no-properties
                                 (point-min) (point-max)))
                    (entry-mode org-agenda-entry-text-mode))
                (org-agenda-clockreport-mode)
                (let ((clockreport (buffer-substring-no-properties
                                    (point-min) (point-max)))
                      (clock-mode org-agenda-clockreport-mode))
                  (org-agenda-filter-apply '("+work" "-internal") 'tag t)
                  (let ((filtered (buffer-substring-no-properties
                                   (point-min) (point-max)))
                        (tag-filter org-agenda-tag-filter))
                    (org-agenda-filter-remove-all)
                    (goto-char (point-min))
                    (search-forward "Alpha")
                    (beginning-of-line)
                    (let ((org-log-reschedule nil)
                          (org-log-redeadline nil)
                          (org-log-done nil))
                      (org-agenda-priority ?A)
                      (org-agenda-schedule nil "2026-05-28 10:15")
                      (org-agenda-deadline nil "2026-05-29")
                      (org-agenda-set-tags "review" 'on)
                      (org-agenda-todo "DONE"))
                    (let ((after-mutate
                           (buffer-substring-no-properties
                            (point-min) (point-max))))
                      (list (mapcar (lambda (needle)
                                      (not (null
                                            (string-match-p needle initial))))
                                    '("Alpha" "Beta" "Gamma"))
                            entry-mode
                            (mapcar (lambda (needle)
                                      (not (null
                                            (string-match-p needle entry-text))))
                                    '("Body alpha line one"
                                      "Body alpha line two"
                                      "Body beta"))
                            clock-mode
                            (mapcar (lambda (needle)
                                      (not (null
                                            (string-match-p needle
                                                            clockreport))))
                                    '("Clock summary" "Alpha" "Beta"
                                      "0:30" "1:15"))
                            tag-filter
                            (mapcar (lambda (needle)
                                      (not (null
                                            (string-match-p needle
                                                            filtered))))
                                    '("Alpha" "Beta" "Gamma"))
                            (replace-regexp-in-string
                             "org-agenda-modes[^ \n|]+\\.org"
                             "org-agenda-modes<tmp>.org"
                             after-mutate)
                            (with-current-buffer (find-file-noselect file)
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_skip_done_tags_represented_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((t nil nil nil) (\"Probe\") (\"billable\" \"work\") \"Headlines with TAGS match: +work\\nPress ‘C-u r’ to search again\\nProbe:   0:30 TODO Keep                                         :work:billable:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-skip" nil ".org"
                "#+CATEGORY: Probe
* TODO Keep :work:billable:
:PROPERTIES:
:Effort: 0:30
:END:
* DONE Skip :work:
CLOSED: [2026-05-26 Tue]
* WAIT Also keep :work:blocked:
:PROPERTIES:
:Effort: 1:15
:END:
* TODO Other :home:
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c%5e %s")
         (org-agenda-skip-function
          (lambda ()
            (org-agenda-skip-entry-if 'todo 'done))))
    (unwind-protect
        (progn
          (org-tags-view t "+work")
          (with-current-buffer org-agenda-buffer-name
            (let ((text (buffer-substring-no-properties
                         (point-min) (point-max))))
              (list (mapcar (lambda (needle)
                              (not (null (string-match-p needle text))))
                            '("TODO Keep" "WAIT Also keep" "DONE Skip" "TODO Other"))
                    (sort (org-agenda-get-represented-categories) #'string<)
                    (sort (org-agenda-get-represented-tags) #'string<)
                    text))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_log_mode_deadline_schedule_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((t t t t t) \"2 days\" \"2 days-agenda (W22):\\nWednesday  27 May 2026\\n 9:00...... Log:    DONE Finished                                        :work:\\n10:00...... Log:    Closed:     DONE Finished                            :work:\\n14:00-15:00 Log:    TODO Timed event\\nThursday   28 May 2026\\nLog:    Deadline:   TODO Due soon                                        :work:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-log" nil ".org"
                "#+CATEGORY: Log
* DONE Finished :work:
CLOSED: [2026-05-27 Wed 10:00]
SCHEDULED: <2026-05-27 Wed 09:00>
* TODO Due soon :work:
DEADLINE: <2026-05-28 Thu>
* TODO Timed event
<2026-05-27 Wed 14:00-15:00>
"))
         (org-agenda-files (list file))
         (org-agenda-start-day "2026-05-27")
         (org-agenda-span 2)
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-start-with-log-mode t)
         (org-agenda-log-mode-items '(closed clock state))
         (org-agenda-prefix-format "%?-12t%-8:c% s"))
    (unwind-protect
        (progn
          (org-agenda-list nil "2026-05-27" 2)
          (with-current-buffer org-agenda-buffer-name
            (let ((text (buffer-substring-no-properties
                         (point-min) (point-max))))
              (list (mapcar (lambda (needle)
                              (not (null (string-match-p needle text))))
                            '("Finished" "Closed" "Due soon" "Timed event"
                              "14:00-15:00"))
                    (org-agenda-span-name org-agenda-current-span)
                    text))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_filter_apply_remove_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-filter" nil ".org"
                "#+CATEGORY: Filter
* TODO Alpha :work:billable:
:PROPERTIES:
:Effort: 0:30
:END:
* TODO Beta :work:internal:
:PROPERTIES:
:Effort: 2:00
:END:
* TODO Home :home:
:PROPERTIES:
:Effort: 0:15
:END:
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c%5e %s")
         (org-agenda-show-all-dates nil))
    (unwind-protect
        (progn
          (org-tags-view t "+TODO")
          (with-current-buffer org-agenda-buffer-name
            (let ((all (buffer-substring-no-properties
                        (point-min) (point-max))))
              (org-agenda-filter-apply '("+work") 'tag)
              (let ((work (buffer-substring-no-properties
                           (point-min) (point-max)))
                    (filter-tag org-agenda-tag-filter))
                (org-agenda-filter-apply '("<1:00") 'effort)
                (let ((effort (buffer-substring-no-properties
                               (point-min) (point-max)))
                      (filter-effort org-agenda-effort-filter))
                  (org-agenda-filter-remove-all)
                  (list (mapcar (lambda (needle)
                                  (not (null (string-match-p needle all))))
                                '("Alpha" "Beta" "Home"))
                        (mapcar (lambda (needle)
                                  (not (null (string-match-p needle work))))
                                '("Alpha" "Beta" "Home"))
                        (mapcar (lambda (needle)
                                  (not (null (string-match-p needle effort))))
                                '("Alpha" "Beta" "Home"))
                        filter-tag
                        filter-effort
                        org-agenda-tag-filter
                        org-agenda-effort-filter)))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_priority_effort_source_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"Global list of TODO items of type: ALL\\nPress ‘N r’ (e.g. ‘0 r’) to search again: (0)[ALL] (1)DONE (2)TODO\\nEdit:    0:30 TODO [#A] Alpha\\nEdit:    1:00 TODO Beta\\n\" \"Global list of TODO items of type: ALL\\nPress ‘N r’ (e.g. ‘0 r’) to search again: (0)[ALL] (1)DONE (2)TODO\\nEdit:    0:30 TODO [#A] Alpha\\nEdit:    1:00 TODO [#A] Beta\\n\" \"#+CATEGORY: Edit\\n* TODO [#A] Alpha\\n:PROPERTIES:\\n:Effort:   2:30\\n:END:\\n* TODO [#A] Beta\\n:PROPERTIES:\\n:Effort: 1:00\\n:END:\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-edit" nil ".org"
                "#+CATEGORY: Edit
* TODO Alpha
:PROPERTIES:
:Effort: 0:30
:END:
* TODO Beta
:PROPERTIES:
:Effort: 1:00
:END:
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c%5e %s")
         (org-priority-enable-commands t))
    (unwind-protect
        (progn
          (org-todo-list)
          (with-current-buffer org-agenda-buffer-name
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (org-agenda-priority ?A)
            (cl-letf (((symbol-function 'completing-read)
                       (lambda (&rest _) "2:30")))
              (org-agenda-set-effort))
            (let ((agenda-after-alpha
                   (buffer-substring-no-properties
                    (point-min) (point-max))))
              (search-forward "Beta")
              (beginning-of-line)
              (org-agenda-priority 'down)
              (list agenda-after-alpha
                    (buffer-substring-no-properties
                     (point-min) (point-max))
                    (with-current-buffer (find-file-noselect file)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_bulk_mark_toggle_regexp_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((2 nil (\"Beta\" \"Alpha\")) (0 nil) 2 \"Global list of TODO items of type: ALL\\nPress ‘N r’ (e.g. ‘0 r’) to search again: (0)[ALL] (1)DONE (2)TODO\\nBulk:   TODO Alpha                                                       :work:\\nBulk:   TODO Beta                                                        :home:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-bulk" nil ".org"
                "#+CATEGORY: Bulk
* TODO Alpha :work:
* TODO Beta :home:
* WAIT Gamma :work:
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c% s"))
    (unwind-protect
        (progn
          (org-todo-list)
          (with-current-buffer org-agenda-buffer-name
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (org-agenda-bulk-mark 2)
            (let ((after-two
                   (list (length org-agenda-bulk-marked-entries)
                         (org-agenda-bulk-marked-p)
                         (mapcar (lambda (m)
                                   (with-current-buffer (marker-buffer m)
                                     (save-excursion
                                       (goto-char m)
                                       (org-get-heading t t t t))))
                                 org-agenda-bulk-marked-entries))))
              (org-agenda-bulk-unmark-all)
              (org-agenda-bulk-mark-regexp "Gamma")
              (let ((after-regexp
                     (list (length org-agenda-bulk-marked-entries)
                           (mapcar (lambda (m)
                                     (with-current-buffer (marker-buffer m)
                                       (save-excursion
                                         (goto-char m)
                                         (org-get-heading t t t t))))
                                   org-agenda-bulk-marked-entries))))
                (org-agenda-bulk-toggle-all)
                (list after-two
                      after-regexp
                      (length org-agenda-bulk-marked-entries)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_agenda_filter_matcher_visibility_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-filter-matrix" nil ".org"
                "#+CATEGORY: Matrix
* TODO Alpha :work:billable:
:PROPERTIES:
:Effort: 0:30
:Owner: Ada
:END:
* WAIT Beta :work:internal:
:PROPERTIES:
:Effort: 2:00
:Owner: Bea
:END:
* TODO Home :home:
:PROPERTIES:
:Effort: 0:15
:Owner: Cy
:END:
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c%5e %s")
         (org-agenda-show-all-dates nil)
         (org-agenda-hide-tags-regexp nil))
    (unwind-protect
        (progn
          (org-tags-view t "+TODO")
          (with-current-buffer org-agenda-buffer-name
            (let ((all (buffer-substring-no-properties
                        (point-min) (point-max)))
                  (tag-matcher
                   (org-agenda-filter-make-matcher-tag-exp
                    '("+work" "-internal") 'and))
                  (effort-form
                   (org-agenda-filter-effort-form "<1:00")))
              (org-agenda-filter-by-regexp nil)
              (let ((after-regexp-filter org-agenda-regexp-filter))
                (org-agenda-filter-apply '("+work" "-internal") 'tag t)
                (let ((work-billable
                       (buffer-substring-no-properties
                        (point-min) (point-max)))
                      (tag-filter org-agenda-tag-filter))
                  (org-agenda-filter-apply '("<1:00") 'effort)
                  (let ((effort-filtered
                         (buffer-substring-no-properties
                          (point-min) (point-max)))
                        (effort-filter org-agenda-effort-filter)
                        (line-states nil))
                    (goto-char (point-min))
                    (while (re-search-forward "^[ \t]*Matrix" nil t)
                      (push (list
                             (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position))
                             (get-text-property (line-beginning-position)
                                                'invisible))
                            line-states))
                    (org-agenda-filter-remove-all)
                    (list (mapcar (lambda (needle)
                                    (not (null (string-match-p needle all))))
                                  '("Alpha" "Beta" "Home"))
                          tag-matcher
                          effort-form
                          after-regexp-filter
                          tag-filter
                          effort-filter
                          (mapcar (lambda (needle)
                                    (not (null (string-match-p
                                                needle work-billable))))
                                  '("Alpha" "Beta" "Home"))
                          (mapcar (lambda (needle)
                                    (not (null (string-match-p
                                                needle effort-filtered))))
                                  '("Alpha" "Beta" "Home"))
                          (nreverse line-states)
                          org-agenda-tag-filter
                          org-agenda-effort-filter))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_agenda_clockreport_archives_mode_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-clock)
  (let* ((file (make-temp-file
                "org-agenda-clockreport" nil ".org"
                "#+CATEGORY: Report
* TODO Alpha :work:
SCHEDULED: <2026-05-27 Wed>
CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:00] =>  1:00
* TODO Beta :ARCHIVE:
SCHEDULED: <2026-05-27 Wed>
CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 12:30] =>  1:30
* TODO Gamma :work:
SCHEDULED: <2026-05-28 Thu>
CLOCK: [2026-05-28 Thu 08:00]--[2026-05-28 Thu 08:45] =>  0:45
"))
         (org-agenda-files (list file))
         (org-agenda-start-day "2026-05-27")
         (org-agenda-span 2)
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-clockreport-parameter-plist
          '(:link nil :maxlevel 3 :fileskip0 t))
         (org-agenda-prefix-format "%-8:c%?-12t% s"))
    (unwind-protect
        (progn
          (org-agenda-list nil "2026-05-27" 2)
          (with-current-buffer org-agenda-buffer-name
            (let ((initial (buffer-substring-no-properties
                            (point-min) (point-max))))
              (org-agenda-clockreport-mode)
              (let ((clockreport (buffer-substring-no-properties
                                  (point-min) (point-max)))
                    (clock-mode org-agenda-clockreport-mode))
                (org-agenda-archives-mode)
                (let ((archives (buffer-substring-no-properties
                                 (point-min) (point-max)))
                      (archive-mode org-agenda-archives-mode))
                  (list (mapcar (lambda (needle)
                                  (not (null (string-match-p needle initial))))
                                '("Alpha" "Beta" "Gamma"))
                        (mapcar (lambda (needle)
                                  (not (null (string-match-p needle clockreport))))
                                '("Clock summary" "Alpha" "Gamma" "1:00" "0:45"))
                        clock-mode
                        (mapcar (lambda (needle)
                                  (not (null (string-match-p needle archives))))
                                '("Alpha" "Beta" "Gamma" "1:30"))
                        archive-mode
                        clockreport
                        archives)))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_entry_text_switch_context_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil (nil nil nil nil) t (\"Alpha\" \"* TODO Alpha :work:\") nil \"Global list of TODO items of type: ALL\\nPress ‘N r’ (e.g. ‘0 r’) to search again: (0)[ALL] (1)DONE (2)TODO\\nText:   TODO Alpha                                                       :work:\\nText:   TODO Beta                                                        :home:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-entry-text" nil ".org"
                "#+CATEGORY: Text
* TODO Alpha :work:
First line.
Second line.
Third line.
* TODO Beta :home:
Beta body.
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c% s")
         (org-agenda-entry-text-maxlines 2))
    (unwind-protect
        (progn
          (org-todo-list)
          (with-current-buffer org-agenda-buffer-name
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (let ((before (buffer-substring-no-properties
                           (point-min) (point-max))))
              (org-agenda-entry-text-mode 2)
              (let ((with-text (buffer-substring-no-properties
                                (point-min) (point-max)))
                    (mode-on org-agenda-entry-text-mode))
                (org-agenda-switch-to)
                (let ((source (with-current-buffer (find-file-noselect file)
                                (list (org-get-heading t t t t)
                                      (buffer-substring-no-properties
                                       (line-beginning-position)
                                       (line-end-position))))))
                  (with-current-buffer org-agenda-buffer-name
                    (org-agenda-entry-text-mode)
                    (list (not (null (string-match-p "First line" before)))
                          (mapcar (lambda (needle)
                                    (not (null
                                          (string-match-p needle with-text))))
                                  '("First line" "Second line" "Third line"
                                    "Beta body"))
                          mode-on
                          source
                          org-agenda-entry-text-mode
                          (buffer-substring-no-properties
                           (point-min) (point-max)))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_archive_sibling_source_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"Finished\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-archive)
  (let* ((file (make-temp-file
                "org-agenda-archive-sibling" nil ".org"
                "#+CATEGORY: Archive
* TODO Keep
* DONE Finished
:PROPERTIES:
:Effort: 0:30
:END:
Body.
* TODO Later
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%-8:c% s")
         (org-archive-location "::* Archive"))
    (unwind-protect
        (progn
          (org-todo-list)
          (with-current-buffer org-agenda-buffer-name
            (goto-char (point-min))
            (search-forward "Finished")
            (beginning-of-line)
            (org-agenda-archive-to-archive-sibling)
            (let ((agenda-after (buffer-substring-no-properties
                                 (point-min) (point-max))))
              (with-current-buffer (find-file-noselect file)
                (let ((text (buffer-substring-no-properties
                             (point-min) (point-max))))
                  (list (mapcar (lambda (needle)
                                  (not (null
                                        (string-match-p needle agenda-after))))
                                '("Keep" "Finished" "Later"))
                        (mapcar (lambda (needle)
                                  (not (null (string-match-p needle text))))
                                '("* Archive" "** DONE Finished" ":Effort:"
                                  "Body." "* TODO Later"))
                        agenda-after
                        text))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_day_entries_properties_timestamp_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable org-agenda-show-log-scoped)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-day-entries" nil ".org"
                "#+CATEGORY: Day
* TODO Scheduled [#A] :work:
SCHEDULED: <2026-05-27 Wed 09:00>
:PROPERTIES:
:Effort: 0:45
:END:
* TODO Deadline [#C] :work:
DEADLINE: <2026-05-27 Wed -1d>
:PROPERTIES:
:Effort: 1:30
:END:
* DONE Closed :done:
CLOSED: [2026-05-27 Wed 17:00]
* Event heading :event:
<2026-05-27 Wed 14:00-15:30>
* Repeating deadline :repeat:
DEADLINE: <2026-05-20 Wed +1w>
"))
         (org-agenda-files (list file))
         (org-agenda-prefix-format "%?-12t%-8:c%5e % s")
         (org-agenda-show-inherited-tags t)
         (org-agenda-use-tag-inheritance t)
         (org-agenda-sorting-strategy-selected
          '(time-up priority-down deadline-up scheduled-up)))
    (unwind-protect
        (let* ((date '(5 27 2026))
               (summary
                (lambda (items)
                  (mapcar
                   (lambda (item)
                     (let* ((marker (or (get-text-property 0 'org-hd-marker item)
                                        (get-text-property 0 'org-marker item)))
                            (heading
                             (and (markerp marker)
                                  (marker-buffer marker)
                                  (with-current-buffer (marker-buffer marker)
                                    (save-excursion
                                      (goto-char marker)
                                      (org-get-heading t t t t))))))
                       (list (substring-no-properties item)
                             (get-text-property 0 'type item)
                             (get-text-property 0 'todo-state item)
                             (get-text-property 0 'priority item)
                             (get-text-property 0 'effort item)
                             (get-text-property 0 'effort-minutes item)
                             (get-text-property 0 'org-category item)
                             (get-text-property 0 'ts-date item)
                             heading)))
                   items)))
               (all (org-agenda-get-day-entries
                     file date
                     :deadline :scheduled :timestamp :closed))
               (deadline-only (org-agenda-get-day-entries
                               file date :deadline*))
               (scheduled-only (org-agenda-get-day-entries
                                file date :scheduled*))
               timestamp-sorts)
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (dolist (strategy '((deadline-up)
                                (scheduled-up)
                                (ts-up)
                                (timestamp-up)))
              (let ((org-agenda-sorting-strategy-selected strategy))
                (goto-char (point-min))
                (search-forward "Scheduled")
                (beginning-of-line)
                (push (list strategy
                            (org-agenda-entry-get-agenda-timestamp
                             (point)))
                      timestamp-sorts))))
          (list (funcall summary all)
                (funcall summary deadline-only)
                (funcall summary scheduled-only)
                (nreverse timestamp-sorts)))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_date_shift_redo_marker_source_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((t t t nil t nil) ((\"Shift:        TODO Window                                           :work:ship:\" \"timestamp\" \"TODO\" nil nil nil \"Window\")) nil (t t t) \" 9:00...... Shift:        Scheduled: TODO Window                    :work:ship:\" \"13:00-14:00 Shift:        TODO Range                                :work:call:\" (t t t t) (t t t t nil) ((\"Shift:        TODO Window                                           :work:ship:\" \"timestamp\" \"TODO\" nil nil nil \"Window\")) \"#+CATEGORY: Shift\\n* TODO Window :work:ship:\\nSCHEDULED: <2026-05-29 Fri 09:00>\\nDEADLINE: <2026-05-29 Fri>\\n:PROPERTIES:\\n:Effort: 1:00\\n:END:\\n* TODO Range :work:call:\\n<2026-05-27 Wed 15:00-16:00>\\n* WAIT Future :home:\\nSCHEDULED: <2026-05-28 Thu 08:30>\\n\")""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file
                "org-agenda-date-shift" nil ".org"
                "#+CATEGORY: Shift
* TODO Window :work:ship:
SCHEDULED: <2026-05-27 Wed 09:00>
DEADLINE: <2026-05-29 Fri>
:PROPERTIES:
:Effort: 1:00
:END:
* TODO Range :work:call:
<2026-05-27 Wed 13:00-14:00>
* WAIT Future :home:
SCHEDULED: <2026-05-28 Thu 08:30>
"))
         (org-agenda-files (list file))
         (org-agenda-start-day "2026-05-27")
         (org-agenda-span 3)
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-prefix-format "%?-12t%-8:c%5e %s")
         (org-timestamp-rounding-minutes '(0 15))
         (org-log-reschedule nil)
         (org-log-redeadline nil))
    (unwind-protect
        (progn
          (org-agenda-list nil "2026-05-27" 3)
          (with-current-buffer org-agenda-buffer-name
            (let ((line-summary
                   (lambda ()
                     (let (rows)
                       (save-excursion
                         (goto-char (point-min))
                         (while (re-search-forward
                                 "^[ \t]*Shift:.*\\(Window\\|Range\\|Future\\)"
                                 nil t)
                           (let* ((pos (line-beginning-position))
                                  (marker (or (get-text-property pos
                                                                 'org-hd-marker)
                                              (get-text-property pos
                                                                 'org-marker)))
                                  (heading
                                   (and (markerp marker)
                                        (marker-buffer marker)
                                        (with-current-buffer
                                            (marker-buffer marker)
                                          (save-excursion
                                            (goto-char marker)
                                            (org-get-heading t t t t))))))
                             (push (list
                                    (buffer-substring-no-properties
                                     pos (line-end-position))
                                    (get-text-property pos 'type)
                                    (get-text-property pos 'todo-state)
                                    (get-text-property pos 'time-of-day)
                                    (get-text-property pos 'duration)
                                    (get-text-property pos 'effort-minutes)
                                    heading)
                                   rows))))
                       (nreverse rows)))))
              (let ((initial (buffer-substring-no-properties
                              (point-min) (point-max)))
                    (initial-summary (funcall line-summary)))
                (org-agenda-filter-apply '("+work") 'tag t)
                (let ((filtered (buffer-substring-no-properties
                                 (point-min) (point-max)))
                      (tag-filter org-agenda-tag-filter))
                  (org-agenda-filter-remove-all)
                  (goto-char (point-min))
                  (search-forward "Window")
                  (beginning-of-line)
                  (org-agenda-date-later 2)
                  (let ((after-window-display
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))))
                    (goto-char (point-min))
                    (search-forward "Range")
                    (beginning-of-line)
                    (org-agenda-date-later-hours 2)
                    (let ((after-range-display
                           (buffer-substring-no-properties
                            (line-beginning-position) (line-end-position)))
                          (source-after-edits
                           (with-current-buffer (find-file-noselect file)
                             (buffer-substring-no-properties
                              (point-min) (point-max)))))
                      (org-agenda-redo)
                      (let ((after-redo
                             (buffer-substring-no-properties
                              (point-min) (point-max)))
                            (after-redo-summary (funcall line-summary)))
                        (list
                         (mapcar (lambda (needle)
                                   (not (null
                                         (string-match-p needle initial))))
                                 '("Window" "Range" "Future"
                                   "09:00" "13:00-14:00" "08:30"))
                         initial-summary
                         tag-filter
                         (mapcar (lambda (needle)
                                   (not (null
                                         (string-match-p needle filtered))))
                                 '("Window" "Range" "Future"))
                         after-window-display
                         after-range-display
                         (mapcar (lambda (needle)
                                   (not (null
                                         (string-match-p needle
                                                         source-after-edits))))
                                 '("SCHEDULED: <2026-05-29 Fri 09:00>"
                                   "<2026-05-27 Wed 15:00-16:00>"
                                   "DEADLINE: <2026-05-29 Fri>"
                                   "SCHEDULED: <2026-05-28 Thu 08:30>"))
                         (mapcar (lambda (needle)
                                   (not (null
                                         (string-match-p needle after-redo))))
                                 '("Window" "Range" "Future"
                                   "15:00-16:00" "08:30"))
                          after-redo-summary
                          source-after-edits)))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_agenda_clockreport_mode_habit_consistency_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil nil nil \"Week-agenda (W22):\\nMonday     25 May 2026 W22\\nTuesday    26 May 2026\\nWednesday  27 May 2026\\n  Work:       Scheduled:  TODO Review code                               :work:\\nThursday   28 May 2026\\nFriday     29 May 2026\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-habit)
  (require 'org-clock)
  (let* ((root (make-temp-file "org-agenda-cr" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'week)
         (org-agenda-start-day "2026-05-25")
         (org-agenda-start-on-weekday 1)
         (org-agenda-clockreport-mode t)
         (org-agenda-show-log t)
         (org-agenda-log-mode-items '(closed clock state))
         (org-habit-show-habits t)
         (org-habit-show-all-today t)
         (org-habit-following-days 7)
         (org-habit-preceding-days 14)
         (org-agenda-use-time-grid nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+CATEGORY: Work\n")
            (insert "* TODO Write report :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed .+2d/4d>\n")
            (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 2:00\n:END:\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-25 Sun 10:00]--[2026-05-25 Sun 11:30] =>  1:30\n")
            (insert "CLOCK: [2026-05-26 Mon 09:00]--[2026-05-26 Mon 10:00] =>  1:00\n")
            (insert ":END:\n")
            (insert "* TODO Review code :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-26 Mon 14:00]--[2026-05-26 Mon 15:30] =>  1:30\n")
            (insert ":END:\n")
            (insert "* DONE Deploy :ops:\n")
            (insert "CLOSED: [2026-05-26 Mon 16:00]\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-26 Mon 15:30]--[2026-05-26 Mon 16:00] =>  0:30\n")
            (insert ":END:\n"))
          (org-agenda-list nil "2026-05-25" 7)
          (with-current-buffer org-agenda-buffer-name
            (let ((agenda-text
                   (buffer-substring-no-properties (point-min) (point-max)))
                  (has-habit nil)
                  (has-clockreport nil)
                  (has-clocked nil))
              (goto-char (point-min))
              (setq has-habit
                    (not (null (re-search-forward "habit" nil t))))
              (goto-char (point-min))
              (setq has-clockreport
                    (not (null (re-search-forward "Clock report" nil t))))
              (goto-char (point-min))
              (setq has-clocked
                    (not (null (re-search-forward "Clocked" nil t))))
              (list has-habit
                    has-clockreport
                    has-clocked
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>"
                     agenda-text)))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_log_mode_clock_state_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable agenda-text)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-log" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'day)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-log t)
         (org-agenda-log-mode-items '(closed clock state))
         (org-agenda-use-time-grid nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+CATEGORY: Work\n")
            (insert "* TODO Write report\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\n")
            (insert "CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\n")
            (insert ":END:\n")
            (insert "* DONE Deploy\n")
            (insert "CLOSED: [2026-05-27 Wed 12:00]\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-27 Wed 12:00]--[2026-05-27 Wed 12:30] =>  0:30\n")
            (insert "- State \"DONE\"  from \"TODO\"  [2026-05-27 Wed 12:00]\n")
            (insert ":END:\n")
            (insert "* Review code\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-27 Wed 14:00]--[2026-05-27 Wed 15:00] =>  1:00\n")
            (insert ":END:\n"))
          (org-agenda-list nil "2026-05-27" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((agenda-text
                   (buffer-substring-no-properties (point-min) (point-max)))
                  ;; Count specific patterns in agenda
                  (clocked-count
                   (let ((c 0) (s 0))
                     (while (string-match "Clocked" agenda-text s)
                       (setq s (match-end 0) c (1+ c)))
                     c))
                  (closed-count
                   (let ((c 0) (s 0))
                     (while (string-match "Closed" agenda-text s)
                       (setq s (match-end 0) c (1+ c)))
                     c))
                  (state-count
                   (let ((c 0) (s 0))
                     (while (string-match "State" agenda-text s)
                       (setq s (match-end 0) c (1+ c)))
                     c))
                  ;; Extract time entries
                  (time-entries
                   (let ((entries nil) (s 0))
                     (while (string-match
                             "\\([0-9]+:[0-9]+\\)\\s-+.*Clocked" agenda-text s)
                       (push (match-string 1 agenda-text) entries)
                       (setq s (match-end 0)))
                     (nreverse entries))))
              (list clocked-count
                    closed-count
                    state-count
                    time-entries
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>" agenda-text)))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_filter_tag_todo_match_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nWednesday  27 May 2026\\n  tasks:      Scheduled:  TODO Write report                       :work:urgent:\\n  tasks:      Scheduled:  TODO Buy groceries                             :home:\\n  tasks:      Scheduled:  WAIT Fix bug                                   :work:\\n\" \"Day-agenda (W22):\\nWednesday  27 May 2026\\n  tasks:      Scheduled:  TODO Write report                       :work:urgent:\\n  tasks:      Scheduled:  TODO Buy groceries                             :home:\\n  tasks:      Scheduled:  WAIT Fix bug                                   :work:\\n\" \"Day-agenda (W22):\\nWednesday  27 May 2026\\n  tasks:      Scheduled:  TODO Write report                       :work:urgent:\\n  tasks:      Scheduled:  TODO Buy groceries                                                                         :home:\\n  tasks:      Scheduled:  WAIT Fix bug                                   :work:\\n\" \"Day-agenda (W22):\\nWednesday  27 May 2026\\n  tasks:      Scheduled:  TODO Write report                       :work:urgent:\\n  tasks:      Scheduled:  TODO Buy groceries                                                                         :home:\\n  tasks:      Scheduled:  WAIT Fix bug                                   :work:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-filter" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'day)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Write report :work:urgent:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert "* DONE Review code :work:\n")
            (insert "CLOSED: [2026-05-27 Wed]\n")
            (insert "* TODO Buy groceries :home:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert "* WAIT Fix bug :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert "* DONE Deploy :ops:\n")
            (insert "CLOSED: [2026-05-27 Wed]\n"))
          (org-agenda-list nil "2026-05-27" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((full-text (buffer-substring-no-properties
                              (point-min) (point-max))))
              (org-agenda-filter-apply '("+work") 'tag)
              (let ((work-text (buffer-substring-no-properties
                                (point-min) (point-max))))
                (org-agenda-filter-remove-all)
                (org-agenda-filter-apply '("+TODO") 'todo)
                (let ((todo-text (buffer-substring-no-properties
                                  (point-min) (point-max))))
                  (org-agenda-filter-remove-all)
                  (org-agenda-filter-apply '("+DONE") 'todo)
                  (let ((done-text (buffer-substring-no-properties
                                    (point-min) (point-max))))
                    (list (replace-regexp-in-string
                           (regexp-quote root) "<root>" full-text)
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>" work-text)
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>" todo-text)
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>" done-text))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_date_shift_redo_source_mutation_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Week-agenda (W22):\\nMonday     25 May 2026 W22\\nTuesday    26 May 2026\\nWednesday  27 May 2026\\n  tasks:      Scheduled:  TODO Alpha\\nThursday   28 May 2026\\n  tasks:      Scheduled:  TODO Beta\\nFriday     29 May 2026\\n  tasks:      Deadline:   TODO Gamma\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\" \"Week-agenda (W22):\\nMonday     25 May 2026 W22\\nTuesday    26 May 2026\\nWednesday  27 May 2026\\n  tasks:      Scheduled:  TODO Alpha                      \\nThursday   28 May 2026\\n  tasks:      Scheduled:  TODO Beta\\nFriday     29 May 2026\\n  tasks:      Deadline:   TODO Gamma\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\" \"Week-agenda (W22):\\nMonday     25 May 2026 W22\\nTuesday    26 May 2026\\nWednesday  27 May 2026\\nThursday   28 May 2026\\n  tasks:      Scheduled:  TODO Beta\\nFriday     29 May 2026\\n  tasks:      Deadline:   TODO Gamma\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\" \"* TODO Alpha\\nSCHEDULED: <2026-06-15 Mon>\\n* TODO Beta\\nSCHEDULED: <2026-05-28 Thu>\\n* TODO Gamma\\nDEADLINE: <2026-05-29 Fri>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-shift" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'week)
         (org-agenda-start-day "2026-05-25")
         (org-agenda-start-on-weekday 1)
         (org-agenda-use-time-grid nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert "* TODO Beta\n")
            (insert "SCHEDULED: <2026-05-28 Thu>\n")
            (insert "* TODO Gamma\n")
            (insert "DEADLINE: <2026-05-29 Fri>\n"))
          (org-agenda-list nil "2026-05-25" 7)
          (with-current-buffer org-agenda-buffer-name
            (let ((initial (replace-regexp-in-string
                            (regexp-quote root) "<root>"
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-agenda-date-later 1)
              (let ((after-shift (replace-regexp-in-string
                                  (regexp-quote root) "<root>"
                                  (buffer-substring-no-properties
                                   (point-min) (point-max)))))
                (org-agenda-redo)
                (let ((after-redo (replace-regexp-in-string
                                   (regexp-quote root) "<root>"
                                   (buffer-substring-no-properties
                                    (point-min) (point-max)))))
                  (let ((source-content
                         (with-current-buffer (find-file-noselect file)
                           (prog1 (buffer-substring-no-properties
                                   (point-min) (point-max))
                             (kill-buffer)))))
                    (list initial
                          after-shift
                          after-redo
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           source-content))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_bulk_mark_tag_filter_effort_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-bulk" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'day)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
            (insert "* TODO Beta :home:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 0:30\n:END:\n")
            (insert "* TODO Gamma :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 1:00\n:END:\n"))
          (org-agenda-list nil "2026-05-27" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((initial (replace-regexp-in-string
                            (regexp-quote root) "<root>"
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))
              ;; Tag filter +work
              (org-agenda-filter-apply '("+work") 'tag)
              (let ((work-filter (replace-regexp-in-string
                                  (regexp-quote root) "<root>"
                                  (buffer-substring-no-properties
                                   (point-min) (point-max)))))
                ;; Clear and apply effort filter
                (org-agenda-filter-remove-all)
                (org-agenda-filter-apply '("1:00") 'effort)
                (let ((effort-filter (replace-regexp-in-string
                                      (regexp-quote root) "<root>"
                                      (buffer-substring-no-properties
                                       (point-min) (point-max)))))
                  (list initial
                        work-filter
                        effort-filter)))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_entry_text_switch_context_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument markerp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-entry" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'day)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
            (insert "Alpha body paragraph.\n")
            (insert "* WAIT Beta :home:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 0:30\n:END:\n")
            (insert "Beta body paragraph.\n"))
          (org-agenda-list nil "2026-05-27" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((agenda-text
                   (replace-regexp-in-string
                    (regexp-quote root) "<root>"
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (let ((entry-text
                     (org-agenda-get-some-entry-text
                      (point) 100)))
                (let ((cat (org-entry-get (point) "CATEGORY"))
                      (effort (org-entry-get (point) "Effort")))
                  (list agenda-text entry-text cat effort)))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_day_entries_properties_timestamp_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-day" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'week)
         (org-agenda-start-day "2026-05-25")
         (org-agenda-start-on-weekday 1)
         (org-agenda-use-time-grid nil))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Morning :work:\n")
            (insert "SCHEDULED: <2026-05-26 Mon 09:00-10:00>\n")
            (insert ":PROPERTIES:\n:Effort: 1:00\n:END:\n")
            (insert "* TODO Afternoon :home:\n")
            (insert "SCHEDULED: <2026-05-27 Wed 14:00-15:30>\n")
            (insert ":PROPERTIES:\n:Effort: 1:30\n:END:\n")
            (insert "* DONE Completed\n")
            (insert "CLOSED: [2026-05-26 Mon 16:00]\n")
            (insert "* WAIT Pending\n")
            (insert "DEADLINE: <2026-05-28 Thu>\n")
            (insert "* TODO Weekend\n")
            (insert "SCHEDULED: <2026-05-31 Sat>\n"))
          (org-agenda-list nil "2026-05-25" 7)
          (with-current-buffer org-agenda-buffer-name
            (let ((agenda-text
                   (replace-regexp-in-string
                    (regexp-quote root) "<root>"
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))
              (let ((mon-count (let ((c 0) (s 0))
                                 (while (string-match "Monday" agenda-text s)
                                   (setq s (match-end 0) c (1+ c)))
                                 c))
                    (wed-count (let ((c 0) (s 0))
                                 (while (string-match "Wednesday" agenda-text s)
                                   (setq s (match-end 0) c (1+ c)))
                                 c)))
                (let ((has-0900 (string-match-p "09:00" agenda-text))
                      (has-1400 (string-match-p "14:00" agenda-text)))
                  (list agenda-text mon-count wed-count has-0900 has-1400)))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_clockreport_filter_effort_todo_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 60 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-cr" t))
         (file (expand-file-name "tasks.org" root))
         (org-agenda-files (list file))
         (org-agenda-span 'day)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil)
         (org-agenda-clockreport-mode t)
         (org-agenda-clock-reporting-file file))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Write report :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\n")
            (insert "CLOCK: [2026-05-27 Wed 14:00]--[2026-05-27 Wed 15:00] =>  1:00\n")
            (insert ":END:\n")
            (insert "* TODO Review code :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:Effort: 1:00\n:END:\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 12:00] =>  1:00\n")
            (insert ":END:\n")
            (insert "* DONE Deploy :ops:\n")
            (insert "CLOSED: [2026-05-27 Wed]\n")
            (insert ":PROPERTIES:\n:Effort: 0:30\n:END:\n")
            (insert ":LOGBOOK:\n")
            (insert "CLOCK: [2026-05-27 Wed 16:00]--[2026-05-27 Wed 16:30] =>  0:30\n")
            (insert ":END:\n"))
          (org-agenda-list nil "2026-05-27" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((full-text (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
              (org-agenda-filter-apply '("+work") 'tag)
              (let ((work-text (replace-regexp-in-string
                                (regexp-quote root) "<root>"
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                (org-agenda-filter-remove-all)
                (org-agenda-filter-apply '("1:00") 'effort)
                (let ((effort-text (replace-regexp-in-string
                                    (regexp-quote root) "<root>"
                                    (buffer-substring-no-properties
                                     (point-min) (point-max)))))
                  (org-agenda-filter-remove-all)
                  (org-agenda-filter-apply '("+TODO") 'todo)
                  (let ((todo-text (replace-regexp-in-string
                                    (regexp-quote root) "<root>"
                                    (buffer-substring-no-properties
                                     (point-min) (point-max)))))
                    (list full-text work-text effort-text todo-text))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_list_edit_todo_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Alpha\\n  a:          Scheduled:  DONE Beta\\n  a:          Scheduled:  TODO Gamma                                     :work:\\n  a:          Scheduled:  NEXT Delta\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE Alpha\\n  a:          Scheduled:  DONE Beta\\n  a:          Scheduled:  TODO Gamma                                     :work:\\n  a:          Scheduled:  NEXT Delta\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-edit-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaEdit*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Beta\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Gamma :work:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Delta\nSCHEDULED: <2026-05-28 Wed>\n"))
          ;; First agenda
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((agenda1 (replace-regexp-in-string
                            (regexp-quote root) "<root>"
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: change Alpha to DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO Alpha")
                (replace-match "DONE Alpha"))
              ;; Second agenda
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((agenda2 (replace-regexp-in-string
                                (regexp-quote root) "<root>"
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list agenda1 agenda2))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_tag_filter_edit_clock_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"Command not allowed in this line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-clock)
  (let* ((root (make-temp-file "org-agenda-clock-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaClock*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha :work:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Beta :home:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Gamma :work:urgent:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Delta :home:\nSCHEDULED: <2026-05-28 Wed>\n"))
          ;; Agenda full
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((full (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              ;; Filter by work tag
              (org-agenda-filter-apply '("+work") 'tag)
              (let ((work-only (replace-regexp-in-string
                                (regexp-quote root) "<root>"
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                (org-agenda-filter-remove-all)
                ;; Edit: clock in on Alpha
                (org-agenda-goto nil)
                (with-current-buffer (current-buffer)
                  (org-clock-in)
                  (org-clock-out))
                ;; Re-agenda
                (org-agenda-quit)
                (org-agenda-list nil "2026-05-28" 1)
                (with-current-buffer org-agenda-buffer-name
                  (let ((after-clock (replace-regexp-in-string
                                     (regexp-quote root) "<root>"
                                     (buffer-substring-no-properties
                                      (point-min) (point-max)))))
                    (org-agenda-quit)
                    (list full work-only after-clock))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_multi_file_edit_clock_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Alpha                                     :work:\\n  a:          Scheduled:  DONE Beta                                      :home:\\n  b:          Scheduled:  NEXT Gamma                                     :work:\\n  b:          Scheduled:  TODO Delta                                     :home:\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE Alpha                                     :work:\\n  a:          Scheduled:  DONE Beta                                      :home:\\n  b:          Scheduled:  NEXT Gamma                                     :work:\\n  b:          Scheduled:  TODO Delta                                     :home:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-clock)
  (let* ((root (make-temp-file "org-agenda-multi-" t))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" root))
         (org-agenda-files (list file-a file-b))
         (org-agenda-buffer-name "*TestAgendaMulti*"))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* TODO Alpha :work:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Beta :home:\nSCHEDULED: <2026-05-28 Wed>\n"))
          (with-temp-file file-b
            (insert "* NEXT Gamma :work:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Delta :home:\nSCHEDULED: <2026-05-28 Wed>\n"))
          ;; Agenda
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((agenda1 (replace-regexp-in-string
                            (regexp-quote root) "<root>"
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: change Alpha to DONE in file-a
              (with-current-buffer (find-file-noselect file-a)
                (goto-char (point-min))
                (search-forward "TODO Alpha")
                (replace-match "DONE Alpha"))
              ;; Re-agenda
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((agenda2 (replace-regexp-in-string
                                (regexp-quote root) "<root>"
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list agenda1 agenda2))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_todo_filter_edit_reschedule_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"Command not allowed in this line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-filter-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaFilter*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Beta\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Gamma\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Delta\nSCHEDULED: <2026-05-29 Thu>\n"))
          ;; Agenda for two days
          (org-agenda-list nil "2026-05-28" 2)
          (with-current-buffer org-agenda-buffer-name
            (let ((full (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              ;; Filter TODO
              (org-agenda-filter-apply '("+TODO") 'todo)
              (let ((todo-only (replace-regexp-in-string
                                (regexp-quote root) "<root>"
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                (org-agenda-filter-remove-all)
                ;; Edit: reschedule Gamma to Thursday
                (org-agenda-goto nil)
                (with-current-buffer (current-buffer)
                  (org-reschedule nil '(5 29 2026)))
                ;; Re-agenda
                (org-agenda-quit)
                (org-agenda-list nil "2026-05-28" 2)
                (with-current-buffer org-agenda-buffer-name
                  (let ((after-reschedule (replace-regexp-in-string
                                          (regexp-quote root) "<root>"
                                          (buffer-substring-no-properties
                                           (point-min) (point-max)))))
                    (org-agenda-quit)
                    (list full todo-only after-reschedule))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_week_view_clock_filter_edit_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Week-agenda (W22):\\nMonday     25 May 2026 W22\\n  a:          Scheduled:  TODO Alpha\\nTuesday    26 May 2026\\n  a:          Scheduled:  TODO Beta\\nWednesday  27 May 2026\\n  a:          Scheduled:  DONE Gamma\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Delta\\nFriday     29 May 2026\\n  a:          Scheduled:  NEXT Epsilon\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\" \"Week-agenda (W22):\\nMonday     25 May 2026 W22\\n  a:          Scheduled:  DONE Alpha\\nTuesday    26 May 2026\\n  a:          Scheduled:  TODO Beta\\nWednesday  27 May 2026\\n  a:          Scheduled:  DONE Gamma\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Delta\\nFriday     29 May 2026\\n  a:          Scheduled:  NEXT Epsilon\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-week-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaWeek*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha\nSCHEDULED: <2026-05-25 Sun>\n")
            (insert "* TODO Beta\nSCHEDULED: <2026-05-26 Mon>\n")
            (insert "* DONE Gamma\nSCHEDULED: <2026-05-27 Tue>\n")
            (insert "* TODO Delta\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Epsilon\nSCHEDULED: <2026-05-29 Thu>\n"))
          ;; Week view
          (org-agenda-list nil "2026-05-25" 7)
          (with-current-buffer org-agenda-buffer-name
            (let ((week-view (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: change Alpha to DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO Alpha")
                (replace-match "DONE Alpha"))
              ;; Re-agenda
              (org-agenda-list nil "2026-05-25" 7)
              (with-current-buffer org-agenda-buffer-name
                (let ((after-edit (replace-regexp-in-string
                                   (regexp-quote root) "<root>"
                                   (buffer-substring-no-properties
                                    (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list week-view after-edit))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_tag_filter_edit_todo_change_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 45 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-tag-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaTag*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha :work:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Beta :home:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Gamma :work:urgent:\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Delta :home:\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((full (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              ;; Filter by work
              (org-agenda-filter-apply '("+work") 'tag)
              (let ((work-only (replace-regexp-in-string
                                (regexp-quote root) "<root>"
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                (org-agenda-filter-remove-all)
                ;; Edit: change Gamma TODO->DONE
                (with-current-buffer (find-file-noselect file)
                  (goto-char (point-min))
                  (search-forward "TODO Gamma")
                  (replace-match "DONE Gamma"))
                ;; Re-agenda
                (org-agenda-quit)
                (org-agenda-list nil "2026-05-28" 1)
                (with-current-buffer org-agenda-buffer-name
                  (let ((after-edit (replace-regexp-in-string
                                     (regexp-quote root) "<root>"
                                     (buffer-substring-no-properties
                                      (point-min) (point-max)))))
                    (org-agenda-quit)
                    (list full work-only after-edit))))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_two_day_view_edit_reschedule_multi_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"2 days-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Alpha\\n  a:          Scheduled:  DONE Beta\\nFriday     29 May 2026\\n  a:          Scheduled:  TODO Gamma\\n  a:          Scheduled:  NEXT Delta\\n\" \"2 days-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE Beta\\nFriday     29 May 2026\\n  a:          Scheduled:  TODO Gamma\\n  a:          Scheduled:  NEXT Delta\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-twoday-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaTwoday*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Alpha\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Beta\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Gamma\nSCHEDULED: <2026-05-29 Thu>\n")
            (insert "* NEXT Delta\nSCHEDULED: <2026-05-29 Thu>\n"))
          ;; Two-day view
          (org-agenda-list nil "2026-05-28" 2)
          (with-current-buffer org-agenda-buffer-name
            (let ((twoday (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: reschedule Alpha to Thursday
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO Alpha")
                (beginning-of-line)
                (org-schedule nil '(5 29 2026)))
              ;; Re-agenda
              (org-agenda-list nil "2026-05-28" 2)
              (with-current-buffer org-agenda-buffer-name
                (let ((after-reschedule (replace-regexp-in-string
                                         (regexp-quote root) "<root>"
                                         (buffer-substring-no-properties
                                          (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list twoday after-reschedule))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_three_day_multi_file_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"3 days-agenda (W22):\\nWednesday  27 May 2026\\n  work:       Scheduled:  DONE Review\\nThursday   28 May 2026\\n  work:       Scheduled:  TODO Deploy\\n  home:       Scheduled:  TODO Shopping\\nFriday     29 May 2026\\n  home:       Scheduled:  NEXT Exercise\\n\" \"3 days-agenda (W22):\\nWednesday  27 May 2026\\n  work:       Scheduled:  DONE Review\\nThursday   28 May 2026\\n  work:       Scheduled:  DONE Deploy\\n  home:       Scheduled:  TODO Shopping\\nFriday     29 May 2026\\n  home:       Scheduled:  NEXT Exercise\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-3day-" t))
         (file-a (expand-file-name "work.org" root))
         (file-b (expand-file-name "home.org" root))
         (org-agenda-files (list file-a file-b))
         (org-agenda-buffer-name "*TestAgenda3Day*"))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* TODO Deploy\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Review\nSCHEDULED: <2026-05-27 Tue>\n"))
          (with-temp-file file-b
            (insert "* TODO Shopping\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Exercise\nSCHEDULED: <2026-05-29 Thu>\n"))
          ;; Three-day view
          (org-agenda-list nil "2026-05-27" 3)
          (with-current-buffer org-agenda-buffer-name
            (let ((view (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: change Deploy to DONE
              (with-current-buffer (find-file-noselect file-a)
                (goto-char (point-min))
                (search-forward "TODO Deploy")
                (replace-match "DONE Deploy"))
              ;; Re-agenda
              (org-agenda-list nil "2026-05-27" 3)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list view after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_week_view_multi_edit_todo_reagenda_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Week-agenda (W22):\\nMonday     25 May 2026 W22\\n  a:          Scheduled:  TODO Mon-task\\nTuesday    26 May 2026\\n  a:          Scheduled:  DONE Tue-task\\nWednesday  27 May 2026\\n  a:          Scheduled:  TODO Wed-task\\nThursday   28 May 2026\\n  a:          Scheduled:  NEXT Thu-task\\nFriday     29 May 2026\\n  a:          Scheduled:  TODO Fri-task\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\" \"Week-agenda (W22):\\nMonday     25 May 2026 W22\\n  a:          Scheduled:  TODO Mon-task\\nTuesday    26 May 2026\\n  a:          Scheduled:  DONE Tue-task\\nWednesday  27 May 2026\\n  a:          Scheduled:  DONE Wed-task\\nThursday   28 May 2026\\n  a:          Scheduled:  NEXT Thu-task\\nFriday     29 May 2026\\n  a:          Scheduled:  TODO Fri-task\\nSaturday   30 May 2026\\nSunday     31 May 2026\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-weekmulti-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaWeekMulti*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Mon-task\nSCHEDULED: <2026-05-25 Mon>\n")
            (insert "* DONE Tue-task\nSCHEDULED: <2026-05-26 Tue>\n")
            (insert "* TODO Wed-task\nSCHEDULED: <2026-05-27 Wed>\n")
            (insert "* NEXT Thu-task\nSCHEDULED: <2026-05-28 Thu>\n")
            (insert "* TODO Fri-task\nSCHEDULED: <2026-05-29 Fri>\n"))
          ;; Week view
          (org-agenda-list nil "2026-05-25" 7)
          (with-current-buffer org-agenda-buffer-name
            (let ((week (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: change Wed-task to DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO Wed-task")
                (replace-match "DONE Wed-task"))
              ;; Re-agenda
              (org-agenda-list nil "2026-05-25" 7)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list week after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_single_day_todo_filter_edit_reagenda_v2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Task-1\\n  a:          Scheduled:  DONE Task-2\\n  a:          Scheduled:  TODO Task-3\\n  a:          Scheduled:  NEXT Task-4\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO Task-1\\n  a:          Scheduled:  DONE Task-2\\n  a:          Scheduled:  DONE Task-3\\n  a:          Scheduled:  NEXT Task-4\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-1day-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgenda1Day*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Task-1\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE Task-2\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO Task-3\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT Task-4\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((full (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: change Task-3 to DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO Task-3")
                (replace-match "DONE Task-3"))
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list full after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_single_day_multi_todo_edit_reagenda_v2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO A1\\n  a:          Scheduled:  DONE A2\\n  a:          Scheduled:  TODO A3\\n  a:          Scheduled:  NEXT A4\\n  a:          Scheduled:  TODO A5\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE A1\\n  a:          Scheduled:  DONE A2\\n  a:          Scheduled:  DONE A3\\n  a:          Scheduled:  NEXT A4\\n  a:          Scheduled:  TODO A5\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-sday2-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgendaSday2*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO A1\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE A2\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO A3\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT A4\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO A5\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((before (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: A1->DONE, A3->DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO A1")
                (replace-match "DONE A1")
                (goto-char (point-min))
                (search-forward "TODO A3")
                (replace-match "DONE A3"))
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list before after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_single_day_five_tasks_multi_edit_reagenda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO T1\\n  a:          Scheduled:  DONE T2\\n  a:          Scheduled:  TODO T3\\n  a:          Scheduled:  NEXT T4\\n  a:          Scheduled:  TODO T5\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE T1\\n  a:          Scheduled:  DONE T2\\n  a:          Scheduled:  DONE T3\\n  a:          Scheduled:  NEXT T4\\n  a:          Scheduled:  DONE T5\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-5task-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgenda5Task*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO T1\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE T2\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO T3\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT T4\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO T5\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((before (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: T1->DONE, T3->DONE, T5->DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO T1")
                (replace-match "DONE T1")
                (goto-char (point-min))
                (search-forward "TODO T3")
                (replace-match "DONE T3")
                (goto-char (point-min))
                (search-forward "TODO T5")
                (replace-match "DONE T5"))
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list before after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_single_day_six_tasks_multi_edit_reagenda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO F1\\n  a:          Scheduled:  DONE F2\\n  a:          Scheduled:  TODO F3\\n  a:          Scheduled:  NEXT F4\\n  a:          Scheduled:  TODO F5\\n  a:          Scheduled:  WAIT F6\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE F1\\n  a:          Scheduled:  DONE F2\\n  a:          Scheduled:  DONE F3\\n  a:          Scheduled:  NEXT F4\\n  a:          Scheduled:  DONE F5\\n  a:          Scheduled:  WAIT F6\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-6task-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgenda6Task*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO F1\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE F2\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO F3\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT F4\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO F5\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* WAIT F6\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((before (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: F1->DONE, F3->DONE, F5->DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO F1")
                (replace-match "DONE F1")
                (goto-char (point-min))
                (search-forward "TODO F3")
                (replace-match "DONE F3")
                (goto-char (point-min))
                (search-forward "TODO F5")
                (replace-match "DONE F5"))
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list before after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_single_day_seven_tasks_multi_edit_reagenda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO G1\\n  a:          Scheduled:  DONE G2\\n  a:          Scheduled:  TODO G3\\n  a:          Scheduled:  NEXT G4\\n  a:          Scheduled:  TODO G5\\n  a:          Scheduled:  WAIT G6\\n  a:          Scheduled:  TODO G7\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE G1\\n  a:          Scheduled:  DONE G2\\n  a:          Scheduled:  DONE G3\\n  a:          Scheduled:  NEXT G4\\n  a:          Scheduled:  DONE G5\\n  a:          Scheduled:  WAIT G6\\n  a:          Scheduled:  DONE G7\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-7task-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgenda7Task*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO G1\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE G2\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO G3\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT G4\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO G5\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* WAIT G6\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO G7\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((before (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: G1->DONE, G3->DONE, G5->DONE, G7->DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO G1")
                (replace-match "DONE G1")
                (goto-char (point-min))
                (search-forward "TODO G3")
                (replace-match "DONE G3")
                (goto-char (point-min))
                (search-forward "TODO G5")
                (replace-match "DONE G5")
                (goto-char (point-min))
                (search-forward "TODO G7")
                (replace-match "DONE G7"))
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list before after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_agenda_single_day_eight_tasks_multi_edit_reagenda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  TODO H1\\n  a:          Scheduled:  DONE H2\\n  a:          Scheduled:  TODO H3\\n  a:          Scheduled:  NEXT H4\\n  a:          Scheduled:  TODO H5\\n  a:          Scheduled:  WAIT H6\\n  a:          Scheduled:  TODO H7\\n  a:          Scheduled:  DONE H8\\n\" \"Day-agenda (W22):\\nThursday   28 May 2026\\n  a:          Scheduled:  DONE H1\\n  a:          Scheduled:  DONE H2\\n  a:          Scheduled:  DONE H3\\n  a:          Scheduled:  NEXT H4\\n  a:          Scheduled:  DONE H5\\n  a:          Scheduled:  WAIT H6\\n  a:          Scheduled:  DONE H7\\n  a:          Scheduled:  DONE H8\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((root (make-temp-file "org-agenda-8task-" t))
         (org-agenda-files (list (expand-file-name "a.org" root)))
         (org-agenda-buffer-name "*TestAgenda8Task*")
         (file (expand-file-name "a.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO H1\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE H2\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO H3\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* NEXT H4\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO H5\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* WAIT H6\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* TODO H7\nSCHEDULED: <2026-05-28 Wed>\n")
            (insert "* DONE H8\nSCHEDULED: <2026-05-28 Wed>\n"))
          (org-agenda-list nil "2026-05-28" 1)
          (with-current-buffer org-agenda-buffer-name
            (let ((before (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (org-agenda-quit)
              ;; Edit: H1->DONE, H3->DONE, H5->DONE, H7->DONE
              (with-current-buffer (find-file-noselect file)
                (goto-char (point-min))
                (search-forward "TODO H1")
                (replace-match "DONE H1")
                (goto-char (point-min))
                (search-forward "TODO H3")
                (replace-match "DONE H3")
                (goto-char (point-min))
                (search-forward "TODO H5")
                (replace-match "DONE H5")
                (goto-char (point-min))
                (search-forward "TODO H7")
                (replace-match "DONE H7"))
              (org-agenda-list nil "2026-05-28" 1)
              (with-current-buffer org-agenda-buffer-name
                (let ((after (replace-regexp-in-string
                              (regexp-quote root) "<root>"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                  (org-agenda-quit)
                  (list before after))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-directory root t))))"##,
        expect,
    );
}
