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

;;; Borderless mode toggle

(defun neomacs-toggle-decorations (&optional frame)
  "Toggle between decorated and borderless window mode.
In borderless mode, enable the custom title bar and rounded corners.
In decorated mode, disable them."
  (interactive)
  (let* ((f (or frame (selected-frame)))
         (currently-undecorated (frame-parameter f 'undecorated))
         (go-borderless (not currently-undecorated)))
    (set-frame-parameter f 'undecorated go-borderless)
    (when (fboundp 'neomacs-set-titlebar-height)
      (neomacs-set-titlebar-height (if go-borderless 30 0)))
    (when (fboundp 'neomacs-set-corner-radius)
      (neomacs-set-corner-radius
       (if go-borderless
           (if (boundp 'neomacs-corner-radius) neomacs-corner-radius 8)
         0)))))

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

;;; Extra spacing (line-height and letter-spacing)

(declare-function neomacs-set-extra-spacing "neomacsterm.c"
  (line-spacing letter-spacing))

(defcustom neomacs-extra-line-spacing 0
  "Extra vertical spacing between text rows in pixels.
Applied at the render level on top of Emacs line-spacing."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-extra-spacing)
           (neomacs-set-extra-spacing
            val (if (boundp 'neomacs-extra-letter-spacing)
                    neomacs-extra-letter-spacing 0)))))

(defcustom neomacs-extra-letter-spacing 0
  "Extra horizontal spacing between characters in pixels.
Applied at the render level."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-extra-spacing)
           (neomacs-set-extra-spacing
            (if (boundp 'neomacs-extra-line-spacing)
                neomacs-extra-line-spacing 0)
            val))))

;;; Background gradient

(declare-function neomacs-set-background-gradient "neomacsterm.c"
  (top-color bottom-color))

(defcustom neomacs-background-gradient nil
  "Background gradient colors as (TOP-COLOR . BOTTOM-COLOR).
Set to nil to disable.  Colors are strings like \"#1a1a2e\"."
  :type '(choice (const :tag "Disabled" nil)
                 (cons :tag "Gradient"
                       (color :tag "Top color")
                       (color :tag "Bottom color")))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-background-gradient)
           (if val
               (neomacs-set-background-gradient (car val) (cdr val))
             (neomacs-set-background-gradient nil nil)))))

;;; Scroll bar appearance

(declare-function neomacs-set-scroll-bar-config "neomacsterm.c"
  (&optional width thumb-radius track-opacity hover-brightness))

(defcustom neomacs-scroll-bar-width 12
  "Scroll bar width in pixels."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-bar-config)
           (neomacs-set-scroll-bar-config
            val
            (if (boundp 'neomacs-scroll-bar-thumb-radius)
                neomacs-scroll-bar-thumb-radius 40)
            (if (boundp 'neomacs-scroll-bar-track-opacity)
                neomacs-scroll-bar-track-opacity 60)
            (if (boundp 'neomacs-scroll-bar-hover-brightness)
                neomacs-scroll-bar-hover-brightness 140)))))

(defcustom neomacs-scroll-bar-thumb-radius 40
  "Scroll bar thumb corner radius (0-100, as percentage)."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-bar-config)
           (neomacs-set-scroll-bar-config
            (if (boundp 'neomacs-scroll-bar-width)
                neomacs-scroll-bar-width 12)
            val
            (if (boundp 'neomacs-scroll-bar-track-opacity)
                neomacs-scroll-bar-track-opacity 60)
            (if (boundp 'neomacs-scroll-bar-hover-brightness)
                neomacs-scroll-bar-hover-brightness 140)))))

(defcustom neomacs-scroll-bar-track-opacity 60
  "Scroll bar track background opacity (0-100, as percentage)."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-bar-config)
           (neomacs-set-scroll-bar-config
            (if (boundp 'neomacs-scroll-bar-width)
                neomacs-scroll-bar-width 12)
            (if (boundp 'neomacs-scroll-bar-thumb-radius)
                neomacs-scroll-bar-thumb-radius 40)
            val
            (if (boundp 'neomacs-scroll-bar-hover-brightness)
                neomacs-scroll-bar-hover-brightness 140)))))

(defcustom neomacs-scroll-bar-hover-brightness 140
  "Scroll bar thumb hover brightness (0-200, as percentage of 1.0)."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-bar-config)
           (neomacs-set-scroll-bar-config
            (if (boundp 'neomacs-scroll-bar-width)
                neomacs-scroll-bar-width 12)
            (if (boundp 'neomacs-scroll-bar-thumb-radius)
                neomacs-scroll-bar-thumb-radius 40)
            (if (boundp 'neomacs-scroll-bar-track-opacity)
                neomacs-scroll-bar-track-opacity 60)
            val))))

;;; Indent guides

(declare-function neomacs-set-indent-guides "neomacsterm.c"
  (&optional enabled color))

(defcustom neomacs-indent-guides nil
  "Enable indent guide rendering.
Non-nil enables thin vertical lines at indentation levels."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-indent-guides)
           (neomacs-set-indent-guides
            val
            (if (boundp 'neomacs-indent-guide-color)
                neomacs-indent-guide-color
              "gray30")))))

(defcustom neomacs-indent-guide-color "gray30"
  "Color for indent guide lines."
  :type 'color
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-indent-guides)
                    (boundp 'neomacs-indent-guides)
                    neomacs-indent-guides)
           (neomacs-set-indent-guides t val))))

;;; Rainbow indent guides

(declare-function neomacs-set-indent-guide-rainbow "neomacsterm.c"
  (&optional enabled colors))

(defcustom neomacs-indent-guide-rainbow nil
  "Enable rainbow indent guide coloring.
Non-nil cycles indent guide colors by depth level."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-indent-guide-rainbow)
           (neomacs-set-indent-guide-rainbow
            val
            (if (boundp 'neomacs-indent-guide-rainbow-colors)
                neomacs-indent-guide-rainbow-colors
              nil)))))

(defcustom neomacs-indent-guide-rainbow-colors
  '("#e04040" "#e09040" "#e0e040" "#40e040" "#40e0e0" "#a040e0")
  "List of colors for rainbow indent guides (up to 6).
Each deeper indentation level uses the next color in the cycle."
  :type '(repeat color)
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-indent-guide-rainbow)
                    (boundp 'neomacs-indent-guide-rainbow)
                    neomacs-indent-guide-rainbow)
           (neomacs-set-indent-guide-rainbow t val))))

;;; Current line highlight

(declare-function neomacs-set-line-highlight "neomacsterm.c"
  (&optional enabled color))

(defcustom neomacs-line-highlight nil
  "Enable GPU-rendered current line highlight.
Non-nil highlights the line at point with a subtle background."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-line-highlight)
           (neomacs-set-line-highlight
            val
            (if (boundp 'neomacs-line-highlight-color)
                neomacs-line-highlight-color
              nil)))))

