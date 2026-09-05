;;; neo-win.el --- parse relevant switches and set up for Neomacs  -*- lexical-binding: t -*-

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

;; Neomacs is a GPU-accelerated display backend implemented in Rust.

;;; Code:

(eval-when-compile (require 'cl-lib))
(unless (featurep 'neomacs)
  (error "%s: Loading neo-win.el but not compiled with NEOMACS"
         invocation-name))

;; The GUI runtime loader loads common-win first; keep the dependency explicit
;; so loading this file directly has the same prerequisites.
(require 'term/common-win)
;; This is an implementation fragment, not a public feature: loading it must not
;; add a Neomacs-only symbol to `features'.  Reapplying its key defaults is
;; idempotent and also keeps direct source-loading robust outside a dumped image.
(load "term/neo-preload")
;; `define-minor-mode' is a MACRO, so `easy-mmode' is a compile-time
;; dependency and not a load-time one: a `:global t' mode without `:keymap'
;; expands to `defcustom' / `defun' / `add-minor-mode', all of which are
;; already in the dumped image.  No GNU `term/FOO-win.el' loads `easy-mmode'
;; -- `grep -c define-minor-mode' answers 0 for all eight of them, and the two
;; under `lisp/term/' that do have one (`tvi970.el:102', `vt100.el:41') are TTY
;; files that are never dumped -- so requiring it at load time made this file
;; the only window-system file in either editor that drags the library in.
;; `eval-when-compile' still evaluates its body when the file is loaded as
;; SOURCE, so the macro is available there too.  DIVERGENCES.md 194.
(eval-when-compile (require 'easy-mmode))
(require 'frame)
(require 'mouse)
(require 'scroll-bar)
(require 'select)
(require 'faces)
(require 'menu-bar)
(require 'fontset)

(defvar x-invocation-args)
(defvar x-command-line-resources)

(defun neomacs-suspend-error ()
  "Don't allow suspending if any of the frames are Neomacs frames."
  (if (memq 'neo (mapcar 'window-system (frame-list)))
      (error "Cannot suspend Emacs while a Neomacs GUI frame exists")))

(defalias 'x-win-suspend-error #'neomacs-suspend-error)

(defvar neomacs-initialized nil
  "Non-nil if Neomacs windowing has been initialized.")

(declare-function x-handle-args "common-win" (args))
(declare-function x-open-connection "neomacsfns.c"
                  (display &optional xrm-string must-succeed))
(defvar initial-window-system)

;; `x-display-name' is NOT re-declared here.  It belongs to
;; `term/common-win.el' (:145), which this file requires above and which
;; `lisp/loadup.el' now preloads, exactly as GNU's six window-system branches
;; do -- and no `term/FOO-win.el' in GNU re-declares it either; `term/x-win.el'
;; only writes `(defvar x-display-name)' at :1223, the value-less form that
;; silences the compiler.  A second `defvar' WITH a docstring is not a no-op:
;; `internal--define-uninitialized-variable' (src/eval.c:911) installs the
;; docstring unconditionally, so the copy that used to sit here -- "The display
;; name specifying the display to connect to." -- overwrote GNU's text ("The
;; name of the window display on which Emacs was started ...") the moment a GUI
;; frame opened.  DIVERGENCES.md 179.

(add-to-list 'display-format-alist '(".*" . neo))

(defun neomacs--window-setup ()
  "Finish Neomacs GUI setup after GNU startup completes frame setup."
  (remove-hook 'window-setup-hook #'neomacs--window-setup)

  ;; Cursor blinking is handled by the render thread.  Sync blink state to
  ;; the render thread and suppress the Emacs-side blink timer.
  (neomacs--setup-cursor-blink)

  ;; Enable pixel-precise scrolling for smooth touchpad support.
  (when (fboundp 'pixel-scroll-precision-mode)
    (pixel-scroll-precision-mode 1)))

;; Do the actual window system setup here.
(cl-defmethod window-system-initialization (&context (window-system neo)
                                            &optional display)
  "Initialize the Neomacs window system.
WINDOW-SYSTEM is, aptly, `neo'.
DISPLAY is the name of the display Emacs should connect to."
  (cl-assert (not neomacs-initialized))

  ;; Make sure we have a valid resource name.
  (when (boundp 'x-resource-name)
    (unless (stringp x-resource-name)
      (let (i)
	(setq x-resource-name (copy-sequence invocation-name))
	;; Change any . or * characters in x-resource-name to hyphens.
	(while (setq i (string-match "[.*]" x-resource-name))
	  (aset x-resource-name i ?-)))))

  ;; Open the display connection
  (x-open-connection (or display
                         (setq x-display-name (or (getenv "DISPLAY" (selected-frame))
                                                  (getenv "DISPLAY"))))
		     x-command-line-resources
		     ;; Exit Emacs with fatal error if this fails and we
                     ;; are the initial display.
		     (eq initial-window-system 'neo))

  ;; Setup the default fontset.
  (create-default-fontset)
  ;; Create the standard fontset.
  (condition-case err
      (create-fontset-from-fontset-spec standard-fontset-spec t)
    (error (display-warning
            'initialization
            (format "Creation of the standard fontset failed: %s" err)
            :error)))

  ;; Create fontsets specified in X resources "Fontset-N" (N is 0, 1, ...).
  (create-fontset-from-x-resource)

  (add-hook 'suspend-hook #'neomacs-suspend-error)
  (add-hook 'window-setup-hook #'neomacs--window-setup)

  (setq neomacs-initialized t))

;; Handle args function (required by common-win)
(cl-defmethod handle-args-function (args &context (window-system neo))
  (x-handle-args args))

;; Frame creation for Neomacs
;; Use x-create-frame-with-faces to properly initialize faces with colors
(cl-defmethod frame-creation-function (params &context (window-system neo))
  (x-create-frame-with-faces params))

;; Typed visual configuration owned by the Rust display protocol.
(declare-function neomacs-effect-set "neomacsterm.c" (effect &rest properties))
(declare-function neomacs-effect-get "neomacsterm.c" (effect))
(declare-function neomacs-effect-reset "neomacsterm.c" (effect))
(declare-function neomacs-effects-apply "neomacsterm.c" (profile))
(declare-function neomacs-effect-names "neomacsterm.c" (&optional scope))

;; Clipboard integration
(declare-function neomacs-clipboard-set "neomacsfns.c" (text))
(declare-function neomacs-clipboard-get "neomacsfns.c" ())

;; Primary selection integration
(declare-function neomacs-primary-selection-set "neomacsfns.c" (text))
(declare-function neomacs-primary-selection-get "neomacsfns.c" ())
(declare-function neomacs-primary-selection-owner "neomacsfns.c" ())

(defun neomacs--sync-cursor-blink ()
  "Sync `blink-cursor-mode' state to the render thread."
  (when (fboundp 'neomacs-effect-set)
    (neomacs-effect-set
     'cursor-blink
     :enabled (and (boundp 'blink-cursor-mode) blink-cursor-mode t)
     :interval (if (boundp 'blink-cursor-interval) blink-cursor-interval 0.5))))

(defun neomacs--cancel-core-cursor-blink-timers ()
  "Cancel GNU Lisp cursor blink timers.
Neomacs renders cursor blink in the Rust render thread, so the Lisp timers
should not keep waking the command loop while Emacs is otherwise idle."
  (when (and (boundp 'blink-cursor-timer) blink-cursor-timer)
    (cancel-timer blink-cursor-timer)
    (setq blink-cursor-timer nil))
  (when (and (boundp 'blink-cursor-idle-timer) blink-cursor-idle-timer)
    (cancel-timer blink-cursor-idle-timer)
    (setq blink-cursor-idle-timer nil)))

(defun neomacs--blink-cursor-check ()
  "Neomacs replacement for `blink-cursor-check'.
Return the same focus/mode predicate as GNU, but leave visual blinking to the
render thread instead of creating Lisp timers."
  (remove-hook 'post-command-hook #'blink-cursor-check)
  (neomacs--cancel-core-cursor-blink-timers)
  (neomacs--sync-cursor-blink)
  (blink-cursor--should-blink))

(defun neomacs--blink-cursor-start ()
  "Neomacs replacement for `blink-cursor-start'."
  (neomacs--blink-cursor-check))

(defun neomacs--blink-cursor-start-idle-timer ()
  "Neomacs replacement for `blink-cursor--start-idle-timer'."
  (neomacs--blink-cursor-check))

(defun neomacs--blink-cursor-start-timer ()
  "Neomacs replacement for `blink-cursor--start-timer'."
  (neomacs--blink-cursor-check))

(defun neomacs--blink-cursor-timer-function ()
  "Neomacs replacement for `blink-cursor-timer-function'."
  (neomacs--blink-cursor-check)
  nil)

(defun neomacs--sync-cursor-blink-after-mode (&rest _)
  "Sync render-thread cursor blink after `blink-cursor-mode' changes."
  (neomacs--cancel-core-cursor-blink-timers)
  (neomacs--sync-cursor-blink))

(defun neomacs--override-cursor-blink-function (symbol function)
  "Install FUNCTION as Neomacs override advice for SYMBOL."
  (advice-remove symbol function)
  (advice-add symbol :override function `((name . ,function))))

(defun neomacs--setup-cursor-blink ()
  "Set up render-thread cursor blinking.
Syncs current blink state and advises `blink-cursor-mode' for future changes.
Also suppresses the Emacs-side blink timer since the render thread handles it."
  (neomacs--cancel-core-cursor-blink-timers)
  (neomacs--override-cursor-blink-function
   'blink-cursor-check #'neomacs--blink-cursor-check)
  (neomacs--override-cursor-blink-function
   'blink-cursor-start #'neomacs--blink-cursor-start)
  (neomacs--override-cursor-blink-function
   'blink-cursor--start-idle-timer #'neomacs--blink-cursor-start-idle-timer)
  (neomacs--override-cursor-blink-function
   'blink-cursor--start-timer #'neomacs--blink-cursor-start-timer)
  (neomacs--override-cursor-blink-function
   'blink-cursor-timer-function #'neomacs--blink-cursor-timer-function)
  (when (fboundp 'blink-cursor-mode)
    (advice-remove 'blink-cursor-mode #'neomacs--sync-cursor-blink-after-mode)
    (advice-add 'blink-cursor-mode :after
                #'neomacs--sync-cursor-blink-after-mode
                '((name . neomacs-sync-blink))))
  (neomacs--sync-cursor-blink))

(defun x-clipboard-yank ()
  "Insert the clipboard contents, or the last stretch of killed text."
  (declare (obsolete clipboard-yank "25.1"))
  (interactive "*")
  (let ((clipboard-text (gui--selection-value-internal 'CLIPBOARD))
        (select-enable-clipboard t))
    (when (and clipboard-text (> (length clipboard-text) 0))
      ;; Avoid asserting ownership of CLIPBOARD, which will cause
      ;; `gui-selection-value' to return nil in the future.
      (let ((select-enable-clipboard nil))
        (kill-new clipboard-text)))
    (yank)))

;; Selection protocol (CLIPBOARD + PRIMARY)
(cl-defmethod gui-backend-set-selection (selection value
                                         &context (window-system neo))
  "Set SELECTION to VALUE on the Neomacs display.
SELECTION is a symbol like `CLIPBOARD' or `PRIMARY'."
  (let ((text (and value
                   (if (stringp value) value
                     (substring-no-properties (symbol-name value))))))
    (cond
     ((eq selection 'CLIPBOARD)
      (when (fboundp 'neomacs-clipboard-set)
        (neomacs-clipboard-set text)))
     ((eq selection 'PRIMARY)
      (when (fboundp 'neomacs-primary-selection-set)
        (neomacs-primary-selection-set text))))))

(cl-defmethod gui-backend-selection-owner-p (selection
                                             &context (window-system neo))
  "Return non-nil if this Emacs owns SELECTION on the Neomacs display.
nil means PRIMARY, as in GNU (nsselect.m:506, w32-win.el:450).  Ownership of
PRIMARY comes from the native backend that owns its state.  The
system CLIPBOARD changes hands without notice, so this reports nil for it,
and `gui--selection-value-internal' only trusts this predicate for CLIPBOARD
on x and haiku anyway (lisp/select.el:230-236).

`deactivate-mark' (lisp/simple.el:7056-7066) republishes the region to
PRIMARY only when this predicate holds or nobody owns PRIMARY; without it
an earlier PRIMARY value stayed stale on displays whose PRIMARY is
process-local.  On Linux, an ownership result of `unknown' is conservative:
it must not be treated as ours and bypass GNU's Bug#11772 foreign-owner
guard."
  (and (memq selection '(nil PRIMARY))
       (fboundp 'neomacs-primary-selection-owner)
       (eq (neomacs-primary-selection-owner) 'this-process)))

(cl-defmethod gui-backend-get-selection (selection-symbol _target-type
                                          &context (window-system neo)
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
                                              &context (window-system neo))
  "Return non-nil if SELECTION exists on the Neomacs display."
  (cond
   ((eq selection 'CLIPBOARD)
    (when (fboundp 'neomacs-clipboard-get)
      (let ((text (neomacs-clipboard-get)))
        (and text (not (string-empty-p text))))))
   ((eq selection 'PRIMARY)
    (when (fboundp 'neomacs-primary-selection-get)
      (not (null (neomacs-primary-selection-get)))))))

(defcustom x-display-cursor-at-start-of-preedit-string nil
  "If non-nil, display the cursor at the start of any pre-edit text."
  :version "29.1"
  :type 'boolean
  :group 'neomacs)

(defvar x-preedit-overlay nil
  "The overlay currently used to display preedit text from a compose sequence.")

(defun x-clear-preedit-text ()
  "Clear the pre-edit overlay and remove itself from `pre-command-hook'."
  (when x-preedit-overlay
    (delete-overlay x-preedit-overlay)
    (setq x-preedit-overlay nil))
  (remove-hook 'pre-command-hook #'x-clear-preedit-text))

(defun x-preedit-text (event)
  "Display preedit text from a compose sequence in EVENT."
  (interactive "e")
  (when x-preedit-overlay
    (delete-overlay x-preedit-overlay)
    (setq x-preedit-overlay nil)
    (remove-hook 'pre-command-hook #'x-clear-preedit-text))
  (when (nth 1 event)
    (let ((string (propertize (nth 1 event) 'face '(:underline t))))
      (setq x-preedit-overlay (make-overlay (point) (point)))
      (add-hook 'pre-command-hook #'x-clear-preedit-text)
      (overlay-put x-preedit-overlay 'window (selected-window))
      (overlay-put x-preedit-overlay 'before-string
                   (if x-display-cursor-at-start-of-preedit-string
                       (propertize string 'cursor t)
                     string)))))

(defun x-device-class (name)
  "Return the device class of NAME.
Users should not call this function; see `device-class' instead."
  (and name
       (let ((downcased-name (downcase name)))
         (cond
          ((string-match-p "XTEST" name) 'test)
          ((string= "Virtual core pointer" name) 'core-pointer)
          ((string= "Virtual core keyboard" name) 'core-keyboard)
          ((string-match-p "eraser" downcased-name) 'eraser)
          ((string-match-p " pad" downcased-name) 'pad)
          ((or (string-match-p "wacom" downcased-name)
               (string-match-p "pen" downcased-name)
               (string-match-p "stylus" downcased-name))
           'pen)
          ((or (string-prefix-p "xwayland-touch:" name)
               (string-match-p "touchscreen" downcased-name))
           'touchscreen)
          ((or (string-match-p "trackpoint" downcased-name)
               (string-match-p "stick" downcased-name))
           'trackpoint)
          ((or (string-match-p "mouse" downcased-name)
               (string-match-p "optical" downcased-name)
               (string-match-p "pointer" downcased-name))
           'mouse)
          ((string-match-p "cursor" downcased-name) 'puck)
          ((or (string-match-p "keyboard" downcased-name)
               (string= name "USB USB Keykoard"))
           'keyboard)
          ((string-match-p "button" downcased-name) 'power-button)
          ((string-match-p "touchpad" downcased-name) 'touchpad)
          ((or (string-match-p "midi" downcased-name)
               (string-match-p "piano" downcased-name))
           'piano)
          ((or (string-match-p "wskbd" downcased-name)
               (and (string-match-p "/dev" downcased-name)
                    (string-match-p "kbd" downcased-name)))
           'keyboard)))))

;; The value-less form, which is what GNU writes here: `term/x-win.el:1634'
;; is `(defvar x-input-coding-function)', because the variable itself is
;; `src/xterm.c:32993' `DEFVAR_LISP' and this file only silences the compiler
;; before `setq'ing it below (GNU does the same at `term/x-win.el:1654').  A
;; `defvar' WITH a docstring is not a no-op:
;; `internal--define-uninitialized-variable' installs the docstring
;; unconditionally (`src/eval.c:909-912'), so the copy that used to sit here --
;; "Function used to determine the coding system for input method text." --
;; replaced GNU's C text ("Function used to determine the coding system used by
;; input methods.") the moment a GUI frame opened, and replaced the integer
;; `variable-documentation' the snarf installs with a string, which is exactly
;; what `lisp/help-fns.el:531-538' reads to decide a variable is defined in C.
;; Same defect as the `x-display-name' one ledger 179 deleted at :70, 296 lines
;; further down the same file.  DIVERGENCES.md 189.
(defvar x-input-coding-function)

(defun x-get-input-coding-system (x-locale)
  "Return a coding system for the locale X-LOCALE.
Return a coding system able to decode text sent with the input
method locale X-LOCALE, or nil if no coding system was found."
  (if (equal x-locale "C")
      'ascii
    (let ((locale (locale-translate (downcase x-locale))))
      (or (locale-name-match locale locale-preferred-coding-systems)
          (when locale
            (if (string-match "\\.\\([^@]+\\)" locale)
                (locale-charset-to-coding-system
                 (match-string 1 locale))))
          (let ((language-name
                 (locale-name-match locale locale-language-names)))
            (and (consp language-name) (cdr language-name)))))))

(setq x-input-coding-function #'x-get-input-coding-system)

(define-key special-event-map [preedit-text] #'x-preedit-text)

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
(defun neomacs-menu-bar-open (&optional frame initial-x)
  "Open the Neomacs menu bar.
This follows the X/PGTK backend shape: when the frame has a menu bar,
the backend menu accelerator is used if present; otherwise fall back to
`popup-menu' so Lisp still drives command lookup and execution."
  (interactive "i")
  (cond
   ((and (not (zerop (or (frame-parameter frame 'menu-bar-lines) 0)))
         (fboundp 'accelerate-menu))
    (accelerate-menu frame))
   (t
    (force-mode-line-update)
    (redisplay)
    (let* ((x (max (or initial-x 0) tty-menu--initial-menu-x))
           (menu (menu-bar-menu-at-x-y x 0 frame)))
      (popup-menu (or
                   (lookup-key-ignore-too-long
                    global-map (vector 'menu-bar menu))
                   (lookup-key-ignore-too-long
                    (current-local-map) (vector 'menu-bar menu))
                   (cdar (minor-mode-key-binding (vector 'menu-bar menu)))
                   (mouse-menu-bar-map))
                  (posn-at-x-y x 0 frame t) nil t)))))

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
  :init-value nil
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

;;; GPU Toolbar

(declare-function neomacs-set-toolbar-config "neomacsterm.c"
  (&optional icon-size padding))

(defcustom neomacs-toolbar-icon-theme 'vscode-like
  "Icon theme used for GPU tool-bar image lookup.
The value `gnu' keeps GNU Emacs image lookup unchanged.  Other values
replace recognized GNU tool-bar image names with SVG files from
\"etc/toolbar-icons\" and fall back to GNU images when no themed icon exists."
  :type '(choice (const :tag "GNU Emacs images" gnu)
                 (const :tag "Neomacs" neomacs)
                 (const :tag "VS Code-like" vscode-like)
                 (const :tag "JetBrains-like" jetbrains-like)
                 (const :tag "Atom-like" atom-like)
                 (const :tag "Material" material))
  :group 'frames)

(defcustom neomacs-toolbar-icon-directory nil
  "Directory containing user-provided SVG tool-bar icons.
When non-nil, this directory is checked before `neomacs-toolbar-icon-theme'.
Files should use GNU tool-bar image base names, for example \"save.svg\" or
\"mail/compose.svg\"."
  :type '(choice (const :tag "Use selected theme" nil)
                 directory)
  :group 'frames)

(defcustom neomacs-toolbar-icon-size 24
  "Size of toolbar icons in pixels."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-toolbar-config)
           (neomacs-set-toolbar-config
            val (if (boundp 'neomacs-toolbar-padding)
                    neomacs-toolbar-padding 6)))))

(defcustom neomacs-toolbar-padding 6
  "Padding around toolbar icons in pixels."
  :type 'integer
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-toolbar-config)
           (neomacs-set-toolbar-config
            (if (boundp 'neomacs-toolbar-icon-size)
                neomacs-toolbar-icon-size 24)
            val))))

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

;;; Font metrics backend

(declare-function neomacs-set-font-backend "neomacsterm.c" (backend))

(defcustom neomacs-font-backend 'cosmic
  "Font metrics backend for the layout engine.
`cosmic' (default) uses cosmic-text font metrics, matching the render
thread's font resolution.  This eliminates width mismatches between
layout and rendering when C fontconfig and cosmic-text resolve
different font files.
`emacs' uses the legacy C/fontconfig font metrics."
  :type '(choice (const :tag "Emacs C (fontconfig)" emacs)
                 (const :tag "Cosmic-text" cosmic))
  :group 'frames
  :set (lambda (sym val)
         (set-default sym val)
         (when (fboundp 'neomacs-set-font-backend)
           (neomacs-set-font-backend val))))

;;; Background gradient

(defcustom neomacs-effects nil
  "Visual effect and animation profile for the Neomacs renderer.
Each entry has the form (EFFECT :PROPERTY VALUE ...).  EFFECT and its
accepted properties come from the Rust effect registry; inspect them with
`neomacs-effect-names' and `neomacs-effect-get'.

Changing this option replaces the complete effect profile atomically.  For
example:

  ((cursor-glow :enabled t :color \"#66CCFF\" :radius 48)
   (rain-effect :enabled t :drop-count 30 :speed 120.0)
   (scroll-transition :effect page-curl :easing spring))

Use `neomacs-effect-set' for an incremental update to one effect and
`neomacs-effect-reset' to restore one effect's Rust-defined defaults."
  :type '(repeat (sexp :tag "Effect entry"))
  :group 'frames
  :set (lambda (symbol value)
         (neomacs-effects-apply value)
         (set-default symbol value)))

(defvar-local neomacs-cursor-effect nil
  "Per-buffer cursor effect profile.
The value is one entry or a list of entries in `neomacs-effects' format.
Only cursor effects are used when rendering this buffer's cursor.")

;; Provide the feature
(provide 'neo-win)
(provide 'term/neo-win)

;;; neo-win.el ends here
