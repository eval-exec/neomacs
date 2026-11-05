;;; -*- lexical-binding: nil; -*-

(progn
  (defvar vm-ifc-log nil)
  (defvar vm-ifc-calls 0)
  (defvar vm-ifc-result 5)
  (defun vm-ifc-handler (operation &rest args)
    (if (eq operation 'insert-file-contents)
        (progn
          (setq vm-ifc-calls (1+ vm-ifc-calls))
          (save-excursion (insert "h\u00e9llo"))
          (setq last-coding-system-used 'utf-8-unix)
          (and vm-ifc-result (list (car args) vm-ifc-result)))
      (let ((inhibit-file-name-handlers
             (cons 'vm-ifc-handler inhibit-file-name-handlers))
            (inhibit-file-name-operation operation))
        (apply operation args))))
  (defmacro vm-ifc-with (&rest body)
    `(let ((file-name-handler-alist '(("\\`/vm-ifc:" . vm-ifc-handler)))
           (after-insert-file-functions
            (list (lambda (n)
                    (push (list 'after-insert n (point) (buffer-modified-p))
                          vm-ifc-log)
                    nil))))
       (setq vm-ifc-log nil vm-ifc-calls 0)
       ,@body))
  nil)
(vm-ifc-with
 (with-temp-buffer
   (setq buffer-file-coding-system 'undecided)
   (list (insert-file-contents "/vm-ifc:x")
         buffer-file-coding-system vm-ifc-log (buffer-string))))
(vm-ifc-with
 (let ((buf (generate-new-buffer "vm-ifc-visit")))
   (unwind-protect
       (with-current-buffer buf
         (setq buffer-file-coding-system 'undecided)
         (list (insert-file-contents "/vm-ifc:x" t)
               buffer-file-coding-system vm-ifc-log
               (buffer-modified-p) buffer-undo-list buffer-saved-size))
     (kill-buffer buf))))
(let ((vm-ifc-result 99))
  (vm-ifc-with
   (with-temp-buffer
     (setq buffer-file-coding-system 'undecided)
     (list (insert-file-contents "/vm-ifc:x")
           buffer-file-coding-system vm-ifc-log))))
(let ((vm-ifc-result nil))
  (vm-ifc-with
   (with-temp-buffer
     (list (insert-file-contents "/vm-ifc:x") vm-ifc-log))))
(vm-ifc-with
 (with-temp-buffer
   (setq buffer-read-only t)
   (list (condition-case err (insert-file-contents "/vm-ifc:x")
           (buffer-read-only (car err)))
         vm-ifc-calls)))
(vm-ifc-with
 (with-temp-buffer
   (let ((indirect (make-indirect-buffer (current-buffer) "vm-ifc-indirect")))
     (unwind-protect
         (with-current-buffer indirect
           (list (condition-case err (insert-file-contents "/vm-ifc:x" t)
                   (error (error-message-string err)))
                 vm-ifc-calls))
       (kill-buffer indirect)))))
(mapcar
 (lambda (count)
   (let ((vm-ifc-result count))
     (vm-ifc-with
      (let ((original (symbol-function 'format-decode)))
        (unwind-protect
            (progn
              (fset 'format-decode
                    (lambda (_ n visit)
                      (push (list 'format-decode n (point) visit) vm-ifc-log)
                      n))
              (with-temp-buffer
                (list (insert-file-contents "/vm-ifc:x") (reverse vm-ifc-log))))
          (fset 'format-decode original))))))
 '(5 99))
