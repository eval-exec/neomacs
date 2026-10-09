//! GNU-refreshed GDN primitive opcode regressions. Run with NEOVM_JIT=0,
//! and with NEOVM_JIT_THRESHOLD=1 NEOVM_JIT_BG=sync for deterministic tier-up.
#[path = "../src/common.rs"]
mod common;

#[test]
fn oracle_gdn_compiled_foreign_marker_reads() {
    // GNU bytecode.c:1440-1442 delegates Bchar_after to editfns.c:1052-1057.
    // bytecomp.el:5956-5958 rewrites char-before; runtime funcall also checks
    // the primitive's distinct foreign-marker byte coordinates.
    common::assert_oracle_parity_expect(
        r#"(progn (require 'bytecomp)
  (let ((fn (byte-compile (lambda (m before)
      (list (char-after m) (char-before m) (funcall before m)))))
        (donor (generate-new-buffer " gdn-compiled-marker")))
    (unwind-protect
      (progn
        (with-current-buffer donor (insert "é中😀abcd"))
        (mapcar (lambda (shape)
          (with-temp-buffer
            (when (eq shape 'unibyte) (set-buffer-multibyte nil))
            (insert (if (eq shape 'mixed) "é中😀abcdefghijk" "abcdefghijklmnop"))
            (let ((m (with-current-buffer donor (copy-marker 4))) answer)
              (dotimes (_ 20) (setq answer (funcall fn m #'char-before))) answer)))
          '(unibyte ascii-multibyte mixed)))
      (kill-buffer donor))))"#,
        expect_test::expect![[r#""OK ((106 99 105) (106 99 105) (97 128512 128512))""#]],
    );
}

#[test]
fn oracle_gdn_compiled_script_word_motion() {
    // GNU bytecode.c:1492-1494 delegates Bforward_word to syntax.c:1477-1556.
    common::assert_oracle_parity_expect(
        r#"(progn (require 'bytecomp)
  (let ((fn (byte-compile (lambda (n) (forward-word n) (point)))))
    (mapcar (lambda (text)
      (with-temp-buffer (insert text)
        (let (forward backward)
          (dotimes (_ 20)
            (goto-char (point-min)) (setq forward (funcall fn 1))
            (goto-char (point-max)) (setq backward (funcall fn -1)))
          (list forward backward))))
      '("中bc" "abαβ" "ひらカタ" "abc中" "éabc"))))"#,
        expect_test::expect![[r#""OK ((2 2) (3 3) (3 3) (4 4) (5 1))""#]],
    );
}

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
