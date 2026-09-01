use expect_test::expect;

use super::ParityBatchCase;

fn async_backup_predicates_receive_expanded_file_in_order_before_process_launch() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_backup_predicates_receive_expanded_file_in_order_before_process_launch",
        r##"(let* ((work
                (async-backup-test-path "predicates/work/"))
               (default-directory work)
               (file
                (async-backup-test-write-file
                 "predicates/work/input.org"
                 "* input\n"))
               (async-backup-location
                (async-backup-test-path "predicates/backups"))
               events)
          (make-directory work t)
          (let ((async-backup-predicates
                 (list
                  (lambda (candidate)
                    (push
                     (list :first candidate
                           (file-readable-p candidate))
                     events)
                    t)
                  (lambda (candidate)
                    (push
                     (list :second candidate
                           (string-suffix-p ".org" candidate))
                     events)
                    'accepted))))
            (cl-letf (((symbol-function 'format-time-string)
                       (lambda (&rest _) "PRED"))
                      ((symbol-function 'start-process)
                       (lambda (&rest command)
                         (push (list :start command) events)
                         :process)))
              (list
               (async-backup "input.org")
               (equal file
                      (expand-file-name
                       "input.org"
                       default-directory))
               (mapcar
                (lambda (event)
                  (if (eq (car event) :start)
                      (list
                       :start
                       (async-backup-test-normalize-command
                        (cadr event)))
                    (list
                     (car event)
                     (replace-regexp-in-string
                      (regexp-quote
                       (getenv
                        "NEOMACS_TEST_SANDBOX_ROOT"))
                      "$ROOT/"
                      (cadr event))
                     (nth 2 event))))
                (nreverse events))))))"##,
        expect![[
            r#"OK (:process t ((:first "$ROOT//predicates/work/input.org" t) (:second "$ROOT//predicates/work/input.org" t) (:start ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//predicates/work/input.org\" \"$ROOT//predicates/backups$ROOT//predicates/work/input-PRED.org\")"))))"#
        ]],
    )
}

fn async_backup_false_predicate_short_circuits_later_predicates_and_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_false_predicate_short_circuits_later_predicates_and_process",
        r##"(let* ((file
                (async-backup-test-write-file
                 "predicate-false/input.txt"
                 "input\n"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-false/backups"))
               events)
          (let ((async-backup-predicates
                 (list
                  (lambda (_file)
                    (push :first events)
                    nil)
                  (lambda (_file)
                    (push :should-not-run events)
                    t))))
            (cl-letf (((symbol-function 'start-process)
                       (lambda (&rest _)
                         (push :should-not-start events)
                         :unexpected)))
              (list
               (async-backup file)
               (nreverse events)
               (file-directory-p
                (concat
                 (directory-file-name
                  async-backup-location)
                 (file-name-directory file)))))))"##,
        expect!["OK (nil (:first) t)"],
    )
}

fn async_backup_empty_predicate_list_vacuously_launches_backup() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_empty_predicate_list_vacuously_launches_backup",
        r##"(let* ((file
                (async-backup-test-write-file
                 "predicate-empty/input"
                 "input\n"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-empty/backups"))
               (async-backup-predicates nil)
               captured)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "EMPTY"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :launched)))
            (list
             (async-backup file)
             (async-backup-test-normalize-command captured))))"##,
        expect![[
            r#"OK (:launched ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//predicate-empty/input\" \"$ROOT//predicate-empty/backups$ROOT//predicate-empty/input-EMPTY\")"))"#
        ]],
    )
}

fn async_backup_default_identity_predicate_accepts_any_non_nil_expanded_path() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_default_identity_predicate_accepts_any_non_nil_expanded_path",
        r##"(let* ((file
                (async-backup-test-path
                 "identity/missing-but-non-nil.txt"))
               (async-backup-location
                (async-backup-test-path
                 "identity/backups"))
               captured)
          (cl-letf (((symbol-function 'format-time-string)
                     (lambda (&rest _) "IDENTITY"))
                    ((symbol-function 'start-process)
                     (lambda (&rest command)
                       (setq captured command)
                       :launched)))
            (list
             async-backup-predicates
             (file-exists-p file)
             (async-backup file)
             (async-backup-test-normalize-command captured))))"##,
        expect![[
            r#"OK ((identity) nil :launched ("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//identity/missing-but-non-nil.txt\" \"$ROOT//identity/backups$ROOT//identity/missing-but-non-nil-IDENTITY.txt\")"))"#
        ]],
    )
}

fn async_backup_predicate_signal_propagates_after_directory_creation_without_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_predicate_signal_propagates_after_directory_creation_without_process",
        r##"(let* ((file
                (async-backup-test-write-file
                 "predicate-error/input.txt"
                 "input\n"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-error/backups"))
               started)
          (let ((async-backup-predicates
                 (list
                  (lambda (candidate)
                    (error "predicate rejected %s"
                           (file-name-nondirectory candidate))))))
            (cl-letf (((symbol-function 'start-process)
                       (lambda (&rest _)
                         (setq started t)
                         :unexpected)))
              (list
               (async-backup-test-error-data
                (lambda ()
                  (async-backup file)))
               started
               (file-directory-p
                (concat
                 (directory-file-name
                  async-backup-location)
                 (file-name-directory file)))))))"##,
        expect![[r#"OK ((:error error ("predicate rejected input.txt")) nil t)"#]],
    )
}

