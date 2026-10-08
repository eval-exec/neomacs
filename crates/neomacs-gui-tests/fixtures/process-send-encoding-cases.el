;;; process-send-encoding-cases.el --- Real process encoding cases -*- lexical-binding: t -*-

(defvar neomacs-process-encoding-hook-process nil)
(defvar neomacs-process-encoding-in-hook nil)

(defun neomacs-process-encoding-capture (coding text region inhibit &optional chunks)
  "Capture exact bytes sent through CODING to a real pipe process."
  (let* ((output "")
         (process (make-process
                   :name "process-encoding-bytes" :command '("od" "-An" "-tx1")
                   :connection-type 'pipe :coding (cons 'binary coding)
                   :noquery t
                   :filter (lambda (_process bytes)
                             (setq output (concat output bytes)))))
         (neomacs-process-encoding-hook-process process)
         (deadline (+ (float-time) 2)))
    (unwind-protect
        (progn
          (let ((inhibit-eol-conversion inhibit))
            (if region
                (with-temp-buffer
                  (set-buffer-multibyte (multibyte-string-p text))
                  (insert "prefix" text "suffix")
                  ;; Bounds are character positions, including for Japanese.
                  (process-send-region process 7 (- (point-max) 6)))
              (dolist (part (or chunks (list text)))
                (process-send-string process part))))
          (process-send-eof process)
          (while (and (process-live-p process) (< (float-time) deadline))
            (accept-process-output process 0.05))
          (when (process-live-p process)
            (error "Timed out collecting bytes for %s" coding))
          (accept-process-output process 0.05)
          (unless (= (process-exit-status process) 0)
            (error "Byte collector failed for %s" coding))
          (mapconcat #'identity (split-string output) ""))
      (when (process-live-p process) (delete-process process)))))

(defun neomacs-process-encoding-multibyte-cases ()
  "Exercise both sending APIs with literal GNU byte expectations."
  (let (results)
    (dolist (case '((euc-jp "a4aba4f3a4b80a")
                    (shift_jis "82a982f182b60a")
                    (japanese-iso-8bit "a4aba4f3a4b80a")
                    (utf-8 "e3818be38293e381980a")))
      (dolist (region '(nil t))
        (let ((actual (neomacs-process-encoding-capture
                       (car case) "かんじ\n" region nil)))
          (push (list (symbol-name (car case))
                      (if region "region" "string")
                      actual (cadr case) (equal actual (cadr case))) results))))
    (nreverse results)))

(defun neomacs-process-encoding-unibyte-cases ()
  "Unibyte data bypasses charset conversion while retaining EOL policy."
  (let (results)
    (dolist (region '(nil t))
      (dolist (inhibit '(nil t))
        (let* ((expected (if inhibit "a4ab0a410d0a" "a4ab0d0a410d0d0a"))
               (actual (neomacs-process-encoding-capture
                        'shift_jis-dos (unibyte-string #xa4 #xab 10 65 13 10)
                        region inhibit)))
          (push (list "shift_jis-dos" (if region "region" "string")
                      actual expected (equal actual expected)) results))))
    (nreverse results)))

(defun neomacs-process-encoding-stateful-cases ()
  "ISO-2022 persists state and restores its encoder after raw-byte sends."
  (let ((expected "1b2442242b24732438") results)
    (dolist (mode '(string chunks region))
      (let ((actual (neomacs-process-encoding-capture
                     'iso-2022-jp "かんじ" (eq mode 'region) nil
                     (and (eq mode 'chunks) '("か" "ん" "じ")))))
        (push (list "iso-2022-jp" (symbol-name mode)
                    actual expected (equal actual expected)) results)))
    (dolist (case
             (list
              (list "raw-highbytes" (unibyte-string #x80 #xa4)
                    "1b2442242b80a41b24422473")
              (list "raw-ascii" (unibyte-string 65)
                    "1b2442242b411b24422473")
              (list "raw-empty" (unibyte-string)
                    "1b2442242b1b24422473")
              (list "multibyte-ascii" (string-to-multibyte "A")
                    "1b2442242b1b2842411b24422473")
              (list "multibyte-empty" (string-to-multibyte "")
                    "1b2442242b2473")))
      (let* ((actual (neomacs-process-encoding-capture
                      'iso-2022-jp "" nil nil (list "か" (nth 1 case) "ん")))
             (expected (nth 2 case)))
        (push (list "iso-2022-jp" (car case)
                    actual expected (equal actual expected)) results)))
    (nreverse results)))

(defun neomacs-process-encoding-append-z (_from to)
  (goto-char to)
  (insert "Z"))

(defun neomacs-process-encoding-nested-send (_from _to)
  (unless neomacs-process-encoding-in-hook
    (let ((neomacs-process-encoding-in-hook t))
      (process-send-string neomacs-process-encoding-hook-process "ん"))))

(defun neomacs-process-encoding-hook-cases ()
  "Pre-write hooks transform legacy text and may reenter the same process."
  (define-coding-system 'neomacs-process-hook-sjis "Shift-JIS pre-write test"
    :coding-type 'shift-jis :mnemonic ?S
    :charset-list '(ascii katakana-jisx0201 japanese-jisx0208)
    :pre-write-conversion 'neomacs-process-encoding-append-z)
  (define-coding-system 'neomacs-process-hook-iso "ISO-2022 nested-send test"
    :coding-type 'iso-2022 :mnemonic ?J
    :designation [(ascii japanese-jisx0208-1978 japanese-jisx0208 latin-jisx0201) nil nil nil]
    :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation)
    :charset-list '(ascii japanese-jisx0208 japanese-jisx0208-1978 latin-jisx0201)
    :pre-write-conversion 'neomacs-process-encoding-nested-send)
  (let (results)
    (dolist (region '(nil t))
      (let* ((expected "82a95a")
             (actual (neomacs-process-encoding-capture
                      'neomacs-process-hook-sjis "か" region nil)))
        (push (list "hook-shift-jis" (if region "region" "string")
                    actual expected (equal actual expected)) results)))
    (let* ((expected "1b24422473242b24732438")
           (actual (neomacs-process-encoding-capture
                    'neomacs-process-hook-iso "" nil nil '("か" "じ"))))
      (push (list "hook-iso-2022" "nested-sends"
                  actual expected (equal actual expected)) results))
    (nreverse results)))
