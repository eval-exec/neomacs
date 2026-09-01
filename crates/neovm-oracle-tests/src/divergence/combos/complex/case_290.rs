//! Complex combo batch 290 — `symbol` / `obarray` / `intern` deep:
//! `intern-soft`, `unintern`, `mapatoms`, `symbol-plist`, `get`/`put`/
//! `remprop`, `fboundp`/`fmakunbound`/`defalias`/`fset` with chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx290_obarray_intern_soft_unintern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (make-obarray 31)))
  (intern "alpha" ob)
  (intern "beta" ob)
  (intern "gamma" ob)
  (let ((before (hash-table-count ob)))
    (unintern "beta" ob)
    (let ((after-unintern (hash-table-count ob)))
      (intern "delta" ob)
      (list before after-unintern (hash-table-count ob)
            (intern-soft "alpha" ob)
            (intern-soft "beta" ob)
            (intern-soft "delta" ob)))))
"##,
        expect,
    )
}

#[test]
fn div_cx290_mapatoms_collect_filtered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"neo-cx290-sym-alpha\" \"neo-cx290-sym-beta\" \"neo-cx290-sym-gamma\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(intern "neo-cx290-sym-alpha")
(intern "neo-cx290-sym-beta")
(intern "neo-cx290-sym-gamma")
(let (collected)
  (mapatoms (lambda (s)
              (when (string-prefix-p "neo-cx290-sym-" (symbol-name s))
                (push s collected))))
  (sort (mapcar #'symbol-name collected) #'string<))
"##,
        expect,
    )
}

#[test]
fn div_cx290_symbol_plist_get_put_remprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments get 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sym (intern "neo-cx290-plist-test")))
  (put sym 'var-doc "documentation")
  (put sym 'custom-type 'string)
  (put sym 'neo-cx290-custom :val)
  (list (get sym 'var-doc)
        (get sym 'custom-type)
        (get sym 'neo-cx290-custom)
        (get sym 'missing :default)
        (symbol-plist sym)
        (plist-member (symbol-plist sym) 'custom-type)
        (remprop sym 'neo-cx290-custom)
        (get sym 'neo-cx290-custom)))
"##,
        expect,
    )
}

#[test]
fn div_cx290_fboundp_fmakunbound_defalias_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t t (closure (t) nil :orig) :orig neo-cx290-alias1 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defalias 'neo-cx290-orig (lambda () :orig))
(defalias 'neo-cx290-alias1 'neo-cx290-orig)
(defalias 'neo-cx290-alias2 'neo-cx290-alias1)
(list (fboundp 'neo-cx290-orig)
      (fboundp 'neo-cx290-alias1)
      (fboundp 'neo-cx290-alias2)
      (indirect-function 'neo-cx290-alias2)
      (funcall 'neo-cx290-alias2)
      (fmakunbound 'neo-cx290-alias1)
      (fboundp 'neo-cx290-alias1)
      (indirect-function 'neo-cx290-alias2))
"##,
        expect,
    )
}

#[test]
fn div_cx290_symbol_function_vs_indirect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((closure (t) nil :fn1) (closure (t) nil :fn1) neo-cx290-a (closure (t) nil :fn1) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fn1 (lambda () :fn1))
      (fn2 (lambda () :fn2)))
  (defalias 'neo-cx290-a fn1)
  (defalias 'neo-cx290-b 'neo-cx290-a)
  (list (symbol-function 'neo-cx290-a)
        (indirect-function 'neo-cx290-a)
        (symbol-function 'neo-cx290-b)
        (indirect-function 'neo-cx290-b)
        (eq (indirect-function 'neo-cx290-a) (indirect-function 'neo-cx290-b))))
"##,
        expect,
    )
}

#[test]
fn div_cx290_boundp_makunbound_void_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 42 neo-cx290-bound-var nil :void)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defvar neo-cx290-bound-var 42)
(list (boundp 'neo-cx290-bound-var)
      neo-cx290-bound-var
      (makunbound 'neo-cx290-bound-var)
      (boundp 'neo-cx290-bound-var)
      (condition-case e neo-cx290-bound-var (void-variable :void)))
"##,
        expect,
    )
}

#[test]
fn div_cx290_function_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"docstring\" nil t \"Return the car of LIST.  If LIST is nil, return nil.\\nError if LIST is not nil and not a cons cell.  See also ‘car-safe’.\\n\\nSee Info node ‘(elisp)Cons Cells’ for a discussion of related basic\\nLisp concepts such as car, cdr, cons cell and list.\\n\\n(fn LIST)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx290-fn-doc () "docstring" :result)
(list (documentation 'neo-cx290-fn-doc)
      (function-get 'neo-cx290-fn-doc 'func-documentation)
      (fboundp 'car)
      (documentation 'car))
"##,
        expect,
    )
}

#[test]
fn div_cx290_obarray_hash_table_internals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (make-obarray 31)))
  (dotimes (i 20)
    (intern (format "sym-%d" i) ob))
  (list (obarrayp ob)
        (hash-table-p ob)
        (hash-table-count ob)
        (> (hash-table-count ob) 0)
        (intern-soft "sym-0" ob)
        (intern-soft "sym-19" ob)
        (intern-soft "sym-20" ob)))
"##,
        expect,
    )
}

#[test]
fn div_cx290_default_boundp_setq_default_local_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t :global :local t :new-default :local :new-default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx290-default :global)
(let ((buf (get-buffer-create " *neo-cx290-dv*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx290-default) :local))
  (list (default-boundp 'neo-cx290-default)
        (default-value 'neo-cx290-default)
        (buffer-local-value 'neo-cx290-default buf)
        (local-variable-p 'neo-cx290-default buf)
        (setq-default neo-cx290-default :new-default)
        (buffer-local-value 'neo-cx290-default buf)
        (default-value 'neo-cx290-default)))
"##,
        expect,
    )
}

#[test]
fn div_cx290_symbol_obarray_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (make-obarray 31))
      (ht (make-hash-table :test 'equal)))
  (intern "neo-cx290-mega-alpha" ob)
  (intern "neo-cx290-mega-beta" ob)
  (puthash "key1" :val1 ht)
  (puthash "key2" :val2 ht)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Symbol/obarray mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (hash-table-count ob)
                         (intern-soft "neo-cx290-mega-alpha" ob)
                         (hash-table-count ht)
                         (gethash "key1" ht)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
