//! Oracle parity for the JIT's leaf builtins (`NEOVM_JIT_LEAF`, design
//! `p1-2-builtin-intrinsics`): every form byte-compiles its callers, warms
//! them past the tier-up threshold, and runs under `NEOVM_JIT_THRESHOLD=1`
//! with every leaf part on (GNU ignores both variables), so the values,
//! the error data and the backtrace frames printed come from the leaf
//! paths: the opcode trampolines, the Bcall trampolines with their lazy
//! frame, and the inline string `aref`/`aset`.
//!
//! GNU records a frame for `Bnth`'s and `Belt`'s error only (neomacs
//! records none for any inline opcode: design §1.6, commit 13), so the
//! opcode form compares error data, not frames.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Every leaf part on, every function compiled at its first call.
const LEAF_ENV: &[(&str, &str)] = &[("NEOVM_JIT_THRESHOLD", "1"), ("NEOVM_JIT_LEAF", "all")];

/// `gethash`, `plist-get` and `get-char-property` through `Op::Call`:
/// values (including a user-defined hash test and a PREDICATE, which the
/// leaves decline), and on a signal the builtin's frame with the call's own
/// arguments, as `handler-bind` + `mapbacktrace` see it (GNU `Bcall`).
#[test]
fn oracle_jit_leaf_bcall_values_errors_and_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--leaf-frames
    (lambda (thunk)
      (let (frames)
        (condition-case err
            (handler-bind
                ((error
                  (lambda (_e)
                    (mapbacktrace
                     (lambda (_evald fn args _flags)
                       (when (memq fn '(gethash plist-get get-char-property))
                         (push (cons fn args) frames)))))))
              (list 'value (funcall thunk)))
          (error (list 'error err (nreverse frames)))))))
  (defalias 'neovm--leaf-gh (byte-compile (lambda (k h) (gethash k h))))
  (defalias 'neovm--leaf-gh3 (byte-compile (lambda (k h d) (gethash k h d))))
  (defalias 'neovm--leaf-pg (byte-compile (lambda (p k) (plist-get p k))))
  (defalias 'neovm--leaf-pg3 (byte-compile (lambda (p k f) (plist-get p k f))))
  (defalias 'neovm--leaf-gcp (byte-compile (lambda (pos prop) (get-char-property pos prop))))
  (define-hash-table-test 'neovm--leaf-ci
    (lambda (a b) (string= (upcase a) (upcase b)))
    (lambda (k) (sxhash-equal (upcase k))))
  (let ((h (make-hash-table))
        (ci (make-hash-table :test 'neovm--leaf-ci)))
    (puthash 1 'one h)
    (puthash "Key" 'ci-value ci)
    (with-temp-buffer
      (insert "hello world")
      (put-text-property 3 5 'p 'text)
      (overlay-put (make-overlay 4 7) 'p 'overlay)
      (dotimes (_ 60)
        (neovm--leaf-gh 1 h) (neovm--leaf-gh3 2 h 'd) (neovm--leaf-pg '(a 1) 'a)
        (neovm--leaf-pg3 '(a 1) 'a nil) (neovm--leaf-gcp 3 'p))
      (list
       (neovm--leaf-gh 1 h)
       (neovm--leaf-gh3 2 h 'dflt)
       (neovm--leaf-gh "KEY" ci)
       (neovm--leaf-pg '(a 1 b 2) 'b)
       (neovm--leaf-pg3 '("a" 1 "b" 2) "b" #'equal)
       (neovm--leaf-pg 5 'a)
       (list (neovm--leaf-gcp 3 'p) (neovm--leaf-gcp 5 'p) (neovm--leaf-gcp 8 'p))
       (neovm--leaf-frames (lambda () (neovm--leaf-gh 1 5)))
       (neovm--leaf-frames (lambda () (neovm--leaf-gh3 1 'x 'd)))
       (neovm--leaf-frames (lambda () (neovm--leaf-pg3 '(a 1) 'a 'neovm--no-such-fn)))
       (neovm--leaf-frames (lambda () (neovm--leaf-gcp 100 'p)))
       (neovm--leaf-frames (lambda () (neovm--leaf-gcp 'x 'p)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (one dflt ci-value 2 2 nil (text overlay nil) (error (wrong-type-argument hash-table-p 5) ((gethash 1 5))) (error (wrong-type-argument hash-table-p x) ((gethash 1 x d))) (error (void-function neovm--no-such-fn) ((plist-get (a 1) a neovm--no-such-fn))) (error (args-out-of-range 100) ((get-char-property 100 p))) (error (wrong-type-argument integer-or-marker-p x) ((get-char-property x p))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, LEAF_ENV, expect);
}

/// The opcode leaves (`Bnth`, `Bnthcdr`, `Belt`, `Blength`, `Bget`,
/// `Bmember`, `Bequal`, `Bstring_eqlsign`, `Bstring_lessp`): values and the
/// exact error data, including `Bnth`'s and `Belt`'s tail datum.
#[test]
fn oracle_jit_leaf_opcode_error_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--leaf-try
    (lambda (thunk)
      (condition-case err (list 'value (funcall thunk)) (error (list 'error err)))))
  (defalias 'neovm--leaf-nth (byte-compile (lambda (n l) (nth n l))))
  (defalias 'neovm--leaf-nthcdr (byte-compile (lambda (n l) (nthcdr n l))))
  (defalias 'neovm--leaf-elt (byte-compile (lambda (s n) (elt s n))))
  (defalias 'neovm--leaf-length (byte-compile (lambda (s) (length s))))
  (defalias 'neovm--leaf-get (byte-compile (lambda (s p) (get s p))))
  (defalias 'neovm--leaf-member (byte-compile (lambda (e l) (member e l))))
  (defalias 'neovm--leaf-equal (byte-compile (lambda (a b) (equal a b))))
  (defalias 'neovm--leaf-string= (byte-compile (lambda (a b) (string= a b))))
  (defalias 'neovm--leaf-string< (byte-compile (lambda (a b) (string< a b))))
  (put 'neovm--leaf-sym 'p 'prop)
  (dotimes (_ 60)
    (neovm--leaf-nth 1 '(a b)) (neovm--leaf-nthcdr 1 '(a b)) (neovm--leaf-elt '(a b) 1)
    (neovm--leaf-length '(a b)) (neovm--leaf-get 'neovm--leaf-sym 'p)
    (neovm--leaf-member 'b '(a b)) (neovm--leaf-equal "a" "a")
    (neovm--leaf-string= "a" "a") (neovm--leaf-string< "a" "b"))
  (list
   (list (neovm--leaf-nth 1 '(a b c)) (neovm--leaf-nth 5 '(a b)) (neovm--leaf-nth -1 '(a b))
         (neovm--leaf-nth 200 '(a b)) (neovm--leaf-nthcdr 2 '(a b c)) (neovm--leaf-elt [x y] 1)
         (neovm--leaf-elt '(x y) 1) (neovm--leaf-length '(a b c)) (neovm--leaf-length "abc")
         (neovm--leaf-length [1 2]) (neovm--leaf-get 'neovm--leaf-sym 'p)
         (neovm--leaf-member "b" '("a" "b")) (neovm--leaf-equal '(1 "x") '(1 "x"))
         (neovm--leaf-string= "abc" 'abc) (neovm--leaf-string< 'a "b"))
   (neovm--leaf-try (lambda () (neovm--leaf-nth 2 '(a . b))))
   (neovm--leaf-try (lambda () (neovm--leaf-nth 'x '(a b))))
   (neovm--leaf-try (lambda () (neovm--leaf-nthcdr 3 '(a . b))))
   (neovm--leaf-try (lambda () (neovm--leaf-elt [1 2] 5)))
   (neovm--leaf-try (lambda () (neovm--leaf-elt '(1 . 2) 3)))
   (neovm--leaf-try (lambda () (neovm--leaf-elt '(a . b) 2)))
   (neovm--leaf-try (lambda () (neovm--leaf-elt '(a b . c) 3)))
   (neovm--leaf-try (lambda () (neovm--leaf-elt '(a . b) 200)))
   (neovm--leaf-try (lambda () (neovm--leaf-length '(a b . c))))
   (neovm--leaf-try (lambda () (neovm--leaf-length 5)))
   (neovm--leaf-try (lambda () (neovm--leaf-get 5 'p)))
   (neovm--leaf-try (lambda () (neovm--leaf-member 1 '(a . b))))
   (neovm--leaf-try (lambda () (neovm--leaf-string= 1 "a")))
   (neovm--leaf-try (lambda () (neovm--leaf-string< "a" 2)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((b nil a nil (c) y y 3 3 2 prop (\"b\") t t t) (error (wrong-type-argument listp b)) (error (wrong-type-argument integerp x)) (error (wrong-type-argument listp (a . b))) (error (args-out-of-range [1 2] 5)) (error (wrong-type-argument listp 2)) (error (wrong-type-argument listp b)) (error (wrong-type-argument listp c)) (error (wrong-type-argument listp (a . b))) (error (wrong-type-argument listp c)) (error (wrong-type-argument sequencep 5)) (error (wrong-type-argument symbolp 5)) (error (wrong-type-argument listp (a . b))) (error (wrong-type-argument stringp 1)) (error (wrong-type-argument stringp 2)))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, LEAF_ENV, expect);
}

/// Inline string `aref`/`aset` (I1/I2): unibyte, all-ASCII multibyte and
/// non-ASCII strings, width-changing stores (GNU 31 refuses them), and the
/// exact `args-out-of-range`/`wrong-type-argument` data.
#[test]
fn oracle_jit_leaf_string_aref_aset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--leaf-aref (byte-compile (lambda (s i) (aref s i))))
  (defalias 'neovm--leaf-aset (byte-compile (lambda (s i c) (aset s i c))))
  (defalias 'neovm--leaf-try
    (lambda (thunk) (condition-case err (list 'value (funcall thunk)) (error (list 'error err)))))
  (let ((u (make-string 4 ?a))
        (m (string-to-multibyte (make-string 4 ?a)))
        (n (copy-sequence "aβc"))
        (b (string-to-unibyte "a\377c")))
    (dotimes (i 60)
      (neovm--leaf-aref u (% i 4)) (neovm--leaf-aset u (% i 4) ?a)
      (neovm--leaf-aref m (% i 4)) (neovm--leaf-aset m (% i 4) ?a))
    (list
     (list (neovm--leaf-aset u 0 ?z) (neovm--leaf-aset u 3 255) (neovm--leaf-aref u 3)
           u (multibyte-string-p u))
     (list (neovm--leaf-aset m 1 ?q) (neovm--leaf-aref m 1) m (multibyte-string-p m))
     (list (neovm--leaf-aref n 1) (neovm--leaf-aset n 0 ?Z) n)
     (list (neovm--leaf-aref b 1) (neovm--leaf-aset b 1 ?Y) b)
     (let ((w (make-string 2 ?a)))
       (list (neovm--leaf-try (lambda () (neovm--leaf-aset w 0 ?λ))) w (multibyte-string-p w)))
     (let ((w (string-to-multibyte (make-string 2 ?a))))
       (list (neovm--leaf-try (lambda () (neovm--leaf-aset w 1 200))) w (multibyte-string-p w)))
     (neovm--leaf-try (lambda () (neovm--leaf-aref "abc" 5)))
     (neovm--leaf-try (lambda () (neovm--leaf-aref "abc" -1)))
     (neovm--leaf-try (lambda () (neovm--leaf-aref "abc" 'x)))
     (neovm--leaf-try (lambda () (neovm--leaf-aref "" 0)))
     (neovm--leaf-try (lambda () (neovm--leaf-aset (make-string 2 ?a) 5 ?x)))
     (neovm--leaf-try (lambda () (neovm--leaf-aset (make-string 2 ?a) -1 ?x)))
     (neovm--leaf-try (lambda () (neovm--leaf-aset (make-string 2 ?a) 0 'x)))
     (neovm--leaf-try (lambda () (neovm--leaf-aset (make-string 2 ?a) 0 -1)))
     (neovm--leaf-try (lambda () (neovm--leaf-aset (make-string 2 ?a) 0 #x400000))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((122 255 255 \"zaa�\" nil) (113 113 \"aqaa\" t) (946 90 \"Zβc\") (255 89 \"aYc\") ((error (error \"Attempt to store non-byte value into unibyte string\")) \"aa\" nil) ((error (error \"Attempt to store non-ASCII char into multibyte string\")) \"aa\" t) (error (args-out-of-range \"abc\" 5)) (error (args-out-of-range \"abc\" -1)) (error (wrong-type-argument fixnump x)) (error (args-out-of-range \"\" 0)) (error (args-out-of-range \"aa\" 5)) (error (args-out-of-range \"aa\" -1)) (error (wrong-type-argument characterp x)) (error (wrong-type-argument characterp -1)) (error (wrong-type-argument characterp 4194304)))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, LEAF_ENV, expect);
}

/// Redefinition (GNU `Bcall` reads the function cell every call): advice on
/// `plist-get` reaches a compiled Bcall site, advice on `nth` does not reach
/// compiled `Bnth`, an `fset` of `gethash` takes effect at the next call,
/// and restoring the definitions restores the answers.
#[test]
fn oracle_jit_leaf_redefinition_and_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--leaf-pg (byte-compile (lambda (p k) (plist-get p k))))
  (defalias 'neovm--leaf-gh (byte-compile (lambda (k h) (gethash k h))))
  (defalias 'neovm--leaf-nth (byte-compile (lambda (n l) (nth n l))))
  (let ((h (make-hash-table)))
    (puthash 1 'one h)
    (dotimes (_ 60) (neovm--leaf-pg '(a 1) 'a) (neovm--leaf-gh 1 h) (neovm--leaf-nth 0 '(x)))
    (let ((before (list (neovm--leaf-pg '(a 1) 'a) (neovm--leaf-gh 1 h) (neovm--leaf-nth 0 '(x))))
          (wrap (lambda (f &rest args) (list 'advised (apply f args)))))
      (advice-add 'plist-get :around wrap '((name . neovm--leaf-adv)))
      (advice-add 'nth :around wrap '((name . neovm--leaf-adv)))
      (let ((during (list (neovm--leaf-pg '(a 1) 'a) (neovm--leaf-nth 0 '(x)))))
        (advice-remove 'plist-get 'neovm--leaf-adv)
        (advice-remove 'nth 'neovm--leaf-adv)
        (let ((orig (symbol-function 'gethash)))
          (unwind-protect
              (progn
                (fset 'gethash (lambda (_k _h &optional _d) 'redefined))
                (let ((redefined (neovm--leaf-gh 1 h)))
                  (fset 'gethash orig)
                  (list before during redefined
                        (list (neovm--leaf-pg '(a 1) 'a) (neovm--leaf-gh 1 h)
                              (neovm--leaf-nth 0 '(x))))))
            (fset 'gethash orig)))))))"#;
    let expect = expect_test::expect![[r#""OK ((1 one x) ((advised 1) x) redefined (1 one x))""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, LEAF_ENV, expect);
}
