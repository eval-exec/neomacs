use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_make_tags_matcher_scan_properties_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"+work+urgent+TODO=\\\"TODO\\\"+Effort>=1\" nil ((\"Alpha\" (\"urgent\") \"Ada\" \"1.5\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-tag-inheritance t))
      (org-mode)
      (insert "#+TODO: TODO WAIT | DONE\n")
      (insert "* TODO Parent :work:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** TODO Alpha :urgent:\n")
      (insert ":PROPERTIES:\n:Effort: 1.5\n:END:\n")
      (insert "** WAIT Beta :urgent:\n")
      (insert ":PROPERTIES:\n:Effort: 2.0\n:END:\n")
      (insert "* TODO Gamma :home:\n")
      (insert ":PROPERTIES:\n:Effort: 3.0\n:END:\n")
      (goto-char (point-min))
      (let* ((compiled (org-make-tags-matcher
                        "+work+urgent+TODO=\"TODO\"+Effort>=1"))
             (matcher (cdr compiled)))
        (list
         (car compiled)
         org--matcher-tags-todo-only
         (org-scan-tags
          (lambda ()
            (list (org-get-heading t t t t)
                  (org-get-tags nil t)
                  (org-entry-get nil "Owner" t)
                  (org-entry-get nil "Effort")))
          matcher
          nil))))))"##,
        expect,
    );
}

