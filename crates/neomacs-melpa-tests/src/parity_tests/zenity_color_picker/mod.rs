use crate::{CachedMelpaOracle, ZENITY_COLOR_PICKER_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZENITY_COLOR_PICKER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'zenity-color-picker)

(defun neomacs-zenity-cp-test--prepare-runner (root)
  (let* ((bin-directory (expand-file-name "bin" root))
         (runner (expand-file-name "zenity" bin-directory))
         (log (expand-file-name "zenity.argv" root)))
    (make-directory bin-directory t)
    (with-temp-file runner
      (insert
       "#!/bin/sh\n"
       "{\n"
       "  printf 'program=%s\\n' \"$0\"\n"
       "  printf 'argc=%s\\n' \"$#\"\n"
       "  index=0\n"
       "  for argument in \"$@\"; do\n"
       "    printf 'arg[%s]=%s\\n' \"$index\" \"$argument\"\n"
       "    index=$((index + 1))\n"
       "  done\n"
       "} > \"$ZENITY_CP_TEST_LOG\"\n"
       "printf '%s' \"$ZENITY_CP_TEST_RESPONSE\"\n"
       "exit \"${ZENITY_CP_TEST_STATUS:-0}\"\n"))
    (set-file-modes runner #o755)
    (list :bin-directory bin-directory :runner runner :log log)))

(defun neomacs-zenity-cp-test--read-file (path)
  (when (file-exists-p path)
    (with-temp-buffer
      (insert-file-contents-literally path)
      (buffer-string))))

(defun neomacs-zenity-cp-test--outcome (function)
  (condition-case error-data
      (list :value (funcall function))
    (error (list :signal (car error-data) :data (cdr error-data)))))

(defmacro neomacs-zenity-cp-test--with-runner
    (name response status &rest body)
  (declare (indent 3))
  `(let* ((root
           (file-name-as-directory
            (expand-file-name
             ,name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
          (_cleanup
           (when (file-directory-p root)
             (delete-directory root t)))
          (runner-state
           (neomacs-zenity-cp-test--prepare-runner root))
          (log (plist-get runner-state :log))
          (exec-path
           (cons (plist-get runner-state :bin-directory) exec-path))
          (zenity-cp-zenity-bin "zenity")
          (process-environment
           (append
            (list
             (concat "ZENITY_CP_TEST_LOG=" log)
             (concat "ZENITY_CP_TEST_RESPONSE=" ,response)
             (format "ZENITY_CP_TEST_STATUS=%s" ,status))
            process-environment)))
     (unwind-protect
         (progn ,@body)
       (when (file-directory-p root)
         (delete-directory root t)))))
"####;

fn zenity_color_picker_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZENITY_COLOR_PICKER_MELPA_PIN, "zenity-color-picker.el")
        .expect("prepare pinned zenity-color-picker source below ./tmp")
        .with_prelude(ZENITY_COLOR_PICKER_TEST_PRELUDE)
}

#[test]
fn zenity_color_picker_package_batch() {
    assert_oracle_batch_cases(
        zenity_color_picker_oracle(),
        "zenity_color_picker_package_batch",
        "zenity_color_picker_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
