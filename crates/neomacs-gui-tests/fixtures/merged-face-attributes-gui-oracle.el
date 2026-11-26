;;; merged-face-attributes-gui-oracle.el --- #853 merged-face GNU/Neomacs GUI oracle -*- lexical-binding: t; -*-
;;
;; State-gated (window-setup-hook -> redisplay -> write -> kill-emacs; zero
;; sleeps or timers) oracle comparing merged face attributes, font-at weight
;; and slant, cursor color parameters, and rendered foreground/background
;; colors across stacked text-property faces, :inherit chains, overlays
;; (including hl-line with and without :foreground, integer and cons
;; priorities), face-remapping-alist, inverse-video, and box-cursor paint.

(require 'cl-lib)
(require 'json)
(require 'hl-line)

(setq inhibit-startup-screen t
      initial-scratch-message nil
      native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(set-fringe-mode 0)

(defface p853-top '((t :foreground "#ff2222")) "Top stacked face.")
(defface p853-bottom '((t :foreground "#0000ff" :background "#112233" :weight bold)) "Bottom stacked face.")
(defface p853-parent '((t :foreground "#cc44cc" :weight bold :slant normal)) "Parent face.")
(defface p853-child '((t :inherit p853-parent :slant italic)) "Child inheriting face.")
(defface p853-fg-red '((t :foreground "#ff2222")) "Red fg face.")
(defface p853-remap '((t :foreground "#ff2222" :background "#141820" :weight normal)) "Remap target face.")
(defface p853-inherit-remap '((t :inherit p853-remap)) "Inherits remapped face.")
(defface p853-unspec-fg '((t :foreground unspecified :background "#334455")) "Unspecified fg.")
(defface p853-inv '((t :inverse-video t)) "Inverse video face.")
(defface p853-hl-bg '((t :background "#223344")) "hl-line bg-only face.")
(defface p853-hl-fg '((t :foreground "#00dd44" :background "#223344")) "hl-line with fg face.")

(defun p853-probe-pos (name pos row-idx win)
  (let* ((fap-one (save-excursion (goto-char pos) (face-at-point)))
         (fap-all (save-excursion (goto-char pos) (face-at-point nil t)))
         (raw-text (get-text-property pos 'face))
         (raw-char (get-char-property pos 'face))
         (font (font-at pos win))
         (xy (posn-x-y (posn-at-point pos win)))
         (next-xy (save-excursion
                    (goto-char pos)
                    (forward-line 1)
                    (posn-x-y (posn-at-point (point) win)))))
    (list (cons "name" name)
          (cons "row" row-idx)
          (cons "pos" pos)
          (cons "face_at_point" (prin1-to-string fap-one))
          (cons "faces_at_point" (prin1-to-string fap-all))
          (cons "text_prop_face" (prin1-to-string raw-text))
          (cons "char_prop_face" (prin1-to-string raw-char))
          (cons "font_weight" (if font (symbol-name (font-get font :weight)) "nil"))
          (cons "font_slant" (if font (symbol-name (font-get font :slant)) "nil"))
          (cons "x" (car xy))
          (cons "y" (cdr xy))
          (cons "next_y" (cdr next-xy)))))

(defun p853-run ()
  (condition-case err
      (let ((buf (get-buffer-create "*p853-merged-face-oracle*"))
            (entries nil)
            (row 0))
        (set-frame-font "DejaVu Sans Mono-16" nil t)
        (modify-frame-parameters nil '((background-color . "#101010")
                                       (foreground-color . "#eeeeee")
                                       (cursor-type . box)))
        (set-face-attribute 'default nil :foreground "#eeeeee" :background "#101010")
        ;; Doom themes set cursor color via `set-face-attribute' on `cursor'.
        (set-face-attribute 'cursor nil :background "#00ccff")
        (switch-to-buffer buf)
        (erase-buffer)
        (setq-local mode-line-format nil
                    header-line-format nil
                    line-spacing nil)
        (setq-local face-remapping-alist
                    '((p853-remap (:foreground "#0088ff" :weight bold) p853-remap)))

        (cl-flet ((record (name pos)
                    (push (list name pos row) entries)))
          ;; Row 0: stacked list (earlier wins)
          (record "stacked-list" (point))
          (insert (propertize "MMMMMMMM" 'face '(p853-top p853-bottom)) "\n")
          (setq row (1+ row))

          ;; Row 1: :inherit chain (own :slant italic + inherited :foreground & :weight)
          (record "inherit-chain" (point))
          (insert (propertize "MMMMMMMM" 'face 'p853-child) "\n")
          (setq row (1+ row))

          ;; Row 2: inline anonymous plist over named face
          (record "inline-over-named" (point))
          (insert (propertize "MMMMMMMM" 'face '((:foreground "#ff8800") p853-bottom)) "\n")
          (setq row (1+ row))

          ;; Row 3: hl-line bg-only overlay (priority -50) over red text property
          (let ((p (point)))
            (record "hl-line-bg-only" p)
            (insert (propertize "MMMMMMMM" 'face 'p853-fg-red) "\n")
            (let ((ov (make-overlay p (point))))
              (overlay-put ov 'face 'p853-hl-bg)
              (overlay-put ov 'priority -50)))
          (setq row (1+ row))

          ;; Row 4: hl-line overlay WITH :foreground over red text property (#853 suspect 1)
          (let ((p (point)))
            (record "hl-line-with-fg" p)
            (insert (propertize "MMMMMMMM" 'face 'p853-fg-red) "\n")
            (let ((ov (make-overlay p (point))))
              (overlay-put ov 'face 'p853-hl-fg)
              (overlay-put ov 'priority -50)))
          (setq row (1+ row))

          ;; Row 5: priority-ordered overlays (10 vs 20) over red text property
          (let ((p (point)))
            (record "priority-overlays" p)
            (insert (propertize "MMMMMMMM" 'face 'p853-fg-red) "\n")
            (let ((ov1 (make-overlay p (1- (point))))
                  (ov2 (make-overlay p (1- (point)))))
              (overlay-put ov1 'face '(:foreground "#11aaff" :background "#442222"))
              (overlay-put ov1 'priority 10)
              (overlay-put ov2 'face '(:foreground "#ffaa00"))
              (overlay-put ov2 'priority 20)))
          (setq row (1+ row))

          ;; Row 6: cons priority (10 . 5) beats integer priority 10
          (let ((p (point)))
            (record "cons-priority-overlay" p)
            (insert (propertize "MMMMMMMM" 'face '(:foreground "#ff0000")) "\n")
            (let ((ov1 (make-overlay p (1- (point))))
                  (ov2 (make-overlay p (1- (point)))))
              (overlay-put ov1 'face '(:foreground "#00ff00"))
              (overlay-put ov1 'priority 10)
              (overlay-put ov2 'face '(:foreground "#0088ff"))
              (overlay-put ov2 'priority '(10 . 5))))
          (setq row (1+ row))

          ;; Row 7: face-remapping-alist target (color + bold weight)
          (record "remap-named" (point))
          (insert (propertize "MMMMMMMM" 'face 'p853-remap) "\n")
          (setq row (1+ row))

          ;; Row 8: named face inheriting a remapped face
          (record "inherit-remapped" (point))
          (insert (propertize "MMMMMMMM" 'face 'p853-inherit-remap) "\n")
          (setq row (1+ row))

          ;; Row 9: overlay whose face inherits a remapped face
          (let ((p (point)))
            (record "overlay-inherit-remapped" p)
            (insert "MMMMMMMM\n")
            (let ((ov (make-overlay p (1- (point)))))
              (overlay-put ov 'face 'p853-inherit-remap)))
          (setq row (1+ row))

          ;; Row 10: remapped text face under hl-line overlay with :foreground
          (let ((p (point)))
            (record "remap-plus-hl-fg" p)
            (insert (propertize "MMMMMMMM" 'face 'p853-remap) "\n")
            (let ((ov (make-overlay p (point))))
              (overlay-put ov 'face 'p853-hl-fg)
              (overlay-put ov 'priority -50)))
          (setq row (1+ row))

          ;; Row 11: explicit :foreground unspecified does not clobber underlying remapped fg
          (record "explicit-unspec-fg" (point))
          (insert (propertize "MMMMMMMM" 'face '(p853-unspec-fg p853-remap)) "\n")
          (setq row (1+ row))

          ;; Row 12: anonymous plist :inherit list
          (record "anon-inherit-list" (point))
          (insert (propertize "MMMMMMMM" 'face '(:inherit (p853-unspec-fg p853-remap))) "\n")
          (setq row (1+ row))

          ;; Row 13: :inverse-video t merged over stacked faces swaps fg and bg
          (record "inverse-video-merged" (point))
          (insert (propertize "MMMMMMMM" 'face '(p853-inv p853-top p853-bottom)) "\n")
          (setq row (1+ row))

          ;; Row 14: live hl-line-mode row with box cursor at col 0 and neighbor at col 4
          (let ((p (point)))
            (record "cursor-on-hl-line" p)
            (record "neighbor-on-hl-line" (+ p 4))
            (insert (propertize "MMMMMMMM" 'face 'p853-fg-red) "\n")
            (set-face-attribute 'hl-line nil :background "#223344" :foreground 'unspecified)
            (hl-line-mode 1)
            (goto-char p)
            (hl-line-highlight)))

        (setq entries (nreverse entries))
        (redisplay t)
        (let* ((win (selected-window))
               (edges (window-inside-pixel-edges win))
               (cases (mapcar (lambda (entry)
                                (p853-probe-pos (nth 0 entry) (nth 1 entry) (nth 2 entry) win))
                              entries))
               (payload (list (cons "native_engine"
                                    (if (fboundp 'neomacs--write-frame-snapshot) t :json-false))
                              (cons "graphic" (if (display-graphic-p) t :json-false))
                              (cons "cursor_color_param" (frame-parameter nil 'cursor-color))
                              (cons "cursor_face_bg" (face-attribute 'cursor :background nil t))
                              (cons "window_edges" (vconcat edges))
                              (cons "cases" (vconcat cases)))))
          (with-temp-file (getenv "NEOMACS_MERGED_FACE_ORACLE_JSON")
            (insert (json-encode payload)))
          (when (and (getenv "NEOMACS_MERGED_FACE_SNAP_JSON")
                     (fboundp 'neomacs--write-frame-snapshot))
            (neomacs--write-frame-snapshot (getenv "NEOMACS_MERGED_FACE_SNAP_JSON") nil 'json))
          (when (and (getenv "NEOMACS_MERGED_FACE_GNU_PNG")
                     (fboundp 'x-export-frames))
            (let ((coding-system-for-write 'no-conversion))
              (with-temp-file (getenv "NEOMACS_MERGED_FACE_GNU_PNG")
                (set-buffer-multibyte nil)
                (insert (x-export-frames nil 'png)))))))
    (error
     (with-temp-file (getenv "NEOMACS_MERGED_FACE_ORACLE_JSON")
       (insert (format "{\"error\": %S}\n" err)))
     (kill-emacs 1)))
  (kill-emacs 0))

(add-hook 'window-setup-hook #'p853-run)
