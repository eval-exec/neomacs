;;; font-frame-display.el --- Font listing with live GUI and TTY frames -*- lexical-binding: t -*-

(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defvar font-frame-observations nil)
(defvar font-frame-passes nil)

(defun font-frame-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font-frame-observe (label expected operation)
  (condition-case problem
      (let* ((value (funcall operation))
             (passed (equal value expected)))
        (push passed font-frame-passes)
        (push `((label . ,label) (passed . ,(if passed t :json-false))
                (value . ,(format "%S" value))
                (expected . ,(format "%S" expected)))
              font-frame-observations))
    (error
     (push nil font-frame-passes)
     (push `((label . ,label) (passed . :json-false)
             (error . ,(format "%S" problem)))
           font-frame-observations))))

(defun font-frame-script-case (label expected spec)
  (font-frame-observe
   label expected
   (lambda () (not (null (list-fonts spec nil 1))))))

(defun font-frame-script-cases ()
  ;; Every named script has a unique specification. GNU caches script
  ;; selection, so this never changes representatives for a previously used
  ;; specification. Explicit nil must precede an unconstrained Hack query.
  (let ((script-representative-chars
         '((nil . [#x4e00])
           (font-frame-all ?A #x4e00)
           (font-frame-any . [?A #x4e00])
           (font-frame-empty . [])
           (font-frame-invalid . [nil t font-frame-not-a-character])
           (font-frame-legacy-all ?A #x4e00)
           (font-frame-legacy-any . [#x4e00]))))
    (font-frame-script-case
     "script-explicit-nil-han" nil (font-spec :family "Hack" :script nil))
    (font-frame-script-case
     "script-missing-property" t (font-spec :family "Hack"))
    (font-frame-script-case
     "script-all-latin-and-han" nil
     (font-spec :family "Hack" :script 'font-frame-all))
    (font-frame-script-case
     "script-any-latin-or-han" t
     (font-spec :family "Hack" :script 'font-frame-any))
    (font-frame-script-case
     "script-any-empty" nil (font-spec :family "Hack" :script 'font-frame-empty))
    (font-frame-script-case
     "script-any-invalid" nil (font-spec :family "Hack" :script 'font-frame-invalid))
    (font-frame-script-case
     "script-unknown" t (font-spec :family "Hack" :script 'font-frame-unknown))
    (font-frame-script-case
     "script-legacy-overrides-all" t
     (font-spec :family "Hack" :registry "iso8859-2" :script 'font-frame-legacy-all))
    (font-frame-script-case
     "script-legacy-overrides-any" t
     (font-spec :family "Hack" :registry "iso8859-2" :script 'font-frame-legacy-any))))

(defun font-frame-probe ()
  (condition-case problem
      (let* ((gui (selected-frame))
             (pattern "-SRC-Hack-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1")
             tty names)
        (font-frame-script-cases)
        (setq tty (make-terminal-frame
                   `((tty . ,(getenv "NEOMACS_FONT_FRAME_TTY"))
                     (tty-type . "xterm-256color")
                     (name . "private-font-frame-tty"))))
        (select-frame gui)
        (setq names (x-list-fonts pattern nil gui 1))
        (unless (and (display-graphic-p gui) (not (display-graphic-p tty))
                     (= (length names) 1) (stringp (car names)))
          (error "Mixed-frame setup did not supply the pinned Hack font: %S" names))
        ;; GNU clears FACE for a TTY target before applying WIDTH. Listing
        ;; then uses the selected GUI frame's font driver.
        (font-frame-observe
         "gui-selected-tty-default-face-negative-width" names
         (lambda () (x-list-fonts pattern 'default tty 1 -1)))
        (font-frame-observe
         "gui-selected-tty-invalid-face-negative-width" names
         (lambda () (x-list-fonts pattern [] tty 1 -1)))
        (select-frame tty)
        (font-frame-observe
         "tty-selected-nil-frame" nil
         (lambda () (x-list-fonts pattern nil nil 1)))
        (font-frame-observe
         "tty-selected-tty-frame" nil
         (lambda () (x-list-fonts pattern nil tty 1)))
        ;; A live explicit GUI frame remains usable when the selected frame
        ;; and the dynamically bound startup symbols both describe a TTY.
        (let ((window-system nil) (initial-window-system nil))
          (font-frame-observe
           "tty-selected-gui-frame-globals-nil" names
           (lambda () (x-list-fonts pattern nil gui 1))))
        (let ((passed (not (memq nil font-frame-passes))))
          (font-frame-write
           `((passed . ,(if passed t :json-false)) (version . ,(emacs-version))
             (gui-graphic . t) (tty-graphic . :json-false)
             (script-cases . 9)
             (observations . ,(vconcat (reverse font-frame-observations)))))
          (select-frame gui)
          (kill-emacs (if passed 0 1))))
    (error
     (font-frame-write
      `((passed . :json-false) (error . ,(format "%S" problem))
        (observations . ,(vconcat (reverse font-frame-observations)))))
     (kill-emacs 2))))

(run-at-time 0.7 nil #'font-frame-probe)
