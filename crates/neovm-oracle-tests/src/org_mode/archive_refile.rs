use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_archive_subtree_file_context_properties_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+CATEGORY: Work\\n* Parent :client:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n** TODO Keep\\n\" \"#    -*- mode: org -*-\\n\\n\\nArchived entries from file <source-file>\\n\\n\\n* Archive\\n\\n** TODO Ship feature                                          :client:urgent:\\nDEADLINE: <2026-06-01 Mon>\\n:PROPERTIES:\\n:ARCHIVE_FILE: <source-file>\\n:ARCHIVE_OLPATH: Parent\\n:ARCHIVE_CATEGORY: Work\\n:ARCHIVE_TODO: TODO\\n:ARCHIVE_ITAGS: client\\n:END:\\nBody\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-archive)
  (let* ((file (make-temp-file "org-archive-source" nil ".org"))
         (archive (concat file "_archive")))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+CATEGORY: Work\n")
            (insert "* Parent :client:\n")
            (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
            (insert "** TODO Ship feature :urgent:\n")
            (insert "DEADLINE: <2026-06-01 Mon>\n")
            (insert "Body\n")
            (insert "** TODO Keep\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (let ((org-archive-location (concat archive "::* Archive"))
                  (org-archive-stamp-time nil)
                  (org-archive-subtree-add-inherited-tags t)
                  (org-archive-save-context-info '(file olpath category todo itags))
                  (org-archive-subtree-save-file-p nil))
              (goto-char (point-min))
              (search-forward "Ship feature")
              (beginning-of-line)
              (org-archive-subtree)
              (save-buffer)
              (let ((source (buffer-substring-no-properties
                             (point-min) (point-max)))
                    (archived (with-current-buffer
                                  (find-file-noselect archive)
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                (list source
                      (replace-regexp-in-string
                       (regexp-quote file)
                       "<source-file>"
                       archived))))))
      (dolist (buf (list (get-file-buffer file)
                         (get-file-buffer archive)))
        (when buf (kill-buffer buf)))
      (when (file-exists-p file) (delete-file file))
      (when (file-exists-p archive) (delete-file archive)))))"##,
        expect,
    );
}

#[test]
fn org_refile_copy_with_logbook_and_bookmark_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 55)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((file (make-temp-file "org-refile-copy" nil ".org"
                              "* Inbox\n** TODO Task :inbox:\nBody\n* Projects\n** Target\n")))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (let ((org-refile-keep t)
                (org-log-refile 'time)
                (org-log-into-drawer t))
            (goto-char (point-min))
            (search-forward "Task")
            (beginning-of-line)
            (let ((target-pos (save-excursion
                                (goto-char (point-min))
                                (search-forward "Target")
                                (line-beginning-position))))
              (org-refile nil nil (list "Target" file nil target-pos)))
            (save-buffer)
            (list (plist-get org-bookmark-names-plist :last-refile)
                  (replace-regexp-in-string
                   "- Refiled on \\[.*\\]"
                   "- Refiled on [stamp]"
                   (buffer-substring-no-properties
                    (point-min) (point-max)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_sparse_tree_match_visibility_and_map_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" (\"work\") t) (\"WAIT Hidden child\" (\"work\") t) (\"Beta\" (\"home\") t) (\"Matched child\" (\"work\") t) (\"Gamma\" (\"work\") t)) ((\"Alpha\" t) (\"WAIT Hidden child\" t) (\"Beta\" t) (\"Matched child\" t) (\"Gamma\" t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TODO: TODO WAIT | DONE\n")
    (insert "* TODO Alpha :work:\nAlpha body\n")
    (insert "** WAIT Hidden child :work:\nChild body\n")
    (insert "* TODO Beta :home:\nBeta body\n")
    (insert "** TODO Matched child :work:\nChild body\n")
    (insert "* DONE Gamma :work:\nGamma body\n")
    (goto-char (point-min))
    (org-match-sparse-tree nil "+work+TODO=\"TODO\"")
    (list
     (org-map-entries
      (lambda ()
        (list (org-get-heading t t t t)
              (org-get-tags nil t)
              (not (null (org-invisible-p (line-end-position))))))
      nil
      nil)
     (let (states)
       (goto-char (point-min))
       (while (re-search-forward "^\\*+ " nil t)
         (push (list (org-get-heading t t t t)
                     (not (null (org-invisible-p (line-end-position)))))
               states))
       (nreverse states)))))"##,
        expect,
    );
}

