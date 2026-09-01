//! Complex combo batch 320 — `advice` ultimate: add/remove with all
//! combinations, advice-member-p, advice-mapc iterate, advice on subr
//! builtin, advice with :override completely replacing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx320_advice_before_after_around_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (42 ((:around-enter 21) (:before 21) (:primary 21) (:after 21) (:around-exit 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx320-target :before
              (lambda (x) (push (list :before x) calls)) '((name . adv-b)))
  (advice-add 'neo-cx320-target :after
              (lambda (x) (push (list :after x) calls)) '((name . adv-a)))
  (advice-add 'neo-cx320-target :around
              (lambda (fn x) (push (list :around-enter x) calls)
                (let ((r (funcall fn x)))
                  (push (list :around-exit r) calls)
                  r)) '((name . adv-ar)))
  (prog1 (list (neo-cx320-target 21) (nreverse calls))
    (advice-remove 'neo-cx320-target 'adv-b)
    (advice-remove 'neo-cx320-target 'adv-a)
    (advice-remove 'neo-cx320-target 'adv-ar)))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_override_completely_replaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (500 (:override) 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-orig (x) (push :orig calls) (* x 2))
  (advice-add 'neo-cx320-orig :override
              (lambda (x) (push :override calls) (* x 100)) '((name . adv-ov)))
  (let ((r (neo-cx320-orig 5)))
    (advice-remove 'neo-cx320-orig 'adv-ov)
    (list r (nreverse calls) (neo-cx320-orig 5))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_filter_args_modifies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (60 ((:primary (10 20 30))))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-sum (&rest args) (push (list :primary args) calls) (apply #'+ args))
  (advice-add 'neo-cx320-sum :filter-args
              (lambda (args) (mapcar (lambda (x) (* x 10)) args)) '((name . adv-fa)))
  (let ((r (neo-cx320-sum 1 2 3)))
    (advice-remove 'neo-cx320-sum 'adv-fa)
    (list r (nreverse calls))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_filter_return_modifies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (200 (:primary (:filtered 100)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-base () (push :primary calls) 100)
  (advice-add 'neo-cx320-base :filter-return
              (lambda (r) (push (list :filtered r) calls) (* r 2)) '((name . adv-fr)))
  (let ((r (neo-cx320-base)))
    (advice-remove 'neo-cx320-base 'adv-fr)
    (list r (nreverse calls))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_member_p_and_mapc_iterate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function symbol<)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx320-mp () :ok)
(advice-add 'neo-cx320-mp :before (lambda () :a) '((name . my-advice)))
(advice-add 'neo-cx320-mp :after (lambda () :b) '((name . other-advice)))
(let (names)
  (advice-mapc (lambda (adv props) (push (plist-get props 'name) names)) 'neo-cx320-mp)
  (let ((result (sort names #'symbol<)))
    (advice-remove 'neo-cx320-mp 'my-advice)
    (advice-remove 'neo-cx320-mp 'other-advice)
    result))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_on_subr_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (calls)
      (advice-add 'car :around
                  (lambda (fn x) (push :around calls) (funcall fn x))
                  '((name . subr-adv)))
      (let ((r (car '(1 2 3))))
        (advice-remove 'car 'subr-adv)
        (list r (length calls) (car '(4 5 6)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_multiple_before_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable neo-cx320-multi-wrapper)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-multi () (push :primary calls) :r)
  (dolist (name '(:first :second :third))
    (add-function :before (var 'neo-cx320-multi-wrapper)
                  (let ((n name))
                    (lambda (&rest _) (push n calls)))
                  `((name . ,name))))
  (let ((result (funcall neo-cx320-multi-wrapper)))
    (dolist (name '(:first :second :third))
      (remove-function (var 'neo-cx320-multi-wrapper) name))
    (list result (nreverse calls))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_remove_restores_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 10 ((:before 5) (:primary 5) (:primary 5)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-rm (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx320-rm :before
              (lambda (x) (push (list :before x) calls)) '((name . adv-rm)))
  (let ((with-advice (neo-cx320-rm 5)))
    (advice-remove 'neo-cx320-rm 'adv-rm)
    (let ((after-remove (neo-cx320-rm 5)))
      (list with-advice after-remove (nreverse calls)))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_define_advice_legacy_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 50""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defun neo-cx320-da (x) x)
      (define-advice neo-cx320-da (:filter-args (args))
        (mapcar (lambda (a) (* a 10)) args))
      (let ((r (neo-cx320-da 5)))
        (advice-remove 'neo-cx320-da (intern "neo-cx320-da@:filter-args"))
        r))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx320_advice_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx320-mega (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx320-mega :before
              (lambda (x) (push (list :before x) calls)) '((name . mega-adv-1)))
  (advice-add 'neo-cx320-mega :after
              (lambda (x) (push (list :after x) calls)) '((name . mega-adv-2)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Advice mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((r (neo-cx320-mega 21)))
        (let ((state (list r (nreverse calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (advice-remove 'neo-cx320-mega 'mega-adv-1)
          (advice-remove 'neo-cx320-mega 'mega-adv-2)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