(defcustom neomacs-line-highlight-color nil
  "Color for current line highlight.
A color string, or nil for default."
  :type '(choice (const :tag "Default" nil)
                 (color :tag "Color"))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-line-highlight)
                    (boundp 'neomacs-line-highlight)
                    neomacs-line-highlight)
           (neomacs-set-line-highlight t val))))

;;; Visible whitespace

(declare-function neomacs-set-show-whitespace "neomacsterm.c"
  (&optional enabled color))

(defcustom neomacs-show-whitespace nil
  "Enable GPU-rendered visible whitespace.
Non-nil shows dots for spaces and arrows for tabs."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-show-whitespace)
           (neomacs-set-show-whitespace
            val
            (if (boundp 'neomacs-whitespace-color)
                neomacs-whitespace-color
              nil)))))

(defcustom neomacs-whitespace-color nil
  "Color for visible whitespace indicators.
A color string, or nil for default gray."
  :type '(choice (const :tag "Default" nil)
                 (color :tag "Color"))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-show-whitespace)
                    (boundp 'neomacs-show-whitespace)
                    neomacs-show-whitespace)
           (neomacs-set-show-whitespace t val))))

(defcustom neomacs-inactive-dim nil
  "Enable dimming of inactive windows.
Non-nil draws a dark overlay on non-selected windows."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-inactive-dim)
           (neomacs-set-inactive-dim
            val
            (if (boundp 'neomacs-inactive-dim-opacity)
                neomacs-inactive-dim-opacity
              nil)))))

(defcustom neomacs-inactive-dim-opacity 0.15
  "Opacity for inactive window dimming overlay.
A number between 0.0 (no dimming) and 1.0 (fully black)."
  :type '(number :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-inactive-dim)
                    (boundp 'neomacs-inactive-dim)
                    neomacs-inactive-dim)
           (neomacs-set-inactive-dim t val))))

(defcustom neomacs-mode-line-separator nil
  "Style for mode-line separator decoration.
A symbol: nil (none), `line' (thin line), `shadow' (shadow effect),
or `gradient' (gradient fade above mode-line)."
  :type '(choice (const :tag "None" nil)
                 (const :tag "Thin line" line)
                 (const :tag "Shadow" shadow)
                 (const :tag "Gradient" gradient))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-mode-line-separator)
           (neomacs-set-mode-line-separator
            val
            (if (boundp 'neomacs-mode-line-separator-color)
                neomacs-mode-line-separator-color
              nil)
            (if (boundp 'neomacs-mode-line-separator-height)
                neomacs-mode-line-separator-height
              nil)))))

(defcustom neomacs-mode-line-separator-color nil
  "Color for mode-line separator.
A color string, or nil for default black."
  :type '(choice (const :tag "Default" nil)
                 (color :tag "Color"))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-mode-line-separator)
                    (boundp 'neomacs-mode-line-separator)
                    neomacs-mode-line-separator)
           (neomacs-set-mode-line-separator
            neomacs-mode-line-separator val
            (if (boundp 'neomacs-mode-line-separator-height)
                neomacs-mode-line-separator-height
              nil)))))

(defcustom neomacs-mode-line-separator-height 3
  "Height of the mode-line separator in pixels."
  :type '(integer :tag "Height")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-mode-line-separator)
                    (boundp 'neomacs-mode-line-separator)
                    neomacs-mode-line-separator)
           (neomacs-set-mode-line-separator
            neomacs-mode-line-separator
            (if (boundp 'neomacs-mode-line-separator-color)
                neomacs-mode-line-separator-color
              nil)
            val))))

(defcustom neomacs-cursor-glow nil
  "Enable a soft glow effect around the cursor.
Non-nil renders concentric semi-transparent rings behind the cursor."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-glow)
           (neomacs-set-cursor-glow
            val
            (if (boundp 'neomacs-cursor-glow-color)
                neomacs-cursor-glow-color
              nil)
            (if (boundp 'neomacs-cursor-glow-radius)
                neomacs-cursor-glow-radius
              nil)))))

(defcustom neomacs-cursor-glow-color nil
  "Color for cursor glow effect.
A color string, or nil for default light blue."
  :type '(choice (const :tag "Default" nil)
                 (color :tag "Color"))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-glow)
                    (boundp 'neomacs-cursor-glow)
                    neomacs-cursor-glow)
           (neomacs-set-cursor-glow t val
            (if (boundp 'neomacs-cursor-glow-radius)
                neomacs-cursor-glow-radius
              nil)))))

(defcustom neomacs-cursor-glow-radius 30
  "Radius of cursor glow in pixels."
  :type '(integer :tag "Radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-glow)
                    (boundp 'neomacs-cursor-glow)
                    neomacs-cursor-glow)
           (neomacs-set-cursor-glow t
            (if (boundp 'neomacs-cursor-glow-color)
                neomacs-cursor-glow-color
              nil)
            val))))

;;; Cursor pulse

(declare-function neomacs-set-cursor-pulse "neomacsterm.c"
  (&optional enabled speed))

(defcustom neomacs-cursor-pulse nil
  "Enable sinusoidal pulse animation on cursor glow.
Non-nil modulates the cursor glow opacity with a smooth sine wave.
Requires `neomacs-cursor-glow' to be enabled for visible effect."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-pulse)
           (neomacs-set-cursor-pulse
            val
            (if (boundp 'neomacs-cursor-pulse-speed)
                neomacs-cursor-pulse-speed
              nil)))))

(defcustom neomacs-cursor-pulse-speed 100
  "Cursor pulse speed (100 = 1.0 Hz, 200 = 2.0 Hz, 50 = 0.5 Hz)."
  :type '(integer :tag "Speed")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pulse)
                    (boundp 'neomacs-cursor-pulse)
                    neomacs-cursor-pulse)
           (neomacs-set-cursor-pulse t val))))

;;; Focus mode

(declare-function neomacs-set-focus-mode "neomacsterm.c"
  (&optional enabled opacity))

(defcustom neomacs-focus-mode nil
  "Enable focus mode that dims text outside the current paragraph.
Non-nil draws semi-transparent overlays above and below the paragraph
containing the cursor, helping focus attention on the current context."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-focus-mode)
           (neomacs-set-focus-mode
            val
            (if (boundp 'neomacs-focus-mode-opacity)
                neomacs-focus-mode-opacity
              nil)))))

(defcustom neomacs-focus-mode-opacity 40
  "Opacity of the dimming overlay in focus mode (0-100).
Higher values produce a darker overlay, more strongly dimming
non-focused paragraphs."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-mode)
                    (boundp 'neomacs-focus-mode)
                    neomacs-focus-mode)
           (neomacs-set-focus-mode t val))))

;;; Minimap

(declare-function neomacs-set-minimap "neomacsterm.c"
  (&optional enabled width))

