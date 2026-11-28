;;; wayland-drag-selection.el --- Count-based Wayland drag coalescing regression fixture -*- lexical-binding: t; -*-

(require 'json)

(setq inhibit-startup-screen t
      select-active-regions t
      mouse-fine-grained-tracking nil
      native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(transient-mark-mode 1)
(global-hl-line-mode 1)
(show-paren-mode 1)

(defvar drag-test-ready (getenv "NEOMACS_DRAG_TEST_READY"))
(defvar drag-test-pressed (getenv "NEOMACS_DRAG_TEST_PRESSED"))
(defvar drag-test-pointer (getenv "NEOMACS_DRAG_TEST_POINTER"))
(defvar drag-test-out (getenv "NEOMACS_DRAG_TEST_OUT"))
(defvar drag-test-move-count 0)
(defvar drag-test-redisplay-count 0)
(defvar drag-test-selection-count 0)
(defvar drag-test-start-x 120)
(defvar drag-test-start-y 40)
(defvar drag-test-end-x 620)
(defvar drag-test-end-y 540)
(defvar drag-test-active nil)

(defun drag-test-cell-changes ()
  "How many times the glyph cell under the drag path changes.
The test sweeps 300 steps from the start to the end point.  GNU reports a
`mouse-movement' only when the pointer leaves the glyph it was last over
(`x_note_mouse_movement', xterm.c), so this is an upper bound on its moves."
  (let ((last nil) (changes 0))
    (dotimes (i 301)
      (let* ((x (+ drag-test-start-x (/ (* (- drag-test-end-x drag-test-start-x) i) 300)))
             (y (+ drag-test-start-y (/ (* (- drag-test-end-y drag-test-start-y) i) 300)))
             (posn (posn-at-x-y x y (selected-frame)))
             (cell (and posn (cons (posn-window posn) (posn-col-row posn)))))
        (unless (equal cell last)
          (setq changes (1+ changes) last cell))))
    changes))

(defun drag-test-write-result ()
  (when drag-test-out
    (let* ((final-pt (point))
           (final-mk (and (mark t) (mark t)))
           (region-active (if (use-region-p) t :json-false))
           (start-posn (posn-at-x-y drag-test-start-x drag-test-start-y (selected-frame)))
           (end-posn (posn-at-x-y drag-test-end-x drag-test-end-y (selected-frame)))
           (expected-start (and start-posn (posn-point start-posn)))
           (expected-end (and end-posn (posn-point end-posn)))
           (tmp (concat drag-test-out ".tmp")))
      (with-temp-file tmp
        (insert (json-encode
                 `((move_count . ,drag-test-move-count)
                   (cell_changes . ,(drag-test-cell-changes))
                   (redisplay_count . ,drag-test-redisplay-count)
                   (selection_count . ,drag-test-selection-count)
                   (final_pt . ,final-pt)
                   (final_mk . ,final-mk)
                   (expected_start . ,expected-start)
                   (expected_end . ,expected-end)
                   (region_active . ,region-active)))))
      (rename-file tmp drag-test-out t))))

(defun drag-test-await-pointer ()
  "Report once this frame has received the pointer at the drag start.
The compositor delivers a button to the surface under the pointer, so a
press sent before the frame has the pointer reaches no client.  This
waits on the editor's own record of the pointer, not on a delay."
  (let ((pos (mouse-pixel-position)))
    (if (and (eq (car pos) (selected-frame))
             (eql (cadr pos) drag-test-start-x)
             (eql (cddr pos) drag-test-start-y))
        (let ((tmp (concat drag-test-pointer ".tmp")))
          (with-temp-file tmp
            (insert (json-encode `((x . ,(cadr pos)) (y . ,(cddr pos))))))
          (rename-file tmp drag-test-pointer t))
      (run-at-time 0.02 nil #'drag-test-await-pointer))))

(defun drag-test-setup ()
  (let ((buf (get-buffer-create "*wayland-drag-test*")))
    (switch-to-buffer buf)
    (emacs-lisp-mode)
    (display-line-numbers-mode 1)
    (erase-buffer)
    (dotimes (i 250)
      (insert (format ";; Line %03d: Wayland drag selection coalescing regression fixture\n(defun drag-fixture-fn-%03d (a b)\n  (let ((sum (+ a b %d)))\n    (when (> sum 0) (message \"sum=%%d\" sum))))\n"
                      i i i)))
    (font-lock-ensure)
    (goto-char (point-min))
    (add-hook 'pre-redisplay-functions
              (lambda (_)
                (when drag-test-active
                  (setq drag-test-redisplay-count (1+ drag-test-redisplay-count))))
              nil t)
    ;; Small deterministic per-command work (simulating Doom/Evil hooks) so
    ;; queued pointer bursts accumulate if coalescing regresses.
    (add-hook 'post-command-hook
              (lambda ()
                (when drag-test-active
                  (let ((acc 0))
                    (dotimes (k 8000)
                      (setq acc (+ acc k)))
                    acc)))
              nil t)
    (advice-add 'gui-set-selection :before
                (lambda (&rest _)
                  (when drag-test-active
                    (setq drag-test-selection-count (1+ drag-test-selection-count)))))
    (advice-add 'mouse-drag-region :before
                (lambda (&rest _)
                  (setq drag-test-active t
                        drag-test-move-count 0
                        drag-test-redisplay-count 0
                        drag-test-selection-count 0)
                  (when drag-test-pressed
                    (let ((tmp (concat drag-test-pressed ".tmp")))
                      (with-temp-file tmp
                        (insert (json-encode `((pt . ,(point))))))
                      (rename-file tmp drag-test-pressed t)))))
    (advice-add 'mouse--drag-set-mark-and-point :before
                (lambda (&rest _)
                  (when drag-test-active
                    (setq drag-test-move-count (1+ drag-test-move-count)))))
    (advice-add 'mouse-set-region :after
                (lambda (&rest _)
                  (setq drag-test-active nil)
                  (drag-test-write-result)
                  (run-at-time 0.05 nil (lambda () (kill-emacs 0)))))
    (redisplay t)
    (when drag-test-ready
      (let ((tmp (concat drag-test-ready ".tmp")))
        (with-temp-file tmp
          (insert (json-encode `((pid . ,(emacs-pid))
                                 (width . ,(frame-pixel-width))
                                 (height . ,(frame-pixel-height))))))
        (rename-file tmp drag-test-ready t)))
    (when drag-test-pointer
      (drag-test-await-pointer))))

(run-at-time 0.2 nil #'drag-test-setup)
(run-at-time 30 nil (lambda () (drag-test-write-result) (kill-emacs 1)))