#[test]
fn org_refile_targets_cache_new_child_outline_path_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let* ((one (make-temp-file "org-refile-one" nil ".org"
                              "#+TITLE: One\n* Projects\n** Alpha :work:\n*** Leaf\n* Inbox\n"))
         (two (make-temp-file "org-refile-two" nil ".org"
                              "#+TITLE: Two\n* Areas\n** Beta :home:\n"))
         (org-refile-targets `((,(list one two) . (:maxlevel . 3))))
         (org-refile-use-outline-path 'title)
         (org-refile-use-cache t)
         (normalize-file
          (lambda (file)
            (cond
             ((null file) nil)
             ((string-prefix-p "org-refile-one" file) "<one>")
             ((string-prefix-p "org-refile-two" file) "<two>")
             (t file))))
         first second child)
    (unwind-protect
        (progn
          (org-refile-cache-clear)
          (setq first (mapcar (lambda (target)
                                (list (car target)
                                      (funcall normalize-file
                                               (and (nth 1 target)
                                                    (file-name-nondirectory
                                                     (nth 1 target))))
                                      (not (null (nth 3 target)))))
                              (with-current-buffer (find-file-noselect one)
                                (org-mode)
                                (org-refile-get-targets))))
          (setq second (mapcar (lambda (target)
                                 (list (car target)
                                       (funcall normalize-file
                                                (and (nth 1 target)
                                                     (file-name-nondirectory
                                                      (nth 1 target))))
                                       (not (null (nth 3 target)))))
                               (with-current-buffer (find-file-noselect one)
                                 (org-mode)
                                 (org-refile-get-targets))))
          (setq child
                (with-current-buffer (find-file-noselect two)
                  (org-mode)
                  (let* ((targets (org-refile-get-targets))
                         (parent (seq-find
                                  (lambda (target)
                                    (string-match-p "/Areas/Beta\\'" (car target)))
                                  targets)))
                    (org-refile-new-child parent "Gamma :new:")
                    (save-buffer)
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))
          (list first
                second
                child
                (not (null (org-refile-cache-get
                            (expand-file-name one)
                            "^\\*\\{1,3\\}[ \t]")))))
      (dolist (file (list one two))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_capture_refile_to_file_headline_then_archive_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-capture)
  (require 'org-refile)
  (require 'org-archive)
  (let* ((root (make-temp-file "org-cap-ref-archive" t))
         (main (expand-file-name "main.org" root))
         (archive (expand-file-name "archive.org" root))
         (org-capture-templates
          `(("t" "Todo" entry (file+headline ,main "Inbox")
             "** TODO %?\n:PROPERTIES:\n:Source: %a\n:END:\n"
             :empty-lines 0)))
         (org-refile-targets `((,main :maxlevel . 2)))
         (org-archive-location
          (concat archive "::"))
         (org-log-done 'time)
         (org-log-refile 'time)
         (org-stored-links nil))
    (unwind-protect
        (progn
          (with-temp-file main
            (insert "#+TITLE: Main\n")
            (insert "* Inbox\n")
            (insert "* Project :work:\n")
            (insert "** TODO Existing\n")
            (insert "Existing body.\n")
            (insert "** DONE Finished\n")
            (insert "Finished body.\n")
            (insert "* Archive target\n"))
          (with-current-buffer (find-file-noselect main)
            (org-mode)
            (let ((result nil))
              (org-capture-string "New task from capture" "t")
              (org-capture-finalize)
              (setq result
                    (cons 'after-capture
                          (list (buffer-substring-no-properties
                                 (point-min) (point-max)))))
              (goto-char (point-min))
              (search-forward "New task from capture")
              (beginning-of-line)
              (let ((org-refile-use-outline-path t)
                    (org-outline-path-complete-in-steps nil))
                (org-refile nil nil
                            (list "Existing" main nil
                                  (save-excursion
                                    (goto-char (point-min))
                                    (search-forward "Existing")
                                    (line-beginning-position)))))
              (save-buffer)
              (setq result
                    (cons (list 'after-refile
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))
                          result))
              (goto-char (point-min))
              (search-forward "Finished")
              (beginning-of-line)
              (org-archive-to-archive-sibling)
              (save-buffer)
              (setq result
                    (cons (list 'after-archive-main
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))
                          result))
              (let ((archive-content
                     (when (file-exists-p archive)
                       (with-temp-buffer
                         (insert-file-contents archive)
                         (buffer-string)))))
                (setq result
                      (cons (list 'archive-file
                                  (replace-regexp-in-string
                                   ":ARCHIVE_TIME: \\[.*\\]"
                                   ":ARCHIVE_TIME: [stamp]"
                                   (or archive-content "")))
                            result)))
              (kill-buffer)
              (list (nreverse result)
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>"
                     (mapconcat
                      #'identity
                      (mapcar (lambda (s)
                                (replace-regexp-in-string
                                 (regexp-quote root) "<root>" s))
                              org-stored-links)
                      "\n")))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_archive_sibling_reversed_order_stats_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Parent [0/1]\\n** TODO Keep\\n** Archive                                                          :ARCHIVE:\\n*** DONE Old one\\n:PROPERTIES:\\n:ARCHIVE_TIME: [stamp]\\n:END:\\nBody one\\n*** DONE Old two\\n:PROPERTIES:\\n:ARCHIVE_TIME: [stamp]\\n:END:\\nBody two\\n\" ((\"Parent\" 1 nil) (\"Archive\" 2 t) (\"Old one\" 3 t) (\"Old two\" 3 t) (\"Keep\" 2 nil)))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-archive)
  (with-temp-buffer
    (let ((org-archive-reversed-order t)
          (org-provide-todo-statistics t)
          (org-todo-keywords '((sequence "TODO" "|" "DONE"))))
      (org-mode)
      (insert "* Parent [1/3]\n")
      (insert "** DONE Old one\n")
      (insert "Body one\n")
      (insert "** DONE Old two\n")
      (insert "Body two\n")
      (insert "** TODO Keep\n")
      (goto-char (point-min))
      (search-forward "Old two")
      (beginning-of-line)
      (org-archive-to-archive-sibling)
      (goto-char (point-min))
      (search-forward "Old one")
      (beginning-of-line)
      (org-archive-to-archive-sibling)
      (org-update-statistics-cookies t)
      (list (replace-regexp-in-string
             ":ARCHIVE_TIME: .*"
             ":ARCHIVE_TIME: [stamp]"
             (buffer-substring-no-properties (point-min) (point-max)))
            (mapcar
             (lambda (needle)
               (save-excursion
                 (goto-char (point-min))
                 (search-forward needle)
                 (list needle
                       (org-current-level)
                       (not (null (org-invisible-p (line-end-position)))))))
             '("Parent" "Archive" "Old one" "Old two" "Keep"))))))"##,
        expect,
    );
}

#[test]
fn org_archive_all_done_tag_then_move_old_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"No file associated to buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-archive)
  (with-temp-buffer
    (let ((org-archive-location "::* Archived")
          (org-archive-save-context-info '(time todo category olpath))
          (org-archive-stamp-time nil)
          (org-confirm-babel-evaluate nil))
      (org-mode)
      (insert "#+CATEGORY: Batch\n")
      (insert "* Project\n")
      (insert "** DONE Closed\nCLOSED: [2026-05-01 Fri]\n")
      (insert "** DONE Old timestamp\nSCHEDULED: <2026-05-01 Fri>\n")
      (insert "** TODO Active\nSCHEDULED: <2026-06-01 Mon>\n")
      (insert "** DONE Fresh\nSCHEDULED: <2026-05-27 Wed>\n")
      (goto-char (point-min))
      (search-forward "Project")
      (beginning-of-line)
      (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t)))
        (org-archive-all-done 'tag)
        (org-archive-all-old nil))
      (list (buffer-substring-no-properties (point-min) (point-max))
            (org-map-entries
             (lambda ()
               (list (org-get-heading t t t t)
                     (org-get-todo-state)
                     (org-get-tags nil t)
                     (org-entry-get nil "ARCHIVE_CATEGORY")
                     (org-entry-get nil "ARCHIVE_TODO")))
             nil nil)))))"##,
        expect,
    );
}

#[test]
fn org_archive_property_locations_hooks_files_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"archive-a.org\" \"* Project Archive\") (\"archive-b.org\" \"* Other Archive\") ((\"Task A\" 2 (\"done\")) (\"Task B\" 2 (\"closed\"))) (\"archive-a.org\" \"archive-b.org\") (\"archive-a.org\" \"archive-b.org\" \"source.org\") \"#+CATEGORY: Cases\\n* Project :client:\\n:PROPERTIES:\\n:ARCHIVE: <root>/archive-a.org::* Project Archive\\n:END:\\n** TODO Active\\n* Other :ops:\\n:PROPERTIES:\\n:ARCHIVE: <root>/archive-b.org::* Other Archive\\n:END:\\n\" \"\\nArchived entries from file <root>/source.org\\n\\n\\n* Project Archive\\n\\n** DONE Task A                                                  :client:done:\\n:PROPERTIES:\\n:ARCHIVE_FILE: <root>/source.org\\n:ARCHIVE_CATEGORY: Cases\\n:ARCHIVE_TODO: DONE\\n:ARCHIVE_OLPATH: Project\\n:ARCHIVE_ITAGS: client\\n:ARCHIVE_LTAGS: done\\n:END:\\nBody A\\n\" \"\\nArchived entries from file <root>/source.org\\n\\n\\n* Other Archive\\n\\n** DONE Task B                                                   :ops:closed:\\n:PROPERTIES:\\n:ARCHIVE_FILE: <root>/source.org\\n:ARCHIVE_CATEGORY: Cases\\n:ARCHIVE_TODO: DONE\\n:ARCHIVE_OLPATH: Other\\n:ARCHIVE_ITAGS: ops\\n:ARCHIVE_LTAGS: closed\\n:END:\\nBody B\\n\")""##
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-archive)
  (let* ((root (make-temp-file "org-archive-props" t))
         (source (expand-file-name "source.org" root))
         (archive-a (expand-file-name "archive-a.org" root))
         (archive-b (expand-file-name "archive-b.org" root))
         (events nil)
         (org-archive-location "%s_archive::* Default Archive")
         (org-archive-save-context-info
          '(file category todo olpath itags ltags))
         (org-archive-subtree-add-inherited-tags t)
         (org-archive-subtree-save-file-p t)
         (org-archive-hook
          (list (lambda ()
                  (push (list (org-get-heading t t t t)
                              (org-current-level)
                              (org-get-tags nil t))
                        events)))))
    (unwind-protect
        (progn
          (with-temp-file source
            (insert "#+CATEGORY: Cases\n")
            (insert "* Project :client:\n")
            (insert ":PROPERTIES:\n:ARCHIVE: " archive-a "::* Project Archive\n:END:\n")
            (insert "** DONE Task A :done:\n")
            (insert "Body A\n")
            (insert "** TODO Active\n")
            (insert "* Other :ops:\n")
            (insert ":PROPERTIES:\n:ARCHIVE: " archive-b "::* Other Archive\n:END:\n")
            (insert "** DONE Task B :closed:\n")
            (insert "Body B\n"))
          (with-current-buffer (find-file-noselect source)
            (org-mode)
            (goto-char (point-min))
            (search-forward "Task A")
            (beginning-of-line)
            (let ((loc-a (org-archive--compute-location
                          (org-entry-get nil "ARCHIVE" t))))
              (org-archive-subtree)
              (goto-char (point-min))
              (search-forward "Task B")
              (beginning-of-line)
              (let ((loc-b (org-archive--compute-location
                            (org-entry-get nil "ARCHIVE" t))))
                (org-archive-subtree)
                (save-buffer)
                (list (list (file-relative-name (car loc-a) root)
                            (cdr loc-a))
                      (list (file-relative-name (car loc-b) root)
                            (cdr loc-b))
                      (nreverse events)
                      (sort (mapcar (lambda (file)
                                      (file-relative-name file root))
                                    (org-all-archive-files))
                            #'string<)
                      (sort (mapcar (lambda (file)
                                      (file-relative-name file root))
                                    (org-add-archive-files
                                     (list source)))
                            #'string<)
                      (replace-regexp-in-string
                       (regexp-quote root)
                       "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max)))
                      (with-current-buffer (find-file-noselect archive-a)
                        (replace-regexp-in-string
                         (regexp-quote root)
                         "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max))))
                      (with-current-buffer (find-file-noselect archive-b)
                        (replace-regexp-in-string
                         (regexp-quote root)
                         "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))))))
      (dolist (file (list source archive-a archive-b))
        (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
        (when (file-exists-p file) (delete-file file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_refile_then_archive_logged_context_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"Alpha\" 3 (\"work\"))) \"** DONE Beta :work:\\nBeta body\\n\" \"#+CATEGORY: Flow\\n* Inbox :inbox:\\n* Projects :project:\\n** Target :client:\\nTarget body\\n*** TODO Alpha                                                         :work:\\nAlpha body\\n\" \"\\nArchived entries from file <root>/tasks.org\\n\\n\\n* Archive\\n\\n** DONE Beta                                                     :inbox:work:\\n:PROPERTIES:\\n:ARCHIVE_FILE: <root>/tasks.org\\n:ARCHIVE_OLPATH: Inbox\\n:ARCHIVE_CATEGORY: Flow\\n:ARCHIVE_TODO: DONE\\n:ARCHIVE_ITAGS: inbox\\n:ARCHIVE_LTAGS: work\\n:END:\\nBeta body\\n\" ((\"Inbox\" 1 nil (\"inbox\") nil nil) (\"Projects\" 1 nil (\"project\") nil nil) (\"Target\" 2 nil (\"client\") nil nil) (\"Alpha\" 3 \"TODO\" (\"work\") nil nil)))""##
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (require 'org-archive)
  (let* ((root (make-temp-file "org-refile-archive" t))
         (file (expand-file-name "tasks.org" root))
         (archive (expand-file-name "archive.org" root))
         (events nil)
         (org-log-refile 'time)
         (org-log-into-drawer "LOGBOOK")
         (org-archive-location (concat archive "::* Archive"))
         (org-archive-stamp-time nil)
         (org-archive-subtree-add-inherited-tags t)
         (org-archive-save-context-info
          '(file olpath category todo itags ltags))
         (org-after-refile-insert-hook
          (list (lambda ()
                  (push (list (org-get-heading t t t t)
                              (org-current-level)
                              (org-get-tags nil t))
                        events)))))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+CATEGORY: Flow\n")
            (insert "* Inbox :inbox:\n")
            (insert "** TODO Alpha :work:\nAlpha body\n")
            (insert "** DONE Beta :work:\nBeta body\n")
            (insert "* Projects :project:\n")
            (insert "** Target :client:\nTarget body\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (let ((target-pos
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward "Target")
                     (line-beginning-position))))
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (org-refile nil nil (list "Target" file nil target-pos))
              (goto-char (point-min))
              (search-forward "Beta")
              (beginning-of-line)
              (org-refile '(4) nil (list "Target" file nil target-pos))
              (goto-char (point-min))
              (search-forward "Beta")
              (beginning-of-line)
              (let ((copied-beta
                     (buffer-substring-no-properties
                      (line-beginning-position)
                      (save-excursion (org-end-of-subtree t t)))))
                (org-archive-subtree)
                (save-buffer)
                (let ((source
                       (replace-regexp-in-string
                        "- Refiled on \\[.*\\]"
                        "- Refiled on [stamp]"
                        (buffer-substring-no-properties
                         (point-min) (point-max))))
                      (archived
                       (with-current-buffer (find-file-noselect archive)
                         (replace-regexp-in-string
                          (regexp-quote root)
                          "<root>"
                          (buffer-substring-no-properties
                           (point-min) (point-max))))))
                  (list (nreverse events)
                        (replace-regexp-in-string
                         "- Refiled on \\[.*\\]"
                         "- Refiled on [stamp]"
                         copied-beta)
                        source
                        archived
                        (org-map-entries
                         (lambda ()
                           (list (org-get-heading t t t t)
                                 (org-current-level)
                                 (org-get-todo-state)
                                 (org-get-tags nil t)
                                 (org-entry-get nil "ARCHIVE_TODO")
                                 (org-entry-get nil "ARCHIVE_CATEGORY")))
                         nil nil)))))))
      (dolist (buf (list (get-file-buffer file)
                         (get-file-buffer archive)))
        (when buf (kill-buffer buf)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_refile_completion_new_parent_verify_history_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp \"Inbox/\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let* ((root (make-temp-file "org-refile-complete" t))
         (file (expand-file-name "targets.org" root))
         (org-refile-targets `((,file . (:maxlevel . 3))))
         (org-refile-use-outline-path t)
         (org-outline-path-complete-in-steps t)
         (org-refile-allow-creating-parent-nodes 'confirm)
         (org-refile-history nil)
         prompts answers)
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Inbox\n")
            (insert "* Projects :target:\n")
            (insert "** Skip :skip:\n*** Hidden target\n")
            (insert "** Keep :target:\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (let ((org-refile-target-verify-function
                   (lambda ()
                     (let ((tags (org-get-tags nil t)))
                       (cond
                        ((member "skip" tags)
                         (org-end-of-subtree t t)
                         nil)
                        ((or (= (org-current-level) 1)
                             (member "target" tags))
                         t)
                        (t nil))))))
              (cl-letf (((symbol-function 'completing-read)
                         (lambda (prompt collection &rest _)
                           (push (list prompt
                                       (sort
                                        (mapcar #'car
                                                (if (functionp collection)
                                                    (all-completions
                                                     "" collection)
                                                  collection))
                                        #'string<))
                                 prompts)
                           (pop answers)))
                        ((symbol-function 'y-or-n-p)
                         (lambda (prompt)
                           (push prompt prompts)
                           t)))
                (setq answers '("Projects/Keep/New child"))
                (let ((new-target
                       (org-refile-get-location "Move to" nil
                                                org-refile-allow-creating-parent-nodes))
                      (after-new (buffer-substring-no-properties
                                  (point-min) (point-max))))
                  (setq answers '("Projects/Keep/New child"))
                  (let ((existing
                         (org-refile-get-location "Again" nil nil)))
                    (list (list (car new-target)
                                (file-relative-name (nth 1 new-target) root)
                                (nth 2 new-target)
                                (not (null (nth 3 new-target))))
                          (list (car existing)
                                (file-relative-name (nth 1 existing) root)
                                (nth 2 existing)
                                (not (null (nth 3 existing))))
                          (nreverse prompts)
                          org-refile-history
                          after-new
                          (buffer-substring-no-properties
                           (point-min) (point-max)))))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_refile_targets_outline_path_markers_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let* ((file (make-temp-file "org-refile-deep" nil ".org"
                               "* Inbox\n** TODO Task one\nbody one\n** TODO Task two\nbody two\n* Project\n** Active\n*** Sub A\n*** Sub B\n* Archive\n")))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (let* ((org-refile-targets `((,file :maxlevel . 3)))
                 (org-refile-use-outline-path t)
                 (org-outline-path-complete-in-steps nil)
                 ;; Get refile targets with deep state
                 (targets
                  (org-refile-get-targets))
                 ;; Get outline paths
                 (outline-paths
                  (mapcar (lambda (tgt)
                            (list (car tgt)
                                  (cadr tgt)
                                  (caddr tgt)))
                          targets))
                 ;; Snapshot before refile
                 (before (buffer-substring-no-properties
                          (point-min) (point-max)))
                 (headings-before
                  (org-element-map (org-element-parse-buffer) 'headline
                    (lambda (h)
                      (list (org-element-property :level h)
                            (org-element-property :raw-value h)
                            (org-element-property :todo-keyword h)))))
                 ;; Refile Task one to Sub A
                 (target-pos
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "Sub A")
                    (line-beginning-position))))
            (goto-char (point-min))
            (search-forward "Task one")
            (beginning-of-line)
            (org-refile nil nil (list "Sub A" file nil target-pos))
            (save-buffer)
            (let ((after-refile (buffer-substring-no-properties
                                 (point-min) (point-max)))
                  (headings-after
                   (org-element-map (org-element-parse-buffer) 'headline
                     (lambda (h)
                       (list (org-element-property :level h)
                             (org-element-property :raw-value h)
                             (org-element-property :todo-keyword h)))))
                  (sub-a-content
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward "Sub A")
                     (let ((beg (point)))
                       (org-end-of-subtree)
                       (buffer-substring-no-properties beg (point))))))
              (list outline-paths
                    headings-before
                    before
                    headings-after
                    sub-a-content
                    after-refile)))
          (kill-buffer))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_refile_active_region_reverse_order_log_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Loose note line\" \"Task A\") \"org-refile-last-stored\" \"* Inbox\\n\\n** TODO Task B :inbox:\\nTask B body\\n* Projects\\n** Target\\n*** Existing child\\n** Loose note line\\nContinued context\\n*** TODO Task A                                                       :inbox:\\nTask body\\n\")""#
    ]];
    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let* ((file (make-temp-file "org-refile-region" nil ".org"
                               "* Inbox
Loose note line
Continued context
** TODO Task A :inbox:
Task body
** TODO Task B :inbox:
Task B body
* Projects
** Target
*** Existing child
"))
         (events nil))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (let ((org-refile-active-region-within-subtree t)
                (org-log-refile 'time)
                (org-log-into-drawer t)
                (org-reverse-note-order nil)
                (org-after-refile-insert-hook
                 (list (lambda ()
                         (push (org-get-heading t t t t) events)))))
            (let ((target-pos (save-excursion
                                (goto-char (point-min))
                                (search-forward "Target")
                                (line-beginning-position))))
              (goto-char (point-min))
              (search-forward "Loose note line")
              (beginning-of-line)
              (let ((beg (point)))
                (search-forward "Continued context")
                (end-of-line)
                (transient-mark-mode 1)
                (set-mark beg)
                (activate-mark)
                (org-refile nil nil (list "Target" file nil target-pos)))
              (goto-char (point-min))
              (search-forward "Task A")
              (beginning-of-line)
              (org-refile-reverse
               nil nil (list "Target" file nil target-pos) "Reverse")
              (save-buffer)
              (list (mapcar #'neovm--oracle-coalesce-string-properties
                            (nreverse events))
                    (plist-get org-bookmark-names-plist :last-refile)
                    (replace-regexp-in-string
                     "- Refiled on \\[.*\\]"
                     "- Refiled on [stamp]"
                     (buffer-substring-no-properties
                      (point-min) (point-max)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"##,
        expect,
    );
}

#[test]
fn org_refile_multi_target_edit_refile_back_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 54 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let* ((root (make-temp-file "org-refile-multi-" t))
         (file-a (expand-file-name "a.org" root))
         (file-b (expand-file-name "b.org" root))
         (org-refile-targets `((,file-b :maxlevel . 2))))
    (unwind-protect
        (progn
          (with-temp-file file-a
            (insert "* Source heading\n")
            (insert "Body of source.\n\n")
            (insert "* Another source\n")
            (insert "Another body.\n"))
          (with-temp-file file-b
            (insert "* Target heading\n")
            (insert "** Sub target\n"))
          (let* ((buf-a (find-file-noselect file-a))
                 (buf-b (find-file-noselect file-b)))
            ;; Refile source heading to file-b
            (with-current-buffer buf-a
              (org-mode)
              (goto-char (point-min))
              (search-forward "Source heading")
              (beginning-of-line)
              (org-refile nil nil (list "Target heading" file-b nil nil)))
            ;; Read both files
            (let ((a-after1 (with-current-buffer buf-a
                              (buffer-substring-no-properties
                               (point-min) (point-max))))
                  (b-after1 (with-current-buffer buf-b
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
              ;; Edit in b: add a sub-heading
              (with-current-buffer buf-b
                (goto-char (point-max))
                (insert "*** New under target\nNew body.\n"))
              ;; Refile another source to file-b
              (with-current-buffer buf-a
                (goto-char (point-min))
                (search-forward "Another source")
                (beginning-of-line)
                (org-refile nil nil (list "Target heading" file-b nil nil)))
              (let ((a-after2 (with-current-buffer buf-a
                                (buffer-substring-no-properties
                                 (point-min) (point-max))))
                    (b-after2 (with-current-buffer buf-b
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                (list a-after1 b-after1 a-after2 b-after2))))))
      (dolist (f (list file-a file-b))
        (when (get-file-buffer f) (kill-buffer (get-file-buffer f)))
        (when (file-exists-p f) (delete-file f)))
      (delete-directory root t))))"##,
        expect,
    );
}
