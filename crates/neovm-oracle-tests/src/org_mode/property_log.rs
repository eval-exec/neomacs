use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_tags_multivalue_property_delete_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"new\") ((\"CATEGORY\" . \"???\") (\"MULTI\" . \"x y z\") (\"A\" . \"updated\")) \"* TODO Task                                                             :new:\\n:PROPERTIES:\\n:A:        updated\\n:Multi:    x y z\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task :old:\n")
    (insert ":PROPERTIES:\n:A: 1\n:B: two\n:END:\n")
    (goto-char (point-min))
    (org-toggle-tag "new" 'on)
    (org-toggle-tag "old" 'off)
    (org-entry-put nil "A" "updated")
    (org-entry-put-multivalued-property nil "Multi" "x" "y" "z")
    (org-entry-delete nil "B")
    (list (org-get-tags)
          (org-entry-properties nil 'standard)
          (buffer-substring-no-properties (point-min) (point-max)))))"#,
        expect,
    );
}

#[test]
fn org_archive_tag_toggle_parse_roundtrip_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* TODO Active\\n** DONE Child                                                       :ARCHIVE:\\nBody\\n** TODO Keep\\n\" \"* TODO Active\\n** DONE Child\\nBody\\n** TODO Keep\\n\" ((\"Active\" nil) (\"Child\" nil) (\"Keep\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-archive)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Active\n** DONE Child\nBody\n** TODO Keep\n")
    (goto-char (point-min))
    (search-forward "Child")
    (beginning-of-line)
    (org-toggle-archive-tag)
    (let ((after-archive
           (buffer-substring-no-properties (point-min) (point-max))))
      (org-toggle-archive-tag)
      (list after-archive
            (buffer-substring-no-properties (point-min) (point-max))
            (org-element-map (org-element-parse-buffer) 'headline
              (lambda (headline)
                (list (org-element-property :raw-value headline)
                      (org-element-property :tags headline))))))))"#,
        expect,
    );
}

#[test]
fn org_done_log_drawer_timestamp_normalized_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (54 \"* DONE Task\\nCLOSED: [stamp]\\n:LOGBOOK:\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task\n")
    (goto-char (point-min))
    (let ((org-log-into-drawer t)
          (org-log-note-clock-out nil)
          (org-log-done 'time))
      (org-todo "DONE")
      (list (org-log-beginning t)
            (replace-regexp-in-string
             "CLOSED: \\[.*\\]"
             "CLOSED: [stamp]"
             (buffer-substring-no-properties (point-min) (point-max)))))))"#,
        expect,
    );
}

