;;; -*- lexical-binding: nil; -*-
;; ring_bell reads and writes the CURRENT binding of ring-bell-function:
;; a dynamic let-binding and a buffer-local value are both visible, the
;; default function is never called while either is current, and the
;; function itself runs with the variable set to nil (src/terminal.c:56-75).

(progn
  (setq m2-ring-log nil)
  (setq ring-bell-function
        (lambda () (push (list 'm2-default (null ring-bell-function)) m2-ring-log)))
  (let ((ring-bell-function
         (lambda () (push (list 'm2-let (null ring-bell-function)) m2-ring-log))))
    (let ((b (get-buffer-create "m2-ring-let")))
      (unwind-protect
          (with-current-buffer b
            (erase-buffer)
            (setq buffer-auto-save-file-name "/nonexistent-m2-ring/#let#")
            (insert "x")
            (set-buffer-modified-p t)
            (do-auto-save t t))
        (kill-buffer b))))
  (list m2-ring-log (functionp ring-bell-function)))

(progn
  (setq m2-ring-log nil)
  (setq m2-ring-local-restored nil)
  (setq ring-bell-function
        (lambda () (push (list 'm2-default (null ring-bell-function)) m2-ring-log)))
  (let ((b (get-buffer-create "m2-ring-local")))
    (unwind-protect
        (with-current-buffer b
          (erase-buffer)
          (setq buffer-auto-save-file-name "/nonexistent-m2-ring/#local#")
          (setq-local ring-bell-function
                      (lambda () (push (list 'm2-local (null ring-bell-function)) m2-ring-log)))
          (insert "x")
          (set-buffer-modified-p t)
          (do-auto-save t t))
      (setq m2-ring-local-restored
            (with-current-buffer b (functionp ring-bell-function)))
      (kill-buffer b)))
  (list m2-ring-log m2-ring-local-restored (functionp ring-bell-function)))

(progn
  (setq m2-ring-log nil)
  (setq ring-bell-function
        (lambda () (push (list 'm2-default (null ring-bell-function)) m2-ring-log)))
  (let ((b (get-buffer-create "m2-ring-default")))
    (unwind-protect
        (with-current-buffer b
          (erase-buffer)
          (setq buffer-auto-save-file-name "/nonexistent-m2-ring/#default#")
          (insert "x")
          (set-buffer-modified-p t)
          (do-auto-save t t))
      (kill-buffer b)))
  (list m2-ring-log (functionp ring-bell-function)))
