use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_habit_parse_urgency_faces_graph_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((739760 2 739763 5 (739756 739759) \".+\") 1180 ((org-habit-clear-face . org-habit-clear-future-face) (org-habit-ready-face . org-habit-ready-future-face) (org-habit-alert-face . org-habit-alert-future-face) (org-habit-overdue-face . org-habit-overdue-future-face) (org-habit-overdue-face . org-habit-overdue-future-face)) #(\" *   !    \" 0 1 (face org-habit-ready-future-face help-echo \"<2026-05-22 Fri>\") 1 2 (face org-habit-ready-face help-echo \"<2026-05-23 Sat> DONE\") 2 3 (face org-habit-ready-future-face help-echo \"<2026-05-24 Sun>\") 3 4 (face org-habit-ready-future-face help-echo \"<2026-05-25 Mon>\") 4 5 (face org-habit-ready-future-face help-echo \"<2026-05-26 Tue>\") 5 6 (face org-habit-alert-face help-echo \"<2026-05-27 Wed>\") 6 7 (face org-habit-overdue-future-face help-echo \"<2026-05-28 Thu>\") 7 8 (face org-habit-overdue-future-face help-echo \"<2026-05-29 Fri>\") 8 9 (face org-habit-overdue-future-face help-echo \"<2026-05-30 Sat>\")) ((32 org-habit-ready-future-face \"<2026-05-22 Fri>\") (42 org-habit-ready-face \"<2026-05-23 Sat> DONE\") (32 org-habit-ready-future-face \"<2026-05-24 Sun>\") (32 org-habit-ready-future-face \"<2026-05-25 Mon>\") (32 org-habit-ready-future-face \"<2026-05-26 Tue>\") (33 org-habit-alert-face \"<2026-05-27 Wed>\") (32 org-habit-overdue-future-face \"<2026-05-28 Thu>\") (32 org-habit-overdue-future-face \"<2026-05-29 Fri>\") (32 org-habit-overdue-future-face \"<2026-05-30 Sat>\") (32 nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-habit)
  (with-temp-buffer
    (let ((org-habit-preceding-days 5)
          (org-habit-following-days 4)
          (org-habit-today-glyph ?!)
          (org-habit-completed-glyph ?*))
      (org-mode)
      (insert "* TODO Run\n")
      (insert "SCHEDULED: <2026-05-24 Sun .+2d/5d>\n")
      (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
      (insert ":LOGBOOK:\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-20 Wed]\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-23 Sat]\n")
      (insert ":END:\n")
      (goto-char (point-min))
      (let* ((habit (org-habit-parse-todo))
             (today (encode-time 0 0 12 27 5 2026))
             (graph (org-habit-build-graph
                     habit
                     (encode-time 0 0 12 22 5 2026)
                     today
                     (encode-time 0 0 12 31 5 2026))))
        (list
         habit
         (org-habit-get-urgency habit today)
         (mapcar (lambda (offset)
                   (org-habit-get-faces
                    habit
                    (+ (time-to-days today) offset)))
                 '(-4 -2 0 2 5))
         graph
         (mapcar (lambda (i)
                   (list (aref graph i)
                         (get-text-property i 'face graph)
                         (get-text-property i 'help-echo graph)))
                 (number-sequence 0 (1- (length graph)))))))))"#,
        expect,
    );
}

#[test]
fn org_habit_repeater_types_shift_graph_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Plus\" (739756 3 739759 6 (739757 739761) \"+\") 1460 #(\"   *!   \" 0 1 (face org-habit-overdue-face help-echo \"<2026-05-22 Fri>\") 1 2 (face org-habit-overdue-face help-echo \"<2026-05-23 Sat>\") 2 3 (face org-habit-overdue-face help-echo \"<2026-05-24 Sun>\") 3 4 (face org-habit-overdue-face help-echo \"<2026-05-25 Mon> DONE\") 4 5 (face org-habit-overdue-face help-echo \"<2026-05-26 Tue>\") 5 6 (face org-habit-overdue-future-face help-echo \"<2026-05-27 Wed>\") 6 7 (face org-habit-overdue-future-face help-echo \"<2026-05-28 Thu>\")) (org-habit-overdue-face org-habit-overdue-face org-habit-overdue-face org-habit-overdue-face org-habit-overdue-face org-habit-overdue-future-face org-habit-overdue-future-face nil)) (\"Double\" (739756 3 739759 6 (739757 739761) \"++\") 1460 #(\"   *!   \" 0 1 (face org-habit-clear-future-face help-echo \"<2026-05-22 Fri>\") 1 2 (face org-habit-ready-future-face help-echo \"<2026-05-23 Sat>\") 2 3 (face org-habit-ready-future-face help-echo \"<2026-05-24 Sun>\") 3 4 (face org-habit-ready-face help-echo \"<2026-05-25 Mon> DONE\") 4 5 (face org-habit-overdue-face help-echo \"<2026-05-26 Tue>\") 5 6 (face org-habit-overdue-future-face help-echo \"<2026-05-27 Wed>\") 6 7 (face org-habit-overdue-future-face help-echo \"<2026-05-28 Thu>\")) (org-habit-clear-future-face org-habit-ready-future-face org-habit-ready-future-face org-habit-ready-face org-habit-overdue-face org-habit-overdue-future-face org-habit-overdue-future-face nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-habit)
  (with-temp-buffer
    (let ((org-habit-preceding-days 4)
          (org-habit-following-days 3))
      (org-mode)
      (insert "* TODO Plus\nSCHEDULED: <2026-05-20 Wed +3d/6d>\n")
      (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-21 Thu]\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-25 Mon]\n")
      (insert "* TODO Double\nSCHEDULED: <2026-05-20 Wed ++3d/6d>\n")
      (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-21 Thu]\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-25 Mon]\n")
      (let (out)
        (goto-char (point-min))
        (while (re-search-forward "^\\* TODO" nil t)
          (beginning-of-line)
          (let* ((habit (org-habit-parse-todo))
                 (graph (org-habit-build-graph
                         habit
                         (encode-time 0 0 12 22 5 2026)
                         (encode-time 0 0 12 26 5 2026)
                         (encode-time 0 0 12 29 5 2026))))
            (push (list (org-get-heading t t t t)
                        habit
                        (org-habit-get-urgency
                         habit
                         (encode-time 0 0 12 26 5 2026))
                        graph
                        (mapcar (lambda (i)
                                  (get-text-property i 'face graph))
                                (number-sequence 0 (1- (length graph)))))
                  out))
          (forward-line 1))
        (nreverse out)))))"#,
        expect,
    );
}

#[test]
fn org_habit_invalid_repeater_errors_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Missing schedule\" (error \"Habit Missing schedule has no scheduled date\")) (\"Missing repeat\" (error \"Habit ‘Missing repeat’ has no scheduled repeat period or has an incorrect one\")) (\"Bad deadline\" (error \"Habit Bad deadline deadline repeat period is less than or equal to scheduled (.+2d/2d)\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-habit)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Missing schedule\n:PROPERTIES:\n:STYLE: habit\n:END:\n")
    (insert "* TODO Missing repeat\nSCHEDULED: <2026-05-27 Wed>\n")
    (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
    (insert "* TODO Bad deadline\nSCHEDULED: <2026-05-27 Wed .+2d/2d>\n")
    (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
    (let (out)
      (goto-char (point-min))
      (while (re-search-forward "^\\* TODO" nil t)
        (beginning-of-line)
        (push
         (list
          (org-get-heading t t t t)
          (condition-case err
              (org-habit-parse-todo)
            (error (list (car err) (cadr err)))))
         out)
        (forward-line 1))
      (nreverse out))))"#,
        expect,
    );
}

