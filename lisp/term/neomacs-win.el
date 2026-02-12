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

;;; Rust display engine

(declare-function neomacs-set-rust-display "neomacsterm.c" (enable))

(defcustom neomacs-rust-display-engine t
  "Use the Rust layout engine instead of the legacy C display engine.
The Rust engine reads buffer data directly and produces the glyph buffer,
bypassing xdisp.c's matrix extraction.  Set to nil to fall back to the
legacy C engine if you encounter display issues."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-rust-display)
           (neomacs-set-rust-display val))))

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

;;; Font ligatures

(declare-function neomacs-set-ligatures-enabled "neomacsterm.c" (enabled))

(defcustom neomacs-ligatures-enabled nil
  "Enable font ligature support.
When non-nil, the layout engine groups same-face character runs so that
HarfBuzz can perform ligature substitution.  This makes programming
ligature fonts like JetBrains Mono and Fira Code render connected glyphs
for sequences like ->, =>, !=, == etc."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-ligatures-enabled)
           (neomacs-set-ligatures-enabled val))))

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

;; --- Window content shadow ---
(declare-function neomacs-set-window-content-shadow "neomacsterm.c"
  (&optional enabled size opacity))

(defcustom neomacs-window-content-shadow nil
  "Enable window content shadow/depth between split panes.
Non-nil renders inner shadows at window edges when multiple windows
are visible, creating a depth/raised pane illusion."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-window-content-shadow)
           (neomacs-set-window-content-shadow val
            (if (boundp 'neomacs-window-content-shadow-size)
                neomacs-window-content-shadow-size nil)
            (if (boundp 'neomacs-window-content-shadow-opacity)
                neomacs-window-content-shadow-opacity nil)))))

(defcustom neomacs-window-content-shadow-size 6
  "Shadow spread size in pixels."
  :type '(integer :tag "Size (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-content-shadow)
                    (boundp 'neomacs-window-content-shadow)
                    neomacs-window-content-shadow)
           (neomacs-set-window-content-shadow t val
            (if (boundp 'neomacs-window-content-shadow-opacity)
                neomacs-window-content-shadow-opacity nil)))))

(defcustom neomacs-window-content-shadow-opacity 15
  "Shadow opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-content-shadow)
                    (boundp 'neomacs-window-content-shadow)
                    neomacs-window-content-shadow)
           (neomacs-set-window-content-shadow t
            (if (boundp 'neomacs-window-content-shadow-size)
                neomacs-window-content-shadow-size nil)
            val))))

;; --- Click halo ---

(declare-function neomacs-set-click-halo "neomacsterm.c"
  (&optional enabled color duration-ms max-radius))

(defcustom neomacs-click-halo nil
  "Enable cursor click halo effect.
Non-nil draws an expanding circular halo animation at the mouse
position when a button is clicked."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-click-halo)
           (neomacs-set-click-halo val
            (if (boundp 'neomacs-click-halo-color)
                neomacs-click-halo-color nil)
            (if (boundp 'neomacs-click-halo-duration)
                neomacs-click-halo-duration nil)
            (if (boundp 'neomacs-click-halo-radius)
                neomacs-click-halo-radius nil)))))

(defcustom neomacs-click-halo-color "#6699FF"
  "Click halo color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-click-halo)
                    (boundp 'neomacs-click-halo)
                    neomacs-click-halo)
           (neomacs-set-click-halo t val
            (if (boundp 'neomacs-click-halo-duration)
                neomacs-click-halo-duration nil)
            (if (boundp 'neomacs-click-halo-radius)
                neomacs-click-halo-radius nil)))))

(defcustom neomacs-click-halo-duration 300
  "Click halo animation duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-click-halo)
                    (boundp 'neomacs-click-halo)
                    neomacs-click-halo)
           (neomacs-set-click-halo t
            (if (boundp 'neomacs-click-halo-color)
                neomacs-click-halo-color nil)
            val
            (if (boundp 'neomacs-click-halo-radius)
                neomacs-click-halo-radius nil)))))

(defcustom neomacs-click-halo-radius 30
  "Maximum click halo radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-click-halo)
                    (boundp 'neomacs-click-halo)
                    neomacs-click-halo)
           (neomacs-set-click-halo t
            (if (boundp 'neomacs-click-halo-color)
                neomacs-click-halo-color nil)
            (if (boundp 'neomacs-click-halo-duration)
                neomacs-click-halo-duration nil)
            val))))

;; --- Scroll velocity fade ---

(declare-function neomacs-set-scroll-velocity-fade "neomacsterm.c"
  (&optional enabled max-opacity fade-ms))

(defcustom neomacs-scroll-velocity-fade nil
  "Enable scroll velocity fade overlay.
Non-nil briefly darkens window content during fast scrolling to
provide visual feedback of scroll speed."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scroll-velocity-fade)
           (neomacs-set-scroll-velocity-fade val
            (if (boundp 'neomacs-scroll-velocity-fade-opacity)
                neomacs-scroll-velocity-fade-opacity nil)
            (if (boundp 'neomacs-scroll-velocity-fade-ms)
                neomacs-scroll-velocity-fade-ms nil)))))

(defcustom neomacs-scroll-velocity-fade-opacity 15
  "Maximum fade overlay opacity at peak velocity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-velocity-fade)
                    (boundp 'neomacs-scroll-velocity-fade)
                    neomacs-scroll-velocity-fade)
           (neomacs-set-scroll-velocity-fade t val
            (if (boundp 'neomacs-scroll-velocity-fade-ms)
                neomacs-scroll-velocity-fade-ms nil)))))

(defcustom neomacs-scroll-velocity-fade-ms 300
  "Fade-out duration in milliseconds after scrolling stops."
  :type '(integer :tag "Fade (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scroll-velocity-fade)
                    (boundp 'neomacs-scroll-velocity-fade)
                    neomacs-scroll-velocity-fade)
           (neomacs-set-scroll-velocity-fade t
            (if (boundp 'neomacs-scroll-velocity-fade-opacity)
                neomacs-scroll-velocity-fade-opacity nil)
            val))))

;; --- Mini-buffer completion highlight ---

(declare-function neomacs-set-minibuffer-highlight "neomacsterm.c"
  (&optional enabled color opacity))

(defcustom neomacs-minibuffer-highlight nil
  "Enable mini-buffer completion highlight glow.
Non-nil draws a soft glow overlay around highlighted completion
candidates in the mini-buffer area."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-minibuffer-highlight)
           (neomacs-set-minibuffer-highlight val
            (if (boundp 'neomacs-minibuffer-highlight-color)
                neomacs-minibuffer-highlight-color nil)
            (if (boundp 'neomacs-minibuffer-highlight-opacity)
                neomacs-minibuffer-highlight-opacity nil)))))

(defcustom neomacs-minibuffer-highlight-color "#6699FF"
  "Highlight glow color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-minibuffer-highlight)
                    (boundp 'neomacs-minibuffer-highlight)
                    neomacs-minibuffer-highlight)
           (neomacs-set-minibuffer-highlight t val
            (if (boundp 'neomacs-minibuffer-highlight-opacity)
                neomacs-minibuffer-highlight-opacity nil)))))

(defcustom neomacs-minibuffer-highlight-opacity 25
  "Highlight glow opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-minibuffer-highlight)
                    (boundp 'neomacs-minibuffer-highlight)
                    neomacs-minibuffer-highlight)
           (neomacs-set-minibuffer-highlight t
            (if (boundp 'neomacs-minibuffer-highlight-color)
                neomacs-minibuffer-highlight-color nil)
            val))))

;; --- Resize padding transition ---

(declare-function neomacs-set-resize-padding "neomacsterm.c"
  (&optional enabled duration-ms max-padding))

(defcustom neomacs-resize-padding nil
  "Enable smooth window padding transition on frame resize.
Non-nil draws a temporary visual padding overlay at window edges
during frame resize that eases to zero, creating a smooth transition."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-resize-padding)
           (neomacs-set-resize-padding val
            (if (boundp 'neomacs-resize-padding-duration)
                neomacs-resize-padding-duration nil)
            (if (boundp 'neomacs-resize-padding-max)
                neomacs-resize-padding-max nil)))))

(defcustom neomacs-resize-padding-duration 200
  "Resize padding transition duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-resize-padding)
                    (boundp 'neomacs-resize-padding)
                    neomacs-resize-padding)
           (neomacs-set-resize-padding t val
            (if (boundp 'neomacs-resize-padding-max)
                neomacs-resize-padding-max nil)))))

(defcustom neomacs-resize-padding-max 12
  "Maximum extra padding in pixels at start of resize transition."
  :type '(integer :tag "Max padding (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-resize-padding)
                    (boundp 'neomacs-resize-padding)
                    neomacs-resize-padding)
           (neomacs-set-resize-padding t
            (if (boundp 'neomacs-resize-padding-duration)
                neomacs-resize-padding-duration nil)
            val))))

;; --- Cursor error pulse ---
(declare-function neomacs-set-cursor-error-pulse "neomacsterm.c"
  (&optional enabled color duration-ms))

(defcustom neomacs-cursor-error-pulse nil
  "Enable cursor error pulse animation.
Non-nil makes the cursor briefly flash a color when Emacs rings
the bell (e.g., on errors like end-of-buffer)."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-error-pulse)
           (neomacs-set-cursor-error-pulse val
            (if (boundp 'neomacs-cursor-error-pulse-color)
                neomacs-cursor-error-pulse-color nil)
            (if (boundp 'neomacs-cursor-error-pulse-duration)
                neomacs-cursor-error-pulse-duration nil)))))

(defcustom neomacs-cursor-error-pulse-color "#FF3333"
  "Color for the cursor error pulse (hex RGB string)."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-error-pulse)
                    (boundp 'neomacs-cursor-error-pulse)
                    neomacs-cursor-error-pulse)
           (neomacs-set-cursor-error-pulse t val
            (if (boundp 'neomacs-cursor-error-pulse-duration)
                neomacs-cursor-error-pulse-duration nil)))))

(defcustom neomacs-cursor-error-pulse-duration 250
  "Cursor error pulse duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-error-pulse)
                    (boundp 'neomacs-cursor-error-pulse)
                    neomacs-cursor-error-pulse)
           (neomacs-set-cursor-error-pulse t
            (if (boundp 'neomacs-cursor-error-pulse-color)
                neomacs-cursor-error-pulse-color nil)
            val))))

;; --- Line wrap indicator ---
(declare-function neomacs-set-wrap-indicator "neomacsterm.c"
  (&optional enabled color opacity))

(defcustom neomacs-wrap-indicator nil
  "Enable line wrap indicator overlay.
Non-nil renders a subtle gradient at the right edge of lines that
wrap, providing a visual cue for line wrapping."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-wrap-indicator)
           (neomacs-set-wrap-indicator val
            (if (boundp 'neomacs-wrap-indicator-color)
                neomacs-wrap-indicator-color nil)
            (if (boundp 'neomacs-wrap-indicator-opacity)
                neomacs-wrap-indicator-opacity nil)))))

(defcustom neomacs-wrap-indicator-color "#8099CC"
  "Color for the line wrap indicator (hex RGB string)."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wrap-indicator)
                    (boundp 'neomacs-wrap-indicator)
                    neomacs-wrap-indicator)
           (neomacs-set-wrap-indicator t val
            (if (boundp 'neomacs-wrap-indicator-opacity)
                neomacs-wrap-indicator-opacity nil)))))

(defcustom neomacs-wrap-indicator-opacity 30
  "Opacity for the line wrap indicator (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wrap-indicator)
                    (boundp 'neomacs-wrap-indicator)
                    neomacs-wrap-indicator)
           (neomacs-set-wrap-indicator t
            (if (boundp 'neomacs-wrap-indicator-color)
                neomacs-wrap-indicator-color nil)
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

;; --- Cursor crosshair guide lines ---
(declare-function neomacs-set-cursor-crosshair "neomacsterm.c"
  (&optional enabled color opacity))

(defcustom neomacs-cursor-crosshair nil
  "Enable cursor crosshair guide lines.
Non-nil draws thin semi-transparent horizontal and vertical lines
from the cursor position across the selected window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-crosshair)
           (neomacs-set-cursor-crosshair val
            (if (boundp 'neomacs-cursor-crosshair-color)
                neomacs-cursor-crosshair-color nil)
            (if (boundp 'neomacs-cursor-crosshair-opacity)
                neomacs-cursor-crosshair-opacity nil)))))

(defcustom neomacs-cursor-crosshair-color "#808080"
  "Crosshair guide line color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-crosshair)
                    (boundp 'neomacs-cursor-crosshair)
                    neomacs-cursor-crosshair)
           (neomacs-set-cursor-crosshair t val
            (if (boundp 'neomacs-cursor-crosshair-opacity)
                neomacs-cursor-crosshair-opacity nil)))))

(defcustom neomacs-cursor-crosshair-opacity 15
  "Crosshair guide line opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-crosshair)
                    (boundp 'neomacs-cursor-crosshair)
                    neomacs-cursor-crosshair)
           (neomacs-set-cursor-crosshair t
            (if (boundp 'neomacs-cursor-crosshair-color)
                neomacs-cursor-crosshair-color nil)
            val))))

;; --- Buffer modified border indicator ---
(declare-function neomacs-set-modified-indicator "neomacsterm.c"
  (&optional enabled color width opacity))

(defcustom neomacs-modified-indicator nil
  "Enable buffer modified border indicator.
Non-nil draws a colored strip along the left edge of windows
whose buffers have unsaved modifications."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-modified-indicator)
           (neomacs-set-modified-indicator val
            (if (boundp 'neomacs-modified-indicator-color)
                neomacs-modified-indicator-color nil)
            (if (boundp 'neomacs-modified-indicator-width)
                neomacs-modified-indicator-width nil)
            (if (boundp 'neomacs-modified-indicator-opacity)
                neomacs-modified-indicator-opacity nil)))))

(defcustom neomacs-modified-indicator-color "#FF9933"
  "Modified indicator strip color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-modified-indicator)
                    (boundp 'neomacs-modified-indicator)
                    neomacs-modified-indicator)
           (neomacs-set-modified-indicator t val
            (if (boundp 'neomacs-modified-indicator-width)
                neomacs-modified-indicator-width nil)
            (if (boundp 'neomacs-modified-indicator-opacity)
                neomacs-modified-indicator-opacity nil)))))

(defcustom neomacs-modified-indicator-width 3
  "Modified indicator strip width in pixels."
  :type '(integer :tag "Width (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-modified-indicator)
                    (boundp 'neomacs-modified-indicator)
                    neomacs-modified-indicator)
           (neomacs-set-modified-indicator t
            (if (boundp 'neomacs-modified-indicator-color)
                neomacs-modified-indicator-color nil)
            val
            (if (boundp 'neomacs-modified-indicator-opacity)
                neomacs-modified-indicator-opacity nil)))))

(defcustom neomacs-modified-indicator-opacity 80
  "Modified indicator strip opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-modified-indicator)
                    (boundp 'neomacs-modified-indicator)
                    neomacs-modified-indicator)
           (neomacs-set-modified-indicator t
            (if (boundp 'neomacs-modified-indicator-color)
                neomacs-modified-indicator-color nil)
            (if (boundp 'neomacs-modified-indicator-width)
                neomacs-modified-indicator-width nil)
            val))))

;; --- Inactive window stained glass effect ---
(declare-function neomacs-set-stained-glass "neomacsterm.c"
  (&optional enabled opacity saturation))

(defcustom neomacs-stained-glass nil
  "Enable inactive window stained glass effect.
Non-nil tints inactive windows with a unique color derived from each
buffer's identity, creating a stained-glass appearance."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-stained-glass)
           (neomacs-set-stained-glass val
            (if (boundp 'neomacs-stained-glass-opacity)
                neomacs-stained-glass-opacity nil)
            (if (boundp 'neomacs-stained-glass-saturation)
                neomacs-stained-glass-saturation nil)))))

(defcustom neomacs-stained-glass-opacity 8
  "Stained glass tint opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-stained-glass)
                    (boundp 'neomacs-stained-glass)
                    neomacs-stained-glass)
           (neomacs-set-stained-glass t val
            (if (boundp 'neomacs-stained-glass-saturation)
                neomacs-stained-glass-saturation nil)))))

(defcustom neomacs-stained-glass-saturation 60
  "Stained glass color saturation (0-100)."
  :type '(integer :tag "Saturation (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-stained-glass)
                    (boundp 'neomacs-stained-glass)
                    neomacs-stained-glass)
           (neomacs-set-stained-glass t
            (if (boundp 'neomacs-stained-glass-opacity)
                neomacs-stained-glass-opacity nil)
            val))))

;; --- Focus gradient border ---
(declare-function neomacs-set-focus-gradient-border "neomacsterm.c"
  (&optional enabled top-color bottom-color width opacity))

