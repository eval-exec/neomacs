use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_columns_compute_summaries_and_update_properties_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"2:00\" \"5.5\" \"[1/2]\" \"[50%]\" ((\"ITEM\" ((\"CHECK\" \"Check\" nil \"X%\" nil) . \"[50%]\") ((\"DONE\" \"Done\" nil \"X/\" nil) . \"[1/2]\") ((\"POINTS\" \"Points\" nil \"+\" \"%.1f\") . \"5.5\") ((\"EFFORT\" \"Effort\" nil \":\" nil) . \"2:00\")) (\"EFFORT\" ((\"CHECK\" \"Check\" nil \"X%\" nil) . \"[50%]\") ((\"DONE\" \"Done\" nil \"X/\" nil) . \"[1/2]\") ((\"POINTS\" \"Points\" nil \"+\" \"%.1f\") . \"5.5\") ((\"EFFORT\" \"Effort\" nil \":\" nil) . \"2:00\")) (\"POINTS\" ((\"CHECK\" \"Check\" nil \"X%\" nil) . \"[50%]\") ((\"DONE\" \"Done\" nil \"X/\" nil) . \"[1/2]\") ((\"POINTS\" \"Points\" nil \"+\" \"%.1f\") . \"5.5\") ((\"EFFORT\" \"Effort\" nil \":\" nil) . \"2:00\")) (\"DONE\" ((\"CHECK\" \"Check\" nil \"X%\" nil) . \"[50%]\") ((\"DONE\" \"Done\" nil \"X/\" nil) . \"[1/2]\") ((\"POINTS\" \"Points\" nil \"+\" \"%.1f\") . \"5.5\") ((\"EFFORT\" \"Effort\" nil \":\" nil) . \"2:00\")) (\"CHECK\" ((\"CHECK\" \"Check\" nil \"X%\" nil) . \"[50%]\") ((\"DONE\" \"Done\" nil \"X/\" nil) . \"[1/2]\") ((\"POINTS\" \"Points\" nil \"+\" \"%.1f\") . \"5.5\") ((\"EFFORT\" \"Effort\" nil \":\" nil) . \"2:00\"))) \"#+COLUMNS: %24ITEM %Effort{:} %Points{+;%.1f} %Done{X/} %Check{X%}\\n* Project\\n:PROPERTIES:\\n:EFFORT:   2:00\\n:POINTS:   5.5\\n:DONE:     [1/2]\\n:CHECK:    [50%]\\n:END:\\n** TODO Alpha\\n:PROPERTIES:\\n:Effort: 1:15\\n:Points: 2.5\\n:Done: [X]\\n:Check: [X]\\n:END:\\n** TODO Beta\\n:PROPERTIES:\\n:Effort: 0:45\\n:Points: 3.0\\n:Done: [ ]\\n:Check: [ ]\\n:END:\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "#+COLUMNS: %24ITEM %Effort{:} %Points{+;%.1f} %Done{X/} %Check{X%}\n")
    (insert "* Project\n")
    (insert ":PROPERTIES:\n:Effort: 0:00\n:Points: 0\n:Done: [ ]\n:Check: [ ]\n:END:\n")
    (insert "** TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 1:15\n:Points: 2.5\n:Done: [X]\n:Check: [X]\n:END:\n")
    (insert "** TODO Beta\n")
    (insert ":PROPERTIES:\n:Effort: 0:45\n:Points: 3.0\n:Done: [ ]\n:Check: [ ]\n:END:\n")
    (goto-char (point-min))
    (search-forward "Project")
    (beginning-of-line)
    (org-columns nil)
    (org-columns-quit)
    (list
     (org-entry-get nil "Effort")
     (org-entry-get nil "Points")
     (org-entry-get nil "Done")
     (org-entry-get nil "Check")
     (mapcar (lambda (spec)
               (cons (car spec)
                     (get-text-property (point) 'org-summaries)))
             org-columns-current-fmt-compiled)
     (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn org_columns_capture_view_filter_skip_indent_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Task\" \"State\" \"Effort\" \"Owner\") hline (2 \"Visible\" \"TODO\" \"0:10\" \"Bea\") (2 \"Empty\" \"TODO\" \"\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "* Root :keep:\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
    (insert "** TODO Visible :work:\n")
    (insert ":PROPERTIES:\n:Effort: 0:30\n:Owner: Bea\n:END:\n")
    (insert "*** TODO Too deep :work:\n")
    (insert ":PROPERTIES:\n:Effort: 0:10\n:END:\n")
    (insert "** TODO Empty :work:\n")
    (insert "** TODO Hidden :skip:work:\n")
    (insert ":PROPERTIES:\n:Effort: 0:20\n:END:\n")
    (goto-char (point-min))
    (org-columns--capture-view
     2 "+work" t '("skip")
     "%20ITEM(Task) %TODO(State) %Effort{:} %Owner"
     nil)))"##,
        expect,
    );
}

#[test]
fn org_duration_custom_units_columns_time_summary_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((630.0 1320.0 105.0) (\"1d 2h 30min\" \"2d 6h 0min\" \"1h 45min\") nil nil \"#+COLUMNS: %18ITEM %Effort{:} %Age{@mean}\\n* Project\\n** A\\n:PROPERTIES:\\n:Effort: 1d 2h 30min\\n:Age: 2d\\n:END:\\n** B\\n:PROPERTIES:\\n:Effort: 0d 1h 45min\\n:Age: 4h\\n:END:\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-duration)
  (require 'org-colview)
  (with-temp-buffer
    (let ((org-duration-units '(("min" . 1) ("h" . 60) ("d" . 480)
                                ("sprint" . 1200)))
          (org-duration-format '(("d" . nil) ("h" . t) ("min" . t))))
      (org-duration-set-regexps)
      (org-mode)
      (insert "#+COLUMNS: %18ITEM %Effort{:} %Age{@mean}\n")
      (insert "* Project\n")
      (insert "** A\n:PROPERTIES:\n:Effort: 1d 2h 30min\n:Age: 2d\n:END:\n")
      (insert "** B\n:PROPERTIES:\n:Effort: 0d 1h 45min\n:Age: 4h\n:END:\n")
      (goto-char (point-min))
      (search-forward "Project")
      (beginning-of-line)
      (let ((org-columns--time
             (float-time (encode-time 0 0 12 27 5 2026))))
        (org-columns nil)
        (org-columns-quit))
      (list
       (mapcar #'org-duration-to-minutes
               '("1d 2h 30min" "1sprint 2h" "0d 1h 45min"))
       (mapcar #'org-duration-from-minutes '(630 1320 105))
       (org-entry-get nil "Effort")
       (org-entry-get nil "Age")
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_columns_allowed_value_cycle_and_update_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Allowed values for this property have not been defined\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "#+COLUMNS: %20ITEM %TODO %Status %Priority %Effort\n")
    (insert "#+PROPERTY: Status_ALL Todo Doing Review Done\n")
    (insert "* Project\n")
    (insert "** TODO Task\n")
    (insert ":PROPERTIES:\n:Status: Todo\n:Priority: B\n:Effort: 0:30\n:END:\n")
    (goto-char (point-min))
    (search-forward "Task")
    (beginning-of-line)
    (org-columns nil)
    (let ((snapshots nil))
      (cl-labels
          ((capture
            (label)
            (push (list label
                        (org-entry-get nil "Status")
                        (org-entry-get nil "TODO")
                        (org-entry-get nil "PRIORITY")
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))
                  snapshots)))
        (search-forward "Todo")
        (org-columns-next-allowed-value)
        (capture 'status-next)
        (org-columns-next-allowed-value nil 4)
        (capture 'status-nth)
        (goto-char (line-beginning-position))
        (search-forward "TODO")
        (let ((org-todo-keywords '((sequence "TODO" "NEXT" "|" "DONE"))))
          (org-columns-next-allowed-value))
        (capture 'todo-next)
        (goto-char (line-beginning-position))
        (search-forward "B")
        (org-columns-next-allowed-value)
        (capture 'priority-next)
        (org-columns-quit)
        (list (nreverse snapshots)
              (org-entry-properties nil 'standard)
              (buffer-substring-no-properties (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_columns_format_store_insert_delete_move_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "* Root\n")
    (insert "#+COLUMNS: %10ITEM %Owner %Effort{:}\n")
    (insert "** A\n:PROPERTIES:\n:Owner: Ada\n:Effort: 0:30\n:END:\n")
    (insert "** B\n:PROPERTIES:\n:Owner: Bea\n:Effort: 1:00\n:END:\n")
    (goto-char (point-min))
    (search-forward "A")
    (beginning-of-line)
    (org-columns nil)
    (let ((fmt-before org-columns-current-fmt)
          (compiled-before org-columns-current-fmt-compiled))
      (org-columns-store-format)
      (org-columns-new "Status" :title "State" :width 8)
      (org-columns-move-left)
      (org-columns-widen 2)
      (org-columns-narrow 1)
      (let ((fmt-edited org-columns-current-fmt)
            (compiled-edited org-columns-current-fmt-compiled)
            (line-with-overlays
             (buffer-substring-no-properties
              (line-beginning-position) (line-end-position))))
        (org-columns-delete)
        (org-columns-quit)
        (list fmt-before
              compiled-before
              fmt-edited
              compiled-edited
              line-with-overlays
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_duration_parse_format_summary_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"\" nil 0.0 0.0) (7 nil 7.0 nil) (\"1:02\" 0 62.0 62.0) (\"2:03:30\" 0 123.5 123.5) (\"1d 2h 15min\" 0 585.0 1575.0) (\"1w 2d 3:30\" 0 3360.0 3360.0) (\"4pt 0:45\" 0 105.0 105.0) (\"bad unit\" nil (error \"Invalid duration format: \\\"bad unit\\\"\") canonical-error)) (\"0:00\" \"1:15\" \"1d 0:00\" \"1w 1d 3:45\" \"-1:15\" \"48:45\" \"48:45:00\" \"6d 3h 45min\" \"2d 0h 45min\" (error \"Unknown unit: nil\")) (h:mm h:mm:ss nil nil) \"4:15\" \"0:30\" \"2:00\" \"1:03\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-duration)
  (require 'org-colview)
  (let ((org-duration-units '(("min" . 1) ("h" . 60) ("d" . 450)
                              ("w" . 2250) ("pt" . 15)))
        (org-duration-format '(("w" . nil) ("d" . nil)
                               (special . h:mm)))
        (samples '("" 7 "1:02" "2:03:30" "1d 2h 15min"
                   "1w 2d 3:30" "4pt 0:45" "bad unit")))
    (org-duration-set-regexps)
    (let ((converted
           (mapcar (lambda (sample)
                     (list sample
                           (org-duration-p sample)
                           (condition-case err
                               (org-duration-to-minutes sample)
                             (error (cons (car err) (cdr err))))
                           (and (stringp sample)
                                (condition-case nil
                                    (org-duration-to-minutes sample t)
                                  (error 'canonical-error)))))
                   samples))
          (formatted
           (mapcar (lambda (entry)
                     (condition-case err
                         (apply #'org-duration-from-minutes entry)
                       (error (cons (car err) (cdr err)))))
                   '((0) (75) (450) (2925) (-75)
                     (2925 h:mm nil)
                     (2925 h:mm:ss nil)
                     (2925 (("d" . nil) ("h" . t) ("min" . t)) nil)
                     (2925 (("d" . nil) ("h" . t) ("min" . t)) t)
                     (75 ((special . 2)) nil))))
          (hmm (mapcar #'org-duration-h:mm-only-p
                       '(("1:02" "2:03")
                         ("1:02:03" "2:03")
                         ("1h" "2:03")
                         ("1d 2:03")))))
      (list converted
            formatted
            hmm
            (org-columns--summary-sum-times
             '("1:00" "2h" "3pt" "0:30") nil)
            (org-columns--summary-min-time
             '("1:00" "2h" "3pt" "0:30") nil)
            (org-columns--summary-max-time
             '("1:00" "2h" "3pt" "0:30") nil)
            (org-columns--summary-mean-time
             '("1:00" "2h" "3pt" "0:30") nil)))))"##,
        expect,
    );
}

#[test]
fn org_columns_property_inheritance_compute_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (require 'org-duration)
  (with-temp-buffer
    (let ((org-use-property-inheritance '("Owner" "Milestone"))
          (org-duration-format '(("h" . t) ("min" . t)))
          (changes nil))
      (add-hook 'org-property-changed-functions
                (lambda (key value) (push (list key value) changes))
                nil t)
      (org-duration-set-regexps)
      (org-mode)
      (insert "#+COLUMNS: %22ITEM %Owner %Milestone %Status %Effort{:} %Score{+;%.1f} %Done{X/}\n")
      (insert "#+PROPERTY: Status_ALL Todo Doing Review Done\n")
      (insert "* Project\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Milestone: M1\n:Effort: 0:00\n:Score: 0\n:Done: [ ]\n:END:\n")
      (insert "** TODO Alpha\n")
      (insert ":PROPERTIES:\n:Status: Todo\n:Effort: 1:15\n:Score: 2.5\n:Done: [X]\n:END:\n")
      (insert "** TODO Beta\n")
      (insert ":PROPERTIES:\n:Owner: Bea\n:Status: Doing\n:Effort: 0:45\n:Score: 3.0\n:Done: [ ]\n:END:\n")
      (insert "*** TODO Beta child\n")
      (insert ":PROPERTIES:\n:Status: Review\n:Effort: 0:30\n:Score: 1.5\n:Done: [X]\n:END:\n")
      (goto-char (point-min))
      (search-forward "Beta child")
      (beginning-of-line)
      (let ((inherited-before
             (list (org-entry-get nil "Owner" 'inherit)
                   (org-entry-get nil "Milestone" 'inherit)
                   (org-entry-get-with-inheritance "Status")))
            (allowed-status
             (org-property-get-allowed-values nil "Status" 'table)))
        (org-entry-put-multivalued-property nil "Tags" "alpha beta" "gamma")
        (org-entry-add-to-multivalued-property nil "Tags" "delta value")
        (org-entry-remove-from-multivalued-property nil "Tags" "gamma")
        (search-forward ":Status:")
        (org-property-next-allowed-value)
        (goto-char (point-min))
        (search-forward "Project")
        (beginning-of-line)
        (org-columns nil)
        (let ((overlay-summary
               (mapcar
                (lambda (ov)
                  (list (overlay-get ov 'before-string)
                        (overlay-get ov 'after-string)
                        (overlay-get ov 'face)
                        (overlay-get ov 'org-columns-key)
                        (overlay-get ov 'org-columns-value)))
                (overlays-in (line-beginning-position)
                             (line-end-position))))
              (compiled org-columns-current-fmt-compiled)
              (content (org-columns-content)))
          (org-columns-compute-all)
          (let ((after-compute
                 (list (org-entry-get nil "Effort")
                       (org-entry-get nil "Score")
                       (org-entry-get nil "Done")))
                (line-after-compute
                 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))))
            (org-columns-quit)
            (let ((tree (org-element-parse-buffer)))
              (list inherited-before
                    allowed-status
                    (org-entry-get-with-inheritance "Tags")
                    (org-entry-get-multivalued-property nil "Tags")
                    (nreverse changes)
                    compiled
                    overlay-summary
                    content
                    after-compute
                    line-after-compute
                    (org-element-map tree 'node-property
                      (lambda (node)
                        (list (org-element-property :key node)
                              (org-element-property :value node))))
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_columns_dblock_property_summary_refresh_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (require 'org-duration)
  (with-temp-buffer
    (let ((org-duration-format '(("h" . t) ("min" . t)))
          (org-use-property-inheritance '("Owner")))
      (org-duration-set-regexps)
      (org-mode)
      (insert "#+COLUMNS: %22ITEM(Task) %TODO(State) %Owner %Effort{:} %Score{+;%.1f} %Done{X%}\n")
      (insert "* Project\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Effort: 0:00\n:Score: 0\n:Done: [ ]\n:END:\n")
      (insert "** TODO Alpha :work:\n")
      (insert ":PROPERTIES:\n:Effort: 1:10\n:Score: 2.5\n:Done: [X]\n:END:\n")
      (insert "** WAIT Beta :work:\n")
      (insert ":PROPERTIES:\n:Owner: Bea\n:Effort: 0:50\n:Score: 1.5\n:Done: [ ]\n:END:\n")
      (insert "** TODO Skip :skip:\n")
      (insert ":PROPERTIES:\n:Effort: 9:00\n:Score: 99\n:Done: [X]\n:END:\n")
      (insert "#+BEGIN: columnview :hlines 1 :id local :match \"+work\" :skip-empty-rows nil :exclude-tags \"skip\"\n")
      (insert "#+END:\n")
      (goto-char (point-min))
      (search-forward "#+BEGIN:")
      (org-update-dblock)
      (let ((first-dblock
             (buffer-substring-no-properties
              (save-excursion
                (goto-char (point-min))
                (search-forward "#+BEGIN:")
                (line-beginning-position))
              (save-excursion
                (goto-char (point-min))
                (search-forward "#+END:")
                (line-end-position)))))
        (goto-char (point-min))
        (search-forward "Beta")
        (beginning-of-line)
        (org-entry-put nil "Effort" "1:20")
        (org-entry-put nil "Score" "3.5")
        (org-entry-put nil "Done" "[X]")
        (goto-char (point-min))
        (search-forward "Project")
        (beginning-of-line)
        (org-columns nil)
        (let ((project-after-compute
               (progn
                 (org-columns-compute-all)
                 (list (org-entry-get nil "Effort")
                       (org-entry-get nil "Score")
                       (org-entry-get nil "Done"))))
              (column-line
               (buffer-substring-no-properties
                (line-beginning-position) (line-end-position)))
              (overlays
               (mapcar (lambda (ov)
                         (list (overlay-start ov)
                               (overlay-end ov)
                               (overlay-get ov 'org-columns-key)
                               (overlay-get ov 'org-columns-value)
                               (overlay-get ov 'before-string)))
                       (overlays-in (line-beginning-position)
                                    (line-end-position)))))
          (org-columns-quit)
          (goto-char (point-min))
          (search-forward "#+BEGIN:")
          (org-update-dblock)
          (let ((second-dblock
                 (buffer-substring-no-properties
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "#+BEGIN:")
                    (line-beginning-position))
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "#+END:")
                    (line-end-position)))))
            (list first-dblock
                  project-after-compute
                  column-line
                  overlays
                  second-dblock
                  (org-columns--capture-view
                   3 "+work" nil '("skip")
                   "%22ITEM(Task) %TODO(State) %Owner %Effort{:} %Score{+;%.1f} %Done{X%}"
                   nil)
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_columns_overlay_row_move_recompute_allowed_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (require 'org-duration)
  (with-temp-buffer
    (let ((org-duration-format '(("h" . t) ("min" . t)))
          (org-use-property-inheritance '("Owner" "Area"))
          states)
      (org-duration-set-regexps)
      (org-mode)
      (insert "#+COLUMNS: %22ITEM(Task) %TODO(State) %Owner %Area %Status %Effort{:} %Score{+;%.1f} %Done{X/}\n")
      (insert "#+PROPERTY: Status_ALL Todo Doing Review Done\n")
      (insert "* Project\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Area: Core\n:Effort: 0:00\n:Score: 0\n:Done: [ ]\n:Status: Todo\n:END:\n")
      (insert "** TODO Alpha\n")
      (insert ":PROPERTIES:\n:Effort: 1:10\n:Score: 2.5\n:Done: [X]\n:Status: Todo\n:END:\n")
      (insert "** WAIT Beta\n")
      (insert ":PROPERTIES:\n:Owner: Bea\n:Effort: 0:50\n:Score: 1.5\n:Done: [ ]\n:Status: Doing\n:END:\n")
      (insert "*** TODO Beta child\n")
      (insert ":PROPERTIES:\n:Effort: 0:20\n:Score: 0.5\n:Done: [X]\n:Status: Review\n:END:\n")
      (let ((snapshot
             (lambda (label)
               (list label
                     (mapcar
                      (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (beginning-of-line)
                          (list needle
                                (org-outline-level)
                                (org-get-todo-state)
                                (org-entry-get nil "Owner" 'inherit)
                                (org-entry-get nil "Area" 'inherit)
                                (org-entry-get nil "Status")
                                (org-entry-get nil "Effort")
                                (org-entry-get nil "Score")
                                (org-entry-get nil "Done")
                                (line-number-at-pos))))
                      '("Project" "Alpha" "Beta" "Beta child"))
                     (buffer-substring-no-properties
                      (point-min) (point-max)))))))
        (push (funcall snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "Beta child")
        (beginning-of-line)
        (org-columns nil)
        (let ((overlay-before
               (mapcar (lambda (ov)
                         (list (overlay-start ov)
                               (overlay-end ov)
                               (overlay-get ov 'org-columns-key)
                               (overlay-get ov 'org-columns-value)
                               (overlay-get ov 'before-string)
                               (overlay-get ov 'after-string)))
                       (overlays-in (line-beginning-position)
                                    (line-end-position))))
              (content-before (org-columns-content))
              (compiled-before org-columns-current-fmt-compiled))
          (search-forward "Review")
          (org-columns-next-allowed-value)
          (goto-char (line-beginning-position))
          (search-forward "TODO")
          (let ((org-todo-keywords '((sequence "TODO" "WAIT" "NEXT" "|"
                                               "DONE"))))
            (org-columns-next-allowed-value))
          (goto-char (line-beginning-position))
          (search-forward "0:20")
          (cl-letf (((symbol-function 'read-string)
                     (lambda (&rest _) "0:45")))
            (org-columns-edit-value "Effort"))
          (goto-char (line-beginning-position))
          (search-forward "0.5")
          (cl-letf (((symbol-function 'read-string)
                     (lambda (&rest _) "4.0")))
            (org-columns-edit-value "Score"))
          (let ((line-after-edits
                 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position)))
                (props-after-edits
                 (list (org-get-todo-state)
                       (org-entry-get nil "Status")
                       (org-entry-get nil "Effort")
                       (org-entry-get nil "Score"))))
            (org-columns-move-row-up)
            (org-columns-redo)
            (let ((line-after-redo
                   (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position)))
                  (overlay-after-redo
                   (mapcar (lambda (ov)
                             (list (overlay-get ov 'org-columns-key)
                                   (overlay-get ov 'org-columns-value)
                                   (overlay-get ov 'before-string)))
                           (overlays-in (line-beginning-position)
                                        (line-end-position)))))
              (goto-char (point-min))
              (search-forward "Project")
              (beginning-of-line)
              (org-columns-compute-all)
              (let ((project-computed
                     (list (org-entry-get nil "Effort")
                           (org-entry-get nil "Score")
                           (org-entry-get nil "Done")))
                    (content-after (org-columns-content)))
                (org-columns-quit)
                (push (funcall snapshot 'after-columns) states)
                (list compiled-before
                      overlay-before
                      content-before
                      line-after-edits
                      props-after-edits
                      line-after-redo
                      overlay-after-redo
                      project-computed
                      content-after
                       (nreverse states))))))))"##,
        expect,
    );
}

#[test]
fn org_columns_compute_all_dblock_insert_refresh_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %Effort{:} %Score{:sum}\n")
    (insert "* TODO Project\n")
    (insert ":PROPERTIES:\n:Effort: 0:00\n:Score: 0\n:END:\n")
    (insert "** TODO Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 1:30\n:Score: 5\n:END:\n")
    (insert "** WAIT Beta\n")
    (insert ":PROPERTIES:\n:Effort: 0:45\n:Score: 3\n:END:\n")
    (insert "*** TODO Beta child\n")
    (insert ":PROPERTIES:\n:Effort: 0:30\n:Score: 2\n:END:\n")
    (insert "* DONE Gamma\n")
    (insert ":PROPERTIES:\n:Effort: 2:00\n:Score: 8\n:END:\n")
    (insert "#+BEGIN: columnview :hlines 1 :id local :indent t\n")
    (insert "#+END:\n")
    ;; Column view
    (goto-char (point-min))
    (org-columns)
    (let ((content (org-columns-content)))
      ;; Compute all
      (goto-char (point-min))
      (search-forward "Project")
      (beginning-of-line)
      (org-columns-compute-all)
      (let ((project-effort (org-entry-get nil "Effort"))
            (project-score (org-entry-get nil "Score"))
            (content-after (org-columns-content)))
        (org-columns-quit)
        ;; Update dblock
        (goto-char (point-min))
        (search-forward "columnview")
        (org-dblock-update)
        (let ((dblock-content
               (buffer-substring-no-properties (point-min) (point-max)))
              (table-lisp
               (progn
                 (goto-char (point-min))
                 (search-forward "|")
                 (org-table-to-lisp))))
          (list content
                project-effort
                project-score
                content-after
                dblock-content
                table-lisp)))))"##,
        expect,
    );
}
