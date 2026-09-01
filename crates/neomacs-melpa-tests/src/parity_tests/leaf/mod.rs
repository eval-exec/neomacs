//! Practical parity for Leaf's public declarative configuration surface.
//!
//! The cases keep macro expansion real, then prove the generated code through
//! owned library loading, autoload dispatch, mode selection, hooks, bindings,
//! variable configuration, protected failure recovery, and definition lookup.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LEAF_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'leaf)

(defconst leaf392-test-source
  '("leaf.el" "c3cb5de6d449e0daf9c024a1eeedfe180584b37a5b9ecaa91e1791af9aa9c456"))

(defun leaf392-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let ((file (symbol-file 'leaf 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file) (car leaf392-test-source))
               (equal (leaf392-test-file-sha256 file) (cadr leaf392-test-source)))
    (error "Unexpected installed Leaf source: %S" file)))

(defvar leaf392-test-ledger nil)
(defvar leaf392-test-map (make-sparse-keymap))
(defvar leaf392-pre nil)
(defvar leaf392-post nil)
(defvar leaf392-option nil)
(defvar leaf392-config-observed nil)
(defvar leaf392-config-final nil)
(defvar leaf392-load-count 0)
(defvar leaf392-found-marker nil)

(defun leaf392-test-condition (condition)
  (list :type (car condition)
        :data (cdr condition)
        :message (error-message-string condition)))

(defun leaf392-test-write-file (file text)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file file (insert text))))

(defun leaf392-test-relative (file root)
  (file-relative-name (expand-file-name file) root))

