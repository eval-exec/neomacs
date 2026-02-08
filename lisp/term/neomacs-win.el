;;; neomacs-win.el --- parse relevant switches and set up for Neomacs  -*- lexical-binding: t -*-

;; Copyright (C) 2024-2026 Free Software Foundation, Inc.

;; Author: FSF
;; Keywords: terminals

;; This file is part of GNU Emacs.

;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;; Neomacs is a GPU-accelerated display backend using GTK4 and Rust.

;;; Code:

(eval-when-compile (require 'cl-lib))
(unless (featurep 'neomacs)
  (error "%s: Loading neomacs-win.el but not compiled with NEOMACS"
         invocation-name))

;; Documentation-purposes only: actually loaded in loadup.el.
(require 'term/common-win)
(require 'frame)
(require 'mouse)
(require 'scroll-bar)
(require 'faces)
(require 'menu-bar)
(require 'fontset)

(defvar x-invocation-args)
(defvar x-command-line-resources)

(defun neomacs-suspend-error ()
  "Don't allow suspending if any of the frames are Neomacs frames."
  (if (memq 'neomacs (mapcar 'window-system (frame-list)))
      (error "Cannot suspend Emacs while a Neomacs GUI frame exists")))

(defvar neomacs-initialized nil
  "Non-nil if Neomacs windowing has been initialized.")

(declare-function x-handle-args "common-win" (args))
(declare-function x-open-connection "neomacsfns.c"
                  (display &optional xrm-string must-succeed))

;; Define x-display-name if not set
(defvar x-display-name nil
  "The display name specifying the display to connect to.")

;; Do the actual window system setup here.
(cl-defmethod window-system-initialization (&context (window-system neomacs)
                                            &optional display)
  "Initialize the Neomacs window system.
WINDOW-SYSTEM is, aptly, `neomacs'.
DISPLAY is the name of the display Emacs should connect to."
  (cl-assert (not neomacs-initialized))

  ;; Handle command line args
  (setq command-line-args (x-handle-args command-line-args))

  ;; Make sure we have a valid resource name.
  (when (boundp 'x-resource-name)
    (unless (stringp x-resource-name)
      (let (i)
	(setq x-resource-name (copy-sequence invocation-name))
	;; Change any . or * characters in x-resource-name to hyphens.
	(while (setq i (string-match "[.*]" x-resource-name))
	  (aset x-resource-name i ?-)))))

  ;; Setup the default fontset.
  (create-default-fontset)
  ;; Create the standard fontset.
  (condition-case err
      (create-fontset-from-fontset-spec standard-fontset-spec t)
    (error (display-warning
            'initialization
            (format "Creation of the standard fontset failed: %s" err)
            :error)))

  ;; Open the display connection
  (x-open-connection (or display
                         x-display-name)
		     x-command-line-resources
		     ;; Exit Emacs with fatal error if this fails.
		     t)

  ;; Create the default faces - use black on white (standard Emacs default)
  (let ((color-map '((foreground-color . "black")
                     (background-color . "white")
                     (cursor-color . "black"))))
    (dolist (param color-map)
      (add-to-list 'default-frame-alist param)))

  (add-hook 'suspend-hook #'neomacs-suspend-error)

  ;; Cursor blinking is handled by the render thread.
  ;; Sync blink-cursor-mode state to the render thread and suppress the
  ;; Emacs-side blink timer (which would fight with the render-thread blink).
  (neomacs--setup-cursor-blink)

  ;; Set up animations (smooth cursor, crossfade, scroll slide)
  (neomacs--setup-animations)

  ;; Clipboard integration via Rust arboard crate
  (setq interprogram-cut-function #'neomacs--clipboard-cut)
  (setq interprogram-paste-function #'neomacs--clipboard-paste)

  ;; Enable pixel-precise scrolling for smooth touchpad support
  (when (fboundp 'pixel-scroll-precision-mode)
    (pixel-scroll-precision-mode 1))

  (setq neomacs-initialized t))

;; Handle args function (required by common-win)
(cl-defmethod handle-args-function (args &context (window-system neomacs))
  (x-handle-args args))

;; Frame creation for Neomacs
;; Use x-create-frame-with-faces to properly initialize faces with colors
(cl-defmethod frame-creation-function (params &context (window-system neomacs))
  (x-create-frame-with-faces params))

;; Cursor blink integration: delegate to render thread
(declare-function neomacs-set-cursor-blink "neomacsterm.c" (enabled &optional interval))

;; Clipboard integration
(declare-function neomacs-clipboard-set "neomacsfns.c" (text))
(declare-function neomacs-clipboard-get "neomacsfns.c" ())

;; Primary selection integration
(declare-function neomacs-primary-selection-set "neomacsfns.c" (text))
(declare-function neomacs-primary-selection-get "neomacsfns.c" ())

;; Animation configuration
(declare-function neomacs-set-cursor-animation "neomacsterm.c" (enabled &optional speed))
(declare-function neomacs-set-animation-config "neomacsterm.c"
                  (cursor-enabled cursor-speed cursor-style cursor-duration
                   crossfade-enabled crossfade-duration
                   scroll-enabled scroll-duration
                   &optional scroll-effect scroll-easing trail-size
                   crossfade-effect crossfade-easing))

(defun neomacs--sync-cursor-blink ()
  "Sync `blink-cursor-mode' state to the render thread."
  (when (fboundp 'neomacs-set-cursor-blink)
    (neomacs-set-cursor-blink
     blink-cursor-mode
     (if (boundp 'blink-cursor-interval) blink-cursor-interval 0.5))))

(defun neomacs--setup-cursor-blink ()
  "Set up render-thread cursor blinking.
Syncs current blink state and advises `blink-cursor-mode' for future changes.
Also suppresses the Emacs-side blink timer since the render thread handles it."
  ;; Sync initial state
  (neomacs--sync-cursor-blink)
  ;; Re-sync whenever blink-cursor-mode toggles
  (advice-add 'blink-cursor-mode :after
              (lambda (&rest _) (neomacs--sync-cursor-blink))
              '((name . neomacs-sync-blink)))
  ;; Suppress Emacs-side blink timer (render thread handles visual blinking)
  (advice-add 'blink-cursor-timer-function :override
              (lambda () nil)
              '((name . neomacs-suppress-blink-timer))))

;; Animation setup
(defun neomacs--setup-animations ()
  "Set up render-thread animations (smooth cursor, crossfade, scroll slide).
Cursor animation styles:
  `exponential'      - smooth deceleration, no fixed duration (uses speed param)
  `spring'           - critically-damped spring, Neovide-like feel (default)
  `ease-out-quad'    - gentle deceleration curve
  `ease-out-cubic'   - stronger deceleration curve
  `ease-out-expo'    - sharp deceleration curve
  `ease-in-out-cubic' - smooth S-curve
  `linear'           - constant speed

Scroll effects (scroll-effect parameter, symbol or integer):
  `slide'                - content slides in scroll direction (default)
  `crossfade'            - alpha blend between old and new
  `scale-zoom'           - destination zooms from 95%% to 100%%
  `fade-edges'           - lines fade at viewport edges
  `cascade'              - lines drop in with stagger delay
  `parallax'             - layers scroll at different speeds
  `tilt'                 - subtle 3D perspective tilt
  `page-curl'            - page turning effect
  `card-flip'            - card flips around X-axis
  `cylinder-roll'        - content wraps around cylinder
  `wobbly'               - jelly-like deformation
  `wave'                 - sine-wave distortion
  `per-line-spring'      - each line springs independently
  `liquid'               - noise-based fluid distortion
  `motion-blur'          - vertical blur during scroll
  `chromatic-aberration' - RGB channel separation
  `ghost-trails'         - semi-transparent afterimages
  `color-temperature'    - warm/cool tint by direction
  `crt-scanlines'        - retro scanline overlay
  `depth-of-field'       - center sharp, edges dim
  `typewriter-reveal'    - lines appear left-to-right

Scroll easing (scroll-easing parameter, symbol or integer):
  `ease-out-quad'        - standard deceleration (default)
  `ease-out-cubic'       - stronger deceleration
  `spring'               - critically damped spring with overshoot
  `linear'               - constant speed
  `ease-in-out-cubic'    - smooth S-curve

Crossfade effect (crossfade-effect parameter, symbol or integer):
  Accepts the same effect symbols as scroll-effect.
  Default is `crossfade' (simple alpha blend).

Crossfade easing (crossfade-easing parameter, symbol or integer):
  Accepts the same easing symbols as scroll-easing.
  Default is `ease-out-quad'."
  (when (fboundp 'neomacs-set-animation-config)
    (neomacs-set-animation-config t 15.0 'spring 150 t 200 t 150 'slide 'ease-out-quad 0.7 'crossfade 'ease-out-quad)))

;; Clipboard integration
(defvar neomacs--last-clipboard-text nil
  "Last text sent to the clipboard, used to detect external changes.")

(defun neomacs--clipboard-cut (text)
  "Send TEXT to the system clipboard.
Used as `interprogram-cut-function'."
  (when (fboundp 'neomacs-clipboard-set)
    (setq neomacs--last-clipboard-text text)
    (neomacs-clipboard-set text)))

(defun neomacs--clipboard-paste ()
  "Return text from the system clipboard if it has changed.
Used as `interprogram-paste-function'."
  (when (fboundp 'neomacs-clipboard-get)
    (let ((text (neomacs-clipboard-get)))
      (when (and text (not (string= text neomacs--last-clipboard-text)))
        (setq neomacs--last-clipboard-text text)
        text))))

;; Selection protocol (CLIPBOARD + PRIMARY)
(cl-defmethod gui-backend-set-selection (selection value
                                         &context (window-system neomacs))
  "Set SELECTION to VALUE on the Neomacs display.
SELECTION is a symbol like `CLIPBOARD' or `PRIMARY'."
  (when value
    (let ((text (if (stringp value) value
                  (substring-no-properties (symbol-name value)))))
      (cond
       ((eq selection 'CLIPBOARD)
        (when (fboundp 'neomacs-clipboard-set)
          (setq neomacs--last-clipboard-text text)
          (neomacs-clipboard-set text)))
       ((eq selection 'PRIMARY)
        (when (fboundp 'neomacs-primary-selection-set)
          (neomacs-primary-selection-set text)))))))

(cl-defmethod gui-backend-get-selection (selection-symbol _target-type
                                          &context (window-system neomacs)
                                          &optional _time-stamp _terminal)
  "Get the value of SELECTION-SYMBOL from the Neomacs display."
  (cond
   ((eq selection-symbol 'CLIPBOARD)
    (when (fboundp 'neomacs-clipboard-get)
      (neomacs-clipboard-get)))
   ((eq selection-symbol 'PRIMARY)
    (when (fboundp 'neomacs-primary-selection-get)
      (neomacs-primary-selection-get)))))

(cl-defmethod gui-backend-selection-exists-p (selection
                                              &context (window-system neomacs))
  "Return non-nil if SELECTION has content on the Neomacs display."
  (cond
   ((eq selection 'CLIPBOARD)
    (when (fboundp 'neomacs-clipboard-get)
      (let ((text (neomacs-clipboard-get)))
        (and text (not (string-empty-p text))))))
   ((eq selection 'PRIMARY)
    (when (fboundp 'neomacs-primary-selection-get)
      (let ((text (neomacs-primary-selection-get)))
        (and text (not (string-empty-p text))))))))

;; Drag-and-drop file handling
(require 'dnd)

(defun neomacs-drag-n-drop (event)
  "Handle a drag-n-drop EVENT by opening dropped files.
Files are opened via the standard `dnd-protocol-alist' handlers."
  (interactive "e")
  (let* ((window (posn-window (event-start event)))
         (urls (car (cdr (cdr event)))))
    (when (windowp window)
      (select-window window))
    (raise-frame)
    (when (listp urls)
      (dnd-handle-multiple-urls window urls 'private))))

(define-key special-event-map [drag-n-drop] #'neomacs-drag-n-drop)

;; Frame opacity
(defun neomacs-set-frame-opacity (opacity &optional frame)
  "Set FRAME's background opacity to OPACITY (0.0 fully transparent, 1.0 opaque).
OPACITY can be a float (0.0-1.0) or an integer (0-100).
If FRAME is nil, use the selected frame.
This sets the `alpha-background' frame parameter, which makes the
background transparent while keeping text fully opaque."
  (interactive "nOpacity (0.0-1.0 or 0-100): ")
  (let ((f (or frame (selected-frame))))
    (when (and (integerp opacity) (> opacity 1))
      (setq opacity (/ (float opacity) 100.0)))
    (set-frame-parameter f 'alpha-background opacity)))

;; Menu bar keyboard access (F10)
(defun neomacs-menu-bar-open (&optional _frame)
  "Open the menu bar via the popup menu system.
_FRAME is ignored; the menu opens on the selected frame."
  (interactive "i")
  (popup-menu (mouse-menu-bar-map) last-nonmenu-event))

;; Window snapping
(defun neomacs--workarea ()
  "Return the workarea (x y width height) of the current monitor."
  (let* ((attrs (car (display-monitor-attributes-list)))
         (wa (alist-get 'workarea attrs)))
    (or wa (list 0 0 (display-pixel-width) (display-pixel-height)))))

(defun neomacs-snap-left (&optional frame)
  "Snap FRAME to the left half of the screen."
  (interactive)
  (let* ((f (or frame (selected-frame)))
         (wa (neomacs--workarea))
         (x (nth 0 wa)) (y (nth 1 wa))
         (w (nth 2 wa)) (h (nth 3 wa)))
    (set-frame-parameter f 'fullscreen nil)
    (set-frame-position f x y)
    (set-frame-size f (/ w 2) h t)))

(defun neomacs-snap-right (&optional frame)
  "Snap FRAME to the right half of the screen."
  (interactive)
  (let* ((f (or frame (selected-frame)))
         (wa (neomacs--workarea))
         (x (nth 0 wa)) (y (nth 1 wa))
         (w (nth 2 wa)) (h (nth 3 wa)))
    (set-frame-parameter f 'fullscreen nil)
    (set-frame-position f (+ x (/ w 2)) y)
    (set-frame-size f (/ w 2) h t)))

(defun neomacs-maximize (&optional frame)
  "Maximize FRAME."
  (interactive)
  (set-frame-parameter (or frame (selected-frame)) 'fullscreen 'maximized))

(defun neomacs-restore (&optional frame)
  "Restore FRAME from maximized/fullscreen state."
  (interactive)
  (set-frame-parameter (or frame (selected-frame)) 'fullscreen nil))

;; Window snap key bindings (Super+arrow)
(global-set-key (kbd "s-<left>")  #'neomacs-snap-left)
(global-set-key (kbd "s-<right>") #'neomacs-snap-right)
(global-set-key (kbd "s-<up>")    #'neomacs-maximize)
(global-set-key (kbd "s-<down>")  #'neomacs-restore)

;; Font size adjustment (Super +/-/0)
(global-set-key (kbd "s-=") #'global-text-scale-adjust)
(global-set-key (kbd "s-+") #'global-text-scale-adjust)
(global-set-key (kbd "s--")
  (lambda () (interactive) (global-text-scale-adjust -1)))
(global-set-key (kbd "s-0")
  (lambda () (interactive) (global-text-scale-adjust 0)))

;;; Scroll indicators

(declare-function neomacs-set-scroll-indicators "neomacsterm.c" (enabled))

(define-minor-mode neomacs-scroll-indicator-mode
  "Toggle scroll position indicators and active window focus ring."
  :global t
  :group 'frames
  :init-value t
  (neomacs-set-scroll-indicators neomacs-scroll-indicator-mode))

;;; Desktop notifications

(defun neomacs-notify (title body &optional urgency)
  "Show a desktop notification with TITLE and BODY.
URGENCY is one of `low', `normal' (default), or `critical'."
  (interactive "sTitle: \nsBody: ")
  (require 'notifications)
  (notifications-notify
   :title title
   :body body
   :app-name "Neomacs"
   :urgency (or urgency 'normal)))

;;; Custom title bar

(declare-function neomacs-set-titlebar-height "neomacsterm.c" (height))

(define-minor-mode neomacs-custom-titlebar-mode
  "Toggle custom title bar for borderless windows.
When enabled, a 30-pixel title bar with close/maximize/minimize buttons
is drawn by the render thread.  When disabled the title bar is hidden."
  :global t
  :group 'frames
  :init-value nil
  (neomacs-set-titlebar-height (if neomacs-custom-titlebar-mode 30 0)))

;;; Rounded corners

(declare-function neomacs-set-corner-radius "neomacsterm.c" (radius))

(defcustom neomacs-corner-radius 8
  "Corner radius in pixels for borderless window rounding.
Only takes effect when window decorations are disabled."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-corner-radius)
           (neomacs-set-corner-radius val))))

;; Provide the feature
(provide 'neomacs-win)
(provide 'term/neomacs-win)

;;; neomacs-win.el ends here
