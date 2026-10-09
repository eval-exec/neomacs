;; GNU eval.c:1091,1130 and lisp.h:5886-5888 step let* tails after callbacks.
;; GNU eval.c:1149,1158-1177 bounds let initializers by the validated length.
(eval
 '(progn
    (defvar gdn-callback-dynamic 200)
    (mapcar
     (lambda (lexical)
       (mapcar
        (lambda (case)
          (let ((phase (car case)) (mode (cadr case)) vars log)
            (set 'gdn-callback-lexical 100)
            (setq gdn-callback-dynamic 200)
            (set 'gdn-callback-value 400)
            (set 'gdn-callback-skipped 300)
            (let* ((callback
                    (lambda (x d)
                      (push (list 'before x d) log)
                      (let ((cursor (if (eq phase 'let*)
                                        (nthcdr 2 vars)
                                      (cdr vars))))
                        (cond
                         ((eq mode 'terminate) (setcdr cursor nil))
                         ((eq mode 'dotted) (setcdr cursor 7))
                         ((eq mode 'cycle) (setcdr cursor cursor))
                         ((eq mode 'error) (setcdr cursor nil))))))
                   (after
                    (lambda (x d) (push (list 'after x d) log)))
                   (second
                    (lambda (x d)
                      (push (list 'second x d) log)
                      (garbage-collect)
                      22))
                   (skipped
                    (lambda () (push 'skipped log) 99))
                   (callback-form
                    (list 'funcall (list 'quote callback)
                          'gdn-callback-lexical 'gdn-callback-dynamic))
                   (star-init
                    (list 'progn callback-form
                          '(setq gdn-callback-lexical 33
                                 gdn-callback-dynamic 44)
                          (list 'funcall (list 'quote after)
                                'gdn-callback-lexical 'gdn-callback-dynamic)
                          '(garbage-collect)
                          (list 'if (list 'quote (eq mode 'error))
                                '(error "callback failure") 55)))
                   (parallel-init
                    (list 'progn callback-form '(garbage-collect) 11))
                   (second-init
                    (list 'funcall (list 'quote second)
                          'gdn-callback-lexical 'gdn-callback-dynamic))
                   (skipped-init (list 'funcall (list 'quote skipped))))
              (setq vars
                    (if (eq phase 'let*)
                        (list (list 'gdn-callback-lexical 11)
                              (list 'gdn-callback-dynamic 22)
                              (list 'gdn-callback-value star-init)
                              (list 'gdn-callback-skipped skipped-init))
                      (list (list 'gdn-callback-lexical parallel-init)
                            (list 'gdn-callback-dynamic second-init)
                            (list 'gdn-callback-skipped skipped-init))))
              (let ((result
                     (condition-case e
                         (eval
                          (list phase vars
                                '(list gdn-callback-lexical gdn-callback-dynamic
                                       gdn-callback-value gdn-callback-skipped))
                          lexical)
                       (error
                        (if (eq (car e) 'circular-list)
                            (list (car e) (car (car (cadr e))))
                          (list (car e) (cadr e)))))))
                (garbage-collect)
                (list phase mode lexical result
                      (list (symbol-value 'gdn-callback-lexical)
                            gdn-callback-dynamic
                            (symbol-value 'gdn-callback-value)
                            (symbol-value 'gdn-callback-skipped))
                      (nreverse log))))))
        '((let* retain) (let* terminate) (let* cycle) (let* error)
          (let terminate) (let dotted) (let cycle))))
     '(nil t)))
 t)
