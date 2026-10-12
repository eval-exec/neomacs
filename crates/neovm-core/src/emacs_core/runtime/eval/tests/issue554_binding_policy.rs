//! GNU binding checks observe policy after Lisp initializers, not at entry.
use crate::emacs_core::eval::{Context, TierIMode};
use crate::emacs_core::print::print_value;

fn check(source: &str, expected: &str) {
    for mode in [TierIMode::Off, TierIMode::On, TierIMode::Verify] {
        let mut context = Context::new();
        context.tier_i.set_mode(mode);
        context.tier_i.set_threshold(1);
        let result = context.eval_str(source).expect("GNU binding policy form");
        assert_eq!(print_value(&result), expected, "tier {mode:?}");
    }
}

#[test]
fn issue554_positioned_bindings_check_policy_after_rhs() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    check(
        r#"(mapcar
  (lambda (kind)
    (mapcar
     (lambda (initial)
       (let* ((swp (position-symbol 'issue554-binding-n 7))
              (rhs (list 'progn
                         (list 'setq 'symbols-with-pos-enabled (null initial))
                         7))
              (form (if (eq kind 'setq)
                        (list 'setq 'issue554-binding-n 7)
                      (list kind (list (list 'issue554-binding-n 7))
                            'issue554-binding-n)))
              (fun (list 'lambda nil form)))
         ;; Warm a bare-symbol body, then mutate its existing binder.
         ;; This exercises the live checks of an already compiled Tier-I body.
         (funcall fun)
         (if (eq kind 'setq)
             (progn (setcar (cdr form) swp)
                    (setcar (cdr (cdr form)) rhs))
           (setcar (car (car (cdr form))) swp)
           (if (eq kind 'let)
               (setcdr (car (cdr form))
                       (list (list 'issue554-binding-switch rhs)))
             (setcar (cdr (car (car (cdr form)))) rhs)))
         (let ((symbols-with-pos-enabled initial))
           (condition-case err (funcall fun)
             (error (car err))))))
     '(nil t)))
  '(setq let let*))"#,
        "((7 wrong-type-argument) (7 wrong-type-argument) (7 wrong-type-argument))",
    );
}

#[test]
fn issue554_positioned_setq_rhs_runs_before_symbol_error() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    check(
        r#"(let ((symbols-with-pos-enabled nil)
      (ran nil)
      (swp (position-symbol 'issue554-binding-n 8)))
  (let ((result (condition-case err
                    (eval (list 'setq swp '(setq ran t)))
                  (error (car err)))))
    (list result ran)))"#,
        "(wrong-type-argument t)",
    );
}

#[test]
fn issue554_positioned_let_initializers_run_before_symbol_error() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    check(
        r#"(let ((symbols-with-pos-enabled nil)
      (ran nil)
      (swp (position-symbol 'issue554-binding-n 8)))
  (let ((result (condition-case err
                    (eval (list 'let (list (list swp 7)
                                          '(issue554-binding-switch (setq ran t)))
                                'issue554-binding-n))
                  (error (car err)))))
    (list result ran)))"#,
        "(wrong-type-argument t)",
    );
}

#[test]
fn issue554_positioned_catch_matches_throw_policy_before_unwind() {
    // GNU eval.c:491-502 (setq),1104-1108 (let*),1154-1180 (let),
    // and1477 (throw EQ before unwinding),lisp.h:1316-1323 (EQ policy).
    check(
        r#"(let ((symbols-with-pos-enabled nil)
      (swp (position-symbol 'issue554-binding-catch 8)))
  (list
   (catch 'issue554-binding-catch
     (let ((symbols-with-pos-enabled t)) (throw swp 7)))
   (catch swp
     (let ((symbols-with-pos-enabled nil)) (throw swp 9)))))"#,
        "(7 9)",
    );
}
