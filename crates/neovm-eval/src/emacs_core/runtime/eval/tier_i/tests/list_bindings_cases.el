;; GNU eval.c:1091,1130 and lisp.h:5886-5888 advance let* after callbacks.
;; GNU eval.c:1151,1158 prevalidate let and bound its initializer traversal.
;; Each interpreted function is called twice with an intact spine, then its
;; original let form is mutated. Thus tier-I executes the already lowered let.
(eval
 '(progn
    (defvar gdn-ti-vars nil)
    (defvar gdn-ti-phase nil)
    (defvar gdn-ti-mode nil)
    (defvar gdn-ti-log nil)
    (defvar gdn-ti-y 200)
    (defvar gdn-ti-z 300)
    (defun gdn-ti-mutate (cursor)
      (cond
       ((eq gdn-ti-mode 'terminate) (setcdr cursor nil))
       ((eq gdn-ti-mode 'dotted) (setcdr cursor 7))
       ((eq gdn-ti-mode 'cycle) (setcdr cursor cursor))
       ((eq gdn-ti-mode 'detach)
        (setcdr (if (eq gdn-ti-phase 'let*) (cdr gdn-ti-vars) gdn-ti-vars) nil))
       ((eq gdn-ti-mode 'error) (setcdr cursor nil))))
    (defun gdn-ti-first (x y)
      (push (list 'first x y) gdn-ti-log)
      (if (eq gdn-ti-phase 'let)
          (gdn-ti-mutate (cdr gdn-ti-vars)))
      (garbage-collect)
      11)
    (defun gdn-ti-second ()
      (push 'second gdn-ti-log)
      (garbage-collect)
      22)
    (defun gdn-ti-third (x y)
      (push (list 'third x y) gdn-ti-log)
      (if (eq gdn-ti-phase 'let*)
          (gdn-ti-mutate (nthcdr 2 gdn-ti-vars)))
      (garbage-collect)
      (if (eq gdn-ti-mode 'error) (error "tier-I callback failure"))
      33)
    (mapcar
     (lambda (lexical)
       (mapcar
        (lambda (phase)
          (mapcar
           (lambda (mode)
             (set 'gdn-ti-local 100)
             (setq gdn-ti-y 200 gdn-ti-z 300
                   gdn-ti-phase phase gdn-ti-mode 'retain gdn-ti-log nil
                   gdn-ti-vars
                   (list (list 'gdn-ti-local '(gdn-ti-first gdn-ti-local gdn-ti-y))
                         (list 'gdn-ti-y '(gdn-ti-second))
                         (list 'gdn-ti-z '(gdn-ti-third gdn-ti-local gdn-ti-y))))
             (defalias 'gdn-ti-hot
               (eval (list 'function
                           (list 'lambda nil
                                 (list phase gdn-ti-vars '(list gdn-ti-local))))
                     lexical))
             (gdn-ti-hot)
             (gdn-ti-hot)
             (setq gdn-ti-log nil gdn-ti-mode mode)
             (cond
              ((eq mode 'initial-cycle) (setcdr (nthcdr 2 gdn-ti-vars) gdn-ti-vars))
              ((eq mode 'initial-dotted) (setcdr (nthcdr 2 gdn-ti-vars) 7)))
             (let ((result
                    (condition-case e (gdn-ti-hot)
                      (error
                       ;; Preserve the payload's identity without printing
                       ;; a potentially circular variable list.
                       (if (eq (car e) 'wrong-type-argument)
                           (list (car e) (nth 1 e) (eq (nth 2 e) gdn-ti-vars))
                         (car e))))))
               ;; Discard the malformed function before probing restored state.
               (fmakunbound 'gdn-ti-hot)
               (setq gdn-ti-vars nil)
               (garbage-collect)
               (list lexical phase mode result
                     (list (symbol-value 'gdn-ti-local) gdn-ti-y gdn-ti-z)
                     (nreverse gdn-ti-log))))
           '(retain terminate dotted cycle detach error initial-cycle initial-dotted)))
        '(let let*)))
     '(nil t)))
 t)
