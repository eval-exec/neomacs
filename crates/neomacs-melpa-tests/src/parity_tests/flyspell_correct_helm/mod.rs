//! Practical parity for Flyspell Correct's documented Helm interface.
//!
//! The cases drive the public adapter through one real Helm session and a
//! narrow UI boundary, pinning suggestion selection, control actions,
//! dictionary precedence, abort/recovery, interface registration, and cleanup.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLYSPELL_CORRECT_HELM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'flyspell-correct-helm)

(defconst fch395-test-source-sha256
  "e87dd25df0fd1f8acaf1e7478c63792d3c1e6e1f55c751573595df6e8961703c")

(defun fch395-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let ((file (symbol-file 'flyspell-correct-helm 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file) "flyspell-correct-helm.el")
               (equal (fch395-test-file-sha256 file)
                      fch395-test-source-sha256))
    (error "Unexpected installed Flyspell Correct Helm source: %S" file)))

(defun fch395-test-condition (condition)
  (list :type (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun fch395-test-snapshot (value)
  (cond ((stringp value) (copy-sequence value))
        ((consp value)
         (cons (fch395-test-snapshot (car value))
               (fch395-test-snapshot (cdr value))))
        ((vectorp value)
         (apply #'vector (mapcar #'fch395-test-snapshot value)))
        (t value)))

(defun fch395-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((old-name (buffer-name)))
        (rename-buffer
         (format " *fch395-parked-%s*" (sxhash-eq buffer)) t)
        (cons buffer old-name)))))

(defun fch395-test-window-state ()
  (list :selected (selected-window)
        :windows
        (mapcar
         (lambda (window)
           (list :window window :buffer (window-buffer window)
                 :point (window-point window) :start (window-start window)
                 :prev (copy-tree (window-prev-buffers window))
                 :next (copy-tree (window-next-buffers window))))
         (window-list nil 'no-minibuf))))

(defun fch395-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry (plist-get state :windows))
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Baseline Flyspell Correct Helm window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce))))

(defun fch395-test-run (body)
  (let* ((window-before (current-window-configuration))
         (window-state-before (fch395-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (interface-before flyspell-correct-interface)
         (parked nil)
         (flyspell-correct-interface flyspell-correct-interface)
         (helm-pattern "")
         (helm-turn-on-show-completion nil)
         (helm-input-idle-delay 0)
         (print-circle nil)
         result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (setq parked (fch395-test-park-buffer "*Helm Flyspell*"))
              (cl-letf (((symbol-function 'make-process)
                         (lambda (&rest arguments)
                           (error "Unexpected process: %S" arguments)))
                        ((symbol-function 'start-process)
                         (lambda (&rest arguments)
                           (error "Unexpected process start: %S" arguments)))
                        ((symbol-function 'call-process)
                         (lambda (&rest arguments)
                           (error "Unexpected synchronous process: %S"
                                  arguments)))
                        ((symbol-function 'process-file)
                         (lambda (&rest arguments)
                           (error "Unexpected file process: %S" arguments)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest arguments)
                           (error "Unexpected network process: %S" arguments)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest arguments)
                           (error "Unexpected URL retrieval: %S" arguments))))
                (save-window-excursion
                  (save-current-buffer
                    (setq result (funcall body))))))
          (t (setq body-error (fch395-test-condition condition))))
      (condition-case condition
          (fch395-test-restore-windows window-before window-state-before)
        (t (push (fch395-test-condition condition) cleanup-errors)))
      (dolist (process (seq-difference (process-list) processes-before #'eq))
        (condition-case condition (delete-process process)
          (t (push (fch395-test-condition condition) cleanup-errors))))
      (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (set-buffer-modified-p nil)
                  (kill-buffer buffer))))
          (t (push (fch395-test-condition condition) cleanup-errors))))
      (dolist (timer (seq-difference timer-list timers-before #'eq))
        (condition-case condition (cancel-timer timer)
          (t (push (fch395-test-condition condition) cleanup-errors))))
      (dolist (frame (seq-difference (frame-list) frames-before #'eq))
        (condition-case condition (delete-frame frame t)
          (t (push (fch395-test-condition condition) cleanup-errors))))
      (when parked
        (condition-case condition
            (if (buffer-live-p (car parked))
                (with-current-buffer (car parked)
                  (rename-buffer (cdr parked) t))
              (error "Parked Flyspell Correct Helm buffer died"))
          (t (push (fch395-test-condition condition) cleanup-errors))))
      (condition-case condition
          (fch395-test-restore-windows window-before window-state-before)
        (t (push (fch395-test-condition condition) cleanup-errors)))
      (condition-case condition
          (when (buffer-live-p buffer-before) (set-buffer buffer-before))
        (t (push (fch395-test-condition condition) cleanup-errors))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list)
                                                     buffers-before #'eq)))
                 :new-processes
                 (length (seq-difference (process-list) processes-before #'eq))
                 :new-timers
                 (length (seq-difference timer-list timers-before #'eq))
                 :new-frames
                 (length (seq-difference (frame-list) frames-before #'eq))
                 :interface-restored (eq flyspell-correct-interface
                                         interface-before)
                 :window-restored
                 (equal (fch395-test-window-state) window-state-before)
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Flyspell Correct Helm workflow failed: %S"
                 (list result cleanup))
        (fch395-test-snapshot
         (list :source fch395-test-source-sha256
               :result result :cleanup cleanup))))))

(defvar fch395-test-real-result nil)
(defvar fch395-test-real-candidates nil)
(defvar fch395-test-real-word nil)

(defun fch395-test-real-command ()
  (interactive)
  (setq fch395-test-real-result
        (condition-case condition
            (flyspell-correct-helm fch395-test-real-candidates
                                   fch395-test-real-word)
          (quit (list :quit (fch395-test-condition condition))))))

(defun fch395-test-helm-lines ()
  (when-let* ((buffer (get-buffer "*Helm Flyspell*")))
    (with-current-buffer buffer
      (split-string (buffer-substring-no-properties (point-min) (point-max))
                    "\n" t))))

(defun fch395-test-real-session (candidates word keys)
  (let ((old-binding (lookup-key global-map (kbd "C-c ;")))
        (fch395-test-real-candidates candidates)
        (fch395-test-real-word word)
        (fch395-test-real-result 'unset))
    (unwind-protect
        (progn
          (global-set-key (kbd "C-c ;") #'fch395-test-real-command)
          (execute-kbd-macro (kbd (concat "C-c ; " keys)))
          (list :result fch395-test-real-result
                :lines (fch395-test-helm-lines)))
      (define-key global-map (kbd "C-c ;") old-binding))))

(defun fch395-test-candidate-state (candidate)
  (if (consp candidate)
      (list :display (copy-sequence (car candidate))
            :value (copy-tree (cdr candidate)))
    (list :display (copy-sequence candidate)
          :value (copy-sequence candidate))))

(defun fch395-test-source-state (source pattern)
  (let* ((helm-pattern pattern)
         (raw (helm-attr 'candidates source))
         (candidates (if (functionp raw) (funcall raw) raw))
         (match (helm-attr 'match source)))
    (list :name (helm-attr 'name source)
          :candidates (mapcar #'fch395-test-candidate-state candidates)
          :action (helm-attr 'action source)
          :limit (helm-attr 'candidate-number-limit source)
          :fuzzy (helm-attr 'fuzzy-match source)
          :volatile (helm-attr 'volatile source)
          :match match
          :match-anything
          (and (memq #'flyspell-correct-helm--always-match
                     (if (listp match) match (list match)))
               (flyspell-correct-helm--always-match "anything")))))

(defun fch395-test-public-through-ui (function pattern source-index display)
  (let (boundary)
    (cl-letf
        (((symbol-function 'helm)
          (lambda (&rest arguments)
            (let* ((sources (plist-get arguments :sources))
                   (states (mapcar
                            (lambda (source)
                              (fch395-test-source-state source pattern))
                            sources))
                   (state (nth source-index states))
                   (candidate
                    (seq-find
                     (lambda (entry)
                       (equal (plist-get entry :display) display))
                     (plist-get state :candidates))))
              (unless (and (= (length sources) 2)
                           (equal (plist-get arguments :buffer)
                                  "*Helm Flyspell*")
                           (equal (plist-get arguments :prompt) "Correction: ")
                           candidate)
                (error "Unexpected Helm boundary: %S" arguments))
              (setq boundary
                    (list :buffer (plist-get arguments :buffer)
                          :prompt (plist-get arguments :prompt)
                          :pattern pattern :sources states
                          :selected (copy-tree candidate)))
              (plist-get candidate :value)))))
      (let ((value (funcall function)))
        (list :value value :boundary boundary)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYSPELL_CORRECT_HELM_MELPA_PIN, "flyspell-correct-helm.el")
        .expect("prepare exact shallow Flyspell Correct Helm source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn real_helm_session_selects_the_first_unicode_suggestion() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_helm_session_selects_the_first_unicode_suggestion",
        r####"
(fch395-test-run
 (lambda ()
   (let ((ispell-local-dictionary "français")
         (ispell-dictionary "english"))
     (fch395-test-real-session
      '("café" "caffè" "coffee") "cafee" "RET"))))
"####,
        expect![[
            r#"OK (:source "e87dd25df0fd1f8acaf1e7478c63792d3c1e6e1f55c751573595df6e8961703c" :result (:result "café" :lines ("Suggestions for \"cafee\" in dictionary \"français\"" "café" "caffè" "coffee" "Options" "Save \"cafee\"" "Accept (session) \"cafee\"" "Accept (buffer) \"cafee\"" "Skip \"cafee\"" "Stop at \"cafee\"")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :interface-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn real_helm_abort_preserves_nil_and_a_second_session_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_helm_abort_preserves_nil_and_a_second_session_recovers",
        r####"
(fch395-test-run
 (lambda ()
   (let ((first (fch395-test-real-session
                 '("receive" "recipe") "recieve" "C-g")))
     (list :abort first
           :recovery
           (fch395-test-real-session
            '("receive" "recipe") "recieve" "RET")))))
"####,
        expect![[
            r#"OK (:source "e87dd25df0fd1f8acaf1e7478c63792d3c1e6e1f55c751573595df6e8961703c" :result (:abort (:result nil :lines ("Suggestions for \"recieve\" in dictionary \"Default\"" "receive" "recipe" "Options" "Save \"recieve\"" "Accept (session) \"recieve\"" "Accept (buffer) \"recieve\"" "Skip \"recieve\"" "Stop at \"recieve\"")) :recovery (:result "receive" :lines ("Suggestions for \"recieve\" in dictionary \"Default\"" "receive" "recipe" "Options" "Save \"recieve\"" "Accept (session) \"recieve\"" "Accept (buffer) \"recieve\"" "Skip \"recieve\"" "Stop at \"recieve\""))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :interface-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn options_source_adds_custom_pattern_actions_without_filtering_controls() -> ParityBatchCase {
    ParityBatchCase::value(
        "options_source_adds_custom_pattern_actions_without_filtering_controls",
        r####"
(fch395-test-run
 (lambda ()
   (list
    :empty
    (fch395-test-public-through-ui
     (lambda () (flyspell-correct-helm '("the" "ten") "teh"))
     "" 1 "Skip \"teh\"")
    :custom
    (fch395-test-public-through-ui
     (lambda () (flyspell-correct-helm '("the" "ten") "teh"))
     "their界" 1 "Save \"their界\""))))
"####,
        expect![[
            r#"OK (:source "e87dd25df0fd1f8acaf1e7478c63792d3c1e6e1f55c751573595df6e8961703c" :result (:empty (:value (skip . "teh") :boundary (:buffer "*Helm Flyspell*" :prompt "Correction: " :pattern "" :sources ((:name "Suggestions for \"teh\" in dictionary \"Default\"" :candidates ((:display "the" :value "the") (:display "ten" :value "ten")) :action identity :limit 9999 :fuzzy t :volatile nil :match (helm-mm-exact-match helm-mm-match helm-fuzzy-match) :match-anything nil) (:name "Options" :candidates ((:display "Save \"teh\"" :value (save . "teh")) (:display "Accept (session) \"teh\"" :value (session . "teh")) (:display "Accept (buffer) \"teh\"" :value (buffer . "teh")) (:display "Skip \"teh\"" :value (skip . "teh")) (:display "Stop at \"teh\"" :value (stop . "teh"))) :action identity :limit 9999 :fuzzy nil :volatile t :match (helm-mm-exact-match helm-mm-match flyspell-correct-helm--always-match) :match-anything t)) :selected (:display "Skip \"teh\"" :value (skip . "teh")))) :custom (:value (save . "their界") :boundary (:buffer "*Helm Flyspell*" :prompt "Correction: " :pattern "their界" :sources ((:name "Suggestions for \"teh\" in dictionary \"Default\"" :candidates ((:display "the" :value "the") (:display "ten" :value "ten")) :action identity :limit 9999 :fuzzy t :volatile nil :match (helm-mm-exact-match helm-mm-match helm-fuzzy-match) :match-anything nil) (:name "Options" :candidates ((:display "Save \"teh\"" :value (save . "teh")) (:display "Accept (session) \"teh\"" :value (session . "teh")) (:display "Accept (buffer) \"teh\"" :value (buffer . "teh")) (:display "Skip \"teh\"" :value (skip . "teh")) (:display "Stop at \"teh\"" :value (stop . "teh")) (:display "Save \"their界\"" :value (save . "their界")) (:display "Accept (session) \"their界\"" :value (session . "their界")) (:display "Accept (buffer) \"their界\"" :value (buffer . "their界"))) :action identity :limit 9999 :fuzzy nil :volatile t :match (helm-mm-exact-match helm-mm-match flyspell-correct-helm--always-match) :match-anything t)) :selected (:display "Save \"their界\"" :value (save . "their界"))))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :interface-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn package_load_registers_the_public_interface_and_default_dictionary() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_load_registers_the_public_interface_and_default_dictionary",
        r####"
(fch395-test-run
 (lambda ()
   (let ((ispell-local-dictionary nil)
         (ispell-dictionary nil))
     (list
      :registered (eq flyspell-correct-interface #'flyspell-correct-helm)
      :call
      (fch395-test-public-through-ui
       (lambda ()
         (funcall flyspell-correct-interface '("naïve" "native") "naive"))
       "" 0 "naïve")))))
"####,
        expect![[
            r#"OK (:source "e87dd25df0fd1f8acaf1e7478c63792d3c1e6e1f55c751573595df6e8961703c" :result (:registered t :call (:value "naïve" :boundary (:buffer "*Helm Flyspell*" :prompt "Correction: " :pattern "" :sources ((:name "Suggestions for \"naive\" in dictionary \"Default\"" :candidates ((:display "naïve" :value "naïve") (:display "native" :value "native")) :action identity :limit 9999 :fuzzy t :volatile nil :match (helm-mm-exact-match helm-mm-match helm-fuzzy-match) :match-anything nil) (:name "Options" :candidates ((:display "Save \"naive\"" :value (save . "naive")) (:display "Accept (session) \"naive\"" :value (session . "naive")) (:display "Accept (buffer) \"naive\"" :value (buffer . "naive")) (:display "Skip \"naive\"" :value (skip . "naive")) (:display "Stop at \"naive\"" :value (stop . "naive"))) :action identity :limit 9999 :fuzzy nil :volatile t :match (helm-mm-exact-match helm-mm-match flyspell-correct-helm--always-match) :match-anything t)) :selected (:display "naïve" :value "naïve")))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :interface-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn public_flyspell_correct_helm_workflows_match() {
    let cases: Vec<ParityBatchCase> = vec![
        real_helm_session_selects_the_first_unicode_suggestion(),
        real_helm_abort_preserves_nil_and_a_second_session_recovers(),
        options_source_adds_custom_pattern_actions_without_filtering_controls(),
        package_load_registers_the_public_interface_and_default_dictionary(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        std::thread::current()
            .name()
            .unwrap_or("unnamed Flyspell Correct Helm parity test"),
        "flyspell-correct-helm-rank395",
        &cases,
    );
}
