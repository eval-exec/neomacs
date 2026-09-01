//! Complex combo batch 90 — advice system deep: advice-add/remove with
//! `:before`/`:after`/`:around`/`:override`/`:filter-args`, advice on builtins
//! and lambdas, `advice-member-p`, `advice-mapc`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx90_advice_add_before_after_around() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (42 ((:around-enter 21) (:before 21) (:primary 21) (:after 21) (:around-exit 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx90-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx90-target :before
              (lambda (x) (push (list :before x) calls)))
  (advice-add 'neo-cx90-target :after
              (lambda (x) (push (list :after x) calls)))
  (advice-add 'neo-cx90-target :around
              (lambda (fn x) (push (list :around-enter x) calls)
                (let ((r (funcall fn x)))
                  (push (list :around-exit r) calls)
                  r)))
  (prog1 (list (neo-cx90-target 21)
               (nreverse calls))
    (advice-remove 'neo-cx90-target (advice--p (advice-member-p nil 'neo-cx90-target)))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_override_completely_replaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 4100 4100)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx90-orig (x) (+ x 1))
(let ((before-orig (neo-cx90-orig 41)))
  (advice-add 'neo-cx90-orig :override (lambda (x) (* x 100)))
  (let ((after-advice (neo-cx90-orig 41)))
    (advice-remove 'neo-cx90-orig (advice--p (advice-member-p nil 'neo-cx90-orig)))
    (list before-orig after-advice (neo-cx90-orig 41))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_filter_args_modifies_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 60""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx90-sum (&rest args) (apply #'+ args))
(advice-add 'neo-cx90-sum :filter-args
            (lambda (args) (mapcar (lambda (x) (* x 10)) args)))
(let ((result (neo-cx90-sum 1 2 3)))
  (advice-remove 'neo-cx90-sum (advice--p (advice-member-p nil 'neo-cx90-sum)))
  result)
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_filter_return_modifies_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 200""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defun neo-cx90-r () 100)
      (advice-add 'neo-cx90-r :filter-return
                  (lambda (r) (* r 2)))
      (let ((result (neo-cx90-r)))
        (advice-remove 'neo-cx90-r (advice--p (advice-member-p nil 'neo-cx90-r)))
        result))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_before_until_skips_primary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (:bu))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (calls)
      (defun neo-cx90-bu (x) (push :primary calls) :primary-ran)
      (advice-add 'neo-cx90-bu :before-until
                  (lambda (x) (push :bu calls) t))
      (let ((result (neo-cx90-bu 5)))
        (advice-remove 'neo-cx90-bu (advice--p (advice-member-p nil 'neo-cx90-bu)))
        (list result (nreverse calls))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_after_while_runs_only_if_return_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (:aw))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (calls)
      (defun neo-cx90-aw () :result)
      (advice-add 'neo-cx90-aw :after-while
                  (lambda (&rest _) (push :aw calls) nil))
      (let ((r1 (neo-cx90-aw)))
        (advice-remove 'neo-cx90-aw (advice--p (advice-member-p nil 'neo-cx90-aw)))
        (list r1 (nreverse calls))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_multiple_ordering_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:r (:third :second :first :primary))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx90-multi () (push :primary calls) :r)
  (dolist (name '(:first :second :third))
    (advice-add 'neo-cx90-multi :before
                (let ((n name))
                  (lambda () (push n calls)))
                `((name . ,name))))
  (prog1 (list (neo-cx90-multi) (nreverse calls))
    (dolist (name '(:first :second :third))
      (advice-remove 'neo-cx90-multi name))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_member_p_and_advice_mapc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((advice oclosure) 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx90-check () :ok)
(let ((adv (lambda () :adviced)))
  (advice-add 'neo-cx90-check :before adv `((name . my-advice)))
  (let ((members-before (advice--p (advice-member-p 'my-advice 'neo-cx90-check)))
        (count 0))
    (advice-mapc (lambda (_a _p) (cl-incf count)) 'neo-cx90-check)
    (advice-remove 'neo-cx90-check 'my-advice)
    (let ((members-after (advice--p (advice-member-p 'my-advice 'neo-cx90-check))))
      (list members-before count members-after))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_chain_on_subr_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (calls)
      (advice-add 'car :around
                  (lambda (fn x) (push :around calls) (funcall fn x))
                  '((name . neo-cx90-car-advice)))
      (let ((result (car '(1 2 3)))
            (num-calls (length calls)))
        (advice-remove 'car 'neo-cx90-car-advice)
        (list result num-calls (car '(4 5 6)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_on_lambda_via_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 (:before :primary))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defalias 'neo-cx90-lam (lambda (x) (push :primary calls) (* x 3)))
  (advice-add 'neo-cx90-lam :before (lambda (x) (push :before calls)))
  (let ((r (neo-cx90-lam 5)))
    (advice-remove 'neo-cx90-lam (advice--p (advice-member-p nil 'neo-cx90-lam)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_define_advice_legacy_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 50""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defun neo-cx90-da (x) x)
      (define-advice neo-cx90-da (:filter-args (args))
        (mapcar (lambda (a) (* a 10)) args))
      (let ((r (neo-cx90-da 5)))
        (advice-remove 'neo-cx90-da (intern "neo-cx90-da@:filter-args"))
        r))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx90_advice_chain_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx90-mega-fn (x)
    (push (list :primary x) calls)
    (* x 2))
  (advice-add 'neo-cx90-mega-fn :before
              (lambda (x) (push (list :before x) calls))
              '((name . before-advice)))
  (advice-add 'neo-cx90-mega-fn :after
              (lambda (x) (push (list :after x) calls))
              '((name . after-advice)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Advice test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((r (neo-cx90-mega-fn 21)))
        (let ((state (list r (nreverse calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (advice-remove 'neo-cx90-mega-fn 'before-advice)
          (advice-remove 'neo-cx90-mega-fn 'after-advice)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
