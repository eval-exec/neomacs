use std::time::Duration;

use crate::{CachedMelpaOracle, IMENU_ANYWHERE_MELPA_PIN, PROJECTILE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const IMENU_ANYWHERE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const IMENU_ANYWHERE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'xref)
(require 'ido)
(require 'python)
(require 'projectile)

(defvar ia352-test-owned-buffers nil)
(defvar ia352-test-buffer-list nil)
(defvar ia352-test-completion-ledger nil)
(defvar ia352-test-input-events nil)
(defvar ia352-test-ido-observations nil)
(defvar ia352-test-message-ledger nil)
(defvar ia352-test-xref-history nil)
(defvar ia352-test-after-jump-ledger nil)

(defun ia352-test-write-buffer (root name contents mode)
  "Write CONTENTS below ROOT and visit it in real MODE."
  (let ((file (expand-file-name name root)))
    (unless (string-prefix-p root file)
      (error "IMENU-ANYWHERE fixture escaped root: %s" file))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert contents))
    (let ((buffer (find-file-noselect file)))
      (push buffer ia352-test-owned-buffers)
      (with-current-buffer buffer
        (funcall mode)
        (set-buffer-modified-p nil)
        (set-visited-file-modtime))
      buffer)))

(defun ia352-test-buffer-list-function ()
  "Return the exact owned buffer plan for the current workflow."
  (copy-sequence ia352-test-buffer-list))

(defun ia352-test-minibuffer-setup ()
  "Observe a real GNU completion minibuffer, then feed owned user events."
  (unless ia352-test-input-events
    (error "IMENU-ANYWHERE real minibuffer has no scripted input"))
  (push
   (list :prompt
         (buffer-substring-no-properties (point-min) (minibuffer-prompt-end))
         :initial-input (minibuffer-contents-no-properties)
         :require-match minibuffer--require-match
         :must-match-map (eq (current-local-map)
                             minibuffer-local-must-match-map)
         :collection
         (mapcar #'substring-no-properties
                 (all-completions "" minibuffer-completion-table
                                  minibuffer-completion-predicate))
         :predicate minibuffer-completion-predicate
         :default (copy-tree minibuffer-default)
         :reader completing-read-function)
   ia352-test-completion-ledger)
  (add-hook 'minibuffer-exit-hook
            #'ia352-test-record-completion-exit nil t)
  (setq unread-command-events
        (append ia352-test-input-events unread-command-events)
        ia352-test-input-events nil))

(defun ia352-test-record-completion-exit ()
  "Append the real final minibuffer input to the active observation."
  (unless ia352-test-completion-ledger
    (error "IMENU-ANYWHERE completion exited without an observation"))
  (setcar ia352-test-completion-ledger
          (append (car ia352-test-completion-ledger)
                  (list :final-input (minibuffer-contents-no-properties)))))

(defun ia352-test-completion-calls ()
  "Return completion calls in user order without shared list tails."
  (reverse (copy-tree ia352-test-completion-ledger)))

(defun ia352-test-real-select (text &optional keys)
  "Invoke public `imenu-anywhere' with literal TEXT and real KEYS."
  (when (or unread-command-events (active-minibuffer-window))
    (error "IMENU-ANYWHERE dirty minibuffer before public selection"))
  (let ((executing-kbd-macro t)
        (completing-read-function #'completing-read-default)
        (ia352-test-input-events
         (append (string-to-list text)
                 (listify-key-sequence (kbd (or keys "RET"))))))
    (minibuffer-with-setup-hook #'ia352-test-minibuffer-setup
      (call-interactively #'imenu-anywhere)))
  (when (or ia352-test-input-events unread-command-events
            (active-minibuffer-window))
    (error "IMENU-ANYWHERE incomplete real completion session: %S / %S"
           ia352-test-input-events unread-command-events))
  (unless ia352-test-completion-ledger
    (error "IMENU-ANYWHERE completion returned without an observation"))
  (setcar ia352-test-completion-ledger
          (append (car ia352-test-completion-ledger)
                  (list :selected (ia352-test-selected-candidate-at-point)))))

(defun ia352-test-selected-candidate-at-point ()
  "Return the unique real cached candidate at the current destination."
  (let ((target-buffer (current-buffer))
        (target-position (point))
        matches)
    (dolist (buffer ia352-test-owned-buffers)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (dolist (entry imenu-anywhere--cached-candidates)
            (let ((marker (cdr entry)))
              (when (and (markerp marker)
                         (eq (marker-buffer marker) target-buffer)
                         (= (marker-position marker) target-position))
                (push (substring-no-properties (car entry)) matches)))))))
    (setq matches (delete-dups matches))
    (unless (= (length matches) 1)
      (error "IMENU-ANYWHERE expected one selected candidate, got %S"
             matches))
    (car matches)))

(defun ia352-test-ido-install-recorder ()
  "Observe and drive the real Ido minibuffer without replacing its reader."
  (push (list :phase 'setup
              :prompt
              (buffer-substring-no-properties (point-min)
                                               (minibuffer-prompt-end))
              :choices
              (mapcar (lambda (candidate)
                        (substring-no-properties (ido-name candidate)))
                      ido-choice-list)
              :reader completing-read-function
              :preprocessor imenu-anywhere-preprocess-entry-function
              :next-match-key (key-binding (kbd "C-s")))
        ia352-test-ido-observations)
  (add-hook 'minibuffer-exit-hook #'ia352-test-record-ido-exit nil t)
  (setq unread-command-events
        (append ia352-test-input-events unread-command-events)
        ia352-test-input-events nil))

(defun ia352-test-record-ido-exit ()
  "Append the real final Ido minibuffer input to its observation."
  (unless ia352-test-ido-observations
    (error "IMENU-ANYWHERE Ido exited without an observation"))
  (setcar ia352-test-ido-observations
          (append (car ia352-test-ido-observations)
                  (list :final-input (minibuffer-contents-no-properties)))))

(defun ia352-test-ido-select (text keys)
  "Invoke public `ido-imenu-anywhere' with literal TEXT and real KEYS."
  (let ((executing-kbd-macro t)
        (ia352-test-input-events
         (append (string-to-list text) (listify-key-sequence (kbd keys))))
        (ido-minibuffer-setup-hook
         (cons #'ia352-test-ido-install-recorder
               ido-minibuffer-setup-hook)))
    (call-interactively #'ido-imenu-anywhere))
  (when (or ia352-test-input-events unread-command-events
            (active-minibuffer-window))
    (error "IMENU-ANYWHERE incomplete Ido session"))
  (unless ia352-test-ido-observations
    (error "IMENU-ANYWHERE Ido returned without an observation"))
  (setcar ia352-test-ido-observations
          (append (car ia352-test-ido-observations)
                  (list :selected (ia352-test-selected-candidate-at-point))))
  (let ((observations (nreverse ia352-test-ido-observations)))
    (unless (= (length observations) 1)
      (error "IMENU-ANYWHERE expected one Ido setup, got %d"
             (length observations)))
    (setq ia352-test-ido-observations nil)
    observations))

(defun ia352-test-cached-marker (buffer name)
  "Return BUFFER's exact cached marker for candidate NAME."
  (with-current-buffer buffer
    (let ((entry (assoc-string name imenu-anywhere--cached-candidates nil)))
      (unless (and entry (markerp (cdr entry)))
        (error "IMENU-ANYWHERE missing exact cached candidate %S in %s: %S"
               name (buffer-name buffer)
               (mapcar #'car imenu-anywhere--cached-candidates)))
      (cdr entry))))

(defun ia352-test-candidate-names (candidates)
  "Return CANDIDATES' names without renderer-owned text properties."
  (mapcar (lambda (candidate)
            (substring-no-properties (car candidate)))
          candidates))

(defun ia352-test-after-jump ()
  "Record one real `imenu-after-jump-hook' observation."
  (push (ia352-test-location) ia352-test-after-jump-ledger))

(defun ia352-test-failing-index-provider ()
  "Represent one failing documented major-mode Imenu provider."
  (error "IA352 provider exploded Ω"))

(defun ia352-test-capture (function)
  "Return FUNCTION's value or exact nonlocal condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition) :data (cdr condition)
             :message (error-message-string condition)))))

(defun ia352-test-helm-after-load-registration ()
  "Return the package's one exact deferred Helm registration."
  (let ((matches
         (seq-filter
          (lambda (entry)
            (and (stringp (car entry))
                 (string-match-p
                  "helm-source-imenu-anywhere"
                  (prin1-to-string (cdr entry)))))
          after-load-alist)))
    (unless (= (length matches) 1)
      (error "IMENU-ANYWHERE expected one Helm registration, got %d"
             (length matches)))
    (car matches)))

(defun ia352-test-line ()
  "Return the selected source line without properties."
  (buffer-substring-no-properties
   (line-beginning-position) (line-end-position)))

(defun ia352-test-location ()
  "Return a complete stable description of the current location."
  (list :buffer (buffer-name)
        :file (and buffer-file-name
                   (file-name-nondirectory buffer-file-name))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :text (ia352-test-line)
        :restriction (list (point-min) (point-max))
        :selected (eq (current-buffer) (window-buffer))))

(defun ia352-test-marker (marker)
  "Return a stable source description of MARKER."
  (let ((buffer (marker-buffer marker)))
    (and buffer
         (with-current-buffer buffer
           (list :file (and buffer-file-name
                            (file-name-nondirectory buffer-file-name))
                 :point (marker-position marker)
                 :line (line-number-at-pos marker)
                 :column (save-excursion
                           (goto-char marker)
                           (current-column)))))))

(defun ia352-test-xref-state ()
  "Return both sides of the isolated real Xref history."
  (list :backward (mapcar #'ia352-test-marker (car ia352-test-xref-history))
        :forward (mapcar #'ia352-test-marker (cdr ia352-test-xref-history))))

(defun ia352-test-clear-xref ()
  "Detach and empty every isolated Xref marker."
  (dolist (stack (list (car ia352-test-xref-history)
                       (cdr ia352-test-xref-history)))
    (dolist (marker stack) (set-marker marker nil nil)))
  (setcar ia352-test-xref-history nil)
  (setcdr ia352-test-xref-history nil))

(defun ia352-test-position (buffer needle &optional occurrence offset)
  "Select BUFFER and move to NEEDLE plus OFFSET."
  (switch-to-buffer buffer)
  (goto-char (point-min))
  (let ((case-fold-search nil))
    (dotimes (_ (or occurrence 1))
      (unless (search-forward needle nil t)
        (error "IMENU-ANYWHERE missing fixture needle: %S" needle))))
  (goto-char (+ (match-beginning 0) (or offset 0)))
  (point))

(defun ia352-test-public-message (function)
  "Run FUNCTION while recording and delegating exact `message' calls."
  (let ((original-message (symbol-function 'message)))
    (cl-letf (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((rendered
                        (if format-string
                            (apply #'format-message format-string arguments)
                          nil)))
                   (push rendered ia352-test-message-ledger)
                   (apply original-message format-string arguments)))))
      (funcall function))))

(defun ia352-test-run (name function)
  "Run FUNCTION in one owned real multi-buffer Imenu world NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root)
                 (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" name)
      (error "IMENU-ANYWHERE invalid case name: %S" name))
    ;; Initialize editor-owned Unicode coding caches before ownership baselines.
    (encode-coding-string "Ω" 'utf-8)
    (let* ((root (file-name-as-directory (expand-file-name name sandbox-root)))
           (root-owned nil)
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (window-buffer-baseline (window-buffer))
           (completion-baseline completing-read-function)
           (filter-baseline (copy-sequence imenu-anywhere-buffer-filter-functions))
           (friendly-baseline (copy-tree imenu-anywhere-friendly-modes))
           (prep-baseline imenu-anywhere-preprocess-entry-function)
           (delimiter-baseline imenu-anywhere-delimiter)
           (jump-hook-baseline (copy-sequence imenu-after-jump-hook))
           (ido-setup-hook-baseline (copy-sequence ido-setup-hook))
           (ido-minibuffer-hook-baseline
            (copy-sequence ido-minibuffer-setup-hook))
           (minibuffer-setup-hook-baseline
            (copy-sequence minibuffer-setup-hook))
           (choose-completion-hook-baseline
            (copy-sequence choose-completion-string-functions))
           (ia352-test-owned-buffers nil)
           (ia352-test-buffer-list nil)
           (ia352-test-completion-ledger nil)
           (ia352-test-input-events nil)
           (ia352-test-ido-observations nil)
           (ia352-test-message-ledger nil)
           (ia352-test-after-jump-ledger nil)
           (ia352-test-xref-history (xref--make-xref-history))
           (xref-history-storage (lambda () ia352-test-xref-history))
           (global-mark-ring nil)
           (completing-read-function #'completing-read-default)
           (unread-command-events nil)
           (executing-kbd-macro nil)
           (minibuffer-history nil)
           (ido-buffer-history nil)
           (ido-file-history nil)
           (ido-use-faces nil)
           (ido-case-fold nil)
           (ido-enable-flex-matching nil)
           (ido-enable-prefix nil)
           (ido-record-commands nil)
           (ido-completion-map nil)
           (projectile-project-root-cache (make-hash-table :test 'equal))
           (projectile-known-projects nil)
           (projectile-known-projects-on-file nil)
           (projectile-track-known-projects-automatically nil)
           (default-directory root)
           (enable-local-variables nil)
           (enable-local-eval nil)
           result cleanup body-error cleanup-errors)
      (when (file-exists-p root)
        (error "IMENU-ANYWHERE owned case root already exists: %s" root))
      (cl-labels
          ((attempt (phase callback)
             (condition-case condition
                 (funcall callback)
               (t (push (list phase condition) cleanup-errors) nil))))
        (unwind-protect
            (condition-case condition
                (progn
                  (make-directory root)
                  (setq root-owned t)
                  (save-window-excursion
                    (save-current-buffer
                      (setq result (funcall function root)))))
              (t (setq body-error condition)))
          (attempt 'xref #'ia352-test-clear-xref)
          (dolist (buffer ia352-test-owned-buffers)
            (attempt
             'buffer
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer
                   (dolist (entry imenu-anywhere--cached-candidates)
                     (let ((position (cdr entry)))
                       (when (markerp position) (set-marker position nil nil))))
                   (when (fboundp 'imenu--cleanup) (imenu--cleanup))
                   (setq imenu-anywhere--cached-candidates nil
                         imenu-anywhere--cached-tick nil
                         imenu-anywhere--cached-prep-function nil
                         imenu--index-alist nil)
                   (widen)
                   (set-buffer-modified-p nil))
                 (kill-buffer buffer)))))
          (dolist (process (seq-difference (process-list) process-baseline #'eq))
            (attempt
             'process
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist (timer (seq-difference timer-idle-list
                                          idle-timer-baseline #'eq))
            (attempt 'idle-timer (lambda () (cancel-timer timer))))
          (dolist (timer (seq-difference timer-list timer-baseline #'eq))
            (attempt 'timer (lambda () (cancel-timer timer))))
          (attempt
           'restore-globals
           (lambda ()
             (setq imenu-anywhere-buffer-filter-functions
                   (copy-sequence filter-baseline)
                   imenu-anywhere-friendly-modes (copy-tree friendly-baseline)
                   imenu-anywhere-preprocess-entry-function prep-baseline
                   imenu-anywhere-delimiter delimiter-baseline
                   imenu-after-jump-hook (copy-sequence jump-hook-baseline)
                   ido-setup-hook (copy-sequence ido-setup-hook-baseline)
                   ido-minibuffer-setup-hook
                   (copy-sequence ido-minibuffer-hook-baseline)
                   minibuffer-setup-hook
                   (copy-sequence minibuffer-setup-hook-baseline)
                   choose-completion-string-functions
                   (copy-sequence choose-completion-hook-baseline))))
          (attempt
           'root
           (lambda ()
             (when root-owned
               (when (file-exists-p root) (delete-directory root t))
               (unless (file-exists-p root) (setq root-owned nil)))))
          (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
            (attempt
             'late-buffer
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer (set-buffer-modified-p nil))
                 (kill-buffer buffer)))))
          (attempt
           'state
           (lambda ()
             (setq cleanup
                   (list
                    :new-buffers
                    (delq nil
                          (mapcar (lambda (buffer)
                                    (and (buffer-live-p buffer) (buffer-name buffer)))
                                  (seq-difference (buffer-list)
                                                  buffer-baseline #'eq)))
                    :owned-live (and (seq-some #'buffer-live-p
                                               ia352-test-owned-buffers) t)
                    :new-processes
                    (mapcar #'process-name
                            (seq-difference (process-list) process-baseline #'eq))
                    :new-timers
                    (+ (length (seq-difference timer-list timer-baseline #'eq))
                       (length (seq-difference timer-idle-list
                                               idle-timer-baseline #'eq)))
                    :xref (ia352-test-xref-state)
                    :input-events ia352-test-input-events
                    :unread-events unread-command-events
                    :minibuffer-active (and (active-minibuffer-window) t)
                    :root-exists (file-exists-p root)
                    :root-owned root-owned
                    :window-restored (eq (window-buffer) window-buffer-baseline)
                    :completion-restored
                    (eq completing-read-function completion-baseline)
                    :filters-restored
                    (equal imenu-anywhere-buffer-filter-functions filter-baseline)
                    :friendly-restored
                    (equal imenu-anywhere-friendly-modes friendly-baseline)
                    :preprocessor-restored
                    (eq imenu-anywhere-preprocess-entry-function prep-baseline)
                    :delimiter-restored
                    (equal imenu-anywhere-delimiter delimiter-baseline)
                    :jump-hook-restored
                    (equal imenu-after-jump-hook jump-hook-baseline)
                    :ido-hook-restored
                    (equal ido-setup-hook ido-setup-hook-baseline)
                    :ido-minibuffer-hook-restored
                    (equal ido-minibuffer-setup-hook
                           ido-minibuffer-hook-baseline)
                    :minibuffer-hook-restored
                    (equal minibuffer-setup-hook
                           minibuffer-setup-hook-baseline)
                    :choose-completion-hook-restored
                    (equal choose-completion-string-functions
                           choose-completion-hook-baseline)
                    :body-error body-error
                    :cleanup-errors (nreverse cleanup-errors)))))))
      (let ((dirty
             (or body-error cleanup-errors
                 (plist-get cleanup :new-buffers)
                 (plist-get cleanup :owned-live)
                 (plist-get cleanup :new-processes)
                 (not (= (plist-get cleanup :new-timers) 0))
                 (plist-get cleanup :input-events)
                 (plist-get cleanup :unread-events)
                 (plist-get cleanup :minibuffer-active)
                 (plist-get cleanup :root-exists)
                 (plist-get cleanup :root-owned)
                 (not (plist-get cleanup :window-restored))
                 (not (plist-get cleanup :completion-restored))
                 (not (plist-get cleanup :filters-restored))
                 (not (plist-get cleanup :friendly-restored))
                 (not (plist-get cleanup :preprocessor-restored))
                 (not (plist-get cleanup :delimiter-restored))
                 (not (plist-get cleanup :jump-hook-restored))
                 (not (plist-get cleanup :ido-hook-restored))
                 (not (plist-get cleanup :ido-minibuffer-hook-restored))
                 (not (plist-get cleanup :minibuffer-hook-restored))
                 (not (plist-get cleanup :choose-completion-hook-restored)))))
        (when dirty
          (error "IMENU-ANYWHERE world failed: body=%S cleanup=%S"
                 body-error cleanup))
        (list :result result
              :interactions (ia352-test-completion-calls)
              :ido (copy-tree ia352-test-ido-observations)
              :after-jump (reverse (copy-tree ia352-test-after-jump-ledger))
              :messages (reverse (copy-sequence ia352-test-message-ledger))
              :cleanup cleanup)))))
"####;

fn imenu_anywhere_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IMENU_ANYWHERE_MELPA_PIN, "imenu-anywhere.el")
        .expect("prepare exact imenu-anywhere source below ./tmp")
        .with_melpa_dependency(PROJECTILE_MELPA_PIN)
        .expect("prepare exact optional Projectile integration below ./tmp")
        .with_prelude(IMENU_ANYWHERE_TEST_PRELUDE)
        .with_timeout(IMENU_ANYWHERE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed imenu-anywhere parity test")
        .into()
}

fn assert_imenu_anywhere_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        imenu_anywhere_oracle(),
        &current_test_name(),
        "imenu_anywhere_parity",
        cases,
    );
}

#[test]
fn imenu_anywhere_package_batch() {
    assert_imenu_anywhere_batch(&workflows::public_workflow_cases());
}
