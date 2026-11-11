//! GNU `internal-set-lisp-face-attribute` refuses an `:inherit' value that
//! would make a face inherit from itself (src/xfaces.c, the QCinherit arm and
//! `face_inheritance_cycle').

/// The cases are evaluated in order, so each one sees the faces left by the
/// earlier ones.
#[test]
fn set_face_attribute_rejects_inheritance_cycles_like_gnu() {
    let form = r#"(progn
      (dolist (face '(cyc-self cyc-a cyc-b cyc-x cyc-y cyc-z cyc-t1 cyc-t2 cyc-ok cyc-list cyc-circ))
        (make-face face))
      (let ((try (lambda (face frame value)
                   (condition-case err
                       (progn (set-face-attribute face frame :inherit value)
                              (list 'ok (face-attribute face :inherit frame)))
                     (error err)))))
        (list
         ;; A face cannot inherit from itself.
         (funcall try 'cyc-self nil 'cyc-self)
         ;; GNU's xfaces-test-circular-inheritance (bug#79672).
         (funcall try 'cyc-a nil 'cyc-b)
         (funcall try 'cyc-b nil 'cyc-a)
         ;; The refused value is not stored.
         (face-attribute 'cyc-b :inherit)
         ;; Indirect cycles, through a symbol and through a list.
         (funcall try 'cyc-x nil 'cyc-y)
         (funcall try 'cyc-y nil 'cyc-z)
         (funcall try 'cyc-z nil 'cyc-x)
         (funcall try 'cyc-z nil '(bold cyc-x))
         ;; Inheriting from faces outside the chain is fine.
         (funcall try 'cyc-z nil '(bold italic))
         ;; The new-frame defaults are checked on their own.
         (funcall try 'cyc-t1 t 'cyc-t2)
         (funcall try 'cyc-t2 t 'cyc-t1)
         ;; Resetting the attribute is always allowed.
         (funcall try 'cyc-a nil 'unspecified)
         (funcall try 'cyc-b nil 'cyc-a)
         (funcall try 'cyc-ok nil nil)
         ;; GNU compares the parents of the list members with the face, not
         ;; the members themselves, so this is accepted.
         (funcall try 'cyc-list nil '(cyc-list))
;; A long list of repeated parents is checked quickly.
         (progn (set-face-attribute 'cyc-ok nil :inherit (make-list 2000 'bold))
                (length (face-attribute 'cyc-ok :inherit)))
         ;; A circular list does not hang the check.
         (let ((parents (list 'bold)))
           (setcdr parents parents)
           (condition-case err
               (progn (set-face-attribute 'cyc-circ t :inherit parents) 'ok)
             (error err))))))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        concat!(
            r#"OK ((error "Face inheritance results in inheritance cycle" cyc-self)"#,
            r#" (ok cyc-b)"#,
            r#" (error "Face inheritance results in inheritance cycle" cyc-a)"#,
            r#" unspecified"#,
            r#" (ok cyc-y)"#,
            r#" (ok cyc-z)"#,
            r#" (error "Face inheritance results in inheritance cycle" cyc-x)"#,
            r#" (error "Face inheritance results in inheritance cycle" (bold cyc-x))"#,
            r#" (ok (bold italic))"#,
            r#" (ok cyc-t2)"#,
            r#" (error "Face inheritance results in inheritance cycle" cyc-t1)"#,
            r#" (ok unspecified)"#,
            r#" (ok cyc-a)"#,
            r#" (ok nil)"#,
            r#" (ok (cyc-list))"#,
            r#" 2000"#,
            r#" ok)"#,
        )
    );
}
