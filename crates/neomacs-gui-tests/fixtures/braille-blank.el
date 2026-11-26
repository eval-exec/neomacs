;;; braille-blank.el --- Empty glyphs survive repeated repaint -*- lexical-binding: t; -*-

;; GNU 31.1 ftcrfont.c:94-116 keeps ink bounds separate from advance.
;; ftcrfont_draw (:604-641) accepts a valid glyph with no ink and still moves x.
(require 'json)
(setq inhibit-startup-screen t native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(set-fringe-mode 0)

(defvar braille-blank-rows nil)
(defvar braille-blank-neighbors nil)
(defvar braille-blank-edits 0)
(defvar braille-blank-timer nil)
(defvar braille-blank-start-time nil)

(defun braille-blank-start ()
  (switch-to-buffer (get-buffer-create "*braille-blank*"))
  (setq-local mode-line-format nil header-line-format nil cursor-type nil
              line-spacing nil truncate-lines t)
  (set-face-attribute 'default nil :font
                      (font-spec :family "DejaVu Sans" :size 18 :weight 'regular)
                      :foreground "black" :background "white")
  (erase-buffer)
  (dolist (character '(32 #x2800 #x2801))
    (let ((position (point)))
      (insert (make-string 8 character) "\n")
      (push (cons character position) braille-blank-rows)))
  (setq braille-blank-rows (nreverse braille-blank-rows)
        braille-blank-neighbors (point))
  ;; Pinned upright H keeps its ink inside its advance; an arbitrary left
  ;; neighbor could legitimately overhang into the blank cell.
  (insert (string ?H #x2800 ?B) "\n\nrepaint x")
  (goto-char (point-min))
  (redisplay t)
  ;; Only this ASCII suffix changes. Every measured row remains immutable.
  ;; 600 edits at a nominal 50 Hz keep the empty-glyph retry path exercised.
  (setq braille-blank-start-time (float-time)
        braille-blank-timer (run-at-time 0.02 0.02 #'braille-blank-edit)))

(defun braille-blank-edit ()
  (with-current-buffer "*braille-blank*"
    (save-excursion
      (goto-char (point-max))
      (delete-char -1)
      (insert (if (= (% braille-blank-edits 2) 0) "y" "x")))
    (setq braille-blank-edits (1+ braille-blank-edits))
    (redisplay t)
    (when (= braille-blank-edits 600)
      (cancel-timer braille-blank-timer)
      (setq braille-blank-timer nil)
      (run-at-time 0.3 nil #'braille-blank-observe))))

(defun braille-blank-xy (position window)
  (let ((xy (posn-x-y (posn-at-point position window))))
    ;; Dotted pairs are not JSON arrays. Retain every coordinate explicitly.
    (vector (car xy) (cdr xy))))

(defun braille-blank-observe ()
  (with-current-buffer "*braille-blank*"
    (redisplay t)
    (let* ((window (get-buffer-window (current-buffer)))
           (edges (window-inside-pixel-edges window))
           (rows
            (mapcar
             (lambda (entry)
               (let* ((character (car entry)) (position (cdr entry))
                      (font (font-at position window))
                      (info (font-info font))
                      (entity (font-get font :font-entity))
                      (positions (vconcat
                                  (mapcar (lambda (offset)
                                            (braille-blank-xy (+ position offset) window))
                                          '(0 1 2 3 4 5 6 7 8))))
                      (next (braille-blank-xy (+ position 9) window)))
                 `((character . ,character)
                   (family . ,(format "%s" (font-get font :family)))
                   (size . ,(font-get font :size))
                   (file . ,(aref info 12))
                   (font_index . ,(and (consp entity) (cdr entity)))
                   (has_character . ,(if (font-has-char-p font character) t :json-false))
                   (glyph_metrics . ,(font-get-glyphs font 0 1 (string character)))
                   (positions . ,positions)
                   (bounds . ,(vector (+ (nth 0 edges) (aref (aref positions 0) 0))
                                      (+ (nth 1 edges) (aref (aref positions 0) 1))
                                      (+ (nth 0 edges) (aref (aref positions 8) 0))
                                      (+ (nth 1 edges) (aref next 1)))))))
             braille-blank-rows))
           (neighbor-positions
            (vconcat (mapcar (lambda (offset)
                              (braille-blank-xy (+ braille-blank-neighbors offset) window))
                            '(0 1 2 3))))
           (neighbor-bottom
            (aref (braille-blank-xy (+ braille-blank-neighbors 4) window) 1))
           (neighbor-bounds
            (vconcat
             (mapcar (lambda (index)
                       (vector (+ (nth 0 edges) (aref (aref neighbor-positions index) 0))
                               (+ (nth 1 edges) (aref (aref neighbor-positions index) 1))
                               (+ (nth 0 edges) (aref (aref neighbor-positions (1+ index)) 0))
                               (+ (nth 1 edges) neighbor-bottom)))
                     '(0 1 2))))
           (state `((complete . t) (edits . ,braille-blank-edits)
                    (edit_interval . 0.02)
                    (edit_seconds . ,(- (float-time) braille-blank-start-time))
                    (rows . ,(vconcat rows))
                    (neighbor_characters . [72 10240 66])
                    (neighbor_positions . ,neighbor-positions)
                    (neighbor_bounds . ,neighbor-bounds))))
      (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
        (insert (json-encode state)))
      (princ (format "BRAILLE-BLANK %S\n" state) 'external-debugging-output)))
  ;; The host captures the actual native window after all repeated edits stop.
  (run-at-time 12 nil (lambda () (kill-emacs 0))))

(defun braille-blank-guard (function &rest arguments)
  (condition-case problem
      (apply function arguments)
    (error
     (princ (format "BRAILLE-BLANK-FAIL %S\n" problem) 'external-debugging-output)
     (kill-emacs 1))))

(dolist (function '(braille-blank-start braille-blank-edit braille-blank-observe))
  (advice-add function :around #'braille-blank-guard))
(run-at-time 0.5 nil #'braille-blank-start)
(run-at-time 50 nil (lambda () (kill-emacs 2)))