(defcustom neomacs-minimap nil
  "Enable minimap code overview column on the right side of windows.
Non-nil renders a zoomed-out view of visible code using tiny colored
blocks representing syntax-highlighted characters."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-minimap)
           (neomacs-set-minimap
            val
            (if (boundp 'neomacs-minimap-width)
                neomacs-minimap-width
              nil)))))

(defcustom neomacs-minimap-width 80
  "Width of the minimap column in pixels."
  :type '(integer :tag "Width")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-minimap)
                    (boundp 'neomacs-minimap)
                    neomacs-minimap)
           (neomacs-set-minimap t val))))

;;; Typing ripple

(declare-function neomacs-set-typing-ripple "neomacsterm.c"
  (&optional enabled max-radius duration-ms))

(defcustom neomacs-typing-ripple nil
  "Enable ripple effect animation on cursor movement.
Non-nil shows expanding circles originating from the cursor
whenever it moves, providing visual feedback during typing."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-typing-ripple)
           (neomacs-set-typing-ripple
            val
            (if (boundp 'neomacs-typing-ripple-radius)
                neomacs-typing-ripple-radius
              nil)
            (if (boundp 'neomacs-typing-ripple-duration)
                neomacs-typing-ripple-duration
              nil)))))

(defcustom neomacs-typing-ripple-radius 40
  "Maximum radius of typing ripple in pixels."
  :type '(integer :tag "Radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-typing-ripple)
                    (boundp 'neomacs-typing-ripple)
                    neomacs-typing-ripple)
           (neomacs-set-typing-ripple t val
            (if (boundp 'neomacs-typing-ripple-duration)
                neomacs-typing-ripple-duration
              nil)))))

(defcustom neomacs-typing-ripple-duration 300
  "Duration of typing ripple animation in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-typing-ripple)
                    (boundp 'neomacs-typing-ripple)
                    neomacs-typing-ripple)
           (neomacs-set-typing-ripple t
            (if (boundp 'neomacs-typing-ripple-radius)
                neomacs-typing-ripple-radius
              nil)
            val))))

;;; Search highlight pulse

(declare-function neomacs-set-search-pulse "neomacsterm.c"
  (&optional enabled))

(defcustom neomacs-search-pulse nil
  "Enable pulsing glow animation on the current isearch match.
Non-nil draws a pulsing highlight around isearch matches to make
the active match stand out.  Automatically activates during isearch."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-search-pulse)
           (if val
               (progn
                 (add-hook 'isearch-mode-hook #'neomacs--search-pulse-start)
                 (add-hook 'isearch-mode-end-hook #'neomacs--search-pulse-stop))
             (remove-hook 'isearch-mode-hook #'neomacs--search-pulse-start)
             (remove-hook 'isearch-mode-end-hook #'neomacs--search-pulse-stop)
             (neomacs-set-search-pulse nil)))))

(defun neomacs--search-pulse-start ()
  "Activate search pulse when isearch starts."
  (when (and (boundp 'neomacs-search-pulse) neomacs-search-pulse
             (fboundp 'neomacs-set-search-pulse))
    (neomacs-set-search-pulse t)))

(defun neomacs--search-pulse-stop ()
  "Deactivate search pulse when isearch ends."
  (when (fboundp 'neomacs-set-search-pulse)
    (neomacs-set-search-pulse nil)))

;;; Background pattern

(declare-function neomacs-set-background-pattern "neomacsterm.c"
  (&optional style spacing color opacity))

(defcustom neomacs-background-pattern 0
  "Background pattern style for the frame.
0 = none, 1 = dots, 2 = grid, 3 = crosshatch."
  :type '(choice (const :tag "None" 0)
                 (const :tag "Dots" 1)
                 (const :tag "Grid" 2)
                 (const :tag "Crosshatch" 3))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-background-pattern)
           (neomacs-set-background-pattern
            val
            (if (boundp 'neomacs-background-pattern-spacing)
                neomacs-background-pattern-spacing nil)
            (if (boundp 'neomacs-background-pattern-color)
                neomacs-background-pattern-color nil)
            (if (boundp 'neomacs-background-pattern-opacity)
                neomacs-background-pattern-opacity nil)))))

(defcustom neomacs-background-pattern-spacing 20
  "Spacing between background pattern elements in pixels."
  :type '(integer :tag "Spacing")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-background-pattern)
                    (boundp 'neomacs-background-pattern)
                    (> neomacs-background-pattern 0))
           (neomacs-set-background-pattern
            neomacs-background-pattern val
            (if (boundp 'neomacs-background-pattern-color)
                neomacs-background-pattern-color nil)
            (if (boundp 'neomacs-background-pattern-opacity)
                neomacs-background-pattern-opacity nil)))))

(defcustom neomacs-background-pattern-color "gray50"
  "Color of the background pattern."
  :type '(color :tag "Color")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-background-pattern)
                    (boundp 'neomacs-background-pattern)
                    (> neomacs-background-pattern 0))
           (neomacs-set-background-pattern
            neomacs-background-pattern
            (if (boundp 'neomacs-background-pattern-spacing)
                neomacs-background-pattern-spacing nil)
            val
            (if (boundp 'neomacs-background-pattern-opacity)
                neomacs-background-pattern-opacity nil)))))

(defcustom neomacs-background-pattern-opacity 5
  "Opacity of the background pattern (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-background-pattern)
                    (boundp 'neomacs-background-pattern)
                    (> neomacs-background-pattern 0))
           (neomacs-set-background-pattern
            neomacs-background-pattern
            (if (boundp 'neomacs-background-pattern-spacing)
                neomacs-background-pattern-spacing nil)
            (if (boundp 'neomacs-background-pattern-color)
                neomacs-background-pattern-color nil)
            val))))

;;; Zen mode

(declare-function neomacs-set-zen-mode "neomacsterm.c"
  (&optional enabled content-width-pct margin-opacity))

(defcustom neomacs-zen-mode nil
  "Enable zen/distraction-free mode with centered content.
Non-nil dims the margins of each window, visually centering the
text content for a focused writing experience."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-zen-mode)
           (neomacs-set-zen-mode
            val
            (if (boundp 'neomacs-zen-content-width)
                neomacs-zen-content-width nil)
            (if (boundp 'neomacs-zen-margin-opacity)
                neomacs-zen-margin-opacity nil)))))

(defcustom neomacs-zen-content-width 60
  "Content width as percentage of window width in zen mode (20-100)."
  :type '(integer :tag "Width %")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zen-mode)
                    (boundp 'neomacs-zen-mode)
                    neomacs-zen-mode)
           (neomacs-set-zen-mode t val
            (if (boundp 'neomacs-zen-margin-opacity)
                neomacs-zen-margin-opacity nil)))))

(defcustom neomacs-zen-margin-opacity 30
  "Opacity of zen mode margin overlays (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zen-mode)
                    (boundp 'neomacs-zen-mode)
                    neomacs-zen-mode)
           (neomacs-set-zen-mode t
            (if (boundp 'neomacs-zen-content-width)
                neomacs-zen-content-width nil)
            val))))

;; --- Cursor color cycling ---
(declare-function neomacs-set-cursor-color-cycle "neomacsterm.c"
  (&optional enabled speed saturation lightness))

(defcustom neomacs-cursor-color-cycle nil
  "Enable cursor color cycling (rainbow hue rotation).
Non-nil smoothly cycles the cursor color through the rainbow
spectrum, creating a colorful animated cursor effect."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-color-cycle)
           (neomacs-set-cursor-color-cycle
            val
            (if (boundp 'neomacs-cursor-color-cycle-speed)
                neomacs-cursor-color-cycle-speed nil)
            (if (boundp 'neomacs-cursor-color-cycle-saturation)
                neomacs-cursor-color-cycle-saturation nil)
            (if (boundp 'neomacs-cursor-color-cycle-lightness)
                neomacs-cursor-color-cycle-lightness nil)))))

(defcustom neomacs-cursor-color-cycle-speed 50
  "Speed of cursor color cycling (cycles per second * 100).
50 = 0.5 cycles per second (one full rainbow every 2 seconds)."
  :type '(integer :tag "Speed")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-color-cycle)
                    (boundp 'neomacs-cursor-color-cycle)
                    neomacs-cursor-color-cycle)
           (neomacs-set-cursor-color-cycle t val
            (if (boundp 'neomacs-cursor-color-cycle-saturation)
                neomacs-cursor-color-cycle-saturation nil)
            (if (boundp 'neomacs-cursor-color-cycle-lightness)
                neomacs-cursor-color-cycle-lightness nil)))))

(defcustom neomacs-cursor-color-cycle-saturation 80
  "Saturation of cursor color cycling colors (0-100)."
  :type '(integer :tag "Saturation")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-color-cycle)
                    (boundp 'neomacs-cursor-color-cycle)
                    neomacs-cursor-color-cycle)
           (neomacs-set-cursor-color-cycle t
            (if (boundp 'neomacs-cursor-color-cycle-speed)
                neomacs-cursor-color-cycle-speed nil)
            val
            (if (boundp 'neomacs-cursor-color-cycle-lightness)
                neomacs-cursor-color-cycle-lightness nil)))))

(defcustom neomacs-cursor-color-cycle-lightness 60
  "Lightness of cursor color cycling colors (0-100)."
  :type '(integer :tag "Lightness")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-color-cycle)
                    (boundp 'neomacs-cursor-color-cycle)
                    neomacs-cursor-color-cycle)
           (neomacs-set-cursor-color-cycle t
            (if (boundp 'neomacs-cursor-color-cycle-speed)
                neomacs-cursor-color-cycle-speed nil)
            (if (boundp 'neomacs-cursor-color-cycle-saturation)
                neomacs-cursor-color-cycle-saturation nil)
            val))))

;; --- Scroll progress indicator ---
(declare-function neomacs-set-scroll-progress "neomacsterm.c"
  (&optional enabled height r g b opacity))

(defcustom neomacs-scroll-progress nil
  "Enable scroll progress indicator bar at top of windows.
Non-nil shows a thin colored bar indicating scroll position within the buffer."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-progress)
           (neomacs-set-scroll-progress val))))

