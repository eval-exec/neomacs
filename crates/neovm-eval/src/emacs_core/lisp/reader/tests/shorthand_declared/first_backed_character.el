(progn
  (require 'bytecomp)
  (defun gdj-backed-storage (s)
    (list (length s) (string-bytes s) (multibyte-string-p s)
          (append (string-as-unibyte s) nil)))
  (defun gdj-backed-set-on-copy (setter source index replacement)
    (let ((copy (copy-sequence source)))
      (list
       (condition-case caught
           (list 'ok (funcall setter copy index replacement))
         (error (list 'error (car caught) (error-message-string caught))))
       (gdj-backed-storage copy))))
  (defun gdj-backed-set-paths (source index replacement compiled-setter)
    (let ((direct (lambda (s i c) (aset s i c)))
          (iteration 0) last)
      (list
       (gdj-backed-set-on-copy direct source index replacement)
       (gdj-backed-set-on-copy compiled-setter source index replacement)
       (progn
         (while (< iteration 100)
           ;; Every attempt owns a fresh copy, including successful controls.
           (setq last (gdj-backed-set-on-copy
                       compiled-setter source index replacement)
                 iteration (1+ iteration)))
         (list iteration last)))))
  (let* ((short "gdj-backed-head-") (long "gdj-backed-expanded-")
         (read-symbol-shorthands (list (cons short long)))
         (input (concat (string-as-unibyte short)
                        (unibyte-string #xe9 #xe9 #xe9)))
         (name (symbol-name (intern input (obarray-make))))
         (index (length long))
         (compiled-getter (byte-compile '(lambda (s i) (aref s i))))
         (compiled-setter (byte-compile '(lambda (s i c) (aset s i c))))
         (ascii-mb (string-to-multibyte "A"))
         (high-ub (unibyte-string #xe9))
         (iteration 0) warmed-value)
    (list
     (gdj-backed-storage name)
     ;; This is the FIRST high character only. Its entire three-byte E9 E9 E9
     ;; head is initialized. No trailing index is read or iterated.
     (aref name index)
     (funcall compiled-getter name index)
     (progn
       (while (< iteration 100)
         (setq warmed-value (funcall compiled-getter name index)
               iteration (1+ iteration)))
       (list iteration warmed-value))
     ;; Both replacement-domain and existing-byte restrictions are covered.
     (gdj-backed-set-paths name index ?A compiled-setter)
     (gdj-backed-set-paths name index #xe9 compiled-setter)
     (list 'ascii-multibyte (gdj-backed-storage ascii-mb)
           (gdj-backed-set-paths ascii-mb 0 ?B compiled-setter)
           (gdj-backed-set-paths ascii-mb 0 #xe9 compiled-setter))
     (list 'high-unibyte (gdj-backed-storage high-ub)
           (gdj-backed-set-paths high-ub 0 ?B compiled-setter)
           (gdj-backed-set-paths high-ub 0 #xe9 compiled-setter))
     ;; The interned source was never passed to aset.
     (gdj-backed-storage name))))