#[test]
fn org_property_inheritance_allowed_cycle_delete_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-property-inheritance '("Owner" "Milestone"))
          (changes nil))
      (add-hook 'org-property-changed-functions
                (lambda (key value) (push (list key value) changes))
                nil t)
      (org-mode)
      (insert "#+PROPERTY: Status_ALL Todo Doing Done :ETC\n")
      (insert "* Project\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Milestone: M1\n:END:\n")
      (insert "** Task\n")
      (insert ":PROPERTIES:\n:Status: Todo\n:Owner: Bea\n:Other: keep\n:END:\n")
      (goto-char (point-min))
      (search-forward "Task")
      (beginning-of-line)
      (let ((inherited (list (org-entry-get nil "Owner" 'inherit)
                             (org-entry-get nil "Milestone" 'inherit)
                             (org-entry-get-with-inheritance "Milestone")))
            (allowed (org-property-get-allowed-values nil "Status" 'table)))
        (search-forward ":Status:")
        (org-property-next-allowed-value)
        (org-property-next-allowed-value)
        (org-property-previous-allowed-value)
        (goto-char (point-min))
        (search-forward "Task")
        (beginning-of-line)
        (org-entry-add-to-multivalued-property nil "Multi" "x")
        (org-entry-add-to-multivalued-property nil "Multi" "y")
        (org-entry-remove-from-multivalued-property nil "Multi" "x")
        (org-entry-delete nil "Other")
        (list inherited
              allowed
              (org-entry-get nil "Status")
              (org-entry-get-multivalued-property nil "Multi")
              (org-entry-member-in-multivalued-property nil "Multi" "y")
              (nreverse changes)
              (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_property_values_global_delete_roundtrip_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"Ada\" \"Bea\") (\"0:15\" \"0:30\" \"1:00\") (\"Ada\" \"Cy\") nil ((\"CATEGORY\" . \"???\") (\"OWNER\" . \"Cy\")) \"#+PROPERTY: Owner_ALL Ada Bea Cy\\n* A\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n** A1\\n:PROPERTIES:\\n:Owner:    Cy\\n:END:\\n* B\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+PROPERTY: Owner_ALL Ada Bea Cy\n")
    (insert "* A\n:PROPERTIES:\n:Owner: Ada\n:Effort: 0:30\n:END:\n")
    (insert "** A1\n:PROPERTIES:\n:Owner: Bea\n:Effort: 0:15\n:END:\n")
    (insert "* B\n:PROPERTIES:\n:Owner: Ada\n:Effort: 1:00\n:END:\n")
    (goto-char (point-min))
    (let ((owners-before (sort (copy-sequence (org-property-values "Owner"))
                               #'string<))
          (efforts-before (sort (copy-sequence (org-property-values "Effort"))
                                #'string<)))
      (org-delete-property-globally "Effort")
      (goto-char (point-min))
      (search-forward "A1")
      (beginning-of-line)
      (org-entry-put nil "Owner" "Cy")
      (list owners-before
            efforts-before
            (sort (copy-sequence (org-property-values "Owner")) #'string<)
            (org-property-values "Effort")
            (org-entry-properties nil 'standard)
            (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_property_set_delete_allowed_values_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+PROPERTY: Phase_ALL Plan Build Ship\n")
    (insert "* Project\n")
    (insert "** Task\n")
    (goto-char (point-min))
    (search-forward "Task")
    (beginning-of-line)
    (let ((org-last-set-property "Phase")
          (org-last-set-property-value "Build"))
      (org-set-property "Phase" "Plan")
      (org-set-property "Owner" "Ada")
      (let ((after-set (buffer-substring-no-properties
                        (point-min) (point-max)))
            (allowed (org-property-get-allowed-values nil "Phase" 'table)))
        (org-delete-property "Owner")
        (search-forward ":Phase:")
        (org-property-next-allowed-value)
        (org-property-next-allowed-value)
        (org-property-next-allowed-value)
        (list after-set
              allowed
              (org-entry-properties nil 'standard)
              org-last-set-property
              org-last-set-property-value
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_todo_done_note_log_drawer_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task\n")
    (goto-char (point-min))
    (let ((org-log-into-drawer "LOGBOOK")
          (org-log-done 'note)
          (org-log-note-how 'time)
          (org-log-note-clock-out nil)
          (org-log-note-headings
           '((done . "State %-12s from %-12S %t")
             (note . "Note taken on %t"))))
      (cl-letf (((symbol-function 'read-string)
                 (lambda (&rest _) "Finished carefully"))
                ((symbol-function 'read-char-exclusive)
                 (lambda (&rest _) ?\C-c)))
        (org-todo "DONE")
        (when (and (boundp 'org-log-note-marker)
                   org-log-note-marker)
          (with-current-buffer (marker-buffer org-log-note-marker)
            (goto-char org-log-note-marker)
            (insert "Finished carefully")
            (org-add-log-note))))
      (list (org-entry-get nil "CLOSED")
            (replace-regexp-in-string
             "\\[[0-9][^]\n]+\\]"
             "[stamp]"
             (buffer-substring-no-properties
              (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_log_repeat_reschedule_redeadline_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Repeater\n")
    (insert "SCHEDULED: <2026-05-27 Wed +1w>\n")
    (insert "DEADLINE: <2026-05-28 Thu +2w>\n")
    (goto-char (point-min))
    (let ((org-log-into-drawer t)
          (org-log-reschedule 'time)
          (org-log-redeadline 'time)
          (org-log-repeat 'time)
          (org-log-done nil))
      (org-schedule nil "2026-06-03")
      (org-deadline nil "2026-06-11")
      (org-todo "DONE")
      (list (org-entry-get nil "LAST_REPEAT")
            (replace-regexp-in-string
             "\\[[0-9][^]\n]+\\]"
             "[stamp]"
             (buffer-substring-no-properties
              (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_property_clock_drawer_fold_element_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (require 'org-element)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-use-property-inheritance '("Client" "Sprint"))
          (org-clock-into-drawer "LOGBOOK")
          (org-log-into-drawer "LOGBOOK")
          (org-clock-history-length 8)
          (org-clock-persist nil)
          (org-clock-out-remove-zero-time-clocks t))
      (org-mode)
      (insert "#+PROPERTY: Status_ALL Todo Doing Blocked Done\n")
      (insert "* Project :work:\n")
      (insert ":PROPERTIES:\n:Client: Acme\n:Sprint: S1\n:END:\n")
      (insert "** TODO Alpha :billable:\n")
      (insert ":PROPERTIES:\n:Status: Todo\n:Owner: Ada\n:END:\n")
      (insert "Alpha body\n")
      (insert "** TODO Beta :internal:\n")
      (insert ":PROPERTIES:\n:Owner: Bea\n:END:\n")
      (insert "Beta body\n")
      (insert "*** WAIT Beta child :blocked:\n")
      (insert ":PROPERTIES:\n:Status: Blocked\n:Owner: Cy\n:END:\n")
      (insert "Child body\n")
      (insert "* Tail\nTail body\n")
      (let ((snapshot
             (lambda (label)
               (list label
                     (mapcar
                      (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle
                                (line-number-at-pos)
                                (invisible-p (point))
                                (org-element-type
                                 (org-element-at-point)))))
                      '("Project" ":Client:" "Alpha" ":Status:" "Alpha body"
                        "CLOCK:" "Beta" "Beta child" "Child body" "Tail"))
                     (org-element-map (org-element-parse-buffer)
                         '(headline drawer property-drawer clock planning)
                       (lambda (el)
                         (list (org-element-type el)
                               (org-element-property :begin el)
                               (org-element-property :end el)
                               (org-element-property :raw-value el)
                               (org-element-property :todo-keyword el)
                               (org-element-property :tags el))))
                     (save-excursion
                       (goto-char (point-min))
                       (let (out)
                         (while (re-search-forward "^\\*+ " nil t)
                           (push (list (org-get-heading t t t t)
                                       (org-entry-get nil "Client" 'inherit)
                                       (org-entry-get nil "Sprint" 'inherit)
                                       (org-entry-get nil "Status")
                                       (org-entry-get-multivalued-property
                                        nil "Multi")
                                       (get-text-property
                                        (line-beginning-position)
                                        :probe-minutes))
                                 out))
                         (nreverse out)))
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))
        (let (states)
          (push (funcall snapshot 'initial) states)
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-entry-put nil "Status" "Doing")
          (org-entry-add-to-multivalued-property nil "Multi" "review")
          (org-entry-add-to-multivalued-property nil "Multi" "api")
          (org-clock-in nil (encode-time 0 0 9 27 5 2026))
          (org-clock-out nil t (encode-time 0 45 10 27 5 2026))
          (push (funcall snapshot 'after-alpha-clock) states)
          (goto-char (point-min))
          (search-forward "Beta child")
          (beginning-of-line)
          (org-entry-put nil "Sprint" "S2")
          (org-entry-remove-from-multivalued-property nil "Multi" "api")
          (org-clock-in nil (encode-time 0 15 11 27 5 2026))
          (org-clock-out nil t (encode-time 0 0 12 27 5 2026))
          (push (funcall snapshot 'after-child-clock) states)
          (goto-char (point-min))
          (org-clock-sum "2026-05-27" "2026-05-28" nil :probe-minutes)
          (push (funcall snapshot 'after-clock-sum) states)
          (org-fold-hide-drawer-all)
          (push (funcall snapshot 'drawers-hidden) states)
          (goto-char (point-min))
          (search-forward "CLOCK:")
          (org-fold-show-context 'default)
          (push (funcall snapshot 'clock-context) states)
          (org-fold-show-all)
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-entry-delete nil "Owner")
          (org-property-next-allowed-value)
          (push (funcall snapshot 'after-property-cycle) states)
          (list (nreverse states)
                (sort (copy-sequence (org-property-values "Owner"))
                      #'string<)
                (org-clock-sum-current-item "2026-05-27")
                (mapcar (lambda (m)
                          (and (markerp m)
                               (marker-buffer m)
                               (with-current-buffer (marker-buffer m)
                                 (save-excursion
                                   (goto-char m)
                                   (org-get-heading t t t t)))))
                        org-clock-history)
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_property_space_multivalue_cleanup_parse_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-property-inheritance t)
          (changes nil))
      (add-hook 'org-property-changed-functions
                (lambda (key value) (push (list key value) changes))
                nil t)
      (org-mode)
      (insert "* Parent\n")
      (insert ":PROPERTIES:\n:Owner: Ada Lovelace\n:Multi: old value\n:END:\n")
      (insert "** Child\n")
      (insert ":PROPERTIES:\n:Local: keep\n:END:\n")
      (goto-char (point-min))
      (search-forward "Parent")
      (beginning-of-line)
      (org-entry-put-multivalued-property
       nil "Multi" "alpha beta" "gamma" "delta value")
      (let ((parent-before (org-entry-properties nil 'standard))
            (protected (mapcar #'org-entry-protect-space
                               '("alpha beta" "gamma" "delta value")))
            (restored (mapcar #'org-entry-restore-space
                              '("alpha_beta" "gamma" "delta_value"))))
        (search-forward "Child")
        (beginning-of-line)
        (let ((inherited-before
               (list (org-entry-get nil "Owner" 'inherit)
                     (org-entry-get-multivalued-property nil "Multi")
                     (org-entry-get-with-inheritance "Multi"))))
          (goto-char (point-min))
          (search-forward "Parent")
          (beginning-of-line)
          (org-entry-remove-from-multivalued-property
           nil "Multi" "alpha beta")
          (org-entry-delete nil "Owner")
          (org-entry-delete nil "Multi")
          (let ((tree (org-element-parse-buffer)))
            (list parent-before
                  protected
                  restored
                  inherited-before
                  (nreverse changes)
                  (org-entry-properties nil 'standard)
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
fn org_startup_log_options_todo_property_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-todo-keywords
           '((sequence "TODO(t)" "WAIT(w@/!)" "|" "DONE(d!)" "CANCELED(c@)")))
          (org-log-note-headings
           '((done . "DONE %-12s from %-12S %t")
             (state . "STATE %-12s from %-12S %t")
             (note . "NOTE %t")))
          (org-log-note-clock-out nil))
      (org-mode)
      (insert "#+STARTUP: logdrawer lognotedone lognoterepeat nologreschedule logredeadline nologstatesreversed\n")
      (insert "#+PROPERTY: Owner_ALL Ada Bea Cy\n")
      (insert "* TODO Parent :project:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** TODO Child\n")
      (insert "SCHEDULED: <2026-05-27 Wed +1w>\n")
      (insert "DEADLINE: <2026-05-28 Thu>\n")
      (goto-char (point-min))
      (org-set-regexps-and-options)
      (let ((startup-settings
             (list org-log-into-drawer
                   org-log-done
                   org-log-repeat
                   org-log-reschedule
                   org-log-redeadline
                   org-log-states-order-reversed)))
        (search-forward "Child")
        (beginning-of-line)
        (let ((inherited-owner (org-entry-get nil "Owner" 'inherit))
              (allowed-owner
               (org-property-get-allowed-values nil "Owner" 'table)))
          (org-set-property "Owner" "Bea")
          (org-schedule nil "2026-06-03")
          (org-deadline nil "2026-06-04")
          (org-todo "WAIT")
          (when (and (boundp 'org-log-note-marker)
                     org-log-note-marker
                     (marker-buffer org-log-note-marker))
            (with-current-buffer (marker-buffer org-log-note-marker)
              (goto-char org-log-note-marker)
              (insert "Waiting on review")
              (org-add-log-note)))
          (org-todo "DONE")
          (when (and (boundp 'org-log-note-marker)
                     org-log-note-marker
                     (marker-buffer org-log-note-marker))
            (with-current-buffer (marker-buffer org-log-note-marker)
              (goto-char org-log-note-marker)
              (insert "Finished after review")
              (org-add-log-note)))
          (list startup-settings
                inherited-owner
                allowed-owner
                (org-entry-properties nil 'standard)
                (org-log-beginning nil)
                (replace-regexp-in-string
                 "\\[[0-9][^]\n]+\\]"
                 "[stamp]"
                 (buffer-substring-no-properties
                  (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_property_inherit_literal_special_views_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-property-inheritance '("Owner" "NilLike" "Effort"))
          (org-property-format ":%s: %s")
          (changes nil))
      (add-hook 'org-property-changed-functions
                (lambda (key value) (push (list key value) changes))
                nil t)
      (org-mode)
      (insert "#+CATEGORY: DemoCat\n")
      (insert "#+PROPERTY: Owner_ALL Ada Bea Cy\n")
      (insert "#+COLUMNS: %25ITEM %TODO %PRIORITY %Owner %Effort{:}\n")
      (insert "* TODO Parent [#B]\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:NilLike: nil\n:Effort: 1:00\n:END:\n")
      (insert "** WAIT Child [#C]\n")
      (insert ":PROPERTIES:\n:Local: child-only\n:END:\n")
      (goto-char (point-min))
      (org-set-regexps-and-options)
      (search-forward "Child")
      (beginning-of-line)
      (let ((inherit-flags
             (mapcar #'org-property-inherit-p
                     '("Owner" "Local" "Effort" "CATEGORY" "NilLike")))
            (literal-values
             (list (org-entry-get nil "NilLike" 'inherit)
                   (org-entry-get nil "NilLike" 'inherit 'literal-nil)
                   (org-entry-get-with-inheritance "NilLike")
                   (org-entry-get-with-inheritance "NilLike" 'literal-nil)))
            (special-before
             (list (org-entry-get nil "TODO")
                   (org-entry-get nil "PRIORITY")
                   (org-entry-get nil "CATEGORY")
                   (org-entry-get nil "ITEM")
                   (org-property-or-variable-value "COLUMNS" 'inherit)))
            (props-standard-before (org-entry-properties nil 'standard))
            (props-special-before (org-entry-properties nil 'special))
            (props-all-before (org-entry-properties nil)))
        (org-entry-put nil "TODO" "DONE")
        (org-entry-put nil "PRIORITY" "A")
        (org-entry-put nil "Owner" "Bea")
        (org-entry-put nil "NilLike" nil)
        (org-entry-delete nil "Local")
        (list inherit-flags
              literal-values
              special-before
              props-standard-before
              props-special-before
              props-all-before
              (nreverse changes)
              (org-entry-properties nil 'standard)
              (org-entry-properties nil 'special)
              (org-entry-get nil "TODO")
              (org-entry-get nil "PRIORITY")
              (org-entry-get nil "NilLike" 'inherit 'literal-nil)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_property_separators_postprocess_global_cleanup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-property-inheritance t)
          (org-property-separators
           '((("Tags" "Path") . ";")
             ("^List" . "|")))
          (org-properties-postprocess-alist
           '(("Score" . (lambda (value)
                          (number-to-string
                           (* 2 (string-to-number value)))))
             ("Owner" . upcase)))
          (org-property-format "%-12s :: %s")
          (changes nil))
      (add-hook 'org-property-changed-functions
                (lambda (key value) (push (list key value) changes))
                nil t)
      (org-mode)
      (insert "#+PROPERTY: Owner_ALL Ada Bea Cy\n")
      (insert "#+PROPERTY: Score_ALL 1 2 3 4\n")
      (insert "* Parent\n")
      (insert ":PROPERTIES:\n:Tags: root;shared\n:Path: /a;/b\n:ListOne: p|q\n:Score: 1\n:END:\n")
      (insert "** Child\n")
      (insert ":PROPERTIES:\n:Tags+: child\n:Path+: /c\n:ListOne+: r\n:Owner: Ada\n:Score: 2\n:END:\n")
      (goto-char (point-min))
      (org-set-regexps-and-options)
      (search-forward "Child")
      (beginning-of-line)
      (let ((before
             (list (org-entry-get nil "Tags" 'inherit)
                   (org-entry-get nil "Path" 'inherit)
                   (org-entry-get nil "ListOne" 'inherit)
                   (org-entry-get-multivalued-property nil "Tags")
                   (org-entry-get-multivalued-property nil "Path")
                   (org-entry-get-multivalued-property nil "ListOne")
                   (org-property-get-allowed-values nil "Owner" 'table)
                   (org-property-get-allowed-values nil "Score" 'table)))
            after-set after-delete ast)
        (org-set-property "Owner" "bea")
        (org-set-property "Score" "3")
        (org-entry-put-multivalued-property nil "Tags" "alpha beta" "gamma")
        (org-entry-add-to-multivalued-property nil "Tags" "delta value")
        (org-entry-remove-from-multivalued-property nil "Tags" "gamma")
        (org-entry-put nil "Path+" "/d")
        (org-entry-put nil "ListOne+" "s|t")
        (setq after-set
              (list (org-entry-properties nil 'standard)
                    (org-entry-get nil "Owner")
                    (org-entry-get nil "Score")
                    (org-entry-get nil "Tags" 'inherit)
                    (org-entry-get nil "Path" 'inherit)
                    (org-entry-get nil "ListOne" 'inherit)
                    (org-entry-get-multivalued-property nil "Tags")
                    (buffer-substring-no-properties
                     (point-min) (point-max))))
        (org-delete-property-globally "Path")
        (org-delete-property-globally "ListOne")
        (setq after-delete
              (list (org-property-values "Path")
                    (org-property-values "ListOne")
                    (org-entry-get nil "Path" 'inherit)
                    (org-entry-get nil "ListOne" 'inherit)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))
        (goto-char (point-min))
        (while (re-search-forward org-property-re nil t)
          (org--align-node-property))
        (setq ast
              (org-element-map (org-element-parse-buffer)
                  '(headline property-drawer node-property)
                (lambda (el)
                  (list (org-element-type el)
                        (org-element-property :raw-value el)
                        (org-element-property :key el)
                        (org-element-property :value el)
                        (org-element-property :begin el)
                        (org-element-property :end el)))))
        (list before
              after-set
              after-delete
              ast
              (nreverse changes)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_todo_tag_property_clock_state_mutation_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Project\" (\"work\") \"5:00\" \"Ada\" \"TODO\" (\"TODO\" \"WAIT\") nil) (\"Task A\" (\"urgent\") \"1:30\" \"Ada\" \"TODO\" (\"TODO\" \"WAIT\") nil) (t) (#(\"Task A\" 0 6 (org-todo-head \"TODO\")) (\"urgent\") \"1:30\" \"Ada\" \"DONE\" nil (\"DONE\" \"CANCELED\")) (\"Sub A1\" nil \"1:30\" \"Ada\" \"TODO\" (\"TODO\" \"WAIT\") nil) (\"Task B\" (\"blocked\" \"waiting\") \"0:45\" \"Ada\" \"WAIT\" (\"WAIT\") nil) \"* TODO Project :work:\\n:PROPERTIES:\\n:Owner: Ada\\n:Effort: 5:00\\n:END:\\n** DONE Task A                                                       :urgent:\\nCLOSED: [stamp]\\n:PROPERTIES:\\n:Effort: 1:30\\n:ORDERED: t\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\nSCHEDULED: <2026-05-27 Wed>\\n*** TODO Sub A1\\n:PROPERTIES:\\n:Priority: High\\n:END:\\n*** TODO Sub A2\\n** WAIT Task B                                              :blocked:waiting:\\n:PROPERTIES:\\n:Effort: 0:45\\n:END:\\n* DONE Finished :work:\\nCLOSED: [stamp]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (let ((org-log-done 'time)
          (org-log-into-drawer t)
          (org-use-property-inheritance t)
          (org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE" "CANCELED"))))
      (org-mode)
      (insert "* TODO Project :work:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Effort: 5:00\n:END:\n")
      (insert "** TODO Task A :urgent:\n")
      (insert ":PROPERTIES:\n:Effort: 1:30\n:ORDERED: t\n:END:\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert "*** TODO Sub A1\n")
      (insert "*** TODO Sub A2\n")
      (insert "** WAIT Task B\n")
      (insert ":PROPERTIES:\n:Effort: 0:45\n:END:\n")
      (insert "* DONE Finished :work:\n")
      (insert "CLOSED: [2026-05-26 Mon]\n")
      (let ((snap (lambda ()
                    (list (org-get-heading t t t t)
                          (org-get-tags nil t)
                          (org-entry-get nil "Effort" t)
                          (org-entry-get nil "Owner" t)
                          (substring-no-properties (or (org-get-todo-state) ""))
                          (org-entry-is-todo-p)
                          (org-entry-is-done-p)))))
        (goto-char (point-min))
        (search-forward "Project")
        (beginning-of-line)
        (let ((p1 (funcall snap)))
          (goto-char (point-min))
          (search-forward "Task A")
          (beginning-of-line)
          (let ((p2 (funcall snap)))
            (org-clock-in)
            (let ((p3 (list (org-clocking-p))))
              (org-clock-out)
              (goto-char (point-min))
              (search-forward "Task A")
              (beginning-of-line)
              (org-todo "DONE")
              (let ((p4 (funcall snap)))
                (goto-char (point-min))
                (search-forward "Sub A1")
                (beginning-of-line)
                (org-set-property "Priority" "High")
                (let ((p5 (funcall snap)))
                  (goto-char (point-min))
                  (search-forward "Task B")
                  (beginning-of-line)
                  (org-set-tags '("blocked" "waiting"))
                  (let ((p6 (funcall snap))
                        (buf (replace-regexp-in-string
                              "CLOSED: \\[.*\\]" "CLOSED: [stamp]"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
                     (list p1 p2 p3 p4 p5 p6 buf)))))))))))"##,
        expect,
    );
}

#[test]
fn org_property_inherit_set_delete_globally_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Ada\" \"Ada\" \"main\" \"main\" \"5:00\" \"5:00\") (nil \"Ada\" \"main\" \"main\" \"2:00\" \"2:00\") (nil \"Ada\" \"main\" \"main\" nil \"2:00\") (nil \"Ada\" \"main\" \"main\" nil \"5:00\") (\"Bob\" \"Bob\" \"???\" \"???\" nil nil) ((\"CATEGORY\" . \"main\") (\"STATUS\" . \"active\") (\"EFFORT\" . \"2:00\")) nil \"* Root\\n:PROPERTIES:\\n:CATEGORY: main\\n:Effort: 5:00\\n:END:\\n** Child A\\n:PROPERTIES:\\n:Effort: 2:00\\n:Status:   active\\n:END:\\n*** Grandchild\\n** Child B\\n* Other\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-property-inheritance t))
      (org-mode)
      (insert "* Root\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:CATEGORY: main\n:Effort: 5:00\n:END:\n")
      (insert "** Child A\n")
      (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
      (insert "*** Grandchild\n")
      (insert "** Child B\n")
      (insert "* Other\n")
      (insert ":PROPERTIES:\n:Owner: Bob\n:END:\n")
      ;; Get inherited properties
      (let ((snap (lambda (pos)
                    (goto-char pos)
                    (list (org-entry-get nil "Owner")
                          (org-entry-get nil "Owner" 'inherit)
                          (org-entry-get nil "CATEGORY")
                          (org-entry-get nil "CATEGORY" 'inherit)
                          (org-entry-get nil "Effort")
                          (org-entry-get nil "Effort" 'inherit)))))
        (goto-char (point-min))
        (search-forward "Root")
        (let ((root-props (funcall snap (line-beginning-position))))
          (search-forward "Child A")
          (let ((child-a (funcall snap (line-beginning-position))))
            (search-forward "Grandchild")
            (let ((grandchild (funcall snap (line-beginning-position))))
              (goto-char (point-min))
              (search-forward "Child B")
              (let ((child-b (funcall snap (line-beginning-position))))
                (goto-char (point-min))
                (search-forward "Other")
                (let ((other (funcall snap (line-beginning-position))))
                  ;; Set a new property
                  (goto-char (point-min))
                  (search-forward "Child A")
                  (beginning-of-line)
                  (org-set-property "Status" "active")
                  ;; Get all properties
                  (let ((all-a (org-entry-properties nil 'standard)))
                    ;; Delete globally
                    (org-delete-property-globally "Owner")
                    ;; Check after delete
                    (goto-char (point-min))
                    (search-forward "Grandchild")
                    (let ((owner-after-delete (org-entry-get nil "Owner" 'inherit))
                          (full-buf (buffer-substring-no-properties
                                     (point-min) (point-max))))
                      (list root-props
                            child-a
                            grandchild
                            child-b
                            other
                            all-a
                            owner-after-delete
                            full-buf))))))))))))"##,
        expect,
    );
}

#[test]
fn org_property_set_delete_multivalue_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Ada\" \"Ada\" \"main\" \"main\" nil nil) (nil \"Ada\" \"main\" \"main\" \"2:00\" \"2:00\") (nil \"Ada\" \"main\" \"main\" nil \"2:00\") ((\"CATEGORY\" . \"main\") (\"STATUS\" . \"active\") (\"EFFORT\" . \"2:00\")) nil \"* Root\\n:PROPERTIES:\\n:CATEGORY: main\\n:END:\\n** Child A\\n:PROPERTIES:\\n:Effort: 2:00\\n:Status:   active\\n:END:\\n*** Grandchild\\n** Child B\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-property-inheritance t))
      (org-mode)
      (insert "* Root\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:CATEGORY: main\n:END:\n")
      (insert "** Child A\n")
      (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
      (insert "*** Grandchild\n")
      (insert "** Child B\n")
      (let ((snap (lambda ()
                    (list (org-entry-get nil "Owner")
                          (org-entry-get nil "Owner" 'inherit)
                          (org-entry-get nil "CATEGORY")
                          (org-entry-get nil "CATEGORY" 'inherit)
                          (org-entry-get nil "Effort")
                          (org-entry-get nil "Effort" 'inherit)))))
        (goto-char (point-min))
        (search-forward "Root")
        (let ((root (funcall snap)))
          (search-forward "Child A")
          (let ((child-a (funcall snap)))
            (search-forward "Grandchild")
            (let ((grandchild (funcall snap)))
              ;; Set new property on Child A
              (goto-char (point-min))
              (search-forward "Child A")
              (beginning-of-line)
              (org-set-property "Status" "active")
              ;; Get all properties
              (let ((all-a (org-entry-properties nil 'standard)))
                ;; Delete Owner globally
                (org-delete-property-globally "Owner")
                ;; Check after delete
                (goto-char (point-min))
                (search-forward "Grandchild")
                (let ((owner-after (org-entry-get nil "Owner" 'inherit))
                      (buf (buffer-substring-no-properties
                            (point-min) (point-max))))
                  (list root child-a grandchild all-a
                        owner-after buf))))))))))"##,
        expect,
    );
}

#[test]
fn org_property_inherit_clock_edit_delete_global_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 52 52)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (org-mode)
    (insert "* Project :project:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:Budget: 100h\n:END:\n")
    (insert "** Task Alpha\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Assigned: Bob\n:END:\n")
    (insert ":LOGBOOK:\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 11:00] =>  2:00\n:END:\n")
    (insert "Body alpha.\n\n")
    (insert "** Task Beta\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:END:\n")
    (insert "Body beta.\n\n")
    (let ((snap (lambda (name)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward name)
                    (list name
                          (org-entry-get nil "Owner" 'inherit)
                          (org-entry-get nil "Budget" 'inherit)
                          (org-entry-get nil "Effort")
                          (org-entry-get nil "Assigned" 'inherit))))))
      (let ((alpha (funcall snap "Alpha"))
            (beta (funcall snap "Beta")))
        ;; Edit: set new property on Alpha
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-set-property "Status" "active")
        ;; Delete Owner globally
        (org-delete-property-globally "Owner")
        ;; Check after
        (goto-char (point-min))
        (search-forward "Alpha")
        (let ((alpha-after
               (list "Alpha"
                     (org-entry-get nil "Owner" 'inherit)
                     (org-entry-get nil "Budget" 'inherit)
                     (org-entry-get nil "Effort")
                     (org-entry-get nil "Assigned" 'inherit)
                     (org-entry-get nil "Status"))))
          (goto-char (point-min))
          (search-forward "Beta")
          (let ((beta-after
                 (list "Beta"
                       (org-entry-get nil "Owner" 'inherit)
                       (org-entry-get nil "Budget" 'inherit)
                       (org-entry-get nil "Effort"))))
             (list alpha beta alpha-after beta-after
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_property_block_tags_inherit_set_delete_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 41 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Project :project:\n")
    (insert ":PROPERTIES:\n:Owner: Alice\n:CATEGORY: work\n:END:\n")
    (insert "** Sub A :alpha:\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Priority: high\n:END:\n")
    (insert "*** Leaf A1\nBody.\n\n")
    (insert "** Sub B :beta:\n")
    (insert ":PROPERTIES:\n:Effort: 3h\n:Owner: Bob\n:END:\n")
    (insert "*** Leaf B1\nBody.\n\n")
    (let ((snap (lambda (name)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward name)
                    (list name
                          (org-entry-get nil "Owner" 'inherit)
                          (org-entry-get nil "CATEGORY" 'inherit)
                          (org-entry-get nil "Effort")
                          (org-entry-get nil "Priority" 'inherit)
                          (org-get-tags nil t))))))
      (let ((leaf-a1 (funcall snap "Leaf A1"))
            (leaf-b1 (funcall snap "Leaf B1")))
        ;; Set Priority on Sub A
        (goto-char (point-min))
        (search-forward "Sub A")
        (beginning-of-line)
        (org-set-property "Priority" "critical")
        ;; Delete Owner from Sub B
        (goto-char (point-min))
        (search-forward "Sub B")
        (beginning-of-line)
        (org-delete-property "Owner")
        ;; Check after
        (let ((leaf-a1-after (funcall snap "Leaf A1"))
              (leaf-b1-after (funcall snap "Leaf B1")))
           (list leaf-a1 leaf-b1
                 leaf-a1-after leaf-b1-after
                 (buffer-substring-no-properties
                  (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_property_inherit_effort_category_set_multi_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 51 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Phase 1\n")
    (insert ":PROPERTIES:\n:CATEGORY: planning\n:Owner: Alice\n:END:\n")
    (insert "** Step A\n")
    (insert ":PROPERTIES:\n:Effort: 2h\n:Assigned: Bob\n:END:\n")
    (insert "*** Sub A1\nBody.\n\n")
    (insert "*** Sub A2\nBody.\n\n")
    (insert "** Step B\n")
    (insert ":PROPERTIES:\n:Effort: 1h\n:Assigned: Carol\n:END:\n")
    (insert "*** Sub B1\nBody.\n\n")
    (insert "* Phase 2\n")
    (insert ":PROPERTIES:\n:CATEGORY: execution\n:Owner: Dave\n:END:\n")
    (insert "** Step C\n")
    (insert ":PROPERTIES:\n:Effort: 4h\n:END:\n")
    (let ((snap (lambda (name)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward name)
                    (list name
                          (org-entry-get nil "Owner" 'inherit)
                          (org-entry-get nil "CATEGORY" 'inherit)
                          (org-entry-get nil "Effort")
                          (org-entry-get nil "Assigned" 'inherit))))))
      (let ((sub-a1 (funcall snap "Sub A1"))
            (sub-b1 (funcall snap "Sub B1"))
            (step-c (funcall snap "Step C")))
        ;; Set properties
        (goto-char (point-min))
        (search-forward "Step A")
        (beginning-of-line)
        (org-set-property "Status" "active")
        (goto-char (point-min))
        (search-forward "Phase 2")
        (beginning-of-line)
        (org-set-property "Priority" "high")
        ;; Delete
        (goto-char (point-min))
        (search-forward "Step B")
        (beginning-of-line)
        (org-delete-property "Assigned")
        ;; Check
        (let ((sub-a1-after (funcall snap "Sub A1"))
              (sub-b1-after (funcall snap "Sub B1"))
              (step-c-after (funcall snap "Step C")))
          (list sub-a1 sub-b1 step-c
                sub-a1-after sub-b1-after step-c-after
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}
