//! Practical parity coverage for rank 417 `ido-ubiquitous`.
//!
//! The historical package now ships inside `ido-completing-read+`. These cases
//! drive its public global mode and completion function while doubling only the
//! minibuffer UI boundary.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, IDO_UBIQUITOUS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'ido)
(require 'ido-completing-read+)

(get-buffer-create " *code-conversion-work*")

(defconst iub417-test-upstream-main-sha
  "a286f3a58e19ae87ae69220b2c7ccf8b3bed545936e5ec9bcb91a38262ab9a42")
(defconst iub417-test-installed-main-sha
  "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")
(defconst iub417-test-installed-pkg-sha
  "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112")

(defvar iub417-test-ido-plan nil)
(defvar iub417-test-ido-ledger nil)
(defvar iub417-test-fallback-plan nil)
(defvar iub417-test-fallback-ledger nil)
(defvar iub417-test-dynamic-ledger nil)
(defvar iub417-test-history nil)

(defun iub417-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun iub417-test-source-state ()
  (let* ((main (file-truename (locate-library "ido-completing-read+.el")))
         (pkg (expand-file-name "ido-completing-read+-pkg.el"
                                (file-name-directory main)))
         (manifest
          (list (cons "ido-completing-read+-pkg.el" (iub417-test-file-sha pkg))
                (cons "ido-completing-read+.el" (iub417-test-file-sha main)))))
    (unless (and (file-regular-p main) (not (file-symlink-p main))
                 (file-regular-p pkg) (not (file-symlink-p pkg))
                 (equal manifest
                        `(("ido-completing-read+-pkg.el"
                           . ,iub417-test-installed-pkg-sha)
                          ("ido-completing-read+.el"
                           . ,iub417-test-installed-main-sha))))
      (error "Ido Completing Read+ installed source mismatch: %S" manifest))
    (list :upstream-sha256 iub417-test-upstream-main-sha
          :installed-sha256 manifest
          :version
          (package-version-join
           (package-desc-version
            (cadr (assq 'ido-completing-read+ package-alist))))
          :feature (featurep 'ido-completing-read+)
          :advice
          (mapcar
           (lambda (entry)
             (cons (car entry) (and (advice-member-p (cdr entry) (car entry)) t)))
           '((call-interactively . call-interactively@ido-cr+-record-current-command)
             (ido-select-text . ido-select-text@ido-cr+-fix-require-match)
             (ido-complete . ido-complete@ido-cr+-update-dynamic-collection)
             (ido-restrict-to-matches
              . ido-restrict-to-matches@ido-cr+-record-restriction))))))

(defun iub417-test-normalize-value (value)
  (cond
   ((stringp value)
    (list :text (substring-no-properties value)
          :properties
          (let ((position 0) runs)
            (while (< position (length value))
              (let ((next (next-property-change position value (length value))))
                (when (text-properties-at position value)
                  (push (list position next
                              (copy-tree (text-properties-at position value)))
                        runs))
                (setq position next)))
            (nreverse runs))))
   ((consp value) (mapcar #'iub417-test-normalize-value value))
   (t value)))

(defun iub417-test-ido-read
    (prompt collection predicate require-match initial-input hist def
            inherit-input-method)
  (unless iub417-test-ido-plan
    (error "Unexpected ido-completing-read invocation"))
  (let ((plan (pop iub417-test-ido-plan)))
    (push
     (list :prompt prompt :collection (copy-tree collection)
           :predicate predicate :require-match require-match
           :initial-input (copy-tree initial-input) :history hist
           :default (copy-tree def)
           :inherit-input-method inherit-input-method
           :minibuffer-depth ido-cr+-minibuffer-depth
           :dynamic (and ido-cr+-dynamic-collection t))
     iub417-test-ido-ledger)
    (when (plist-get plan :clear-hook)
      (setq minibuffer-setup-hook nil))
    (when (plist-member plan :exit)
      (setq ido-exit (plist-get plan :exit)))
    (copy-sequence (plist-get plan :result))))

(defun iub417-test-fallback
    (prompt collection predicate require-match initial-input hist def
            inherit-input-method)
  (unless iub417-test-fallback-plan
    (error "Unexpected Ido Completing Read+ fallback"))
  (let ((result (pop iub417-test-fallback-plan)))
    (push
     (list :prompt prompt :collection (copy-tree collection)
           :predicate predicate :require-match require-match
           :initial-input (copy-tree initial-input) :history hist
           :default (copy-tree def)
           :inherit-input-method inherit-input-method
           :marker-hook (and (memq #'iub417-test-marker-hook
                                   minibuffer-setup-hook) t))
     iub417-test-fallback-ledger)
    result))

(defun iub417-test-marker-hook ())
(defun iub417-test-non-alpha-p (candidate)
  (not (string= candidate "alpha")))
(defun iub417-test-disabled-command ()
  (completing-read "Disabled: " '("alpha" "café" "界面") nil t))
(defun iub417-test-allowed-command ()
  (completing-read "Allowed: " '("alpha" "café" "界面") nil t))
(defun iub417-test-unlisted-command ()
  (completing-read "Unlisted: " '("alpha" "café" "界面") nil t))

(defun iub417-test-dynamic-collection (string predicate action)
  (push (list string predicate action) iub417-test-dynamic-ledger)
  (complete-with-action action '("cacao" "café" "界面") string predicate))

(defun iub417-test-forbid-external (operation &rest arguments)
  (error "Unexpected Ido Completing Read+ external boundary: %S %S"
         operation arguments))

(defun iub417-test-window-state ()
  (length (window-list nil 'no-minibuffer)))

(defun iub417-test-run (body)
  (let* ((buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (window-state-before (iub417-test-window-state))
         (source-before (iub417-test-source-state))
         (completing-read-before completing-read-function)
         (mode-before ido-ubiquitous-mode)
         (disable-before ido-cr+-disable-list)
         (allow-before ido-cr+-allow-list)
         (max-before ido-cr+-max-items)
         (fallback-before ido-cr+-fallback-function)
         (current-command-before ido-cr+-current-command)
         (ido-exit-before ido-exit)
         (minibuffer-hook-before minibuffer-setup-hook)
         (history-before iub417-test-history)
         (completing-read-function completing-read-function)
         (ido-ubiquitous-mode ido-ubiquitous-mode)
         (ido-cr+-disable-list (copy-tree ido-cr+-disable-list))
         (ido-cr+-allow-list (copy-tree ido-cr+-allow-list))
         (ido-cr+-max-items ido-cr+-max-items)
         (ido-cr+-fallback-function #'iub417-test-fallback)
         (ido-cr+-current-command nil)
         (ido-cr+-dynamic-update-timer nil)
         (ido-exit ido-exit)
         (minibuffer-setup-hook (copy-sequence minibuffer-setup-hook))
         (iub417-test-history (copy-tree iub417-test-history))
         (iub417-test-ido-plan nil)
         (iub417-test-ido-ledger nil)
         (iub417-test-fallback-plan nil)
         (iub417-test-fallback-ledger nil)
         (iub417-test-dynamic-ledger nil)
         result source-after cleanup-errors)
    (unwind-protect
        (cl-letf (((symbol-function 'ido-completing-read) #'iub417-test-ido-read)
                  ((symbol-function 'call-process)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external 'call-process args)))
                  ((symbol-function 'call-process-region)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external
                            'call-process-region args)))
                  ((symbol-function 'make-process)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external 'make-process args)))
                  ((symbol-function 'process-file)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external 'process-file args)))
                  ((symbol-function 'start-file-process)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external
                            'start-file-process args)))
                  ((symbol-function 'start-process)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external 'start-process args)))
                  ((symbol-function 'url-retrieve)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external 'url-retrieve args)))
                  ((symbol-function 'url-retrieve-synchronously)
                   (lambda (&rest args)
                     (apply #'iub417-test-forbid-external
                            'url-retrieve-synchronously args))))
          (setq result (funcall body))
          (when iub417-test-ido-plan
            (error "Unused ido UI plan: %S" iub417-test-ido-plan))
          (when iub417-test-fallback-plan
            (error "Unused fallback plan: %S" iub417-test-fallback-plan))
          (setq source-after (iub417-test-source-state))
          (unless (equal source-before source-after)
            (error "Ido Completing Read+ source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition) (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (when ido-cr+-dynamic-update-timer
          (attempt 'dynamic-timer
                   (lambda () (cancel-timer ido-cr+-dynamic-update-timer)))
          (setq ido-cr+-dynamic-update-timer nil))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (setq completing-read-function completing-read-before
              ido-ubiquitous-mode mode-before
              ido-cr+-disable-list disable-before
              ido-cr+-allow-list allow-before
              ido-cr+-max-items max-before
              ido-cr+-fallback-function fallback-before
              ido-cr+-current-command current-command-before
              ido-exit ido-exit-before
              minibuffer-setup-hook minibuffer-hook-before
              iub417-test-history history-before)
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (b) (memq b buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (p) (memq p processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :window-restored
                 (equal window-state-before (iub417-test-window-state))
                 :state-restored
                 (and (eq completing-read-function completing-read-before)
                      (eq ido-ubiquitous-mode mode-before)
                      (eq ido-cr+-disable-list disable-before)
                      (eq ido-cr+-allow-list allow-before)
                      (eq ido-cr+-max-items max-before)
                      (eq ido-cr+-fallback-function fallback-before)
                      (eq ido-cr+-current-command current-command-before)
                      (eq ido-exit ido-exit-before)
                      (eq minibuffer-setup-hook minibuffer-hook-before)
                      (eq iub417-test-history history-before))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Ido Completing Read+ cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IDO_UBIQUITOUS_MELPA_PIN, "ido-completing-read+.el")
        .expect("prepare exact ido-completing-read+ source and memoize closure below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_mode_transforms_collection_defaults_and_result_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mode_transforms_collection_defaults_and_result_properties",
        r####"
(iub417-test-run
 (lambda ()
   (setq ido-cr+-disable-list nil
         ido-cr+-allow-list nil
         iub417-test-ido-plan
         (list (list :result (propertize "café" 'display "abbreviated"))))
   (let ((before completing-read-function))
     (ido-ubiquitous-mode 1)
     (let* ((enabled (list ido-ubiquitous-mode completing-read-function))
            (answer
             (completing-read
              "Choose: " '("alpha" "café" "界面") #'iub417-test-non-alpha-p
              t '("ca" . 1) 'iub417-test-history '("界面" "café") nil))
            (answer-state (iub417-test-normalize-value answer)))
       (ido-ubiquitous-mode -1)
       (list :before before :enabled enabled :answer answer-state
             :ui (nreverse iub417-test-ido-ledger)
             :disabled (list ido-ubiquitous-mode completing-read-function))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "a286f3a58e19ae87ae69220b2c7ccf8b3bed545936e5ec9bcb91a38262ab9a42" :installed-sha256 (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :version "20240130.30" :feature t :advice ((call-interactively . t) (ido-select-text . t) (ido-complete . t) (ido-restrict-to-matches . t))) :result (:before completing-read-default :enabled (t ido-completing-read+) :answer (:text "café" :properties nil) :ui ((:prompt "Choose: " :collection ("界面" "café") :predicate iub417-test-non-alpha-p :require-match t :initial-input ("ca" . 1) :history iub417-test-history :default nil :inherit-input-method nil :minibuffer-depth 1 :dynamic nil)) :disabled (nil iub417-test-fallback)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_caller_policies_choose_ido_or_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_caller_policies_choose_ido_or_fallback",
        r####"
(iub417-test-run
 (lambda ()
   (ido-ubiquitous-mode 1)
   (setq ido-cr+-disable-list '(iub417-test-disabled-command)
         ido-cr+-allow-list nil
         iub417-test-fallback-plan '("disabled-fallback"))
   (let ((disabled (iub417-test-disabled-command)))
     (setq ido-cr+-disable-list nil
           ido-cr+-allow-list '(iub417-test-allowed-command)
           iub417-test-ido-plan '((:result "café"))
           iub417-test-fallback-plan '("unlisted-fallback"))
     (let ((allowed (iub417-test-allowed-command))
           (unlisted (iub417-test-unlisted-command)))
       (list :disabled disabled :allowed allowed :unlisted unlisted
             :ido (nreverse iub417-test-ido-ledger)
             :fallback (nreverse iub417-test-fallback-ledger))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "a286f3a58e19ae87ae69220b2c7ccf8b3bed545936e5ec9bcb91a38262ab9a42" :installed-sha256 (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :version "20240130.30" :feature t :advice ((call-interactively . t) (ido-select-text . t) (ido-complete . t) (ido-restrict-to-matches . t))) :result (:disabled "disabled-fallback" :allowed "café" :unlisted "unlisted-fallback" :ido ((:prompt "Allowed: " :collection ("" "alpha" "café" "界面") :predicate nil :require-match t :initial-input nil :history nil :default nil :inherit-input-method nil :minibuffer-depth 1 :dynamic nil)) :fallback ((:prompt "Disabled: " :collection ("alpha" "café" "界面") :predicate nil :require-match t :initial-input nil :history nil :default nil :inherit-input-method nil :marker-hook nil) (:prompt "Unlisted: " :collection ("alpha" "café" "界面") :predicate nil :require-match t :initial-input nil :history nil :default nil :inherit-input-method nil :marker-hook nil))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_dynamic_collection_expands_prefixes_and_preserves_unicode() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_dynamic_collection_expands_prefixes_and_preserves_unicode",
        r####"
(iub417-test-run
 (lambda ()
   (setq ido-cr+-disable-list nil
         ido-cr+-allow-list nil
         iub417-test-ido-plan '((:result "café")))
   (let ((answer
          (ido-completing-read+
           "Dynamic: " #'iub417-test-dynamic-collection nil t "ca"
           'iub417-test-history nil nil)))
     (list :answer answer
           :collection-calls (nreverse iub417-test-dynamic-ledger)
           :ui (nreverse iub417-test-ido-ledger)
           :memoize-loaded (featurep 'memoize)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "a286f3a58e19ae87ae69220b2c7ccf8b3bed545936e5ec9bcb91a38262ab9a42" :installed-sha256 (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :version "20240130.30" :feature t :advice ((call-interactively . t) (ido-select-text . t) (ido-complete . t) (ido-restrict-to-matches . t))) :result (:answer "café" :collection-calls (("" nil t) ("c" nil t) ("ca" nil t)) :ui ((:prompt "Dynamic: " :collection ("" "cacao" "café" "界面") :predicate nil :require-match t :initial-input "ca" :history iub417-test-history :default nil :inherit-input-method nil :minibuffer-depth 1 :dynamic t)) :memoize-loaded t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_manual_fallback_restores_temporary_minibuffer_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_manual_fallback_restores_temporary_minibuffer_hooks",
        r####"
(iub417-test-run
 (lambda ()
   (setq ido-cr+-disable-list nil
         ido-cr+-allow-list nil
         minibuffer-setup-hook '(iub417-test-marker-hook)
         iub417-test-ido-plan
         '((:result "ignored" :exit fallback :clear-hook t))
         iub417-test-fallback-plan '("fallback-界"))
   (let ((answer
          (ido-completing-read+
           "Manual: " '("alpha" "café" "界面") nil t "ca"
           'iub417-test-history "café" nil)))
     (list :answer answer
           :ui (nreverse iub417-test-ido-ledger)
           :fallback (nreverse iub417-test-fallback-ledger)
           :hook-after (and (memq #'iub417-test-marker-hook
                                  minibuffer-setup-hook) t)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "a286f3a58e19ae87ae69220b2c7ccf8b3bed545936e5ec9bcb91a38262ab9a42" :installed-sha256 (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :version "20240130.30" :feature t :advice ((call-interactively . t) (ido-select-text . t) (ido-complete . t) (ido-restrict-to-matches . t))) :result (:answer "fallback-界" :ui ((:prompt "Manual: " :collection ("café" "alpha" "界面") :predicate nil :require-match t :initial-input "ca" :history iub417-test-history :default nil :inherit-input-method nil :minibuffer-depth 1 :dynamic nil)) :fallback ((:prompt "Manual: " :collection ("alpha" "café" "界面") :predicate nil :require-match t :initial-input "ca" :history iub417-test-history :default "café" :inherit-input-method nil :marker-hook t)) :hook-after nil) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_unsupported_inputs_fall_back_with_original_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_unsupported_inputs_fall_back_with_original_arguments",
        r####"
(iub417-test-run
 (lambda ()
   (setq ido-cr+-disable-list nil
         ido-cr+-allow-list nil
         ido-cr+-max-items 2
         iub417-test-fallback-plan '("large" "empty" "input-method"))
   (let ((large
          (ido-completing-read+ "Large: " '("a" "b" "c") nil nil nil nil nil nil))
         (empty
          (ido-completing-read+ "Empty: " nil nil nil nil nil nil nil))
         (input-method
          (let ((current-input-method "latin-1-prefix"))
            (ido-completing-read+
             "Input: " '("a" "b") nil nil nil nil nil t))))
     (list :large large :empty empty :input-method input-method
           :fallback (nreverse iub417-test-fallback-ledger)
           :ido (nreverse iub417-test-ido-ledger)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "a286f3a58e19ae87ae69220b2c7ccf8b3bed545936e5ec9bcb91a38262ab9a42" :installed-sha256 (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :version "20240130.30" :feature t :advice ((call-interactively . t) (ido-select-text . t) (ido-complete . t) (ido-restrict-to-matches . t))) :result (:large "large" :empty "empty" :input-method "input-method" :fallback ((:prompt "Large: " :collection ("a" "b" "c") :predicate nil :require-match nil :initial-input nil :history nil :default nil :inherit-input-method nil :marker-hook nil) (:prompt "Empty: " :collection nil :predicate nil :require-match nil :initial-input nil :history nil :default nil :inherit-input-method nil :marker-hook nil) (:prompt "Input: " :collection ("a" "b") :predicate nil :require-match nil :initial-input nil :history nil :default nil :inherit-input-method t :marker-hook nil)) :ido nil) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn ido_ubiquitous_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_mode_transforms_collection_defaults_and_result_properties(),
        public_caller_policies_choose_ido_or_fallback(),
        public_dynamic_collection_expands_prefixes_and_preserves_unicode(),
        public_manual_fallback_restores_temporary_minibuffer_hooks(),
        public_unsupported_inputs_fall_back_with_original_arguments(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "ido-ubiquitous-rank417",
        "ido_ubiquitous_parity",
        &cases,
    );
}