(defcustom neomacs-focus-gradient-border nil
  "Enable focused window gradient border.
Non-nil renders a vertical gradient border on the active window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-focus-gradient-border)
           (neomacs-set-focus-gradient-border val
            (if (boundp 'neomacs-focus-gradient-border-top-color)
                neomacs-focus-gradient-border-top-color nil)
            (if (boundp 'neomacs-focus-gradient-border-bottom-color)
                neomacs-focus-gradient-border-bottom-color nil)
            (if (boundp 'neomacs-focus-gradient-border-width)
                neomacs-focus-gradient-border-width nil)
            (if (boundp 'neomacs-focus-gradient-border-opacity)
                neomacs-focus-gradient-border-opacity nil)))))

(defcustom neomacs-focus-gradient-border-top-color "#4D99FF"
  "Gradient border top color."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-gradient-border)
                    (boundp 'neomacs-focus-gradient-border)
                    neomacs-focus-gradient-border)
           (neomacs-set-focus-gradient-border t val
            (if (boundp 'neomacs-focus-gradient-border-bottom-color)
                neomacs-focus-gradient-border-bottom-color nil)
            (if (boundp 'neomacs-focus-gradient-border-width)
                neomacs-focus-gradient-border-width nil)
            (if (boundp 'neomacs-focus-gradient-border-opacity)
                neomacs-focus-gradient-border-opacity nil)))))

(defcustom neomacs-focus-gradient-border-bottom-color "#994DFF"
  "Gradient border bottom color."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-gradient-border)
                    (boundp 'neomacs-focus-gradient-border)
                    neomacs-focus-gradient-border)
           (neomacs-set-focus-gradient-border t
            (if (boundp 'neomacs-focus-gradient-border-top-color)
                neomacs-focus-gradient-border-top-color nil)
            val
            (if (boundp 'neomacs-focus-gradient-border-width)
                neomacs-focus-gradient-border-width nil)
            (if (boundp 'neomacs-focus-gradient-border-opacity)
                neomacs-focus-gradient-border-opacity nil)))))

(defcustom neomacs-focus-gradient-border-width 2
  "Gradient border width in pixels."
  :type '(integer :tag "Width (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-gradient-border)
                    (boundp 'neomacs-focus-gradient-border)
                    neomacs-focus-gradient-border)
           (neomacs-set-focus-gradient-border t
            (if (boundp 'neomacs-focus-gradient-border-top-color)
                neomacs-focus-gradient-border-top-color nil)
            (if (boundp 'neomacs-focus-gradient-border-bottom-color)
                neomacs-focus-gradient-border-bottom-color nil)
            val
            (if (boundp 'neomacs-focus-gradient-border-opacity)
                neomacs-focus-gradient-border-opacity nil)))))

(defcustom neomacs-focus-gradient-border-opacity 60
  "Gradient border opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-focus-gradient-border)
                    (boundp 'neomacs-focus-gradient-border)
                    neomacs-focus-gradient-border)
           (neomacs-set-focus-gradient-border t
            (if (boundp 'neomacs-focus-gradient-border-top-color)
                neomacs-focus-gradient-border-top-color nil)
            (if (boundp 'neomacs-focus-gradient-border-bottom-color)
                neomacs-focus-gradient-border-bottom-color nil)
            (if (boundp 'neomacs-focus-gradient-border-width)
                neomacs-focus-gradient-border-width nil)
            val))))

;; --- Cursor magnetism effect ---
(declare-function neomacs-set-cursor-magnetism "neomacsterm.c"
  (&optional enabled color ring-count duration-ms opacity))

(defcustom neomacs-cursor-magnetism nil
  "Enable cursor magnetism effect on jump.
Non-nil renders collapsing rings when cursor jumps far."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-magnetism)
           (neomacs-set-cursor-magnetism val
            (if (boundp 'neomacs-cursor-magnetism-color)
                neomacs-cursor-magnetism-color nil)
            (if (boundp 'neomacs-cursor-magnetism-ring-count)
                neomacs-cursor-magnetism-ring-count nil)
            (if (boundp 'neomacs-cursor-magnetism-duration-ms)
                neomacs-cursor-magnetism-duration-ms nil)
            (if (boundp 'neomacs-cursor-magnetism-opacity)
                neomacs-cursor-magnetism-opacity nil)))))

(defcustom neomacs-cursor-magnetism-color "#66B3FF"
  "Magnetism ring color."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-magnetism)
                    (boundp 'neomacs-cursor-magnetism)
                    neomacs-cursor-magnetism)
           (neomacs-set-cursor-magnetism t val
            (if (boundp 'neomacs-cursor-magnetism-ring-count)
                neomacs-cursor-magnetism-ring-count nil)
            (if (boundp 'neomacs-cursor-magnetism-duration-ms)
                neomacs-cursor-magnetism-duration-ms nil)
            (if (boundp 'neomacs-cursor-magnetism-opacity)
                neomacs-cursor-magnetism-opacity nil)))))

(defcustom neomacs-cursor-magnetism-ring-count 3
  "Number of concentric rings."
  :type '(integer :tag "Ring count")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-magnetism)
                    (boundp 'neomacs-cursor-magnetism)
                    neomacs-cursor-magnetism)
           (neomacs-set-cursor-magnetism t
            (if (boundp 'neomacs-cursor-magnetism-color)
                neomacs-cursor-magnetism-color nil)
            val
            (if (boundp 'neomacs-cursor-magnetism-duration-ms)
                neomacs-cursor-magnetism-duration-ms nil)
            (if (boundp 'neomacs-cursor-magnetism-opacity)
                neomacs-cursor-magnetism-opacity nil)))))

(defcustom neomacs-cursor-magnetism-duration-ms 300
  "Magnetism animation duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-magnetism)
                    (boundp 'neomacs-cursor-magnetism)
                    neomacs-cursor-magnetism)
           (neomacs-set-cursor-magnetism t
            (if (boundp 'neomacs-cursor-magnetism-color)
                neomacs-cursor-magnetism-color nil)
            (if (boundp 'neomacs-cursor-magnetism-ring-count)
                neomacs-cursor-magnetism-ring-count nil)
            val
            (if (boundp 'neomacs-cursor-magnetism-opacity)
                neomacs-cursor-magnetism-opacity nil)))))

(defcustom neomacs-cursor-magnetism-opacity 50
  "Magnetism effect max opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-magnetism)
                    (boundp 'neomacs-cursor-magnetism)
                    neomacs-cursor-magnetism)
           (neomacs-set-cursor-magnetism t
            (if (boundp 'neomacs-cursor-magnetism-color)
                neomacs-cursor-magnetism-color nil)
            (if (boundp 'neomacs-cursor-magnetism-ring-count)
                neomacs-cursor-magnetism-ring-count nil)
            (if (boundp 'neomacs-cursor-magnetism-duration-ms)
                neomacs-cursor-magnetism-duration-ms nil)
            val))))

;; --- Window depth shadow layers ---
(declare-function neomacs-set-depth-shadow "neomacsterm.c"
  (&optional enabled layers offset color opacity))

(defcustom neomacs-depth-shadow nil
  "Enable window depth shadow layers.
Non-nil renders shadow layers for a 3D stacked paper effect."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-depth-shadow)
           (neomacs-set-depth-shadow val
            (if (boundp 'neomacs-depth-shadow-layers)
                neomacs-depth-shadow-layers nil)
            (if (boundp 'neomacs-depth-shadow-offset)
                neomacs-depth-shadow-offset nil)
            (if (boundp 'neomacs-depth-shadow-color)
                neomacs-depth-shadow-color nil)
            (if (boundp 'neomacs-depth-shadow-opacity)
                neomacs-depth-shadow-opacity nil)))))

(defcustom neomacs-depth-shadow-layers 3
  "Number of shadow layers."
  :type '(integer :tag "Layers")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-depth-shadow)
                    (boundp 'neomacs-depth-shadow)
                    neomacs-depth-shadow)
           (neomacs-set-depth-shadow t val
            (if (boundp 'neomacs-depth-shadow-offset)
                neomacs-depth-shadow-offset nil)
            (if (boundp 'neomacs-depth-shadow-color)
                neomacs-depth-shadow-color nil)
            (if (boundp 'neomacs-depth-shadow-opacity)
                neomacs-depth-shadow-opacity nil)))))

(defcustom neomacs-depth-shadow-offset 2
  "Pixels offset per shadow layer."
  :type '(integer :tag "Offset (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-depth-shadow)
                    (boundp 'neomacs-depth-shadow)
                    neomacs-depth-shadow)
           (neomacs-set-depth-shadow t
            (if (boundp 'neomacs-depth-shadow-layers)
                neomacs-depth-shadow-layers nil)
            val
            (if (boundp 'neomacs-depth-shadow-color)
                neomacs-depth-shadow-color nil)
            (if (boundp 'neomacs-depth-shadow-opacity)
                neomacs-depth-shadow-opacity nil)))))

(defcustom neomacs-depth-shadow-color "#000000"
  "Shadow color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-depth-shadow)
                    (boundp 'neomacs-depth-shadow)
                    neomacs-depth-shadow)
           (neomacs-set-depth-shadow t
            (if (boundp 'neomacs-depth-shadow-layers)
                neomacs-depth-shadow-layers nil)
            (if (boundp 'neomacs-depth-shadow-offset)
                neomacs-depth-shadow-offset nil)
            val
            (if (boundp 'neomacs-depth-shadow-opacity)
                neomacs-depth-shadow-opacity nil)))))

(defcustom neomacs-depth-shadow-opacity 15
  "Shadow opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-depth-shadow)
                    (boundp 'neomacs-depth-shadow)
                    neomacs-depth-shadow)
           (neomacs-set-depth-shadow t
            (if (boundp 'neomacs-depth-shadow-layers)
                neomacs-depth-shadow-layers nil)
            (if (boundp 'neomacs-depth-shadow-offset)
                neomacs-depth-shadow-offset nil)
            (if (boundp 'neomacs-depth-shadow-color)
                neomacs-depth-shadow-color nil)
            val))))

;; --- Mode-line gradient background ---
(declare-function neomacs-set-mode-line-gradient "neomacsterm.c"
  (&optional enabled left-color right-color opacity))

(defcustom neomacs-mode-line-gradient nil
  "Enable mode-line gradient background.
Non-nil renders a horizontal gradient across the mode-line."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-mode-line-gradient)
           (neomacs-set-mode-line-gradient val
            (if (boundp 'neomacs-mode-line-gradient-left-color)
                neomacs-mode-line-gradient-left-color nil)
            (if (boundp 'neomacs-mode-line-gradient-right-color)
                neomacs-mode-line-gradient-right-color nil)
            (if (boundp 'neomacs-mode-line-gradient-opacity)
                neomacs-mode-line-gradient-opacity nil)))))

(defcustom neomacs-mode-line-gradient-left-color "#334D80"
  "Mode-line gradient left color."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-mode-line-gradient)
                    (boundp 'neomacs-mode-line-gradient)
                    neomacs-mode-line-gradient)
           (neomacs-set-mode-line-gradient t val
            (if (boundp 'neomacs-mode-line-gradient-right-color)
                neomacs-mode-line-gradient-right-color nil)
            (if (boundp 'neomacs-mode-line-gradient-opacity)
                neomacs-mode-line-gradient-opacity nil)))))

(defcustom neomacs-mode-line-gradient-right-color "#804D33"
  "Mode-line gradient right color."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-mode-line-gradient)
                    (boundp 'neomacs-mode-line-gradient)
                    neomacs-mode-line-gradient)
           (neomacs-set-mode-line-gradient t
            (if (boundp 'neomacs-mode-line-gradient-left-color)
                neomacs-mode-line-gradient-left-color nil)
            val
            (if (boundp 'neomacs-mode-line-gradient-opacity)
                neomacs-mode-line-gradient-opacity nil)))))

(defcustom neomacs-mode-line-gradient-opacity 30
  "Mode-line gradient opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-mode-line-gradient)
                    (boundp 'neomacs-mode-line-gradient)
                    neomacs-mode-line-gradient)
           (neomacs-set-mode-line-gradient t
            (if (boundp 'neomacs-mode-line-gradient-left-color)
                neomacs-mode-line-gradient-left-color nil)
            (if (boundp 'neomacs-mode-line-gradient-right-color)
                neomacs-mode-line-gradient-right-color nil)
            val))))

;; --- Window corner fold effect ---
(declare-function neomacs-set-corner-fold "neomacsterm.c"
  (&optional enabled size color opacity))

(defcustom neomacs-corner-fold nil
  "Enable window corner fold effect.
Non-nil renders a triangular page fold in the top-right corner."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-corner-fold)
           (neomacs-set-corner-fold val
            (if (boundp 'neomacs-corner-fold-size)
                neomacs-corner-fold-size nil)
            (if (boundp 'neomacs-corner-fold-color)
                neomacs-corner-fold-color nil)
            (if (boundp 'neomacs-corner-fold-opacity)
                neomacs-corner-fold-opacity nil)))))

(defcustom neomacs-corner-fold-size 20
  "Corner fold size in pixels."
  :type '(integer :tag "Size (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-corner-fold)
                    (boundp 'neomacs-corner-fold)
                    neomacs-corner-fold)
           (neomacs-set-corner-fold t val
            (if (boundp 'neomacs-corner-fold-color)
                neomacs-corner-fold-color nil)
            (if (boundp 'neomacs-corner-fold-opacity)
                neomacs-corner-fold-opacity nil)))))

(defcustom neomacs-corner-fold-color "#996633"
  "Corner fold accent color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-corner-fold)
                    (boundp 'neomacs-corner-fold)
                    neomacs-corner-fold)
           (neomacs-set-corner-fold t
            (if (boundp 'neomacs-corner-fold-size)
                neomacs-corner-fold-size nil)
            val
            (if (boundp 'neomacs-corner-fold-opacity)
                neomacs-corner-fold-opacity nil)))))

(defcustom neomacs-corner-fold-opacity 50
  "Corner fold opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-corner-fold)
                    (boundp 'neomacs-corner-fold)
                    neomacs-corner-fold)
           (neomacs-set-corner-fold t
            (if (boundp 'neomacs-corner-fold-size)
                neomacs-corner-fold-size nil)
            (if (boundp 'neomacs-corner-fold-color)
                neomacs-corner-fold-color nil)
            val))))

;; --- Frosted window border effect ---
(declare-function neomacs-set-frosted-border "neomacsterm.c"
  (&optional enabled width opacity color))

(defcustom neomacs-frosted-border nil
  "Enable frosted window border effect.
Non-nil renders multi-layered semi-transparent borders."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-frosted-border)
           (neomacs-set-frosted-border val
            (if (boundp 'neomacs-frosted-border-width)
                neomacs-frosted-border-width nil)
            (if (boundp 'neomacs-frosted-border-opacity)
                neomacs-frosted-border-opacity nil)
            (if (boundp 'neomacs-frosted-border-color)
                neomacs-frosted-border-color nil)))))

(defcustom neomacs-frosted-border-width 4
  "Frosted border width in pixels."
  :type '(integer :tag "Width (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frosted-border)
                    (boundp 'neomacs-frosted-border)
                    neomacs-frosted-border)
           (neomacs-set-frosted-border t val
            (if (boundp 'neomacs-frosted-border-opacity)
                neomacs-frosted-border-opacity nil)
            (if (boundp 'neomacs-frosted-border-color)
                neomacs-frosted-border-color nil)))))

(defcustom neomacs-frosted-border-opacity 15
  "Frosted border opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frosted-border)
                    (boundp 'neomacs-frosted-border)
                    neomacs-frosted-border)
           (neomacs-set-frosted-border t
            (if (boundp 'neomacs-frosted-border-width)
                neomacs-frosted-border-width nil)
            val
            (if (boundp 'neomacs-frosted-border-color)
                neomacs-frosted-border-color nil)))))

(defcustom neomacs-frosted-border-color "#FFFFFF"
  "Frosted border color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frosted-border)
                    (boundp 'neomacs-frosted-border)
                    neomacs-frosted-border)
           (neomacs-set-frosted-border t
            (if (boundp 'neomacs-frosted-border-width)
                neomacs-frosted-border-width nil)
            (if (boundp 'neomacs-frosted-border-opacity)
                neomacs-frosted-border-opacity nil)
            val))))

;; --- Line number pulse on cursor line ---
(declare-function neomacs-set-line-number-pulse "neomacsterm.c"
  (&optional enabled color intensity cycle-ms))

(defcustom neomacs-line-number-pulse nil
  "Enable line number pulse on cursor line.
Non-nil renders a pulsing glow on the line number gutter."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-line-number-pulse)
           (neomacs-set-line-number-pulse val
            (if (boundp 'neomacs-line-number-pulse-color)
                neomacs-line-number-pulse-color nil)
            (if (boundp 'neomacs-line-number-pulse-intensity)
                neomacs-line-number-pulse-intensity nil)
            (if (boundp 'neomacs-line-number-pulse-cycle-ms)
                neomacs-line-number-pulse-cycle-ms nil)))))

(defcustom neomacs-line-number-pulse-color "#6699FF"
  "Line number pulse color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-line-number-pulse)
                    (boundp 'neomacs-line-number-pulse)
                    neomacs-line-number-pulse)
           (neomacs-set-line-number-pulse t val
            (if (boundp 'neomacs-line-number-pulse-intensity)
                neomacs-line-number-pulse-intensity nil)
            (if (boundp 'neomacs-line-number-pulse-cycle-ms)
                neomacs-line-number-pulse-cycle-ms nil)))))

(defcustom neomacs-line-number-pulse-intensity 30
  "Line number pulse intensity (0-100)."
  :type '(integer :tag "Intensity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-line-number-pulse)
                    (boundp 'neomacs-line-number-pulse)
                    neomacs-line-number-pulse)
           (neomacs-set-line-number-pulse t
            (if (boundp 'neomacs-line-number-pulse-color)
                neomacs-line-number-pulse-color nil)
            val
            (if (boundp 'neomacs-line-number-pulse-cycle-ms)
                neomacs-line-number-pulse-cycle-ms nil)))))

(defcustom neomacs-line-number-pulse-cycle-ms 2000
  "Line number pulse cycle duration in milliseconds."
  :type '(integer :tag "Cycle (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-line-number-pulse)
                    (boundp 'neomacs-line-number-pulse)
                    neomacs-line-number-pulse)
           (neomacs-set-line-number-pulse t
            (if (boundp 'neomacs-line-number-pulse-color)
                neomacs-line-number-pulse-color nil)
            (if (boundp 'neomacs-line-number-pulse-intensity)
                neomacs-line-number-pulse-intensity nil)
            val))))

;; --- Window breathing border animation ---
(declare-function neomacs-set-breathing-border "neomacsterm.c"
  (&optional enabled color min-opacity max-opacity cycle-ms))

(defcustom neomacs-breathing-border nil
  "Enable window breathing border animation.
Non-nil animates window borders with sinusoidal opacity."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-breathing-border)
           (neomacs-set-breathing-border val
            (if (boundp 'neomacs-breathing-border-color)
                neomacs-breathing-border-color nil)
            (if (boundp 'neomacs-breathing-border-min-opacity)
                neomacs-breathing-border-min-opacity nil)
            (if (boundp 'neomacs-breathing-border-max-opacity)
                neomacs-breathing-border-max-opacity nil)
            (if (boundp 'neomacs-breathing-border-cycle-ms)
                neomacs-breathing-border-cycle-ms nil)))))

(defcustom neomacs-breathing-border-color "#808080"
  "Breathing border color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-breathing-border)
                    (boundp 'neomacs-breathing-border)
                    neomacs-breathing-border)
           (neomacs-set-breathing-border t val
            (if (boundp 'neomacs-breathing-border-min-opacity)
                neomacs-breathing-border-min-opacity nil)
            (if (boundp 'neomacs-breathing-border-max-opacity)
                neomacs-breathing-border-max-opacity nil)
            (if (boundp 'neomacs-breathing-border-cycle-ms)
                neomacs-breathing-border-cycle-ms nil)))))

(defcustom neomacs-breathing-border-min-opacity 5
  "Breathing border minimum opacity (0-100)."
  :type '(integer :tag "Min Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-breathing-border)
                    (boundp 'neomacs-breathing-border)
                    neomacs-breathing-border)
           (neomacs-set-breathing-border t
            (if (boundp 'neomacs-breathing-border-color)
                neomacs-breathing-border-color nil)
            val
            (if (boundp 'neomacs-breathing-border-max-opacity)
                neomacs-breathing-border-max-opacity nil)
            (if (boundp 'neomacs-breathing-border-cycle-ms)
                neomacs-breathing-border-cycle-ms nil)))))

(defcustom neomacs-breathing-border-max-opacity 30
  "Breathing border maximum opacity (0-100)."
  :type '(integer :tag "Max Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-breathing-border)
                    (boundp 'neomacs-breathing-border)
                    neomacs-breathing-border)
           (neomacs-set-breathing-border t
            (if (boundp 'neomacs-breathing-border-color)
                neomacs-breathing-border-color nil)
            (if (boundp 'neomacs-breathing-border-min-opacity)
                neomacs-breathing-border-min-opacity nil)
            val
            (if (boundp 'neomacs-breathing-border-cycle-ms)
                neomacs-breathing-border-cycle-ms nil)))))

(defcustom neomacs-breathing-border-cycle-ms 3000
  "Breathing border full cycle duration in milliseconds."
  :type '(integer :tag "Cycle (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-breathing-border)
                    (boundp 'neomacs-breathing-border)
                    neomacs-breathing-border)
           (neomacs-set-breathing-border t
            (if (boundp 'neomacs-breathing-border-color)
                neomacs-breathing-border-color nil)
            (if (boundp 'neomacs-breathing-border-min-opacity)
                neomacs-breathing-border-min-opacity nil)
            (if (boundp 'neomacs-breathing-border-max-opacity)
                neomacs-breathing-border-max-opacity nil)
            val))))

;; --- Window scanline (CRT) effect ---
(declare-function neomacs-set-scanlines "neomacsterm.c"
  (&optional enabled spacing opacity color))

(defcustom neomacs-scanlines nil
  "Enable window scanline (CRT) effect.
Non-nil overlays subtle horizontal scanlines across the screen."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-scanlines)
           (neomacs-set-scanlines val
            (if (boundp 'neomacs-scanlines-spacing)
                neomacs-scanlines-spacing nil)
            (if (boundp 'neomacs-scanlines-opacity)
                neomacs-scanlines-opacity nil)
            (if (boundp 'neomacs-scanlines-color)
                neomacs-scanlines-color nil)))))

(defcustom neomacs-scanlines-spacing 2
  "Scanline spacing in pixels."
  :type '(integer :tag "Spacing (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scanlines)
                    (boundp 'neomacs-scanlines)
                    neomacs-scanlines)
           (neomacs-set-scanlines t val
            (if (boundp 'neomacs-scanlines-opacity)
                neomacs-scanlines-opacity nil)
            (if (boundp 'neomacs-scanlines-color)
                neomacs-scanlines-color nil)))))

(defcustom neomacs-scanlines-opacity 8
  "Scanline opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scanlines)
                    (boundp 'neomacs-scanlines)
                    neomacs-scanlines)
           (neomacs-set-scanlines t
            (if (boundp 'neomacs-scanlines-spacing)
                neomacs-scanlines-spacing nil)
            val
            (if (boundp 'neomacs-scanlines-color)
                neomacs-scanlines-color nil)))))

(defcustom neomacs-scanlines-color "#000000"
  "Scanline color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-scanlines)
                    (boundp 'neomacs-scanlines)
                    neomacs-scanlines)
           (neomacs-set-scanlines t
            (if (boundp 'neomacs-scanlines-spacing)
                neomacs-scanlines-spacing nil)
            (if (boundp 'neomacs-scanlines-opacity)
                neomacs-scanlines-opacity nil)
            val))))

;; --- Cursor comet tail effect ---
(declare-function neomacs-set-cursor-comet "neomacsterm.c"
  (&optional enabled trail-length fade-ms color opacity))

(defcustom neomacs-cursor-comet nil
  "Enable cursor comet tail effect.
Non-nil leaves a fading trail of ghost cursor shapes along the path."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-comet)
           (neomacs-set-cursor-comet val
            (if (boundp 'neomacs-cursor-comet-trail-length)
                neomacs-cursor-comet-trail-length nil)
            (if (boundp 'neomacs-cursor-comet-fade-ms)
                neomacs-cursor-comet-fade-ms nil)
            (if (boundp 'neomacs-cursor-comet-color)
                neomacs-cursor-comet-color nil)
            (if (boundp 'neomacs-cursor-comet-opacity)
                neomacs-cursor-comet-opacity nil)))))

(defcustom neomacs-cursor-comet-trail-length 5
  "Number of ghost cursor copies in the comet trail."
  :type '(integer :tag "Trail length")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-comet)
                    (boundp 'neomacs-cursor-comet)
                    neomacs-cursor-comet)
           (neomacs-set-cursor-comet t val
            (if (boundp 'neomacs-cursor-comet-fade-ms)
                neomacs-cursor-comet-fade-ms nil)
            (if (boundp 'neomacs-cursor-comet-color)
                neomacs-cursor-comet-color nil)
            (if (boundp 'neomacs-cursor-comet-opacity)
                neomacs-cursor-comet-opacity nil)))))

(defcustom neomacs-cursor-comet-fade-ms 300
  "Comet trail fade-out duration in milliseconds."
  :type '(integer :tag "Fade (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-comet)
                    (boundp 'neomacs-cursor-comet)
                    neomacs-cursor-comet)
           (neomacs-set-cursor-comet t
            (if (boundp 'neomacs-cursor-comet-trail-length)
                neomacs-cursor-comet-trail-length nil)
            val
            (if (boundp 'neomacs-cursor-comet-color)
                neomacs-cursor-comet-color nil)
            (if (boundp 'neomacs-cursor-comet-opacity)
                neomacs-cursor-comet-opacity nil)))))

(defcustom neomacs-cursor-comet-color "#80B3FF"
  "Comet trail color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-comet)
                    (boundp 'neomacs-cursor-comet)
                    neomacs-cursor-comet)
           (neomacs-set-cursor-comet t
            (if (boundp 'neomacs-cursor-comet-trail-length)
                neomacs-cursor-comet-trail-length nil)
            (if (boundp 'neomacs-cursor-comet-fade-ms)
                neomacs-cursor-comet-fade-ms nil)
            val
            (if (boundp 'neomacs-cursor-comet-opacity)
                neomacs-cursor-comet-opacity nil)))))

(defcustom neomacs-cursor-comet-opacity 60
  "Comet trail max opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-comet)
                    (boundp 'neomacs-cursor-comet)
                    neomacs-cursor-comet)
           (neomacs-set-cursor-comet t
            (if (boundp 'neomacs-cursor-comet-trail-length)
                neomacs-cursor-comet-trail-length nil)
            (if (boundp 'neomacs-cursor-comet-fade-ms)
                neomacs-cursor-comet-fade-ms nil)
            (if (boundp 'neomacs-cursor-comet-color)
                neomacs-cursor-comet-color nil)
            val))))

;; --- Cursor spotlight effect ---
(declare-function neomacs-set-cursor-spotlight "neomacsterm.c"
  (&optional enabled radius intensity color))

(defcustom neomacs-cursor-spotlight nil
  "Enable cursor spotlight/radial gradient effect.
Non-nil renders a radial gradient centered on the cursor."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-spotlight)
           (neomacs-set-cursor-spotlight val
            (if (boundp 'neomacs-cursor-spotlight-radius)
                neomacs-cursor-spotlight-radius nil)
            (if (boundp 'neomacs-cursor-spotlight-intensity)
                neomacs-cursor-spotlight-intensity nil)
            (if (boundp 'neomacs-cursor-spotlight-color)
                neomacs-cursor-spotlight-color nil)))))

(defcustom neomacs-cursor-spotlight-radius 200
  "Spotlight radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-spotlight)
                    (boundp 'neomacs-cursor-spotlight)
                    neomacs-cursor-spotlight)
           (neomacs-set-cursor-spotlight t val
            (if (boundp 'neomacs-cursor-spotlight-intensity)
                neomacs-cursor-spotlight-intensity nil)
            (if (boundp 'neomacs-cursor-spotlight-color)
                neomacs-cursor-spotlight-color nil)))))

(defcustom neomacs-cursor-spotlight-intensity 15
  "Spotlight intensity (0-100)."
  :type '(integer :tag "Intensity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-spotlight)
                    (boundp 'neomacs-cursor-spotlight)
                    neomacs-cursor-spotlight)
           (neomacs-set-cursor-spotlight t
            (if (boundp 'neomacs-cursor-spotlight-radius)
                neomacs-cursor-spotlight-radius nil)
            val
            (if (boundp 'neomacs-cursor-spotlight-color)
                neomacs-cursor-spotlight-color nil)))))

(defcustom neomacs-cursor-spotlight-color "#FFFFE6"
  "Spotlight color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-spotlight)
                    (boundp 'neomacs-cursor-spotlight)
                    neomacs-cursor-spotlight)
           (neomacs-set-cursor-spotlight t
            (if (boundp 'neomacs-cursor-spotlight-radius)
                neomacs-cursor-spotlight-radius nil)
            (if (boundp 'neomacs-cursor-spotlight-intensity)
                neomacs-cursor-spotlight-intensity nil)
            val))))

;; --- Cursor particle trail effect ---
(declare-function neomacs-set-cursor-particles "neomacsterm.c"
  (&optional enabled color count lifetime-ms gravity))

(defcustom neomacs-cursor-particles nil
  "Enable cursor particle trail effect.
Non-nil emits small colored particles that scatter from the cursor
position when it moves, with physics-based motion and fade-out."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-particles)
           (neomacs-set-cursor-particles val
            (if (boundp 'neomacs-cursor-particles-color)
                neomacs-cursor-particles-color nil)
            (if (boundp 'neomacs-cursor-particles-count)
                neomacs-cursor-particles-count nil)
            (if (boundp 'neomacs-cursor-particles-lifetime)
                neomacs-cursor-particles-lifetime nil)
            (if (boundp 'neomacs-cursor-particles-gravity)
                neomacs-cursor-particles-gravity nil)))))

(defcustom neomacs-cursor-particles-color "#FF9933"
  "Cursor particle color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-particles)
                    (boundp 'neomacs-cursor-particles)
                    neomacs-cursor-particles)
           (neomacs-set-cursor-particles t val
            (if (boundp 'neomacs-cursor-particles-count)
                neomacs-cursor-particles-count nil)
            (if (boundp 'neomacs-cursor-particles-lifetime)
                neomacs-cursor-particles-lifetime nil)
            (if (boundp 'neomacs-cursor-particles-gravity)
                neomacs-cursor-particles-gravity nil)))))

(defcustom neomacs-cursor-particles-count 6
  "Number of particles emitted per cursor move."
  :type '(integer :tag "Count")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-particles)
                    (boundp 'neomacs-cursor-particles)
                    neomacs-cursor-particles)
           (neomacs-set-cursor-particles t
            (if (boundp 'neomacs-cursor-particles-color)
                neomacs-cursor-particles-color nil)
            val
            (if (boundp 'neomacs-cursor-particles-lifetime)
                neomacs-cursor-particles-lifetime nil)
            (if (boundp 'neomacs-cursor-particles-gravity)
                neomacs-cursor-particles-gravity nil)))))

(defcustom neomacs-cursor-particles-lifetime 800
  "Cursor particle lifetime in milliseconds."
  :type '(integer :tag "Lifetime (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-particles)
                    (boundp 'neomacs-cursor-particles)
                    neomacs-cursor-particles)
           (neomacs-set-cursor-particles t
            (if (boundp 'neomacs-cursor-particles-color)
                neomacs-cursor-particles-color nil)
            (if (boundp 'neomacs-cursor-particles-count)
                neomacs-cursor-particles-count nil)
            val
            (if (boundp 'neomacs-cursor-particles-gravity)
                neomacs-cursor-particles-gravity nil)))))

(defcustom neomacs-cursor-particles-gravity 120
  "Cursor particle gravity in pixels/sec^2 (positive = downward)."
  :type '(integer :tag "Gravity (px/s^2)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-particles)
                    (boundp 'neomacs-cursor-particles)
                    neomacs-cursor-particles)
           (neomacs-set-cursor-particles t
            (if (boundp 'neomacs-cursor-particles-color)
                neomacs-cursor-particles-color nil)
            (if (boundp 'neomacs-cursor-particles-count)
                neomacs-cursor-particles-count nil)
            (if (boundp 'neomacs-cursor-particles-lifetime)
                neomacs-cursor-particles-lifetime nil)
            val))))

;; --- Per-window rounded border ---
(declare-function neomacs-set-window-border-radius "neomacsterm.c"
  (&optional enabled radius border-width color opacity))

(defcustom neomacs-window-border-radius nil
  "Enable per-window rounded border.
Non-nil draws a rounded rectangle border around each non-minibuffer
window content area."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-window-border-radius)
           (neomacs-set-window-border-radius val
            (if (boundp 'neomacs-window-border-radius-size)
                neomacs-window-border-radius-size nil)
            (if (boundp 'neomacs-window-border-radius-width)
                neomacs-window-border-radius-width nil)
            (if (boundp 'neomacs-window-border-radius-color)
                neomacs-window-border-radius-color nil)
            (if (boundp 'neomacs-window-border-radius-opacity)
                neomacs-window-border-radius-opacity nil)))))

(defcustom neomacs-window-border-radius-size 8
  "Window rounded border corner radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-border-radius)
                    (boundp 'neomacs-window-border-radius)
                    neomacs-window-border-radius)
           (neomacs-set-window-border-radius t val
            (if (boundp 'neomacs-window-border-radius-width)
                neomacs-window-border-radius-width nil)
            (if (boundp 'neomacs-window-border-radius-color)
                neomacs-window-border-radius-color nil)
            (if (boundp 'neomacs-window-border-radius-opacity)
                neomacs-window-border-radius-opacity nil)))))

(defcustom neomacs-window-border-radius-width 1
  "Window rounded border line width in pixels."
  :type '(integer :tag "Width (px)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-border-radius)
                    (boundp 'neomacs-window-border-radius)
                    neomacs-window-border-radius)
           (neomacs-set-window-border-radius t
            (if (boundp 'neomacs-window-border-radius-size)
                neomacs-window-border-radius-size nil)
            val
            (if (boundp 'neomacs-window-border-radius-color)
                neomacs-window-border-radius-color nil)
            (if (boundp 'neomacs-window-border-radius-opacity)
                neomacs-window-border-radius-opacity nil)))))

(defcustom neomacs-window-border-radius-color "#808080"
  "Window rounded border color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-border-radius)
                    (boundp 'neomacs-window-border-radius)
                    neomacs-window-border-radius)
           (neomacs-set-window-border-radius t
            (if (boundp 'neomacs-window-border-radius-size)
                neomacs-window-border-radius-size nil)
            (if (boundp 'neomacs-window-border-radius-width)
                neomacs-window-border-radius-width nil)
            val
            (if (boundp 'neomacs-window-border-radius-opacity)
                neomacs-window-border-radius-opacity nil)))))

(defcustom neomacs-window-border-radius-opacity 30
  "Window rounded border opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-window-border-radius)
                    (boundp 'neomacs-window-border-radius)
                    neomacs-window-border-radius)
           (neomacs-set-window-border-radius t
            (if (boundp 'neomacs-window-border-radius-size)
                neomacs-window-border-radius-size nil)
            (if (boundp 'neomacs-window-border-radius-width)
                neomacs-window-border-radius-width nil)
            (if (boundp 'neomacs-window-border-radius-color)
                neomacs-window-border-radius-color nil)
            val))))

;; --- Typing heat map overlay ---
(declare-function neomacs-set-typing-heatmap "neomacsterm.c"
  (&optional enabled color fade-ms opacity))

(defcustom neomacs-typing-heatmap nil
  "Enable typing heat map overlay.
Non-nil highlights recently-edited character cells with a decaying
colored overlay, creating a visual heat map of editing activity."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-typing-heatmap)
           (neomacs-set-typing-heatmap val
            (if (boundp 'neomacs-typing-heatmap-color)
                neomacs-typing-heatmap-color nil)
            (if (boundp 'neomacs-typing-heatmap-fade-ms)
                neomacs-typing-heatmap-fade-ms nil)
            (if (boundp 'neomacs-typing-heatmap-opacity)
                neomacs-typing-heatmap-opacity nil)))))

(defcustom neomacs-typing-heatmap-color "#FF6619"
  "Typing heat map color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-typing-heatmap)
                    (boundp 'neomacs-typing-heatmap)
                    neomacs-typing-heatmap)
           (neomacs-set-typing-heatmap t val
            (if (boundp 'neomacs-typing-heatmap-fade-ms)
                neomacs-typing-heatmap-fade-ms nil)
            (if (boundp 'neomacs-typing-heatmap-opacity)
                neomacs-typing-heatmap-opacity nil)))))

(defcustom neomacs-typing-heatmap-fade-ms 2000
  "Typing heat map fade-out duration in milliseconds."
  :type '(integer :tag "Fade (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-typing-heatmap)
                    (boundp 'neomacs-typing-heatmap)
                    neomacs-typing-heatmap)
           (neomacs-set-typing-heatmap t
            (if (boundp 'neomacs-typing-heatmap-color)
                neomacs-typing-heatmap-color nil)
            val
            (if (boundp 'neomacs-typing-heatmap-opacity)
                neomacs-typing-heatmap-opacity nil)))))

(defcustom neomacs-typing-heatmap-opacity 15
  "Typing heat map maximum opacity (0-100)."
  :type '(integer :tag "Opacity (%)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-typing-heatmap)
                    (boundp 'neomacs-typing-heatmap)
                    neomacs-typing-heatmap)
           (neomacs-set-typing-heatmap t
            (if (boundp 'neomacs-typing-heatmap-color)
                neomacs-typing-heatmap-color nil)
            (if (boundp 'neomacs-typing-heatmap-fade-ms)
                neomacs-typing-heatmap-fade-ms nil)
            val))))

;; --- Smooth theme transition ---
(declare-function neomacs-set-theme-transition "neomacsterm.c"
  (&optional enabled duration-ms))

(defcustom neomacs-theme-transition nil
  "Enable smooth theme transition.
Non-nil crossfades the entire frame when the background color changes,
such as after calling `load-theme'."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-theme-transition)
           (neomacs-set-theme-transition val
            (if (boundp 'neomacs-theme-transition-duration)
                neomacs-theme-transition-duration nil)))))

(defcustom neomacs-theme-transition-duration 300
  "Theme transition crossfade duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-theme-transition)
                    (boundp 'neomacs-theme-transition)
                    neomacs-theme-transition)
           (neomacs-set-theme-transition t val))))

;; --- Window edge snap indicator ---
(declare-function neomacs-set-edge-snap "neomacsterm.c"
  (&optional enabled color duration-ms))

(defcustom neomacs-edge-snap nil
  "Enable window edge snap indicator.
Non-nil flashes a gradient bar at the top or bottom edge of the
selected window when Emacs rings the bell at buffer boundaries."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-edge-snap)
           (neomacs-set-edge-snap val
            (if (boundp 'neomacs-edge-snap-color)
                neomacs-edge-snap-color nil)
            (if (boundp 'neomacs-edge-snap-duration)
                neomacs-edge-snap-duration nil)))))

(defcustom neomacs-edge-snap-color "#FF6633"
  "Edge snap indicator color as hex RGB string."
  :type '(string :tag "Color (#RRGGBB)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-edge-snap)
                    (boundp 'neomacs-edge-snap)
                    neomacs-edge-snap)
           (neomacs-set-edge-snap t val
            (if (boundp 'neomacs-edge-snap-duration)
                neomacs-edge-snap-duration nil)))))

(defcustom neomacs-edge-snap-duration 200
  "Edge snap indicator flash duration in milliseconds."
  :type '(integer :tag "Duration (ms)")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-edge-snap)
                    (boundp 'neomacs-edge-snap)
                    neomacs-edge-snap)
           (neomacs-set-edge-snap t
            (if (boundp 'neomacs-edge-snap-color)
                neomacs-edge-snap-color nil)
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

;; --- Matrix/digital rain effect ---
(declare-function neomacs-set-matrix-rain "neomacsterm.c"
  (&optional enabled color speed opacity))

(defcustom neomacs-matrix-rain nil
  "Enable matrix/digital rain effect.
Non-nil renders animated vertical columns of cascading drops."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-matrix-rain)
           (neomacs-set-matrix-rain
            val
            (if (boundp 'neomacs-matrix-rain-color)
                neomacs-matrix-rain-color nil)
            (if (boundp 'neomacs-matrix-rain-speed)
                neomacs-matrix-rain-speed nil)
            (if (boundp 'neomacs-matrix-rain-opacity)
                neomacs-matrix-rain-opacity nil)))))

(defcustom neomacs-matrix-rain-color "#00CC33"
  "Matrix rain drop color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-matrix-rain)
                    (boundp 'neomacs-matrix-rain)
                    neomacs-matrix-rain)
           (neomacs-set-matrix-rain t val
            (if (boundp 'neomacs-matrix-rain-speed)
                neomacs-matrix-rain-speed nil)
            (if (boundp 'neomacs-matrix-rain-opacity)
                neomacs-matrix-rain-opacity nil)))))

(defcustom neomacs-matrix-rain-speed 150
  "Matrix rain fall speed in pixels per second."
  :type '(integer :tag "Speed")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-matrix-rain)
                    (boundp 'neomacs-matrix-rain)
                    neomacs-matrix-rain)
           (neomacs-set-matrix-rain t
            (if (boundp 'neomacs-matrix-rain-color)
                neomacs-matrix-rain-color nil)
            val
            (if (boundp 'neomacs-matrix-rain-opacity)
                neomacs-matrix-rain-opacity nil)))))

(defcustom neomacs-matrix-rain-opacity 12
  "Matrix rain opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-matrix-rain)
                    (boundp 'neomacs-matrix-rain)
                    neomacs-matrix-rain)
           (neomacs-set-matrix-rain t
            (if (boundp 'neomacs-matrix-rain-color)
                neomacs-matrix-rain-color nil)
            (if (boundp 'neomacs-matrix-rain-speed)
                neomacs-matrix-rain-speed nil)
            val))))

;; --- Cursor elastic snap animation ---
(declare-function neomacs-set-cursor-elastic-snap "neomacsterm.c"
  (&optional enabled overshoot duration-ms))

(defcustom neomacs-cursor-elastic-snap nil
  "Enable cursor elastic snap animation.
Non-nil makes the cursor overshoot its target and bounce back."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-elastic-snap)
           (neomacs-set-cursor-elastic-snap
            val
            (if (boundp 'neomacs-cursor-elastic-snap-overshoot)
                neomacs-cursor-elastic-snap-overshoot nil)
            (if (boundp 'neomacs-cursor-elastic-snap-duration)
                neomacs-cursor-elastic-snap-duration nil)))))

(defcustom neomacs-cursor-elastic-snap-overshoot 15
  "Cursor elastic snap overshoot amount (0-50 percent)."
  :type '(integer :tag "Overshoot %")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-elastic-snap)
                    (boundp 'neomacs-cursor-elastic-snap)
                    neomacs-cursor-elastic-snap)
           (neomacs-set-cursor-elastic-snap t val
            (if (boundp 'neomacs-cursor-elastic-snap-duration)
                neomacs-cursor-elastic-snap-duration nil)))))

(defcustom neomacs-cursor-elastic-snap-duration 200
  "Cursor elastic snap duration in milliseconds."
  :type '(integer :tag "Duration ms")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-elastic-snap)
                    (boundp 'neomacs-cursor-elastic-snap)
                    neomacs-cursor-elastic-snap)
           (neomacs-set-cursor-elastic-snap t
            (if (boundp 'neomacs-cursor-elastic-snap-overshoot)
                neomacs-cursor-elastic-snap-overshoot nil)
            val))))

;; --- Frost/ice border effect ---
(declare-function neomacs-set-frost-border "neomacsterm.c"
  (&optional enabled color width opacity))

(defcustom neomacs-frost-border nil
  "Enable frost/ice border effect.
Non-nil renders irregular crystalline frost patterns around window edges."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-frost-border)
           (neomacs-set-frost-border
            val
            (if (boundp 'neomacs-frost-border-color)
                neomacs-frost-border-color nil)
            (if (boundp 'neomacs-frost-border-width)
                neomacs-frost-border-width nil)
            (if (boundp 'neomacs-frost-border-opacity)
                neomacs-frost-border-opacity nil)))))

(defcustom neomacs-frost-border-color "#B3D9FF"
  "Frost border color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frost-border)
                    (boundp 'neomacs-frost-border)
                    neomacs-frost-border)
           (neomacs-set-frost-border t val
            (if (boundp 'neomacs-frost-border-width)
                neomacs-frost-border-width nil)
            (if (boundp 'neomacs-frost-border-opacity)
                neomacs-frost-border-opacity nil)))))

(defcustom neomacs-frost-border-width 6
  "Frost border width in pixels."
  :type '(integer :tag "Width")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frost-border)
                    (boundp 'neomacs-frost-border)
                    neomacs-frost-border)
           (neomacs-set-frost-border t
            (if (boundp 'neomacs-frost-border-color)
                neomacs-frost-border-color nil)
            val
            (if (boundp 'neomacs-frost-border-opacity)
                neomacs-frost-border-opacity nil)))))

(defcustom neomacs-frost-border-opacity 20
  "Frost border opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-frost-border)
                    (boundp 'neomacs-frost-border)
                    neomacs-frost-border)
           (neomacs-set-frost-border t
            (if (boundp 'neomacs-frost-border-color)
                neomacs-frost-border-color nil)
            (if (boundp 'neomacs-frost-border-width)
                neomacs-frost-border-width nil)
            val))))

;; --- Cursor afterimage ghost effect ---
(declare-function neomacs-set-cursor-ghost "neomacsterm.c"
  (&optional enabled color fade-ms opacity))

(defcustom neomacs-cursor-ghost nil
  "Enable cursor afterimage ghost effect.
Non-nil draws ghost copies of the cursor at previous positions."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-ghost)
           (neomacs-set-cursor-ghost
            val
            (if (boundp 'neomacs-cursor-ghost-color)
                neomacs-cursor-ghost-color nil)
            (if (boundp 'neomacs-cursor-ghost-fade-ms)
                neomacs-cursor-ghost-fade-ms nil)
            (if (boundp 'neomacs-cursor-ghost-opacity)
                neomacs-cursor-ghost-opacity nil)))))

(defcustom neomacs-cursor-ghost-color "#8080FF"
  "Cursor ghost color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ghost)
                    (boundp 'neomacs-cursor-ghost)
                    neomacs-cursor-ghost)
           (neomacs-set-cursor-ghost t val
            (if (boundp 'neomacs-cursor-ghost-fade-ms)
                neomacs-cursor-ghost-fade-ms nil)
            (if (boundp 'neomacs-cursor-ghost-opacity)
                neomacs-cursor-ghost-opacity nil)))))

(defcustom neomacs-cursor-ghost-fade-ms 600
  "Cursor ghost fade duration in milliseconds."
  :type '(integer :tag "Fade ms")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ghost)
                    (boundp 'neomacs-cursor-ghost)
                    neomacs-cursor-ghost)
           (neomacs-set-cursor-ghost t
            (if (boundp 'neomacs-cursor-ghost-color)
                neomacs-cursor-ghost-color nil)
            val
            (if (boundp 'neomacs-cursor-ghost-opacity)
                neomacs-cursor-ghost-opacity nil)))))

(defcustom neomacs-cursor-ghost-opacity 40
  "Cursor ghost opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ghost)
                    (boundp 'neomacs-cursor-ghost)
                    neomacs-cursor-ghost)
           (neomacs-set-cursor-ghost t
            (if (boundp 'neomacs-cursor-ghost-color)
                neomacs-cursor-ghost-color nil)
            (if (boundp 'neomacs-cursor-ghost-fade-ms)
                neomacs-cursor-ghost-fade-ms nil)
            val))))

;; --- Edge glow on scroll boundaries ---
(declare-function neomacs-set-edge-glow "neomacsterm.c"
  (&optional enabled color height opacity))

(defcustom neomacs-edge-glow nil
  "Enable edge glow effect when scrolling hits buffer boundaries.
Non-nil flashes a soft gradient glow at the top or bottom edge
of the selected window."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-edge-glow)
           (neomacs-set-edge-glow
            val
            (if (boundp 'neomacs-edge-glow-color)
                neomacs-edge-glow-color nil)
            (if (boundp 'neomacs-edge-glow-height)
                neomacs-edge-glow-height nil)
            (if (boundp 'neomacs-edge-glow-opacity)
                neomacs-edge-glow-opacity nil)))))

(defcustom neomacs-edge-glow-color "#6699FF"
  "Edge glow color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-edge-glow)
                    (boundp 'neomacs-edge-glow)
                    neomacs-edge-glow)
           (neomacs-set-edge-glow t val
            (if (boundp 'neomacs-edge-glow-height)
                neomacs-edge-glow-height nil)
            (if (boundp 'neomacs-edge-glow-opacity)
                neomacs-edge-glow-opacity nil)))))

(defcustom neomacs-edge-glow-height 40
  "Edge glow height in pixels."
  :type '(integer :tag "Height")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-edge-glow)
                    (boundp 'neomacs-edge-glow)
                    neomacs-edge-glow)
           (neomacs-set-edge-glow t
            (if (boundp 'neomacs-edge-glow-color)
                neomacs-edge-glow-color nil)
            val
            (if (boundp 'neomacs-edge-glow-opacity)
                neomacs-edge-glow-opacity nil)))))

(defcustom neomacs-edge-glow-opacity 30
  "Edge glow opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-edge-glow)
                    (boundp 'neomacs-edge-glow)
                    neomacs-edge-glow)
           (neomacs-set-edge-glow t
            (if (boundp 'neomacs-edge-glow-color)
                neomacs-edge-glow-color nil)
            (if (boundp 'neomacs-edge-glow-height)
                neomacs-edge-glow-height nil)
            val))))

;; --- Rain/drip ambient effect ---
(declare-function neomacs-set-rain-effect "neomacsterm.c"
  (&optional enabled color drop-count speed opacity))

(defcustom neomacs-rain-effect nil
  "Enable rain/drip ambient effect.
Non-nil renders animated vertical rain drops falling across the frame."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-rain-effect)
           (neomacs-set-rain-effect
            val
            (if (boundp 'neomacs-rain-effect-color)
                neomacs-rain-effect-color nil)
            (if (boundp 'neomacs-rain-effect-drop-count)
                neomacs-rain-effect-drop-count nil)
            (if (boundp 'neomacs-rain-effect-speed)
                neomacs-rain-effect-speed nil)
            (if (boundp 'neomacs-rain-effect-opacity)
                neomacs-rain-effect-opacity nil)))))

(defcustom neomacs-rain-effect-color "#8099CC"
  "Rain drop color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rain-effect)
                    (boundp 'neomacs-rain-effect)
                    neomacs-rain-effect)
           (neomacs-set-rain-effect t val
            (if (boundp 'neomacs-rain-effect-drop-count)
                neomacs-rain-effect-drop-count nil)
            (if (boundp 'neomacs-rain-effect-speed)
                neomacs-rain-effect-speed nil)
            (if (boundp 'neomacs-rain-effect-opacity)
                neomacs-rain-effect-opacity nil)))))

(defcustom neomacs-rain-effect-drop-count 30
  "Number of simultaneous rain drops."
  :type '(integer :tag "Drop Count")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rain-effect)
                    (boundp 'neomacs-rain-effect)
                    neomacs-rain-effect)
           (neomacs-set-rain-effect t
            (if (boundp 'neomacs-rain-effect-color)
                neomacs-rain-effect-color nil)
            val
            (if (boundp 'neomacs-rain-effect-speed)
                neomacs-rain-effect-speed nil)
            (if (boundp 'neomacs-rain-effect-opacity)
                neomacs-rain-effect-opacity nil)))))

(defcustom neomacs-rain-effect-speed 120
  "Rain drop fall speed in pixels per second."
  :type '(integer :tag "Speed")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rain-effect)
                    (boundp 'neomacs-rain-effect)
                    neomacs-rain-effect)
           (neomacs-set-rain-effect t
            (if (boundp 'neomacs-rain-effect-color)
                neomacs-rain-effect-color nil)
            (if (boundp 'neomacs-rain-effect-drop-count)
                neomacs-rain-effect-drop-count nil)
            val
            (if (boundp 'neomacs-rain-effect-opacity)
                neomacs-rain-effect-opacity nil)))))

(defcustom neomacs-rain-effect-opacity 15
  "Rain drop opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rain-effect)
                    (boundp 'neomacs-rain-effect)
                    neomacs-rain-effect)
           (neomacs-set-rain-effect t
            (if (boundp 'neomacs-rain-effect-color)
                neomacs-rain-effect-color nil)
            (if (boundp 'neomacs-rain-effect-drop-count)
                neomacs-rain-effect-drop-count nil)
            (if (boundp 'neomacs-rain-effect-speed)
                neomacs-rain-effect-speed nil)
            val))))

;; --- Cursor ripple wave effect ---
(declare-function neomacs-set-cursor-ripple-wave "neomacsterm.c"
  (&optional enabled color max-radius duration-ms opacity))

(defcustom neomacs-cursor-ripple-wave nil
  "Enable cursor ripple wave effect.
Non-nil draws expanding concentric ring waves from the cursor
position each time it moves."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-ripple-wave)
           (neomacs-set-cursor-ripple-wave
            val
            (if (boundp 'neomacs-cursor-ripple-wave-color)
                neomacs-cursor-ripple-wave-color nil)
            (if (boundp 'neomacs-cursor-ripple-wave-max-radius)
                neomacs-cursor-ripple-wave-max-radius nil)
            (if (boundp 'neomacs-cursor-ripple-wave-duration)
                neomacs-cursor-ripple-wave-duration nil)
            (if (boundp 'neomacs-cursor-ripple-wave-opacity)
                neomacs-cursor-ripple-wave-opacity nil)))))

(defcustom neomacs-cursor-ripple-wave-color "#6699FF"
  "Cursor ripple wave color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-wave)
                    (boundp 'neomacs-cursor-ripple-wave)
                    neomacs-cursor-ripple-wave)
           (neomacs-set-cursor-ripple-wave t val
            (if (boundp 'neomacs-cursor-ripple-wave-max-radius)
                neomacs-cursor-ripple-wave-max-radius nil)
            (if (boundp 'neomacs-cursor-ripple-wave-duration)
                neomacs-cursor-ripple-wave-duration nil)
            (if (boundp 'neomacs-cursor-ripple-wave-opacity)
                neomacs-cursor-ripple-wave-opacity nil)))))

(defcustom neomacs-cursor-ripple-wave-max-radius 80
  "Maximum radius of cursor ripple wave in pixels."
  :type '(integer :tag "Max Radius")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-wave)
                    (boundp 'neomacs-cursor-ripple-wave)
                    neomacs-cursor-ripple-wave)
           (neomacs-set-cursor-ripple-wave t
            (if (boundp 'neomacs-cursor-ripple-wave-color)
                neomacs-cursor-ripple-wave-color nil)
            val
            (if (boundp 'neomacs-cursor-ripple-wave-duration)
                neomacs-cursor-ripple-wave-duration nil)
            (if (boundp 'neomacs-cursor-ripple-wave-opacity)
                neomacs-cursor-ripple-wave-opacity nil)))))

(defcustom neomacs-cursor-ripple-wave-duration 500
  "Cursor ripple wave duration in milliseconds."
  :type '(integer :tag "Duration ms")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-wave)
                    (boundp 'neomacs-cursor-ripple-wave)
                    neomacs-cursor-ripple-wave)
           (neomacs-set-cursor-ripple-wave t
            (if (boundp 'neomacs-cursor-ripple-wave-color)
                neomacs-cursor-ripple-wave-color nil)
            (if (boundp 'neomacs-cursor-ripple-wave-max-radius)
                neomacs-cursor-ripple-wave-max-radius nil)
            val
            (if (boundp 'neomacs-cursor-ripple-wave-opacity)
                neomacs-cursor-ripple-wave-opacity nil)))))

(defcustom neomacs-cursor-ripple-wave-opacity 30
  "Cursor ripple wave opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-wave)
                    (boundp 'neomacs-cursor-ripple-wave)
                    neomacs-cursor-ripple-wave)
           (neomacs-set-cursor-ripple-wave t
            (if (boundp 'neomacs-cursor-ripple-wave-color)
                neomacs-cursor-ripple-wave-color nil)
            (if (boundp 'neomacs-cursor-ripple-wave-max-radius)
                neomacs-cursor-ripple-wave-max-radius nil)
            (if (boundp 'neomacs-cursor-ripple-wave-duration)
                neomacs-cursor-ripple-wave-duration nil)
            val))))

;; --- Aurora/northern lights effect ---
(declare-function neomacs-set-aurora "neomacsterm.c"
  (&optional enabled color1 color2 height opacity))

(defcustom neomacs-aurora nil
  "Enable aurora/northern lights effect.
Non-nil renders animated flowing color bands at the top of the frame."
  :type 'boolean
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-aurora)
           (neomacs-set-aurora
            val
            (if (boundp 'neomacs-aurora-color1)
                neomacs-aurora-color1 nil)
            (if (boundp 'neomacs-aurora-color2)
                neomacs-aurora-color2 nil)
            (if (boundp 'neomacs-aurora-height)
                neomacs-aurora-height nil)
            (if (boundp 'neomacs-aurora-opacity)
                neomacs-aurora-opacity nil)))))

(defcustom neomacs-aurora-color1 "#33CC66"
  "Primary aurora color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-aurora)
                    (boundp 'neomacs-aurora)
                    neomacs-aurora)
           (neomacs-set-aurora t val
            (if (boundp 'neomacs-aurora-color2)
                neomacs-aurora-color2 nil)
            (if (boundp 'neomacs-aurora-height)
                neomacs-aurora-height nil)
            (if (boundp 'neomacs-aurora-opacity)
                neomacs-aurora-opacity nil)))))

(defcustom neomacs-aurora-color2 "#4D66E6"
  "Secondary aurora color as a hex string."
  :type 'string
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-aurora)
                    (boundp 'neomacs-aurora)
                    neomacs-aurora)
           (neomacs-set-aurora t
            (if (boundp 'neomacs-aurora-color1)
                neomacs-aurora-color1 nil)
            val
            (if (boundp 'neomacs-aurora-height)
                neomacs-aurora-height nil)
            (if (boundp 'neomacs-aurora-opacity)
                neomacs-aurora-opacity nil)))))

(defcustom neomacs-aurora-height 60
  "Aurora band height in pixels."
  :type '(integer :tag "Height")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-aurora)
                    (boundp 'neomacs-aurora)
                    neomacs-aurora)
           (neomacs-set-aurora t
            (if (boundp 'neomacs-aurora-color1)
                neomacs-aurora-color1 nil)
            (if (boundp 'neomacs-aurora-color2)
                neomacs-aurora-color2 nil)
            val
            (if (boundp 'neomacs-aurora-opacity)
                neomacs-aurora-opacity nil)))))

(defcustom neomacs-aurora-opacity 12
  "Aurora opacity percentage (0-100)."
  :type '(integer :tag "Opacity")
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-aurora)
                    (boundp 'neomacs-aurora)
                    neomacs-aurora)
           (neomacs-set-aurora t
            (if (boundp 'neomacs-aurora-color1)
                neomacs-aurora-color1 nil)
            (if (boundp 'neomacs-aurora-color2)
                neomacs-aurora-color2 nil)
            (if (boundp 'neomacs-aurora-height)
                neomacs-aurora-height nil)
            val))))

;; Heat distortion effect
(declare-function neomacs-set-heat-distortion "neomacsterm.c")

(defcustom neomacs-heat-distortion nil
  "Enable heat distortion/shimmer effect along window edges."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-heat-distortion)
           (neomacs-set-heat-distortion val))))

(defcustom neomacs-heat-distortion-intensity 30
  "Intensity of heat distortion effect (0-100)."
  :type '(integer :tag "Intensity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-heat-distortion)
                    (boundp 'neomacs-heat-distortion)
                    neomacs-heat-distortion)
           (neomacs-set-heat-distortion t val))))

(defcustom neomacs-heat-distortion-speed 100
  "Animation speed for heat distortion (0-100)."
  :type '(integer :tag "Speed (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-heat-distortion)
                    (boundp 'neomacs-heat-distortion)
                    neomacs-heat-distortion)
           (neomacs-set-heat-distortion t
            (if (boundp 'neomacs-heat-distortion-intensity)
                neomacs-heat-distortion-intensity nil)
            val))))

(defcustom neomacs-heat-distortion-edge-width 30
  "Width of heat distortion edge region in pixels."
  :type '(integer :tag "Edge width (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-heat-distortion)
                    (boundp 'neomacs-heat-distortion)
                    neomacs-heat-distortion)
           (neomacs-set-heat-distortion t
            (if (boundp 'neomacs-heat-distortion-intensity)
                neomacs-heat-distortion-intensity nil)
            (if (boundp 'neomacs-heat-distortion-speed)
                neomacs-heat-distortion-speed nil)
            val))))

;; Cursor lighthouse beam effect
(declare-function neomacs-set-cursor-lighthouse "neomacsterm.c")

(defcustom neomacs-cursor-lighthouse nil
  "Enable cursor lighthouse beam effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-lighthouse)
           (neomacs-set-cursor-lighthouse val))))

(defcustom neomacs-cursor-lighthouse-color "#FFE64D"
  "Color of the lighthouse beam."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lighthouse)
                    (boundp 'neomacs-cursor-lighthouse)
                    neomacs-cursor-lighthouse)
           (neomacs-set-cursor-lighthouse t val))))

(defcustom neomacs-cursor-lighthouse-beam-width 15
  "Angular width of the lighthouse beam in degrees."
  :type '(integer :tag "Beam width (degrees)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lighthouse)
                    (boundp 'neomacs-cursor-lighthouse)
                    neomacs-cursor-lighthouse)
           (neomacs-set-cursor-lighthouse t
            (if (boundp 'neomacs-cursor-lighthouse-color)
                neomacs-cursor-lighthouse-color nil)
            val))))

(defcustom neomacs-cursor-lighthouse-rotation-speed 50
  "Rotation speed of lighthouse beam (revolutions/sec * 100)."
  :type '(integer :tag "Speed (0-200)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lighthouse)
                    (boundp 'neomacs-cursor-lighthouse)
                    neomacs-cursor-lighthouse)
           (neomacs-set-cursor-lighthouse t
            (if (boundp 'neomacs-cursor-lighthouse-color)
                neomacs-cursor-lighthouse-color nil)
            (if (boundp 'neomacs-cursor-lighthouse-beam-width)
                neomacs-cursor-lighthouse-beam-width nil)
            val))))

;; Neon border effect
(declare-function neomacs-set-neon-border "neomacsterm.c")

(defcustom neomacs-neon-border nil
  "Enable neon border glow effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-neon-border)
           (neomacs-set-neon-border val))))

(defcustom neomacs-neon-border-color "#00FFCC"
  "Color of the neon border glow."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-neon-border)
                    (boundp 'neomacs-neon-border)
                    neomacs-neon-border)
           (neomacs-set-neon-border t val))))

(defcustom neomacs-neon-border-intensity 60
  "Neon border glow intensity (0-100)."
  :type '(integer :tag "Intensity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-neon-border)
                    (boundp 'neomacs-neon-border)
                    neomacs-neon-border)
           (neomacs-set-neon-border t
            (if (boundp 'neomacs-neon-border-color)
                neomacs-neon-border-color nil)
            val))))

(defcustom neomacs-neon-border-flicker 10
  "Neon border flicker amount (0-100, 0=no flicker)."
  :type '(integer :tag "Flicker (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-neon-border)
                    (boundp 'neomacs-neon-border)
                    neomacs-neon-border)
           (neomacs-set-neon-border t
            (if (boundp 'neomacs-neon-border-color)
                neomacs-neon-border-color nil)
            (if (boundp 'neomacs-neon-border-intensity)
                neomacs-neon-border-intensity nil)
            val))))

;; Cursor sonar ping effect
(declare-function neomacs-set-cursor-sonar-ping "neomacsterm.c")

(defcustom neomacs-cursor-sonar-ping nil
  "Enable cursor sonar ping effect (expanding rings on keypress)."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-sonar-ping)
           (neomacs-set-cursor-sonar-ping val))))

(defcustom neomacs-cursor-sonar-ping-color "#4DB3FF"
  "Color of sonar ping rings."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sonar-ping)
                    (boundp 'neomacs-cursor-sonar-ping)
                    neomacs-cursor-sonar-ping)
           (neomacs-set-cursor-sonar-ping t val))))

(defcustom neomacs-cursor-sonar-ping-ring-count 3
  "Number of concentric rings per sonar ping."
  :type '(integer :tag "Ring count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sonar-ping)
                    (boundp 'neomacs-cursor-sonar-ping)
                    neomacs-cursor-sonar-ping)
           (neomacs-set-cursor-sonar-ping t
            (if (boundp 'neomacs-cursor-sonar-ping-color)
                neomacs-cursor-sonar-ping-color nil)
            val))))

(defcustom neomacs-cursor-sonar-ping-max-radius 60
  "Maximum expansion radius in pixels for sonar ping."
  :type '(integer :tag "Max radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sonar-ping)
                    (boundp 'neomacs-cursor-sonar-ping)
                    neomacs-cursor-sonar-ping)
           (neomacs-set-cursor-sonar-ping t
            (if (boundp 'neomacs-cursor-sonar-ping-color)
                neomacs-cursor-sonar-ping-color nil)
            (if (boundp 'neomacs-cursor-sonar-ping-ring-count)
                neomacs-cursor-sonar-ping-ring-count nil)
            val))))

;; Lightning bolt effect
(declare-function neomacs-set-lightning-bolt "neomacsterm.c")

(defcustom neomacs-lightning-bolt nil
  "Enable lightning bolt effect along window borders."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-lightning-bolt)
           (neomacs-set-lightning-bolt val))))

(defcustom neomacs-lightning-bolt-color "#B3CCFF"
  "Color of lightning bolt discharges."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-lightning-bolt)
                    (boundp 'neomacs-lightning-bolt)
                    neomacs-lightning-bolt)
           (neomacs-set-lightning-bolt t val))))

(defcustom neomacs-lightning-bolt-frequency 100
  "Bolts per second * 100 (100 = 1.0 bolt/sec)."
  :type '(integer :tag "Frequency")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-lightning-bolt)
                    (boundp 'neomacs-lightning-bolt)
                    neomacs-lightning-bolt)
           (neomacs-set-lightning-bolt t
            (if (boundp 'neomacs-lightning-bolt-color)
                neomacs-lightning-bolt-color nil)
            val))))

(defcustom neomacs-lightning-bolt-intensity 80
  "Bolt brightness (0-100)."
  :type '(integer :tag "Intensity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-lightning-bolt)
                    (boundp 'neomacs-lightning-bolt)
                    neomacs-lightning-bolt)
           (neomacs-set-lightning-bolt t
            (if (boundp 'neomacs-lightning-bolt-color)
                neomacs-lightning-bolt-color nil)
            (if (boundp 'neomacs-lightning-bolt-frequency)
                neomacs-lightning-bolt-frequency nil)
            val))))

(defcustom neomacs-lightning-bolt-opacity 30
  "Overall opacity of lightning bolts (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-lightning-bolt)
                    (boundp 'neomacs-lightning-bolt)
                    neomacs-lightning-bolt)
           (neomacs-set-lightning-bolt t
            (if (boundp 'neomacs-lightning-bolt-color)
                neomacs-lightning-bolt-color nil)
            (if (boundp 'neomacs-lightning-bolt-frequency)
                neomacs-lightning-bolt-frequency nil)
            (if (boundp 'neomacs-lightning-bolt-intensity)
                neomacs-lightning-bolt-intensity nil)
            val))))

;; Cursor orbit particles effect
(declare-function neomacs-set-cursor-orbit-particles "neomacsterm.c")

(defcustom neomacs-cursor-orbit-particles nil
  "Enable cursor orbit particles effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-orbit-particles)
           (neomacs-set-cursor-orbit-particles val))))

(defcustom neomacs-cursor-orbit-particles-color "#FFCC4D"
  "Color of orbiting particles."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-orbit-particles)
                    (boundp 'neomacs-cursor-orbit-particles)
                    neomacs-cursor-orbit-particles)
           (neomacs-set-cursor-orbit-particles t val))))

(defcustom neomacs-cursor-orbit-particles-count 6
  "Number of orbiting particles."
  :type '(integer :tag "Particle count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-orbit-particles)
                    (boundp 'neomacs-cursor-orbit-particles)
                    neomacs-cursor-orbit-particles)
           (neomacs-set-cursor-orbit-particles t
            (if (boundp 'neomacs-cursor-orbit-particles-color)
                neomacs-cursor-orbit-particles-color nil)
            val))))

(defcustom neomacs-cursor-orbit-particles-radius 25
  "Orbit radius in pixels."
  :type '(integer :tag "Radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-orbit-particles)
                    (boundp 'neomacs-cursor-orbit-particles)
                    neomacs-cursor-orbit-particles)
           (neomacs-set-cursor-orbit-particles t
            (if (boundp 'neomacs-cursor-orbit-particles-color)
                neomacs-cursor-orbit-particles-color nil)
            (if (boundp 'neomacs-cursor-orbit-particles-count)
                neomacs-cursor-orbit-particles-count nil)
            val))))

(defcustom neomacs-cursor-orbit-particles-speed 150
  "Orbit speed * 100 (150 = 1.5 rev/sec)."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-orbit-particles)
                    (boundp 'neomacs-cursor-orbit-particles)
                    neomacs-cursor-orbit-particles)
           (neomacs-set-cursor-orbit-particles t
            (if (boundp 'neomacs-cursor-orbit-particles-color)
                neomacs-cursor-orbit-particles-color nil)
            (if (boundp 'neomacs-cursor-orbit-particles-count)
                neomacs-cursor-orbit-particles-count nil)
            (if (boundp 'neomacs-cursor-orbit-particles-radius)
                neomacs-cursor-orbit-particles-radius nil)
            val))))

(defcustom neomacs-cursor-orbit-particles-opacity 35
  "Particle opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-orbit-particles)
                    (boundp 'neomacs-cursor-orbit-particles)
                    neomacs-cursor-orbit-particles)
           (neomacs-set-cursor-orbit-particles t
            (if (boundp 'neomacs-cursor-orbit-particles-color)
                neomacs-cursor-orbit-particles-color nil)
            (if (boundp 'neomacs-cursor-orbit-particles-count)
                neomacs-cursor-orbit-particles-count nil)
            (if (boundp 'neomacs-cursor-orbit-particles-radius)
                neomacs-cursor-orbit-particles-radius nil)
            (if (boundp 'neomacs-cursor-orbit-particles-speed)
                neomacs-cursor-orbit-particles-speed nil)
            val))))

;; Plasma border effect
(declare-function neomacs-set-plasma-border "neomacsterm.c")

(defcustom neomacs-plasma-border nil
  "Enable animated plasma border effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-plasma-border)
           (neomacs-set-plasma-border val))))

(defcustom neomacs-plasma-border-color1 "#FF3380"
  "Primary plasma border color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plasma-border)
                    (boundp 'neomacs-plasma-border)
                    neomacs-plasma-border)
           (neomacs-set-plasma-border t val))))

(defcustom neomacs-plasma-border-color2 "#3380FF"
  "Secondary plasma border color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plasma-border)
                    (boundp 'neomacs-plasma-border)
                    neomacs-plasma-border)
           (neomacs-set-plasma-border t
            (if (boundp 'neomacs-plasma-border-color1)
                neomacs-plasma-border-color1 nil)
            val))))

(defcustom neomacs-plasma-border-width 4
  "Plasma border width in pixels."
  :type '(integer :tag "Width (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plasma-border)
                    (boundp 'neomacs-plasma-border)
                    neomacs-plasma-border)
           (neomacs-set-plasma-border t
            (if (boundp 'neomacs-plasma-border-color1)
                neomacs-plasma-border-color1 nil)
            (if (boundp 'neomacs-plasma-border-color2)
                neomacs-plasma-border-color2 nil)
            val))))

(defcustom neomacs-plasma-border-speed 100
  "Plasma animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plasma-border)
                    (boundp 'neomacs-plasma-border)
                    neomacs-plasma-border)
           (neomacs-set-plasma-border t
            (if (boundp 'neomacs-plasma-border-color1)
                neomacs-plasma-border-color1 nil)
            (if (boundp 'neomacs-plasma-border-color2)
                neomacs-plasma-border-color2 nil)
            (if (boundp 'neomacs-plasma-border-width)
                neomacs-plasma-border-width nil)
            val))))

(defcustom neomacs-plasma-border-opacity 30
  "Plasma border opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plasma-border)
                    (boundp 'neomacs-plasma-border)
                    neomacs-plasma-border)
           (neomacs-set-plasma-border t
            (if (boundp 'neomacs-plasma-border-color1)
                neomacs-plasma-border-color1 nil)
            (if (boundp 'neomacs-plasma-border-color2)
                neomacs-plasma-border-color2 nil)
            (if (boundp 'neomacs-plasma-border-width)
                neomacs-plasma-border-width nil)
            (if (boundp 'neomacs-plasma-border-speed)
                neomacs-plasma-border-speed nil)
            val))))

;; Cursor heartbeat pulse effect
(declare-function neomacs-set-cursor-heartbeat "neomacsterm.c")

(defcustom neomacs-cursor-heartbeat nil
  "Enable cursor heartbeat pulse effect (double-pulse rings)."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-heartbeat)
           (neomacs-set-cursor-heartbeat val))))

(defcustom neomacs-cursor-heartbeat-color "#FF4D4D"
  "Color of heartbeat pulse rings."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-heartbeat)
                    (boundp 'neomacs-cursor-heartbeat)
                    neomacs-cursor-heartbeat)
           (neomacs-set-cursor-heartbeat t val))))

(defcustom neomacs-cursor-heartbeat-bpm 72
  "Heartbeat rate in beats per minute."
  :type '(integer :tag "BPM")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-heartbeat)
                    (boundp 'neomacs-cursor-heartbeat)
                    neomacs-cursor-heartbeat)
           (neomacs-set-cursor-heartbeat t
            (if (boundp 'neomacs-cursor-heartbeat-color)
                neomacs-cursor-heartbeat-color nil)
            val))))

(defcustom neomacs-cursor-heartbeat-max-radius 50
  "Maximum expansion radius in pixels for heartbeat pulse."
  :type '(integer :tag "Max radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-heartbeat)
                    (boundp 'neomacs-cursor-heartbeat)
                    neomacs-cursor-heartbeat)
           (neomacs-set-cursor-heartbeat t
            (if (boundp 'neomacs-cursor-heartbeat-color)
                neomacs-cursor-heartbeat-color nil)
            (if (boundp 'neomacs-cursor-heartbeat-bpm)
                neomacs-cursor-heartbeat-bpm nil)
            val))))

(defcustom neomacs-cursor-heartbeat-opacity 20
  "Heartbeat pulse opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-heartbeat)
                    (boundp 'neomacs-cursor-heartbeat)
                    neomacs-cursor-heartbeat)
           (neomacs-set-cursor-heartbeat t
            (if (boundp 'neomacs-cursor-heartbeat-color)
                neomacs-cursor-heartbeat-color nil)
            (if (boundp 'neomacs-cursor-heartbeat-bpm)
                neomacs-cursor-heartbeat-bpm nil)
            (if (boundp 'neomacs-cursor-heartbeat-max-radius)
                neomacs-cursor-heartbeat-max-radius nil)
            val))))

;; Topographic contour effect
(declare-function neomacs-set-topo-contour "neomacsterm.c")

(defcustom neomacs-topo-contour nil
  "Enable topographic contour line effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-topo-contour)
           (neomacs-set-topo-contour val))))

(defcustom neomacs-topo-contour-color "#66B380"
  "Contour line color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-topo-contour)
                    (boundp 'neomacs-topo-contour)
                    neomacs-topo-contour)
           (neomacs-set-topo-contour t val))))

(defcustom neomacs-topo-contour-spacing 30
  "Line spacing in pixels."
  :type '(integer :tag "Spacing (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-topo-contour)
                    (boundp 'neomacs-topo-contour)
                    neomacs-topo-contour)
           (neomacs-set-topo-contour t
            (if (boundp 'neomacs-topo-contour-color)
                neomacs-topo-contour-color nil)
            val))))

(defcustom neomacs-topo-contour-speed 100
  "Animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-topo-contour)
                    (boundp 'neomacs-topo-contour)
                    neomacs-topo-contour)
           (neomacs-set-topo-contour t
            (if (boundp 'neomacs-topo-contour-color)
                neomacs-topo-contour-color nil)
            (if (boundp 'neomacs-topo-contour-spacing)
                neomacs-topo-contour-spacing nil)
            val))))

(defcustom neomacs-topo-contour-opacity 10
  "Contour line opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-topo-contour)
                    (boundp 'neomacs-topo-contour)
                    neomacs-topo-contour)
           (neomacs-set-topo-contour t
            (if (boundp 'neomacs-topo-contour-color)
                neomacs-topo-contour-color nil)
            (if (boundp 'neomacs-topo-contour-spacing)
                neomacs-topo-contour-spacing nil)
            (if (boundp 'neomacs-topo-contour-speed)
                neomacs-topo-contour-speed nil)
            val))))

;; Cursor metronome tick effect
(declare-function neomacs-set-cursor-metronome "neomacsterm.c")

(defcustom neomacs-cursor-metronome nil
  "Enable cursor metronome tick effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-metronome)
           (neomacs-set-cursor-metronome val))))

(defcustom neomacs-cursor-metronome-color "#E58033"
  "Metronome tick color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-metronome)
                    (boundp 'neomacs-cursor-metronome)
                    neomacs-cursor-metronome)
           (neomacs-set-cursor-metronome t val))))

(defcustom neomacs-cursor-metronome-tick-height 20
  "Tick height in pixels."
  :type '(integer :tag "Height (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-metronome)
                    (boundp 'neomacs-cursor-metronome)
                    neomacs-cursor-metronome)
           (neomacs-set-cursor-metronome t
            (if (boundp 'neomacs-cursor-metronome-color)
                neomacs-cursor-metronome-color nil)
            val))))

(defcustom neomacs-cursor-metronome-fade-ms 300
  "Tick fade duration in milliseconds."
  :type '(integer :tag "Fade (ms)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-metronome)
                    (boundp 'neomacs-cursor-metronome)
                    neomacs-cursor-metronome)
           (neomacs-set-cursor-metronome t
            (if (boundp 'neomacs-cursor-metronome-color)
                neomacs-cursor-metronome-color nil)
            (if (boundp 'neomacs-cursor-metronome-tick-height)
                neomacs-cursor-metronome-tick-height nil)
            val))))

(defcustom neomacs-cursor-metronome-opacity 40
  "Metronome tick opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-metronome)
                    (boundp 'neomacs-cursor-metronome)
                    neomacs-cursor-metronome)
           (neomacs-set-cursor-metronome t
            (if (boundp 'neomacs-cursor-metronome-color)
                neomacs-cursor-metronome-color nil)
            (if (boundp 'neomacs-cursor-metronome-tick-height)
                neomacs-cursor-metronome-tick-height nil)
            (if (boundp 'neomacs-cursor-metronome-fade-ms)
                neomacs-cursor-metronome-fade-ms nil)
            val))))

;; Constellation overlay effect
(declare-function neomacs-set-constellation "neomacsterm.c")

(defcustom neomacs-constellation nil
  "Enable constellation overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-constellation)
           (neomacs-set-constellation val))))

(defcustom neomacs-constellation-color "#B3CCFF"
  "Star and connection color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-constellation)
                    (boundp 'neomacs-constellation)
                    neomacs-constellation)
           (neomacs-set-constellation t val))))

(defcustom neomacs-constellation-star-count 50
  "Number of stars."
  :type '(integer :tag "Star count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-constellation)
                    (boundp 'neomacs-constellation)
                    neomacs-constellation)
           (neomacs-set-constellation t
            (if (boundp 'neomacs-constellation-color)
                neomacs-constellation-color nil)
            val))))

(defcustom neomacs-constellation-connect-dist 80
  "Connection distance in pixels."
  :type '(integer :tag "Distance (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-constellation)
                    (boundp 'neomacs-constellation)
                    neomacs-constellation)
           (neomacs-set-constellation t
            (if (boundp 'neomacs-constellation-color)
                neomacs-constellation-color nil)
            (if (boundp 'neomacs-constellation-star-count)
                neomacs-constellation-star-count nil)
            val))))

(defcustom neomacs-constellation-twinkle-speed 100
  "Twinkle speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-constellation)
                    (boundp 'neomacs-constellation)
                    neomacs-constellation)
           (neomacs-set-constellation t
            (if (boundp 'neomacs-constellation-color)
                neomacs-constellation-color nil)
            (if (boundp 'neomacs-constellation-star-count)
                neomacs-constellation-star-count nil)
            (if (boundp 'neomacs-constellation-connect-dist)
                neomacs-constellation-connect-dist nil)
            val))))

(defcustom neomacs-constellation-opacity 15
  "Constellation opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-constellation)
                    (boundp 'neomacs-constellation)
                    neomacs-constellation)
           (neomacs-set-constellation t
            (if (boundp 'neomacs-constellation-color)
                neomacs-constellation-color nil)
            (if (boundp 'neomacs-constellation-star-count)
                neomacs-constellation-star-count nil)
            (if (boundp 'neomacs-constellation-connect-dist)
                neomacs-constellation-connect-dist nil)
            (if (boundp 'neomacs-constellation-twinkle-speed)
                neomacs-constellation-twinkle-speed nil)
            val))))

;; Cursor radar sweep effect
(declare-function neomacs-set-cursor-radar "neomacsterm.c")

(defcustom neomacs-cursor-radar nil
  "Enable cursor radar sweep effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-radar)
           (neomacs-set-cursor-radar val))))

(defcustom neomacs-cursor-radar-color "#33E566"
  "Radar sweep color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-radar)
                    (boundp 'neomacs-cursor-radar)
                    neomacs-cursor-radar)
           (neomacs-set-cursor-radar t val))))

(defcustom neomacs-cursor-radar-radius 40
  "Sweep radius in pixels."
  :type '(integer :tag "Radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-radar)
                    (boundp 'neomacs-cursor-radar)
                    neomacs-cursor-radar)
           (neomacs-set-cursor-radar t
            (if (boundp 'neomacs-cursor-radar-color)
                neomacs-cursor-radar-color nil)
            val))))

(defcustom neomacs-cursor-radar-speed 150
  "Rotation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-radar)
                    (boundp 'neomacs-cursor-radar)
                    neomacs-cursor-radar)
           (neomacs-set-cursor-radar t
            (if (boundp 'neomacs-cursor-radar-color)
                neomacs-cursor-radar-color nil)
            (if (boundp 'neomacs-cursor-radar-radius)
                neomacs-cursor-radar-radius nil)
            val))))

(defcustom neomacs-cursor-radar-opacity 20
  "Radar sweep opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-radar)
                    (boundp 'neomacs-cursor-radar)
                    neomacs-cursor-radar)
           (neomacs-set-cursor-radar t
            (if (boundp 'neomacs-cursor-radar-color)
                neomacs-cursor-radar-color nil)
            (if (boundp 'neomacs-cursor-radar-radius)
                neomacs-cursor-radar-radius nil)
            (if (boundp 'neomacs-cursor-radar-speed)
                neomacs-cursor-radar-speed nil)
            val))))

;; Kaleidoscope overlay effect
(declare-function neomacs-set-kaleidoscope "neomacsterm.c")

(defcustom neomacs-kaleidoscope nil
  "Enable kaleidoscope overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-kaleidoscope)
           (neomacs-set-kaleidoscope val))))

(defcustom neomacs-kaleidoscope-color "#9933E5"
  "Kaleidoscope pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-kaleidoscope)
                    (boundp 'neomacs-kaleidoscope)
                    neomacs-kaleidoscope)
           (neomacs-set-kaleidoscope t val))))

(defcustom neomacs-kaleidoscope-segments 6
  "Number of mirror segments."
  :type '(integer :tag "Segments")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-kaleidoscope)
                    (boundp 'neomacs-kaleidoscope)
                    neomacs-kaleidoscope)
           (neomacs-set-kaleidoscope t
            (if (boundp 'neomacs-kaleidoscope-color)
                neomacs-kaleidoscope-color nil)
            val))))

(defcustom neomacs-kaleidoscope-speed 50
  "Rotation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-kaleidoscope)
                    (boundp 'neomacs-kaleidoscope)
                    neomacs-kaleidoscope)
           (neomacs-set-kaleidoscope t
            (if (boundp 'neomacs-kaleidoscope-color)
                neomacs-kaleidoscope-color nil)
            (if (boundp 'neomacs-kaleidoscope-segments)
                neomacs-kaleidoscope-segments nil)
            val))))

(defcustom neomacs-kaleidoscope-opacity 10
  "Kaleidoscope overlay opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-kaleidoscope)
                    (boundp 'neomacs-kaleidoscope)
                    neomacs-kaleidoscope)
           (neomacs-set-kaleidoscope t
            (if (boundp 'neomacs-kaleidoscope-color)
                neomacs-kaleidoscope-color nil)
            (if (boundp 'neomacs-kaleidoscope-segments)
                neomacs-kaleidoscope-segments nil)
            (if (boundp 'neomacs-kaleidoscope-speed)
                neomacs-kaleidoscope-speed nil)
            val))))

;; Cursor ripple ring effect
(declare-function neomacs-set-cursor-ripple-ring "neomacsterm.c")

(defcustom neomacs-cursor-ripple-ring nil
  "Enable cursor ripple ring effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-ripple-ring)
           (neomacs-set-cursor-ripple-ring val))))

(defcustom neomacs-cursor-ripple-ring-color "#4DCCE5"
  "Ripple ring color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-ring)
                    (boundp 'neomacs-cursor-ripple-ring)
                    neomacs-cursor-ripple-ring)
           (neomacs-set-cursor-ripple-ring t val))))

(defcustom neomacs-cursor-ripple-ring-max-radius 60
  "Max ring radius in pixels."
  :type '(integer :tag "Max radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-ring)
                    (boundp 'neomacs-cursor-ripple-ring)
                    neomacs-cursor-ripple-ring)
           (neomacs-set-cursor-ripple-ring t
            (if (boundp 'neomacs-cursor-ripple-ring-color)
                neomacs-cursor-ripple-ring-color nil)
            val))))

(defcustom neomacs-cursor-ripple-ring-count 3
  "Number of concentric rings."
  :type '(integer :tag "Ring count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-ring)
                    (boundp 'neomacs-cursor-ripple-ring)
                    neomacs-cursor-ripple-ring)
           (neomacs-set-cursor-ripple-ring t
            (if (boundp 'neomacs-cursor-ripple-ring-color)
                neomacs-cursor-ripple-ring-color nil)
            (if (boundp 'neomacs-cursor-ripple-ring-max-radius)
                neomacs-cursor-ripple-ring-max-radius nil)
            val))))

(defcustom neomacs-cursor-ripple-ring-speed 200
  "Expansion speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-ring)
                    (boundp 'neomacs-cursor-ripple-ring)
                    neomacs-cursor-ripple-ring)
           (neomacs-set-cursor-ripple-ring t
            (if (boundp 'neomacs-cursor-ripple-ring-color)
                neomacs-cursor-ripple-ring-color nil)
            (if (boundp 'neomacs-cursor-ripple-ring-max-radius)
                neomacs-cursor-ripple-ring-max-radius nil)
            (if (boundp 'neomacs-cursor-ripple-ring-count)
                neomacs-cursor-ripple-ring-count nil)
            val))))

(defcustom neomacs-cursor-ripple-ring-opacity 25
  "Ripple ring opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-ripple-ring)
                    (boundp 'neomacs-cursor-ripple-ring)
                    neomacs-cursor-ripple-ring)
           (neomacs-set-cursor-ripple-ring t
            (if (boundp 'neomacs-cursor-ripple-ring-color)
                neomacs-cursor-ripple-ring-color nil)
            (if (boundp 'neomacs-cursor-ripple-ring-max-radius)
                neomacs-cursor-ripple-ring-max-radius nil)
            (if (boundp 'neomacs-cursor-ripple-ring-count)
                neomacs-cursor-ripple-ring-count nil)
            (if (boundp 'neomacs-cursor-ripple-ring-speed)
                neomacs-cursor-ripple-ring-speed nil)
            val))))

;; Noise field overlay effect
(declare-function neomacs-set-noise-field "neomacsterm.c")

(defcustom neomacs-noise-field nil
  "Enable noise field overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-noise-field)
           (neomacs-set-noise-field val))))

(defcustom neomacs-noise-field-color "#80B34D"
  "Noise field color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-noise-field)
                    (boundp 'neomacs-noise-field)
                    neomacs-noise-field)
           (neomacs-set-noise-field t val))))

(defcustom neomacs-noise-field-scale 50
  "Noise cell size in pixels."
  :type '(integer :tag "Scale (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-noise-field)
                    (boundp 'neomacs-noise-field)
                    neomacs-noise-field)
           (neomacs-set-noise-field t
            (if (boundp 'neomacs-noise-field-color)
                neomacs-noise-field-color nil)
            val))))

(defcustom neomacs-noise-field-speed 50
  "Animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-noise-field)
                    (boundp 'neomacs-noise-field)
                    neomacs-noise-field)
           (neomacs-set-noise-field t
            (if (boundp 'neomacs-noise-field-color)
                neomacs-noise-field-color nil)
            (if (boundp 'neomacs-noise-field-scale)
                neomacs-noise-field-scale nil)
            val))))

(defcustom neomacs-noise-field-opacity 8
  "Noise field opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-noise-field)
                    (boundp 'neomacs-noise-field)
                    neomacs-noise-field)
           (neomacs-set-noise-field t
            (if (boundp 'neomacs-noise-field-color)
                neomacs-noise-field-color nil)
            (if (boundp 'neomacs-noise-field-scale)
                neomacs-noise-field-scale nil)
            (if (boundp 'neomacs-noise-field-speed)
                neomacs-noise-field-speed nil)
            val))))

;; Cursor scope effect
(declare-function neomacs-set-cursor-scope "neomacsterm.c")

(defcustom neomacs-cursor-scope nil
  "Enable cursor scope effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-scope)
           (neomacs-set-cursor-scope val))))

(defcustom neomacs-cursor-scope-color "#FFCC33"
  "Scope line color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-scope)
                    (boundp 'neomacs-cursor-scope)
                    neomacs-cursor-scope)
           (neomacs-set-cursor-scope t val))))

(defcustom neomacs-cursor-scope-thickness 1
  "Scope line thickness in pixels."
  :type '(integer :tag "Thickness (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-scope)
                    (boundp 'neomacs-cursor-scope)
                    neomacs-cursor-scope)
           (neomacs-set-cursor-scope t
            (if (boundp 'neomacs-cursor-scope-color)
                neomacs-cursor-scope-color nil)
            val))))

(defcustom neomacs-cursor-scope-gap 10
  "Gap around cursor in pixels."
  :type '(integer :tag "Gap (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-scope)
                    (boundp 'neomacs-cursor-scope)
                    neomacs-cursor-scope)
           (neomacs-set-cursor-scope t
            (if (boundp 'neomacs-cursor-scope-color)
                neomacs-cursor-scope-color nil)
            (if (boundp 'neomacs-cursor-scope-thickness)
                neomacs-cursor-scope-thickness nil)
            val))))

(defcustom neomacs-cursor-scope-opacity 30
  "Cursor scope opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-scope)
                    (boundp 'neomacs-cursor-scope)
                    neomacs-cursor-scope)
           (neomacs-set-cursor-scope t
            (if (boundp 'neomacs-cursor-scope-color)
                neomacs-cursor-scope-color nil)
            (if (boundp 'neomacs-cursor-scope-thickness)
                neomacs-cursor-scope-thickness nil)
            (if (boundp 'neomacs-cursor-scope-gap)
                neomacs-cursor-scope-gap nil)
            val))))

;; Spiral vortex overlay effect
(declare-function neomacs-set-spiral-vortex "neomacsterm.c")

(defcustom neomacs-spiral-vortex nil
  "Enable spiral vortex overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-spiral-vortex)
           (neomacs-set-spiral-vortex val))))

(defcustom neomacs-spiral-vortex-color "#6633CC"
  "Spiral vortex color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-spiral-vortex)
                    (boundp 'neomacs-spiral-vortex)
                    neomacs-spiral-vortex)
           (neomacs-set-spiral-vortex t val))))

(defcustom neomacs-spiral-vortex-arms 4
  "Number of spiral arms."
  :type '(integer :tag "Arms")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-spiral-vortex)
                    (boundp 'neomacs-spiral-vortex)
                    neomacs-spiral-vortex)
           (neomacs-set-spiral-vortex t
            (if (boundp 'neomacs-spiral-vortex-color)
                neomacs-spiral-vortex-color nil)
            val))))

(defcustom neomacs-spiral-vortex-speed 50
  "Rotation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-spiral-vortex)
                    (boundp 'neomacs-spiral-vortex)
                    neomacs-spiral-vortex)
           (neomacs-set-spiral-vortex t
            (if (boundp 'neomacs-spiral-vortex-color)
                neomacs-spiral-vortex-color nil)
            (if (boundp 'neomacs-spiral-vortex-arms)
                neomacs-spiral-vortex-arms nil)
            val))))

(defcustom neomacs-spiral-vortex-opacity 10
  "Spiral vortex opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-spiral-vortex)
                    (boundp 'neomacs-spiral-vortex)
                    neomacs-spiral-vortex)
           (neomacs-set-spiral-vortex t
            (if (boundp 'neomacs-spiral-vortex-color)
                neomacs-spiral-vortex-color nil)
            (if (boundp 'neomacs-spiral-vortex-arms)
                neomacs-spiral-vortex-arms nil)
            (if (boundp 'neomacs-spiral-vortex-speed)
                neomacs-spiral-vortex-speed nil)
            val))))

;; Cursor shockwave effect
(declare-function neomacs-set-cursor-shockwave "neomacsterm.c")

(defcustom neomacs-cursor-shockwave nil
  "Enable cursor shockwave effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-shockwave)
           (neomacs-set-cursor-shockwave val))))

(defcustom neomacs-cursor-shockwave-color "#FF9933"
  "Shockwave ring color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shockwave)
                    (boundp 'neomacs-cursor-shockwave)
                    neomacs-cursor-shockwave)
           (neomacs-set-cursor-shockwave t val))))

(defcustom neomacs-cursor-shockwave-radius 80
  "Max expansion radius in pixels."
  :type '(integer :tag "Radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shockwave)
                    (boundp 'neomacs-cursor-shockwave)
                    neomacs-cursor-shockwave)
           (neomacs-set-cursor-shockwave t
            (if (boundp 'neomacs-cursor-shockwave-color)
                neomacs-cursor-shockwave-color nil)
            val))))

(defcustom neomacs-cursor-shockwave-decay 200
  "Decay speed * 100."
  :type '(integer :tag "Decay speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shockwave)
                    (boundp 'neomacs-cursor-shockwave)
                    neomacs-cursor-shockwave)
           (neomacs-set-cursor-shockwave t
            (if (boundp 'neomacs-cursor-shockwave-color)
                neomacs-cursor-shockwave-color nil)
            (if (boundp 'neomacs-cursor-shockwave-radius)
                neomacs-cursor-shockwave-radius nil)
            val))))

(defcustom neomacs-cursor-shockwave-opacity 30
  "Shockwave opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-shockwave)
                    (boundp 'neomacs-cursor-shockwave)
                    neomacs-cursor-shockwave)
           (neomacs-set-cursor-shockwave t
            (if (boundp 'neomacs-cursor-shockwave-color)
                neomacs-cursor-shockwave-color nil)
            (if (boundp 'neomacs-cursor-shockwave-radius)
                neomacs-cursor-shockwave-radius nil)
            (if (boundp 'neomacs-cursor-shockwave-decay)
                neomacs-cursor-shockwave-decay nil)
            val))))

;; Diamond lattice overlay effect
(declare-function neomacs-set-diamond-lattice "neomacsterm.c")

(defcustom neomacs-diamond-lattice nil
  "Enable diamond lattice overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-diamond-lattice)
           (neomacs-set-diamond-lattice val))))

(defcustom neomacs-diamond-lattice-color "#B380E5"
  "Diamond lattice color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-diamond-lattice)
                    (boundp 'neomacs-diamond-lattice)
                    neomacs-diamond-lattice)
           (neomacs-set-diamond-lattice t val))))

(defcustom neomacs-diamond-lattice-cell-size 30
  "Diamond cell size in pixels."
  :type '(integer :tag "Cell size (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-diamond-lattice)
                    (boundp 'neomacs-diamond-lattice)
                    neomacs-diamond-lattice)
           (neomacs-set-diamond-lattice t
            (if (boundp 'neomacs-diamond-lattice-color)
                neomacs-diamond-lattice-color nil)
            val))))

(defcustom neomacs-diamond-lattice-shimmer-speed 80
  "Shimmer animation speed * 100."
  :type '(integer :tag "Shimmer speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-diamond-lattice)
                    (boundp 'neomacs-diamond-lattice)
                    neomacs-diamond-lattice)
           (neomacs-set-diamond-lattice t
            (if (boundp 'neomacs-diamond-lattice-color)
                neomacs-diamond-lattice-color nil)
            (if (boundp 'neomacs-diamond-lattice-cell-size)
                neomacs-diamond-lattice-cell-size nil)
            val))))

(defcustom neomacs-diamond-lattice-opacity 8
  "Diamond lattice opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-diamond-lattice)
                    (boundp 'neomacs-diamond-lattice)
                    neomacs-diamond-lattice)
           (neomacs-set-diamond-lattice t
            (if (boundp 'neomacs-diamond-lattice-color)
                neomacs-diamond-lattice-color nil)
            (if (boundp 'neomacs-diamond-lattice-cell-size)
                neomacs-diamond-lattice-cell-size nil)
            (if (boundp 'neomacs-diamond-lattice-shimmer-speed)
                neomacs-diamond-lattice-shimmer-speed nil)
            val))))

;; Cursor gravity well effect
(declare-function neomacs-set-cursor-gravity-well "neomacsterm.c")

(defcustom neomacs-cursor-gravity-well nil
  "Enable cursor gravity well effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-gravity-well)
           (neomacs-set-cursor-gravity-well val))))

(defcustom neomacs-cursor-gravity-well-color "#4D99FF"
  "Gravity field line color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-gravity-well)
                    (boundp 'neomacs-cursor-gravity-well)
                    neomacs-cursor-gravity-well)
           (neomacs-set-cursor-gravity-well t val))))

(defcustom neomacs-cursor-gravity-well-field-radius 80
  "Field radius in pixels."
  :type '(integer :tag "Radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-gravity-well)
                    (boundp 'neomacs-cursor-gravity-well)
                    neomacs-cursor-gravity-well)
           (neomacs-set-cursor-gravity-well t
            (if (boundp 'neomacs-cursor-gravity-well-color)
                neomacs-cursor-gravity-well-color nil)
            val))))

(defcustom neomacs-cursor-gravity-well-line-count 8
  "Number of field lines."
  :type '(integer :tag "Line count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-gravity-well)
                    (boundp 'neomacs-cursor-gravity-well)
                    neomacs-cursor-gravity-well)
           (neomacs-set-cursor-gravity-well t
            (if (boundp 'neomacs-cursor-gravity-well-color)
                neomacs-cursor-gravity-well-color nil)
            (if (boundp 'neomacs-cursor-gravity-well-field-radius)
                neomacs-cursor-gravity-well-field-radius nil)
            val))))

(defcustom neomacs-cursor-gravity-well-opacity 20
  "Gravity well opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-gravity-well)
                    (boundp 'neomacs-cursor-gravity-well)
                    neomacs-cursor-gravity-well)
           (neomacs-set-cursor-gravity-well t
            (if (boundp 'neomacs-cursor-gravity-well-color)
                neomacs-cursor-gravity-well-color nil)
            (if (boundp 'neomacs-cursor-gravity-well-field-radius)
                neomacs-cursor-gravity-well-field-radius nil)
            (if (boundp 'neomacs-cursor-gravity-well-line-count)
                neomacs-cursor-gravity-well-line-count nil)
            val))))

;; Tessellation overlay effect
(declare-function neomacs-set-tessellation "neomacsterm.c")

(defcustom neomacs-tessellation nil
  "Enable tessellation overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-tessellation)
           (if val
               (neomacs-set-tessellation t)
             (neomacs-set-tessellation nil)))))

(defcustom neomacs-tessellation-color "#8080B3"
  "Tessellation color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-tessellation)
                    (boundp 'neomacs-tessellation)
                    neomacs-tessellation)
           (neomacs-set-tessellation t val))))

(defcustom neomacs-tessellation-tile-size 40
  "Tessellation tile size in pixels."
  :type '(integer :tag "Tile size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-tessellation)
                    (boundp 'neomacs-tessellation)
                    neomacs-tessellation)
           (neomacs-set-tessellation t
            (if (boundp 'neomacs-tessellation-color)
                neomacs-tessellation-color nil)
            val))))

(defcustom neomacs-tessellation-rotation 0
  "Tessellation rotation angle * 100."
  :type '(integer :tag "Rotation (*100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-tessellation)
                    (boundp 'neomacs-tessellation)
                    neomacs-tessellation)
           (neomacs-set-tessellation t
            (if (boundp 'neomacs-tessellation-color)
                neomacs-tessellation-color nil)
            (if (boundp 'neomacs-tessellation-tile-size)
                neomacs-tessellation-tile-size nil)
            val))))

(defcustom neomacs-tessellation-opacity 4
  "Tessellation opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-tessellation)
                    (boundp 'neomacs-tessellation)
                    neomacs-tessellation)
           (neomacs-set-tessellation t
            (if (boundp 'neomacs-tessellation-color)
                neomacs-tessellation-color nil)
            (if (boundp 'neomacs-tessellation-tile-size)
                neomacs-tessellation-tile-size nil)
            (if (boundp 'neomacs-tessellation-rotation)
                neomacs-tessellation-rotation nil)
            val))))

;; Cursor water drop effect
(declare-function neomacs-set-cursor-water-drop "neomacsterm.c")

(defcustom neomacs-cursor-water-drop nil
  "Enable cursor water drop effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-water-drop)
           (if val
               (neomacs-set-cursor-water-drop t)
             (neomacs-set-cursor-water-drop nil)))))

(defcustom neomacs-cursor-water-drop-color "#4D99E6"
  "Cursor water drop color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-water-drop)
                    (boundp 'neomacs-cursor-water-drop)
                    neomacs-cursor-water-drop)
           (neomacs-set-cursor-water-drop t val))))

(defcustom neomacs-cursor-water-drop-ripple-count 4
  "Number of ripple rings."
  :type '(integer :tag "Ripple count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-water-drop)
                    (boundp 'neomacs-cursor-water-drop)
                    neomacs-cursor-water-drop)
           (neomacs-set-cursor-water-drop t
            (if (boundp 'neomacs-cursor-water-drop-color)
                neomacs-cursor-water-drop-color nil)
            val))))

(defcustom neomacs-cursor-water-drop-expand-speed 100
  "Ripple expansion speed * 100."
  :type '(integer :tag "Expand speed (*100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-water-drop)
                    (boundp 'neomacs-cursor-water-drop)
                    neomacs-cursor-water-drop)
           (neomacs-set-cursor-water-drop t
            (if (boundp 'neomacs-cursor-water-drop-color)
                neomacs-cursor-water-drop-color nil)
            (if (boundp 'neomacs-cursor-water-drop-ripple-count)
                neomacs-cursor-water-drop-ripple-count nil)
            val))))

(defcustom neomacs-cursor-water-drop-opacity 15
  "Water drop opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-water-drop)
                    (boundp 'neomacs-cursor-water-drop)
                    neomacs-cursor-water-drop)
           (neomacs-set-cursor-water-drop t
            (if (boundp 'neomacs-cursor-water-drop-color)
                neomacs-cursor-water-drop-color nil)
            (if (boundp 'neomacs-cursor-water-drop-ripple-count)
                neomacs-cursor-water-drop-ripple-count nil)
            (if (boundp 'neomacs-cursor-water-drop-expand-speed)
                neomacs-cursor-water-drop-expand-speed nil)
            val))))

;; Guilloche overlay effect
(declare-function neomacs-set-guilloche "neomacsterm.c")

(defcustom neomacs-guilloche nil
  "Enable guilloche overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-guilloche)
           (if val
               (neomacs-set-guilloche t)
             (neomacs-set-guilloche nil)))))

(defcustom neomacs-guilloche-color "#9966B3"
  "Guilloche color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-guilloche)
                    (boundp 'neomacs-guilloche)
                    neomacs-guilloche)
           (neomacs-set-guilloche t val))))

(defcustom neomacs-guilloche-curve-count 8
  "Number of overlapping curves."
  :type '(integer :tag "Curve count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-guilloche)
                    (boundp 'neomacs-guilloche)
                    neomacs-guilloche)
           (neomacs-set-guilloche t
            (if (boundp 'neomacs-guilloche-color)
                neomacs-guilloche-color nil)
            val))))

(defcustom neomacs-guilloche-wave-freq 100
  "Wave frequency * 100."
  :type '(integer :tag "Wave freq (*100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-guilloche)
                    (boundp 'neomacs-guilloche)
                    neomacs-guilloche)
           (neomacs-set-guilloche t
            (if (boundp 'neomacs-guilloche-color)
                neomacs-guilloche-color nil)
            (if (boundp 'neomacs-guilloche-curve-count)
                neomacs-guilloche-curve-count nil)
            val))))

(defcustom neomacs-guilloche-opacity 5
  "Guilloche opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-guilloche)
                    (boundp 'neomacs-guilloche)
                    neomacs-guilloche)
           (neomacs-set-guilloche t
            (if (boundp 'neomacs-guilloche-color)
                neomacs-guilloche-color nil)
            (if (boundp 'neomacs-guilloche-curve-count)
                neomacs-guilloche-curve-count nil)
            (if (boundp 'neomacs-guilloche-wave-freq)
                neomacs-guilloche-wave-freq nil)
            val))))

;; Cursor pixel dust effect
(declare-function neomacs-set-cursor-pixel-dust "neomacsterm.c")

(defcustom neomacs-cursor-pixel-dust nil
  "Enable cursor pixel dust effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-pixel-dust)
           (if val
               (neomacs-set-cursor-pixel-dust t)
             (neomacs-set-cursor-pixel-dust nil)))))

(defcustom neomacs-cursor-pixel-dust-color "#CCCC99"
  "Cursor pixel dust color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pixel-dust)
                    (boundp 'neomacs-cursor-pixel-dust)
                    neomacs-cursor-pixel-dust)
           (neomacs-set-cursor-pixel-dust t val))))

(defcustom neomacs-cursor-pixel-dust-count 15
  "Number of dust particles."
  :type '(integer :tag "Dust count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pixel-dust)
                    (boundp 'neomacs-cursor-pixel-dust)
                    neomacs-cursor-pixel-dust)
           (neomacs-set-cursor-pixel-dust t
            (if (boundp 'neomacs-cursor-pixel-dust-color)
                neomacs-cursor-pixel-dust-color nil)
            val))))

(defcustom neomacs-cursor-pixel-dust-scatter-speed 100
  "Dust scatter speed * 100."
  :type '(integer :tag "Scatter speed (*100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pixel-dust)
                    (boundp 'neomacs-cursor-pixel-dust)
                    neomacs-cursor-pixel-dust)
           (neomacs-set-cursor-pixel-dust t
            (if (boundp 'neomacs-cursor-pixel-dust-color)
                neomacs-cursor-pixel-dust-color nil)
            (if (boundp 'neomacs-cursor-pixel-dust-count)
                neomacs-cursor-pixel-dust-count nil)
            val))))

(defcustom neomacs-cursor-pixel-dust-opacity 20
  "Pixel dust opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pixel-dust)
                    (boundp 'neomacs-cursor-pixel-dust)
                    neomacs-cursor-pixel-dust)
           (neomacs-set-cursor-pixel-dust t
            (if (boundp 'neomacs-cursor-pixel-dust-color)
                neomacs-cursor-pixel-dust-color nil)
            (if (boundp 'neomacs-cursor-pixel-dust-count)
                neomacs-cursor-pixel-dust-count nil)
            (if (boundp 'neomacs-cursor-pixel-dust-scatter-speed)
                neomacs-cursor-pixel-dust-scatter-speed nil)
            val))))

;; Celtic knot overlay effect
(declare-function neomacs-set-celtic-knot "neomacsterm.c")

(defcustom neomacs-celtic-knot nil
  "Enable celtic knot overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-celtic-knot)
           (if val
               (neomacs-set-celtic-knot t)
             (neomacs-set-celtic-knot nil)))))

(defcustom neomacs-celtic-knot-color "#009933"
  "Celtic knot color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-celtic-knot)
                    (boundp 'neomacs-celtic-knot)
                    neomacs-celtic-knot)
           (neomacs-set-celtic-knot t val))))

(defcustom neomacs-celtic-knot-scale 60
  "Celtic knot cell size in pixels."
  :type '(integer :tag "Knot scale (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-celtic-knot)
                    (boundp 'neomacs-celtic-knot)
                    neomacs-celtic-knot)
           (neomacs-set-celtic-knot t
            (if (boundp 'neomacs-celtic-knot-color)
                neomacs-celtic-knot-color nil)
            val))))

(defcustom neomacs-celtic-knot-weave-speed 100
  "Celtic knot weave animation speed (* 100)."
  :type '(integer :tag "Weave speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-celtic-knot)
                    (boundp 'neomacs-celtic-knot)
                    neomacs-celtic-knot)
           (neomacs-set-celtic-knot t
            (if (boundp 'neomacs-celtic-knot-color)
                neomacs-celtic-knot-color nil)
            (if (boundp 'neomacs-celtic-knot-scale)
                neomacs-celtic-knot-scale nil)
            val))))

(defcustom neomacs-celtic-knot-opacity 6
  "Celtic knot opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-celtic-knot)
                    (boundp 'neomacs-celtic-knot)
                    neomacs-celtic-knot)
           (neomacs-set-celtic-knot t
            (if (boundp 'neomacs-celtic-knot-color)
                neomacs-celtic-knot-color nil)
            (if (boundp 'neomacs-celtic-knot-scale)
                neomacs-celtic-knot-scale nil)
            (if (boundp 'neomacs-celtic-knot-weave-speed)
                neomacs-celtic-knot-weave-speed nil)
            val))))

;; Cursor candle flame effect
(declare-function neomacs-set-cursor-candle-flame "neomacsterm.c")

(defcustom neomacs-cursor-candle-flame nil
  "Enable cursor candle flame effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-candle-flame)
           (if val
               (neomacs-set-cursor-candle-flame t)
             (neomacs-set-cursor-candle-flame nil)))))

(defcustom neomacs-cursor-candle-flame-color "#FFB333"
  "Cursor candle flame color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-candle-flame)
                    (boundp 'neomacs-cursor-candle-flame)
                    neomacs-cursor-candle-flame)
           (neomacs-set-cursor-candle-flame t val))))

(defcustom neomacs-cursor-candle-flame-height 20
  "Candle flame height in pixels."
  :type '(integer :tag "Flame height (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-candle-flame)
                    (boundp 'neomacs-cursor-candle-flame)
                    neomacs-cursor-candle-flame)
           (neomacs-set-cursor-candle-flame t
            (if (boundp 'neomacs-cursor-candle-flame-color)
                neomacs-cursor-candle-flame-color nil)
            val))))

(defcustom neomacs-cursor-candle-flame-flicker-speed 100
  "Candle flame flicker speed (* 100)."
  :type '(integer :tag "Flicker speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-candle-flame)
                    (boundp 'neomacs-cursor-candle-flame)
                    neomacs-cursor-candle-flame)
           (neomacs-set-cursor-candle-flame t
            (if (boundp 'neomacs-cursor-candle-flame-color)
                neomacs-cursor-candle-flame-color nil)
            (if (boundp 'neomacs-cursor-candle-flame-height)
                neomacs-cursor-candle-flame-height nil)
            val))))

(defcustom neomacs-cursor-candle-flame-opacity 20
  "Candle flame opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-candle-flame)
                    (boundp 'neomacs-cursor-candle-flame)
                    neomacs-cursor-candle-flame)
           (neomacs-set-cursor-candle-flame t
            (if (boundp 'neomacs-cursor-candle-flame-color)
                neomacs-cursor-candle-flame-color nil)
            (if (boundp 'neomacs-cursor-candle-flame-height)
                neomacs-cursor-candle-flame-height nil)
            (if (boundp 'neomacs-cursor-candle-flame-flicker-speed)
                neomacs-cursor-candle-flame-flicker-speed nil)
            val))))

;; Argyle pattern overlay effect
(declare-function neomacs-set-argyle-pattern "neomacsterm.c")

(defcustom neomacs-argyle-pattern nil
  "Enable argyle pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-argyle-pattern)
           (if val
               (neomacs-set-argyle-pattern t)
             (neomacs-set-argyle-pattern nil)))))

(defcustom neomacs-argyle-pattern-color "#804D4D"
  "Argyle pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-argyle-pattern)
                    (boundp 'neomacs-argyle-pattern)
                    neomacs-argyle-pattern)
           (neomacs-set-argyle-pattern t val))))

(defcustom neomacs-argyle-pattern-diamond-size 30
  "Argyle diamond size in pixels."
  :type '(integer :tag "Diamond size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-argyle-pattern)
                    (boundp 'neomacs-argyle-pattern)
                    neomacs-argyle-pattern)
           (neomacs-set-argyle-pattern t
            (if (boundp 'neomacs-argyle-pattern-color)
                neomacs-argyle-pattern-color nil)
            val))))

(defcustom neomacs-argyle-pattern-line-width 1
  "Argyle diagonal line width in pixels."
  :type '(integer :tag "Line width (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-argyle-pattern)
                    (boundp 'neomacs-argyle-pattern)
                    neomacs-argyle-pattern)
           (neomacs-set-argyle-pattern t
            (if (boundp 'neomacs-argyle-pattern-color)
                neomacs-argyle-pattern-color nil)
            (if (boundp 'neomacs-argyle-pattern-diamond-size)
                neomacs-argyle-pattern-diamond-size nil)
            val))))

(defcustom neomacs-argyle-pattern-opacity 5
  "Argyle pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-argyle-pattern)
                    (boundp 'neomacs-argyle-pattern)
                    neomacs-argyle-pattern)
           (neomacs-set-argyle-pattern t
            (if (boundp 'neomacs-argyle-pattern-color)
                neomacs-argyle-pattern-color nil)
            (if (boundp 'neomacs-argyle-pattern-diamond-size)
                neomacs-argyle-pattern-diamond-size nil)
            (if (boundp 'neomacs-argyle-pattern-line-width)
                neomacs-argyle-pattern-line-width nil)
            val))))

;; Cursor moth flame effect
(declare-function neomacs-set-cursor-moth-flame "neomacsterm.c")

(defcustom neomacs-cursor-moth-flame nil
  "Enable cursor moth flame effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-moth-flame)
           (if val
               (neomacs-set-cursor-moth-flame t)
             (neomacs-set-cursor-moth-flame nil)))))

(defcustom neomacs-cursor-moth-flame-color "#CCB380"
  "Cursor moth flame color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth-flame)
                    (boundp 'neomacs-cursor-moth-flame)
                    neomacs-cursor-moth-flame)
           (neomacs-set-cursor-moth-flame t val))))

(defcustom neomacs-cursor-moth-flame-count 5
  "Number of moths."
  :type '(integer :tag "Moth count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth-flame)
                    (boundp 'neomacs-cursor-moth-flame)
                    neomacs-cursor-moth-flame)
           (neomacs-set-cursor-moth-flame t
            (if (boundp 'neomacs-cursor-moth-flame-color)
                neomacs-cursor-moth-flame-color nil)
            val))))

(defcustom neomacs-cursor-moth-flame-orbit-speed 100
  "Moth orbit speed (* 100)."
  :type '(integer :tag "Orbit speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth-flame)
                    (boundp 'neomacs-cursor-moth-flame)
                    neomacs-cursor-moth-flame)
           (neomacs-set-cursor-moth-flame t
            (if (boundp 'neomacs-cursor-moth-flame-color)
                neomacs-cursor-moth-flame-color nil)
            (if (boundp 'neomacs-cursor-moth-flame-count)
                neomacs-cursor-moth-flame-count nil)
            val))))

(defcustom neomacs-cursor-moth-flame-opacity 18
  "Cursor moth flame opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth-flame)
                    (boundp 'neomacs-cursor-moth-flame)
                    neomacs-cursor-moth-flame)
           (neomacs-set-cursor-moth-flame t
            (if (boundp 'neomacs-cursor-moth-flame-color)
                neomacs-cursor-moth-flame-color nil)
            (if (boundp 'neomacs-cursor-moth-flame-count)
                neomacs-cursor-moth-flame-count nil)
            (if (boundp 'neomacs-cursor-moth-flame-orbit-speed)
                neomacs-cursor-moth-flame-orbit-speed nil)
            val))))

;; Basket weave overlay effect
(declare-function neomacs-set-basket-weave "neomacsterm.c")

(defcustom neomacs-basket-weave nil
  "Enable basket weave overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-basket-weave)
           (if val
               (neomacs-set-basket-weave t)
             (neomacs-set-basket-weave nil)))))

(defcustom neomacs-basket-weave-color "#8C6640"
  "Basket weave color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-basket-weave)
                    (boundp 'neomacs-basket-weave)
                    neomacs-basket-weave)
           (neomacs-set-basket-weave t val))))

(defcustom neomacs-basket-weave-strip-width 6
  "Basket weave strip width in pixels."
  :type '(integer :tag "Strip width (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-basket-weave)
                    (boundp 'neomacs-basket-weave)
                    neomacs-basket-weave)
           (neomacs-set-basket-weave t
            (if (boundp 'neomacs-basket-weave-color)
                neomacs-basket-weave-color nil)
            val))))

(defcustom neomacs-basket-weave-strip-spacing 20
  "Basket weave strip spacing in pixels."
  :type '(integer :tag "Strip spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-basket-weave)
                    (boundp 'neomacs-basket-weave)
                    neomacs-basket-weave)
           (neomacs-set-basket-weave t
            (if (boundp 'neomacs-basket-weave-color)
                neomacs-basket-weave-color nil)
            (if (boundp 'neomacs-basket-weave-strip-width)
                neomacs-basket-weave-strip-width nil)
            val))))

(defcustom neomacs-basket-weave-opacity 5
  "Basket weave opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-basket-weave)
                    (boundp 'neomacs-basket-weave)
                    neomacs-basket-weave)
           (neomacs-set-basket-weave t
            (if (boundp 'neomacs-basket-weave-color)
                neomacs-basket-weave-color nil)
            (if (boundp 'neomacs-basket-weave-strip-width)
                neomacs-basket-weave-strip-width nil)
            (if (boundp 'neomacs-basket-weave-strip-spacing)
                neomacs-basket-weave-strip-spacing nil)
            val))))

;; Cursor sparkler effect
(declare-function neomacs-set-cursor-sparkler "neomacsterm.c")

(defcustom neomacs-cursor-sparkler nil
  "Enable cursor sparkler effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-sparkler)
           (if val
               (neomacs-set-cursor-sparkler t)
             (neomacs-set-cursor-sparkler nil)))))

(defcustom neomacs-cursor-sparkler-color "#FFD94D"
  "Cursor sparkler color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkler)
                    (boundp 'neomacs-cursor-sparkler)
                    neomacs-cursor-sparkler)
           (neomacs-set-cursor-sparkler t val))))

(defcustom neomacs-cursor-sparkler-spark-count 12
  "Number of sparkler sparks."
  :type '(integer :tag "Spark count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkler)
                    (boundp 'neomacs-cursor-sparkler)
                    neomacs-cursor-sparkler)
           (neomacs-set-cursor-sparkler t
            (if (boundp 'neomacs-cursor-sparkler-color)
                neomacs-cursor-sparkler-color nil)
            val))))

(defcustom neomacs-cursor-sparkler-burn-speed 100
  "Cursor sparkler burn speed (* 100)."
  :type '(integer :tag "Burn speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkler)
                    (boundp 'neomacs-cursor-sparkler)
                    neomacs-cursor-sparkler)
           (neomacs-set-cursor-sparkler t
            (if (boundp 'neomacs-cursor-sparkler-color)
                neomacs-cursor-sparkler-color nil)
            (if (boundp 'neomacs-cursor-sparkler-spark-count)
                neomacs-cursor-sparkler-spark-count nil)
            val))))

(defcustom neomacs-cursor-sparkler-opacity 25
  "Cursor sparkler opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkler)
                    (boundp 'neomacs-cursor-sparkler)
                    neomacs-cursor-sparkler)
           (neomacs-set-cursor-sparkler t
            (if (boundp 'neomacs-cursor-sparkler-color)
                neomacs-cursor-sparkler-color nil)
            (if (boundp 'neomacs-cursor-sparkler-spark-count)
                neomacs-cursor-sparkler-spark-count nil)
            (if (boundp 'neomacs-cursor-sparkler-burn-speed)
                neomacs-cursor-sparkler-burn-speed nil)
            val))))

;; Fish scale overlay effect
(declare-function neomacs-set-fish-scale "neomacsterm.c")

(defcustom neomacs-fish-scale nil
  "Enable fish scale overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-fish-scale)
           (if val
               (neomacs-set-fish-scale t)
             (neomacs-set-fish-scale nil)))))

(defcustom neomacs-fish-scale-color "#4D99B3"
  "Fish scale color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-fish-scale)
                    (boundp 'neomacs-fish-scale)
                    neomacs-fish-scale)
           (neomacs-set-fish-scale t val))))

(defcustom neomacs-fish-scale-size 16
  "Fish scale size in pixels."
  :type '(integer :tag "Scale size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-fish-scale)
                    (boundp 'neomacs-fish-scale)
                    neomacs-fish-scale)
           (neomacs-set-fish-scale t
            (if (boundp 'neomacs-fish-scale-color)
                neomacs-fish-scale-color nil)
            val))))

(defcustom neomacs-fish-scale-row-offset 50
  "Fish scale row offset (* 100)."
  :type '(integer :tag "Row offset (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-fish-scale)
                    (boundp 'neomacs-fish-scale)
                    neomacs-fish-scale)
           (neomacs-set-fish-scale t
            (if (boundp 'neomacs-fish-scale-color)
                neomacs-fish-scale-color nil)
            (if (boundp 'neomacs-fish-scale-size)
                neomacs-fish-scale-size nil)
            val))))

(defcustom neomacs-fish-scale-opacity 4
  "Fish scale opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-fish-scale)
                    (boundp 'neomacs-fish-scale)
                    neomacs-fish-scale)
           (neomacs-set-fish-scale t
            (if (boundp 'neomacs-fish-scale-color)
                neomacs-fish-scale-color nil)
            (if (boundp 'neomacs-fish-scale-size)
                neomacs-fish-scale-size nil)
            (if (boundp 'neomacs-fish-scale-row-offset)
                neomacs-fish-scale-row-offset nil)
            val))))

;; Cursor plasma ball effect
(declare-function neomacs-set-cursor-plasma-ball "neomacsterm.c")

(defcustom neomacs-cursor-plasma-ball nil
  "Enable cursor plasma ball effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-plasma-ball)
           (if val
               (neomacs-set-cursor-plasma-ball t)
             (neomacs-set-cursor-plasma-ball nil)))))

(defcustom neomacs-cursor-plasma-ball-color "#B34DFF"
  "Cursor plasma ball color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-plasma-ball)
                    (boundp 'neomacs-cursor-plasma-ball)
                    neomacs-cursor-plasma-ball)
           (neomacs-set-cursor-plasma-ball t val))))

(defcustom neomacs-cursor-plasma-ball-tendril-count 6
  "Number of plasma ball tendrils."
  :type '(integer :tag "Tendril count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-plasma-ball)
                    (boundp 'neomacs-cursor-plasma-ball)
                    neomacs-cursor-plasma-ball)
           (neomacs-set-cursor-plasma-ball t
            (if (boundp 'neomacs-cursor-plasma-ball-color)
                neomacs-cursor-plasma-ball-color nil)
            val))))

(defcustom neomacs-cursor-plasma-ball-arc-speed 100
  "Cursor plasma ball arc speed (* 100)."
  :type '(integer :tag "Arc speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-plasma-ball)
                    (boundp 'neomacs-cursor-plasma-ball)
                    neomacs-cursor-plasma-ball)
           (neomacs-set-cursor-plasma-ball t
            (if (boundp 'neomacs-cursor-plasma-ball-color)
                neomacs-cursor-plasma-ball-color nil)
            (if (boundp 'neomacs-cursor-plasma-ball-tendril-count)
                neomacs-cursor-plasma-ball-tendril-count nil)
            val))))

(defcustom neomacs-cursor-plasma-ball-opacity 20
  "Cursor plasma ball opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-plasma-ball)
                    (boundp 'neomacs-cursor-plasma-ball)
                    neomacs-cursor-plasma-ball)
           (neomacs-set-cursor-plasma-ball t
            (if (boundp 'neomacs-cursor-plasma-ball-color)
                neomacs-cursor-plasma-ball-color nil)
            (if (boundp 'neomacs-cursor-plasma-ball-tendril-count)
                neomacs-cursor-plasma-ball-tendril-count nil)
            (if (boundp 'neomacs-cursor-plasma-ball-arc-speed)
                neomacs-cursor-plasma-ball-arc-speed nil)
            val))))

;; Trefoil knot overlay effect
(declare-function neomacs-set-trefoil-knot "neomacsterm.c")

(defcustom neomacs-trefoil-knot nil
  "Enable trefoil knot overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-trefoil-knot)
           (if val
               (neomacs-set-trefoil-knot t)
             (neomacs-set-trefoil-knot nil)))))

(defcustom neomacs-trefoil-knot-color "#6699E6"
  "Trefoil knot color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-trefoil-knot)
                    (boundp 'neomacs-trefoil-knot)
                    neomacs-trefoil-knot)
           (neomacs-set-trefoil-knot t val))))

(defcustom neomacs-trefoil-knot-size 80
  "Trefoil knot size in pixels."
  :type '(integer :tag "Knot size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-trefoil-knot)
                    (boundp 'neomacs-trefoil-knot)
                    neomacs-trefoil-knot)
           (neomacs-set-trefoil-knot t
            (if (boundp 'neomacs-trefoil-knot-color)
                neomacs-trefoil-knot-color nil)
            val))))

(defcustom neomacs-trefoil-knot-rotation-speed 100
  "Trefoil knot rotation speed (* 100)."
  :type '(integer :tag "Rotation speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-trefoil-knot)
                    (boundp 'neomacs-trefoil-knot)
                    neomacs-trefoil-knot)
           (neomacs-set-trefoil-knot t
            (if (boundp 'neomacs-trefoil-knot-color)
                neomacs-trefoil-knot-color nil)
            (if (boundp 'neomacs-trefoil-knot-size)
                neomacs-trefoil-knot-size nil)
            val))))

(defcustom neomacs-trefoil-knot-opacity 6
  "Trefoil knot opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-trefoil-knot)
                    (boundp 'neomacs-trefoil-knot)
                    neomacs-trefoil-knot)
           (neomacs-set-trefoil-knot t
            (if (boundp 'neomacs-trefoil-knot-color)
                neomacs-trefoil-knot-color nil)
            (if (boundp 'neomacs-trefoil-knot-size)
                neomacs-trefoil-knot-size nil)
            (if (boundp 'neomacs-trefoil-knot-rotation-speed)
                neomacs-trefoil-knot-rotation-speed nil)
            val))))

;; Cursor quill pen effect
(declare-function neomacs-set-cursor-quill-pen "neomacsterm.c")

(defcustom neomacs-cursor-quill-pen nil
  "Enable cursor quill pen effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-quill-pen)
           (if val
               (neomacs-set-cursor-quill-pen t)
             (neomacs-set-cursor-quill-pen nil)))))

(defcustom neomacs-cursor-quill-pen-color "#4D260D"
  "Cursor quill pen color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-quill-pen)
                    (boundp 'neomacs-cursor-quill-pen)
                    neomacs-cursor-quill-pen)
           (neomacs-set-cursor-quill-pen t val))))

(defcustom neomacs-cursor-quill-pen-trail-length 8
  "Number of ink drips in quill trail."
  :type '(integer :tag "Trail length")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-quill-pen)
                    (boundp 'neomacs-cursor-quill-pen)
                    neomacs-cursor-quill-pen)
           (neomacs-set-cursor-quill-pen t
            (if (boundp 'neomacs-cursor-quill-pen-color)
                neomacs-cursor-quill-pen-color nil)
            val))))

(defcustom neomacs-cursor-quill-pen-ink-speed 100
  "Cursor quill pen ink speed (* 100)."
  :type '(integer :tag "Ink speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-quill-pen)
                    (boundp 'neomacs-cursor-quill-pen)
                    neomacs-cursor-quill-pen)
           (neomacs-set-cursor-quill-pen t
            (if (boundp 'neomacs-cursor-quill-pen-color)
                neomacs-cursor-quill-pen-color nil)
            (if (boundp 'neomacs-cursor-quill-pen-trail-length)
                neomacs-cursor-quill-pen-trail-length nil)
            val))))

(defcustom neomacs-cursor-quill-pen-opacity 20
  "Cursor quill pen opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-quill-pen)
                    (boundp 'neomacs-cursor-quill-pen)
                    neomacs-cursor-quill-pen)
           (neomacs-set-cursor-quill-pen t
            (if (boundp 'neomacs-cursor-quill-pen-color)
                neomacs-cursor-quill-pen-color nil)
            (if (boundp 'neomacs-cursor-quill-pen-trail-length)
                neomacs-cursor-quill-pen-trail-length nil)
            (if (boundp 'neomacs-cursor-quill-pen-ink-speed)
                neomacs-cursor-quill-pen-ink-speed nil)
            val))))

;; Herringbone pattern overlay effect
(declare-function neomacs-set-herringbone-pattern "neomacsterm.c")

(defcustom neomacs-herringbone-pattern nil
  "Enable herringbone pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-herringbone-pattern)
           (if val
               (neomacs-set-herringbone-pattern t)
             (neomacs-set-herringbone-pattern nil)))))

(defcustom neomacs-herringbone-pattern-color "#998066"
  "Herringbone pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-herringbone-pattern)
                    (boundp 'neomacs-herringbone-pattern)
                    neomacs-herringbone-pattern)
           (neomacs-set-herringbone-pattern t val))))

(defcustom neomacs-herringbone-pattern-tile-width 20
  "Herringbone tile width in pixels."
  :type '(integer :tag "Tile width (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-herringbone-pattern)
                    (boundp 'neomacs-herringbone-pattern)
                    neomacs-herringbone-pattern)
           (neomacs-set-herringbone-pattern t
            (if (boundp 'neomacs-herringbone-pattern-color)
                neomacs-herringbone-pattern-color nil)
            val))))

(defcustom neomacs-herringbone-pattern-tile-height 10
  "Herringbone tile height in pixels."
  :type '(integer :tag "Tile height (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-herringbone-pattern)
                    (boundp 'neomacs-herringbone-pattern)
                    neomacs-herringbone-pattern)
           (neomacs-set-herringbone-pattern t
            (if (boundp 'neomacs-herringbone-pattern-color)
                neomacs-herringbone-pattern-color nil)
            (if (boundp 'neomacs-herringbone-pattern-tile-width)
                neomacs-herringbone-pattern-tile-width nil)
            val))))

(defcustom neomacs-herringbone-pattern-opacity 5
  "Herringbone pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-herringbone-pattern)
                    (boundp 'neomacs-herringbone-pattern)
                    neomacs-herringbone-pattern)
           (neomacs-set-herringbone-pattern t
            (if (boundp 'neomacs-herringbone-pattern-color)
                neomacs-herringbone-pattern-color nil)
            (if (boundp 'neomacs-herringbone-pattern-tile-width)
                neomacs-herringbone-pattern-tile-width nil)
            (if (boundp 'neomacs-herringbone-pattern-tile-height)
                neomacs-herringbone-pattern-tile-height nil)
            val))))

;; Cursor aurora borealis effect
(declare-function neomacs-set-cursor-aurora-borealis "neomacsterm.c")

(defcustom neomacs-cursor-aurora-borealis nil
  "Enable cursor aurora borealis effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-aurora-borealis)
           (if val
               (neomacs-set-cursor-aurora-borealis t)
             (neomacs-set-cursor-aurora-borealis nil)))))

(defcustom neomacs-cursor-aurora-borealis-color "#33E680"
  "Cursor aurora borealis color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-aurora-borealis)
                    (boundp 'neomacs-cursor-aurora-borealis)
                    neomacs-cursor-aurora-borealis)
           (neomacs-set-cursor-aurora-borealis t val))))

(defcustom neomacs-cursor-aurora-borealis-band-count 5
  "Number of aurora bands."
  :type '(integer :tag "Band count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-aurora-borealis)
                    (boundp 'neomacs-cursor-aurora-borealis)
                    neomacs-cursor-aurora-borealis)
           (neomacs-set-cursor-aurora-borealis t
            (if (boundp 'neomacs-cursor-aurora-borealis-color)
                neomacs-cursor-aurora-borealis-color nil)
            val))))

(defcustom neomacs-cursor-aurora-borealis-shimmer-speed 100
  "Cursor aurora borealis shimmer speed (* 100)."
  :type '(integer :tag "Shimmer speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-aurora-borealis)
                    (boundp 'neomacs-cursor-aurora-borealis)
                    neomacs-cursor-aurora-borealis)
           (neomacs-set-cursor-aurora-borealis t
            (if (boundp 'neomacs-cursor-aurora-borealis-color)
                neomacs-cursor-aurora-borealis-color nil)
            (if (boundp 'neomacs-cursor-aurora-borealis-band-count)
                neomacs-cursor-aurora-borealis-band-count nil)
            val))))

(defcustom neomacs-cursor-aurora-borealis-opacity 15
  "Cursor aurora borealis opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-aurora-borealis)
                    (boundp 'neomacs-cursor-aurora-borealis)
                    neomacs-cursor-aurora-borealis)
           (neomacs-set-cursor-aurora-borealis t
            (if (boundp 'neomacs-cursor-aurora-borealis-color)
                neomacs-cursor-aurora-borealis-color nil)
            (if (boundp 'neomacs-cursor-aurora-borealis-band-count)
                neomacs-cursor-aurora-borealis-band-count nil)
            (if (boundp 'neomacs-cursor-aurora-borealis-shimmer-speed)
                neomacs-cursor-aurora-borealis-shimmer-speed nil)
            val))))

;; Target reticle overlay effect
(declare-function neomacs-set-target-reticle "neomacsterm.c")

(defcustom neomacs-target-reticle nil
  "Enable target reticle overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-target-reticle)
           (if val
               (neomacs-set-target-reticle t)
             (neomacs-set-target-reticle nil)))))

(defcustom neomacs-target-reticle-color "#33CC33"
  "Target reticle color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-target-reticle)
                    (boundp 'neomacs-target-reticle)
                    neomacs-target-reticle)
           (neomacs-set-target-reticle t val))))

(defcustom neomacs-target-reticle-ring-count 3
  "Number of reticle rings."
  :type '(integer :tag "Ring count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-target-reticle)
                    (boundp 'neomacs-target-reticle)
                    neomacs-target-reticle)
           (neomacs-set-target-reticle t
            (if (boundp 'neomacs-target-reticle-color)
                neomacs-target-reticle-color nil)
            val))))

(defcustom neomacs-target-reticle-pulse-speed 100
  "Target reticle pulse speed (* 100)."
  :type '(integer :tag "Pulse speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-target-reticle)
                    (boundp 'neomacs-target-reticle)
                    neomacs-target-reticle)
           (neomacs-set-target-reticle t
            (if (boundp 'neomacs-target-reticle-color)
                neomacs-target-reticle-color nil)
            (if (boundp 'neomacs-target-reticle-ring-count)
                neomacs-target-reticle-ring-count nil)
            val))))

(defcustom neomacs-target-reticle-opacity 8
  "Target reticle opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-target-reticle)
                    (boundp 'neomacs-target-reticle)
                    neomacs-target-reticle)
           (neomacs-set-target-reticle t
            (if (boundp 'neomacs-target-reticle-color)
                neomacs-target-reticle-color nil)
            (if (boundp 'neomacs-target-reticle-ring-count)
                neomacs-target-reticle-ring-count nil)
            (if (boundp 'neomacs-target-reticle-pulse-speed)
                neomacs-target-reticle-pulse-speed nil)
            val))))

;; Cursor feather effect
(declare-function neomacs-set-cursor-feather "neomacsterm.c")

(defcustom neomacs-cursor-feather nil
  "Enable cursor feather effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-feather)
           (if val
               (neomacs-set-cursor-feather t)
             (neomacs-set-cursor-feather nil)))))

(defcustom neomacs-cursor-feather-color "#E6D9B3"
  "Cursor feather color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-feather)
                    (boundp 'neomacs-cursor-feather)
                    neomacs-cursor-feather)
           (neomacs-set-cursor-feather t val))))

(defcustom neomacs-cursor-feather-count 4
  "Number of feather wisps."
  :type '(integer :tag "Feather count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-feather)
                    (boundp 'neomacs-cursor-feather)
                    neomacs-cursor-feather)
           (neomacs-set-cursor-feather t
            (if (boundp 'neomacs-cursor-feather-color)
                neomacs-cursor-feather-color nil)
            val))))

(defcustom neomacs-cursor-feather-drift-speed 100
  "Cursor feather drift speed (* 100)."
  :type '(integer :tag "Drift speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-feather)
                    (boundp 'neomacs-cursor-feather)
                    neomacs-cursor-feather)
           (neomacs-set-cursor-feather t
            (if (boundp 'neomacs-cursor-feather-color)
                neomacs-cursor-feather-color nil)
            (if (boundp 'neomacs-cursor-feather-count)
                neomacs-cursor-feather-count nil)
            val))))

(defcustom neomacs-cursor-feather-opacity 18
  "Cursor feather opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-feather)
                    (boundp 'neomacs-cursor-feather)
                    neomacs-cursor-feather)
           (neomacs-set-cursor-feather t
            (if (boundp 'neomacs-cursor-feather-color)
                neomacs-cursor-feather-color nil)
            (if (boundp 'neomacs-cursor-feather-count)
                neomacs-cursor-feather-count nil)
            (if (boundp 'neomacs-cursor-feather-drift-speed)
                neomacs-cursor-feather-drift-speed nil)
            val))))

;; Plaid pattern overlay effect
(declare-function neomacs-set-plaid-pattern "neomacsterm.c")

(defcustom neomacs-plaid-pattern nil
  "Enable plaid pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-plaid-pattern)
           (if val
               (neomacs-set-plaid-pattern t)
             (neomacs-set-plaid-pattern nil)))))

(defcustom neomacs-plaid-pattern-color "#B34D4D"
  "Plaid pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plaid-pattern)
                    (boundp 'neomacs-plaid-pattern)
                    neomacs-plaid-pattern)
           (neomacs-set-plaid-pattern t val))))

(defcustom neomacs-plaid-pattern-band-width 4
  "Plaid band width in pixels."
  :type '(integer :tag "Band width (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plaid-pattern)
                    (boundp 'neomacs-plaid-pattern)
                    neomacs-plaid-pattern)
           (neomacs-set-plaid-pattern t
            (if (boundp 'neomacs-plaid-pattern-color)
                neomacs-plaid-pattern-color nil)
            val))))

(defcustom neomacs-plaid-pattern-band-spacing 30
  "Plaid band spacing in pixels."
  :type '(integer :tag "Band spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plaid-pattern)
                    (boundp 'neomacs-plaid-pattern)
                    neomacs-plaid-pattern)
           (neomacs-set-plaid-pattern t
            (if (boundp 'neomacs-plaid-pattern-color)
                neomacs-plaid-pattern-color nil)
            (if (boundp 'neomacs-plaid-pattern-band-width)
                neomacs-plaid-pattern-band-width nil)
            val))))

(defcustom neomacs-plaid-pattern-opacity 5
  "Plaid pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-plaid-pattern)
                    (boundp 'neomacs-plaid-pattern)
                    neomacs-plaid-pattern)
           (neomacs-set-plaid-pattern t
            (if (boundp 'neomacs-plaid-pattern-color)
                neomacs-plaid-pattern-color nil)
            (if (boundp 'neomacs-plaid-pattern-band-width)
                neomacs-plaid-pattern-band-width nil)
            (if (boundp 'neomacs-plaid-pattern-band-spacing)
                neomacs-plaid-pattern-band-spacing nil)
            val))))

;; Cursor stardust effect
(declare-function neomacs-set-cursor-stardust "neomacsterm.c")

(defcustom neomacs-cursor-stardust nil
  "Enable cursor stardust effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-stardust)
           (if val
               (neomacs-set-cursor-stardust t)
             (neomacs-set-cursor-stardust nil)))))

(defcustom neomacs-cursor-stardust-color "#FFE680"
  "Cursor stardust color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-stardust)
                    (boundp 'neomacs-cursor-stardust)
                    neomacs-cursor-stardust)
           (neomacs-set-cursor-stardust t val))))

(defcustom neomacs-cursor-stardust-particle-count 20
  "Number of stardust particles."
  :type '(integer :tag "Particle count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-stardust)
                    (boundp 'neomacs-cursor-stardust)
                    neomacs-cursor-stardust)
           (neomacs-set-cursor-stardust t
            (if (boundp 'neomacs-cursor-stardust-color)
                neomacs-cursor-stardust-color nil)
            val))))

(defcustom neomacs-cursor-stardust-fall-speed 100
  "Cursor stardust fall speed (* 100)."
  :type '(integer :tag "Fall speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-stardust)
                    (boundp 'neomacs-cursor-stardust)
                    neomacs-cursor-stardust)
           (neomacs-set-cursor-stardust t
            (if (boundp 'neomacs-cursor-stardust-color)
                neomacs-cursor-stardust-color nil)
            (if (boundp 'neomacs-cursor-stardust-particle-count)
                neomacs-cursor-stardust-particle-count nil)
            val))))

(defcustom neomacs-cursor-stardust-opacity 20
  "Cursor stardust opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-stardust)
                    (boundp 'neomacs-cursor-stardust)
                    neomacs-cursor-stardust)
           (neomacs-set-cursor-stardust t
            (if (boundp 'neomacs-cursor-stardust-color)
                neomacs-cursor-stardust-color nil)
            (if (boundp 'neomacs-cursor-stardust-particle-count)
                neomacs-cursor-stardust-particle-count nil)
            (if (boundp 'neomacs-cursor-stardust-fall-speed)
                neomacs-cursor-stardust-fall-speed nil)
            val))))

;; Brick wall overlay effect
(declare-function neomacs-set-brick-wall "neomacsterm.c")

(defcustom neomacs-brick-wall nil
  "Enable brick wall overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-brick-wall)
           (if val
               (neomacs-set-brick-wall t)
             (neomacs-set-brick-wall nil)))))

(defcustom neomacs-brick-wall-color "#996644"
  "Brick wall color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-brick-wall)
                    (boundp 'neomacs-brick-wall)
                    neomacs-brick-wall)
           (neomacs-set-brick-wall t val))))

(defcustom neomacs-brick-wall-width 40
  "Brick width in pixels."
  :type '(integer :tag "Brick width (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-brick-wall)
                    (boundp 'neomacs-brick-wall)
                    neomacs-brick-wall)
           (neomacs-set-brick-wall t
            (if (boundp 'neomacs-brick-wall-color)
                neomacs-brick-wall-color nil)
            val))))

(defcustom neomacs-brick-wall-height 20
  "Brick height in pixels."
  :type '(integer :tag "Brick height (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-brick-wall)
                    (boundp 'neomacs-brick-wall)
                    neomacs-brick-wall)
           (neomacs-set-brick-wall t
            (if (boundp 'neomacs-brick-wall-color)
                neomacs-brick-wall-color nil)
            (if (boundp 'neomacs-brick-wall-width)
                neomacs-brick-wall-width nil)
            val))))

(defcustom neomacs-brick-wall-opacity 6
  "Brick wall opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-brick-wall)
                    (boundp 'neomacs-brick-wall)
                    neomacs-brick-wall)
           (neomacs-set-brick-wall t
            (if (boundp 'neomacs-brick-wall-color)
                neomacs-brick-wall-color nil)
            (if (boundp 'neomacs-brick-wall-width)
                neomacs-brick-wall-width nil)
            (if (boundp 'neomacs-brick-wall-height)
                neomacs-brick-wall-height nil)
            val))))

;; Cursor compass needle effect
(declare-function neomacs-set-cursor-compass-needle "neomacsterm.c")

(defcustom neomacs-cursor-compass-needle nil
  "Enable cursor compass needle effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-compass-needle)
           (if val
               (neomacs-set-cursor-compass-needle t)
             (neomacs-set-cursor-compass-needle nil)))))

(defcustom neomacs-cursor-compass-needle-color "#FF4D4D"
  "Compass needle color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass-needle)
                    (boundp 'neomacs-cursor-compass-needle)
                    neomacs-cursor-compass-needle)
           (neomacs-set-cursor-compass-needle t val))))

(defcustom neomacs-cursor-compass-needle-length 20
  "Needle length in pixels."
  :type '(integer :tag "Length (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass-needle)
                    (boundp 'neomacs-cursor-compass-needle)
                    neomacs-cursor-compass-needle)
           (neomacs-set-cursor-compass-needle t
            (if (boundp 'neomacs-cursor-compass-needle-color)
                neomacs-cursor-compass-needle-color nil)
            val))))

(defcustom neomacs-cursor-compass-needle-spin-speed 200
  "Spin speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass-needle)
                    (boundp 'neomacs-cursor-compass-needle)
                    neomacs-cursor-compass-needle)
           (neomacs-set-cursor-compass-needle t
            (if (boundp 'neomacs-cursor-compass-needle-color)
                neomacs-cursor-compass-needle-color nil)
            (if (boundp 'neomacs-cursor-compass-needle-length)
                neomacs-cursor-compass-needle-length nil)
            val))))

(defcustom neomacs-cursor-compass-needle-opacity 20
  "Compass needle opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass-needle)
                    (boundp 'neomacs-cursor-compass-needle)
                    neomacs-cursor-compass-needle)
           (neomacs-set-cursor-compass-needle t
            (if (boundp 'neomacs-cursor-compass-needle-color)
                neomacs-cursor-compass-needle-color nil)
            (if (boundp 'neomacs-cursor-compass-needle-length)
                neomacs-cursor-compass-needle-length nil)
            (if (boundp 'neomacs-cursor-compass-needle-spin-speed)
                neomacs-cursor-compass-needle-spin-speed nil)
            val))))

;; Sine wave overlay effect
(declare-function neomacs-set-sine-wave "neomacsterm.c")

(defcustom neomacs-sine-wave nil
  "Enable sine wave overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-sine-wave)
           (if val
               (neomacs-set-sine-wave t)
             (neomacs-set-sine-wave nil)))))

(defcustom neomacs-sine-wave-color "#4DB3FF"
  "Sine wave color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sine-wave)
                    (boundp 'neomacs-sine-wave)
                    neomacs-sine-wave)
           (neomacs-set-sine-wave t val))))

(defcustom neomacs-sine-wave-amplitude 20
  "Wave amplitude in pixels."
  :type '(integer :tag "Amplitude (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sine-wave)
                    (boundp 'neomacs-sine-wave)
                    neomacs-sine-wave)
           (neomacs-set-sine-wave t
            (if (boundp 'neomacs-sine-wave-color)
                neomacs-sine-wave-color nil)
            val))))

(defcustom neomacs-sine-wave-wavelength 80
  "Wavelength in pixels."
  :type '(integer :tag "Wavelength (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sine-wave)
                    (boundp 'neomacs-sine-wave)
                    neomacs-sine-wave)
           (neomacs-set-sine-wave t
            (if (boundp 'neomacs-sine-wave-color)
                neomacs-sine-wave-color nil)
            (if (boundp 'neomacs-sine-wave-amplitude)
                neomacs-sine-wave-amplitude nil)
            val))))

(defcustom neomacs-sine-wave-speed 100
  "Animation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sine-wave)
                    (boundp 'neomacs-sine-wave)
                    neomacs-sine-wave)
           (neomacs-set-sine-wave t
            (if (boundp 'neomacs-sine-wave-color)
                neomacs-sine-wave-color nil)
            (if (boundp 'neomacs-sine-wave-amplitude)
                neomacs-sine-wave-amplitude nil)
            (if (boundp 'neomacs-sine-wave-wavelength)
                neomacs-sine-wave-wavelength nil)
            val))))

(defcustom neomacs-sine-wave-opacity 6
  "Sine wave opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sine-wave)
                    (boundp 'neomacs-sine-wave)
                    neomacs-sine-wave)
           (neomacs-set-sine-wave t
            (if (boundp 'neomacs-sine-wave-color)
                neomacs-sine-wave-color nil)
            (if (boundp 'neomacs-sine-wave-amplitude)
                neomacs-sine-wave-amplitude nil)
            (if (boundp 'neomacs-sine-wave-wavelength)
                neomacs-sine-wave-wavelength nil)
            (if (boundp 'neomacs-sine-wave-speed)
                neomacs-sine-wave-speed nil)
            val))))

;; Cursor galaxy effect
(declare-function neomacs-set-cursor-galaxy "neomacsterm.c")

(defcustom neomacs-cursor-galaxy nil
  "Enable cursor galaxy effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-galaxy)
           (if val
               (neomacs-set-cursor-galaxy t)
             (neomacs-set-cursor-galaxy nil)))))

(defcustom neomacs-cursor-galaxy-color "#CCCCFF"
  "Galaxy color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-galaxy)
                    (boundp 'neomacs-cursor-galaxy)
                    neomacs-cursor-galaxy)
           (neomacs-set-cursor-galaxy t val))))

(defcustom neomacs-cursor-galaxy-star-count 30
  "Number of stars."
  :type '(integer :tag "Star count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-galaxy)
                    (boundp 'neomacs-cursor-galaxy)
                    neomacs-cursor-galaxy)
           (neomacs-set-cursor-galaxy t
            (if (boundp 'neomacs-cursor-galaxy-color)
                neomacs-cursor-galaxy-color nil)
            val))))

(defcustom neomacs-cursor-galaxy-radius 30
  "Galaxy radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-galaxy)
                    (boundp 'neomacs-cursor-galaxy)
                    neomacs-cursor-galaxy)
           (neomacs-set-cursor-galaxy t
            (if (boundp 'neomacs-cursor-galaxy-color)
                neomacs-cursor-galaxy-color nil)
            (if (boundp 'neomacs-cursor-galaxy-star-count)
                neomacs-cursor-galaxy-star-count nil)
            val))))

(defcustom neomacs-cursor-galaxy-opacity 20
  "Cursor galaxy opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-galaxy)
                    (boundp 'neomacs-cursor-galaxy)
                    neomacs-cursor-galaxy)
           (neomacs-set-cursor-galaxy t
            (if (boundp 'neomacs-cursor-galaxy-color)
                neomacs-cursor-galaxy-color nil)
            (if (boundp 'neomacs-cursor-galaxy-star-count)
                neomacs-cursor-galaxy-star-count nil)
            (if (boundp 'neomacs-cursor-galaxy-radius)
                neomacs-cursor-galaxy-radius nil)
            val))))

;; Rotating gear overlay effect
(declare-function neomacs-set-rotating-gear "neomacsterm.c")

(defcustom neomacs-rotating-gear nil
  "Enable rotating gear overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-rotating-gear)
           (if val
               (neomacs-set-rotating-gear t)
             (neomacs-set-rotating-gear nil)))))

(defcustom neomacs-rotating-gear-color "#99B3CC"
  "Rotating gear color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rotating-gear)
                    (boundp 'neomacs-rotating-gear)
                    neomacs-rotating-gear)
           (neomacs-set-rotating-gear t val))))

(defcustom neomacs-rotating-gear-size 40
  "Gear size in pixels."
  :type '(integer :tag "Gear size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rotating-gear)
                    (boundp 'neomacs-rotating-gear)
                    neomacs-rotating-gear)
           (neomacs-set-rotating-gear t
            (if (boundp 'neomacs-rotating-gear-color)
                neomacs-rotating-gear-color nil)
            val))))

(defcustom neomacs-rotating-gear-speed 50
  "Rotation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rotating-gear)
                    (boundp 'neomacs-rotating-gear)
                    neomacs-rotating-gear)
           (neomacs-set-rotating-gear t
            (if (boundp 'neomacs-rotating-gear-color)
                neomacs-rotating-gear-color nil)
            (if (boundp 'neomacs-rotating-gear-size)
                neomacs-rotating-gear-size nil)
            val))))

(defcustom neomacs-rotating-gear-opacity 8
  "Rotating gear opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-rotating-gear)
                    (boundp 'neomacs-rotating-gear)
                    neomacs-rotating-gear)
           (neomacs-set-rotating-gear t
            (if (boundp 'neomacs-rotating-gear-color)
                neomacs-rotating-gear-color nil)
            (if (boundp 'neomacs-rotating-gear-size)
                neomacs-rotating-gear-size nil)
            (if (boundp 'neomacs-rotating-gear-speed)
                neomacs-rotating-gear-speed nil)
            val))))

;; Cursor prism effect
(declare-function neomacs-set-cursor-prism "neomacsterm.c")

(defcustom neomacs-cursor-prism nil
  "Enable cursor prism effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-prism)
           (if val
               (neomacs-set-cursor-prism t)
             (neomacs-set-cursor-prism nil)))))

(defcustom neomacs-cursor-prism-color "#FFFFFF"
  "Cursor prism color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-prism)
                    (boundp 'neomacs-cursor-prism)
                    neomacs-cursor-prism)
           (neomacs-set-cursor-prism t val))))

(defcustom neomacs-cursor-prism-ray-count 7
  "Number of prismatic rays."
  :type '(integer :tag "Ray count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-prism)
                    (boundp 'neomacs-cursor-prism)
                    neomacs-cursor-prism)
           (neomacs-set-cursor-prism t
            (if (boundp 'neomacs-cursor-prism-color)
                neomacs-cursor-prism-color nil)
            val))))

(defcustom neomacs-cursor-prism-spread 30
  "Ray spread distance in pixels."
  :type '(integer :tag "Spread (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-prism)
                    (boundp 'neomacs-cursor-prism)
                    neomacs-cursor-prism)
           (neomacs-set-cursor-prism t
            (if (boundp 'neomacs-cursor-prism-color)
                neomacs-cursor-prism-color nil)
            (if (boundp 'neomacs-cursor-prism-ray-count)
                neomacs-cursor-prism-ray-count nil)
            val))))

(defcustom neomacs-cursor-prism-opacity 15
  "Cursor prism opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-prism)
                    (boundp 'neomacs-cursor-prism)
                    neomacs-cursor-prism)
           (neomacs-set-cursor-prism t
            (if (boundp 'neomacs-cursor-prism-color)
                neomacs-cursor-prism-color nil)
            (if (boundp 'neomacs-cursor-prism-ray-count)
                neomacs-cursor-prism-ray-count nil)
            (if (boundp 'neomacs-cursor-prism-spread)
                neomacs-cursor-prism-spread nil)
            val))))

;; Crosshatch pattern overlay effect
(declare-function neomacs-set-crosshatch-pattern "neomacsterm.c")

(defcustom neomacs-crosshatch-pattern nil
  "Enable crosshatch pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-crosshatch-pattern)
           (if val
               (neomacs-set-crosshatch-pattern t)
             (neomacs-set-crosshatch-pattern nil)))))

(defcustom neomacs-crosshatch-pattern-color "#809966"
  "Crosshatch pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-crosshatch-pattern)
                    (boundp 'neomacs-crosshatch-pattern)
                    neomacs-crosshatch-pattern)
           (neomacs-set-crosshatch-pattern t val))))

(defcustom neomacs-crosshatch-pattern-line-spacing 20
  "Line spacing in pixels."
  :type '(integer :tag "Line spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-crosshatch-pattern)
                    (boundp 'neomacs-crosshatch-pattern)
                    neomacs-crosshatch-pattern)
           (neomacs-set-crosshatch-pattern t
            (if (boundp 'neomacs-crosshatch-pattern-color)
                neomacs-crosshatch-pattern-color nil)
            val))))

(defcustom neomacs-crosshatch-pattern-angle 45
  "Line angle in degrees."
  :type '(integer :tag "Angle (degrees)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-crosshatch-pattern)
                    (boundp 'neomacs-crosshatch-pattern)
                    neomacs-crosshatch-pattern)
           (neomacs-set-crosshatch-pattern t
            (if (boundp 'neomacs-crosshatch-pattern-color)
                neomacs-crosshatch-pattern-color nil)
            (if (boundp 'neomacs-crosshatch-pattern-line-spacing)
                neomacs-crosshatch-pattern-line-spacing nil)
            val))))

(defcustom neomacs-crosshatch-pattern-speed 30
  "Animation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-crosshatch-pattern)
                    (boundp 'neomacs-crosshatch-pattern)
                    neomacs-crosshatch-pattern)
           (neomacs-set-crosshatch-pattern t
            (if (boundp 'neomacs-crosshatch-pattern-color)
                neomacs-crosshatch-pattern-color nil)
            (if (boundp 'neomacs-crosshatch-pattern-line-spacing)
                neomacs-crosshatch-pattern-line-spacing nil)
            (if (boundp 'neomacs-crosshatch-pattern-angle)
                neomacs-crosshatch-pattern-angle nil)
            val))))

(defcustom neomacs-crosshatch-pattern-opacity 6
  "Crosshatch pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-crosshatch-pattern)
                    (boundp 'neomacs-crosshatch-pattern)
                    neomacs-crosshatch-pattern)
           (neomacs-set-crosshatch-pattern t
            (if (boundp 'neomacs-crosshatch-pattern-color)
                neomacs-crosshatch-pattern-color nil)
            (if (boundp 'neomacs-crosshatch-pattern-line-spacing)
                neomacs-crosshatch-pattern-line-spacing nil)
            (if (boundp 'neomacs-crosshatch-pattern-angle)
                neomacs-crosshatch-pattern-angle nil)
            (if (boundp 'neomacs-crosshatch-pattern-speed)
                neomacs-crosshatch-pattern-speed nil)
            val))))

;; Cursor moth effect
(declare-function neomacs-set-cursor-moth "neomacsterm.c")

(defcustom neomacs-cursor-moth nil
  "Enable cursor moth effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-moth)
           (if val
               (neomacs-set-cursor-moth t)
             (neomacs-set-cursor-moth nil)))))

(defcustom neomacs-cursor-moth-color "#E6CC80"
  "Cursor moth color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth)
                    (boundp 'neomacs-cursor-moth)
                    neomacs-cursor-moth)
           (neomacs-set-cursor-moth t val))))

(defcustom neomacs-cursor-moth-count 5
  "Number of moths."
  :type '(integer :tag "Moth count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth)
                    (boundp 'neomacs-cursor-moth)
                    neomacs-cursor-moth)
           (neomacs-set-cursor-moth t
            (if (boundp 'neomacs-cursor-moth-color)
                neomacs-cursor-moth-color nil)
            val))))

(defcustom neomacs-cursor-moth-wing-size 8
  "Wing size in pixels."
  :type '(integer :tag "Wing size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth)
                    (boundp 'neomacs-cursor-moth)
                    neomacs-cursor-moth)
           (neomacs-set-cursor-moth t
            (if (boundp 'neomacs-cursor-moth-color)
                neomacs-cursor-moth-color nil)
            (if (boundp 'neomacs-cursor-moth-count)
                neomacs-cursor-moth-count nil)
            val))))

(defcustom neomacs-cursor-moth-opacity 20
  "Cursor moth opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-moth)
                    (boundp 'neomacs-cursor-moth)
                    neomacs-cursor-moth)
           (neomacs-set-cursor-moth t
            (if (boundp 'neomacs-cursor-moth-color)
                neomacs-cursor-moth-color nil)
            (if (boundp 'neomacs-cursor-moth-count)
                neomacs-cursor-moth-count nil)
            (if (boundp 'neomacs-cursor-moth-wing-size)
                neomacs-cursor-moth-wing-size nil)
            val))))

;; Concentric rings overlay effect
(declare-function neomacs-set-concentric-rings "neomacsterm.c")

(defcustom neomacs-concentric-rings nil
  "Enable concentric rings overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-concentric-rings)
           (if val
               (neomacs-set-concentric-rings t)
             (neomacs-set-concentric-rings nil)))))

(defcustom neomacs-concentric-rings-color "#6699FF"
  "Concentric rings color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-concentric-rings)
                    (boundp 'neomacs-concentric-rings)
                    neomacs-concentric-rings)
           (neomacs-set-concentric-rings t val))))

(defcustom neomacs-concentric-rings-spacing 30
  "Ring spacing in pixels."
  :type '(integer :tag "Ring spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-concentric-rings)
                    (boundp 'neomacs-concentric-rings)
                    neomacs-concentric-rings)
           (neomacs-set-concentric-rings t
            (if (boundp 'neomacs-concentric-rings-color)
                neomacs-concentric-rings-color nil)
            val))))

(defcustom neomacs-concentric-rings-expansion-speed 100
  "Expansion speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-concentric-rings)
                    (boundp 'neomacs-concentric-rings)
                    neomacs-concentric-rings)
           (neomacs-set-concentric-rings t
            (if (boundp 'neomacs-concentric-rings-color)
                neomacs-concentric-rings-color nil)
            (if (boundp 'neomacs-concentric-rings-spacing)
                neomacs-concentric-rings-spacing nil)
            val))))

(defcustom neomacs-concentric-rings-opacity 8
  "Concentric rings opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-concentric-rings)
                    (boundp 'neomacs-concentric-rings)
                    neomacs-concentric-rings)
           (neomacs-set-concentric-rings t
            (if (boundp 'neomacs-concentric-rings-color)
                neomacs-concentric-rings-color nil)
            (if (boundp 'neomacs-concentric-rings-spacing)
                neomacs-concentric-rings-spacing nil)
            (if (boundp 'neomacs-concentric-rings-expansion-speed)
                neomacs-concentric-rings-expansion-speed nil)
            val))))

;; Cursor flame effect
(declare-function neomacs-set-cursor-flame "neomacsterm.c")

(defcustom neomacs-cursor-flame nil
  "Enable cursor flame effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-flame)
           (if val
               (neomacs-set-cursor-flame t)
             (neomacs-set-cursor-flame nil)))))

(defcustom neomacs-cursor-flame-color "#FF6633"
  "Cursor flame color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-flame)
                    (boundp 'neomacs-cursor-flame)
                    neomacs-cursor-flame)
           (neomacs-set-cursor-flame t val))))

(defcustom neomacs-cursor-flame-particle-count 12
  "Number of flame particles."
  :type '(integer :tag "Particle count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-flame)
                    (boundp 'neomacs-cursor-flame)
                    neomacs-cursor-flame)
           (neomacs-set-cursor-flame t
            (if (boundp 'neomacs-cursor-flame-color)
                neomacs-cursor-flame-color nil)
            val))))

(defcustom neomacs-cursor-flame-height 40
  "Flame height in pixels."
  :type '(integer :tag "Height (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-flame)
                    (boundp 'neomacs-cursor-flame)
                    neomacs-cursor-flame)
           (neomacs-set-cursor-flame t
            (if (boundp 'neomacs-cursor-flame-color)
                neomacs-cursor-flame-color nil)
            (if (boundp 'neomacs-cursor-flame-particle-count)
                neomacs-cursor-flame-particle-count nil)
            val))))

(defcustom neomacs-cursor-flame-opacity 15
  "Cursor flame opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-flame)
                    (boundp 'neomacs-cursor-flame)
                    neomacs-cursor-flame)
           (neomacs-set-cursor-flame t
            (if (boundp 'neomacs-cursor-flame-color)
                neomacs-cursor-flame-color nil)
            (if (boundp 'neomacs-cursor-flame-particle-count)
                neomacs-cursor-flame-particle-count nil)
            (if (boundp 'neomacs-cursor-flame-height)
                neomacs-cursor-flame-height nil)
            val))))

;; Zigzag pattern overlay effect
(declare-function neomacs-set-zigzag-pattern "neomacsterm.c")

(defcustom neomacs-zigzag-pattern nil
  "Enable zigzag pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-zigzag-pattern)
           (if val
               (neomacs-set-zigzag-pattern t)
             (neomacs-set-zigzag-pattern nil)))))

(defcustom neomacs-zigzag-pattern-color "#99CC66"
  "Zigzag pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zigzag-pattern)
                    (boundp 'neomacs-zigzag-pattern)
                    neomacs-zigzag-pattern)
           (neomacs-set-zigzag-pattern t val))))

(defcustom neomacs-zigzag-pattern-amplitude 15
  "Wave amplitude in pixels."
  :type '(integer :tag "Amplitude (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zigzag-pattern)
                    (boundp 'neomacs-zigzag-pattern)
                    neomacs-zigzag-pattern)
           (neomacs-set-zigzag-pattern t
            (if (boundp 'neomacs-zigzag-pattern-color)
                neomacs-zigzag-pattern-color nil)
            val))))

(defcustom neomacs-zigzag-pattern-frequency 50
  "Wave frequency (multiplied by 100)."
  :type '(integer :tag "Frequency * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zigzag-pattern)
                    (boundp 'neomacs-zigzag-pattern)
                    neomacs-zigzag-pattern)
           (neomacs-set-zigzag-pattern t
            (if (boundp 'neomacs-zigzag-pattern-color)
                neomacs-zigzag-pattern-color nil)
            (if (boundp 'neomacs-zigzag-pattern-amplitude)
                neomacs-zigzag-pattern-amplitude nil)
            val))))

(defcustom neomacs-zigzag-pattern-speed 80
  "Animation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zigzag-pattern)
                    (boundp 'neomacs-zigzag-pattern)
                    neomacs-zigzag-pattern)
           (neomacs-set-zigzag-pattern t
            (if (boundp 'neomacs-zigzag-pattern-color)
                neomacs-zigzag-pattern-color nil)
            (if (boundp 'neomacs-zigzag-pattern-amplitude)
                neomacs-zigzag-pattern-amplitude nil)
            (if (boundp 'neomacs-zigzag-pattern-frequency)
                neomacs-zigzag-pattern-frequency nil)
            val))))

(defcustom neomacs-zigzag-pattern-opacity 6
  "Zigzag pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-zigzag-pattern)
                    (boundp 'neomacs-zigzag-pattern)
                    neomacs-zigzag-pattern)
           (neomacs-set-zigzag-pattern t
            (if (boundp 'neomacs-zigzag-pattern-color)
                neomacs-zigzag-pattern-color nil)
            (if (boundp 'neomacs-zigzag-pattern-amplitude)
                neomacs-zigzag-pattern-amplitude nil)
            (if (boundp 'neomacs-zigzag-pattern-frequency)
                neomacs-zigzag-pattern-frequency nil)
            (if (boundp 'neomacs-zigzag-pattern-speed)
                neomacs-zigzag-pattern-speed nil)
            val))))

;; Cursor crystal effect
(declare-function neomacs-set-cursor-crystal "neomacsterm.c")

(defcustom neomacs-cursor-crystal nil
  "Enable cursor crystal effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-crystal)
           (if val
               (neomacs-set-cursor-crystal t)
             (neomacs-set-cursor-crystal nil)))))

(defcustom neomacs-cursor-crystal-color "#AADDFF"
  "Cursor crystal color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-crystal)
                    (boundp 'neomacs-cursor-crystal)
                    neomacs-cursor-crystal)
           (neomacs-set-cursor-crystal t val))))

(defcustom neomacs-cursor-crystal-facet-count 6
  "Number of crystal facets."
  :type '(integer :tag "Facet count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-crystal)
                    (boundp 'neomacs-cursor-crystal)
                    neomacs-cursor-crystal)
           (neomacs-set-cursor-crystal t
            (if (boundp 'neomacs-cursor-crystal-color)
                neomacs-cursor-crystal-color nil)
            val))))

(defcustom neomacs-cursor-crystal-radius 25
  "Crystal radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-crystal)
                    (boundp 'neomacs-cursor-crystal)
                    neomacs-cursor-crystal)
           (neomacs-set-cursor-crystal t
            (if (boundp 'neomacs-cursor-crystal-color)
                neomacs-cursor-crystal-color nil)
            (if (boundp 'neomacs-cursor-crystal-facet-count)
                neomacs-cursor-crystal-facet-count nil)
            val))))

(defcustom neomacs-cursor-crystal-opacity 12
  "Cursor crystal opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-crystal)
                    (boundp 'neomacs-cursor-crystal)
                    neomacs-cursor-crystal)
           (neomacs-set-cursor-crystal t
            (if (boundp 'neomacs-cursor-crystal-color)
                neomacs-cursor-crystal-color nil)
            (if (boundp 'neomacs-cursor-crystal-facet-count)
                neomacs-cursor-crystal-facet-count nil)
            (if (boundp 'neomacs-cursor-crystal-radius)
                neomacs-cursor-crystal-radius nil)
            val))))

;; Moiré pattern overlay effect
(declare-function neomacs-set-moire-pattern "neomacsterm.c")

(defcustom neomacs-moire-pattern nil
  "Enable moiré pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-moire-pattern)
           (if val
               (neomacs-set-moire-pattern t)
             (neomacs-set-moire-pattern nil)))))

(defcustom neomacs-moire-pattern-color "#8080CC"
  "Moiré pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-moire-pattern)
                    (boundp 'neomacs-moire-pattern)
                    neomacs-moire-pattern)
           (neomacs-set-moire-pattern t val))))

(defcustom neomacs-moire-pattern-line-spacing 8
  "Line spacing in pixels."
  :type '(integer :tag "Line spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-moire-pattern)
                    (boundp 'neomacs-moire-pattern)
                    neomacs-moire-pattern)
           (neomacs-set-moire-pattern t
            (if (boundp 'neomacs-moire-pattern-color)
                neomacs-moire-pattern-color nil)
            val))))

(defcustom neomacs-moire-pattern-angle-offset 5
  "Angle offset between grids in degrees."
  :type '(integer :tag "Angle (degrees)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-moire-pattern)
                    (boundp 'neomacs-moire-pattern)
                    neomacs-moire-pattern)
           (neomacs-set-moire-pattern t
            (if (boundp 'neomacs-moire-pattern-color)
                neomacs-moire-pattern-color nil)
            (if (boundp 'neomacs-moire-pattern-line-spacing)
                neomacs-moire-pattern-line-spacing nil)
            val))))

(defcustom neomacs-moire-pattern-speed 30
  "Rotation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-moire-pattern)
                    (boundp 'neomacs-moire-pattern)
                    neomacs-moire-pattern)
           (neomacs-set-moire-pattern t
            (if (boundp 'neomacs-moire-pattern-color)
                neomacs-moire-pattern-color nil)
            (if (boundp 'neomacs-moire-pattern-line-spacing)
                neomacs-moire-pattern-line-spacing nil)
            (if (boundp 'neomacs-moire-pattern-angle-offset)
                neomacs-moire-pattern-angle-offset nil)
            val))))

(defcustom neomacs-moire-pattern-opacity 6
  "Moiré pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-moire-pattern)
                    (boundp 'neomacs-moire-pattern)
                    neomacs-moire-pattern)
           (neomacs-set-moire-pattern t
            (if (boundp 'neomacs-moire-pattern-color)
                neomacs-moire-pattern-color nil)
            (if (boundp 'neomacs-moire-pattern-line-spacing)
                neomacs-moire-pattern-line-spacing nil)
            (if (boundp 'neomacs-moire-pattern-angle-offset)
                neomacs-moire-pattern-angle-offset nil)
            (if (boundp 'neomacs-moire-pattern-speed)
                neomacs-moire-pattern-speed nil)
            val))))

;; Cursor lightning effect
(declare-function neomacs-set-cursor-lightning "neomacsterm.c")

(defcustom neomacs-cursor-lightning nil
  "Enable cursor lightning effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-lightning)
           (if val
               (neomacs-set-cursor-lightning t)
             (neomacs-set-cursor-lightning nil)))))

(defcustom neomacs-cursor-lightning-color "#99CCFF"
  "Lightning color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lightning)
                    (boundp 'neomacs-cursor-lightning)
                    neomacs-cursor-lightning)
           (neomacs-set-cursor-lightning t val))))

(defcustom neomacs-cursor-lightning-bolt-count 4
  "Number of lightning bolts."
  :type '(integer :tag "Bolt count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lightning)
                    (boundp 'neomacs-cursor-lightning)
                    neomacs-cursor-lightning)
           (neomacs-set-cursor-lightning t
            (if (boundp 'neomacs-cursor-lightning-color)
                neomacs-cursor-lightning-color nil)
            val))))

(defcustom neomacs-cursor-lightning-max-length 50
  "Maximum bolt length in pixels."
  :type '(integer :tag "Max length (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lightning)
                    (boundp 'neomacs-cursor-lightning)
                    neomacs-cursor-lightning)
           (neomacs-set-cursor-lightning t
            (if (boundp 'neomacs-cursor-lightning-color)
                neomacs-cursor-lightning-color nil)
            (if (boundp 'neomacs-cursor-lightning-bolt-count)
                neomacs-cursor-lightning-bolt-count nil)
            val))))

(defcustom neomacs-cursor-lightning-opacity 40
  "Lightning opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-lightning)
                    (boundp 'neomacs-cursor-lightning)
                    neomacs-cursor-lightning)
           (neomacs-set-cursor-lightning t
            (if (boundp 'neomacs-cursor-lightning-color)
                neomacs-cursor-lightning-color nil)
            (if (boundp 'neomacs-cursor-lightning-bolt-count)
                neomacs-cursor-lightning-bolt-count nil)
            (if (boundp 'neomacs-cursor-lightning-max-length)
                neomacs-cursor-lightning-max-length nil)
            val))))

;; Dot matrix overlay effect
(declare-function neomacs-set-dot-matrix "neomacsterm.c")

(defcustom neomacs-dot-matrix nil
  "Enable dot matrix overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-dot-matrix)
           (if val
               (neomacs-set-dot-matrix t)
             (neomacs-set-dot-matrix nil)))))

(defcustom neomacs-dot-matrix-color "#4DFF4D"
  "Dot matrix color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-dot-matrix)
                    (boundp 'neomacs-dot-matrix)
                    neomacs-dot-matrix)
           (neomacs-set-dot-matrix t val))))

(defcustom neomacs-dot-matrix-spacing 12
  "Dot spacing in pixels."
  :type '(integer :tag "Spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-dot-matrix)
                    (boundp 'neomacs-dot-matrix)
                    neomacs-dot-matrix)
           (neomacs-set-dot-matrix t
            (if (boundp 'neomacs-dot-matrix-color)
                neomacs-dot-matrix-color nil)
            val))))

(defcustom neomacs-dot-matrix-pulse-speed 100
  "Pulse animation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-dot-matrix)
                    (boundp 'neomacs-dot-matrix)
                    neomacs-dot-matrix)
           (neomacs-set-dot-matrix t
            (if (boundp 'neomacs-dot-matrix-color)
                neomacs-dot-matrix-color nil)
            (if (boundp 'neomacs-dot-matrix-spacing)
                neomacs-dot-matrix-spacing nil)
            val))))

(defcustom neomacs-dot-matrix-opacity 6
  "Dot matrix opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-dot-matrix)
                    (boundp 'neomacs-dot-matrix)
                    neomacs-dot-matrix)
           (neomacs-set-dot-matrix t
            (if (boundp 'neomacs-dot-matrix-color)
                neomacs-dot-matrix-color nil)
            (if (boundp 'neomacs-dot-matrix-spacing)
                neomacs-dot-matrix-spacing nil)
            (if (boundp 'neomacs-dot-matrix-pulse-speed)
                neomacs-dot-matrix-pulse-speed nil)
            val))))

;; Cursor snowflake effect
(declare-function neomacs-set-cursor-snowflake "neomacsterm.c")

(defcustom neomacs-cursor-snowflake nil
  "Enable cursor snowflake effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-snowflake)
           (if val
               (neomacs-set-cursor-snowflake t)
             (neomacs-set-cursor-snowflake nil)))))

(defcustom neomacs-cursor-snowflake-color "#CCE5FF"
  "Snowflake color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-snowflake)
                    (boundp 'neomacs-cursor-snowflake)
                    neomacs-cursor-snowflake)
           (neomacs-set-cursor-snowflake t val))))

(defcustom neomacs-cursor-snowflake-count 8
  "Number of snowflakes."
  :type '(integer :tag "Count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-snowflake)
                    (boundp 'neomacs-cursor-snowflake)
                    neomacs-cursor-snowflake)
           (neomacs-set-cursor-snowflake t
            (if (boundp 'neomacs-cursor-snowflake-color)
                neomacs-cursor-snowflake-color nil)
            val))))

(defcustom neomacs-cursor-snowflake-fall-speed 30
  "Snowflake fall speed in pixels per second."
  :type '(integer :tag "Fall speed (px/s)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-snowflake)
                    (boundp 'neomacs-cursor-snowflake)
                    neomacs-cursor-snowflake)
           (neomacs-set-cursor-snowflake t
            (if (boundp 'neomacs-cursor-snowflake-color)
                neomacs-cursor-snowflake-color nil)
            (if (boundp 'neomacs-cursor-snowflake-count)
                neomacs-cursor-snowflake-count nil)
            val))))

(defcustom neomacs-cursor-snowflake-opacity 30
  "Snowflake opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-snowflake)
                    (boundp 'neomacs-cursor-snowflake)
                    neomacs-cursor-snowflake)
           (neomacs-set-cursor-snowflake t
            (if (boundp 'neomacs-cursor-snowflake-color)
                neomacs-cursor-snowflake-color nil)
            (if (boundp 'neomacs-cursor-snowflake-count)
                neomacs-cursor-snowflake-count nil)
            (if (boundp 'neomacs-cursor-snowflake-fall-speed)
                neomacs-cursor-snowflake-fall-speed nil)
            val))))

;; Sunburst pattern overlay effect
(declare-function neomacs-set-sunburst-pattern "neomacsterm.c")

(defcustom neomacs-sunburst-pattern nil
  "Enable sunburst pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-sunburst-pattern)
           (if val
               (neomacs-set-sunburst-pattern t)
             (neomacs-set-sunburst-pattern nil)))))

(defcustom neomacs-sunburst-pattern-color "#FFCC4D"
  "Sunburst color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sunburst-pattern)
                    (boundp 'neomacs-sunburst-pattern)
                    neomacs-sunburst-pattern)
           (neomacs-set-sunburst-pattern t val))))

(defcustom neomacs-sunburst-pattern-ray-count 12
  "Number of rays."
  :type '(integer :tag "Ray count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sunburst-pattern)
                    (boundp 'neomacs-sunburst-pattern)
                    neomacs-sunburst-pattern)
           (neomacs-set-sunburst-pattern t
            (if (boundp 'neomacs-sunburst-pattern-color)
                neomacs-sunburst-pattern-color nil)
            val))))

(defcustom neomacs-sunburst-pattern-speed 50
  "Rotation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sunburst-pattern)
                    (boundp 'neomacs-sunburst-pattern)
                    neomacs-sunburst-pattern)
           (neomacs-set-sunburst-pattern t
            (if (boundp 'neomacs-sunburst-pattern-color)
                neomacs-sunburst-pattern-color nil)
            (if (boundp 'neomacs-sunburst-pattern-ray-count)
                neomacs-sunburst-pattern-ray-count nil)
            val))))

(defcustom neomacs-sunburst-pattern-opacity 8
  "Sunburst opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-sunburst-pattern)
                    (boundp 'neomacs-sunburst-pattern)
                    neomacs-sunburst-pattern)
           (neomacs-set-sunburst-pattern t
            (if (boundp 'neomacs-sunburst-pattern-color)
                neomacs-sunburst-pattern-color nil)
            (if (boundp 'neomacs-sunburst-pattern-ray-count)
                neomacs-sunburst-pattern-ray-count nil)
            (if (boundp 'neomacs-sunburst-pattern-speed)
                neomacs-sunburst-pattern-speed nil)
            val))))

;; Cursor firework effect
(declare-function neomacs-set-cursor-firework "neomacsterm.c")

(defcustom neomacs-cursor-firework nil
  "Enable cursor firework effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-firework)
           (if val
               (neomacs-set-cursor-firework t)
             (neomacs-set-cursor-firework nil)))))

(defcustom neomacs-cursor-firework-color "#FF9933"
  "Firework color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-firework)
                    (boundp 'neomacs-cursor-firework)
                    neomacs-cursor-firework)
           (neomacs-set-cursor-firework t val))))

(defcustom neomacs-cursor-firework-particle-count 16
  "Number of particles."
  :type '(integer :tag "Particle count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-firework)
                    (boundp 'neomacs-cursor-firework)
                    neomacs-cursor-firework)
           (neomacs-set-cursor-firework t
            (if (boundp 'neomacs-cursor-firework-color)
                neomacs-cursor-firework-color nil)
            val))))

(defcustom neomacs-cursor-firework-burst-radius 60
  "Burst radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-firework)
                    (boundp 'neomacs-cursor-firework)
                    neomacs-cursor-firework)
           (neomacs-set-cursor-firework t
            (if (boundp 'neomacs-cursor-firework-color)
                neomacs-cursor-firework-color nil)
            (if (boundp 'neomacs-cursor-firework-particle-count)
                neomacs-cursor-firework-particle-count nil)
            val))))

(defcustom neomacs-cursor-firework-opacity 30
  "Firework opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-firework)
                    (boundp 'neomacs-cursor-firework)
                    neomacs-cursor-firework)
           (neomacs-set-cursor-firework t
            (if (boundp 'neomacs-cursor-firework-color)
                neomacs-cursor-firework-color nil)
            (if (boundp 'neomacs-cursor-firework-particle-count)
                neomacs-cursor-firework-particle-count nil)
            (if (boundp 'neomacs-cursor-firework-burst-radius)
                neomacs-cursor-firework-burst-radius nil)
            val))))

;; Honeycomb dissolve overlay effect
(declare-function neomacs-set-honeycomb-dissolve "neomacsterm.c")

(defcustom neomacs-honeycomb-dissolve nil
  "Enable honeycomb dissolve overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-honeycomb-dissolve)
           (if val
               (neomacs-set-honeycomb-dissolve t)
             (neomacs-set-honeycomb-dissolve nil)))))

(defcustom neomacs-honeycomb-dissolve-color "#CC9933"
  "Honeycomb color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-honeycomb-dissolve)
                    (boundp 'neomacs-honeycomb-dissolve)
                    neomacs-honeycomb-dissolve)
           (neomacs-set-honeycomb-dissolve t val))))

(defcustom neomacs-honeycomb-dissolve-cell-size 30
  "Hexagonal cell size in pixels."
  :type '(integer :tag "Cell size (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-honeycomb-dissolve)
                    (boundp 'neomacs-honeycomb-dissolve)
                    neomacs-honeycomb-dissolve)
           (neomacs-set-honeycomb-dissolve t
            (if (boundp 'neomacs-honeycomb-dissolve-color)
                neomacs-honeycomb-dissolve-color nil)
            val))))

(defcustom neomacs-honeycomb-dissolve-speed 80
  "Dissolve animation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-honeycomb-dissolve)
                    (boundp 'neomacs-honeycomb-dissolve)
                    neomacs-honeycomb-dissolve)
           (neomacs-set-honeycomb-dissolve t
            (if (boundp 'neomacs-honeycomb-dissolve-color)
                neomacs-honeycomb-dissolve-color nil)
            (if (boundp 'neomacs-honeycomb-dissolve-cell-size)
                neomacs-honeycomb-dissolve-cell-size nil)
            val))))

(defcustom neomacs-honeycomb-dissolve-opacity 8
  "Honeycomb dissolve opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-honeycomb-dissolve)
                    (boundp 'neomacs-honeycomb-dissolve)
                    neomacs-honeycomb-dissolve)
           (neomacs-set-honeycomb-dissolve t
            (if (boundp 'neomacs-honeycomb-dissolve-color)
                neomacs-honeycomb-dissolve-color nil)
            (if (boundp 'neomacs-honeycomb-dissolve-cell-size)
                neomacs-honeycomb-dissolve-cell-size nil)
            (if (boundp 'neomacs-honeycomb-dissolve-speed)
                neomacs-honeycomb-dissolve-speed nil)
            val))))

;; Cursor tornado effect
(declare-function neomacs-set-cursor-tornado "neomacsterm.c")

(defcustom neomacs-cursor-tornado nil
  "Enable cursor tornado effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-tornado)
           (if val
               (neomacs-set-cursor-tornado t)
             (neomacs-set-cursor-tornado nil)))))

(defcustom neomacs-cursor-tornado-color "#80B3FF"
  "Tornado color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-tornado)
                    (boundp 'neomacs-cursor-tornado)
                    neomacs-cursor-tornado)
           (neomacs-set-cursor-tornado t val))))

(defcustom neomacs-cursor-tornado-radius 40
  "Tornado radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-tornado)
                    (boundp 'neomacs-cursor-tornado)
                    neomacs-cursor-tornado)
           (neomacs-set-cursor-tornado t
            (if (boundp 'neomacs-cursor-tornado-color)
                neomacs-cursor-tornado-color nil)
            val))))

(defcustom neomacs-cursor-tornado-particle-count 12
  "Number of tornado particles."
  :type '(integer :tag "Particle count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-tornado)
                    (boundp 'neomacs-cursor-tornado)
                    neomacs-cursor-tornado)
           (neomacs-set-cursor-tornado t
            (if (boundp 'neomacs-cursor-tornado-color)
                neomacs-cursor-tornado-color nil)
            (if (boundp 'neomacs-cursor-tornado-radius)
                neomacs-cursor-tornado-radius nil)
            val))))

(defcustom neomacs-cursor-tornado-opacity 25
  "Tornado opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-tornado)
                    (boundp 'neomacs-cursor-tornado)
                    neomacs-cursor-tornado)
           (neomacs-set-cursor-tornado t
            (if (boundp 'neomacs-cursor-tornado-color)
                neomacs-cursor-tornado-color nil)
            (if (boundp 'neomacs-cursor-tornado-radius)
                neomacs-cursor-tornado-radius nil)
            (if (boundp 'neomacs-cursor-tornado-particle-count)
                neomacs-cursor-tornado-particle-count nil)
            val))))

;; Wave interference overlay effect
(declare-function neomacs-set-wave-interference "neomacsterm.c")

(defcustom neomacs-wave-interference nil
  "Enable wave interference overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-wave-interference)
           (if val
               (neomacs-set-wave-interference t)
             (neomacs-set-wave-interference nil)))))

(defcustom neomacs-wave-interference-color "#4D99E5"
  "Wave interference color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wave-interference)
                    (boundp 'neomacs-wave-interference)
                    neomacs-wave-interference)
           (neomacs-set-wave-interference t val))))

(defcustom neomacs-wave-interference-wavelength 60
  "Wave wavelength in pixels."
  :type '(integer :tag "Wavelength (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wave-interference)
                    (boundp 'neomacs-wave-interference)
                    neomacs-wave-interference)
           (neomacs-set-wave-interference t
            (if (boundp 'neomacs-wave-interference-color)
                neomacs-wave-interference-color nil)
            val))))

(defcustom neomacs-wave-interference-source-count 3
  "Number of wave sources."
  :type '(integer :tag "Source count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wave-interference)
                    (boundp 'neomacs-wave-interference)
                    neomacs-wave-interference)
           (neomacs-set-wave-interference t
            (if (boundp 'neomacs-wave-interference-color)
                neomacs-wave-interference-color nil)
            (if (boundp 'neomacs-wave-interference-wavelength)
                neomacs-wave-interference-wavelength nil)
            val))))

(defcustom neomacs-wave-interference-speed 100
  "Wave animation speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wave-interference)
                    (boundp 'neomacs-wave-interference)
                    neomacs-wave-interference)
           (neomacs-set-wave-interference t
            (if (boundp 'neomacs-wave-interference-color)
                neomacs-wave-interference-color nil)
            (if (boundp 'neomacs-wave-interference-wavelength)
                neomacs-wave-interference-wavelength nil)
            (if (boundp 'neomacs-wave-interference-source-count)
                neomacs-wave-interference-source-count nil)
            val))))

(defcustom neomacs-wave-interference-opacity 10
  "Wave interference opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-wave-interference)
                    (boundp 'neomacs-wave-interference)
                    neomacs-wave-interference)
           (neomacs-set-wave-interference t
            (if (boundp 'neomacs-wave-interference-color)
                neomacs-wave-interference-color nil)
            (if (boundp 'neomacs-wave-interference-wavelength)
                neomacs-wave-interference-wavelength nil)
            (if (boundp 'neomacs-wave-interference-source-count)
                neomacs-wave-interference-source-count nil)
            (if (boundp 'neomacs-wave-interference-speed)
                neomacs-wave-interference-speed nil)
            val))))

;; Cursor portal effect
(declare-function neomacs-set-cursor-portal "neomacsterm.c")

(defcustom neomacs-cursor-portal nil
  "Enable cursor portal effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-portal)
           (if val
               (neomacs-set-cursor-portal t)
             (neomacs-set-cursor-portal nil)))))

(defcustom neomacs-cursor-portal-color "#9966FF"
  "Portal color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-portal)
                    (boundp 'neomacs-cursor-portal)
                    neomacs-cursor-portal)
           (neomacs-set-cursor-portal t val))))

(defcustom neomacs-cursor-portal-radius 30
  "Portal radius in pixels."
  :type '(integer :tag "Radius (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-portal)
                    (boundp 'neomacs-cursor-portal)
                    neomacs-cursor-portal)
           (neomacs-set-cursor-portal t
            (if (boundp 'neomacs-cursor-portal-color)
                neomacs-cursor-portal-color nil)
            val))))

(defcustom neomacs-cursor-portal-speed 200
  "Portal swirl speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-portal)
                    (boundp 'neomacs-cursor-portal)
                    neomacs-cursor-portal)
           (neomacs-set-cursor-portal t
            (if (boundp 'neomacs-cursor-portal-color)
                neomacs-cursor-portal-color nil)
            (if (boundp 'neomacs-cursor-portal-radius)
                neomacs-cursor-portal-radius nil)
            val))))

(defcustom neomacs-cursor-portal-opacity 25
  "Portal opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-portal)
                    (boundp 'neomacs-cursor-portal)
                    neomacs-cursor-portal)
           (neomacs-set-cursor-portal t
            (if (boundp 'neomacs-cursor-portal-color)
                neomacs-cursor-portal-color nil)
            (if (boundp 'neomacs-cursor-portal-radius)
                neomacs-cursor-portal-radius nil)
            (if (boundp 'neomacs-cursor-portal-speed)
                neomacs-cursor-portal-speed nil)
            val))))

;; Chevron pattern overlay effect
(declare-function neomacs-set-chevron-pattern "neomacsterm.c")

(defcustom neomacs-chevron-pattern nil
  "Enable chevron pattern overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-chevron-pattern)
           (if val
               (neomacs-set-chevron-pattern t)
             (neomacs-set-chevron-pattern nil)))))

(defcustom neomacs-chevron-pattern-color "#4DE5B0"
  "Chevron pattern color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-chevron-pattern)
                    (boundp 'neomacs-chevron-pattern)
                    neomacs-chevron-pattern)
           (neomacs-set-chevron-pattern t val))))

(defcustom neomacs-chevron-pattern-spacing 40
  "Chevron spacing in pixels."
  :type '(integer :tag "Spacing (px)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-chevron-pattern)
                    (boundp 'neomacs-chevron-pattern)
                    neomacs-chevron-pattern)
           (neomacs-set-chevron-pattern t
            (if (boundp 'neomacs-chevron-pattern-color)
                neomacs-chevron-pattern-color nil)
            val))))

(defcustom neomacs-chevron-pattern-speed 50
  "Chevron scroll speed (multiplied by 100)."
  :type '(integer :tag "Speed * 100")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-chevron-pattern)
                    (boundp 'neomacs-chevron-pattern)
                    neomacs-chevron-pattern)
           (neomacs-set-chevron-pattern t
            (if (boundp 'neomacs-chevron-pattern-color)
                neomacs-chevron-pattern-color nil)
            (if (boundp 'neomacs-chevron-pattern-spacing)
                neomacs-chevron-pattern-spacing nil)
            val))))

(defcustom neomacs-chevron-pattern-opacity 8
  "Chevron pattern opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-chevron-pattern)
                    (boundp 'neomacs-chevron-pattern)
                    neomacs-chevron-pattern)
           (neomacs-set-chevron-pattern t
            (if (boundp 'neomacs-chevron-pattern-color)
                neomacs-chevron-pattern-color nil)
            (if (boundp 'neomacs-chevron-pattern-spacing)
                neomacs-chevron-pattern-spacing nil)
            (if (boundp 'neomacs-chevron-pattern-speed)
                neomacs-chevron-pattern-speed nil)
            val))))

;; Cursor bubble effect
(declare-function neomacs-set-cursor-bubble "neomacsterm.c")

(defcustom neomacs-cursor-bubble nil
  "Enable cursor bubble effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-bubble)
           (if val
               (neomacs-set-cursor-bubble t)
             (neomacs-set-cursor-bubble nil)))))

(defcustom neomacs-cursor-bubble-color "#66CCFF"
  "Bubble color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-bubble)
                    (boundp 'neomacs-cursor-bubble)
                    neomacs-cursor-bubble)
           (neomacs-set-cursor-bubble t val))))

(defcustom neomacs-cursor-bubble-count 6
  "Number of bubbles."
  :type '(integer :tag "Count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-bubble)
                    (boundp 'neomacs-cursor-bubble)
                    neomacs-cursor-bubble)
           (neomacs-set-cursor-bubble t
            (if (boundp 'neomacs-cursor-bubble-color)
                neomacs-cursor-bubble-color nil)
            val))))

(defcustom neomacs-cursor-bubble-rise-speed 40
  "Bubble rise speed in pixels per second."
  :type '(integer :tag "Rise speed (px/s)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-bubble)
                    (boundp 'neomacs-cursor-bubble)
                    neomacs-cursor-bubble)
           (neomacs-set-cursor-bubble t
            (if (boundp 'neomacs-cursor-bubble-color)
                neomacs-cursor-bubble-color nil)
            (if (boundp 'neomacs-cursor-bubble-count)
                neomacs-cursor-bubble-count nil)
            val))))

(defcustom neomacs-cursor-bubble-opacity 20
  "Bubble opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-bubble)
                    (boundp 'neomacs-cursor-bubble)
                    neomacs-cursor-bubble)
           (neomacs-set-cursor-bubble t
            (if (boundp 'neomacs-cursor-bubble-color)
                neomacs-cursor-bubble-color nil)
            (if (boundp 'neomacs-cursor-bubble-count)
                neomacs-cursor-bubble-count nil)
            (if (boundp 'neomacs-cursor-bubble-rise-speed)
                neomacs-cursor-bubble-rise-speed nil)
            val))))

;; Gaussian blur post-processing effect
(declare-function neomacs-set-blur "neomacsterm.c")

(defun neomacs--apply-blur ()
  "Apply current blur settings."
  (when (fboundp 'neomacs-set-blur)
    (neomacs-set-blur
     (if (and (boundp 'neomacs-blur) neomacs-blur) t nil)
     (if (boundp 'neomacs-blur-radius)
         neomacs-blur-radius 100)
     (if (boundp 'neomacs-blur-passes)
         neomacs-blur-passes 2))))

(defcustom neomacs-blur nil
  "Enable Gaussian blur post-processing effect.
When enabled, applies a GPU-accelerated Gaussian blur to the entire frame.
This is a cross-platform feature using wgpu shaders."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (neomacs--apply-blur)))

(defcustom neomacs-blur-radius 100
  "Blur radius multiplied by 100 (100 = standard 1.0x radius).
Higher values produce a wider, more diffuse blur."
  :type '(integer :tag "Radius (x100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (neomacs--apply-blur)))

(defcustom neomacs-blur-passes 2
  "Number of blur passes (iterations).
Each pass applies horizontal + vertical Gaussian blur.
1 = light blur, 2-3 = medium, 4+ = heavy."
  :type '(integer :tag "Passes")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (neomacs--apply-blur)))

;; Hex grid overlay effect
(declare-function neomacs-set-hex-grid "neomacsterm.c")

(defcustom neomacs-hex-grid nil
  "Enable hex grid overlay effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-hex-grid)
           (neomacs-set-hex-grid val))))

(defcustom neomacs-hex-grid-color "#4D99E5"
  "Color of hex grid lines."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-hex-grid)
                    (boundp 'neomacs-hex-grid)
                    neomacs-hex-grid)
           (neomacs-set-hex-grid t val))))

(defcustom neomacs-hex-grid-cell-size 40
  "Hexagon cell size in pixels."
  :type '(integer :tag "Cell size (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-hex-grid)
                    (boundp 'neomacs-hex-grid)
                    neomacs-hex-grid)
           (neomacs-set-hex-grid t
            (if (boundp 'neomacs-hex-grid-color)
                neomacs-hex-grid-color nil)
            val))))

(defcustom neomacs-hex-grid-pulse-speed 100
  "Pulse animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-hex-grid)
                    (boundp 'neomacs-hex-grid)
                    neomacs-hex-grid)
           (neomacs-set-hex-grid t
            (if (boundp 'neomacs-hex-grid-color)
                neomacs-hex-grid-color nil)
            (if (boundp 'neomacs-hex-grid-cell-size)
                neomacs-hex-grid-cell-size nil)
            val))))

(defcustom neomacs-hex-grid-opacity 10
  "Hex grid opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-hex-grid)
                    (boundp 'neomacs-hex-grid)
                    neomacs-hex-grid)
           (neomacs-set-hex-grid t
            (if (boundp 'neomacs-hex-grid-color)
                neomacs-hex-grid-color nil)
            (if (boundp 'neomacs-hex-grid-cell-size)
                neomacs-hex-grid-cell-size nil)
            (if (boundp 'neomacs-hex-grid-pulse-speed)
                neomacs-hex-grid-pulse-speed nil)
            val))))

;; Cursor sparkle burst effect
(declare-function neomacs-set-cursor-sparkle-burst "neomacsterm.c")

(defcustom neomacs-cursor-sparkle-burst nil
  "Enable cursor sparkle burst effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-sparkle-burst)
           (neomacs-set-cursor-sparkle-burst val))))

(defcustom neomacs-cursor-sparkle-burst-color "#FFD94D"
  "Sparkle particle color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkle-burst)
                    (boundp 'neomacs-cursor-sparkle-burst)
                    neomacs-cursor-sparkle-burst)
           (neomacs-set-cursor-sparkle-burst t val))))

(defcustom neomacs-cursor-sparkle-burst-count 12
  "Number of sparkle particles per burst."
  :type '(integer :tag "Particle count")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkle-burst)
                    (boundp 'neomacs-cursor-sparkle-burst)
                    neomacs-cursor-sparkle-burst)
           (neomacs-set-cursor-sparkle-burst t
            (if (boundp 'neomacs-cursor-sparkle-burst-color)
                neomacs-cursor-sparkle-burst-color nil)
            val))))

(defcustom neomacs-cursor-sparkle-burst-radius 30
  "Burst radius in pixels."
  :type '(integer :tag "Radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkle-burst)
                    (boundp 'neomacs-cursor-sparkle-burst)
                    neomacs-cursor-sparkle-burst)
           (neomacs-set-cursor-sparkle-burst t
            (if (boundp 'neomacs-cursor-sparkle-burst-color)
                neomacs-cursor-sparkle-burst-color nil)
            (if (boundp 'neomacs-cursor-sparkle-burst-count)
                neomacs-cursor-sparkle-burst-count nil)
            val))))

(defcustom neomacs-cursor-sparkle-burst-opacity 40
  "Sparkle burst opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-sparkle-burst)
                    (boundp 'neomacs-cursor-sparkle-burst)
                    neomacs-cursor-sparkle-burst)
           (neomacs-set-cursor-sparkle-burst t
            (if (boundp 'neomacs-cursor-sparkle-burst-color)
                neomacs-cursor-sparkle-burst-color nil)
            (if (boundp 'neomacs-cursor-sparkle-burst-count)
                neomacs-cursor-sparkle-burst-count nil)
            (if (boundp 'neomacs-cursor-sparkle-burst-radius)
                neomacs-cursor-sparkle-burst-radius nil)
            val))))

;; Circuit board trace effect
(declare-function neomacs-set-circuit-trace "neomacsterm.c")

(defcustom neomacs-circuit-trace nil
  "Enable circuit board trace effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-circuit-trace)
           (neomacs-set-circuit-trace val))))

(defcustom neomacs-circuit-trace-color "#33CC66"
  "Circuit trace color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-circuit-trace)
                    (boundp 'neomacs-circuit-trace)
                    neomacs-circuit-trace)
           (neomacs-set-circuit-trace t val))))

(defcustom neomacs-circuit-trace-width 2
  "Trace width in pixels."
  :type '(integer :tag "Width (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-circuit-trace)
                    (boundp 'neomacs-circuit-trace)
                    neomacs-circuit-trace)
           (neomacs-set-circuit-trace t
            (if (boundp 'neomacs-circuit-trace-color)
                neomacs-circuit-trace-color nil)
            val))))

(defcustom neomacs-circuit-trace-speed 100
  "Circuit trace animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-circuit-trace)
                    (boundp 'neomacs-circuit-trace)
                    neomacs-circuit-trace)
           (neomacs-set-circuit-trace t
            (if (boundp 'neomacs-circuit-trace-color)
                neomacs-circuit-trace-color nil)
            (if (boundp 'neomacs-circuit-trace-width)
                neomacs-circuit-trace-width nil)
            val))))

(defcustom neomacs-circuit-trace-opacity 20
  "Circuit trace opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-circuit-trace)
                    (boundp 'neomacs-circuit-trace)
                    neomacs-circuit-trace)
           (neomacs-set-circuit-trace t
            (if (boundp 'neomacs-circuit-trace-color)
                neomacs-circuit-trace-color nil)
            (if (boundp 'neomacs-circuit-trace-width)
                neomacs-circuit-trace-width nil)
            (if (boundp 'neomacs-circuit-trace-speed)
                neomacs-circuit-trace-speed nil)
            val))))

;; Cursor compass rose effect
(declare-function neomacs-set-cursor-compass "neomacsterm.c")

(defcustom neomacs-cursor-compass nil
  "Enable cursor compass rose effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-compass)
           (neomacs-set-cursor-compass val))))

(defcustom neomacs-cursor-compass-color "#E59933"
  "Compass rose color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass)
                    (boundp 'neomacs-cursor-compass)
                    neomacs-cursor-compass)
           (neomacs-set-cursor-compass t val))))

(defcustom neomacs-cursor-compass-size 20
  "Compass size in pixels."
  :type '(integer :tag "Size (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass)
                    (boundp 'neomacs-cursor-compass)
                    neomacs-cursor-compass)
           (neomacs-set-cursor-compass t
            (if (boundp 'neomacs-cursor-compass-color)
                neomacs-cursor-compass-color nil)
            val))))

(defcustom neomacs-cursor-compass-speed 100
  "Compass rotation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass)
                    (boundp 'neomacs-cursor-compass)
                    neomacs-cursor-compass)
           (neomacs-set-cursor-compass t
            (if (boundp 'neomacs-cursor-compass-color)
                neomacs-cursor-compass-color nil)
            (if (boundp 'neomacs-cursor-compass-size)
                neomacs-cursor-compass-size nil)
            val))))

(defcustom neomacs-cursor-compass-opacity 25
  "Compass rose opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-compass)
                    (boundp 'neomacs-cursor-compass)
                    neomacs-cursor-compass)
           (neomacs-set-cursor-compass t
            (if (boundp 'neomacs-cursor-compass-color)
                neomacs-cursor-compass-color nil)
            (if (boundp 'neomacs-cursor-compass-size)
                neomacs-cursor-compass-size nil)
            (if (boundp 'neomacs-cursor-compass-speed)
                neomacs-cursor-compass-speed nil)
            val))))

;; Warp/distortion grid effect
(declare-function neomacs-set-warp-grid "neomacsterm.c")

(defcustom neomacs-warp-grid nil
  "Enable warp/distortion grid effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-warp-grid)
           (neomacs-set-warp-grid val))))

(defcustom neomacs-warp-grid-color "#4D80E5"
  "Color of the warp grid lines."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-warp-grid)
                    (boundp 'neomacs-warp-grid)
                    neomacs-warp-grid)
           (neomacs-set-warp-grid t val))))

(defcustom neomacs-warp-grid-density 20
  "Grid cell density (cells across width)."
  :type '(integer :tag "Density")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-warp-grid)
                    (boundp 'neomacs-warp-grid)
                    neomacs-warp-grid)
           (neomacs-set-warp-grid t
            (if (boundp 'neomacs-warp-grid-color)
                neomacs-warp-grid-color nil)
            val))))

(defcustom neomacs-warp-grid-amplitude 5
  "Distortion amplitude in pixels."
  :type '(integer :tag "Amplitude (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-warp-grid)
                    (boundp 'neomacs-warp-grid)
                    neomacs-warp-grid)
           (neomacs-set-warp-grid t
            (if (boundp 'neomacs-warp-grid-color)
                neomacs-warp-grid-color nil)
            (if (boundp 'neomacs-warp-grid-density)
                neomacs-warp-grid-density nil)
            val))))

(defcustom neomacs-warp-grid-speed 100
  "Animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-warp-grid)
                    (boundp 'neomacs-warp-grid)
                    neomacs-warp-grid)
           (neomacs-set-warp-grid t
            (if (boundp 'neomacs-warp-grid-color)
                neomacs-warp-grid-color nil)
            (if (boundp 'neomacs-warp-grid-density)
                neomacs-warp-grid-density nil)
            (if (boundp 'neomacs-warp-grid-amplitude)
                neomacs-warp-grid-amplitude nil)
            val))))

(defcustom neomacs-warp-grid-opacity 15
  "Warp grid opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-warp-grid)
                    (boundp 'neomacs-warp-grid)
                    neomacs-warp-grid)
           (neomacs-set-warp-grid t
            (if (boundp 'neomacs-warp-grid-color)
                neomacs-warp-grid-color nil)
            (if (boundp 'neomacs-warp-grid-density)
                neomacs-warp-grid-density nil)
            (if (boundp 'neomacs-warp-grid-amplitude)
                neomacs-warp-grid-amplitude nil)
            (if (boundp 'neomacs-warp-grid-speed)
                neomacs-warp-grid-speed nil)
            val))))

;; Cursor DNA helix trail effect
(declare-function neomacs-set-cursor-dna-helix "neomacsterm.c")

(defcustom neomacs-cursor-dna-helix nil
  "Enable cursor DNA helix trail effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-dna-helix)
           (neomacs-set-cursor-dna-helix val))))

(defcustom neomacs-cursor-dna-helix-color1 "#4DE580"
  "Primary DNA strand color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-dna-helix)
                    (boundp 'neomacs-cursor-dna-helix)
                    neomacs-cursor-dna-helix)
           (neomacs-set-cursor-dna-helix t val))))

(defcustom neomacs-cursor-dna-helix-color2 "#804DE5"
  "Secondary DNA strand color."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-dna-helix)
                    (boundp 'neomacs-cursor-dna-helix)
                    neomacs-cursor-dna-helix)
           (neomacs-set-cursor-dna-helix t
            (if (boundp 'neomacs-cursor-dna-helix-color1)
                neomacs-cursor-dna-helix-color1 nil)
            val))))

(defcustom neomacs-cursor-dna-helix-radius 12
  "Helix radius in pixels."
  :type '(integer :tag "Radius (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-dna-helix)
                    (boundp 'neomacs-cursor-dna-helix)
                    neomacs-cursor-dna-helix)
           (neomacs-set-cursor-dna-helix t
            (if (boundp 'neomacs-cursor-dna-helix-color1)
                neomacs-cursor-dna-helix-color1 nil)
            (if (boundp 'neomacs-cursor-dna-helix-color2)
                neomacs-cursor-dna-helix-color2 nil)
            val))))

(defcustom neomacs-cursor-dna-helix-speed 150
  "Helix rotation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-dna-helix)
                    (boundp 'neomacs-cursor-dna-helix)
                    neomacs-cursor-dna-helix)
           (neomacs-set-cursor-dna-helix t
            (if (boundp 'neomacs-cursor-dna-helix-color1)
                neomacs-cursor-dna-helix-color1 nil)
            (if (boundp 'neomacs-cursor-dna-helix-color2)
                neomacs-cursor-dna-helix-color2 nil)
            (if (boundp 'neomacs-cursor-dna-helix-radius)
                neomacs-cursor-dna-helix-radius nil)
            val))))

(defcustom neomacs-cursor-dna-helix-opacity 30
  "DNA helix opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-dna-helix)
                    (boundp 'neomacs-cursor-dna-helix)
                    neomacs-cursor-dna-helix)
           (neomacs-set-cursor-dna-helix t
            (if (boundp 'neomacs-cursor-dna-helix-color1)
                neomacs-cursor-dna-helix-color1 nil)
            (if (boundp 'neomacs-cursor-dna-helix-color2)
                neomacs-cursor-dna-helix-color2 nil)
            (if (boundp 'neomacs-cursor-dna-helix-radius)
                neomacs-cursor-dna-helix-radius nil)
            (if (boundp 'neomacs-cursor-dna-helix-speed)
                neomacs-cursor-dna-helix-speed nil)
            val))))

;; Prism/rainbow edge effect
(declare-function neomacs-set-prism-edge "neomacsterm.c")

(defcustom neomacs-prism-edge nil
  "Enable prism/rainbow edge effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-prism-edge)
           (neomacs-set-prism-edge val))))

(defcustom neomacs-prism-edge-width 6
  "Spectrum band width in pixels."
  :type '(integer :tag "Width (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-prism-edge)
                    (boundp 'neomacs-prism-edge)
                    neomacs-prism-edge)
           (neomacs-set-prism-edge t val))))

(defcustom neomacs-prism-edge-speed 100
  "Prism animation speed * 100."
  :type '(integer :tag "Speed")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-prism-edge)
                    (boundp 'neomacs-prism-edge)
                    neomacs-prism-edge)
           (neomacs-set-prism-edge t
            (if (boundp 'neomacs-prism-edge-width)
                neomacs-prism-edge-width nil)
            val))))

(defcustom neomacs-prism-edge-saturation 80
  "Color saturation (0-100)."
  :type '(integer :tag "Saturation (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-prism-edge)
                    (boundp 'neomacs-prism-edge)
                    neomacs-prism-edge)
           (neomacs-set-prism-edge t
            (if (boundp 'neomacs-prism-edge-width)
                neomacs-prism-edge-width nil)
            (if (boundp 'neomacs-prism-edge-speed)
                neomacs-prism-edge-speed nil)
            val))))

(defcustom neomacs-prism-edge-opacity 25
  "Prism edge opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-prism-edge)
                    (boundp 'neomacs-prism-edge)
                    neomacs-prism-edge)
           (neomacs-set-prism-edge t
            (if (boundp 'neomacs-prism-edge-width)
                neomacs-prism-edge-width nil)
            (if (boundp 'neomacs-prism-edge-speed)
                neomacs-prism-edge-speed nil)
            (if (boundp 'neomacs-prism-edge-saturation)
                neomacs-prism-edge-saturation nil)
            val))))

;; Cursor pendulum swing effect
(declare-function neomacs-set-cursor-pendulum "neomacsterm.c")

(defcustom neomacs-cursor-pendulum nil
  "Enable cursor pendulum swing effect."
  :type 'boolean
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-cursor-pendulum)
           (neomacs-set-cursor-pendulum val))))

(defcustom neomacs-cursor-pendulum-color "#E5B34D"
  "Color of pendulum swing arc."
  :type 'color
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pendulum)
                    (boundp 'neomacs-cursor-pendulum)
                    neomacs-cursor-pendulum)
           (neomacs-set-cursor-pendulum t val))))

(defcustom neomacs-cursor-pendulum-arc-length 40
  "Arc length in pixels."
  :type '(integer :tag "Arc length (pixels)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pendulum)
                    (boundp 'neomacs-cursor-pendulum)
                    neomacs-cursor-pendulum)
           (neomacs-set-cursor-pendulum t
            (if (boundp 'neomacs-cursor-pendulum-color)
                neomacs-cursor-pendulum-color nil)
            val))))

(defcustom neomacs-cursor-pendulum-damping 50
  "Swing damping factor (0-100, higher = faster decay)."
  :type '(integer :tag "Damping (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pendulum)
                    (boundp 'neomacs-cursor-pendulum)
                    neomacs-cursor-pendulum)
           (neomacs-set-cursor-pendulum t
            (if (boundp 'neomacs-cursor-pendulum-color)
                neomacs-cursor-pendulum-color nil)
            (if (boundp 'neomacs-cursor-pendulum-arc-length)
                neomacs-cursor-pendulum-arc-length nil)
            val))))

(defcustom neomacs-cursor-pendulum-opacity 30
  "Pendulum swing opacity (0-100)."
  :type '(integer :tag "Opacity (0-100)")
  :group 'neomacs
  :set (lambda (sym val)
         (set-default sym val)
         (when (and (fboundp 'neomacs-set-cursor-pendulum)
                    (boundp 'neomacs-cursor-pendulum)
                    neomacs-cursor-pendulum)
           (neomacs-set-cursor-pendulum t
            (if (boundp 'neomacs-cursor-pendulum-color)
                neomacs-cursor-pendulum-color nil)
            (if (boundp 'neomacs-cursor-pendulum-arc-length)
                neomacs-cursor-pendulum-arc-length nil)
            (if (boundp 'neomacs-cursor-pendulum-damping)
                neomacs-cursor-pendulum-damping nil)
            val))))

;; Provide the feature
(provide 'neomacs-win)
(provide 'term/neomacs-win)

;;; neomacs-win.el ends here
