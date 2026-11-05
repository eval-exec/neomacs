(progn
  (require 'cl-lib)
  (require 'bytecomp)
  (cl-labels
      ((metadata (symbol)
         (let ((name (symbol-name symbol)))
           ;; Supplied-count text is never decoded or indexed for this proof.
           (list (length name) (string-bytes name)
                 (multibyte-string-p name)
                 (append (string-as-unibyte name) nil))))
       (observe (function query table)
         (condition-case err
             (let ((value (funcall function query table)))
               (list 'ok (symbol-with-pos-p value)
                     (and (symbol-with-pos-p value) (symbol-with-pos-pos value))
                     ;; Disable wrapper projection only for physical identity.
                     (let ((symbols-with-pos-enabled nil)) (eq value query))
                     (and value (metadata value))))
           (error
            (list 'error (car err) (cadr err)
                  ;; GNU signals with the original NAME or invalid OBARRAY,
                  ;; never with a projected/reconstructed operand. Physical
                  ;; identity remains observable while projection is disabled.
                  (let ((symbols-with-pos-enabled nil))
                    (eq (caddr err) query))
                  (let ((symbols-with-pos-enabled nil))
                    (eq (caddr err) table)))))))
    (let* ((interpreted (lambda (query &optional table) (intern-soft query table)))
           (compiled (byte-compile
                      (lambda (query &optional table) (intern-soft query table))))
           (functions (list interpreted compiled)))
      (list
       ;; Reuse each callable across live policy changes, including nil/t.
       (mapcar
        (lambda (enabled)
          (let ((symbols-with-pos-enabled enabled)
                (read-symbol-shorthands '(("nil" . "car") ("t" . "cdr")
                                          ("car" . "cdr"))))
            (mapcar
             (lambda (function)
               (let ((table (obarray-make 2)))
                 (intern "gdj-positioned-cross-custom" table)
                 (let* ((symbol (intern-soft "gdj-positioned-cross-custom" table))
                        (wrapper (position-symbol symbol 37))
                        (other (position-symbol
                                (make-symbol (symbol-name symbol)) 38)))
                   (list (observe function (position-symbol nil 11) nil)
                         (observe function (position-symbol t 12) nil)
                         (observe function (position-symbol 'car 13) nil)
                         (observe function wrapper table)
                         (observe function other table)
                         ;; Invalid obarray takes precedence over NAME policy.
                         (observe function wrapper 'gdj-not-an-obarray)))))
             functions)))
        '(nil t nil t))
       ;; Ordinary global fallback and an active rewrite both retain the first
       ;; name object's exact encoding/properties, without a new adoption.
       (mapcar
        (lambda (properties)
          (let* ((name (string-to-multibyte
                        (if properties "gdj-positioned-cross-global-property"
                          "gdj-positioned-cross-global-plain")))
                 (read-symbol-shorthands nil))
            (when properties (put-text-property 0 (length name) 'gdj-marker 'source name))
            (let* ((symbol (intern name)) (original-name (symbol-name symbol))
                   (wrapper (position-symbol symbol 41)))
              (let ((symbols-with-pos-enabled t)
                    (read-symbol-shorthands
                     (list (cons "gdj-cross-short-" name))))
                (list (metadata symbol) (eq name original-name)
                      (text-properties-at 0 name)
                      (mapcar
                       (lambda (function)
                         (list (observe function name nil)
                               (observe function "gdj-cross-short-" nil)
                               (observe function wrapper nil)
                               (eq original-name (symbol-name symbol))))
                       functions))))))
        '(nil t))
       ;; 1 - 2 + 1 = 0 declared characters, with one initialized ASCII byte.
       ;; The fresh custom obarray isolates this key from computed "a" names.
       (mapcar
        (lambda (properties)
          (let* ((query (string #xe9)) (table (obarray-make 2))
                 (read-symbol-shorthands
                  (list (cons (unibyte-string 195 169) (unibyte-string 97)))))
            (when properties (put-text-property 0 (length query) 'gdj-marker 'source query))
            (let* ((symbol (intern query table)) (name (symbol-name symbol))
                   (wrapper (position-symbol symbol 51))
                   (other (position-symbol (make-symbol name) 52)))
              (list (metadata symbol)
                    (list (length query) (string-bytes query)
                          (text-properties-at 0 query))
                    (mapcar
                     (lambda (enabled)
                       (let ((symbols-with-pos-enabled enabled))
                         (mapcar
                          (lambda (function)
                            (list (observe function query table)
                                  (observe function name table)
                                  (observe function wrapper table)
                                  (observe function other table)
                                  (observe function "gdj-positioned-cross-absent" table)
                                  (eq name (symbol-name symbol))))
                          functions)))
                     '(nil t nil t))))))
        '(nil t))))))