(defcustom neomacs-scroll-progress-height 2
  "Height of the scroll progress bar in pixels."
  :type '(integer :tag "Height")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-progress)
                    (boundp 'neomacs-scroll-progress)
                    neomacs-scroll-progress)
           (neomacs-set-scroll-progress t val))))

;; --- Active window border glow ---
(declare-function neomacs-set-window-glow "neomacsterm.c"
  (&optional enabled r g b radius intensity))

(defcustom neomacs-window-glow nil
  "Enable border glow effect around the active window.
Non-nil draws a soft glowing border to highlight the selected window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-window-glow)
           (neomacs-set-window-glow val))))

(defcustom neomacs-window-glow-radius 8
  "Radius of the window border glow in pixels."
  :type '(integer :tag "Radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-glow)
                    (boundp 'neomacs-window-glow)
                    neomacs-window-glow)
           (neomacs-set-window-glow t nil nil nil val))))

(defcustom neomacs-window-glow-intensity 40
  "Peak opacity of the window border glow (0-100)."
  :type '(integer :tag "Intensity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-glow)
                    (boundp 'neomacs-window-glow)
                    neomacs-window-glow)
           (neomacs-set-window-glow t nil nil nil nil val))))

;; --- Breadcrumb/path bar ---
(declare-function neomacs-set-breadcrumb "neomacsterm.c"
  (&optional enabled opacity))

(defcustom neomacs-breadcrumb nil
  "Enable breadcrumb/path bar overlay at top of windows.
Non-nil shows the buffer file path as a breadcrumb bar with
directory hierarchy and separator characters."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-breadcrumb)
           (neomacs-set-breadcrumb val
            (if (boundp 'neomacs-breadcrumb-opacity)
                neomacs-breadcrumb-opacity nil)))))

(defcustom neomacs-breadcrumb-opacity 70
  "Background opacity of the breadcrumb bar (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-breadcrumb)
                    (boundp 'neomacs-breadcrumb)
                    neomacs-breadcrumb)
           (neomacs-set-breadcrumb t val))))

;; --- Breadcrumb title fade ---
(declare-function neomacs-set-title-fade "neomacsterm.c"
  (&optional enabled duration-ms))

(defcustom neomacs-title-fade nil
  "Enable crossfade animation for breadcrumb title on buffer switch.
Non-nil smoothly crossfades the old file path to the new one
when switching buffers in a window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-title-fade)
           (neomacs-set-title-fade val
            (if (boundp 'neomacs-title-fade-duration)
                neomacs-title-fade-duration nil)))))

(defcustom neomacs-title-fade-duration 300
  "Duration of the breadcrumb title fade in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-title-fade)
                    (boundp 'neomacs-title-fade)
                    neomacs-title-fade)
           (neomacs-set-title-fade t val))))

;; --- Smooth border color transition ---
(declare-function neomacs-set-border-transition "neomacsterm.c"
  (&optional enabled r g b duration-ms))

(defcustom neomacs-border-transition nil
  "Enable smooth border color transition on window focus change.
Non-nil draws a colored border that smoothly fades in around the
selected window and fades out from the previously selected one."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-border-transition)
           (neomacs-set-border-transition val))))

(defcustom neomacs-border-transition-duration 200
  "Duration of border color transition in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-border-transition)
                    (boundp 'neomacs-border-transition)
                    neomacs-border-transition)
           (neomacs-set-border-transition t nil nil nil val))))

