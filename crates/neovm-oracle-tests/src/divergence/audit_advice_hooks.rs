//! Advice & hooks source-audit divergences (nadvice.el + eval.c vs neovm-core
//! builtins/hooks.rs + hook_runtime.rs + eval.rs).
//!
//! Probes the subtle advice-flavor semantics (:before-until/:before-while/
//! :after-until/:after-while/:filter-args/:filter-return/:override), advice on
//! subrs and macros, nested advice, run-hook-with-args-until-failure/until-
//! success/wrapped return semantics, and make-local-hook.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

fn _u() {}

macro_rules! flav {
    ($name:ident, $form:expr) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            crate::common::assert_oracle_parity($form);
        }
    };
}

flav!(
    div_ah_before_after_order,
    r##"
(let (log)
  (advice-add 'neo-fba :before (lambda (&rest a) (push :before log)))
  (advice-add 'neo-fba :after  (lambda (&rest a) (push :after log)))
  (fset 'neo-fba (lambda (x) (push :main log) x))
  (list (neo-fba 5) (reverse log)))"##
);

flav!(
    div_ah_around_in_out,
    r##"
(let (log)
  (advice-add 'neo-far :around
    (lambda (fn &rest a) (push :in log) (let ((r (apply fn a))) (push :out log) r)))
  (fset 'neo-far (lambda (x) (push :main log) (* x 2)))
  (list (neo-far 5) (reverse log)))"##
);

flav!(
    div_ah_override_flavor,
    r##"
(progn
  (advice-add 'neo-fov :override (lambda (&rest a) :overridden))
  (fset 'neo-fov (lambda () :original))
  (neo-fov))"##
);

flav!(
    div_ah_before_until,
    r##"
(progn
  (advice-add 'neo-fbu :before-until (lambda (&rest a) t))
  (fset 'neo-fbu (lambda (x) (* x 10)))
  (neo-fbu 5))"##
);

flav!(
    div_ah_before_while,
    r##"
(progn
  (advice-add 'neo-fbw :before-while (lambda (&rest a) nil))
  (fset 'neo-fbw (lambda (x) (* x 10)))
  (neo-fbw 5))"##
);

flav!(
    div_ah_after_until,
    r##"
(progn
  (advice-add 'neo-fau :after-until (lambda (&rest a) (cons :after (car a))))
  (fset 'neo-fau (lambda (x) (* x 10)))
  (neo-fau 5))"##
);

flav!(
    div_ah_after_while,
    r##"
(let (ran)
  (advice-add 'neo-faw :after-while (lambda (&rest a) (setq ran :after-ran)))
  (fset 'neo-faw (lambda (x) nil))
  (list (neo-faw 5) ran))"##
);

flav!(
    div_ah_filter_args,
    r##"
(progn
  (advice-add 'neo-ffa :filter-args (lambda (a) (list (* (car a) 100))))
  (fset 'neo-ffa (lambda (x) x))
  (neo-ffa 3))"##
);

flav!(
    div_ah_filter_return,
    r##"
(progn
  (advice-add 'neo-ffr :filter-return (lambda (r) (* r 100)))
  (fset 'neo-ffr (lambda (x) (* x 2)))
  (neo-ffr 3))"##
);

flav!(
    div_ah_advice_on_subr,
    r##"
(progn
  (advice-add '+ :around (lambda (fn &rest a) (1+ (apply fn a))))
  (+ 1 2))"##
);

flav!(
    div_ah_nested_around,
    r##"
(let (log)
  (advice-add 'neo-fn1 :around (lambda (fn &rest a) (push :a-in log) (let ((r (apply fn a))) (push :a-out log) r)))
  (advice-add 'neo-fn1 :around (lambda (fn &rest a) (push :b-in log) (let ((r (apply fn a))) (push :b-out log) r)))
  (fset 'neo-fn1 (lambda () (push :main log)))
  (list (neo-fn1) (reverse log)))"##
);

flav!(
    div_ah_advice_member_p_remove,
    r##"
(let ((adv (lambda (fn &rest a) (apply fn a))))
  (advice-add 'neo-fmr :around adv)
  (list (advice-member-p adv 'neo-fmr)
        (progn (advice-remove 'neo-fmr adv) (advice-member-p adv 'neo-fmr))))"##
);

flav!(
    div_ah_add_function_remove,
    r##"
(progn
  (defvar neo-afg (lambda (x) (1+ x)))
  (add-function :filter-args 'neo-afg (lambda (a) (list (* (car a) 10))))
  (funcall neo-afg 5))"##
);

flav!(
    div_ah_run_hooks_return,
    r##"
(progn
  (defvar neo-rh nil)
  (add-hook 'neo-rh (lambda () :one))
  (list (run-hooks 'neo-rh) (consp neo-rh)))"##
);

flav!(
    div_ah_run_until_failure,
    r##"
(let ((neo-uf (list (lambda (x) (cons :a x)) (lambda (x) nil) (lambda (x) :nope))))
  (run-hook-with-args-until-failure 'neo-uf 7))"##
);

flav!(
    div_ah_run_until_success,
    r##"
(let ((neo-us (list (lambda (x) nil) (lambda (x) (cons :hit x)) (lambda (x) :nope))))
  (run-hook-with-args-until-success 'neo-us 7))"##
);

flav!(
    div_ah_run_hook_wrapped,
    r##"
(let ((neo-wh (list (lambda (x) (* x 2)) (lambda (x) (* x 3)))))
  (run-hook-wrapped 'neo-wh (lambda (fn x) (funcall fn (1+ x))) 5))"##
);

flav!(
    div_ah_make_local_hook,
    r##"
(condition-case e (make-local-hook 'neo-mlh) (error (car e)))"##
);

flav!(
    div_ah_add_hook_local_flag,
    r##"
(let ((neo-lh nil))
  (add-hook 'neo-lh (lambda () :local) t t)
  (list (local-variable-p 'neo-lh)
        (length neo-lh)))"##
);

flav!(
    div_ah_hook_depth_ordering,
    r##"
(let (log (neo-hd nil))
  (add-hook 'neo-hd (lambda () (push 1 log)) t)
  (add-hook 'neo-hd (lambda () (push 2 log)) 90)
  (add-hook 'neo-hd (lambda () (push 3 log)) 10)
  (run-hooks 'neo-hd)
  (nreverse log))"##
);
