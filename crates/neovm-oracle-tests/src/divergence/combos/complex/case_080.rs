//! Complex combo batch 80 — macro / quote / backquote / pcase deep:
//! macroexpand-1, gensym, defmacro with `&body`, `pcase` patterns with
//! guards and `or`/`and` patterns, rx construction, and `pcase-let`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx80_backquote_complex_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((simple 10) (a b c 10) ((nested 10) a b c (deeply (10))) (2 3 4) (:big :ok))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 10)
      (lst '(a b c)))
  (list `(simple ,x)
        `(,@lst ,x)
        `((nested ,x) ,@lst (deeply (,x)))
        `(,@(mapcar #'1+ '(1 2 3)))
        `(,(if (> x 5) :big :small) ,@(if (> x 5) '(:ok) '(:no)))))
"##,
        expect,
    );
}

#[test]
fn div_cx80_macroexpand_vs_macroexpand_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((list :wrapped (+ 1 2)) (list :wrapped (neo-cx80-wrap (+ 1 2))) (neo-cx80-wrap (neo-cx80-wrap (+ 1 2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx80-wrap (form)
  `(list :wrapped ,form))
(defmacro neo-cx80-double-wrap (form)
  `(neo-cx80-wrap (neo-cx80-wrap ,form)))
(let ((simple (macroexpand '(neo-cx80-wrap (+ 1 2)))))
  (list simple
        (macroexpand '(neo-cx80-double-wrap (+ 1 2)))
        (macroexpand-1 '(neo-cx80-double-wrap (+ 1 2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx80_defmacro_with_body_and_rest_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-dolist)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx80-when* (cond &rest body)
  (declare (indent 1))
  `(if ,cond (progn ,@body) nil))
(defmacro neo-cx80-dolist-mac (var list &rest body)
  `(cl-dolist (,var ,list) ,@body))
(list (macroexpand '(neo-cx80-when* (> x 5) (incf x) (message "hi")))
      (macroexpand '(neo-cx80-dolist-mac x '(1 2 3) (print x)))
      (eval '(neo-cx80-when* t 42) t)
      (eval '(let ((acc 0)) (neo-cx80-dolist-mac x '(1 2 3) (cl-incf acc x)) acc) t))
"##,
        expect,
    );
}

#[test]
fn div_cx80_gensym_uniqueness_in_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (g0 g1 prefix-2 t nil nil \"prefix-2\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 (gensym))
       (s2 (gensym))
       (s3 (gensym "prefix-")))
  (list s1 s2 s3
        (symbolp s1)
        (eq s1 s2)
        (eq s1 s3)
        (symbol-name s3)
        (string-prefix-p "prefix-" (symbol-name s3))))
"##,
        expect,
    );
}

#[test]
fn div_cx80_pcase_basic_with_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:int 42) (:str \"hello\") (:list (1 2 3)) (:vec [1 2 3]) :unknown)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((pred integerp) (list :int v))
            ((pred stringp) (list :str v))
            ((pred listp) (list :list v))
            ((pred vectorp) (list :vec v))
            (_ :unknown)))
        '(42 "hello" (1 2 3) [1 2 3] symbol-key))
"##,
        expect,
    );
}

#[test]
fn div_cx80_pcase_app_and_quote_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:start 1) (:mid 1 2) (:end (1 2 3)) :all :has-first-car :no-match)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            (`(start ,x) (list :start x))
            (`(mid ,x ,y) (list :mid x y))
            (`(end . ,rest) (list :end rest))
            ('all :all)
            ((app car-safe 'first) :has-first-car)
            (_ :no-match)))
        '((start 1) (mid 1 2) (end 1 2 3) all (first . rest) (other)))
"##,
        expect,
    );
}

#[test]
fn div_cx80_pcase_with_or_and_and_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:yes :yes :yes :no :positive-int :no :no (:string-of-len 5) (:string-of-len 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((or 'yes 'y 'true) :yes)
            ((and (pred integerp) (pred (> _ 0))) :positive-int)
            ((and (pred stringp) s) (list :string-of-len (length s)))
            (_ :no)))
        '(yes y true true-symbol 42 -5 0 "hello" "x"))
"##,
        expect,
    );
}

#[test]
fn div_cx80_pcase_let_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (name \"alpha\" value 42 ((name . \"alpha\") (value . 42) (tags a b c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '((name . "alpha") (value . 42) (tags . (a b c)))))
  (pcase-let ((`(,(and name-sym 'name) . ,name-val) (car data))
              (`(,(and value-sym 'value) . ,value-val) (cadr data)))
    (list name-sym name-val value-sym value-val data)))
"##,
        expect,
    );
}

#[test]
fn div_cx80_pcase_lambda_and_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-function (pred integerp))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((classifier (pcase-lambda (x)
                     ((pred integerp) :int)
                     ((pred stringp) :str)
                     (`(,a . ,_) :cons)
                     (_ :other))))
  (list (funcall classifier 42)
        (funcall classifier "hi")
        (funcall classifier '(1 2 3))
        (funcall classifier [1 2])))
"##,
        expect,
    );
}

#[test]
fn div_cx80_rx_regexp_macros_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function rx-or)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((var-name 'identifier)
       (pat (rx-to-string
             `(seq symbol-start
                   (group (+ (or (any "a-zA-Z_") (any "0-9"))))
                   symbol-end))))
  (list pat
        (string-match pat "_valid_123_var")
        (match-string 0 "_valid_123_var")
        (string-match pat "9invalid")
        (rx-or (rx "alpha") (rx "beta"))
        (rx-let-eval ((kw (name) `(seq ,name ":")))
          (rx-to-string `(kw "key")))))
"##,
        expect,
    );
}

#[test]
fn div_cx80_defmacro_recursive_lexical_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx80-my-while (cond &rest body)
  `(if ,cond
       (progn ,@body (neo-cx80-my-while ,cond ,@body))
     nil))
(let ((x 0)
      (count 0))
  (neo-cx80-my-while (< x 5)
    (cl-incf x)
    (cl-incf count))
  (list x count))
"##,
        expect,
    );
}

#[test]
fn div_cx80_declared_macro_with_debug_and_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (list 1 2) (:a :b))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx80-with-decl (x &optional y)
  (declare (indent 1) (debug t))
  `(list ,x ,y))
(list (macrop 'neo-cx80-with-decl)
      (macroexpand '(neo-cx80-with-decl 1 2))
      (eval '(neo-cx80-with-decl :a :b) t))
"##,
        expect,
    );
}

#[test]
fn div_cx80_pcase_macro_pcase_dolist_with_marker_overlay_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 . a) (2 . b) (3 . c)) 4 3 8 #(\"0123456789\" 0 4 (face bold)) (face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 4))
        (ov (make-overlay 3 8)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (let (acc)
      (pcase-dolist (`(,k . ,v) '((1 . a) (2 . b) (3 . c)))
        (push (cons k v) acc))
      (list (nreverse acc)
            (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (buffer-string)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
