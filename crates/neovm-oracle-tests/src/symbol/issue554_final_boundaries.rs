//! GNU SYMBOLP/EQ policy at the final public builtin boundaries.
use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

fn check(form: &str, expected: &str) {
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq(expected, &gnu, &neomacs);
}

#[test]
fn issue554_sort_keywords_and_fast_designators_follow_eq_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // fns.c:2486-2496 EQ(option,key/lessp/in-place/reverse).
    check(
        r#"(let ((key (position-symbol :key 0))
                   (identity (position-symbol 'identity 1))
                   (lessp (position-symbol :lessp 2))
                   (value< (position-symbol 'value< 3)))
                 (mapcar (lambda (enabled)
                   (let ((symbols-with-pos-enabled enabled))
                     (condition-case err
                       (sort [3 1 2] key identity lessp value<
                             (position-symbol :reverse 4) nil
                             (position-symbol :in-place 5) nil)
                       (error (car err))))) '(nil t)))"#,
        "(error [1 2 3])",
    );
}

#[test]
fn issue554_fringe_alignment_and_frame_symbol_checks_follow_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // fringe.c:1613-1617 EQ; dispnew.c:7069 CHECK_SYMBOL.
    check(
        r#"(let ((top (position-symbol 'top 0))
                   (variable (position-symbol 'issue554-void-state 1)))
                 (mapcar (lambda (enabled)
                   (let ((symbols-with-pos-enabled enabled))
                     (list (condition-case err
                             (progn (define-fringe-bitmap 'issue554-fringe [0] 1 8 (list top t)) t)
                             (error (car err)))
                           (condition-case err (frame-or-buffer-changed-p variable)
                             (error (car err)))))) '(nil t)))"#,
        "((error wrong-type-argument) (t void-variable))",
    );
}

#[test]
fn issue554_image_library_type_follows_eq_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // image.c:12911,13006 EQ(type,builtin type).
    check(
        r#"(let ((type (position-symbol 'png 0)))
                 (list (let ((symbols-with-pos-enabled nil)) (init-image-library type))
                       (let ((symbols-with-pos-enabled t))
                         (eq (init-image-library type) (init-image-library 'png)))))"#,
        "(nil t)",
    );
}

#[test]
fn issue554_completion_flags_follow_eq_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // minibuf.c:2159-2185 EQ(flag,t/lambda/metadata).
    check(
        r#"(let ((buffer (get-buffer-create "issue554-complete-target")))
                 (unwind-protect
                   (let ((symbols-with-pos-enabled t))
                     (list (internal-complete-buffer "issue554-complete-target" nil
                                                     (position-symbol 'lambda 0))
                           (eq (car (internal-complete-buffer "" nil
                                                             (position-symbol 'metadata 1))) 'metadata)
                           (equal (internal-complete-buffer "issue554-complete-target" nil
                                                            (position-symbol t 2))
                                  '("issue554-complete-target"))))
                   (kill-buffer buffer)))"#,
        "(t t t)",
    );
}

#[test]
#[cfg(target_os = "linux")]
fn issue554_inotify_aspect_symbols_follow_eq_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // inotify.c:117-159 EQ on each aspect,168-180 list/single dispatch.
    check(
        r#"(let ((file (make-temp-file "issue554-inotify-"))
                   (aspect (position-symbol 'modify 0)))
                 (unwind-protect
                   (mapcar (lambda (enabled)
                     (let ((symbols-with-pos-enabled enabled))
                       (condition-case err
                         (let ((watch (inotify-add-watch file (list aspect) #'ignore)))
                           (inotify-rm-watch watch) t)
                         (error (car err))))) '(nil t))
                   (delete-file file)))"#,
        "(file-notify-error t)",
    );
}

#[test]
fn issue554_interpreted_closure_formals_follow_live_symbol_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // eval.c:3553-3574 lambda_arity SYMBOLP/EQ;3399-3405 funcall_lambda.
    check(
        r#"(let* ((symbols-with-pos-enabled t)
                   (fn (make-interpreted-closure
                        (list (position-symbol 'arg 0)) '(arg) nil)))
              (list (func-arity fn)
                    (condition-case err
                        (let ((symbols-with-pos-enabled nil)) (func-arity fn))
                      (error (car err)))
                    (condition-case err
                        (let ((symbols-with-pos-enabled nil)) (funcall fn 7))
                      (error (car err)))))"#,
        "((1 . 1) invalid-function invalid-function)",
    );
}

#[test]
fn issue554_dynamic_formals_update_symbol_policy_after_each_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // A dynamic specbind of the flag affects the next formal's projection.
    check(
        r#"(let* ((symbols-with-pos-enabled t)
                   (fn (list 'lambda
                             (list 'symbols-with-pos-enabled (position-symbol 'n 0))
                             'n)))
              (list (condition-case err (funcall fn nil 7) (error (car err)))
                    (condition-case err (funcall fn t 7) (error (car err)))))"#,
        "(invalid-function 7)",
    );
}

#[test]
fn issue554_closure_equal_and_hash_keys_follow_symbol_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(let* ((arg (position-symbol 'n 0))
                   (a (make-interpreted-closure (list arg) '(n) nil))
                   (b (make-interpreted-closure '(n) '(n) nil)))
              (mapcar (lambda (enabled)
                        (let ((symbols-with-pos-enabled enabled)
                              (table (make-hash-table :test 'equal)))
                          (puthash a 1 table)
                          (puthash b 2 table)
                          (list (equal a b) (hash-table-count table)
                                (gethash a table) (gethash b table)))) '(nil t)))"#,
        "((nil 2 1 2) (t 1 2 2))",
    );
}

#[test]
fn issue554_watcher_membership_follow_symbol_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(let* ((a (make-interpreted-closure (list (position-symbol 'n 0)) '(n) nil))
                   (b (make-interpreted-closure '(n) '(n) nil)))
              (mapcar (lambda (enabled)
                        (let ((symbols-with-pos-enabled enabled)
                              (variable (make-symbol "issue554-watcher")))
                          (add-variable-watcher variable a)
                          (add-variable-watcher variable b)
                          (let ((added (length (get-variable-watchers variable))))
                            (remove-variable-watcher variable b)
                            (list added (length (get-variable-watchers variable)))))) '(nil t)))"#,
        "((2 1) (1 0))",
    );
}

#[test]
fn issue554_formal_watchers_change_policy_before_the_next_formal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Both nil->t and t->nil changes happen in a previous binder's watcher.
    check(
        r#"(mapcar
 (lambda (initial)
   (let* ((symbols-with-pos-enabled initial)
          (variable (make-symbol "issue554-watched-formal"))
          (target (not initial))
          (watcher (list 'lambda '(_symbol _newval operation _where)
                         (list 'if '(eq operation 'let)
                               (list 'setq 'symbols-with-pos-enabled target))))
          (function (list 'lambda
                          (list variable (position-symbol 'n 0)) 'n)))
     (unwind-protect
         (progn
           (add-variable-watcher variable watcher)
           (condition-case err (funcall function 1 7)
             (error (car err))))
       (remove-variable-watcher variable watcher))))
 '(nil t))"#,
        "(7 invalid-function)",
    );
}
