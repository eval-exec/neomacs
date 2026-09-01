use expect_test::expect;

use super::ParityBatchCase;

fn asyncloop_exact_package_descriptor_origin_dependency_and_feature_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_exact_package_descriptor_origin_dependency_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'asyncloop package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'asyncloop)
          (package-installed-p
           'asyncloop
           '(20240818 1247))
          (file-name-nondirectory
           (locate-library "asyncloop"))))"##,
        expect![[
            r#"OK (asyncloop "20240818.1247" "Non-blocking series of functions." nil ((emacs (28))) ((:maintainers ("Martin Edström" . "meedstrom91@gmail.com")) (:authors ("Martin Edström" . "meedstrom91@gmail.com")) (:keywords "tools") (:revdesc . "7d60950d1600") (:commit . "7d60950d160098a879293e049b9863bc955f8666") (:url . "https://github.com/meedstrom/asyncloop")) t t "asyncloop.el")"#
        ]],
    )
}

fn asyncloop_installed_payload_inventory_hashes_only_immutable_archive_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_installed_payload_inventory_hashes_only_immutable_archive_files",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'asyncloop package-alist)))
                 (directory
                  (package-desc-dir descriptor))
                 (archive-files
                  '("asyncloop-pkg.el"
                    "asyncloop.el")))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (if
                  (member file archive-files)
                  (list
                   file
                   :archive
                   (file-attribute-size
                    (file-attributes path))
                   (with-temp-buffer
                     (insert-file-contents-literally path)
                     (secure-hash
                      'sha256
                      (current-buffer))))
                (list
                 file
                 :generated
                 (file-readable-p path)))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("asyncloop-autoloads.el" :generated t) ("asyncloop-pkg.el" :archive 427 "2d745c8d92c4866edfb2b682d8340faf89530b0087444e72461c0e99adb2491f") ("asyncloop.el" :archive 19037 "1f0e73ae87c39d1d286d8fb5c6e9619276d555b2248070d7597dfed3bd855069") ("asyncloop.elc" :generated t))"#
        ]],
    )
}

fn asyncloop_complete_callable_command_arglist_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_complete_callable_command_arglist_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "asyncloop"
                  (symbol-name symbol))
                 (not
                  (string-suffix-p
                   "--inliner"
                   (symbol-name symbol)))
                 (not
                  (string-suffix-p
                   "--cmacro"
                   (symbol-name symbol)))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "asyncloop.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist symbol t))
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((asyncloop-cancel nil nil "(loop &optional quietly)" "asyncloop.el") (asyncloop-chomp nil nil "(loop)" "asyncloop.el") (asyncloop-clock-funcall nil nil "(loop fn)" "asyncloop.el") (asyncloop-create nil nil "(&rest --cl-rest--)" "asyncloop.el") (asyncloop-eat nil nil "(loop)" "asyncloop.el") (asyncloop-immediate-break-on-user-activity nil nil "(x)" "asyncloop.el") (asyncloop-just-launched nil nil "(x)" "asyncloop.el") (asyncloop-keyboard-quit t (interactive nil) "nil" "asyncloop.el") (asyncloop-log nil nil "(loop &rest args)" "asyncloop.el") (asyncloop-log-buffer nil nil "(x)" "asyncloop.el") (asyncloop-log-mode t (interactive nil) "nil" "asyncloop.el") (asyncloop-notify-simultaneity nil nil "(this-loop)" "asyncloop.el") (asyncloop-p nil nil "(x)" "asyncloop.el") (asyncloop-pause nil nil "(loop)" "asyncloop.el") (asyncloop-paused nil nil "(x)" "asyncloop.el") (asyncloop-remainder nil nil "(x)" "asyncloop.el") (asyncloop-reset-all t (interactive nil) "nil" "asyncloop.el") (asyncloop-resume nil nil "(loop)" "asyncloop.el") (asyncloop-run nil nil "(funs &rest --cl-rest--)" "asyncloop.el") (asyncloop-schedule nil nil "(loop &optional secs)" "asyncloop.el") (asyncloop-scheduled nil nil "(x)" "asyncloop.el") (asyncloop-starttime nil nil "(x)" "asyncloop.el") (asyncloop-timer nil nil "(x)" "asyncloop.el") (asyncloop-with-slots nil nil "(spec-list object &rest body)" "asyncloop.el"))"#
        ]],
    )
}

