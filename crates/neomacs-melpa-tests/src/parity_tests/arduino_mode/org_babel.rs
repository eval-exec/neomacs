use expect_test::expect;

use super::ParityBatchCase;

fn org_babel_module_defaults_callable_surface_and_language_registration_are_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "org_babel_module_defaults_callable_surface_and_language_registration_are_exact",
        r##"(list
                    (featurep 'ob-arduino)
                    (featurep 'ob)
                    (mapcar
                     (lambda (symbol)
                       (list
                        symbol
                        (default-value symbol)
                        (get symbol 'custom-type)
                        (get symbol 'custom-group)
                        (documentation-property
                         symbol 'variable-documentation t)))
                     '(ob-arduino:program
                       ob-arduino:port
                       ob-arduino:board))
                    (copy-tree
                     (help-function-arglist
                      'org-babel-execute:arduino t))
                    (commandp
                     'org-babel-execute:arduino)
                    (assoc
                     "arduino" org-src-lang-modes)
                    (assoc
                     "arduino"
                     org-babel-tangle-lang-exts)
                    org-babel-default-header-args:sclang)"##,
        expect![[
            r#"OK (t t ((ob-arduino:program "arduino" string nil "Default Arduino program name.") (ob-arduino:port "/dev/ttyACM0" string nil "Default Arduino port.") (ob-arduino:board "arduino:avr:uno" string nil "Default Arduino board.")) (body params) nil nil nil nil)"#
        ]],
    )
}

fn practical_babel_execution_expands_code_cleans_stale_sketches_writes_source_and_uploads()
-> ParityBatchCase {
    ParityBatchCase::value(
        "practical_babel_execution_expands_code_cleans_stale_sketches_writes_source_and_uploads",
        r##"(let* ((root
                          (file-name-as-directory
                           (make-temp-file
                            "ob-arduino-work-" t)))
                         (org-babel-temporary-directory
                          root)
                         (stale
                          (expand-file-name
                           "ob-arduino-stale.ino" root))
                         (keep
                          (expand-file-name
                           "keep.txt" root))
                         (ob-arduino:program
                          "arduino-cli")
                         events)
                    (unwind-protect
                        (progn
                          (with-temp-file stale
                            (insert "stale"))
                          (with-temp-file keep
                            (insert "keep"))
                          (cl-letf
                              (((symbol-function
                                 'org-babel-expand-body:generic)
                                (lambda (body params)
                                  (push
                                   (list
                                    :expand body params)
                                   events)
                                  (concat
                                   "// expanded\n"
                                   body)))
                               ((symbol-function
                                 'org-babel-eval)
                                (lambda (command body)
                                  (let* ((parts
                                          (split-string
                                           command))
                                         (source
                                          (car
                                           (last parts))))
                                    (push
                                     (list
                                      :eval
                                      (replace-regexp-in-string
                                       (regexp-quote source)
                                       "<SOURCE>"
                                       command t t)
                                      body
                                      (and
                                       (string-prefix-p
                                        "ob-arduino-"
                                        (file-name-nondirectory
                                         source))
                                       (string-suffix-p
                                        ".ino" source))
                                      (with-temp-buffer
                                        (insert-file-contents
                                         source)
                                        (buffer-string)))
                                     events))
                                  "uploaded")))
                            (let ((result
                                   (org-babel-execute:arduino
                                    "void setup() {}\n"
                                    '((:port
                                       . "/dev/ttyACM3")
                                      (:board
                                       . "arduino:avr:mega")))))
                              (list
                               result
                               (nreverse events)
                               (file-exists-p stale)
                               (file-exists-p keep)))))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK ("uploaded" ((:expand "void setup() {}\n" ((:port . "/dev/ttyACM3") (:board . "arduino:avr:mega"))) (:eval "arduino-cli --upload --port /dev/ttyACM3 --board arduino:avr:mega <SOURCE>" "" t "// expanded\nvoid setup() {}\n")) nil t)"#
        ]],
    )
}

fn babel_cleanup_surfaces_relative_directory_misclassification_from_upstream_source()
-> ParityBatchCase {
    ParityBatchCase::signal(
        "babel_cleanup_surfaces_relative_directory_misclassification_from_upstream_source",
        r##"(let* ((root
                          (file-name-as-directory
                           (expand-file-name
                            "ob-arduino-directory-contract"
                            temporary-file-directory)))
                         (org-babel-temporary-directory
                          root)
                         (directory
                          (expand-file-name
                           "directory.ino" root)))
                    (when (file-exists-p root)
                      (delete-directory root t))
                    (make-directory directory t)
                    (unwind-protect
                        (cl-letf
                            (((symbol-function
                               'org-babel-expand-body:generic)
                              (lambda (body _params)
                                body))
                             ((symbol-function
                               'org-babel-eval)
                              (lambda (&rest _args)
                                :unexpected-eval)))
                          (org-babel-execute:arduino
                           "void setup() {}\n" nil))
                      (delete-directory root t)))"##,
        expect![[
            r#"ERR (file-error "Removing old name: is a directory" "[ORACLE-TMPDIR]/ob-arduino-directory-contract/directory.ino")"#
        ]],
    )
}

fn babel_execution_without_optional_headers_preserves_exact_command_spacing() -> ParityBatchCase {
    ParityBatchCase::value(
        "babel_execution_without_optional_headers_preserves_exact_command_spacing",
        r##"(let* ((root
                          (file-name-as-directory
                           (make-temp-file
                            "ob-arduino-defaults-" t)))
                         (org-babel-temporary-directory
                          root)
                         captured)
                    (unwind-protect
                        (cl-letf
                            (((symbol-function
                               'org-babel-expand-body:generic)
                              (lambda (body _params)
                                body))
                             ((symbol-function
                               'org-babel-eval)
                              (lambda (command body)
                                (let ((source
                                       (car
                                        (last
                                         (split-string
                                          command)))))
                                  (setq captured
                                        (list
                                         (replace-regexp-in-string
                                          (regexp-quote source)
                                          "<SOURCE>"
                                          command t t)
                                         body
                                         (with-temp-buffer
                                           (insert-file-contents
                                            source)
                                           (buffer-string)))))
                                :done)))
                          (list
                           (org-babel-execute:arduino
                            "void loop() { delay(10); }\n"
                            nil)
                           captured))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK (:done ("arduino --upload   <SOURCE>" "" "void loop() { delay(10); }\n"))"#
        ]],
    )
}

fn reloading_module_keeps_org_language_and_tangle_registration_idempotent() -> ParityBatchCase {
    ParityBatchCase::value(
        "reloading_module_keeps_org_language_and_tangle_registration_idempotent",
        r##"(progn
                    (load
                     (getenv "NEOMACS_PACKAGE_SOURCE")
                     nil t t)
                    (load
                     (getenv "NEOMACS_PACKAGE_SOURCE")
                     nil t t)
                    (list
                     (seq-count
                      (lambda (entry)
                        (equal
                         entry
                         '("arduino" . arduino)))
                      org-src-lang-modes)
                     (seq-count
                      (lambda (entry)
                        (equal
                         entry
                         '("arduino" . "ino")))
                      org-babel-tangle-lang-exts)
                     (featurep 'ob-arduino)))"##,
        expect!["OK (0 0 t)"],
    )
}

pub(super) fn org_babel_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        org_babel_module_defaults_callable_surface_and_language_registration_are_exact(),
        practical_babel_execution_expands_code_cleans_stale_sketches_writes_source_and_uploads(),
        babel_cleanup_surfaces_relative_directory_misclassification_from_upstream_source(),
        babel_execution_without_optional_headers_preserves_exact_command_spacing(),
        reloading_module_keeps_org_language_and_tangle_registration_idempotent(),
    ]
}
