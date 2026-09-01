;;; Ledger 217 -- the echo-area buffer's multibyteness.
;;;
;;; GNU `set_message_1' (src/xdisp.c:13588-13601) decides it from ONE variable:
;;;   if (!message_enable_multibyte && unibyte_display_via_language_environment
;;;       && !NILP (BVAR (current_buffer, enable_multibyte_characters)))
;;;     Fset_buffer_multibyte (Qnil);
;;;   else if (NILP (BVAR (current_buffer, enable_multibyte_characters)))
;;;     Fset_buffer_multibyte (Qt);
;;; `message_enable_multibyte' is `STRING_MULTIBYTE (string)' (src/xdisp.c:13568).
;;; So with `unibyte-display-via-language-environment' at its nil default the
;;; echo buffer is ALWAYS multibyte, whatever the message string is; only that
;;; variable can make it unibyte.  This probe reads the flag, the text that
;;; landed, and `point-max' -- the three things a unibyte-vs-multibyte insert of
;;; the same bytes separates.
(defvar l217-out (or (getenv "L217_OUT") "./tmp/l217/echo-multibyte.txt"))
(defvar l217-lines nil)
(defun l217-p (fmt &rest args) (push (apply #'format fmt args) l217-lines))
(defun l217-snap (tag)
  (let ((b (get-buffer " *Echo Area 0*")))
    (l217-p "%-34s multibyte=%S point-max=%S string=%S" tag
            (and b (buffer-local-value 'enable-multibyte-characters b))
            (and b (with-current-buffer b (point-max)))
            (and b (with-current-buffer b (buffer-string))))
    (l217-p "%-34s current-message=%S multibyte-msg=%S" tag
            (current-message)
            (and (current-message) (multibyte-string-p (current-message))))))
(defun l217-run ()
  (delete-other-windows)
  (redisplay t)
  (message "%s" "l217 pure ascii multibyte string")
  (redisplay t)
  (l217-snap "1-ascii-multibyte-message")
  ;; A UNIBYTE message carrying raw bytes >= 128.
  (message "%s" (string-to-unibyte (string 65 66 67)))
  (redisplay t)
  (l217-snap "2-unibyte-ascii-message")
  (message "%s" (unibyte-string 65 200 201 66))
  (redisplay t)
  (l217-snap "3-unibyte-highbytes")
  ;; A multibyte message with non-ASCII characters, right after the unibyte one.
  (message "%s" "l217 café 中文")
  (redisplay t)
  (l217-snap "4-multibyte-nonascii")
  ;; Now with the ONE variable GNU consults set to t.
  (setq unibyte-display-via-language-environment t)
  (message "%s" (unibyte-string 65 200 201 66))
  (redisplay t)
  (l217-snap "5-udvle-t-unibyte-highbytes")
  (message "%s" "l217 café again")
  (redisplay t)
  (l217-snap "6-udvle-t-multibyte")
  (setq unibyte-display-via-language-environment nil)
  (message "%s" (unibyte-string 65 200 201 66))
  (redisplay t)
  (l217-snap "7-udvle-nil-again")
  (make-directory (file-name-directory (expand-file-name l217-out)) t)
  (with-temp-file l217-out (insert (mapconcat #'identity (nreverse l217-lines) "\n") "\n")))
(l217-run)
(kill-emacs)