;; --- Buffer-local accent color strip ---
(declare-function neomacs-set-accent-strip "neomacsterm.c"
  (&optional enabled width))

(defcustom neomacs-accent-strip nil
  "Enable buffer-local accent color strip on left edge of windows.
Non-nil renders a thin colored indicator on the left edge, with
color derived from the buffer's file extension for quick identification."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-accent-strip)
           (neomacs-set-accent-strip val
            (if (boundp 'neomacs-accent-strip-width)
                neomacs-accent-strip-width nil)))))

(defcustom neomacs-accent-strip-width 3
  "Width of the accent color strip in pixels."
  :type '(integer :tag "Width")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-accent-strip)
                    (boundp 'neomacs-accent-strip)
                    neomacs-accent-strip)
           (neomacs-set-accent-strip t val))))

;; --- Frosted glass effect ---
(declare-function neomacs-set-frosted-glass "neomacsterm.c"
  (&optional enabled opacity blur))

(defcustom neomacs-frosted-glass nil
  "Enable frosted glass effect on mode-lines.
Non-nil renders a semi-transparent frosted overlay with noise grain
on mode-line areas to simulate a blurred glass appearance."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-frosted-glass)
           (neomacs-set-frosted-glass val
            (if (boundp 'neomacs-frosted-glass-opacity)
                neomacs-frosted-glass-opacity nil)
            (if (boundp 'neomacs-frosted-glass-blur)
                neomacs-frosted-glass-blur nil)))))

(defcustom neomacs-frosted-glass-opacity 30
  "Frost intensity for the frosted glass effect (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frosted-glass)
                    (boundp 'neomacs-frosted-glass)
                    neomacs-frosted-glass)
           (neomacs-set-frosted-glass t val
            (if (boundp 'neomacs-frosted-glass-blur)
                neomacs-frosted-glass-blur nil)))))

(defcustom neomacs-frosted-glass-blur 4
  "Blur spread radius in pixels for the frosted glass effect."
  :type '(integer :tag "Blur radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frosted-glass)
                    (boundp 'neomacs-frosted-glass)
                    neomacs-frosted-glass)
           (neomacs-set-frosted-glass t
            (if (boundp 'neomacs-frosted-glass-opacity)
                neomacs-frosted-glass-opacity nil)
            val))))

;; --- Selection region glow ---
(declare-function neomacs-set-region-glow "neomacsterm.c"
  (&optional enabled face-id radius opacity))

(defcustom neomacs-region-glow nil
  "Enable selection region glow highlight.
Non-nil renders a soft glow around the active text selection, using
the region face background color for the glow tint."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-region-glow)
           (neomacs-set-region-glow val nil
            (if (boundp 'neomacs-region-glow-radius)
                neomacs-region-glow-radius nil)
            (if (boundp 'neomacs-region-glow-opacity)
                neomacs-region-glow-opacity nil)))))

(defcustom neomacs-region-glow-radius 6
  "Glow radius in pixels for region highlight."
  :type '(integer :tag "Radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-region-glow)
                    (boundp 'neomacs-region-glow)
                    neomacs-region-glow)
           (neomacs-set-region-glow t nil val))))

(defcustom neomacs-region-glow-opacity 30
  "Glow opacity for region highlight (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-region-glow)
                    (boundp 'neomacs-region-glow)
                    neomacs-region-glow)
           (neomacs-set-region-glow t nil nil val))))

;; --- Idle screen dimming ---
(declare-function neomacs-set-idle-dim "neomacsterm.c"
  (&optional enabled delay-secs opacity fade-ms))

(defcustom neomacs-idle-dim nil
  "Enable idle screen dimming after inactivity.
Non-nil dims the entire frame after a period of no keyboard or mouse
activity, then smoothly restores brightness on any input."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-idle-dim)
           (neomacs-set-idle-dim val
            (if (boundp 'neomacs-idle-dim-delay)
                neomacs-idle-dim-delay nil)
            (if (boundp 'neomacs-idle-dim-opacity)
                neomacs-idle-dim-opacity nil)
            (if (boundp 'neomacs-idle-dim-fade)
                neomacs-idle-dim-fade nil)))))

(defcustom neomacs-idle-dim-delay 60
  "Seconds of inactivity before idle dimming starts."
  :type '(integer :tag "Delay (seconds)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-idle-dim)
                    (boundp 'neomacs-idle-dim)
                    neomacs-idle-dim)
           (neomacs-set-idle-dim t val))))

(defcustom neomacs-idle-dim-opacity 40
  "Dimming darkness for idle screen dim (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-idle-dim)
                    (boundp 'neomacs-idle-dim)
                    neomacs-idle-dim)
           (neomacs-set-idle-dim t nil val))))

(defcustom neomacs-idle-dim-fade 500
  "Fade transition duration in milliseconds for idle dimming."
  :type '(integer :tag "Fade (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-idle-dim)
                    (boundp 'neomacs-idle-dim)
                    neomacs-idle-dim)
           (neomacs-set-idle-dim t nil nil val))))

;; --- Cursor drop shadow ---
(declare-function neomacs-set-cursor-shadow "neomacsterm.c"
  (&optional enabled offset-x offset-y opacity))

(defcustom neomacs-cursor-shadow nil
  "Enable cursor drop shadow.
Non-nil renders a soft drop shadow behind the cursor for a 3D
depth effect."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-shadow)
           (neomacs-set-cursor-shadow val
            (if (boundp 'neomacs-cursor-shadow-offset-x)
                neomacs-cursor-shadow-offset-x nil)
            (if (boundp 'neomacs-cursor-shadow-offset-y)
                neomacs-cursor-shadow-offset-y nil)
            (if (boundp 'neomacs-cursor-shadow-opacity)
                neomacs-cursor-shadow-opacity nil)))))

(defcustom neomacs-cursor-shadow-offset-x 2
  "Horizontal shadow offset in pixels."
  :type '(integer :tag "X Offset")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shadow)
                    (boundp 'neomacs-cursor-shadow)
                    neomacs-cursor-shadow)
           (neomacs-set-cursor-shadow t val))))

(defcustom neomacs-cursor-shadow-offset-y 2
  "Vertical shadow offset in pixels."
  :type '(integer :tag "Y Offset")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shadow)
                    (boundp 'neomacs-cursor-shadow)
                    neomacs-cursor-shadow)
           (neomacs-set-cursor-shadow t nil val))))

(defcustom neomacs-cursor-shadow-opacity 30
  "Shadow darkness (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shadow)
                    (boundp 'neomacs-cursor-shadow)
                    neomacs-cursor-shadow)
           (neomacs-set-cursor-shadow t nil nil val))))

;; --- Animated focus ring ---
(declare-function neomacs-set-focus-ring "neomacsterm.c"
  (&optional enabled r g b opacity dash-length speed))

