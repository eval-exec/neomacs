use std::time::Duration;

use crate::{ACE_JUMP_HELM_LINE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_JUMP_HELM_LINE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-jump-helm-line labels the candidate lines of a *live* helm session, so
/// every workflow needs a real helm buffer, a real helm window, real
/// `helm-alive-p' state and real keys: avy reads the label with `read-key' from
/// the executing keyboard macro, and the package then drives helm's own
/// selection movement and actions.  Nothing in the package is stubbed; the
/// helm source below is an ordinary user source whose persistent action and
/// action record what helm handed them.
///
/// `ajhl-test-with-helm-session' starts the session with helm's own startup
/// sequence -- `helm-initialize', `helm-display-buffer', `select-window' of
/// `helm-window' and the `helm-update' that `helm-read-from-minibuffer' runs --
/// but stops short of the `read-from-minibuffer' call itself, because Neomacs
/// reads stdin instead of the keyboard macro for any minibuffer prompt and a
/// complete helm session therefore cannot be driven in batch there (see the
/// first workflow, which does drive a complete `helm' session and is left
/// failing on that divergence).
const ACE_JUMP_HELM_LINE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'helm)
(require 'avy)

(defvar ajhl-test-actions nil
  "Everything the helm source was asked to do, newest first.")

(defvar ajhl-test-result nil
  "The value a complete `helm' session returned.")

(defun ajhl-test-setup ()
  "Pin avy's label alphabet and bind the package's commands to real keys."
  (setq avy-keys '(?a ?s ?d ?f ?g ?h ?j ?k ?l)
        avy-style 'at-full
        avy-all-windows nil
        ajhl-test-actions nil
        ajhl-test-result nil)
  (global-set-key (kbd "C-c j") 'ace-jump-helm-line)
  (global-set-key (kbd "C-c s") 'ace-jump-helm-line-and-select)
  (global-set-key (kbd "C-c h") 'ajhl-test-run-helm))

(defun ajhl-test-source ()
  "A realistic helm source: five deployment targets a user picks from."
  (helm-build-sync-source "Deploy targets"
    :candidates '("alpha-api" "bravo-worker" "charlie-cache" "delta-db" "echo-cdn")
    :persistent-action (lambda (candidate)
                         (push (list 'persistent candidate) ajhl-test-actions))
    :action (list (cons "Deploy"
                        (lambda (candidate)
                          (push (list 'deploy candidate) ajhl-test-actions)
                          (format "deployed %s" candidate))))))

(defun ajhl-test-run-helm ()
  "Start a complete helm session, exactly as a helm command does."
  (interactive)
  (setq ajhl-test-actions nil)
  (setq ajhl-test-result
        (helm :sources (list (ajhl-test-source)) :buffer "*helm ajhl*")))

(defmacro ajhl-test-with-helm-session (&rest body)
  "Run BODY with a live helm session, built by helm's own startup sequence."
  `(let ((helm-buffer "*helm ajhl*"))
     (setq ajhl-test-actions nil)
     ;; Every workflow starts from the state a fresh editor is in.
     (when (get-buffer helm-buffer)
       (kill-buffer helm-buffer))
     (unwind-protect
         (progn
           (helm-initialize nil nil nil (list (ajhl-test-source)))
           (helm-display-buffer helm-buffer nil)
           (select-window (helm-window))
           (with-helm-buffer (helm-update))
           ,@body)
       (helm-cleanup)
       (setq helm-alive-p nil))))

(defun ajhl-test-state ()
  "The helm state a jump is supposed to move."
  (list :selection (helm-get-selection)
        :point (with-helm-buffer (point))
        :line (with-helm-buffer (line-number-at-pos))
        :selection-overlay (list (overlay-start helm-selection-overlay)
                                 (overlay-end helm-selection-overlay))
        :alive helm-alive-p
        :actions ajhl-test-actions))

(defun ajhl-test-candidate-text ()
  (with-helm-buffer (buffer-substring-no-properties (point-min) (point-max))))

(defun ajhl-test-labels ()
  "Return (START END LABEL FACE) for every avy label overlay, in line order."
  (with-helm-buffer
    (delq nil
          (mapcar (lambda (overlay)
                    (let ((display (overlay-get overlay 'display)))
                      (when display
                        (list (overlay-start overlay)
                              (overlay-end overlay)
                              (substring-no-properties display)
                              (get-text-property 0 'face display)))))
                  (sort (overlays-in (point-min) (point-max))
                        (lambda (a b) (< (overlay-start a) (overlay-start b))))))))

(defun ajhl-test-linum-labels ()
  "Return (START MARGIN-STRING) for every linum overlay, in line order."
  (with-helm-buffer
    (delq nil
          (mapcar (lambda (overlay)
                    (let ((before (overlay-get overlay 'before-string)))
                      (when before
                        (list (overlay-start overlay)
                              (let ((display (get-text-property 0 'display before)))
                                (and display
                                     (substring-no-properties (car (last display)))))))))
                  (sort (overlays-in (point-min) (point-max))
                        (lambda (a b) (< (overlay-start a) (overlay-start b))))))))

(defun ajhl-test-idle-timers ()
  "Return the pending idle-execution timers the package scheduled."
  (delq nil
        (mapcar (lambda (timer)
                  (when (eq (timer--function timer) 'ace-jump-helm-line--do-if-empty)
                    (let ((delay (float-time (time-subtract (timer--time timer)
                                                            (current-time)))))
                      (list (timer--function timer)
                            (timer--args timer)
                            (timer--repeat-delay timer)
                            (and (> delay 0)
                                 (<= delay ace-jump-helm-line-idle-delay))))))
                timer-list)))

(defun ajhl-test-cancel-idle-timers ()
  (dolist (timer (copy-sequence timer-list))
    (when (eq (timer--function timer) 'ace-jump-helm-line--do-if-empty)
      (cancel-timer timer))))
"##;

fn ace_jump_helm_line_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_JUMP_HELM_LINE_MELPA_PIN, "ace-jump-helm-line.el")
        .expect("prepare pinned ace-jump-helm-line source below ./tmp")
        .with_prelude(ACE_JUMP_HELM_LINE_TEST_PRELUDE)
        .with_timeout(ACE_JUMP_HELM_LINE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-jump-helm-line parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_jump_helm_line_parity` cases (2a).
pub(crate) fn assert_ace_jump_helm_line_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ace_jump_helm_line_oracle(),
        &name,
        "ace_jump_helm_line_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ace_jump_helm_line_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_jump_helm_line_batch(&cases);
}

// END generated package batch tests
