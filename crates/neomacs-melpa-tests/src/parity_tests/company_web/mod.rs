use std::time::Duration;

use crate::{
    COMPANY_MELPA_PIN, COMPANY_WEB_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN,
    EMMET_MODE_MELPA_PIN, JADE_MODE_MELPA_PIN, PUG_MODE_MELPA_PIN, SLIM_MODE_MELPA_PIN,
    WEB_COMPLETION_DATA_MELPA_PIN, WEB_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(240);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'emmet-mode)
(require 'company-web-html)
(require 'company-web-jade)
(require 'company-web-slim)
(require 'jade-mode)
(require 'pug-mode)
(require 'slim-mode)
(require 'web-mode)

(defun neomacs-company-web-test-prepare-session
    (mode backend contents &optional point-offset-from-end)
  "Prepare a real Company Web session over CONTENTS in MODE using BACKEND."
  (switch-to-buffer (current-buffer))
  (funcall mode)
  ;; Major modes normally install a shared map.  Give every batch case a
  ;; private copy before it adds realistic user bindings.
  (use-local-map (copy-keymap (current-local-map)))
  (setq-local company-backends (list backend)
              company-frontends '(company-pseudo-tooltip-frontend)
              company-idle-delay nil)
  (company-mode 1)
  (insert contents)
  (when point-offset-from-end
    (backward-char point-offset-from-end)))

(defun neomacs-company-web-test-plain-candidates ()
  "Return the active Company candidates without display properties."
  (mapcar #'substring-no-properties company-candidates))

(defun neomacs-company-web-test-candidate-snapshot (candidate)
  "Observe CANDIDATE through Company Web's public backend protocol."
  (list :text (substring-no-properties candidate)
        :annotation (company-call-backend 'annotation candidate)
        :framework (get-text-property 0 'annotation candidate)
        :inline-doc (get-text-property 0 'doc candidate)))

(defun neomacs-company-web-test-face-runs ()
  "Return exact face runs in the current documentation buffer."
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (or (next-single-property-change
                        position 'face nil (point-max))
                       (point-max))))
        (when face
          (push (list position next face
                      (buffer-substring-no-properties position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-company-web-test-doc-snapshot (candidate)
  "Open and fully observe CANDIDATE's real Company documentation buffer."
  (let ((buffer (company-call-backend 'doc-buffer candidate)))
    (and buffer
         (with-current-buffer buffer
           (font-lock-ensure)
           (list :buffer (buffer-name)
                 :mode major-mode
                 :read-only buffer-read-only
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :faces (neomacs-company-web-test-face-runs))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_WEB_MELPA_PIN, "company-web.el")
        .expect("prepare exact shallow Company Web source below ./tmp")
        .with_melpa_dependency(COMPANY_MELPA_PIN)
        .expect("prepare exact shallow Company dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow Dash dependency below ./tmp")
        .with_melpa_dependency(WEB_COMPLETION_DATA_MELPA_PIN)
        .expect("prepare exact shallow web-completion-data dependency below ./tmp")
        .with_melpa_dependency(EMMET_MODE_MELPA_PIN)
        .expect("prepare exact shallow Emmet Mode dependency below ./tmp")
        .with_melpa_dependency(JADE_MODE_MELPA_PIN)
        .expect("prepare exact shallow Jade Mode dependency below ./tmp")
        .with_melpa_dependency(PUG_MODE_MELPA_PIN)
        .expect("prepare exact shallow Pug Mode dependency below ./tmp")
        .with_melpa_dependency(SLIM_MODE_MELPA_PIN)
        .expect("prepare exact shallow Slim Mode dependency below ./tmp")
        .with_melpa_dependency(WEB_MODE_MELPA_PIN)
        .expect("prepare exact shallow Web Mode dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn company_web_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "company_web_package_batch",
        "company_web_parity",
        &workflows::workflow_batch_cases(),
    );
}
