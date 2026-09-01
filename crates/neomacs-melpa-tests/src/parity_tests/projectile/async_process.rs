use expect_test::expect;

use super::ParityBatchCase;

fn projectile_async_external_indexing_reports_success_and_nonzero_exit_contracts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "projectile_async_external_indexing_reports_success_and_nonzero_exit_contracts",
        r##"(let ((root (file-name-as-directory
                         (make-temp-file "projectile-async-" t))))
               (unwind-protect
                   (cl-labels
                       ((run
                         (command)
                         (let (done files failure)
                           (projectile-files-via-ext-command-async
                            root
                            command
                            (lambda (result error)
                              (setq files result
                                    failure error
                                    done t)))
                           (let ((deadline (+ (float-time) 10)))
                             (while
                                 (and (not done)
                                      (< (float-time) deadline))
                               (accept-process-output nil 0.05)))
                           (list
                            done
                            files
                            (and failure
                                 (string-match-p
                                  "exit code 3" failure)
                                 t)
                            (and (get-buffer
                                  "*projectile-files-errors*")
                                 (with-current-buffer
                                     "*projectile-files-errors*"
                                   (string-match-p
                                    "boom"
                                    (buffer-string)))
                                 t)))))
                     (list
                      (run "printf './a.el\\0b/c.el\\0'")
                      (run "echo boom >&2; exit 3")
                      (run "printf 'kept.el\\0'; exit 1")))
                 (when (get-buffer "*projectile-files-errors*")
                   (kill-buffer "*projectile-files-errors*"))
                 (delete-directory root t)))"##,
        expect![[r#"OK ((t ("a.el" "b/c.el") nil nil) (t nil t t) (t ("kept.el") nil nil))"#]],
    )
}

pub(super) fn async_process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![projectile_async_external_indexing_reports_success_and_nonzero_exit_contracts()]
}
