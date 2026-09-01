use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_todo_dependency_blockers_and_noblocking_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-enforce-todo-dependencies t)
          (org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE"))))
      (org-mode)
      (insert "* TODO Parent\n")
      (insert "** TODO Child A\n")
      (insert "** TODO Child B\n")
      (insert "* TODO Ordered\n")
      (insert ":PROPERTIES:\n:ORDERED: t\n:END:\n")
      (insert "** TODO First\n")
      (insert "** TODO Second\n")
      (goto-char (point-min))
      (let ((parent-blocked (org-entry-blocked-p))
            parent-done-attempt)
        (setq parent-done-attempt
              (condition-case err
                  (progn (org-todo "DONE") 'ok)
                (error (cons (car err) (cdr err)))))
        (goto-char (point-min))
        (search-forward "Second")
        (beginning-of-line)
        (let ((second-blocked (org-entry-blocked-p))
              second-done-attempt)
          (setq second-done-attempt
                (condition-case err
                    (progn (org-todo "DONE") 'ok)
                  (error (cons (car err) (cdr err)))))
          (org-entry-put nil "NOBLOCKING" "t")
          (let ((second-unblocked (org-entry-blocked-p))
                (second-done-unblocked
                 (condition-case err
                     (progn (org-todo "DONE") 'ok)
                   (error (cons (car err) (cdr err))))))
            (list parent-blocked
                  parent-done-attempt
                  second-blocked
                  second-done-attempt
                  second-unblocked
                  second-done-unblocked
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_checkbox_dependency_statistics_cookie_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-enforce-todo-checkbox-dependencies t)
          (org-todo-keywords '((sequence "TODO" "|" "DONE"))))
      (org-mode)
      (insert "* TODO Checklist [0/3] [0%]\n")
      (insert "- [X] Done item\n")
      (insert "- [ ] Open item\n")
      (insert "- [-] Partial item\n")
      (insert "  - [X] Nested done\n")
      (insert "  - [ ] Nested open\n")
      (goto-char (point-min))
      (let ((initial-blocked (org-entry-blocked-p))
            (initial-attempt
             (condition-case err
                 (progn (org-todo "DONE") 'ok)
               (error (cons (car err) (cdr err))))))
        (search-forward "Open item")
        (org-ctrl-c-ctrl-c)
        (search-forward "Nested open")
        (org-ctrl-c-ctrl-c)
        (goto-char (point-min))
        (org-update-statistics-cookies t)
        (let ((after-checks (buffer-substring-no-properties
                             (point-min) (point-max)))
              (after-blocked (org-entry-blocked-p))
              (after-attempt
               (condition-case err
                   (progn (org-todo "DONE") 'ok)
                 (error (cons (car err) (cdr err))))))
          (list initial-blocked
                initial-attempt
                after-checks
                after-blocked
                after-attempt
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_todo_state_tag_triggers_statistics_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE" "CANCELED")))
          (org-todo-state-tags-triggers
           '(("WAIT" ("waiting" . t) ("active"))
             ("DONE" ("done" . t) ("waiting") ("active"))
             ("CANCELED" ("canceled" . t) ("waiting") ("active"))))
          (org-log-done nil))
      (org-mode)
      (insert "* Project [0/2]\n")
      (insert "** TODO Alpha :active:\n")
      (insert "** TODO Beta :active:\n")
      (goto-char (point-min))
      (search-forward "Alpha")
      (beginning-of-line)
      (org-todo "WAIT")
      (let ((after-wait (buffer-substring-no-properties
                         (point-min) (point-max)))
            (alpha-tags-wait (org-get-tags nil t)))
        (org-todo "DONE")
        (let ((after-done (buffer-substring-no-properties
                           (point-min) (point-max)))
              (alpha-tags-done (org-get-tags nil t)))
          (goto-char (point-min))
          (search-forward "Beta")
          (beginning-of-line)
          (org-todo "CANCELED")
          (goto-char (point-min))
          (org-update-statistics-cookies t)
          (list after-wait
                alpha-tags-wait
                after-done
                alpha-tags-done
                (org-element-map (org-element-parse-buffer) 'headline
                  (lambda (h)
                    (list (org-element-property :todo-keyword h)
                          (org-element-property :raw-value h)
                          (org-element-property :tags h))))
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_todo_planning_tags_fold_cookie_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Before first headline at position 1 in buffer  *temp*\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'cl-lib)
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO" "NEXT" "WAIT" "|"
                                         "DONE" "CANCELED")))
          (org-todo-state-tags-triggers
           '(("NEXT" ("active" . t) ("waiting"))
             ("WAIT" ("waiting" . t) ("active"))
             ("DONE" ("done" . t) ("active") ("waiting"))
             ("CANCELED" ("canceled" . t) ("active") ("waiting"))))
          (org-log-done 'time)
          (org-log-into-drawer "LOGBOOK")
          (org-log-reschedule 'time)
          (org-log-redeadline 'time)
          (org-use-tag-inheritance t)
          (org-tags-exclude-from-inheritance '("private"))
          (org-auto-align-tags t)
          (org-tags-column 52)
          (org-priority-highest ?A)
          (org-priority-lowest ?D)
          (org-priority-default ?C)
          (org-enforce-todo-dependencies nil))
      (org-mode)
      (insert "#+CATEGORY: Combo\n")
      (insert "* TODO Project [0/3] :project:private:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** TODO Alpha :active:\n")
      (insert "SCHEDULED: <2026-05-20 Wed>\n")
      (insert "Alpha body\n")
      (insert "** WAIT Beta :waiting:\n")
      (insert "DEADLINE: <2026-05-25 Mon -2d>\n")
      (insert "Beta body\n")
      (insert "** TODO Gamma [0/2] :active:\n")
      (insert "- [ ] first\n- [X] second\n")
      (insert "*** TODO Gamma child\n")
      (insert "Child body\n")
      (font-lock-ensure (point-min) (point-max))
      (goto-char (point-min))
      (org-fold-hide-subtree)
      (let ((hidden-before
             (mapcar
              (lambda (needle)
                (save-excursion
                  (goto-char (point-min))
                  (search-forward needle)
                  (list needle (invisible-p (point)))))
              '("Alpha body" "Beta body" "Gamma child"))))
        (cl-letf (((symbol-function 'org-current-time)
                   (lambda (&rest _)
                     (encode-time 0 30 10 27 5 2026))))
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-todo "NEXT")
          (org-priority ?A)
          (org-schedule nil "2026-05-28 09:15")
          (org-deadline nil "2026-06-01 +1w")
          (org-toggle-tag "review" 'on)
          (let ((alpha-state
                 (list (org-get-todo-state)
                       (org-get-priority (thing-at-point 'line t))
                       (org-get-tags nil t)
                       (org-entry-get nil "SCHEDULED")
                       (org-entry-get nil "DEADLINE")
                       (org-entry-get nil "Owner" t))))
            (goto-char (point-min))
            (search-forward "Beta")
            (beginning-of-line)
            (org-todo "DONE")
            (org-toggle-tag "review" 'toggle)
            (let ((beta-state
                   (list (org-get-todo-state)
                         (org-get-tags nil t)
                         (org-entry-get nil "CLOSED")
                         (org-entry-get nil "DEADLINE"))))
              (goto-char (point-min))
              (search-forward "Gamma child")
              (beginning-of-line)
              (org-todo "CANCELED")
              (org-toggle-tag "blocked" 'on)
              (let ((child-state
                     (list (org-get-todo-state)
                           (org-get-tags nil t)
                           (org-entry-get nil "CLOSED")
                           (org-entry-get nil "Owner" t))))
                (goto-char (point-min))
                (search-forward "first")
                (org-ctrl-c-ctrl-c)
                (goto-char (point-min))
                (org-update-statistics-cookies t)
                (org-fold-show-all)
                (font-lock-ensure (point-min) (point-max))
                (let ((hidden-after
                       (mapcar
                        (lambda (needle)
                          (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (list needle (invisible-p (point)))))
                        '("Alpha body" "Beta body" "Gamma child")))
                      (parsed
                       (org-element-map (org-element-parse-buffer)
                           '(headline planning node-property item)
                         (lambda (el)
                           (pcase (org-element-type el)
                             ('headline
                              (list 'headline
                                    (org-element-property :level el)
                                    (org-element-property :todo-keyword el)
                                    (org-element-property :priority el)
                                    (org-element-property :raw-value el)
                                    (org-element-property :tags el)))
                             ('planning
                              (list 'planning
                                    (and (org-element-property :scheduled el)
                                         (org-element-property
                                          :raw-value
                                          (org-element-property
                                           :scheduled el)))
                                    (and (org-element-property :deadline el)
                                         (org-element-property
                                          :raw-value
                                          (org-element-property
                                           :deadline el)))
                                    (and (org-element-property :closed el)
                                         (org-element-property
                                          :raw-value
                                          (org-element-property
                                           :closed el)))))
                             ('node-property
                              (list 'property
                                    (org-element-property :key el)
                                    (org-element-property :value el)))
                             ('item
                              (list 'item
                                    (org-element-property :checkbox el)
                                    (org-element-property :counter el)))))))
                      (faces
                       (mapcar
                        (lambda (needle)
                          (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (list needle
                                  (get-text-property
                                   (match-beginning 0) 'face)
                                  (get-text-property
                                   (match-beginning 0)
                                   'font-lock-fontified))))
                        '("NEXT" "DONE" "CANCELED" "[1/3]" "review"
                          "blocked"))))
                  (list hidden-before
                        alpha-state
                        beta-state
                        child-state
                        hidden-after
                        parsed
                        faces
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_ordered_region_statistics_hook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable events)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE")))
          (org-enforce-todo-dependencies t)
          (org-track-ordered-property-with-tag "sequence")
          (org-use-tag-inheritance t)
          (org-auto-align-tags t)
          (org-tags-column 48)
          (org-hierarchical-todo-statistics t)
          (org-log-done nil)
          (org-after-todo-statistics-hook
           (list (lambda (n-done n-not-done)
                   (push (list (org-get-heading t t t t)
                               n-done
                               n-not-done
                               (org-get-todo-state))
                         events)
                   (let ((org-enforce-todo-dependencies nil)
                         (org-log-done nil))
                     (org-todo (if (= n-not-done 0) "DONE" "TODO"))))))
          events)
      (org-mode)
      (insert "#+TAGS: sequence(s) audit(a) team(t)\n")
      (insert "* TODO Project [0/3] :team:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** TODO First :audit:\n")
      (insert "Body one\n")
      (insert "** TODO Second\n")
      (insert "Body two\n")
      (insert "** TODO Third\n")
      (insert ":PROPERTIES:\n:Owner: Cy\n:END:\n")
      (insert "Body three\n")
      (goto-char (point-min))
      (search-forward "Project")
      (beginning-of-line)
      (org-toggle-ordered-property)
      (let ((after-ordered
             (buffer-substring-no-properties (point-min) (point-max)))
            (project-state
             (list (org-entry-get nil "ORDERED")
                   (org-get-tags nil t)
                   (org-entry-get nil "Owner" t)))
            blocked-summary region-summary final-summary parsed)
        (goto-char (point-min))
        (search-forward "Second")
        (beginning-of-line)
        (setq blocked-summary
              (list (org-entry-blocked-p)
                    org-block-entry-blocking
                    (condition-case err
                        (progn (org-todo "DONE") 'ok)
                      (error (cons (car err) (cdr err))))
                    (org-get-todo-state)
                    (org-get-tags nil t)
                    (org-entry-get nil "Owner" t)))
        (goto-char (point-min))
        (search-forward "First")
        (beginning-of-line)
        (org-todo "DONE")
        (org-update-statistics-cookies t)
        (let ((after-first
               (buffer-substring-no-properties (point-min) (point-max))))
          (goto-char (point-min))
          (search-forward "Second")
          (beginning-of-line)
          (org-todo "DONE")
          (goto-char (point-min))
          (search-forward "Second")
          (beginning-of-line)
          (let ((region-beg (point)))
            (search-forward "Body three")
            (let ((region-end (point))
                  (org-loop-over-headlines-in-active-region t)
                  (transient-mark-mode t)
                  (deactivate-mark nil))
              (goto-char region-beg)
              (set-mark region-end)
              (setq mark-active t)
              (org-todo "WAIT")
              (setq region-summary
                    (list (buffer-substring-no-properties
                           region-beg region-end)
                          (mapcar (lambda (needle)
                                    (save-excursion
                                      (goto-char (point-min))
                                      (search-forward needle)
                                      (beginning-of-line)
                                      (list needle
                                            (org-get-todo-state)
                                            (org-entry-blocked-p)
                                            (org-get-tags nil t)
                                            (org-entry-get nil "Owner" t))))
                                  '("Second" "Third")))))))
          (goto-char (point-min))
          (search-forward "Second")
          (beginning-of-line)
          (org-todo "DONE")
          (goto-char (point-min))
          (search-forward "Third")
          (beginning-of-line)
          (org-todo "DONE")
          (goto-char (point-min))
          (search-forward "Project")
          (beginning-of-line)
          (org-update-statistics-cookies t)
          (org-toggle-ordered-property)
          (setq parsed
                (org-element-map (org-element-parse-buffer)
                    '(headline node-property)
                  (lambda (el)
                    (pcase (org-element-type el)
                      ('headline
                       (list 'headline
                             (org-element-property :level el)
                             (org-element-property :todo-keyword el)
                             (org-element-property :raw-value el)
                             (org-element-property :tags el)))
                      ('node-property
                       (list 'property
                             (org-element-property :key el)
                             (org-element-property :value el)))))))
          (setq final-summary
                (list (org-get-todo-state)
                      (org-entry-get nil "ORDERED")
                      (org-get-tags nil t)
                      (nreverse events)
                      parsed
                      (buffer-substring-no-properties
                       (point-min) (point-max))))
          (list after-ordered
                project-state
                blocked-summary
                after-first
                region-summary
                 final-summary))))))"##,
        expect,
    );
}

#[test]
fn org_todo_state_transition_log_drawer_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"* TODO Alpha\\n** NEXT Sub A\\n** WAIT Sub B\\n* DONE Beta\\n\" ((headline \"Alpha\" \"TODO\") (headline \"Sub A\" \"NEXT\") (headline \"Sub B\" \"WAIT\") (headline \"Beta\" \"DONE\"))) (\"* TODO Alpha\\n** WAIT Sub A\\n** WAIT Sub B\\n* DONE Beta\\n\" ((headline \"Alpha\" \"TODO\") (headline \"Sub A\" \"WAIT\") (headline \"Sub B\" \"WAIT\") (headline \"Beta\" \"DONE\"))) (\"* TODO Alpha\\n** WAIT Sub A\\n** DONE Sub B\\nCLOSED: [2026-06-15 Mon 12:00]\\n* DONE Beta\\n\" ((headline \"Alpha\" \"TODO\") (headline \"Sub A\" \"WAIT\") (headline \"Sub B\" \"DONE\") (planning nil nil) (headline \"Beta\" \"DONE\"))) (\"* CANCELED Alpha\\nCLOSED: [2026-06-15 Mon 12:00]\\n** WAIT Sub A\\n** DONE Sub B\\nCLOSED: [2026-06-15 Mon 12:00]\\n* DONE Beta\\n\" ((headline \"Alpha\" \"CANCELED\") (planning nil nil) (headline \"Sub A\" \"WAIT\") (headline \"Sub B\" \"DONE\") (planning nil nil) (headline \"Beta\" \"DONE\"))) nil)""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-log-done 'time)
          (org-log-into-drawer t)
          (org-todo-keywords '((sequence "TODO" "NEXT" "WAIT" "|" "DONE" "CANCELED"))))
      (org-mode)
      (insert "* TODO Alpha\n")
      (insert "** NEXT Sub A\n")
      (insert "** WAIT Sub B\n")
      (insert "* DONE Beta\n")
      (let ((snap (lambda ()
                    (list (buffer-substring-no-properties
                           (point-min) (point-max))
                          (org-element-map (org-element-parse-buffer)
                              '(headline planning drawer)
                            (lambda (el)
                              (list (org-element-type el)
                                    (org-element-property :raw-value el)
                                    (org-element-property :todo-keyword el))))))))
        ;; Initial state
        (let ((initial (funcall snap)))
          ;; Transition Sub A to WAIT
          (goto-char (point-min))
          (search-forward "Sub A")
          (beginning-of-line)
          (org-todo "WAIT")
          (let ((after-a (funcall snap)))
            ;; Transition Sub B to DONE
            (goto-char (point-min))
            (search-forward "Sub B")
            (beginning-of-line)
            (org-todo "DONE")
            (let ((after-b (funcall snap)))
              ;; Transition Alpha to CANCELED
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-todo "CANCELED")
              (let ((after-alpha (funcall snap)))
                ;; Extract log drawer contents
                (let ((log-entries nil))
                  (goto-char (point-min))
                  (while (re-search-forward ":LOGBOOK:" nil t)
                    (let ((beg (point)))
                      (when (re-search-forward ":END:" nil t)
                        (push (buffer-substring-no-properties beg (point))
                              log-entries))))
                  (list initial
                        after-a
                        after-b
                        after-alpha
                        (nreverse log-entries)))))))))))"##,
        expect,
    );
}
