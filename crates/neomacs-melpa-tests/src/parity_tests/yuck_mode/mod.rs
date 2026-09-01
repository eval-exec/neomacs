use crate::{CachedMelpaOracle, YUCK_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YUCK_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'yuck-mode)

(defun neomacs-melpa-yuck-mode--face-runs ()
  (font-lock-ensure)
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push
           (list
            (buffer-substring-no-properties position next)
            face position next)
           runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-melpa-yuck-mode--line-indents ()
  (save-excursion
    (goto-char (point-min))
    (let (indents)
      (while (not (eobp))
        (push
         (list (line-number-at-pos)
               (current-indentation)
               (buffer-substring-no-properties
                (line-beginning-position) (line-end-position)))
         indents)
        (forward-line 1))
      (nreverse indents))))

(defun neomacs-melpa-yuck-mode--syntax-state-at (needle &optional offset)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (goto-char (+ (match-beginning 0) (or offset 0)))
    (let ((state (syntax-ppss)))
      (list
       needle
       (point)
       (nth 0 state)
       (nth 3 state)
       (nth 4 state)
       (nth 8 state)))))

(defun neomacs-melpa-yuck-mode--face-segments (needle)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let ((start (match-beginning 0))
          (end (match-end 0))
          segments)
      (let ((position start))
        (while (< position end)
          (let ((next
                 (min
                  end
                  (next-single-property-change position 'face nil end))))
            (push
             (list
              (buffer-substring-no-properties position next)
              (get-text-property position 'face)
              (- position start)
              (- next start))
             segments)
            (setq position next))))
      (list needle start end (nreverse segments)))))
"##;

fn yuck_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YUCK_MODE_MELPA_PIN, "yuck-mode.el")
        .expect("prepare pinned yuck-mode source below ./tmp")
        .with_prelude(YUCK_MODE_TEST_PRELUDE)
}

#[test]
fn yuck_mode_package_batch() {
    assert_oracle_batch_cases(
        yuck_mode_oracle(),
        "yuck_mode_package_batch",
        "yuck_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
