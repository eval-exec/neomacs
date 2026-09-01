use super::ParityBatchCase;
use expect_test::expect;

fn current_melpa_archive_metadata_and_five_library_identities_match_the_exact_pin()
-> ParityBatchCase {
    ParityBatchCase::value(
        "current_melpa_archive_metadata_and_five_library_identities_match_the_exact_pin",
        r##"
(let* ((description
        (cadr
         (assq 'async package-alist)))
       (directory
        (package-desc-dir description)))
  (list
   (package-version-join
    (package-desc-version description))
   (mapcar
    (lambda (dependency)
      (list
       (car dependency)
       (package-version-join
        (cadr dependency))))
    (package-desc-reqs description))
   (mapcar
    (lambda (filename)
      (let ((contents
             (async-melpa-test-read-file
              (expand-file-name
               filename directory))))
        (list
         filename
         (length contents)
         (and
          (string-match-p
           "Package-Revision: 5faab2891660"
           contents)
          t))))
    '("async.el"
      "async-bytecomp.el"
      "async-package.el"
      "dired-async.el"
      "smtpmail-async.el"))))
"##,
        expect![[
            r#"OK ("20260318.1803" ((emacs "24.4")) (("async.el" 24514 t) ("async-bytecomp.el" 10815 nil) ("async-package.el" 6055 nil) ("dired-async.el" 21840 nil) ("smtpmail-async.el" 2486 nil)))"#
        ]],
    )
}

fn core_registry_matches_every_declared_callable_and_kind() -> ParityBatchCase {
    ParityBatchCase::value(
        "core_registry_matches_every_declared_callable_and_kind",
        r##"
(mapcar
 (lambda (symbol)
   (list
    symbol
    (if (macrop symbol)
        'macro
      (if (commandp symbol)
          'command
        'function))
    (help-function-arglist
     symbol t)))
 '(async--purecopy
   async-inject-variables
   async-inject-environment
   async-handle-result
   async-when-done
   async-read-from-client
   async--receive-sexp
   async--insert-sexp
   async--transmit-sexp
   async-batch-invoke
   async-ready
   async-wait
   async-get
   async-message-p
   async-send
   async-receive
   async-start-process
   async--emacs-program-args
   async-start
   async-sandbox
   async--fold-left
   async-let))
"##,
        expect![
            "OK ((async--purecopy function (object)) (async-inject-variables function #1=(include-regexp &optional predicate exclude-regexp noprops)) (async-inject-environment function #1#) (async-handle-result function (func result buf)) (async-when-done function (proc &optional _change)) (async-read-from-client function (proc string &optional prompt-for-pwd)) (async--receive-sexp function (&optional stream)) (async--insert-sexp function (sexp)) (async--transmit-sexp function (process sexp)) (async-batch-invoke function nil) (async-ready function (future)) (async-wait function (future)) (async-get function (future)) (async-message-p function (value)) (async-send function (process-or-key &rest args)) (async-receive function nil) (async-start-process function (name program finish-func &rest program-args)) (async--emacs-program-args function (&optional sexp)) (async-start function (start-func &optional finish-func)) (async-sandbox macro (func)) (async--fold-left function (fn forms bindings)) (async-let macro (bindings &rest forms)))"
        ],
    )
}

fn bytecomp_registry_matches_every_declared_callable_and_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "bytecomp_registry_matches_every_declared_callable_and_mode",
        r##"
(list
 (featurep 'async-bytecomp)
 (mapcar
  (lambda (symbol)
    (list
     symbol
     (if (macrop symbol)
         'macro
       (if (commandp symbol)
           'command
         'function))
     (help-function-arglist
      symbol t)))
  '(async-bytecomp--file-to-comp-buffer-1
    async-bytecomp--file-to-comp-buffer
    async-bytecomp--comp-buffer-to-file
    async-byte-recompile-directory
    async-bytecomp--get-package-deps
    async--package-compile
    async-bytecomp-package-mode
    async-byte-compile-file)))
"##,
        expect![
            "OK (t ((async-bytecomp--file-to-comp-buffer-1 function (log-file &optional postproc)) (async-bytecomp--file-to-comp-buffer function (file-or-dir &optional quiet type log-file)) (async-bytecomp--comp-buffer-to-file macro nil) (async-byte-recompile-directory function (directory &optional quiet)) (async-bytecomp--get-package-deps function (pkgs)) (async--package-compile function (orig-fun pkg-desc &rest args)) (async-bytecomp-package-mode command (&optional arg)) (async-byte-compile-file command (file))))"
        ],
    )
}

fn dired_registry_matches_every_declared_callable_macro_and_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "dired_registry_matches_every_declared_callable_macro_and_mode",
        r##"
(list
 (featurep 'dired-async)
 (mapcar
  (lambda (symbol)
    (list
     symbol
     (if (macrop symbol)
         'macro
       (if (commandp symbol)
           'command
         'function))
     (help-function-arglist
      symbol t)))
  '(dired-async--modeline-mode
    dired-async-mode-line-message
    dired-async-processes
    dired-async-kill-process
    dired-async-after-file-create
    dired-async-maybe-kill-ftp
    dired-async--directory-p
    dired-async--same-device-p
    dired-async--small-file-p
    dired-async--skip-async-p
    dired-async--smart-create-files
    dired-async--abort-if-file-too-large
    dired-async-create-files
    dired-async-wdired-do-renames
    dired-async-mode
    dired-async--with-async-create-files
    dired-async-do-copy
    dired-async-do-symlink
    dired-async-do-hardlink
    dired-async-do-rename)))
"##,
        expect![
            "OK (t ((dired-async--modeline-mode command (&optional arg)) (dired-async-mode-line-message function (text face &rest args)) (dired-async-processes function (&optional propname)) (dired-async-kill-process command nil) (dired-async-after-file-create function (total operation failures skipped)) (dired-async-maybe-kill-ftp function nil) (dired-async--directory-p function (attributes)) (dired-async--same-device-p function (f1 f2)) (dired-async--small-file-p function (file &optional attrs)) (dired-async--skip-async-p function (file-creator file name-constructor &optional attrs)) (dired-async--smart-create-files function (old-func file-creator operation fn-list name-constructor &optional marker-char)) (dired-async--abort-if-file-too-large function (size op-type filename)) (dired-async-create-files function (file-creator operation fn-list name-constructor &optional _marker-char)) (dired-async-wdired-do-renames function (old-fn &rest args)) (dired-async-mode command (&optional arg)) (dired-async--with-async-create-files macro (&rest body)) (dired-async-do-copy command (&optional arg)) (dired-async-do-symlink command (&optional arg)) (dired-async-do-hardlink command (&optional arg)) (dired-async-do-rename command (&optional arg))))"
        ],
    )
}

fn package_and_smtpmail_registries_match_their_complete_surfaces() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_and_smtpmail_registries_match_their_complete_surfaces",
        r##"
(list
 (featurep 'async-package)
 (mapcar
  (lambda (symbol)
    (list
     symbol
     (commandp symbol)
     (help-function-arglist
      symbol t)))
  '(async-package--modeline-mode
    async-package-do-action))
 async-pkg-install-after-hook
 (get 'async-package-message
      'face-defface-spec))
"##,
        expect![[
            r#"OK (t ((async-package--modeline-mode t (&optional arg)) (async-package-do-action nil (action packages error-file))) nil ((t (:foreground "yellow"))))"#
        ]],
    )
}

fn smtpmail_registry_matches_hook_group_and_send_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "smtpmail_registry_matches_hook_group_and_send_command",
        r##"
(list
 (featurep 'smtpmail-async)
 (featurep 'smtpmail)
 async-smtpmail-before-send-hook
 (get 'smtpmail-async
      'group-documentation)
 (list
  (fboundp
   'async-smtpmail-send-it)
  (help-function-arglist
   'async-smtpmail-send-it t)))
"##,
        expect![[r#"OK (t t nil "Send e-mail with smtpmail.el asynchronously" (t nil))"#]],
    )
}

fn generated_autoloads_publish_current_core_bytecomp_and_dired_entry_points() -> ParityBatchCase {
    ParityBatchCase::value(
        "generated_autoloads_publish_current_core_bytecomp_and_dired_entry_points",
        r##"
(list
 (mapcar
  (lambda (symbol)
    (let ((definition
           (symbol-function symbol)))
      (list
       symbol
       (autoloadp definition)
       (nth 1 definition)
       (nth 3 definition)
       (nth 4 definition))))
  '(async-start-process
    async-start
    async-byte-recompile-directory
    async-bytecomp-package-mode
    async-byte-compile-file
    dired-async-mode
    dired-async-do-copy
    dired-async-do-symlink
    dired-async-do-hardlink
    dired-async-do-rename))
 (featurep 'async-autoloads)
 (featurep 'async))
"##,
        expect![[
            r#"OK (((async-start-process t "async" nil nil) (async-start t "async" nil nil) (async-byte-recompile-directory t "async-bytecomp" nil nil) (async-bytecomp-package-mode t "async-bytecomp" t nil) (async-byte-compile-file t "async-bytecomp" t nil) (dired-async-mode t "dired-async" t nil) (dired-async-do-copy t "dired-async" t nil) (dired-async-do-symlink t "dired-async" t nil) (dired-async-do-hardlink t "dired-async" t nil) (dired-async-do-rename t "dired-async" t nil)) t nil)"#
        ]],
    )
}

pub(super) fn registry_async_melpa_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_melpa_archive_metadata_and_five_library_identities_match_the_exact_pin(),
        core_registry_matches_every_declared_callable_and_kind(),
    ]
}

pub(super) fn registry_async_melpa_bytecomp_batch_cases() -> Vec<ParityBatchCase> {
    vec![bytecomp_registry_matches_every_declared_callable_and_mode()]
}

pub(super) fn registry_async_melpa_dired_batch_cases() -> Vec<ParityBatchCase> {
    vec![dired_registry_matches_every_declared_callable_macro_and_mode()]
}

pub(super) fn registry_async_melpa_package_batch_cases() -> Vec<ParityBatchCase> {
    vec![package_and_smtpmail_registries_match_their_complete_surfaces()]
}

pub(super) fn registry_async_melpa_smtpmail_batch_cases() -> Vec<ParityBatchCase> {
    vec![smtpmail_registry_matches_hook_group_and_send_command()]
}

pub(super) fn registry_async_melpa_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![generated_autoloads_publish_current_core_bytecomp_and_dired_entry_points()]
}
