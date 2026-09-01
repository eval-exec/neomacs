use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_attach_git_default_repo_copy_delete_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"at/tach-git-default\" (\"source.txt\") (\"Synchronized attachments\" \"at/tach-git-default/source.txt\") \"\" nil (\"Synchronized attachments\" \"D\tat/tach-git-default/source.txt\") \"\" \"* Task                                                               :ATTACH:\\n:PROPERTIES:\\n:ID: attach-git-default\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'org-attach-git)
  (let* ((root (make-temp-file "org-attach-git" t))
         (org-file (expand-file-name "notes.org" root))
         (source (expand-file-name "source.txt" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-git-dir 'default)
         (org-attach-git-annex-cutoff nil)
         (org-attach-store-link-p nil)
         (org-attach-after-change-hook '(org-attach-git-commit)))
    (unwind-protect
        (progn
          (make-directory org-attach-id-dir t)
          (let ((default-directory org-attach-id-dir))
            (call-process "git" nil nil nil "init" "-q")
            (call-process "git" nil nil nil "config" "user.email" "org@example.invalid")
            (call-process "git" nil nil nil "config" "user.name" "Org Oracle"))
          (with-temp-file source (insert "payload\n"))
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: attach-git-default\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (org-attach-attach source nil 'cp)
            (let* ((dir (org-attach-dir))
                   (after-copy-files (sort (org-attach-file-list dir) #'string<))
                   (after-copy-log
                    (let ((default-directory org-attach-id-dir))
                      (split-string
                       (shell-command-to-string
                        "git log --format=%s --name-only --diff-filter=AM")
                       "\n" t)))
                   (status-after-copy
                    (let ((default-directory org-attach-id-dir))
                      (shell-command-to-string "git status --short"))))
              (org-attach-delete-one "source.txt")
              (let ((after-delete-files (sort (org-attach-file-list dir) #'string<))
                    (after-delete-log
                     (let ((default-directory org-attach-id-dir))
                       (split-string
                        (shell-command-to-string
                         "git log --format=%s --name-status --diff-filter=DM")
                        "\n" t)))
                    (status-after-delete
                     (let ((default-directory org-attach-id-dir))
                       (shell-command-to-string "git status --short"))))
                (list (file-relative-name dir org-attach-id-dir)
                      after-copy-files
                      after-copy-log
                      status-after-copy
                      after-delete-files
                      after-delete-log
                      status-after-delete
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_git_individual_repo_buffer_sync_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"repo\" nil (\".git\" \"payload.md\") (\"fatal: your current branch 'master' does not have any commits yet\") (\".git\") (\"fatal: your current branch 'master' does not have any commits yet\") \"\" \"* Parent                                                             :ATTACH:\\n:PROPERTIES:\\n:DIR: <root>/repo\\n:END:\\n** Child\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'org-attach-git)
  (let* ((root (make-temp-file "org-attach-git-ind" t))
         (repo (expand-file-name "repo" root))
         (org-file (expand-file-name "notes.org" root))
         (org-attach-git-dir 'individual-repository)
         (org-attach-git-annex-cutoff nil)
         (org-attach-store-link-p nil)
         (org-attach-after-change-hook '(org-attach-git-commit)))
    (unwind-protect
        (progn
          (make-directory repo t)
          (let ((default-directory repo))
            (call-process "git" nil nil nil "init" "-q")
            (call-process "git" nil nil nil "config" "user.email" "org@example.invalid")
            (call-process "git" nil nil nil "config" "user.name" "Org Oracle"))
          (with-temp-file org-file
            (insert "* Parent\n")
            (insert ":PROPERTIES:\n:DIR: " repo "\n:END:\n")
            (insert "** Child\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (search-forward "Parent")
            (let ((payload (get-buffer-create "payload.md")))
              (with-current-buffer payload
                (erase-buffer)
                (insert "# Payload\n\nBody\n"))
              (org-attach-buffer "payload.md")
              (let* ((dir (org-attach-dir))
                     (use-annex (org-attach-git-use-annex))
                     (files-after-add (sort (org-attach-file-list dir) #'string<))
                     (log-after-add
                      (let ((default-directory repo))
                        (split-string
                         (shell-command-to-string
                          "git log --format=%s --name-only --diff-filter=AM")
                         "\n" t))))
                (delete-file (expand-file-name "payload.md" dir))
                (org-attach-sync)
                (let ((files-after-sync (sort (org-attach-file-list dir) #'string<))
                      (log-after-sync
                       (let ((default-directory repo))
                         (split-string
                          (shell-command-to-string
                           "git log --format=%s --name-status --diff-filter=DM")
                          "\n" t)))
                      (status-after-sync
                       (let ((default-directory repo))
                         (shell-command-to-string "git status --short"))))
                  (list (file-relative-name dir root)
                        use-annex
                        files-after-add
                        log-after-add
                        files-after-sync
                        log-after-sync
                        status-after-sync
                        (replace-regexp-in-string
                         (regexp-quote root)
                         "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))))))
      (when (get-buffer "payload.md") (kill-buffer "payload.md"))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_git_annex_detection_and_open_hook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (error \"File <root>/data/an/nex-id/big.bin stored in git annex but unavailable\") \"data/an/nex-id/big.bin\" (\"org-attach-git-annex-get-maybe\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'org-attach-git)
  (let* ((root (make-temp-file "org-attach-annex" t))
         (repo (expand-file-name "data" root))
         (org-file (expand-file-name "notes.org" root))
         (org-attach-id-dir repo)
         (org-attach-git-dir 'default)
         (org-attach-git-annex-cutoff 1)
         (org-attach-git-annex-auto-get nil))
    (unwind-protect
        (progn
          (make-directory (expand-file-name ".git/annex" repo) t)
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: annex-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (let* ((dir (org-attach-dir t))
                   (file (expand-file-name "big.bin" dir)))
              (make-directory dir t)
              (with-temp-file file (insert "content\n"))
              (list (org-attach-git-use-annex)
                    (condition-case err
                        (progn
                          (org-attach-git-annex-get-maybe file)
                          'ok)
                      (error
                       (cons (car err)
                             (mapcar (lambda (part)
                                       (if (stringp part)
                                           (replace-regexp-in-string
                                            (regexp-quote root)
                                            "<root>"
                                            part)
                                         part))
                                     (cdr err)))))
                    (file-relative-name file root)
                    (sort (mapcar #'symbol-name org-attach-open-hook)
                          #'string<)))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_attach_dir_file_list_sync_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t \"<root>/data/fi/xed-attach-id\" (\"doc.txt\" \"img.png\") \"document content\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-deep" t))
         (org-file (expand-file-name "task.org" root))
         (org-attach-id-dir (expand-file-name "data" root)))
    (unwind-protect
        (progn
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: fixed-attach-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-min))
            (let* ((dir (org-attach-dir t))
                   (_ (make-directory dir t))
                   (_ (with-temp-file (expand-file-name "doc.txt" dir)
                        (insert "document content")))
                   (_ (with-temp-file (expand-file-name "img.png" dir)
                        (insert "image data")))
                   (files (sort (mapcar #'file-name-nondirectory
                                        (org-attach-file-list dir))
                                #'string<))
                   (doc-content
                    (with-temp-buffer
                      (insert-file-contents
                       (expand-file-name "doc.txt" dir))
                      (buffer-string)))
                   (dir-exists (file-directory-p dir))
                   (dir-path (replace-regexp-in-string
                              (regexp-quote root) "<root>" dir)))
              (kill-buffer)
              (list dir-exists dir-path files doc-content))))
      (delete-directory root t))))"##,
        expect,
    );
}
