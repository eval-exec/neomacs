;;; line-number-gui-capacity.el --- Native matrix-capacity oracle -*- lexical-binding: t; -*-

;; Replay the peer's ordinary GUI buffer/window-start probe.  GNU 31.1
;; dispnew.c:required_matrix_height and xdisp.c:maybe_produce_line_number
;; reserve glyph-matrix rows beyond the visible text rows.  No expected
;; width is hardcoded here: the Rust test compares both live editors.
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)

(defvar gui-lnum-capacity--setup-seen nil)
(defvar gui-lnum-capacity--started nil)
(defvar gui-lnum-capacity--pending nil)
(defvar gui-lnum-capacity--rows nil)

(defun gui-lnum-capacity--write (path object)
  (with-temp-file path
    (insert (json-encode object))))

(defun gui-lnum-capacity--state ()
  `((window_setup_seen . ,gui-lnum-capacity--setup-seen)
    (graphic . ,(and (display-graphic-p) t))
    (window_system . ,(format "%S" window-system))
    (editor_role . ,(getenv "NEOMACS_GUI_LNUM_EDITOR"))
    (current_buffer . ,(buffer-name))
    (window_buffer . ,(buffer-name (window-buffer (selected-window))))
    (selected_window_on_selected_frame . ,(eq (window-frame) (selected-frame)))
    (ordinary_windows . ,(length (window-list)))
    (frame_width . ,(frame-width))
    (frame_height . ,(frame-height))
    (frame_pixel_width . ,(frame-pixel-width))
    (frame_pixel_height . ,(frame-pixel-height))
    (frame_char_width . ,(frame-char-width))
    (frame_char_height . ,(frame-char-height))
    (window_body_height . ,(window-body-height))
    (window_body_pixels . ,(window-body-height nil t))
    (window_total_height . ,(window-total-height))
    (window_total_pixels . ,(window-pixel-height))
    (point . ,(point))
    (window_start . ,(window-start))
    (point_line . ,(line-number-at-pos (point)))
    (window_start_line . ,(line-number-at-pos (window-start)))
    (point_min . ,(point-min))
    (point_max . ,(point-max))
    (buffer_size . ,(buffer-size))
    (line_numbers . ,(format "%S" display-line-numbers))
    (minimum_width . ,(format "%S" display-line-numbers-width))
    (widen . ,(format "%S" display-line-numbers-widen))
    (offset . ,display-line-numbers-offset)
    (current_absolute . ,(and display-line-numbers-current-absolute t))))

(defun gui-lnum-capacity--metadata ()
  ;; Collect metadata only AFTER every semantic observation has been saved.
  ;; font-info on a string/spec can open a new font, changing the matrix's
  ;; smallest-font metric.  Only inspect an already rendered font object.
  (with-current-buffer (window-buffer (selected-window))
    (append
     `((sampled_after_all_rows . t)
       (geometry_argument . ,(getenv "NEOMACS_GUI_LNUM_GEOMETRY"))
       (frame_pixel_width . ,(frame-pixel-width))
       (frame_pixel_height . ,(frame-pixel-height))
       (frame_char_width . ,(frame-char-width))
       (frame_char_height . ,(frame-char-height))
       (frame_width . ,(frame-width))
       (frame_height . ,(frame-height))
       (window_body_height . ,(window-body-height))
       (window_body_pixels . ,(window-body-height nil t))
       (window_total_height . ,(window-total-height))
       (window_total_pixels . ,(window-pixel-height))
       (frame_font_parameter_type . ,(format "%S" (type-of (frame-parameter nil 'font))))
       (frame_font_parameter . ,(format "%S" (frame-parameter nil 'font)))
       (final_state . ,(gui-lnum-capacity--state)))
     (condition-case font-problem
         (let* ((object (font-at (window-start)))
                (is-object (fontp object 'font-object))
                (info (and is-object (font-info object))))
           `((rendered_font_type . ,(format "%S" (type-of object)))
             (rendered_font_object . ,(and is-object t))
             (font_info . ,(format "%S" info))
             (font_info_height . ,(and (vectorp info) (> (length info) 9) (aref info 3)))
             (font_info_ascent . ,(and (vectorp info) (> (length info) 9) (aref info 8)))
             (font_info_descent . ,(and (vectorp info) (> (length info) 9) (aref info 9)))))
       (error `((font_metadata_error . ,(format "%S" font-problem))))))
  ))

(defun gui-lnum-capacity--finish (&optional problem)
  (gui-lnum-capacity--write
   (getenv "NEOMACS_GUI_LNUM_RESULT")
   `((passed . ,(if problem :json-false t))
     (geometry . ,(getenv "NEOMACS_GUI_LNUM_GEOMETRY"))
     (error . ,(and problem (format "%S" problem)))
     (rows . ,(vconcat (nreverse gui-lnum-capacity--rows)))))
  (gui-lnum-capacity--write
   (getenv "NEOMACS_GUI_LNUM_METADATA")
   (condition-case metadata-problem
       (gui-lnum-capacity--metadata)
     (error `((sampled_after_all_rows . t)
              (metadata_error . ,(format "%S" metadata-problem))))))
  (kill-emacs (if problem 1 0)))

(defun gui-lnum-capacity--sample (count start)
  (with-current-buffer (window-buffer (selected-window))
    (condition-case problem
        (let* ((before (gui-lnum-capacity--state))
               (width (prin1-to-string (line-number-display-width)))
               (columns (prin1-to-string (line-number-display-width 'columns)))
               (after (gui-lnum-capacity--state)))
          (push `((case . ,(format "lines-%d-start-%d" count start))
                  (line_count . ,count) (requested_start_line . ,start)
                  (width_raw . ,width) (columns_raw . ,columns)
                  (before . ,before) (after . ,after))
                gui-lnum-capacity--rows)
          (run-at-time 0.05 nil #'gui-lnum-capacity--step))
      (error (gui-lnum-capacity--finish problem)))
  ))

(defun gui-lnum-capacity--step ()
  (condition-case problem
      (if (null gui-lnum-capacity--pending)
          (gui-lnum-capacity--finish)
        (let* ((case (pop gui-lnum-capacity--pending))
               (count (car case)) (start (cadr case))
               (buffer (get-buffer-create (format "*gui-lnum-capacity-%d*" count))))
          (switch-to-buffer buffer)
          (unless (> (buffer-size) 0)
            (dotimes (i count) (insert (format "line %d\n" (1+ i)))))
          (setq-local display-line-numbers t
                      display-line-numbers-width nil
                      display-line-numbers-widen nil
                      display-line-numbers-offset 0
                      display-line-numbers-current-absolute t)
          (goto-char (point-min))
          (forward-line (1- start))
          (set-window-start nil (point))
          (redisplay t)
          ;; Give the native event loop a presented frame before sampling.
          (run-at-time 0.1 nil #'gui-lnum-capacity--sample count start)))
    (error (gui-lnum-capacity--finish problem))))

(defun gui-lnum-capacity--begin ()
  (unless gui-lnum-capacity--started
    (setq gui-lnum-capacity--started t)
    (condition-case problem
        (progn
          (let* ((editor (getenv "NEOMACS_GUI_LNUM_EDITOR"))
                 (expected-window-system
                  (cond ((equal editor "gnu") 'x)
                        ((equal editor "neo") 'neo)
                        (t (error "Unknown GUI capacity editor role: %S" editor)))))
            (unless (and gui-lnum-capacity--setup-seen (display-graphic-p)
                         (eq window-system expected-window-system))
              (error "Expected actual native window-setup for editor %S" editor)))
          (delete-other-windows)
          (setq gui-lnum-capacity--pending
                (if (equal (getenv "NEOMACS_GUI_LNUM_GEOMETRY") "80x40")
                    '((1100 58) (1100 59) (1100 60) (1100 61) (1100 62)
                      (1100 63) (1100 64) (1100 65) (1100 66) (1100 67)
                      (1100 68) (9 1))
                  '((1100 1) (9 1))))
          (gui-lnum-capacity--step))
      (error (gui-lnum-capacity--finish problem)))))

(add-hook 'window-setup-hook
          (lambda ()
            (setq gui-lnum-capacity--setup-seen t)
            (run-at-time 0.5 nil #'gui-lnum-capacity--begin)))
