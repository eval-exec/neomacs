use expect_test::expect;

use super::ParityBatchCase;

fn auto_package_update_exact_descriptor_dependencies_and_archive_payload_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_package_update_exact_descriptor_dependencies_and_archive_payload_match",
        r##"(let*
                             ((descriptor
                               (cadr
                                (assq
                                 'auto-package-update
                                 package-alist)))
                              (directory
                               (package-desc-dir descriptor)))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-summary descriptor)
                            (package-desc-reqs descriptor)
                            (package-desc-kind descriptor)
                            (package-desc-extras descriptor)
                            (mapcar
                             (lambda (name)
                               (let ((file
                                      (expand-file-name
                                       name
                                       directory)))
                                 (list
                                  name
                                  (file-attribute-size
                                   (file-attributes file))
                                  (with-temp-buffer
                                    (set-buffer-multibyte nil)
                                    (insert-file-contents-literally file)
                                    (secure-hash
                                     'sha256
                                     (current-buffer))))))
                             '("auto-package-update-pkg.el"
                               "auto-package-update.el"))
                            (let ((dash
                                   (cadr
                                    (assq
                                     'dash
                                     package-alist))))
                              (list
                               (package-desc-name dash)
                               (package-version-join
                                (package-desc-version dash))
                               (package-desc-reqs dash)))))"##,
        expect![[
            r#"OK (auto-package-update "20260601.1804" "Automatically update Emacs packages." ((emacs (24 4)) (dash (2 1 0))) nil ((:keywords "package" "update") (:revdesc . "e966c6c95de1") (:commit . "e966c6c95de1742d867250dc15b1c6bd570b6ea5") (:url . "https://github.com/rranelli/auto-package-update.el")) (("auto-package-update-pkg.el" 361 "137a90e8c3931ce94db0eb3a5880d756566cd5fc84db75cba0323b4e0934fc2d") ("auto-package-update.el" 15624 "bfdf1377656ce5d47445734eafd4db1353e87816110a5e7a0a4e78691c012745")) (dash "20260221.1346" ((emacs (24)))))"#
        ]],
    )
}

fn auto_package_update_minor_mode_has_exact_buffer_local_keymap_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_minor_mode_has_exact_buffer_local_keymap_lifecycle",
        r##"(with-temp-buffer
                           (let ((before
                                  (list
                                   auto-package-update-minor-mode
                                   (local-variable-p
                                    'auto-package-update-minor-mode)
                                   (key-binding (kbd "q")))))
                             (auto-package-update-minor-mode 1)
                             (let ((enabled
                                    (list
                                     auto-package-update-minor-mode
                                     (local-variable-p
                                      'auto-package-update-minor-mode)
                                     (key-binding (kbd "q"))
                                     (commandp
                                      (key-binding (kbd "q"))))))
                               (auto-package-update-minor-mode -1)
                               (list
                                before
                                enabled
                                auto-package-update-minor-mode
                                (key-binding (kbd "q"))
                                (get
                                 'auto-package-update-minor-mode
                                 'custom-type)))))"##,
        expect![
            "OK ((nil nil self-insert-command) (t t quit-window t) nil self-insert-command nil)"
        ],
    )
}

fn auto_package_update_generated_autoload_exposes_only_public_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_generated_autoload_exposes_only_public_commands",
        r##"(let*
                             ((history
                               (seq-find
                                (lambda (entry)
                                  (and
                                   (stringp (car entry))
                                   (string-suffix-p
                                    "auto-package-update-autoloads.el"
                                    (car entry))))
                                load-history))
                              (symbols
                               '(auto-package-update-now
                                 auto-package-update-now-async
                                 auto-package-update-at-time
                                 auto-package-update-maybe)))
                           (list
                            (featurep
                             'auto-package-update-autoloads)
                            (featurep 'auto-package-update)
                            (mapcar
                             (lambda (symbol)
                               (list
                                symbol
                                (fboundp symbol)
                                (autoloadp
                                 (symbol-function symbol))
                                (and (commandp symbol) t)
                                (file-name-nondirectory
                                 (or
                                  (symbol-file symbol 'defun)
                                  ""))))
                             symbols)
                            (seq-filter
                             (lambda (event)
                               (memq
                                (car-safe event)
                                '(defun provide)))
                             (cdr history))))"##,
        expect![[
            r#"OK (t nil ((auto-package-update-now t t t "auto-package-update.el") (auto-package-update-now-async t t t "auto-package-update.el") (auto-package-update-at-time t t nil "auto-package-update.el") (auto-package-update-maybe t t nil "auto-package-update.el")) ((defun . auto-package-update-now) (defun . auto-package-update-now-async) (defun . auto-package-update-at-time) (defun . auto-package-update-maybe) (provide . auto-package-update-autoloads)))"#
        ]],
    )
}

pub(super) fn registry_auto_package_update_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_package_update_exact_descriptor_dependencies_and_archive_payload_match(),
        auto_package_update_minor_mode_has_exact_buffer_local_keymap_lifecycle(),
    ]
}

pub(super) fn registry_auto_package_update_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_package_update_generated_autoload_exposes_only_public_commands()]
}
