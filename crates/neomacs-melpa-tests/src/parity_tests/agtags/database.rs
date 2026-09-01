use expect_test::expect;

use super::ParityBatchCase;

/// A user opens a C project, binds agtags to their own prefix key, and builds
/// the tag database over a stale one left by an earlier checkout.  In an
/// unindexed tree agtags offers no xref backend at all; a `GTAGS' file of any
/// kind switches it on, stale or not, because `agtags--is-active' asks only
/// whether the file is regular.  `agtags-update-tags' then removes exactly the
/// three files GNU GLOBAL owns — the neighbouring `GSYMS' survives — runs
/// `gtags -i' in the directory the user chose, and drops the tag history and
/// the completion cache so nothing from the old database can be offered.
fn agtags_update_tags_replaces_a_stale_database_and_activates_the_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "agtags_update_tags_replaces_a_stale_database_and_activates_the_backend",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-database-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       (agtags-key-prefix "C-c g")
       result)
  (unwind-protect
      (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/parser.c" root))))
        (with-current-buffer buffer
          (agtags-mode 1))
        (agtags-bind-keys)
        (let* ((describe-database
                (lambda ()
                  (mapcar
                   (lambda (name)
                     (let ((file (expand-file-name name root)))
                       (list name
                             (and (file-regular-p file) t)
                             (and (string-match-p
                                   "stale"
                                   (neomacs-agtags-test-file-string file))
                                  t))))
                   agtags-created-tag-files)))
               (describe-state
                (lambda ()
                  (with-current-buffer buffer
                    (list (agtags--parse-root)
                          (agtags-xref--backend)
                          (funcall describe-database)))))
               ;; Nothing indexed yet: agtags must offer no backend at all.
               (untagged (funcall describe-state))
               (stale
                (progn
                  ;; A database left by an earlier checkout, plus a file
                  ;; agtags does not own and must not remove.
                  (dolist (name agtags-created-tag-files)
                    (neomacs-agtags-test-write-file
                     (expand-file-name name root)
                     "stale database from an earlier checkout\n"))
                  (neomacs-agtags-test-write-file
                   (expand-file-name "GSYMS" root)
                   "not one of agtags-created-tag-files\n")
                  (setq agtags--history-list '("stale-query")
                        agtags--global-to-list-cache '("stale-key" "stale-result"))
                  (funcall describe-state)))
               (mark (neomacs-agtags-test-messages-point)))
          (cl-letf (((symbol-function 'read-directory-name)
                     (lambda (&rest _arguments) root)))
            (with-current-buffer buffer
              (agtags-update-tags)))
          (setq result
                (list untagged
                      stale
                      (neomacs-agtags-test-messages-since mark)
                      (funcall describe-database)
                      (and (file-regular-p (expand-file-name "GSYMS" root)) t)
                      (list agtags--history-list agtags--global-to-list-cache)
                      (with-current-buffer buffer
                        (list (agtags-xref--backend)
                              (and (memq 'agtags--auto-update before-save-hook) t)
                              (and (memq 'agtags-xref--backend xref-backend-functions) t)
                              (and (memq #'agtags--completion-at-point
                                         completion-at-point-functions)
                                   t)))
                      (mapcar (lambda (suffix)
                                (list suffix
                                      (lookup-key (current-global-map)
                                                  (kbd (concat "C-c g " suffix)))))
                              '("q" "b" "f" "F" "t" "r" "p" "g"))
                      (neomacs-agtags-test-trace tools)))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (("[ORACLE-SANDBOX]/agtags-database-workflow/" nil (("GPATH" nil nil) ("GTAGS" nil nil) ("GRTAGS" nil nil))) ("[ORACLE-SANDBOX]/agtags-database-workflow/" agtags (("GPATH" t t) ("GTAGS" t t) ("GRTAGS" t t))) "Tags create successed: [ORACLE-SANDBOX]/agtags-database-workflow/\n" (("GPATH" t nil) ("GTAGS" t nil) ("GRTAGS" t nil)) t (nil nil) (agtags t t t) (("q" agtags-switch-dwim) ("b" agtags-update-tags) ("f" agtags-open-file) ("F" agtags-find-file) ("t" agtags-find-tag) ("r" agtags-find-rtag) ("p" agtags-find-with-string) ("g" agtags-find-with-pattern)) "gtags cwd=[ORACLE-SANDBOX]/agtags-database-workflow <-i>\n")"#
        ]],
    )
}

fn agtags_reports_failure_and_stays_inert_when_gnu_global_is_not_installed() -> ParityBatchCase {
    ParityBatchCase::value(
        "agtags_reports_failure_and_stays_inert_when_gnu_global_is_not_installed",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "agtags-missing-global"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root)
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-cleanup root)
        (neomacs-agtags-test-make-project root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/main.c" root))))
          (with-current-buffer buffer
            (agtags-mode 1)
            (goto-char (point-min))
            (search-forward "parser_init"))
          (let ((mark (neomacs-agtags-test-messages-point)))
            (cl-letf (((symbol-function 'read-directory-name)
                       (lambda (&rest _arguments) root)))
              (with-current-buffer buffer
                (agtags-update-tags)))
            (let ((failure (neomacs-agtags-test-messages-since mark)))
              (cl-letf (((symbol-function 'completing-read)
                         (lambda (&rest _arguments) "parser_reset"))
                        ((symbol-function 'read-from-minibuffer)
                         (lambda (&rest _arguments) "parser_reset")))
                (with-current-buffer buffer
                  (agtags-find-tag)
                  (agtags-find-file)))
              (setq result
                    (list (list (executable-find "gtags") (executable-find "global"))
                          failure
                          (directory-files root)
                          (with-current-buffer buffer
                            (let ((capf (agtags--completion-at-point)))
                              (list (agtags--parse-root)
                                    (agtags--is-active (agtags--parse-root))
                                    (agtags-xref--backend)
                                    (buffer-substring-no-properties (nth 0 capf) (nth 1 capf))
                                    (all-completions "parse" (nth 2 capf))
                                    (agtags--run-global-to-list '("-c" "parse")))))
                          (mapcar (lambda (name) (and (get-buffer name) t))
                                  '("*agtags-grep*" "*agtags-path*"))))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK ((nil nil) "Tags create failed: [ORACLE-SANDBOX]/agtags-missing-global/\n" ("." ".." ".git" "docs" "include" "src") ("[ORACLE-SANDBOX]/agtags-missing-global/" nil nil "parser_init" nil nil) (nil nil))"#
        ]],
    )
}

pub(super) fn database_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agtags_update_tags_replaces_a_stale_database_and_activates_the_backend(),
        agtags_reports_failure_and_stays_inert_when_gnu_global_is_not_installed(),
    ]
}