fn async_backup_non_function_predicate_signals_without_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_non_function_predicate_signals_without_process",
        r##"(let* ((file
                (async-backup-test-write-file
                 "predicate-type/input.txt"
                 "input\n"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-type/backups"))
               (async-backup-predicates
                (list #'identity 42 #'file-readable-p))
               started)
          (cl-letf (((symbol-function 'start-process)
                     (lambda (&rest _)
                       (setq started t)
                       :unexpected)))
            (list
             (async-backup-test-error-data
              (lambda ()
                (async-backup file)))
             started)))"##,
        expect!["OK ((:error invalid-function (42)) nil)"],
    )
}

fn async_backup_predicate_can_filter_by_real_file_size_and_extension() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_predicate_can_filter_by_real_file_size_and_extension",
        r##"(let* ((small
                (async-backup-test-write-file
                 "predicate-real/small.log"
                 "abc"))
               (large
                (async-backup-test-write-file
                 "predicate-real/large.log"
                 "0123456789abcdef"))
               (wrong-extension
                (async-backup-test-write-file
                 "predicate-real/large.txt"
                 "0123456789abcdef"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-real/backups"))
               launched)
          (let ((async-backup-predicates
                 (list
                  (lambda (file)
                    (> (file-attribute-size
                        (file-attributes file))
                       10))
                  (lambda (file)
                    (string-suffix-p ".log" file)))))
            (cl-letf (((symbol-function 'format-time-string)
                       (lambda (&rest _) "FILTER"))
                      ((symbol-function 'start-process)
                       (lambda (&rest command)
                         (push
                          (async-backup-test-normalize-command
                           command)
                          launched)
                         :started)))
              (list
               (mapcar
                #'async-backup
                (list small large wrong-extension))
               (nreverse launched)))))"##,
        expect![[
            r#"OK ((nil :started nil) (("async-backup" "*async-backup*" "emacs" "-Q" "--batch" "--eval=(copy-file \"$ROOT//predicate-real/large.log\" \"$ROOT//predicate-real/backups$ROOT//predicate-real/large-FILTER.log\")")))"#
        ]],
    )
}

fn async_backup_predicates_observe_symlink_path_and_can_reject_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_predicates_observe_symlink_path_and_can_reject_it",
        r##"(let* ((target
                (async-backup-test-write-file
                 "predicate-symlink/real.txt"
                 "real\n"))
               (link
                (async-backup-test-path
                 "predicate-symlink/link.txt"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-symlink/backups"))
               seen
               started)
          (make-symbolic-link target link)
          (let ((async-backup-predicates
                 (list
                  (lambda (file)
                    (setq seen
                          (list
                           file
                           (file-symlink-p file)
                           (file-truename file)))
                    (not (file-symlink-p file))))))
            (cl-letf (((symbol-function 'start-process)
                       (lambda (&rest _)
                         (setq started t)
                         :unexpected)))
              (list
               (async-backup link)
               (list
                (replace-regexp-in-string
                 (regexp-quote
                  (getenv
                   "NEOMACS_TEST_SANDBOX_ROOT"))
                 "$ROOT/"
                 (nth 0 seen))
                (file-name-nondirectory
                 (nth 1 seen))
                (replace-regexp-in-string
                 (regexp-quote
                  (getenv
                   "NEOMACS_TEST_SANDBOX_ROOT"))
                 "$ROOT/"
                 (nth 2 seen)))
               started))))"##,
        expect![[
            r#"OK (nil ("$ROOT//predicate-symlink/link.txt" "real.txt" "$ROOT//predicate-symlink/real.txt") nil)"#
        ]],
    )
}

fn async_backup_predicate_mutation_affects_later_predicate_in_same_call() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_backup_predicate_mutation_affects_later_predicate_in_same_call",
        r##"(let* ((file
                (async-backup-test-write-file
                 "predicate-mutation/input.txt"
                 "before\n"))
               (async-backup-location
                (async-backup-test-path
                 "predicate-mutation/backups"))
               observations
               started)
          (let ((async-backup-predicates
                 (list
                  (lambda (candidate)
                    (with-temp-file candidate
                      (insert "after\n"))
                    (push :mutated observations)
                    t)
                  (lambda (candidate)
                    (push
                     (async-backup-test-read-file candidate)
                     observations)
                    t))))
            (cl-letf (((symbol-function 'format-time-string)
                       (lambda (&rest _) "MUT"))
                      ((symbol-function 'start-process)
                       (lambda (&rest _)
                         (setq started t)
                         :started)))
              (list
               (async-backup file)
               (nreverse observations)
               (async-backup-test-read-file file)
               started))))"##,
        expect![[r#"OK (:started (:mutated "after\n") "after\n" t)"#]],
    )
}

pub(super) fn predicates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_backup_predicates_receive_expanded_file_in_order_before_process_launch(),
        async_backup_false_predicate_short_circuits_later_predicates_and_process(),
        async_backup_empty_predicate_list_vacuously_launches_backup(),
        async_backup_default_identity_predicate_accepts_any_non_nil_expanded_path(),
        async_backup_predicate_signal_propagates_after_directory_creation_without_process(),
        async_backup_non_function_predicate_signals_without_process(),
        async_backup_predicate_can_filter_by_real_file_size_and_extension(),
        async_backup_predicates_observe_symlink_path_and_can_reject_it(),
        async_backup_predicate_mutation_affects_later_predicate_in_same_call(),
    ]
}
