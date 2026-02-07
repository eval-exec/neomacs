;;; neo-term.el --- GPU-accelerated terminal emulator for Neomacs -*- lexical-binding: t -*-

;; Copyright (C) 2026 Neomacs Contributors
;; License: GPL-3.0-or-later

;;; Commentary:

;; neo-term provides a GPU-accelerated terminal emulator backed by
;; alacritty_terminal + wgpu, integrated into the neomacs display engine.
;;
;; Three display modes:
;;   - Window mode: terminal fills a regular Emacs window/buffer
;;   - Inline mode: terminal embedded inline in a buffer (like images)
;;   - Floating mode: terminal rendered as a floating overlay
;;
;; Usage:
;;   M-x neo-term          -- open a terminal in the current window
;;   M-x neo-term-floating -- open a floating terminal overlay

;;; Code:

(require 'cl-lib)

(defgroup neo-term nil
  "GPU-accelerated terminal emulator."
  :group 'terminals
  :prefix "neo-term-")

(defcustom neo-term-shell nil
  "Shell program to run.  nil means use `explicit-shell-file-name' or $SHELL."
  :type '(choice (const :tag "Default" nil) string)
  :group 'neo-term)

(defcustom neo-term-default-cols 80
  "Default terminal width in columns."
  :type 'integer
  :group 'neo-term)

(defcustom neo-term-default-rows 24
  "Default terminal height in rows."
  :type 'integer
  :group 'neo-term)

(defvar neo-term--terminals (make-hash-table :test 'eql)
  "Hash table mapping terminal-id to terminal info plists.")

(defvar neo-term--next-buffer-num 1
  "Next buffer number for naming.")

;; These are C DEFUN primitives defined in neomacsterm.c
(declare-function neomacs-terminal-create "neomacsterm.c"
                  (cols rows mode &optional shell))
(declare-function neomacs-terminal-write "neomacsterm.c"
                  (terminal-id string))
(declare-function neomacs-terminal-resize "neomacsterm.c"
                  (terminal-id cols rows))
(declare-function neomacs-terminal-destroy "neomacsterm.c"
                  (terminal-id))
(declare-function neomacs-terminal-set-float "neomacsterm.c"
                  (terminal-id x y opacity))
(declare-function neomacs-terminal-get-text "neomacsterm.c"
                  (terminal-id))

(defun neo-term--shell-path ()
  "Return shell program to use."
  (or neo-term-shell
      (bound-and-true-p explicit-shell-file-name)
      (getenv "SHELL")
      "/bin/sh"))

(defun neo-term--create (cols rows mode &optional shell)
  "Create a terminal.  MODE is 0=Window, 1=Inline, 2=Floating.
Returns terminal ID or nil on failure."
  (let ((shell-path (or shell (neo-term--shell-path))))
    (condition-case err
        (let ((id (neomacs-terminal-create cols rows mode shell-path)))
          (when (and id (> id 0))
            (puthash id (list :id id :cols cols :rows rows :mode mode
                              :shell shell-path)
                     neo-term--terminals)
            id))
      (error
       (message "neo-term: failed to create terminal: %s" (error-message-string err))
       nil))))

(defun neo-term--destroy (terminal-id)
  "Destroy a terminal."
  (when terminal-id
    (ignore-errors (neomacs-terminal-destroy terminal-id))
    (remhash terminal-id neo-term--terminals)))

(defun neo-term--write (terminal-id string)
  "Send STRING to the terminal."
  (when (and terminal-id string)
    (neomacs-terminal-write terminal-id string)))

(defun neo-term--resize (terminal-id cols rows)
  "Resize a terminal."
  (when terminal-id
    (neomacs-terminal-resize terminal-id cols rows)))

;;; Major mode

(defvar neo-term-mode-map
  (let ((map (make-sparse-keymap)))
    ;; In raw mode, send everything to the terminal
    (define-key map [remap self-insert-command] #'neo-term-send-key)
    (define-key map (kbd "RET") #'neo-term-send-return)
    (define-key map (kbd "DEL") #'neo-term-send-backspace)
    (define-key map (kbd "TAB") #'neo-term-send-tab)
    (define-key map (kbd "C-c C-c") #'neo-term-send-ctrl-c)
    (define-key map (kbd "C-c C-d") #'neo-term-send-ctrl-d)
    (define-key map (kbd "C-c C-z") #'neo-term-send-ctrl-z)
    (define-key map (kbd "C-c C-\\") #'neo-term-send-ctrl-backslash)
    (define-key map (kbd "C-c C-q") #'neo-term-quit)
    map)
  "Keymap for `neo-term-mode'.")

(define-derived-mode neo-term-mode special-mode "NeoTerm"
  "Major mode for neo-term GPU terminal buffers.

\\{neo-term-mode-map}"
  :group 'neo-term
  (setq-local buffer-read-only t)
  (setq-local truncate-lines t)
  (setq-local neo-term--id nil))

(defvar-local neo-term--id nil
  "Terminal ID for this buffer.")

(defun neo-term-send-key ()
  "Send the current key to the terminal."
  (interactive)
  (when neo-term--id
    (let* ((keys (this-command-keys))
           (str (if (stringp keys) keys (string (event-basic-type last-input-event)))))
      (neo-term--write neo-term--id str))))

(defun neo-term-send-return ()
  "Send Return to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\r")))

(defun neo-term-send-backspace ()
  "Send Backspace to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\177")))

(defun neo-term-send-tab ()
  "Send Tab to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\t")))

(defun neo-term-send-ctrl-c ()
  "Send C-c to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\003")))

(defun neo-term-send-ctrl-d ()
  "Send C-d to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\004")))

(defun neo-term-send-ctrl-z ()
  "Send C-z to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\032")))

(defun neo-term-send-ctrl-backslash ()
  "Send C-\\ to the terminal."
  (interactive)
  (when neo-term--id (neo-term--write neo-term--id "\034")))

(defun neo-term-quit ()
  "Kill the terminal and close the buffer."
  (interactive)
  (when neo-term--id
    (neo-term--destroy neo-term--id))
  (kill-buffer))

;;; Public API

;;;###autoload
(defun neo-term ()
  "Open a new GPU-accelerated terminal in the current window."
  (interactive)
  (let* ((buf-name (format "*neo-term-%d*" neo-term--next-buffer-num))
         (buf (get-buffer-create buf-name))
         (id (neo-term--create neo-term-default-cols neo-term-default-rows
                               0))) ; mode=0 (Window)
    (unless id
      (kill-buffer buf)
      (error "Failed to create terminal"))
    (cl-incf neo-term--next-buffer-num)
    (switch-to-buffer buf)
    (neo-term-mode)
    (setq-local neo-term--id id)
    (message "neo-term: terminal %d created (%dx%d)"
             id neo-term-default-cols neo-term-default-rows)))

;;;###autoload
(defun neo-term-floating (&optional x y cols rows)
  "Open a floating GPU terminal overlay.
Optional X, Y set the floating position.
Optional COLS, ROWS set the terminal size."
  (interactive)
  (let* ((cols (or cols neo-term-default-cols))
         (rows (or rows neo-term-default-rows))
         (id (neo-term--create cols rows 2))) ; mode=2 (Floating)
    (unless id
      (error "Failed to create floating terminal"))
    (when (or x y)
      (neomacs-terminal-set-float
       id (or x 100.0) (or y 100.0) 0.95))
    (message "neo-term: floating terminal %d created (%dx%d)" id cols rows)
    id))

(provide 'neo-term)
;;; neo-term.el ends here