(defcustom neomacs-focus-ring nil
  "Enable animated focus ring around the selected window.
Non-nil renders a marching-ants-style dashed border around the
focused window with continuous animation."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-focus-ring)
           (neomacs-set-focus-ring val nil nil nil
            (if (boundp 'neomacs-focus-ring-opacity)
                neomacs-focus-ring-opacity nil)
            (if (boundp 'neomacs-focus-ring-dash-length)
                neomacs-focus-ring-dash-length nil)
            (if (boundp 'neomacs-focus-ring-speed)
                neomacs-focus-ring-speed nil)))))

(defcustom neomacs-focus-ring-opacity 50
  "Focus ring opacity (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-ring)
                    (boundp 'neomacs-focus-ring)
                    neomacs-focus-ring)
           (neomacs-set-focus-ring t nil nil nil val))))

(defcustom neomacs-focus-ring-dash-length 8
  "Dash segment length in pixels for focus ring."
  :type '(integer :tag "Dash length")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-ring)
                    (boundp 'neomacs-focus-ring)
                    neomacs-focus-ring)
           (neomacs-set-focus-ring t nil nil nil nil val))))

(defcustom neomacs-focus-ring-speed 40
  "Animation speed in pixels per second for focus ring."
  :type '(integer :tag "Speed")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-ring)
                    (boundp 'neomacs-focus-ring)
                    neomacs-focus-ring)
           (neomacs-set-focus-ring t nil nil nil nil nil val))))

;; --- Text fade-in animation ---
(declare-function neomacs-set-text-fade-in "neomacsterm.c"
  (&optional enabled duration-ms))

(defcustom neomacs-text-fade-in nil
  "Enable text fade-in animation for new content.
Non-nil makes text in a window fade in from transparent when the
buffer content changes (scroll, buffer switch)."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-text-fade-in)
           (neomacs-set-text-fade-in val
            (if (boundp 'neomacs-text-fade-in-duration)
                neomacs-text-fade-in-duration nil)))))

(defcustom neomacs-text-fade-in-duration 150
  "Text fade-in duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-text-fade-in)
                    (boundp 'neomacs-text-fade-in)
                    neomacs-text-fade-in)
           (neomacs-set-text-fade-in t val))))

;; --- Scroll line spacing animation ---
(declare-function neomacs-set-scroll-line-spacing "neomacsterm.c"
  (&optional enabled max-spacing duration-ms))

(defcustom neomacs-scroll-line-spacing nil
  "Enable scroll line spacing animation (accordion effect).
Non-nil applies a transient accordion-style line spacing animation
when scrolling, where lines at the leading edge briefly spread
apart and then settle back."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-line-spacing)
           (neomacs-set-scroll-line-spacing val
            (if (boundp 'neomacs-scroll-line-spacing-max)
                neomacs-scroll-line-spacing-max nil)
            (if (boundp 'neomacs-scroll-line-spacing-duration)
                neomacs-scroll-line-spacing-duration nil)))))

(defcustom neomacs-scroll-line-spacing-max 6
  "Maximum extra spacing in pixels at leading edge."
  :type '(integer :tag "Max Spacing")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-line-spacing)
                    (boundp 'neomacs-scroll-line-spacing)
                    neomacs-scroll-line-spacing)
           (neomacs-set-scroll-line-spacing t val))))

(defcustom neomacs-scroll-line-spacing-duration 200
  "Animation duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-line-spacing)
                    (boundp 'neomacs-scroll-line-spacing)
                    neomacs-scroll-line-spacing)
           (neomacs-set-scroll-line-spacing t nil val))))

;; --- Cursor wake animation ---
(declare-function neomacs-set-cursor-wake "neomacsterm.c"
  (&optional enabled duration-ms scale-pct))

(defcustom neomacs-cursor-wake nil
  "Enable cursor wake animation.
Non-nil makes the cursor briefly scale up (pop) when it becomes
visible after blinking off."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-wake)
           (neomacs-set-cursor-wake val
            (if (boundp 'neomacs-cursor-wake-duration)
                neomacs-cursor-wake-duration nil)
            (if (boundp 'neomacs-cursor-wake-scale)
                neomacs-cursor-wake-scale nil)))))

(defcustom neomacs-cursor-wake-duration 120
  "Cursor wake animation duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-wake)
                    (boundp 'neomacs-cursor-wake)
                    neomacs-cursor-wake)
           (neomacs-set-cursor-wake t val
            (if (boundp 'neomacs-cursor-wake-scale)
                neomacs-cursor-wake-scale nil)))))

(defcustom neomacs-cursor-wake-scale 130
  "Initial scale percentage for cursor wake pop effect.
130 means the cursor starts at 130% of its normal size and
animates down to 100%."
  :type '(integer :tag "Scale (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-wake)
                    (boundp 'neomacs-cursor-wake)
                    neomacs-cursor-wake)
           (neomacs-set-cursor-wake t
            (if (boundp 'neomacs-cursor-wake-duration)
                neomacs-cursor-wake-duration nil)
            val))))

;; --- Scroll momentum indicator ---
(declare-function neomacs-set-scroll-momentum "neomacsterm.c"
  (&optional enabled fade-ms width))

(defcustom neomacs-scroll-momentum nil
  "Enable per-window scroll momentum indicator.
Non-nil shows a brief directional gradient bar at the right edge
of each window during scrolling."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-momentum)
           (neomacs-set-scroll-momentum val
            (if (boundp 'neomacs-scroll-momentum-fade)
                neomacs-scroll-momentum-fade nil)
            (if (boundp 'neomacs-scroll-momentum-width)
                neomacs-scroll-momentum-width nil)))))

(defcustom neomacs-scroll-momentum-fade 300
  "Scroll momentum indicator fade-out duration in milliseconds."
  :type '(integer :tag "Fade (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-momentum)
                    (boundp 'neomacs-scroll-momentum)
                    neomacs-scroll-momentum)
           (neomacs-set-scroll-momentum t val
            (if (boundp 'neomacs-scroll-momentum-width)
                neomacs-scroll-momentum-width nil)))))

(defcustom neomacs-scroll-momentum-width 3
  "Scroll momentum indicator bar width in pixels."
  :type '(integer :tag "Width (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-momentum)
                    (boundp 'neomacs-scroll-momentum)
                    neomacs-scroll-momentum)
           (neomacs-set-scroll-momentum t
            (if (boundp 'neomacs-scroll-momentum-fade)
                neomacs-scroll-momentum-fade nil)
            val))))

;; --- Mode-line content transition ---
(declare-function neomacs-set-mode-line-transition "neomacsterm.c"
  (&optional enabled duration-ms))

(defcustom neomacs-mode-line-transition nil
  "Enable smooth mode-line content transition.
Non-nil makes mode-line text fade in when its content changes."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-mode-line-transition)
           (neomacs-set-mode-line-transition val
            (if (boundp 'neomacs-mode-line-transition-duration)
                neomacs-mode-line-transition-duration nil)))))

