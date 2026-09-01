use crate::{CachedMelpaOracle, ZEN_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZEN_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'imenu)
(require 'zen-mode)

(defun neomacs-melpa-zen-mode--face-runs ()
  (font-lock-ensure)
  (let ((position (point-min)) runs)
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

(defun neomacs-melpa-zen-mode--face-segments (needle)
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

(defun neomacs-melpa-zen-mode--syntax-state-at (needle &optional offset)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (goto-char (+ (match-beginning 0) (or offset 0)))
    (let ((state (syntax-ppss)))
      (list
       needle (point)
       :depth (nth 0 state)
       :string (nth 3 state)
       :comment (nth 4 state)
       :start (nth 8 state)
       :syntax-property
       (car-safe (get-text-property (point) 'syntax-table))))))

(defun neomacs-melpa-zen-mode--line-indents ()
  (save-excursion
    (goto-char (point-min))
    (let (indents)
      (while (not (eobp))
        (push
         (list
          (line-number-at-pos)
          (current-indentation)
          (buffer-substring-no-properties
           (line-beginning-position) (line-end-position)))
         indents)
        (forward-line 1))
      (nreverse indents))))

(defun neomacs-melpa-zen-mode--imenu-index ()
  (mapcar
   (lambda (category)
     (cons
      (car category)
      (mapcar
       (lambda (item)
         (let ((position (cdr item)))
           (save-excursion
             (goto-char position)
             (list
              (car item)
              (line-number-at-pos)
              (current-column)
              (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))))
       (cdr category))))
   (imenu-default-create-index-function)))
"####;

fn zen_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZEN_MODE_MELPA_PIN, "zen-mode.el")
        .expect("prepare pinned zen-mode source below ./tmp")
        .with_prelude(ZEN_MODE_TEST_PRELUDE)
}

#[test]
fn zen_mode_package_batch() {
    assert_oracle_batch_cases(
        zen_mode_oracle(),
        "zen_mode_package_batch",
        "zen_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
