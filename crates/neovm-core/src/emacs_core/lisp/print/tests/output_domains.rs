//! Domain metadata fixtures refreshed from GNU 31.1, never handwritten.

#[test]
fn printer_output_domains_gnu() {
    super::oracle(
        "output_domains",
        r###"(let ((print-circle nil) (print-gensym nil)
      (print-escape-nonascii nil) (print-escape-multibyte nil)
      (print-unreadable-function nil) (print-level nil) (print-length nil)
      (snapshot
       (lambda (value)
         (let ((properties nil))
           (dotimes (i (length value))
             (push (list i (text-properties-at i value)) properties))
           (list (multibyte-string-p value) (string-bytes value)
                 (append value nil) (nreverse properties))))))
  (list
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-False-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-False-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-False-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-False-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-True-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-True-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-True-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 97)))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-unibyte-True-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-False-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-False-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-False-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-False-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-True-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-True-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-True-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 97 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-ascii-multibyte-True-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-False-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-False-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-False-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-False-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-True-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-True-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-True-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #xe9 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-unicode-multibyte-True-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-False-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-False-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-False-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-False-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-True-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-True-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-True-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (string-as-unibyte (make-string 2 (unibyte-char-to-multibyte 233) t)))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-unibyte-True-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-False-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-False-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-False-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-False-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-True-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-True-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-True-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 (unibyte-char-to-multibyte 233) t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-raw-multibyte-True-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-False-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-False-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-False-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))

                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-False-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-True-s-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%s" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-True-s-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-True-S-fixnum before
                               (funcall snapshot (format control 12345))
                               (funcall snapshot control))))
(let* ((prefix (make-string 2 #x200000 t))
                            (control (concat prefix "%S" (substring prefix 0 1))))
                       (put-text-property 0 (length control) 'face 'bold control)
                       (let ((before (funcall snapshot control)))
                         (list 'control-nonunicode-multibyte-True-S-list before
                               (funcall snapshot (format control '(12345)))
                               (funcall snapshot control))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-False-format-s (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (format "%s" symbol))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-False-format-S (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (format "%S" symbol))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-False-princ (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (with-temp-buffer (set-buffer-multibyte t) (princ symbol (current-buffer)) (buffer-string)))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-False-prin1-to-string (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (prin1-to-string symbol))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-True-format-s (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (format "%s" (list symbol)))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-True-format-S (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (format "%S" (list symbol)))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-True-princ (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (with-temp-buffer (set-buffer-multibyte t) (princ (list symbol) (current-buffer)) (buffer-string)))
                              (funcall snapshot (symbol-name symbol))))
(let ((symbol (make-symbol (make-string 2 #x200000 t))))
                        (list 'symbol-nonunicode-True-prin1-to-string (funcall snapshot (symbol-name symbol))
                              (funcall snapshot (prin1-to-string (list symbol)))
                              (funcall snapshot (symbol-name symbol))))))"###,
        include_str!("settings_gnu/output_domains.expect"),
    );
}