(defcustom neomacs-mode-line-transition-duration 200
  "Mode-line transition fade duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-mode-line-transition)
                    (boundp 'neomacs-mode-line-transition)
                    neomacs-mode-line-transition)
           (neomacs-set-mode-line-transition t val))))

;; --- Window background tint based on file type ---
(declare-function neomacs-set-window-mode-tint "neomacsterm.c"
  (&optional enabled opacity))

(defcustom neomacs-window-mode-tint nil
  "Enable window background tint based on file type.
Non-nil applies a subtle background color tint to each window based
on the file extension of its buffer."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-window-mode-tint)
           (neomacs-set-window-mode-tint val
            (if (boundp 'neomacs-window-mode-tint-opacity)
                neomacs-window-mode-tint-opacity nil)))))

(defcustom neomacs-window-mode-tint-opacity 3
  "Tint intensity for file-type background coloring (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-mode-tint)
                    (boundp 'neomacs-window-mode-tint)
                    neomacs-window-mode-tint)
           (neomacs-set-window-mode-tint t val))))

;; --- Window watermark for empty buffers ---
(declare-function neomacs-set-window-watermark "neomacsterm.c"
  (&optional enabled opacity threshold))

(defcustom neomacs-window-watermark nil
  "Enable window watermark for empty buffers.
Non-nil renders a large faded text showing the buffer name centered
in windows whose buffer has very little content."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-window-watermark)
           (neomacs-set-window-watermark val
            (if (boundp 'neomacs-window-watermark-opacity)
                neomacs-window-watermark-opacity nil)
            (if (boundp 'neomacs-window-watermark-threshold)
                neomacs-window-watermark-threshold nil)))))

(defcustom neomacs-window-watermark-opacity 8
  "Watermark text opacity (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-watermark)
                    (boundp 'neomacs-window-watermark)
                    neomacs-window-watermark)
           (neomacs-set-window-watermark t val))))

(defcustom neomacs-window-watermark-threshold 10
  "Maximum buffer size in characters to show watermark."
  :type '(integer :tag "Threshold")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-watermark)
                    (boundp 'neomacs-window-watermark)
                    neomacs-window-watermark)
           (neomacs-set-window-watermark t nil val))))

;; --- Cursor trail fade ---
(declare-function neomacs-set-cursor-trail-fade "neomacsterm.c"
  (&optional enabled length fade-ms))

(defcustom neomacs-cursor-trail-fade nil
  "Enable cursor trail fade effect.
Non-nil leaves fading ghost rectangles at previous cursor positions
as the cursor moves, creating a trail effect."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-trail-fade)
           (neomacs-set-cursor-trail-fade val
            (if (boundp 'neomacs-cursor-trail-fade-length)
                neomacs-cursor-trail-fade-length nil)
            (if (boundp 'neomacs-cursor-trail-fade-ms)
                neomacs-cursor-trail-fade-ms nil)))))

(defcustom neomacs-cursor-trail-fade-length 8
  "Maximum number of trail ghost positions to keep."
  :type '(integer :tag "Length")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-trail-fade)
                    (boundp 'neomacs-cursor-trail-fade)
                    neomacs-cursor-trail-fade)
           (neomacs-set-cursor-trail-fade t val))))

(defcustom neomacs-cursor-trail-fade-ms 300
  "Fade duration in milliseconds for each trail ghost."
  :type '(integer :tag "Fade (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-trail-fade)
                    (boundp 'neomacs-cursor-trail-fade)
                    neomacs-cursor-trail-fade)
           (neomacs-set-cursor-trail-fade t nil val))))

;; --- Noise/film grain overlay ---
(declare-function neomacs-set-noise-grain "neomacsterm.c"
  (&optional enabled intensity size))

(defcustom neomacs-noise-grain nil
  "Enable noise/film grain texture overlay.
Non-nil renders a subtle animated grain pattern over the entire frame,
simulating a CRT or film look."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-noise-grain)
           (neomacs-set-noise-grain val
            (if (boundp 'neomacs-noise-grain-intensity)
                neomacs-noise-grain-intensity nil)
            (if (boundp 'neomacs-noise-grain-size)
                neomacs-noise-grain-size nil)))))

(defcustom neomacs-noise-grain-intensity 3
  "Grain visibility intensity (0-100)."
  :type '(integer :tag "Intensity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-noise-grain)
                    (boundp 'neomacs-noise-grain)
                    neomacs-noise-grain)
           (neomacs-set-noise-grain t val
            (if (boundp 'neomacs-noise-grain-size)
                neomacs-noise-grain-size nil)))))

(defcustom neomacs-noise-grain-size 2
  "Grain cell size in pixels."
  :type '(integer :tag "Size")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-noise-grain)
                    (boundp 'neomacs-noise-grain)
                    neomacs-noise-grain)
           (neomacs-set-noise-grain t
            (if (boundp 'neomacs-noise-grain-intensity)
                neomacs-noise-grain-intensity nil)
            val))))

;; --- Window padding gradient ---
(declare-function neomacs-set-padding-gradient "neomacsterm.c"
  (&optional enabled r g b opacity width))

(defcustom neomacs-padding-gradient nil
  "Enable window padding gradient for depth effect.
Non-nil renders a subtle gradient at the inner edges of each window,
blending from an edge color inward, creating a sense of depth."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-padding-gradient)
           (neomacs-set-padding-gradient val
            (if (boundp 'neomacs-padding-gradient-r)
                neomacs-padding-gradient-r nil)
            (if (boundp 'neomacs-padding-gradient-g)
                neomacs-padding-gradient-g nil)
            (if (boundp 'neomacs-padding-gradient-b)
                neomacs-padding-gradient-b nil)
            (if (boundp 'neomacs-padding-gradient-opacity)
                neomacs-padding-gradient-opacity nil)
            (if (boundp 'neomacs-padding-gradient-width)
                neomacs-padding-gradient-width nil)))))

(defcustom neomacs-padding-gradient-opacity 15
  "Peak opacity for the padding gradient at window edges (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-padding-gradient)
                    (boundp 'neomacs-padding-gradient)
                    neomacs-padding-gradient)
           (neomacs-set-padding-gradient t nil nil nil val))))

(defcustom neomacs-padding-gradient-width 8
  "Width of the padding gradient in pixels."
  :type '(integer :tag "Width")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-padding-gradient)
                    (boundp 'neomacs-padding-gradient)
                    neomacs-padding-gradient)
           (neomacs-set-padding-gradient t nil nil nil nil val))))

;; --- Smooth cursor size transition ---
(declare-function neomacs-set-cursor-size-transition "neomacsterm.c"
  (&optional enabled duration-ms))

