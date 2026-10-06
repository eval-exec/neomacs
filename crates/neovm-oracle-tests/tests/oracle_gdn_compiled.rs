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

#[test]
fn oracle_gdn_compiled_multibyte_transpose_anchors() {
    // GNU editfns.c:4631-4641 moves the gap, and 4782-4797 preserves markers.
    common::assert_oracle_parity_expect(
        r#"(progn (require 'bytecomp)
  (let ((fn (byte-compile
    (lambda (shape)
      (with-temp-buffer
        (if (eq shape 'gap)
          (progn (insert "b") (goto-char 1) (insert "é")
            (transpose-regions 1 2 2 3)
            (list (buffer-string) (char-after 1) (char-after 2)))
          (insert "a中")
          (let ((m (copy-marker 2)))
            (transpose-regions 1 2 2 3 t)
            (list (buffer-string) (char-after m) (position-bytes 2)
                  (marker-position m)))))))))
    (mapcar (lambda (shape)
      (let (answer) (dotimes (_ 20) (setq answer (funcall fn shape))) answer))
      '(gap markers))))"#,
        expect_test::expect![[r#""OK ((\"bé\" 98 233) (\"中a\" 97 4 2))""#]],
    );
}
