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