#[test]
fn org_scan_tags_sparse_tree_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" t) (\"Child A\" t) (\"Beta\" t) (\"Child B\" t) (\"Gamma\" t)) \"* TODO Alpha :work:\\nBody A\\n** TODO Child A :work:\\nBody child A\\n* TODO Beta :home:\\nBody B\\n** TODO Child B :work:\\nBody child B\\n* DONE Gamma :work:\\nBody G\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\nBody A\n")
    (insert "** TODO Child A :work:\nBody child A\n")
    (insert "* TODO Beta :home:\nBody B\n")
    (insert "** TODO Child B :work:\nBody child B\n")
    (insert "* DONE Gamma :work:\nBody G\n")
    (goto-char (point-min))
    (let ((matcher (cdr (org-make-tags-matcher "+work+TODO=\"TODO\""))))
      (org-scan-tags 'sparse-tree matcher nil)
      (list
       (let (out)
         (goto-char (point-min))
         (while (re-search-forward "^\\*+ " nil t)
           (push (list (org-get-heading t t t t)
                       (not (null (org-invisible-p
                                   (line-end-position)))))
                 out))
         (nreverse out))
       (buffer-substring-no-properties
        (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_global_tags_completion_table_files_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"done\" \"home\" #(\"project\" 0 7 (inherited t)) \"urgent\" \"work\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((one (make-temp-file "org-tags-one" nil ".org"
                             "#+FILETAGS: :project:
* TODO Alpha :work:urgent:
* DONE Beta :done:
"))
        (two (make-temp-file "org-tags-two" nil ".org"
                             "* WAIT Gamma :home:
:PROPERTIES:
:CATEGORY: House
:END:
")))
    (unwind-protect
        (let* ((org-agenda-files (list one two))
               (table (org-global-tags-completion-table (list one two))))
          (sort
           (mapcar (lambda (entry)
                     (if (consp entry) (car entry) entry))
                   table)
           #'string<))
      (dolist (file (list one two))
        (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
        (when (file-exists-p file) (delete-file file))))))"##,
        expect,
    );
}

#[test]
fn org_tags_group_inheritance_todo_property_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"+project+{urg\\\\|lab}-ARCHIVE+TODO<>\\\"DONE\\\"+Score>=5/NEXT|TODO\" 9 19 (regexp t)) nil ((\"Alpha\" (\"urgent\" \"lab\") \"Ada\" \"8\" \"Mixed\" 2)) \"+secret\" ((\"Parent\" (\"project\" \"secret\"))) \"+project+Score>=6\" ((\"Alpha\" 2 \"8\") (\"Delta\" 2 \"6\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-tag-inheritance t)
          (org-tags-exclude-from-inheritance '("secret"))
          (org-tags-match-list-sublevels t)
          (org-todo-keywords '((sequence "TODO" "NEXT" "WAIT" "|" "DONE")))
          (org-tag-alist '((:startgrouptag)
                           ("project")
                           (:grouptags)
                           ("work")
                           ("lab")
                           (:endgrouptag))))
      (org-mode)
      (insert "#+CATEGORY: Mixed\n")
      (insert "* TODO Parent :project:secret:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Score: 5\n:END:\n")
      (insert "** NEXT Alpha :urgent:lab:\n")
      (insert ":PROPERTIES:\n:Effort: 0:30\n:Score: 8\n:END:\nAlpha body\n")
      (insert "** WAIT Beta :hold:\n")
      (insert ":PROPERTIES:\n:Effort: 1:15\n:Score: 3\n:END:\nBeta body\n")
      (insert "** DONE Gamma :urgent:ARCHIVE:\n")
      (insert ":PROPERTIES:\n:Effort: 2:00\n:Score: 9\n:END:\nGamma body\n")
      (insert "* TODO Home :home:\n")
      (insert ":PROPERTIES:\n:Owner: Bea\n:Score: 7\n:END:\n")
      (insert "** NEXT Delta :work:\n")
      (insert ":PROPERTIES:\n:Effort: 0:45\n:Score: 6\n:END:\nDelta body\n")
      (org-set-regexps-and-options)
      (let* ((org--matcher-tags-todo-only nil)
             (main (org-make-tags-matcher
                    "+project+{urg\\|lab}-ARCHIVE+TODO<>\"DONE\"+Score>=5/NEXT|TODO"))
             (main-todo-only org--matcher-tags-todo-only)
             (main-hits
              (org-scan-tags
               (lambda ()
                 (list (org-get-heading t t t t)
                       (org-get-tags nil t)
                       (org-entry-get nil "Owner" t)
                       (org-entry-get nil "Score")
                       (org-get-category)
                       (org-current-level)))
               (cdr main)
               main-todo-only))
             (local-secret (org-make-tags-matcher "+secret" t))
             (local-secret-hits
              (org-scan-tags
               (lambda ()
                 (list (org-get-heading t t t t)
                       (org-get-tags nil t)))
               (cdr local-secret)
               nil))
             (level-two (org-make-tags-matcher "+project+Score>=6"))
             (level-two-hits
              (org-scan-tags
               (lambda ()
                 (list (org-get-heading t t t t)
                       (org-current-level)
                       (org-entry-get nil "Score")))
               (cdr level-two)
               nil
               2)))
        (list
         (car main)
         main-todo-only
         main-hits
         (car local-secret)
         local-secret-hits
         (car level-two)
         level-two-hits)))))"##,
        expect,
    );
}

#[test]
fn org_tags_matcher_map_entries_inherited_property_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"+work\" ((\"Alpha\" (\"work\") \"2:00\" \"Ada\" \"proj\" \"\") (\"Sub A1\" (\"urgent\") \"0:30\" \"Ada\" \"proj\" \"TODO\") (\"Sub A2\" nil nil \"Ada\" \"proj\" \"DONE\") (\"Sub B1\" (\"work\") nil \"Bob\" \"???\" \"TODO\") (\"Gamma\" (\"work\" \"urgent\") nil nil \"???\" \"\") (\"WAIT Sub G1\" nil \"1:30\" nil \"???\" \"\")) nil ((\"Alpha\" \"2:00\") (\"Sub A2\" nil) (\"Beta\" \"1:00\") (\"Sub B1\" nil) (\"WAIT Sub G1\" \"1:30\")) \"+urgent\" ((\"Sub A1\" (\"urgent\")) (\"Gamma\" (\"work\" \"urgent\")) (\"WAIT Sub G1\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-tag-inheritance t)
          (org-use-property-inheritance t))
      (org-mode)
      (insert "* Alpha :work:\n")
      (insert ":PROPERTIES:\n:Effort: 2:00\n:Owner: Ada\n:CATEGORY: proj\n:END:\n")
      (insert "** TODO Sub A1 :urgent:\n")
      (insert ":PROPERTIES:\n:Effort: 0:30\n:END:\n")
      (insert "** DONE Sub A2\n")
      (insert "* Beta :home:\n")
      (insert ":PROPERTIES:\n:Effort: 1:00\n:Owner: Bob\n:END:\n")
      (insert "** TODO Sub B1 :work:\n")
      (insert "* Gamma :work:urgent:\n")
      (insert "** WAIT Sub G1\n")
      (insert ":PROPERTIES:\n:Effort: 1:30\n:END:\n")
      (let* ((matcher (org-make-tags-matcher "+work"))
             (fn (car matcher))
             (match-fn (cdr matcher))
             (hits (org-map-entries
                    (lambda ()
                      (list (org-get-heading t t t t)
                            (org-get-tags nil t)
                            (org-entry-get nil "Effort")
                            (org-entry-get nil "Owner" t)
                            (org-entry-get nil "CATEGORY" t)
                            (substring-no-properties
                             (or (org-get-todo-state) ""))))
                    "+work" nil))
             (todo-hits (org-map-entries
                         (lambda ()
                           (list (org-get-heading t t t t)
                                 (org-get-tags nil t)))
                         "+work+TODO" nil))
             (prop-hits (org-map-entries
                         (lambda ()
                           (list (org-get-heading t t t t)
                                 (org-entry-get nil "Effort")))
                         "Effort>=\"1:00\"" nil))
             (tag-groups (org-make-tags-matcher "+urgent"))
             (urgent-hits (org-map-entries
                           (lambda ()
                             (list (org-get-heading t t t t)
                                   (org-get-tags nil t)))
                           "+urgent" nil)))
        (list fn
              hits
              todo-hits
              prop-hits
              (car tag-groups)
              urgent-hits)))))"##,
        expect,
    );
}
