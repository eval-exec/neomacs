//! GNU-refreshed GDN primitive opcode regressions. Run with NEOVM_JIT=0,
//! and with NEOVM_JIT_THRESHOLD=1 NEOVM_JIT_BG=sync for deterministic tier-up.
#[path = "../src/common.rs"]
mod common;

#[test]
fn oracle_gdn_compiled_car_cycle_equal() {
    // GNU bytecode.c:1584-1589 delegates Bequal to fns.c:2860-2885.
    common::assert_oracle_parity_expect(
        r#"(progn (require 'bytecomp)
  (let ((fn (byte-compile (lambda (a b) (equal a b))))
        (a (list 1)) (b (list 1)) (c (list 1 2)) (d (list 1 3))
        (x 0) (y 0) same different)
    (setcar a a) (setcar b b) (setcar c c) (setcar d d)
    (dotimes (_ 20)
      (setq same (funcall fn a b) different (funcall fn c d)))
    (dotimes (_ 300) (setq x (list x) y (list y)))
    (list same different
      (condition-case e (funcall fn x y) (error e)))))"#,
        expect_test::expect![[r#""OK (t nil (error \"Stack overflow in equal\"))""#]],
    );
}
