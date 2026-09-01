use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_timestamp_change_toggle_repeater_delay_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* TODO Task\\nSCHEDULED: <2026-05-27 Wed 10:15-11:30 +1w -2d>\\n\" \"* TODO Task\\nSCHEDULED: <2026-06-27 Sat 10:15-11:30 +1w -2d>\\n\" \"* TODO Task\\nSCHEDULED: [2026-06-27 Sat 10:15-11:30 +1w -2d]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task\n")
    (insert "SCHEDULED: <2026-05-27 Wed 09:30-10:45 +1w -2d>\n")
    (goto-char (point-min))
    (search-forward "09:30")
    (org-timestamp-change 45 'minute nil t)
    (let ((after-minute
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "2026")
      (org-timestamp-change 1 'month nil t)
      (let ((after-month
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "<")
        (org-toggle-timestamp-type)
        (list after-minute
              after-month
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_read_date_relative_default_time_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let* ((base (encode-time 0 15 8 27 5 2026))
         (org-read-date-popup-calendar nil)
         (org-overriding-default-time base))
    (list
     (org-read-date nil nil "++2w" nil base)
     (org-read-date t nil "++3d 14:45" nil base)
     (format-time-string
      "%Y-%m-%d %H:%M"
      (org-read-date t t "+1m" nil base))
     (mapcar
      (lambda (s)
        (let ((ts (org-timestamp-from-string s)))
          (list s
                (format-time-string
                 "%Y-%m-%d %H:%M"
                 (org-timestamp-to-time ts))
                (org-timestamp-has-time-p ts))))
      '("<2026-05-27 Wed>"
        "[2026-05-27 Wed 09:30]"
        "<2026-05-27 Wed 09:30-10:45>"))))"##,
        expect,
    );
}

#[test]
fn org_timestamp_time_range_eval_parse_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"2 hours 30 minutes \" \"Reverse <2026-05-28 Thu 14:00>--<2026-05-28 Thu 12:30> - 01:30\" \"| A    | <2026-05-27 Wed 10:00>--<2026-05-29 Fri 12:30> | 2d 02:30 |\" (\"09:15\" \"09:15+2:30\" \"23:50+-24:20\" nil) ((\"<2026-05-27 Wed>\" 739763 \"2026-05-27 00:00\" 1779854400.0) (\"<2026-05-27 Wed 09:15>\" 739763 \"2026-05-27 09:15\" 1779887700.0)) (org-diary-sexp-no-match \"%%(diary-date 5 27 2026)\") (\"2 hours 30 minutes \" \"Time difference inserted\" \"Time difference inserted\") \"* Ranges\\nInline <2026-05-27 Wed 09:15>--<2026-05-27 Wed 11:45>\\nReverse <2026-05-28 Thu 14:00>--<2026-05-28 Thu 12:30> - 01:30\\n| Task | Range                                          | Diff     |\\n| A    | <2026-05-27 Wed 10:00>--<2026-05-29 Fri 12:30> | 2d 02:30 |\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Ranges\n")
    (insert "Inline <2026-05-27 Wed 09:15>--<2026-05-27 Wed 11:45>\n")
    (insert "Reverse <2026-05-28 Thu 14:00>--<2026-05-28 Thu 12:30>\n")
    (insert "| Task | Range | Diff |\n")
    (insert "| A | <2026-05-27 Wed 10:00>--<2026-05-29 Fri 12:30> | |\n")
    (goto-char (point-min))
    (search-forward "Inline")
    (let ((messages nil))
      (cl-letf (((symbol-function 'message)
                 (lambda (fmt &rest args)
                   (push (apply #'format fmt args) messages))))
        (org-evaluate-time-range nil)
        (let ((inline-message (car messages)))
          (goto-char (point-min))
          (search-forward "Reverse")
          (org-evaluate-time-range t)
          (let ((after-reverse
                 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))))
            (goto-char (point-min))
            (search-forward "| A |")
            (org-evaluate-time-range t)
            (let ((after-table
                   (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))))
              (list inline-message
                    after-reverse
                    after-table
                    (mapcar #'org-get-compact-tod
                            '("09:15" "09:15-11:45" "23:50-00:10"
                              "bad"))
                    (mapcar (lambda (s)
                              (list s
                                    (org-time-string-to-absolute s)
                                    (format-time-string
                                     "%Y-%m-%d %H:%M"
                                     (org-time-string-to-time s))
                                    (org-time-string-to-seconds s)))
                            '("<2026-05-27 Wed>"
                              "<2026-05-27 Wed 09:15>"))
                    (condition-case err
                        (org-time-string-to-absolute
                         "%%(diary-date 5 27 2026)")
                      (error (cons (car err) (cdr err))))
                    (nreverse messages)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_planning_repeater_warning_element_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Habit\n")
    (insert "SCHEDULED: <2026-05-27 Wed .+2d/4d> ")
    (insert "DEADLINE: <2026-06-01 Mon +1w -3d>\n")
    (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
    (let ((tree (org-element-parse-buffer)))
      (org-element-map tree 'planning
        (lambda (planning)
          (let ((scheduled (org-element-property :scheduled planning))
                (deadline (org-element-property :deadline planning)))
            (list
             (mapcar
              (lambda (ts)
                (list (org-element-property :raw-value ts)
                      (org-element-property :repeater-type ts)
                      (org-element-property :repeater-value ts)
                      (org-element-property :repeater-unit ts)
                      (org-element-property :warning-type ts)
                      (org-element-property :warning-value ts)
                      (org-element-property :warning-unit ts)))
              (list scheduled deadline))
             (org-deadline-close-p
              (org-element-property :raw-value deadline)
              7)
             (format-time-string
              "%Y-%m-%d"
              (org-timestamp-to-time scheduled))
             (format-time-string
              "%Y-%m-%d"
              (org-timestamp-to-time deadline))))))))"##,
        expect,
    );
}

#[test]
fn org_schedule_deadline_timestamp_range_shift_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-log-reschedule nil)
          (org-log-redeadline nil))
      (org-mode)
      (insert "* TODO Task\n")
      (insert "Body with <2026-05-27 Wed 09:30-10:45> and ")
      (insert "[2026-05-28 Thu].\n")
      (goto-char (point-min))
      (org-schedule nil "2026-06-03 08:15")
      (org-deadline nil "2026-06-10 +1w -2d")
      (let ((after-planning
             (buffer-substring-no-properties (point-min) (point-max))))
        (search-forward "09:30")
        (let* ((range-ts (org-timestamp-at-point))
               (split-start
                (mapcar (lambda (ts)
                          (and ts (org-element-property :raw-value ts)))
                        (org-timestamp-split-range range-ts)))
               (translated-start
                (org-timestamp-translate range-ts 'start))
               (translated-end
                (org-timestamp-translate range-ts 'end)))
          (org-timestamp-up-day 2)
          (search-forward "[2026")
          (org-timestamp-down-day 1)
          (goto-char (point-max))
          (insert "\n")
          (org-timestamp nil nil)
          (insert "\n")
          (org-timestamp-inactive nil)
          (let ((parsed
                 (mapcar
                  (lambda (s)
                    (let ((ts (org-timestamp-from-string s)))
                      (list s
                            (org-element-property :raw-value ts)
                            (org-timestamp-has-time-p ts)
                            (format-time-string
                             "%Y-%m-%d %H:%M"
                             (org-timestamp-to-time ts))
                            (format-time-string
                             "%Y-%m-%d %H:%M"
                             (org-timestamp-to-time ts 'end)))))
                  '("<2026-05-27 Wed 09:30-10:45>"
                    "<2026-06-03 Wed 08:15>"
                    "[2026-05-28 Thu]"))))
            (list after-planning
                  split-start
                  translated-start
                  translated-end
                  parsed
                  (replace-regexp-in-string
                   "\\[[0-9][^]\n]+\\]\\|<[0-9][^>\n]+>"
                   "[stamp]"
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_todo_auto_repeat_planning_logbook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO" "NEXT" "|" "DONE")))
          (org-log-repeat 'time)
          (org-log-into-drawer t)
          (org-log-done nil)
          (org-todo-repeat-hook
           (list (lambda ()
                   (push (list (org-get-heading t t t t)
                               (org-get-todo-state)
                               (org-entry-get nil "SCHEDULED")
                               (org-entry-get nil "DEADLINE")
                               (org-entry-get nil "LAST_REPEAT"))
                         events))))
          events)
      (org-mode)
      (insert "* TODO Repeating task\n")
      (insert ":PROPERTIES:\n")
      (insert ":REPEAT_TO_STATE: NEXT\n")
      (insert ":END:\n")
      (insert "SCHEDULED: <2026-05-20 Wed .+2d> ")
      (insert "DEADLINE: <2026-05-20 Wed +1w -2d>\n")
      (insert "Body timestamp <2026-05-20 Wed +1m>\n")
      (insert ":LOGBOOK:\n")
      (insert "CLOCK: [2026-05-26 Tue 09:00]--[2026-05-26 Tue 10:15] =>  1:15\n")
      (insert ":END:\n")
      (goto-char (point-min))
      (cl-letf (((symbol-function 'org-current-time)
                 (lambda (&rest _)
                   (encode-time 0 0 12 27 5 2026))))
        (org-todo "DONE"))
      (let ((after-done
             (buffer-substring-no-properties (point-min) (point-max)))
            (state-after (org-get-todo-state))
            (repeat-after (org-get-repeat))
            (last-repeat (org-entry-get nil "LAST_REPEAT")))
        (goto-char (point-min))
        (search-forward "+1m")
        (org-cancel-repeater)
        (list state-after
              repeat-after
              last-repeat
              (nreverse events)
              after-done
              (buffer-substring-no-properties
               (point-min) (point-max))
              (org-element-map (org-element-parse-buffer)
                  '(headline planning timestamp node-property)
                (lambda (el)
                  (pcase (org-element-type el)
                    ('headline
                     (list 'headline
                           (org-element-property :todo-keyword el)
                           (org-element-property :raw-value el)))
                    ('planning
                     (list 'planning
                           (and (org-element-property :scheduled el)
                                (org-element-property
                                 :raw-value
                                 (org-element-property :scheduled el)))
                           (and (org-element-property :deadline el)
                                (org-element-property
                                 :raw-value
                                 (org-element-property :deadline el)))))
                    ('timestamp
                     (list 'timestamp
                           (org-element-property :raw-value el)
                           (org-element-property :repeater-type el)
                           (org-element-property :repeater-value el)
                           (org-element-property :repeater-unit el)
                           (org-element-property :warning-type el)
                           (org-element-property :warning-value el)))
                    ('node-property
                     (list 'property
                           (org-element-property :key el)
                           (org-element-property :value el))))))))))"##,
        expect,
    );
}

#[test]
fn org_planning_agenda_repeat_logbook_cookie_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"TODO\" 0 4 (org-todo-head \"TODO\")) \"<2026-06-17 Wed 08:30 .+2d>\" \"<2026-06-06 Sat +1w -2d>\" \"[2026-05-27 Wed 13:45]\" \"Ada\" \"+1w\") (#(\"DONE\" 0 4 (org-todo-head \"TODO\")) #(\"Checklist [2/2]\" 0 15 (org-todo-head \"TODO\")) nil) ((t nil nil t) \"3 days-agenda (W22):\\nWednesday  27 May 2026\\n  Plan:       11:00-12:15 TODO Repeat                                   :work::\\n\") nil \"3 days-agenda (W22):\\nWednesday  27 May 2026\\n  Plan:       11:00-12:15 TODO Repeat                                   :work::\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-clock)
  (let* ((file (make-temp-file
                "org-plan-agenda" nil ".org"
                "#+CATEGORY: Plan
* TODO Project [0/2] :work:
:PROPERTIES:
:Owner: Ada
:END:
** TODO Repeat
SCHEDULED: <2026-05-20 Wed .+2d> DEADLINE: <2026-05-22 Fri +1w -2d>
:LOGBOOK:
CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30
:END:
Body <2026-05-27 Wed 11:00-12:15>
** TODO Checklist [0/2]
- [ ] first
- [X] second
"))
         (org-agenda-files (list file))
         (org-agenda-start-day "2026-05-27")
         (org-agenda-span 3)
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-log-repeat 'time)
         (org-log-done nil)
         (org-log-reschedule 'time)
         (org-log-redeadline 'time)
         (org-log-into-drawer "LOGBOOK")
         (org-todo-keywords '((sequence "TODO" "NEXT" "|" "DONE"))))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (let (repeat-state checklist-state agenda-summary parsed)
            (cl-letf (((symbol-function 'org-current-time)
                       (lambda (&rest _)
                         (encode-time 0 45 13 27 5 2026))))
              (goto-char (point-min))
              (search-forward "Repeat")
              (beginning-of-line)
              (org-schedule nil "2026-05-28 08:30")
              (org-deadline nil "2026-05-30 +1w -1d")
              (org-todo "DONE")
              (setq repeat-state
                    (list (org-get-todo-state)
                          (org-entry-get nil "SCHEDULED")
                          (org-entry-get nil "DEADLINE")
                          (org-entry-get nil "LAST_REPEAT")
                          (org-entry-get nil "Owner" t)
                          (org-get-repeat)))
              (goto-char (point-min))
              (search-forward "first")
              (org-ctrl-c-ctrl-c)
              (goto-char (point-min))
              (search-forward "Checklist")
              (beginning-of-line)
              (org-update-statistics-cookies t)
              (org-todo "DONE")
              (setq checklist-state
                    (list (org-get-todo-state)
                          (neovm--oracle-coalesce-string-properties
                           (org-get-heading t t t t))
                          (org-entry-get nil "CLOSED"))))
            (save-buffer)
            (org-agenda-list nil "2026-05-27" 3)
            (with-current-buffer org-agenda-buffer-name
              (setq agenda-summary
                    (let ((text (buffer-substring-no-properties
                                 (point-min) (point-max))))
                      (list (mapcar (lambda (needle)
                                      (not (null
                                            (string-match-p needle text))))
                                    '("Repeat" "Checklist" "Project"
                                      "Plan"))
                            text))))
            (setq parsed
                  (org-element-map (org-element-parse-buffer)
                      '(headline planning timestamp node-property item)
                    (lambda (el)
                      (pcase (org-element-type el)
                        ('headline
                         (list 'headline
                               (org-element-property :level el)
                               (org-element-property :todo-keyword el)
                               (org-element-property :raw-value el)))
                        ('planning
                         (list 'planning
                               (and (org-element-property :scheduled el)
                                    (org-element-property
                                     :raw-value
                                     (org-element-property :scheduled el)))
                               (and (org-element-property :deadline el)
                                    (org-element-property
                                     :raw-value
                                     (org-element-property :deadline el)))
                               (and (org-element-property :closed el)
                                    (org-element-property
                                     :raw-value
                                     (org-element-property :closed el)))))
                        ('timestamp
                         (list 'timestamp
                               (org-element-property :raw-value el)
                               (org-element-property :repeater-type el)
                               (org-element-property :repeater-value el)
                               (org-element-property :warning-type el)
                               (org-element-property :warning-value el)))
                        ('node-property
                         (list 'property
                               (org-element-property :key el)
                               (org-element-property :value el)))
                        ('item
                         (list 'item
                               (org-element-property :checkbox el)))))))
            (list repeat-state
                  checklist-state
                  agenda-summary
                  parsed
                  (replace-regexp-in-string
                   "org-plan-agenda[^ \n|]+\\.org"
                   "org-plan-agenda<tmp>.org"
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_timestamp_parse_shift_range_element_extract_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-27 Wed 10:00-11:30 +1w -3d>\n")
    (insert "DEADLINE: <2026-05-29 Fri>\n")
    (insert "CLOSED: [2026-05-26 Mon 15:30]\n\n")
    (insert "* WAIT Beta\n")
    (insert "SCHEDULED: <2026-05-28 Thu .+2d/5d>\n")
    (insert "DEADLINE: <2026-06-01 Mon 09:00>\n\n")
    (insert "* Meeting\n")
    (insert "<2026-05-30 Fri 14:00-15:00>\n")
    (insert "[2026-05-27 Wed]\n")
    (let ((parsed
           (org-element-map (org-element-parse-buffer)
               '(planning timestamp headline)
             (lambda (el)
               (pcase (org-element-type el)
                 ('headline
                  (list 'headline
                        (org-element-property :raw-value el)
                        (substring-no-properties
                         (or (org-element-property :todo-keyword el) ""))))
                 ('planning
                  (list 'planning
                        (org-element-property :raw-value
                         (org-element-property :scheduled el))
                        (org-element-property :raw-value
                         (org-element-property :deadline el))
                        (org-element-property :raw-value
                         (org-element-property :closed el))))
                 ('timestamp
                  (list 'timestamp
                        (org-element-property :type el)
                        (org-element-property :raw-value el)
                        (org-element-property :year-start el)
                        (org-element-property :month-start el)
                        (org-element-property :day-start el)
                        (org-element-property :hour-start el)
                        (org-element-property :minute-start el)
                        (org-element-property :hour-end el)
                        (org-element-property :minute-end el)
                        (org-element-property :repeater-type el)
                        (org-element-property :repeater-value el)
                        (org-element-property :repeater-unit el)
                        (org-element-property :warning-type el)
                        (org-element-property :warning-value el)))))))
      ;; Shift timestamp
      (goto-char (point-min))
      (search-forward "Meeting")
      (forward-line 1)
      (beginning-of-line)
      (org-timestamp-change 1 'day)
      (let ((after-shift (buffer-substring-no-properties
                          (point-min) (point-max))))
        ;; Parse after shift
        (goto-char (point-min))
        (let ((shifted-ts
               (org-element-map (org-element-parse-buffer) 'timestamp
                 (lambda (ts)
                   (list (org-element-property :raw-value ts)
                         (org-element-property :day-start ts)
                         (org-element-property :month-start ts))))))
          (list parsed
                after-shift
                shifted-ts))))))"##,
        expect,
    );
}

#[test]
fn org_planning_timestamp_shift_edit_recurring_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a timestamp\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Weekly Review\n")
    (insert "SCHEDULED: <2026-05-25 Sun +1w>\n")
    (insert "DEADLINE: <2026-05-28 Wed -2d>\n")
    (insert "Body.\n\n")
    (insert "* DONE One-shot\n")
    (insert "SCHEDULED: <2026-05-20 Wed>\n")
    (insert "CLOSED: [2026-05-21 Thu 10:00]\n")
    (insert "Body.\n\n")
    ;; Parse timestamps
    (let ((snap (lambda ()
                  (org-element-map (org-element-parse-buffer) 'timestamp
                    (lambda (ts)
                      (list (org-element-property :raw-value ts)
                            (org-element-property :type ts)
                            (org-element-property :day-start ts)
                            (org-element-property :month-start ts)
                            (org-element-property :year-start ts)
                            (org-element-property :repeater-type ts)
                            (org-element-property :repeater-value ts)
                            (org-element-property :repeater-unit ts)
                            (org-element-property :warning-type ts)
                            (org-element-property :warning-value ts)))))))
      (let ((initial (funcall snap)))
        ;; Shift SCHEDULED of Weekly Review by 1 day
        (goto-char (point-min))
        (search-forward "Weekly Review")
        (forward-line 1)
        (beginning-of-line)
        (org-timestamp-change 1 'day)
        (let ((after-shift1 (funcall snap)))
          ;; Shift DEADLINE by 2 days
          (forward-line 1)
          (beginning-of-line)
          (org-timestamp-change 2 'day)
          (let ((after-shift2 (funcall snap)))
            ;; Edit: change TODO to DONE
            (goto-char (point-min))
            (search-forward "TODO Weekly")
            (replace-match "DONE Weekly")
            (let ((after-edit (buffer-substring-no-properties
                               (point-min) (point-max))))
              (list initial
                    after-shift1
                    after-shift2
                    after-edit
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}
