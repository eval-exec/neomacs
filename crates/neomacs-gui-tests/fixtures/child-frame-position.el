;;; child-frame-position.el --- Frame coordinate regression -*- lexical-binding: t -*-

;; Minimal lsp-ui-doc pattern: create a child, then move it with (+ N).
(defun child-position-setup ()
  (switch-to-buffer (get-buffer-create "*child-position-parent*"))
  (insert "fn main() {\n    println!(\"Results:\");\n}\n")
  (defvar child-position-parent (selected-frame))
  (defvar child-position-popup
    (make-frame `((parent-frame . ,child-position-parent)
                  (minibuffer . nil) (visibility . nil)
                  (left . 10) (top . 20) (width . 20) (height . 4)
                  (internal-border-width . 1)
                  (menu-bar-lines . 0) (tool-bar-lines . 0) (tab-bar-lines . 0))))
  (set-window-buffer (frame-root-window child-position-popup)
                     (get-buffer-create "*child-position-popup*"))
  (with-current-buffer "*child-position-popup*" (insert "fn main()"))
  (modify-frame-parameters child-position-popup '((left . (+ 248)) (top . (+ 545))))
  (make-frame-visible child-position-popup)

  (defun child-position-make (name left top)
    (let ((frame (make-frame `((parent-frame . ,child-position-parent)
                               (minibuffer . nil) (visibility . t)
                               (left . ,left) (top . ,top)
                               (width . 15) (height . 3)
                               (menu-bar-lines . 0) (tool-bar-lines . 0) (tab-bar-lines . 0)
                               (internal-border-width . 1)))))
      (set-window-buffer (frame-root-window frame) (get-buffer-create name))
      (with-current-buffer name (insert name))
      frame))

  (child-position-make "*child-position-created-absolute*" '(+ -12) '(+ 37))
  (child-position-make "*child-position-created-far*" '(- 10) -20)
  (child-position-make "*child-position-created-fraction*" 0.5 0.25)
  (defvar child-position-resized (child-position-make "*child-position-resized*" 0 0))
  ;; Resolve against the new size, regardless of alist ordering.
  (modify-frame-parameters child-position-resized
                           '((left . (- 10)) (top . -)
                             (width . (text-pixels . 160)) (height . (text-pixels . 80))))
  (defvar child-position-negative (child-position-make "*child-position-negative*" 0 0))
  (modify-frame-parameters child-position-negative '((left . (+ -12)) (top . (+ -17))))
  (run-at-time 1 nil
               (lambda ()
                 (neomacs--write-frame-snapshot
                  (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
                 (run-at-time 1 nil (lambda () (kill-emacs 0))))))

;; Let the initial native configure establish the parent dimensions first.
;; Far-edge/fraction requests are evaluated when applied; they do not promise
;; automatic re-anchoring when the compositor later resizes the parent.
(run-at-time 1 nil #'child-position-setup)