fn asyncloop_complete_declared_variable_defaults_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_complete_declared_variable_defaults_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "asyncloop"
                  (symbol-name symbol))
                 (boundp symbol)
                 (let ((file
                        (symbol-file symbol 'defvar)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "asyncloop.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (let ((value
                   (default-value symbol)))
              (list
               symbol
               (cond
                ((keymapp value)
                 :keymap)
                ((null value)
                 nil)
                ((numberp value)
                 value)
                (t
                 :other))
               (special-variable-p symbol)
               (local-variable-if-set-p symbol)
               (custom-variable-p symbol)
               (file-name-nondirectory
                (symbol-file symbol 'defvar)))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((asyncloop-log-mode-abbrev-table :other t nil nil "asyncloop.el") (asyncloop-log-mode-hook nil t nil nil "asyncloop.el") (asyncloop-log-mode-map :keymap t nil nil "asyncloop.el") (asyncloop-log-mode-syntax-table :other t nil nil "asyncloop.el") (asyncloop-objects nil t nil nil "asyncloop.el") (asyncloop-recursion-ctr 0 t nil nil "asyncloop.el"))"#
        ]],
    )
}

fn asyncloop_struct_constructor_predicate_accessors_and_mutable_slots_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_struct_constructor_predicate_accessors_and_mutable_slots_match",
        r##"(let* ((loop
                 (asyncloop-create
                  :starttime '(1 2 3 4)
                  :log-buffer nil
                  :immediate-break-on-user-activity t
                  :paused t
                  :remainder '(alpha beta)
                  :scheduled t
                  :just-launched t))
                (initial
                 (list
                  (asyncloop-p loop)
                  (asyncloop-starttime loop)
                  (asyncloop-log-buffer loop)
                  (asyncloop-immediate-break-on-user-activity loop)
                  (timerp
                   (asyncloop-timer loop))
                  (asyncloop-paused loop)
                  (asyncloop-remainder loop)
                  (asyncloop-scheduled loop)
                  (asyncloop-just-launched loop))))
         (setf
          (asyncloop-starttime loop)
          :new-start
          (asyncloop-paused loop)
          nil
          (asyncloop-remainder loop)
          '(gamma)
          (asyncloop-scheduled loop)
          nil
          (asyncloop-just-launched loop)
          nil)
         (list
          initial
          (list
           (asyncloop-starttime loop)
           (asyncloop-paused loop)
           (asyncloop-remainder loop)
           (asyncloop-scheduled loop)
           (asyncloop-just-launched loop))
          (asyncloop-p nil)
          (asyncloop-test-error
           (lambda ()
             (asyncloop-remainder :not-a-loop)))))"##,
        expect![
            "OK ((t (1 2 3 4) nil t t t (alpha beta) t t) (:new-start nil (gamma) nil nil) nil (:signal wrong-type-argument (asyncloop :not-a-loop)))"
        ],
    )
}

fn asyncloop_autoload_surface_exposes_only_the_documented_run_entrypoint() -> ParityBatchCase {
    ParityBatchCase::value(
        "asyncloop_autoload_surface_exposes_only_the_documented_run_entrypoint",
        r##"(list
         (featurep 'asyncloop)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (and
              (fboundp symbol)
              (autoloadp
               (symbol-function symbol)))
             (let ((file
                    (symbol-file symbol 'defun)))
               (and file
                    (file-name-nondirectory file)))))
          '(asyncloop-run
            asyncloop-create
            asyncloop-cancel
            asyncloop-pause
            asyncloop-resume
            asyncloop-log)))"##,
        expect![[
            r#"OK (nil ((asyncloop-run t t "asyncloop.el") (asyncloop-create nil nil nil) (asyncloop-cancel nil nil nil) (asyncloop-pause nil nil nil) (asyncloop-resume nil nil nil) (asyncloop-log nil nil nil)))"#
        ]],
    )
}

pub(super) fn registry_asyncloop_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asyncloop_exact_package_descriptor_origin_dependency_and_feature_contract_match(),
        asyncloop_installed_payload_inventory_hashes_only_immutable_archive_files(),
        asyncloop_complete_callable_command_arglist_and_source_surface_matches(),
        asyncloop_complete_declared_variable_defaults_and_source_surface_matches(),
        asyncloop_struct_constructor_predicate_accessors_and_mutable_slots_match(),
    ]
}

pub(super) fn registry_asyncloop_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![asyncloop_autoload_surface_exposes_only_the_documented_run_entrypoint()]
}
