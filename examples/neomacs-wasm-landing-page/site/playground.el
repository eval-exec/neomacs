;; NEO Emacs / your live Lisp playground
;;
;; Ready? Press C-x C-e now: hold Ctrl, press x,
;; then (still holding Ctrl) press e. Result: 3.
;; For each next example, put the cursor AFTER
;; its final closing parenthesis, then C-x C-e.

(+ 1 2)

;; 01 / Make the editor say hello
(message "Hello from Lisp, inside your browser!")

;; 02 / Change the words. Run it again.
(concat "An editor is " "a place to think.")

;; 03 / Little programs, immediate answers
(mapcar (lambda (n) (* n n)) '(1 2 3 4 5))
;; => (1 4 9 16 25)

;; 04 / Ask the editor about itself
(list :buffer (buffer-name)
      :mode major-mode
      :characters (buffer-size))

;; 05 / Change this buffer's appearance
(text-scale-set 1)
;; Back to the original size:
(text-scale-set 0)

;; 06 / Turn an idea into an editor command
;; Evaluate the WHOLE defun, then M-x neo-greet.
(defun neo-greet ()
  "Say hello from the playground."
  (interactive)
  (message "You just taught your editor a new command."))

;; Keep exploring:
;; C-h f  describe a function     C-g  cancel
;; C-/    undo an edit            M-x write-file  save as
;; Saving preserves the text, not live Lisp state.
;; Unsaved buffers disappear when you reload.
