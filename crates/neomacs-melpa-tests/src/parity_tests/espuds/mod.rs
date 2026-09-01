//! Practical parity for Espuds' public Ecukes step definitions.
//!
//! These cases drive the documented buffer, cursor, region, action-chain,
//! file, mode, face, and message steps through successful workflows, exact
//! assertion failures, and same-route recovery.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ECUKES_MELPA_PIN, ESPUDS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'ecukes-steps)
(require 'ecukes-core)
(require 'espuds)
(get-buffer-create " *code-conversion-work*")
(set-window-configuration (current-window-configuration))
;; Ecukes core advises princ/print/message. The oracle prints results
;; through princ, so keep those two advice wrappers off except when a
;; case temporarily enables `message' to populate ecukes-message-log.
(ad-disable-advice 'princ 'around 'princ-around)
(ad-disable-advice 'print 'around 'print-around)
(ad-disable-advice 'message 'around 'message-around)
(ad-activate 'princ)
(ad-activate 'print)
(ad-activate 'message)

(defconst espuds424-test-tree
  "5ed26cfa394af58ecd0c573b3ca34a9a0a1ce2d4")
(defconst espuds424-test-manifest
  '(("espuds-pkg.el" . "81d2ddb3d95ba28ee519321fc5e356fa077152f24c40f3cd95e0e8cbfb80854d")
    ("espuds.el" . "6e212c78fa3404c0d962128b347c8c87867a7e407f7b31c354fa15ab3fb94e10")))

(defvar espuds424-test-case-index 0)
(defvar espuds424-test-root nil)
(defvar espuds424-test-root-owned nil)

(defun espuds424-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun espuds424-test-source-state ()
  (let* ((located (symbol-file 'espuds-region 'defun))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (espuds424-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/espuds.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car espuds424-test-manifest)))
      (error "Unexpected installed Espuds payload: %S" (or manifest files)))
    (dolist (entry espuds424-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (espuds424-test-sha file) (cdr entry)))
          (error "Unexpected installed Espuds source: %S"
                 (cons entry manifest)))))
    (list :tree espuds424-test-tree
          :manifest espuds424-test-manifest
          :feature (featurep 'espuds)
          :ecukes-steps (fboundp 'Given)
          :version "20230218.910")))

(defun espuds424-test-window-state ()
  (mapcar
   (lambda (window)
     (list window
           (eq window (selected-window))
           (window-buffer window)
           (window-point window)
           (window-start window)
           (window-hscroll window)
           (window-dedicated-p window)
           (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun espuds424-test-mask (string)
  (let ((text (copy-sequence (or string "")))
        (root espuds424-test-root)
        (tmp temporary-file-directory))
    (when (and root (file-name-absolute-p root))
      (setq text (replace-regexp-in-string
                  (regexp-quote root) "[ORACLE-SANDBOX]/" text t t))
      (setq text (replace-regexp-in-string
                  (regexp-quote (directory-file-name root))
                  "[ORACLE-SANDBOX]" text t t)))
    (when (and tmp (file-name-absolute-p tmp))
      (setq text (replace-regexp-in-string
                  (regexp-quote tmp) "[ORACLE-TMPDIR]/" text t t))
      (setq text (replace-regexp-in-string
                  (regexp-quote (directory-file-name tmp))
                  "[ORACLE-TMPDIR]" text t t)))
    text))

(defun espuds424-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (espuds424-test-mask item)
                             (copy-tree item)))
                         (cdr condition))
           :message (espuds424-test-mask (error-message-string condition))))))

(defun espuds424-test-buffer-state ()
  (list :name (copy-sequence (buffer-name))
        :file (and buffer-file-name
                   (file-name-nondirectory buffer-file-name))
        :text (copy-sequence (buffer-string))
        :point (point)
        :size (buffer-size)
        :mark (and (mark t) (mark t))
        :mark-active (and mark-active t)
        :mode major-mode
        :windows (length (window-list nil 'nomini))))

(defun espuds424-test-write (relative contents)
  (let ((file (expand-file-name relative espuds424-test-root)))
    (unless (and espuds424-test-root-owned
                 (file-in-directory-p file espuds424-test-root))
      (error "Refusing Espuds write outside owned root: %S" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix)
          (enable-local-variables nil))
      (with-temp-file file (insert contents)))
    file))

(defun espuds424-test-forbid-external (operation &rest arguments)
  (error "Unexpected Espuds external boundary: %S %S" operation arguments))

(defun espuds424-test-run (body)
  (let* ((index (cl-incf espuds424-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "espuds-%d" index) sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (espuds424-test-window-state))
         (source-before (espuds424-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (tmm-before transient-mark-mode)
         (fill-before fill-column)
         (chain-before espuds-action-chain)
         (chain-active-before espuds-chain-active)
         (prev-input-before espuds-previous-keyboard-input)
         (fontify-fn-before font-lock-fontify-buffer-function)
         (message-log-before (and (boundp 'ecukes-message-log) ecukes-message-log))
         (internal-log-before (and (boundp 'ecukes-internal-message-log)
                                   ecukes-internal-message-log))
         (tmp-before (directory-files temporary-file-directory t))
         (espuds424-test-root root)
         (espuds424-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Espuds sandbox root"))
          (when (file-exists-p root)
            (error "Espuds sandbox root exists: %S" root))
          (make-directory root)
          (setq espuds424-test-root-owned t
                enable-local-variables nil
                debug-on-error nil
                print-circle nil
                default-directory root)
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external 'call-process args)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external
                              'call-process-region args)))
                    ((symbol-function 'process-file)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external 'process-file args)))
                    ((symbol-function 'start-process)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external 'start-process args)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external
                              'start-file-process args)))
                    ((symbol-function 'make-process)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external 'make-process args)))
                    ((symbol-function 'make-network-process)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external
                              'make-network-process args)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external 'url-retrieve args)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external
                              'url-retrieve-synchronously args)))
                    ((symbol-function 'kill-emacs)
                     (lambda (&rest args)
                       (apply #'espuds424-test-forbid-external 'kill-emacs args))))
            (setq result (funcall body)))
          (setq source-after (espuds424-test-source-state))
          (unless (equal source-before source-after)
            (error "Espuds source changed")))
          (t (setq body-error
                   (list :error (car condition)
                         :data (copy-tree (cdr condition))
                         :message (error-message-string condition)))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (setq espuds-action-chain chain-before
              espuds-chain-active chain-active-before
              espuds-previous-keyboard-input prev-input-before)
        (when (fboundp 'ad-disable-advice)
          (ad-disable-advice 'message 'around 'message-around)
          (ad-activate 'message))
        (when (boundp 'ecukes-message-log)
          (setq ecukes-message-log message-log-before))
        (when (boundp 'ecukes-internal-message-log)
          (setq ecukes-internal-message-log internal-log-before))
        (setq
              transient-mark-mode tmm-before
              fill-column fill-before
              default-directory directory-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before)
        (dolist (file (directory-files temporary-file-directory t))
          (unless (member file tmp-before)
            (attempt (list 'temp file)
                     (lambda ()
                       (if (file-directory-p file)
                           (delete-directory file t)
                         (delete-file file))))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (with-current-buffer buffer
                         (let ((kill-buffer-hook nil)
                               (kill-buffer-query-functions nil))
                           (set-buffer-modified-p nil)
                           (kill-buffer buffer)))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (setq font-lock-fontify-buffer-function fontify-fn-before)
        (when espuds424-test-root-owned
          (attempt 'sandbox (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :chain-restored
                 (and (eq espuds-action-chain chain-before)
                      (eq espuds-chain-active chain-active-before)
                      (eq espuds-previous-keyboard-input prev-input-before))
                 :fontify-fn-restored
                 (eq font-lock-fontify-buffer-function fontify-fn-before)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (espuds424-test-window-state) window-state-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Espuds workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ESPUDS_MELPA_PIN, "espuds.el")
        .expect("prepare exact Espuds source below ./tmp")
        .with_melpa_dependency(ECUKES_MELPA_PIN)
        .expect("prepare pinned Ecukes step macros below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_buffer_and_cursor_navigation_distinguishes_space_names_and_unicode() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_buffer_and_cursor_navigation_distinguishes_space_names_and_unicode",
        r####"
(espuds424-test-run
 (lambda ()
   (Given "I switch to buffer \"café ledger\"")
   (Then "I should be in buffer \"café ledger\"")
   (When "I insert \"alpha\"")
   (When "I press \"RET\"")
   (When "I insert \"café 界 beans\"")
   (When "I press \"RET\"")
   (When "I insert \"gamma\"")
   (When "I go to line \"2\"")
   (When "I go to beginning of line")
   (Then "the cursor should be before \"café\"")
   (When "I go to word \"beans\"")
   (Then "the cursor should be before \"beans\"")
   (When "I place the cursor between \"café \" and \"界\"")
   (Then "the cursor should be between \"café \" and \"界\"")
   (When "I place the cursor after \"gamma\"")
   (Then "the cursor should be after \"gamma\"")
   (When "I go to point \"1\"")
   (Then "the cursor should be before \"alpha\"")
   (Given "I am in buffer \"plain pad\"")
   (Then "I should be in buffer \"plain pad\"")
   (let ((missing-line
          (espuds424-test-condition (lambda () (When "I go to line \"9\""))))
         (missing-word
          (espuds424-test-condition (lambda () (When "I go to word \"beans\"")))))
     (Given "I switch to buffer \"café ledger\"")
     (When "I go to word \"beans\"")
     (list :name (copy-sequence (buffer-name))
           :text (copy-sequence (buffer-string))
           :word-point (point)
           :missing-line missing-line
           :missing-word missing-word
           :recovered-word (point)))))
"####,
        expect![[
            r#"OK (:source (:tree "5ed26cfa394af58ecd0c573b3ca34a9a0a1ce2d4" :manifest (("espuds-pkg.el" . "81d2ddb3d95ba28ee519321fc5e356fa077152f24c40f3cd95e0e8cbfb80854d") ("espuds.el" . "6e212c78fa3404c0d962128b347c8c87867a7e407f7b31c354fa15ab3fb94e10")) :feature t :ecukes-steps t :version "20230218.910") :result (:name "café ledger" :text "alpha\ncafé 界 beans\ngamma" :word-point 14 :missing-line (:error error :data ("Requested line ’9’, but buffer only has ’0’ line(s).") :message "Requested line ’9’, but buffer only has ’0’ line(s).") :missing-word (:error error :data ("Can not go to word ’beans’ since it does not exist in the current buffer: ") :message "Can not go to word ’beans’ since it does not exist in the current buffer: ") :recovered-word 14) :cleanup (:source-unchanged t :chain-restored t :fontify-fn-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_insert_see_select_and_region_round_trip_then_missing_select_fails() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_insert_see_select_and_region_round_trip_then_missing_select_fails",
        r####"
(espuds424-test-run
 (lambda ()
   (Given "I switch to buffer \"*espuds424*\"")
   (Given "transient mark mode is active")
   (Given "there is no region selected")
   (When "I insert \"keep café 界\"")
   (When "I press \"RET\"")
   (When "I insert \"drop beans\"")
   (Then "I should see \"café 界\"")
   (Then "I should not see \"missing\"")
   (Then "I should see pattern \"café 界\"")
   (Then "I should not see pattern \"^zzz\"")
   (When "I select \"café 界\"")
   (Then "the region should be \"café 界\"")
   (let ((selected (espuds424-test-buffer-state)))
     (Given "there is no region selected")
     (Then "the region should not be active")
     (let ((missing
            (espuds424-test-condition
             (lambda () (When "I select \"no-such-token\"")))))
       (When "I select \"beans\"")
       (Then "the region should be \"beans\"")
       (Given "I clear the buffer")
       (Then "I should not see anything")
       (Then "the buffer should be empty")
       (list :selected selected
             :missing missing
             :recovered (espuds424-test-buffer-state)
             :see-missing
             (espuds424-test-condition
              (lambda () (Then "I should see \"café\""))))))))
"####,
        expect![[
            r#"OK (:source (:tree "5ed26cfa394af58ecd0c573b3ca34a9a0a1ce2d4" :manifest (("espuds-pkg.el" . "81d2ddb3d95ba28ee519321fc5e356fa077152f24c40f3cd95e0e8cbfb80854d") ("espuds.el" . "6e212c78fa3404c0d962128b347c8c87867a7e407f7b31c354fa15ab3fb94e10")) :feature t :ecukes-steps t :version "20230218.910") :result (:selected (:name "*espuds424*" :file nil :text "keep café 界\ndrop beans" :point 6 :size 22 :mark 12 :mark-active t :mode fundamental-mode :windows 1) :missing (:error error :data ("The text ’no-such-token’ was not found in the current buffer.") :message "The text ’no-such-token’ was not found in the current buffer.") :recovered (:name "*espuds424*" :file nil :text "" :point 1 :size 0 :mark 1 :mark-active t :mode fundamental-mode :windows 1) :see-missing (:error error :data ("Expected\ncafé\nto be part of:\n") :message "Expected\ncafé\nto be part of:\n")) :cleanup (:source-unchanged t :chain-restored t :fontify-fn-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_action_chain_press_type_call_and_variable_set() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_action_chain_press_type_call_and_variable_set",
        r####"
(espuds424-test-run
 (lambda ()
   (Given "I switch to buffer \"*espuds424-chain*\"")
   (When "I start an action chain")
   (When "I press \"M-x\"")
   (When "I type \"text-mode\"")
   (When "I execute the action chain")
   (When "I type \"café \"")
   (When "I type \"界\"")
   (Then "I should see \"café 界\"")
   (When "I start an action chain")
   (When "I press \"RET\"")
   (When "I type \"done\"")
   (When "I execute the action chain")
   (Then "I should see:" "café 界\ndone")
   (When "I turn on abbrev-mode")
   (Given "I switch to buffer \"*espuds424-chain*\"")
   (Then "I should be in buffer \"*espuds424-chain*\"")
   (When "I set fill-column to 42")
   (When "I go to end of buffer")
   (When "I call \"beginning-of-line\"")
   (Then "the cursor should be before \"done\"")
   (When "I press \"C-e\"")
   (Then "the cursor should be after \"e\"")
   (list :buffer (espuds424-test-buffer-state)
         :abbrev (and abbrev-mode t)
         :fill fill-column
         :chain-active espuds-chain-active
         :previous (copy-sequence (or espuds-previous-keyboard-input "")))))
"####,
        expect![[
            r#"OK (:source (:tree "5ed26cfa394af58ecd0c573b3ca34a9a0a1ce2d4" :manifest (("espuds-pkg.el" . "81d2ddb3d95ba28ee519321fc5e356fa077152f24c40f3cd95e0e8cbfb80854d") ("espuds.el" . "6e212c78fa3404c0d962128b347c8c87867a7e407f7b31c354fa15ab3fb94e10")) :feature t :ecukes-steps t :version "20230218.910") :result (:buffer (:name "*espuds424-chain*" :file nil :text "café 界\ndone" :point 12 :size 11 :mark 12 :mark-active t :mode text-mode :windows 1) :abbrev t :fill 42 :chain-active nil :previous "C-e") :cleanup (:source-unchanged t :chain-restored t :fontify-fn-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_owned_file_temp_file_load_and_other_windows() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_owned_file_temp_file_load_and_other_windows",
        r####"
(espuds424-test-run
 (lambda ()
   (let* ((note (espuds424-test-write "notes/café-note.txt" "ledger 界\n"))
          owned temp-state loaded windows)
     (let ((enable-local-variables nil))
       (find-file note))
     (Then "I should be in file \"café-note.txt\"")
     (setq owned (espuds424-test-buffer-state))
     (let ((wrong-file
            (espuds424-test-condition
             (lambda () (Then "I should be in file \"missing.txt\"")))))
       (When "I open temp file \"espuds424\"")
       (setq temp-state
             (list :file-prefix
                   (and buffer-file-name
                        (string-prefix-p
                         (expand-file-name "espuds424" temporary-file-directory)
                         buffer-file-name))
                   :under-tmp
                   (and buffer-file-name
                        (file-in-directory-p buffer-file-name
                                             temporary-file-directory))
                   :visiting (and buffer-file-name t)))
       (When "I load the following:" "(defun espuds424-loaded () 'café)\n")
       (setq loaded (espuds424-test-condition (lambda () (espuds424-loaded))))
       (when (fboundp 'espuds424-loaded)
         (fmakunbound 'espuds424-loaded))
       (split-window)
       (When "I delete other windows")
       (setq windows (length (window-list nil 'nomini)))
       (Given "I switch to buffer \"plain pad\"")
       (let ((not-visiting
              (espuds424-test-condition
               (lambda () (Then "I should be in file \"missing.txt\"")))))
         (find-file note)
         (list :owned owned
               :temp temp-state
               :loaded loaded
               :windows windows
               :wrong-file wrong-file
               :not-visiting not-visiting
               :recovered-file
               (espuds424-test-condition
                (lambda () (Then "I should be in file \"café-note.txt\"")))))))))
"####,
        expect![[
            r#"OK (:source (:tree "5ed26cfa394af58ecd0c573b3ca34a9a0a1ce2d4" :manifest (("espuds-pkg.el" . "81d2ddb3d95ba28ee519321fc5e356fa077152f24c40f3cd95e0e8cbfb80854d") ("espuds.el" . "6e212c78fa3404c0d962128b347c8c87867a7e407f7b31c354fa15ab3fb94e10")) :feature t :ecukes-steps t :version "20230218.910") :result (:owned (:name "café-note.txt" :file "café-note.txt" :text "ledger 界\n" :point 1 :size 9 :mark nil :mark-active nil :mode text-mode :windows 1) :temp (:file-prefix t :under-tmp t :visiting t) :loaded (:returned café) :windows 1 :wrong-file (:error error :data ("Expected file to be ’missing.txt’, but was ’[ORACLE-SANDBOX]/notes/café-note.txt’.") :message "Expected file to be ’missing.txt’, but was ’[ORACLE-SANDBOX]/notes/café-note.txt’.") :not-visiting (:error error :data ("Expected file to be ’missing.txt’, but not visiting any file.") :message "Expected file to be ’missing.txt’, but not visiting any file.") :recovered-file (:returned nil)) :cleanup (:source-unchanged t :chain-restored t :fontify-fn-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_faces_messages_and_failed_assertions_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_faces_messages_and_failed_assertions_recover",
        r####"
(espuds424-test-run
 (lambda ()
   (Given "I switch to buffer \"*espuds424-face*\"")
   (When "I insert \"(defun café ())\"")
   (emacs-lisp-mode)
   (goto-char 2)
   (Then "current point should have the font-lock-keyword-face face")
   (let ((wrong-face
          (espuds424-test-condition
           (lambda ()
             (Then "current point should have the font-lock-comment-face face"))))
         (fontified-at (copy-sequence (format "%S" (espuds-faces-at-point)))))
     (Given "I switch to buffer \"*espuds424-plain*\"")
     (When "I insert \"plain café\"")
     (fundamental-mode)
     (goto-char (point-min))
     (Then "current point should have no face")
     (ad-enable-advice 'message 'around 'message-around)
     (ad-activate 'message)
     (setq ecukes-message-log nil)
     (message "café 界 ready")
     (Then "I should see message \"café 界 ready\"")
     (let ((missing-see
            (espuds424-test-condition
             (lambda () (Then "I should see \"xyzzy\""))))
           (missing-message
            (espuds424-test-condition
             (lambda () (Then "I should see message \"no-such-msg\"")))))
       (Then "I should see \"plain café\"")
       (Then "I should see message \"café 界 ready\"")
       (list :wrong-face wrong-face
             :fontified-at fontified-at
             :missing-see missing-see
             :missing-message missing-message
             :recovered (espuds424-test-buffer-state)
             :no-face
             (espuds424-test-condition
              (lambda ()
                (Then "current point should have no face"))))))))
"####,
        expect![[
            r#"OK (:source (:tree "5ed26cfa394af58ecd0c573b3ca34a9a0a1ce2d4" :manifest (("espuds-pkg.el" . "81d2ddb3d95ba28ee519321fc5e356fa077152f24c40f3cd95e0e8cbfb80854d") ("espuds.el" . "6e212c78fa3404c0d962128b347c8c87867a7e407f7b31c354fa15ab3fb94e10")) :feature t :ecukes-steps t :version "20230218.910") :result (:wrong-face (:error error :data ("Face ’font-lock-comment-face’ was not found at point") :message "Face ’font-lock-comment-face’ was not found at point") :fontified-at "(font-lock-keyword-face)" :missing-see (:error error :data ("Expected\nxyzzy\nto be part of:\nplain café") :message "Expected\nxyzzy\nto be part of:\nplain café") :missing-message (:error error :data ("Expected ’no-such-msg’ to be included in the list of printed messages, but was not.") :message "Expected ’no-such-msg’ to be included in the list of printed messages, but was not.") :recovered (:name "*espuds424-plain*" :file nil :text "plain café" :point 1 :size 10 :mark nil :mark-active nil :mode fundamental-mode :windows 1) :no-face (:returned nil)) :cleanup (:source-unchanged t :chain-restored t :fontify-fn-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn espuds_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_buffer_and_cursor_navigation_distinguishes_space_names_and_unicode(),
        public_insert_see_select_and_region_round_trip_then_missing_select_fails(),
        public_action_chain_press_type_call_and_variable_set(),
        public_owned_file_temp_file_load_and_other_windows(),
        public_faces_messages_and_failed_assertions_recover(),
    ];
    assert_oracle_batch_cases(oracle(), "espuds-rank424", "espuds_parity", &cases);
}