(defcustom neomacs-cursor-size-transition nil
  "Enable smooth cursor size transition on text-scale-adjust.
Non-nil smoothly animates the cursor width and height when the font
size changes, rather than snapping instantly to the new dimensions."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-size-transition)
           (neomacs-set-cursor-size-transition val
            (if (boundp 'neomacs-cursor-size-transition-duration)
                neomacs-cursor-size-transition-duration nil)))))

(defcustom neomacs-cursor-size-transition-duration 150
  "Duration of cursor size transition in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-size-transition)
                    (boundp 'neomacs-cursor-size-transition)
                    neomacs-cursor-size-transition)
           (neomacs-set-cursor-size-transition t val))))

;; --- Typing speed indicator ---
(declare-function neomacs-set-typing-speed "neomacsterm.c"
  (&optional enabled))

(defcustom neomacs-typing-speed nil
  "Enable typing speed (WPM) indicator overlay.
Non-nil shows a live words-per-minute counter in the bottom-right
corner of the active window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-typing-speed)
           (neomacs-set-typing-speed val))))

;; --- Window switch highlight fade ---
(declare-function neomacs-set-window-switch-fade "neomacsterm.c"
  (&optional enabled duration-ms intensity))

(defcustom neomacs-window-switch-fade nil
  "Enable highlight fade on window switch.
Non-nil flashes a brief white overlay on the newly selected window
when switching between windows, providing visual feedback."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-window-switch-fade)
           (neomacs-set-window-switch-fade val
            (if (boundp 'neomacs-window-switch-fade-duration)
                neomacs-window-switch-fade-duration nil)
            (if (boundp 'neomacs-window-switch-fade-intensity)
                neomacs-window-switch-fade-intensity nil)))))

(defcustom neomacs-window-switch-fade-duration 200
  "Duration of window switch fade animation in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-switch-fade)
                    (boundp 'neomacs-window-switch-fade)
                    neomacs-window-switch-fade)
           (neomacs-set-window-switch-fade t val
            (if (boundp 'neomacs-window-switch-fade-intensity)
                neomacs-window-switch-fade-intensity nil)))))

(defcustom neomacs-window-switch-fade-intensity 15
  "Intensity of window switch fade overlay (0-100).
Higher values make the flash brighter."
  :type '(integer :tag "Intensity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-switch-fade)
                    (boundp 'neomacs-window-switch-fade)
                    neomacs-window-switch-fade)
           (neomacs-set-window-switch-fade t
            (if (boundp 'neomacs-window-switch-fade-duration)
                neomacs-window-switch-fade-duration nil)
            val))))

;; --- Inactive window color tint ---
(declare-function neomacs-set-inactive-tint "neomacsterm.c"
  (&optional enabled r g b opacity))

(defcustom neomacs-inactive-tint nil
  "Enable color tint overlay on inactive windows.
Non-nil applies a subtle color wash on non-selected windows
to visually distinguish them from the active window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-inactive-tint)
           (neomacs-set-inactive-tint val))))

(defcustom neomacs-inactive-tint-opacity 10
  "Opacity of the inactive window color tint (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-inactive-tint)
                    (boundp 'neomacs-inactive-tint)
                    neomacs-inactive-tint)
           (neomacs-set-inactive-tint t nil nil nil val))))

;; --- Header/mode-line shadow ---
(declare-function neomacs-set-header-shadow "neomacsterm.c"
  (&optional enabled intensity size))

(defcustom neomacs-header-shadow nil
  "Enable shadow depth effect below header-line and above mode-line.
Non-nil draws a gradient shadow that creates visual depth
separation between content and status areas."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-header-shadow)
           (neomacs-set-header-shadow
            val
            (if (boundp 'neomacs-header-shadow-intensity)
                neomacs-header-shadow-intensity nil)
            (if (boundp 'neomacs-header-shadow-size)
                neomacs-header-shadow-size nil)))))

(defcustom neomacs-header-shadow-intensity 30
  "Intensity of header/mode-line shadow (0-100)."
  :type '(integer :tag "Intensity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-header-shadow)
                    (boundp 'neomacs-header-shadow)
                    neomacs-header-shadow)
           (neomacs-set-header-shadow t val
            (if (boundp 'neomacs-header-shadow-size)
                neomacs-header-shadow-size nil)))))

(defcustom neomacs-header-shadow-size 6
  "Size of header/mode-line shadow gradient in pixels (1-20)."
  :type '(integer :tag "Size (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-header-shadow)
                    (boundp 'neomacs-header-shadow)
                    neomacs-header-shadow)
           (neomacs-set-header-shadow t
            (if (boundp 'neomacs-header-shadow-intensity)
                neomacs-header-shadow-intensity nil)
            val))))

;; --- Line insertion/deletion animation ---
(declare-function neomacs-set-line-animation "neomacsterm.c"
  (&optional enabled duration-ms))

(defcustom neomacs-line-animation nil
  "Enable smooth line insertion/deletion animation.
Non-nil animates lines sliding into position when text is
inserted or deleted, creating a smoother editing experience."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-line-animation)
           (neomacs-set-line-animation
            val
            (if (boundp 'neomacs-line-animation-duration)
                neomacs-line-animation-duration nil)))))

(defcustom neomacs-line-animation-duration 150
  "Duration of line animation in milliseconds (50-500)."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-line-animation)
                    (boundp 'neomacs-line-animation)
                    neomacs-line-animation)
           (neomacs-set-line-animation t val))))

;; --- Vignette (edge darkening) ---
(declare-function neomacs-set-vignette "neomacsterm.c"
  (&optional enabled intensity radius))

(defcustom neomacs-vignette nil
  "Enable vignette effect (darken frame edges).
Non-nil draws a gradient overlay that darkens the edges of the
frame, creating a subtle depth/focus effect."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-vignette)
           (neomacs-set-vignette
            val
            (if (boundp 'neomacs-vignette-intensity)
                neomacs-vignette-intensity nil)
            (if (boundp 'neomacs-vignette-radius)
                neomacs-vignette-radius nil)))))

(defcustom neomacs-vignette-intensity 50
  "Intensity of vignette darkening effect (0-100)."
  :type '(integer :tag "Intensity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-vignette)
                    (boundp 'neomacs-vignette)
                    neomacs-vignette)
           (neomacs-set-vignette t val
            (if (boundp 'neomacs-vignette-radius)
                neomacs-vignette-radius nil)))))

(defcustom neomacs-vignette-radius 80
  "Radius in pixels of vignette gradient from each edge (10-200)."
  :type '(integer :tag "Radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-vignette)
                    (boundp 'neomacs-vignette)
                    neomacs-vignette)
           (neomacs-set-vignette t
            (if (boundp 'neomacs-vignette-intensity)
                neomacs-vignette-intensity nil)
            val))))

;; Provide the feature
(provide 'neomacs-win)
(provide 'term/neomacs-win)

;;; neomacs-win.el ends here
