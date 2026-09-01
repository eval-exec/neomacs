use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_attach_copy_buffer_delete_sync_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"data/at/tach-fixed-id\" (\"payload.txt\" \"source.txt\") (\"payload.txt\") \"from source\\n\" \"from buffer\\n\" ((\"attachment:source.txt\" \"source.txt\")) (\"data/at/tach-fixed-id\" \"data/at/tach-fixed-id\" \"data/at/tach-fixed-id\" \"data/at/tach-fixed-id\") nil \"* Task\\n:PROPERTIES:\\n:ID: attach-fixed-id\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-copy" t))
         (org-file (expand-file-name "notes.org" root))
         (source (expand-file-name "source.txt" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-store-link-p 'attached)
         (org-attach-auto-tag "ATTACH")
         (org-stored-links nil)
         (events nil)
         (org-attach-after-change-hook
          (list (lambda (dir)
                  (push (file-relative-name dir root) events)))))
    (unwind-protect
        (progn
          (with-temp-file source (insert "from source\n"))
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: attach-fixed-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (let ((payload (get-buffer-create "payload.txt")))
              (with-current-buffer payload
                (erase-buffer)
                (insert "from buffer\n"))
              (org-attach-attach source nil 'cp)
              (org-attach-buffer "payload.txt")
              (let* ((dir (org-attach-dir))
                     (files-after-add (sort (org-attach-file-list dir) #'string<))
                     (source-content
                      (with-temp-buffer
                        (insert-file-contents (expand-file-name "source.txt" dir))
                        (buffer-string)))
                     (payload-content
                      (with-temp-buffer
                        (insert-file-contents (expand-file-name "payload.txt" dir))
                        (buffer-string))))
                (org-attach-delete-one "source.txt")
                (let ((files-after-delete (sort (org-attach-file-list dir) #'string<)))
                  (delete-file (expand-file-name "payload.txt" dir))
                  (let ((org-attach-sync-delete-empty-dir nil))
                    (org-attach-sync))
                  (list (file-relative-name dir root)
                        files-after-add
                        files-after-delete
                        source-content
                        payload-content
                        org-stored-links
                        (sort events #'string<)
                        (org-get-tags nil t)
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))
      (when (get-buffer "payload.txt") (kill-buffer "payload.txt"))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_dir_inheritance_expand_links_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"shared\" \"shared/doc.txt\" \"attachment:doc.txt\" \"* Parent\\n:PROPERTIES:\\n:DIR: <root>/shared\\n:END:\\n** Child\\n[[file:<root>/shared/doc.txt][Doc]] and [[file:<root>/shared/missing.txt]]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-dir" t))
         (org-file (expand-file-name "notes.org" root))
         (attach-dir (expand-file-name "shared" root))
         (org-attach-use-inheritance t))
    (unwind-protect
        (progn
          (make-directory attach-dir)
          (with-temp-file (expand-file-name "doc.txt" attach-dir)
            (insert "attached document\n"))
          (with-temp-file org-file
            (insert "* Parent\n")
            (insert ":PROPERTIES:\n:DIR: " attach-dir "\n:END:\n")
            (insert "** Child\n")
            (insert "[[attachment:doc.txt][Doc]] and [[attachment:missing.txt]]\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (search-forward "** Child")
            (beginning-of-line)
            (let ((child-dir (org-attach-dir))
                  (expanded-doc (file-relative-name
                                 (org-attach-expand "doc.txt")
                                 root))
                  (complete-link
                   (cl-letf (((symbol-function 'read-file-name)
                              (lambda (&rest _)
                                (expand-file-name "doc.txt" attach-dir))))
                     (org-attach-complete-link))))
              (org-attach-expand-links nil)
              (list (file-relative-name child-dir root)
                    expanded-doc
                    complete-link
                    (replace-regexp-in-string
                     (regexp-quote root)
                     "<root>"
                     (buffer-substring-no-properties
                      (point-min) (point-max)))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_lint_multiple_checker_report_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate NAME \\\"dup\\\"\" duplicate-name) (#(\"2\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Unknown header argument \\\":bad\\\"\" wrong-header-argument) (#(\"5\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate NAME \\\"dup\\\"\" duplicate-name) (#(\"6\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Missing language in source block\" missing-language-in-src-block) (#(\"9\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Unknown custom ID \\\"missing-custom\\\"\" invalid-custom-id-link) (#(\"9\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Link to non-existent local file \\\"no-such-file.txt\\\"\" link-to-local-file) (#(\"13\" 0 2 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Invalid effort duration format: \\\"invalid\\\"\" invalid-effort-property) (#(\"14\" 0 2 (org-lint-marker #<marker in no buffer>)) \"nil\" \"IDs should not include \\\"::\\\": \\\"bad::id\\\"\" invalid-id-property))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-lint)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: dup\n")
    (insert "#+begin_src emacs-lisp :bad yes\n")
    (insert "(+ 1 2)\n")
    (insert "#+end_src\n")
    (insert "#+NAME: dup\n")
    (insert "#+begin_src\n")
    (insert "body\n")
    (insert "#+end_src\n")
    (insert "[[coderef:missing]] [[#missing-custom]] [[file:no-such-file.txt]]\n")
    (insert "[fn:lost]\n")
    (insert "* H\n:PROPERTIES:\n:EFFORT: invalid\n:ID: bad::id\n:END:\n")
    (let ((reports (org-lint
                    '(duplicate-name
                      missing-language-in-src-block
                      invalid-coderef-link
                      invalid-custom-id-link
                      link-to-local-file
                      undefined-footnote-reference
                      invalid-effort-property
                      invalid-id-property
                      wrong-header-argument))))
      (mapcar (lambda (entry)
                (let ((row (cadr entry)))
                  (list (aref row 0)
                        (aref row 1)
                        (aref row 2)
                        (org-lint-checker-name (aref row 3)))))
              reports))))"##,
        expect,
    );
}

#[test]
fn org_lint_custom_category_marker_report_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate target <<dup-target>>\" duplicate-target \"Report duplicate targets\" nil (link) 0) (#(\"2\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate target <<dup-target>>\" duplicate-target \"Report duplicate targets\" nil (link) 15) (#(\"8\" 0 1 (org-lint-marker #<marker in no buffer>)) \"high\" \"custom headline Two\" combo-custom \"new custom checker\" high (combo structure) 107) (#(\"9\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Possible incomplete drawer \\\":PROPERTIES:\\\"\" incomplete-drawer \"Report probable incomplete drawers\" low nil 113)) ((#(\"8\" 0 1 (org-lint-marker #<marker in no buffer>)) \"high\" \"custom headline Two\" combo-custom \"new custom checker\" high (combo structure) 107)) (combo-custom) \"<<dup-target>>\\n<<dup-target>>\\n* One\\n:PROPERTIES:\\n:CUSTOM_ID: same\\n:END:\\n[[#missing-custom]] [[dup-target]]\\n* Two\\n:PROPERTIES:\\n:CUSTOM_ID: same\\ndrawer never closes\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-lint)
  (let ((original-checkers org-lint--checkers))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (org-lint-add-checker
           'combo-custom "old custom checker"
           (lambda (_ast) nil)
           :trust 'low
           :categories '(combo old))
          (org-lint-add-checker
           'combo-custom "new custom checker"
           (lambda (ast)
             (org-element-map ast 'headline
               (lambda (h)
                 (when (string= (org-element-property :raw-value h) "Two")
                   (list (org-element-property :begin h)
                         "custom headline Two")))))
           :trust 'high
           :categories '(combo structure))
          (insert "<<dup-target>>\n")
          (insert "<<dup-target>>\n")
          (insert "* One\n")
          (insert ":PROPERTIES:\n:CUSTOM_ID: same\n:END:\n")
          (insert "[[#missing-custom]] [[dup-target]]\n")
          (insert "* Two\n")
          (insert ":PROPERTIES:\n:CUSTOM_ID: same\n")
          (insert "drawer never closes\n")
          (let* ((rows (org-lint
                        '(combo-custom
                          duplicate-custom-id
                          duplicate-target
                          invalid-fuzzy-link
                          incomplete-drawer)))
                 (combo-rows
                  (cl-letf (((symbol-function 'completing-read)
                             (lambda (&rest _) "combo")))
                    (org-lint '(4))))
                 (summarize
                  (lambda (reports)
                    (mapcar
                     (lambda (entry)
                       (let* ((row (cadr entry))
                              (line (aref row 0))
                              (checker (aref row 3))
                              (marker (get-text-property
                                       0 'org-lint-marker line)))
                         (list (aref row 0)
                               (aref row 1)
                               (aref row 2)
                               (org-lint-checker-name checker)
                               (org-lint-checker-summary checker)
                               (org-lint-checker-trust checker)
                               (org-lint-checker-categories checker)
                               (and marker
                                    (- (marker-position marker)
                                       (point-min))))))
                     reports))))
            (list (funcall summarize rows)
                  (funcall summarize combo-rows)
                  (mapcar #'org-lint-checker-name
                          (cl-remove-if-not
                           (lambda (c)
                             (memq 'combo
                                   (org-lint-checker-categories c)))
                           org-lint--checkers))
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (setq org-lint--checkers original-checkers))))"##,
        expect,
    );
}

#[test]
fn org_attach_url_set_unset_directory_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-url" t))
         (org-file (expand-file-name "notes.org" root))
         (attach-dir (expand-file-name "relative-dir" root))
         (org-attach-dir-relative t)
         (org-attach-auto-tag "ATTACH")
         (org-attach-store-link-p 'file)
         (org-safe-remote-resources '("https://example.invalid/"))
         (org-stored-links nil)
         (downloads nil)
         (events nil)
         (org-attach-after-change-hook
          (list (lambda (dir)
                  (push (file-relative-name dir root) events)))))
    (unwind-protect
        (progn
          (with-temp-file org-file
            (insert "* Download\n")
            (insert "** Child\n"))
          (make-directory attach-dir)
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (cl-letf (((symbol-function 'read-directory-name)
                       (lambda (&rest _) attach-dir))
                      ((symbol-function 'url-copy-file)
                       (lambda (url file &optional _ok-if-exists _keep-time)
                         (push (list url (file-relative-name file root)) downloads)
                         (with-temp-file file
                           (insert "downloaded from " url "\n")))))
              (let* ((set-dir (org-attach-set-directory))
                     (dir-property (org-entry-get nil "DIR"))
                     (dir-after-set (org-attach-dir))
                     (downloaded
                      (progn
                        (org-attach-url "https://example.invalid/report.txt")
                        (with-temp-buffer
                          (insert-file-contents
                           (expand-file-name "report.txt" attach-dir))
                          (buffer-string))))
                     (files-after-url (sort (org-attach-file-list attach-dir) #'string<)))
                (org-attach-unset-directory)
                (list (file-relative-name set-dir root)
                      dir-property
                      (file-relative-name dir-after-set root)
                      files-after-url
                      downloaded
                      (mapcar (lambda (link)
                                (list (replace-regexp-in-string
                                       (regexp-quote root)
                                       "<root>"
                                       (car link))
                                      (cadr link)))
                              org-stored-links)
                      (sort downloads (lambda (a b) (string< (cadr a) (cadr b))))
                      (sort events #'string<)
                      (org-get-tags nil t)
                      (org-entry-get nil "DIR")
                      (replace-regexp-in-string
                       (regexp-quote root)
                       "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
    );
}

#[test]
fn org_attach_open_follow_hooks_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"report.txt\" \"zeta.txt\") ((\"data/op/en-fixed-id/report.txt\" in-emacs) (\"data/op/en-fixed-id/zeta.txt\" (16))) (\"data/op/en-fixed-id/report.txt\") (\"ATTACH\") \"* Open target                                                        :ATTACH:\\n:PROPERTIES:\\n:ID: open-fixed-id\\n:END:\\n[[file:<root>/data/op/en-fixed-id/report.txt][Report]]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-open" t))
         (org-file (expand-file-name "notes.org" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-auto-tag "ATTACH")
         (opened nil)
         (hooked nil)
         (org-attach-open-hook
          (list (lambda (file)
                  (push (file-relative-name file root) hooked)))))
    (unwind-protect
        (progn
          (with-temp-file org-file
            (insert "* Open target\n")
            (insert ":PROPERTIES:\n:ID: open-fixed-id\n:END:\n")
            (insert "[[attachment:report.txt][Report]]\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (let ((dir (org-attach-dir 'get-create)))
              (with-temp-file (expand-file-name "report.txt" dir)
                (insert "report body\n"))
              (with-temp-file (expand-file-name "zeta.txt" dir)
                (insert "zeta body\n"))
              (org-attach-sync)
              (cl-letf (((symbol-function 'completing-read)
                         (lambda (&rest _) "report.txt"))
                        ((symbol-function 'org-open-file)
                         (lambda (path &optional arg &rest _)
                           (push (list (file-relative-name path root) arg) opened)
                           path)))
                (org-attach-open-in-emacs)
                (org-attach-follow "zeta.txt" '(16))
                (org-attach-expand-links nil)
                (list (sort (org-attach-file-list dir) #'string<)
                      (sort opened (lambda (a b) (string< (car a) (car b))))
                      (sort hooked #'string<)
                      (org-get-tags nil t)
                      (replace-regexp-in-string
                       (regexp-quote root)
                       "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_archive_delete_and_sync_empty_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"ATTACH\") (\"old.txt\") nil nil (\"ATTACH\") nil nil (\"archive-dir\" \"archive-dir\" \"sync-dir\" \"sync-dir\") \"* Archive me\\n:PROPERTIES:\\n:DIR: <root>/archive-dir\\n:END:\\n* Sync me\\n:PROPERTIES:\\n:DIR: <root>/sync-dir\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-clean" t))
         (org-file (expand-file-name "notes.org" root))
         (archive-dir (expand-file-name "archive-dir" root))
         (sync-dir (expand-file-name "sync-dir" root))
         (org-attach-auto-tag "ATTACH")
         (org-attach-archive-delete t)
         (events nil)
         (org-attach-after-change-hook
          (list (lambda (dir)
                  (push (file-relative-name dir root) events)))))
    (unwind-protect
        (progn
          (make-directory archive-dir)
          (make-directory sync-dir)
          (with-temp-file (expand-file-name "old.txt" archive-dir)
            (insert "old\n"))
          (with-temp-file (expand-file-name "sync.txt" sync-dir)
            (insert "sync\n"))
          (with-temp-file org-file
            (insert "* Archive me\n")
            (insert ":PROPERTIES:\n:DIR: " archive-dir "\n:END:\n")
            (insert "* Sync me\n")
            (insert ":PROPERTIES:\n:DIR: " sync-dir "\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (org-attach-sync)
            (let ((archive-tags-before (org-get-tags nil t))
                  (archive-files-before (sort (org-attach-file-list archive-dir) #'string<)))
              (org-attach-archive-delete-maybe)
              (let ((archive-exists-after (file-exists-p archive-dir))
                    (archive-tags-after (org-get-tags nil t)))
                (search-forward "* Sync me")
                (beginning-of-line)
                (org-attach-sync)
                (let ((sync-tags-before (org-get-tags nil t)))
                  (delete-file (expand-file-name "sync.txt" sync-dir))
                  (let ((org-attach-sync-delete-empty-dir t))
                    (org-attach-sync))
                  (list archive-tags-before
                        archive-files-before
                        archive-exists-after
                        archive-tags-after
                        sync-tags-before
                        (file-exists-p sync-dir)
                        (org-get-tags nil t)
                        (sort events #'string<)
                        (replace-regexp-in-string
                         (regexp-quote root)
                         "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_new_delete_all_id_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-new" t))
         (org-file (expand-file-name "notes.org" root))
         (source (expand-file-name "source.txt" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-preferred-new-method 'id)
         (org-attach-store-link-p 'attached)
         (org-attach-auto-tag "ATTACH")
         (org-stored-links nil)
         (events nil)
         (new-buffer nil)
         (org-attach-after-change-hook
          (list (lambda (dir)
                  (push (list 'hook
                              (file-relative-name dir root)
                              (and (file-directory-p dir)
                                   (sort (org-attach-file-list dir) #'string<)))
                        events)))))
    (unwind-protect
        (progn
          (with-temp-file source
            (insert "source body\n"))
          (with-temp-file org-file
            (insert "#+FILETAGS: :global:\n")
            (insert "* TODO Attach lifecycle :work:\n")
            (insert ":PROPERTIES:\n:ID: attach-new-fixed\n:END:\n")
            (insert "See [[attachment:source.txt][source]].\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (search-forward "* TODO")
            (beginning-of-line)
            (let* ((org-buffer (current-buffer))
                   (no-fs-dir (org-attach-dir nil 'no-fs-check))
                   (existing-before (org-attach-dir))
                   (tags-before (org-get-tags nil t)))
              (org-attach-attach source nil 'cp)
              (let* ((dir (org-attach-dir))
                     (after-attach-files
                      (sort (org-attach-file-list dir) #'string<))
                     (after-attach-tags (org-get-tags nil t))
                     (links-after-attach org-stored-links)
                     (source-inside
                      (with-temp-buffer
                        (insert-file-contents (expand-file-name "source.txt" dir))
                        (buffer-string))))
                (org-attach-new "draft-note.org")
                (setq new-buffer (current-buffer))
                (insert "#+TITLE: Draft\n\n* Nested\nBody from new buffer.\n")
                (save-buffer)
                (let ((new-file (file-relative-name (buffer-file-name) root))
                      (new-buffer-name (buffer-name)))
                  (with-current-buffer org-buffer
                    (org-attach-sync)
                    (let* ((after-new-files
                            (sort (org-attach-file-list dir) #'string<))
                           (after-sync-tags (org-get-tags nil t))
                           (draft-inside
                            (with-temp-buffer
                              (insert-file-contents
                               (expand-file-name "draft-note.org" dir))
                              (buffer-string))))
                      (org-attach-delete-all t)
                      (let ((dir-exists-after-delete (file-exists-p dir))
                            (tags-after-delete (org-get-tags nil t)))
                        (org-attach-sync)
                        (list (file-relative-name no-fs-dir root)
                              existing-before
                              tags-before
                              after-attach-files
                              after-attach-tags
                              links-after-attach
                              source-inside
                              new-file
                              new-buffer-name
                              after-new-files
                              draft-inside
                              after-sync-tags
                              dir-exists-after-delete
                              tags-after-delete
                              (org-get-tags nil t)
                              (nreverse events)
                              (replace-regexp-in-string
                               (regexp-quote root)
                               "<root>"
                               (buffer-substring-no-properties
                                (point-min) (point-max)))))))))))
      (when (and new-buffer (buffer-live-p new-buffer))
        (kill-buffer new-buffer))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_lint_include_macro_planning_percent_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"1\" \"low\" \"Non-existent setup file \\\"<root>/missing-setup.org\\\"\" non-existent-setupfile-parameter) (\"2\" \"low\" \"Missing value for option item \\\"missing\\\"\" unknown-options-item) (\"2\" \"low\" \"Unknown OPTIONS item \\\"missing\\\"\" unknown-options-item) (\"2\" \"low\" \"Missing value for option item \\\"bad-option\\\"\" unknown-options-item) (\"2\" \"low\" \"Unknown OPTIONS item \\\"bad-option\\\"\" unknown-options-item) (\"2\" \"low\" \"Missing value for option item \\\"toc\\\"\" unknown-options-item) (\"3\" \"low\" \"Invalid search part \\\"* Missing\\\" in INCLUDE keyword\" wrong-include-link-parameter) (\"4\" \"low\" \"Obsolete markup \\\"HTML\\\" in INCLUDE keyword.  Use \\\"export HTML\\\" instead\" obsolete-include-markup) (\"5\" \"low\" \"Missing name in MACRO keyword\" invalid-macro-argument-and-template) (\"6\" \"low\" \"Missing template in macro \\\"%s\\\"\" invalid-macro-argument-and-template) (\"7\" \"low\" \"Unused placeholders in macro \\\"pair\\\"\" invalid-macro-argument-and-template) (\"9\" \"low\" \"Different repeaters in SCHEDULED and DEADLINE timestamps.\" mismatched-planning-repeaters) (\"11\" \"low\" \"Possible indented diary-sexp\" indented-diary-sexp) (\"12\" \"low\" \"Possible incomplete block \\\"#+BEGIN_bad\\\"\" invalid-block) (\"14\" \"low\" \"Invalid block closing line \\\"#+END_bad trailing\\\"\" invalid-block) (\"15\" \"low\" \"Spurious argument in macro \\\"pair\\\": four\" invalid-macro-argument-and-template) (\"15\" \"low\" \"Undefined macro \\\"unknown\\\"\" invalid-macro-argument-and-template)) 17 (keyword keyword keyword keyword keyword keyword keyword planning link macro macro) \"#+SETUPFILE: \\\"<root>/missing-setup.org\\\"\\n#+OPTIONS: toc: bad-option: missing:\\n#+INCLUDE: \\\"<root>/inc.org::* Missing\\\" :lines \\\"bad\\\"\\n#+INCLUDE: \\\"<root>/inc.org\\\" HTML\\n#+MACRO:\\n#+MACRO: empty\\n#+MACRO: pair $1 $3\\n* TODO Task\\nSCHEDULED: <2026-05-27 Wed +1w> DEADLINE: <2026-05-28 Thu ++2d>\\n[[https://example.org/a%2Fb][bad percent]]\\n  %%(diary-date 5 27 2026)\\n#+BEGIN_bad\\nunfinished block\\n#+END_bad trailing\\n{{{pair(one,two,three,four)}}} {{{unknown()}}}\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-lint)
  (let* ((root (make-temp-file "org-lint-combo" t))
         (inc (expand-file-name "inc.org" root))
         (setup (expand-file-name "missing-setup.org" root)))
    (unwind-protect
        (progn
          (with-temp-file inc
            (insert "* Included\nBody\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+SETUPFILE: \"" setup "\"\n")
            (insert "#+OPTIONS: toc: bad-option: missing:\n")
            (insert "#+INCLUDE: \"" inc "::* Missing\" :lines \"bad\"\n")
            (insert "#+INCLUDE: \"" inc "\" HTML\n")
            (insert "#+MACRO:\n")
            (insert "#+MACRO: empty\n")
            (insert "#+MACRO: pair $1 $3\n")
            (insert "* TODO Task\n")
            (insert "SCHEDULED: <2026-05-27 Wed +1w> DEADLINE: <2026-05-28 Thu ++2d>\n")
            (insert "[[https://example.org/a%2Fb][bad percent]]\n")
            (insert "  %%(diary-date 5 27 2026)\n")
            (insert "#+BEGIN_bad\n")
            (insert "unfinished block\n")
            (insert "#+END_bad trailing\n")
            (insert "{{{pair(one,two,three,four)}}} {{{unknown()}}}\n")
            (let* ((reports
                    (org-lint
                     '(non-existent-setupfile-parameter
                       wrong-include-link-parameter
                       obsolete-include-markup
                       unknown-options-item
                       invalid-macro-argument-and-template
                       mismatched-planning-repeaters
                       misplaced-planning-info
                       indented-diary-sexp
                       invalid-block
                       invalid-keyword-syntax
                       percent-encoding-link-escape)))
                   (summary
                    (mapcar
                     (lambda (entry)
                       (let* ((row (cadr entry))
                              (line (substring-no-properties
                                     (aref row 0)))
                              (message (aref row 2))
                              (checker (aref row 3)))
                         (list line
                               (aref row 1)
                               (replace-regexp-in-string
                                (regexp-quote root) "<root>" message)
                               (org-lint-checker-name checker))))
                     reports)))
              (list summary
                    (length reports)
                    (mapcar #'org-element-type
                            (org-element-map
                                (org-element-parse-buffer)
                                '(keyword planning link macro src-block)
                              #'identity))
                    (replace-regexp-in-string
                     (regexp-quote root)
                     "<root>"
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_lint_missing_dir_stale_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (0 nil ((\"Task\" 1) (\"Other\" 1)) ((\"attachment\" \"missing-file.txt\")) \"nil\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'org-lint)
  (let* ((root (make-temp-file "org-attach-lint" t))
         (file (expand-file-name "task.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* Task\n")
            (insert ":PROPERTIES:\n:ID: lint-test-id\n:ATTACH_DIR: missing-dir\n:END:\n")
            (insert "Body with [[attachment:missing-file.txt]] link.\n\n")
            (insert "* Other\n")
            (insert ":PROPERTIES:\n:ID: other-id\n:END:\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (let* ((ast (org-element-parse-buffer))
                   (lint-reports
                    (condition-case nil
                        (org-lint ast)
                      (error nil)))
                   (attach-dir (org-attach-dir))
                   (has-missing-dir
                    (and lint-reports
                         (some (lambda (r)
                                 (string-match-p "missing-dir"
                                                  (or (nth 1 r) "")))
                               lint-reports)))
                   (headlines
                    (org-element-map ast 'headline
                      (lambda (h)
                        (list (org-element-property :raw-value h)
                              (org-element-property :level h)))))
                   (links
                    (org-element-map ast 'link
                      (lambda (lk)
                        (list (org-element-property :type lk)
                              (org-element-property :path lk))))))
              (kill-buffer)
              (list (length lint-reports)
                    has-missing-dir
                    headlines
                    links
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>"
                     (or attach-dir "nil"))))))
      (delete-directory root t))))"##,
        expect,
    );
}