(defun leaf392-test-window-state ()
  (list :selected (selected-window)
        :windows
        (mapcar (lambda (window)
                  (list window (window-buffer window)))
                (window-list nil 'no-minibuffer))))

(defun leaf392-test-symbol-state (symbol)
  (list symbol
        (boundp symbol) (and (boundp symbol) (symbol-value symbol))
        (fboundp symbol) (and (fboundp symbol) (symbol-function symbol))
        (copy-tree (symbol-plist symbol))))

(defun leaf392-test-restore-symbol (state)
  (let ((symbol (nth 0 state)))
    (if (nth 1 state)
        (set symbol (nth 2 state))
      (makunbound symbol))
    (if (nth 3 state)
        (fset symbol (nth 4 state))
      (fmakunbound symbol))
    (setplist symbol (copy-tree (nth 5 state)))))

(defun leaf392-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "leaf/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (leaf392-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (symbol-states
          (mapcar #'leaf392-test-symbol-state
                  '(leaf392-fixture-mode leaf392-fixture-mode-map
                    leaf392-fixture-mode-syntax-table
                    leaf392-fixture-mode-abbrev-table
                    leaf392-fixture-mode-hook leaf392-fixture-hook
                    leaf392-fixture-command leaf392-hook-ran
                    leaf392-option)))
         (load-path (copy-sequence load-path))
         (auto-mode-alist (copy-tree auto-mode-alist))
         (after-load-alist (copy-tree after-load-alist))
         (leaf--paths (copy-tree leaf--paths))
         (leaf-key-bindlist (copy-tree leaf-key-bindlist))
         (leaf392-test-map (copy-keymap leaf392-test-map))
         (leaf392-test-ledger nil)
         (leaf392-pre nil)
         (leaf392-post nil)
         (leaf392-option nil)
         (leaf392-config-observed nil)
         (leaf392-config-final nil)
         (leaf392-load-count 0)
         (leaf392-found-marker nil)
         result body-error cleanup-errors)
    (unless (and root (file-name-absolute-p root))
      (error "Missing absolute Leaf sandbox root"))
    (when (file-exists-p root)
      (error "Leaf sandbox root already exists: %s" root))
    (make-directory root t)
    (condition-case condition
        (cl-letf (((symbol-function 'call-process)
                   (lambda (&rest args)
                     (error "Unexpected call-process: %S" args)))
                  ((symbol-function 'call-process-region)
                   (lambda (&rest args)
                     (error "Unexpected call-process-region: %S" args)))
                  ((symbol-function 'start-process)
                   (lambda (&rest args)
                     (error "Unexpected start-process: %S" args)))
                  ((symbol-function 'make-process)
                   (lambda (&rest args)
                     (error "Unexpected make-process: %S" args)))
                  ((symbol-function 'make-network-process)
                   (lambda (&rest args)
                     (error "Unexpected network process: %S" args)))
                  ((symbol-function 'url-retrieve)
                   (lambda (&rest args)
                     (error "Unexpected URL retrieval: %S" args))))
          (save-window-excursion
            (save-current-buffer
              (setq result (funcall body root)))))
      (t (setq body-error (leaf392-test-condition condition))))
    (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
      (condition-case condition
          (when (buffer-live-p buffer)
            (with-current-buffer buffer
              (let ((kill-buffer-hook nil)
                    (kill-buffer-query-functions nil))
                (set-buffer-modified-p nil)
                (kill-buffer buffer))))
        (t (push (leaf392-test-condition condition) cleanup-errors))))
    (dolist (feature '(leaf392-fixture leaf392-config))
      (condition-case condition
          (when (featurep feature) (unload-feature feature t))
        (t (push (leaf392-test-condition condition) cleanup-errors))))
    (dolist (state symbol-states)
      (condition-case condition (leaf392-test-restore-symbol state)
        (t (push (leaf392-test-condition condition) cleanup-errors))))
    (dolist (timer (seq-difference timer-list timers-before #'eq))
      (condition-case condition (cancel-timer timer)
        (t (push (leaf392-test-condition condition) cleanup-errors))))
    (dolist (process (seq-difference (process-list) processes-before #'eq))
      (condition-case condition (delete-process process)
        (t (push (leaf392-test-condition condition) cleanup-errors))))
    (dolist (frame (seq-difference (frame-list) frames-before #'eq))
      (condition-case condition (delete-frame frame t)
        (t (push (leaf392-test-condition condition) cleanup-errors))))
    (condition-case condition
        (when (file-exists-p root) (delete-directory root t))
      (t (push (leaf392-test-condition condition) cleanup-errors)))
    (when (buffer-live-p buffer-before) (set-buffer buffer-before))
    (condition-case condition (set-window-configuration window-before)
      (t (push (leaf392-test-condition condition) cleanup-errors)))
    (when (buffer-live-p buffer-before) (set-buffer buffer-before))
    (let* ((window-restored
            (equal (leaf392-test-window-state) window-state-before))
           (cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list) buffers-before #'eq)))
                 :new-processes (length (seq-difference
                                         (process-list) processes-before #'eq))
                 :new-timers (length (seq-difference timer-list timers-before #'eq))
                 :new-frames (length (seq-difference (frame-list) frames-before #'eq))
                 :root-exists (file-exists-p root)
                 :window-restored window-restored
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Leaf workflow failed: %S" (list result cleanup))
        (list :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LEAF_MELPA_PIN, "leaf.el")
        .expect("prepare exact shallow Leaf source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn documented_macroexpand_executes_declarative_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_macroexpand_executes_declarative_order",
        r####"
(leaf392-test-run
 (lambda (_root)
   (let* ((leaf-expand-minimally t)
          (form
           '(leaf leaf392-order
              :preface (push 'preface leaf392-test-ledger)
              :when t
              :init (push 'init leaf392-test-ledger)
              :require leaf
              :config (push 'config leaf392-test-ledger)))
          expansion returned)
     (setq expansion (macroexpand-1 form))
     (setq returned (eval expansion))
     (list :available
           (seq-filter (lambda (key)
                         (memq key '(:preface :when :init :require :config)))
                       (leaf-available-keywords))
           :expansion expansion
           :returned returned
           :order (nreverse leaf392-test-ledger)))))
"####,
        expect![
            "OK (:result (:available (:preface :when :init :require :config) :expansion (prog1 'leaf392-order (push 'preface leaf392-test-ledger) (when t (push 'init leaf392-test-ledger) (require 'leaf) (push 'config leaf392-test-ledger))) :returned leaf392-order :order (preface init config)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"
        ],
    )
}

fn deferred_library_drives_mode_hook_binding_and_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "deferred_library_drives_mode_hook_binding_and_command",
        r####"
(leaf392-test-run
 (lambda (root)
   (let* ((library (expand-file-name "leaf392-fixture.el" root))
          (document (expand-file-name "demo.leaf392" root)))
     (leaf392-test-write-file
      library
      "(defvar leaf392-test-ledger)\n(defvar leaf392-load-count)\n(setq leaf392-load-count (1+ leaf392-load-count))\n(define-derived-mode leaf392-fixture-mode text-mode \"Leaf392\")\n(defun leaf392-fixture-hook () (setq-local leaf392-hook-ran t) (push 'hook leaf392-test-ledger))\n(defun leaf392-fixture-command () (interactive) (push 'command leaf392-test-ledger) \"command-result-界\")\n(provide 'leaf392-fixture)\n")
     (leaf392-test-write-file document "café payload\n")
     (eval
      `(leaf leaf392-fixture
         :load-path ,root
         :commands leaf392-fixture-command
         :mode ("\\.leaf392\\'" . leaf392-fixture-mode)
         :hook (leaf392-fixture-mode-hook . leaf392-fixture-hook)
         :bind (:leaf392-test-map ("C-c l" . leaf392-fixture-command))
         :init (push 'init leaf392-test-ledger)
         :config (push 'config leaf392-test-ledger)))
     (let ((registered
            (list :command-autoload (autoloadp (symbol-function 'leaf392-fixture-command))
                  :mode-autoload (autoloadp (symbol-function 'leaf392-fixture-mode))
                  :hook-autoload (autoloadp (symbol-function 'leaf392-fixture-hook))
                  :mode-entry (assoc "\\.leaf392\\'" auto-mode-alist)
                  :hook-member
                  (and (memq 'leaf392-fixture-hook leaf392-fixture-mode-hook) t)
                  :binding-before-load
                  (lookup-key leaf392-test-map (kbd "C-c l"))))
           mode-state command-result)
       (with-current-buffer (find-file-noselect document)
         (setq mode-state
               (list :mode major-mode
                     :hook-ran (bound-and-true-p leaf392-hook-ran)
                     :text (buffer-string))))
       (setq command-result
             (call-interactively (lookup-key leaf392-test-map (kbd "C-c l"))))
       (list :registered registered
             :loaded (featurep 'leaf392-fixture)
             :load-count leaf392-load-count
             :mode mode-state
             :binding-after-load
             (lookup-key leaf392-test-map (kbd "C-c l"))
             :command command-result
             :order (nreverse leaf392-test-ledger))))))
"####,
        expect![[
            r#"OK (:result (:registered (:command-autoload t :mode-autoload t :hook-autoload t :mode-entry ("\\.leaf392\\'" . leaf392-fixture-mode) :hook-member t :binding-before-load 1) :loaded t :load-count 1 :mode (:mode leaf392-fixture-mode :hook-ran t :text "café payload\n") :binding-after-load leaf392-fixture-command :command "command-result-界" :order (init config hook command)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn require_custom_and_setq_preserve_configuration_phases() -> ParityBatchCase {
    ParityBatchCase::value(
        "require_custom_and_setq_preserve_configuration_phases",
        r####"
(leaf392-test-run
 (lambda (root)
   (leaf392-test-write-file
    (expand-file-name "leaf392-config.el" root)
    "(defvar leaf392-pre)\n(defvar leaf392-post)\n(defcustom leaf392-option 'default \"Owned Leaf option.\" :type 'symbol)\n(setq leaf392-config-observed (list :pre leaf392-pre :option leaf392-option :post leaf392-post))\n(provide 'leaf392-config)\n")
   (eval
    `(leaf leaf392-config
       :load-path ,root
       :pre-setq (leaf392-pre . "café")
       :require t
       :custom (leaf392-option . 'chosen)
       :setq (leaf392-post . "界")
       :config
       (setq leaf392-config-final
             (list :pre leaf392-pre :option leaf392-option :post leaf392-post))))
   (list :load-observed leaf392-config-observed
         :final leaf392-config-final
         :custom-variable (and (custom-variable-p 'leaf392-option) t)
         :customized-value (get 'leaf392-option 'customized-value))))
"####,
        expect![[
            r#"OK (:result (:load-observed (:pre "café" :option chosen :post nil) :final (:pre "café" :option chosen :post "界") :custom-variable t :customized-value ('chosen)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn protected_failure_reports_and_next_block_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "protected_failure_reports_and_next_block_recovers",
        r####"
(leaf392-test-run
 (lambda (_root)
   (let (warnings protected unprotected recovery)
     (cl-letf (((symbol-function 'display-warning)
                (lambda (source message &optional level buffer-name)
                  (push (list :source source :message message
                              :level level :buffer buffer-name)
                        warnings))))
       (setq protected
             (eval
              '(leaf leaf392-protected
                 :config
                 (push 'protected-start leaf392-test-ledger)
                 (error "owned boom 界")
                 (push 'unreachable leaf392-test-ledger))))
       (setq unprotected
             (condition-case condition
                 (eval
                  '(leaf leaf392-unprotected
                     :leaf-protect nil
                     :config (error "unprotected boom")))
               (t (leaf392-test-condition condition))))
       (setq recovery
             (eval
              '(leaf leaf392-recovery
                 :config (push 'recovered leaf392-test-ledger)))))
     (list :protected protected
           :warnings (nreverse warnings)
           :unprotected unprotected
           :recovery recovery
           :order (nreverse leaf392-test-ledger)))))
"####,
        expect![[
            r#"OK (:result (:protected leaf392-protected :warnings ((:source leaf :message "Error in `leaf392-protected' block.  Error msg: owned boom 界" :level nil :buffer nil)) :unprotected (:type error :data ("unprotected boom") :message "unprotected boom") :recovery leaf392-recovery :order (protected-start recovered)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_leaf_find_navigates_to_owned_definition() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_leaf_find_navigates_to_owned_definition",
        r####"
(leaf392-test-run
 (lambda (root)
   (let* ((file (expand-file-name "init-leaf.el" root))
          completion-observed destination)
     (leaf392-test-write-file
      file
      "(require 'leaf)\n\n(leaf leaf392-target\n  :leaf-protect nil\n  :config (setq leaf392-found-marker 'configured))\n")
     (with-current-buffer (find-file-noselect file)
       (eval-buffer))
     (cl-letf (((symbol-function 'completing-read)
                (lambda (prompt collection &rest _)
                  (setq completion-observed
                        (list :prompt prompt
                              :candidates (sort (copy-sequence collection)
                                                #'string-lessp)))
                  "leaf392-target")))
       (call-interactively #'leaf-find))
     (let ((buffer (window-buffer (selected-window))))
       (with-current-buffer buffer
         (goto-char (window-point (selected-window)))
         (setq destination
               (list :file (leaf392-test-relative buffer-file-name root)
                     :line (line-number-at-pos)
                     :column (current-column)
                     :text (buffer-substring-no-properties
                            (line-beginning-position) (line-end-position))))))
     (list :configured leaf392-found-marker
           :paths (mapcar (lambda (entry)
                            (cons (car entry)
                                  (leaf392-test-relative (cdr entry) root)))
                          leaf--paths)
           :completion completion-observed
           :destination destination))))
"####,
        expect![[
            r#"OK (:result (:configured configured :paths ((leaf392-target . "init-leaf.el")) :completion (:prompt "Find leaf: " :candidates (leaf392-target)) :destination (:file "init-leaf.el" :line 3 :column 0 :text "(leaf leaf392-target")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        documented_macroexpand_executes_declarative_order(),
        deferred_library_drives_mode_hook_binding_and_command(),
        require_custom_and_setq_preserve_configuration_phases(),
        protected_failure_reports_and_next_block_recovers(),
        public_leaf_find_navigates_to_owned_definition(),
    ]
}

#[test]
fn public_leaf_configuration_workflows_match() {
    assert_oracle_batch_cases(oracle(), "leaf-rank392", "Leaf", &cases());
}