#[test]
fn org_habit_agenda_graph_toggle_redo_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Habits:       Scheduled:  TODO Normal scheduled                         :plain:\" nil \"led                         :plain:\" ((108 org-scheduled-previously) (101 org-scheduled-previously) (100 org-scheduled-previously) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (58 (org-tag org-scheduled-previously)) (112 (org-tag org-scheduled-previously)) (108 (org-tag org-scheduled-previously)) (97 (org-tag org-scheduled-previously)) (105 (org-tag org-scheduled-previously)) (110 (org-tag org-scheduled-previously)) (58 (org-tag org-scheduled-previously))) \"past-scheduled\" 1099)) ((\"Habits:       Scheduled:  TODO Normal scheduled                         :plain:\" nil \"led                         :plain:\" ((108 org-scheduled-previously) (101 org-scheduled-previously) (100 org-scheduled-previously) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (58 (org-tag org-scheduled-previously)) (112 (org-tag org-scheduled-previously)) (108 (org-tag org-scheduled-previously)) (97 (org-tag org-scheduled-previously)) (105 (org-tag org-scheduled-previously)) (110 (org-tag org-scheduled-previously)) (58 (org-tag org-scheduled-previously))) \"past-scheduled\" 1099)) nil t ((\"Habits:       Scheduled:  TODO Normal scheduled                         :plain:\" nil \"led                         :plain:\" ((108 org-scheduled-previously) (101 org-scheduled-previously) (100 org-scheduled-previously) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (32 nil) (58 (org-tag org-scheduled-previously)) (112 (org-tag org-scheduled-previously)) (108 (org-tag org-scheduled-previously)) (97 (org-tag org-scheduled-previously)) (105 (org-tag org-scheduled-previously)) (110 (org-tag org-scheduled-previously)) (58 (org-tag org-scheduled-previously))) \"past-scheduled\" 1099)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-habit)
  (let* ((file (make-temp-file "org-habit-agenda" nil ".org"))
         (org-agenda-files (list file))
         (org-agenda-span 1)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-prefix-format "%-8:c%5e % s")
         (org-agenda-sorting-strategy '((agenda habit-down priority-down category-keep)))
         (org-habit-preceding-days 3)
         (org-habit-following-days 3)
         (org-habit-graph-column 44)
         (org-habit-today-glyph ?!)
         (org-habit-completed-glyph ?*)
         (org-habit-show-habits t)
         (org-habit-show-habits-only-for-today t)
         (org-habit-show-all-today nil)
         (org-habit-show-done-always-green nil)
         (org-extend-today-until 0))
    (cl-labels
        ((agenda-snapshot
          ()
          (with-current-buffer org-agenda-buffer-name
            (let (rows)
              (goto-char (point-min))
              (while (re-search-forward
                      "Morning check\\|Weekly review\\|Normal scheduled" nil t)
                (beginning-of-line)
                (let* ((bol (point))
                       (eol (line-end-position))
                       (line (buffer-substring bol eol))
                       (habit (get-text-property bol 'org-habit-p))
                       (graph-start
                        (save-excursion
                          (move-to-column org-habit-graph-column)
                          (point)))
                       (graph (and (<= graph-start eol)
                                   (buffer-substring graph-start eol)))
                       (graph-props
                        (and graph
                             (mapcar
                              (lambda (i)
                                (list (aref graph i)
                                      (get-text-property
                                       (+ graph-start i) 'face)))
                              (number-sequence 0 (1- (length graph)))))))
                  (push (list (buffer-substring-no-properties bol eol)
                              (and habit
                                   (list (nth 1 habit)
                                         (nth 2 habit)
                                         (nth 3 habit)
                                         (nth 5 habit)))
                              (and graph
                                   (substring-no-properties graph))
                              graph-props
                              (get-text-property bol 'type)
                              (get-text-property bol 'urgency))
                        rows))
                (forward-line 1))
              (nreverse rows)))))
      (unwind-protect
          (progn
            (with-temp-file file
              (insert "#+CATEGORY: Habits\n")
              (insert "* TODO Morning check :health:\n")
              (insert "SCHEDULED: <2026-05-25 Mon .+1d/3d>\n")
              (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:10\n:END:\n")
              (insert ":LOGBOOK:\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-23 Sat]\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-26 Tue]\n")
              (insert ":END:\n")
              (insert "* TODO Weekly review :work:\n")
              (insert "SCHEDULED: <2026-05-27 Wed ++1w/2w>\n")
              (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 1:00\n:END:\n")
              (insert ":LOGBOOK:\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-13 Wed]\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-20 Wed]\n")
              (insert ":END:\n")
              (insert "* TODO Normal scheduled :plain:\n")
              (insert "SCHEDULED: <2026-05-27 Wed>\n"))
            (org-agenda-list nil "2026-05-27" 1)
            (let ((initial (agenda-snapshot)))
              (with-current-buffer org-agenda-buffer-name
                (org-habit-toggle-display-in-agenda nil))
              (let ((hidden (agenda-snapshot))
                    (hidden-flag org-habit-show-habits))
                (with-current-buffer org-agenda-buffer-name
                  (org-habit-toggle-display-in-agenda nil))
                (list initial
                      hidden
                      hidden-flag
                      org-habit-show-habits
                      (agenda-snapshot)))))
        (when (get-buffer org-agenda-buffer-name)
          (kill-buffer org-agenda-buffer-name))
        (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
        (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_habit_repeat_done_mutation_graph_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'org-habit)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO(t)" "WAIT(w)" "|" "DONE(d)")))
          (org-log-done 'time)
          (org-log-repeat 'time)
          (org-log-into-drawer "LOGBOOK")
          (org-habit-preceding-days 6)
          (org-habit-following-days 5)
          (org-habit-today-glyph ?!)
          (org-habit-completed-glyph ?*)
          (org-habit-show-done-always-green nil))
      (org-mode)
      (insert "#+TODO: TODO(t) WAIT(w) | DONE(d)\n")
      (insert "* TODO Fixed cadence :fixed:\n")
      (insert "SCHEDULED: <2026-05-25 Mon ++2d/5d>\n")
      (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
      (insert ":LOGBOOK:\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-21 Thu]\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-23 Sat]\n")
      (insert ":END:\n")
      (insert "* TODO Completion cadence :complete:\n")
      (insert "SCHEDULED: <2026-05-24 Sun .+3d/7d>\n")
      (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
      (insert ":LOGBOOK:\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-18 Mon]\n")
      (insert "- State \"DONE\" from \"TODO\" [2026-05-24 Sun]\n")
      (insert ":END:\n")
      (cl-labels
          ((graph-props
            (graph)
            (mapcar (lambda (i)
                      (list (aref graph i)
                            (get-text-property i 'face graph)
                            (get-text-property i 'help-echo graph)))
                    (number-sequence 0 (1- (length graph)))))
           (habit-snapshot
            ()
            (let (out)
              (goto-char (point-min))
              (while (re-search-forward "^\\* TODO" nil t)
                (beginning-of-line)
                (let* ((heading (org-get-heading t t t t))
                       (scheduled (org-entry-get (point) "SCHEDULED"))
                       (habit (org-habit-parse-todo))
                       (graph (org-habit-build-graph
                               habit
                               (encode-time 0 0 12 21 5 2026)
                               (encode-time 0 0 12 27 5 2026)
                               (encode-time 0 0 12 1 6 2026))))
                  (push (list heading
                              scheduled
                              habit
                              (org-habit-get-urgency
                               habit
                               (encode-time 0 0 12 27 5 2026))
                              graph
                              (graph-props graph))
                        out))
                (forward-line 1))
              (nreverse out))))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (encode-time 0 45 9 27 5 2026))))
          (let ((before (habit-snapshot)))
            (goto-char (point-min))
            (re-search-forward "^\\* TODO Fixed cadence")
            (beginning-of-line)
            (org-todo "DONE")
            (let ((after-fixed (habit-snapshot)))
              (goto-char (point-min))
              (re-search-forward "^\\* TODO Completion cadence")
              (beginning-of-line)
              (org-todo "DONE")
              (list before
                    after-fixed
                    (habit-snapshot)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
    );
}

#[test]
fn org_habit_agenda_past_delay_all_today_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((default nil nil t nil) (habit-past-window 10 nil t nil) (all-today nil t t nil) (future-with-habits nil nil nil ((\"HabitPast: 0:05 TODO Future stretch                 !                  :future:\" (739765 1 739767 3 nil \".+\") \"    !                  :future:\" ((32 org-habit-overdue-face habit-date) (32 org-habit-overdue-face habit-date) (32 org-habit-overdue-face habit-date) (32 org-habit-overdue-face habit-date) (33 org-habit-overdue-face habit-date) (32 org-habit-overdue-future-face habit-date) (32 org-habit-overdue-future-face habit-date) (32 nil nil) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (32 nil agenda-jump) (58 (org-tag org-scheduled) agenda-jump) (102 (org-tag org-scheduled) agenda-jump) (117 (org-tag org-scheduled) agenda-jump) (116 (org-tag org-scheduled) agenda-jump) (117 (org-tag org-scheduled) agenda-jump) (114 (org-tag org-scheduled) agenda-jump) (101 (org-tag org-scheduled) agenda-jump) (58 (org-tag org-scheduled) agenda-jump)) \"past-scheduled\" 739765 1000 5.0))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (require 'org-habit)
  (let* ((file (make-temp-file "org-habit-past-delay" nil ".org"))
         (org-agenda-files (list file))
         (org-agenda-span 1)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-prefix-format "%-10:c%5e % s")
         (org-agenda-sorting-strategy '((agenda habit-down priority-down category-keep)))
         (org-scheduled-past-days 3)
         (org-scheduled-delay-days 0)
         (org-habit-preceding-days 4)
         (org-habit-following-days 3)
         (org-habit-graph-column 48)
         (org-habit-today-glyph ?!)
         (org-habit-completed-glyph ?*)
         (org-habit-show-habits t)
         (org-habit-show-habits-only-for-today t)
         (org-habit-show-all-today nil)
         (org-habit-scheduled-past-days nil)
         (org-extend-today-until 0))
    (cl-labels
        ((snapshot
          (label)
          (with-current-buffer org-agenda-buffer-name
            (let (rows)
              (goto-char (point-min))
              (while (re-search-forward
                      "Old stretch\\|Recent stretch\\|Future stretch\\|Plain stale"
                      nil t)
                (beginning-of-line)
                (let* ((bol (point))
                       (eol (line-end-position))
                       (plain (buffer-substring-no-properties bol eol))
                       (habit (get-text-property bol 'org-habit-p))
                       (graph-start
                        (save-excursion
                          (move-to-column org-habit-graph-column t)
                          (point)))
                       (graph
                        (and (<= graph-start eol)
                             (buffer-substring graph-start eol)))
                       (graph-props
                        (and graph
                             (mapcar
                              (lambda (i)
                                (let ((help
                                       (get-text-property
                                        (+ graph-start i) 'help-echo)))
                                  (list (aref graph i)
                                        (get-text-property
                                         (+ graph-start i) 'face)
                                        (cond
                                         ((null help) nil)
                                         ((string-match-p "\\`<[^>]+>\\(?: DONE\\)?\\'" help)
                                          'habit-date)
                                         ((string-match-p "jump to Org file" help)
                                          'agenda-jump)
                                         (t help)))))
                              (number-sequence 0 (1- (length graph)))))))
                  (push
                   (list plain
                         (and habit
                              (list (nth 0 habit)
                                    (nth 1 habit)
                                    (nth 2 habit)
                                    (nth 3 habit)
                                    (nth 4 habit)
                                    (nth 5 habit)))
                         (and graph (substring-no-properties graph))
                         graph-props
                         (get-text-property bol 'type)
                         (get-text-property bol 'date)
                         (get-text-property bol 'priority)
                         (get-text-property bol 'effort-minutes))
                   rows))
                (forward-line 1))
              (list label
                    org-habit-scheduled-past-days
                    org-habit-show-all-today
                    org-habit-show-habits-only-for-today
                    (nreverse rows))))))
      (unwind-protect
          (progn
            (with-temp-file file
              (insert "#+CATEGORY: HabitPast\n")
              (insert "* TODO Old stretch :old:\n")
              (insert "SCHEDULED: <2026-05-21 Thu .+1d/4d>\n")
              (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:15\n:END:\n")
              (insert ":LOGBOOK:\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-20 Wed]\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-23 Sat]\n")
              (insert ":END:\n")
              (insert "* TODO Recent stretch :recent:\n")
              (insert "SCHEDULED: <2026-05-25 Mon .+2d/5d>\n")
              (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:20\n:END:\n")
              (insert ":LOGBOOK:\n")
              (insert "- State \"DONE\" from \"TODO\" [2026-05-24 Sun]\n")
              (insert ":END:\n")
              (insert "* TODO Future stretch :future:\n")
              (insert "SCHEDULED: <2026-05-29 Fri .+1d/3d>\n")
              (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:05\n:END:\n")
              (insert "* TODO Plain stale :plain:\n")
              (insert "SCHEDULED: <2026-05-21 Thu>\n"))
            (cl-letf (((symbol-function 'current-time)
                       (lambda () (encode-time 0 0 9 27 5 2026))))
              (org-agenda-list nil "2026-05-27" 1)
              (let ((default (snapshot 'default)))
                (setq org-habit-scheduled-past-days 10)
                (org-agenda-redo)
                (let ((habit-past-window (snapshot 'habit-past-window)))
                  (setq org-habit-scheduled-past-days nil)
                  (with-current-buffer org-agenda-buffer-name
                    (org-habit-toggle-display-in-agenda '(4)))
                  (let ((all-today (snapshot 'all-today)))
                    (setq org-habit-show-habits-only-for-today nil)
                    (setq org-habit-show-all-today nil)
                    (org-agenda-list nil "2026-05-29" 1)
                    (list default
                          habit-past-window
                          all-today
                          (snapshot 'future-with-habits)))))))
        (when (get-buffer org-agenda-buffer-name)
          (kill-buffer org-agenda-buffer-name))
        (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
        (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_habit_graph_properties_text_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable root)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-habit)
  (require 'org-agenda)
  (let* ((file (make-temp-file "org-habit-deep" nil ".org" ""))
         (org-agenda-files (list file))
         (org-habit-show-habits t)
         (org-habit-show-all-today t)
         (org-habit-following-days 7)
         (org-habit-preceding-days 14))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Exercise :health:\n")
            (insert "SCHEDULED: <2026-05-27 Wed .+1d/3d>\n")
            (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:30\n:END:\n")
            (insert ":LOGBOOK:\n")
            (insert "- State \"DONE\" from \"TODO\" [2026-05-26 Mon]\n")
            (insert "- State \"DONE\" from \"TODO\" [2026-05-25 Sun]\n")
            (insert "- State \"DONE\" from \"TODO\" [2026-05-23 Fri]\n")
            (insert ":END:\n"))
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (encode-time 0 0 9 27 5 2026))))
            (org-agenda-list nil "2026-05-27" 1)
            (with-current-buffer org-agenda-buffer-name
              (let ((agenda-text
                     (buffer-substring-no-properties
                      (point-min) (point-max)))
                    (habit-line
                     (let ((result nil))
                       (goto-char (point-min))
                       (when (re-search-forward "Exercise" nil t)
                         (beginning-of-line)
                         (setq result
                               (buffer-substring-no-properties
                                (line-beginning-position)
                                (line-end-position))))
                       result))
                    (habit-props
                     (let ((result nil))
                       (goto-char (point-min))
                       (when (re-search-forward "Exercise" nil t)
                         (beginning-of-line)
                         (let ((pos (point)))
                           (while (< pos (line-end-position))
                             (let ((face (get-text-property pos 'face))
                                   (help (get-text-property pos 'help-echo)))
                               (when (or face help)
                                 (push (list pos face help) result)))
                             (setq pos (1+ pos)))))
                       (nreverse result))))
                (list (replace-regexp-in-string
                       "org-habit-deep[[:alnum:]]+" "org-habit-deep<TMP>"
                       (replace-regexp-in-string
                        (regexp-quote root) "<root>" agenda-text))
                      (replace-regexp-in-string
                       "org-habit-deep[[:alnum:]]+" "org-habit-deep<TMP>"
                       (replace-regexp-in-string
                        (regexp-quote root) "<root>"
                        (or habit-line "")))
                      (mapcar (lambda (p)
                                (list (nth 0 p)
                                      (nth 1 p)
                                      (replace-regexp-in-string
                                       "org-habit-deep[[:alnum:]]+" "org-habit-deep<TMP>"
                                       (replace-regexp-in-string
                                        (regexp-quote root) "<root>"
                                        (or (nth 2 p) "")))))
                              habit-props)))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_habit_done_toggle_graph_urgency_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable root)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-habit)
  (require 'org-agenda)
  (let* ((file (make-temp-file "org-habit-toggle" nil ".org" ""))
         (org-agenda-files (list file))
         (org-habit-show-habits t)
         (org-habit-following-days 7)
         (org-habit-preceding-days 14))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* TODO Exercise :health:\n")
            (insert "SCHEDULED: <2026-05-27 Wed .+1d/3d>\n")
            (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:30\n:END:\n")
            (insert ":LOGBOOK:\n")
            (insert "- State \"DONE\" from \"TODO\" [2026-05-26 Mon]\n")
            (insert "- State \"DONE\" from \"TODO\" [2026-05-25 Sun]\n")
            (insert ":END:\n")
            (insert "* TODO Read :learn:\n")
            (insert "SCHEDULED: <2026-05-27 Wed .+2d/5d>\n")
            (insert ":PROPERTIES:\n:STYLE: habit\n:Effort: 0:20\n:END:\n"))
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (encode-time 0 0 9 27 5 2026))))
            (org-agenda-list nil "2026-05-27" 1)
            (with-current-buffer org-agenda-buffer-name
              (let ((agenda-text
                     (replace-regexp-in-string
                      "org-habit-[a-zA-Z]+[[:alnum:]]*" "habit-<tmp>"
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))
                ;; Check habit presence
                (let ((has-exercise (string-match-p "Exercise" agenda-text))
                      (has-read (string-match-p "Read" agenda-text))
                      ;; Count habit lines
                      (habit-count
                       (let ((c 0) (s 0))
                         (while (string-match "habit-<tmp>" agenda-text s)
                           (setq s (match-end 0) c (1+ c)))
                         c)))
                  (list has-exercise
                        has-read
                        habit-count
                        agenda-text))))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}
