//! Strict combo oracle probes, batch 216: advice :around/:before/:after deep
//! combination. Multi-depth advice ordering via a call log, advice-member-p,
//! advice-mapc enumeration, and advice-remove isolation.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_advice_around_before_after_order_log() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defvar probe-adv-log nil)
  (defun probe-adv-target (x) (push 'primary probe-adv-log) (* x 2))
  (let ((before (lambda (x) (push 'before probe-adv-log)))
        (after (lambda (x) (push 'after probe-adv-log)))
        (around (lambda (fn x) (push 'around-enter probe-adv-log)
                  (let ((r (funcall fn x)))
                    (push 'around-exit probe-adv-log) r))))
    (advice-add 'probe-adv-target :before before)
    (advice-add 'probe-adv-target :after after)
    (advice-add 'probe-adv-target :around around)
    (let ((result (probe-adv-target 5)))
      (prog1
          (list result (nreverse probe-adv-log)
                (advice-member-p around 'probe-adv-target)
                (advice-member-p before 'probe-adv-target)
                (advice-member-p after 'probe-adv-target))
        (advice-remove 'probe-adv-target around)
        (advice-remove 'probe-adv-target before)
        (advice-remove 'probe-adv-target after)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (10 (around-enter before primary after around-exit) #[128 \"���\u{3}#�\" [#[(fn x) ((push 'around-enter probe-adv-log) (let ((r (funcall fn x))) (push 'around-exit probe-adv-log) r)) (t)] #[128 \"��\u{2}\\\"��\u{3}\\\"��\" [#[(x) ((push 'after probe-adv-log)) (t)] #[128 \"��\u{2}\\\"���\u{2}\\\"�\" [#[(x) ((push 'before probe-adv-log)) (t)] #[(x) ((push 'primary probe-adv-log) (* x 2)) (t)] :before nil apply] 4 advice] :after nil apply] 5 advice] :around nil apply] 5 advice] #[128 \"��\u{2}\\\"���\u{2}\\\"�\" [#[(x) ((push 'before probe-adv-log)) (t)] #[(x) ((push 'primary probe-adv-log) (* x 2)) (t)] :before nil apply] 4 advice] #[128 \"��\u{2}\\\"��\u{3}\\\"��\" [#[(x) ((push 'after probe-adv-log)) (t)] #[128 \"��\u{2}\\\"���\u{2}\\\"�\" [#[(x) ((push 'before probe-adv-log)) (t)] #[(x) ((push 'primary probe-adv-log) (* x 2)) (t)] :before nil apply] 4 advice] :after nil apply] 5 advice])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_advice_mapc_filter_after_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defun probe-adv-mapc-target () 'orig)
  (let ((a1 (lambda (&rest _)))
        (a2 (lambda (&rest _))))
    (advice-add 'probe-adv-mapc-target :before a1)
    (advice-add 'probe-adv-mapc-target :after a2)
    (let ((count-before 0))
      (advice-mapc (lambda (_f _p) (setq count-before (1+ count-before)))
                   'probe-adv-mapc-target)
      (advice-remove 'probe-adv-mapc-target a1)
      (let ((count-after 0))
        (advice-mapc (lambda (_f _p) (setq count-after (1+ count-after)))
                     'probe-adv-mapc-target)
        (prog1
            (list count-before count-after
                  (advice-member-p a1 'probe-adv-mapc-target)
                  (advice-member-p a2 'probe-adv-mapc-target))
          (advice-remove 'probe-adv-mapc-target a2))))))
"##;
    let expect = expect_test::expect![[r#""OK (1 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_advice_filter_interpret_args_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defun probe-adv-flt (x y) (list 'result x y))
  (let ((filter-args (lambda (x y) (list (* x 10) (* y 10))))
        (filter-return (lambda (r) (cons 'modified r))))
    (advice-add 'probe-adv-flt :filter-args filter-args)
    (advice-add 'probe-adv-flt :filter-return filter-return)
    (let ((result (probe-adv-flt 2 3)))
      (prog1
          (list result (probe-adv-flt 1 1))
        (advice-remove 'probe-adv-flt filter-args)
        (advice-remove 'probe-adv-flt filter-return)))))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (x y) (list (* x 10) (* y 10))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
