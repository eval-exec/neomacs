//! GNU Fassoc calls TESTFN afresh on every entry (fns.c), unlike sort's
//! captured predicate. Refresh GNU expectations with
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const BOTH: &[&[(&str, &str)]] = &[
    &[("NEOVM_ASSOC_RESOLVED", "off")],
    &[("NEOVM_ASSOC_RESOLVED", "on")],
];

#[test]
fn oracle_assoc_callbacks_eq_and_eql_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((key (copy-sequence "s"))
                  (alist (list '(ignored . 0) 'atom (cons (copy-sequence "s") 1) (cons key 2)))
                  (big (+ (expt 2 80) 3)))
             (list
              (cdr (assoc key alist #'eq))
              (alist-get key alist 'missing nil #'eq)
              (assoc-default key alist #'eq 'atom)
              (member (copy-sequence "s") (list "no" key))
              (cdr (assoc 0.0 '((-0.0 . minus) (0.0 . plus)) #'eql))
              (alist-get 0.0 '((-0.0 . minus) (0.0 . plus)) nil nil #'eql)
              (assoc-default 0.0 '((-0.0 . minus) (0.0 . plus)) #'eql)
              (cdr (assoc (+ (expt 2 80) 3) (list (cons big 'large)) #'eql))
              (cdr (assoc 0.0e+NaN '((1 . no) (0.0e+NaN . nan)) #'eql))
              (assoc 'absent alist #'eq)))"#;
    let expect = expect_test::expect![[r#""OK (2 2 2 (\"s\") plus plus plus large nan nil)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_equal_and_properties_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((key (propertize "s" 'face 'bold))
                  (wrong (propertize "s" 'face 'italic))
                  (right (propertize "s" 'face 'bold))
                  (alist (list 'skip (cons wrong 'first) (cons right 'second))))
             (list
              (cdr (assoc key alist #'equal))
              (alist-get key alist nil nil #'equal)
              (assoc-default key alist #'equal)
              (cdr (assoc key alist #'equal-including-properties))
              (alist-get key alist nil nil #'equal-including-properties)
              (assoc-default key alist #'equal-including-properties)
              (cdr (assoc (list key [1 2])
                          (list (cons (list wrong [1 2]) 'wrong)
                                (cons (list right [1 2]) 'right))
                          #'equal-including-properties))
              (cdr (assoc [1 2] '(([0 1] . no) ([1 2] . yes)) #'equal))
              (length (member key (list "no" wrong right)))
              (length (member [1 2] '([0 1] [1 2] [2 3])))))"#;
    let expect =
        expect_test::expect![[r#""OK (first first first second second second right yes 2 2)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_string_equal_and_aliases_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
             (defalias 'assoc-callback-alias-1 'string=)
             (defalias 'assoc-callback-alias-2 'assoc-callback-alias-1)
             (defalias 'assoc-callback-equal-alias 'equal-including-properties)
             (let ((alist '((no . 1) (yes . 2)))
                   (unicode (list (cons (string-to-unibyte "\351") 'unibyte)
                                  (cons "é" 'multibyte))))
               (list
                (cdr (assoc "yes" alist #'string=))
                (alist-get "yes" alist nil nil #'string=)
                (assoc-default "yes" alist #'string=)
                (cdr (assoc "yes" alist #'assoc-callback-alias-2))
                (cdr (assoc "yes" alist (symbol-function 'string-equal)))
                (cdr (assoc "é" unicode #'string=))
                (cdr (assoc (string-to-unibyte "\351") unicode #'string=))
                (cdr (assoc (propertize "s" 'x 1)
                            (list (cons (propertize "s" 'x 2) 'wrong)
                                  (cons (propertize "s" 'x 1) 'right))
                            #'assoc-callback-equal-alias)))))"#;
    let expect = expect_test::expect![[r#""OK (2 2 2 2 2 multibyte unibyte right)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_redefinition_and_argument_order_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let (seen)
             (fset 'assoc-callback-changing
                   (lambda (entry key)
                     (push (list 'old entry key) seen)
                     (fset 'assoc-callback-changing
                           (lambda (entry key)
                             (push (list 'new entry key) seen)
                             (eq entry 'second)))
                     nil))
             (let ((answer (assoc 'lookup '((first . 1) (second . 2) (third . 3))
                                  #'assoc-callback-changing)))
               (list answer (nreverse seen)
                     (let ((saved (symbol-function 'funcall)))
                       (unwind-protect
                           (let* ((a (lambda (_)
                                       (fset 'funcall (lambda (_entry _key) t)) nil))
                                  (b (lambda (_) nil))
                                  (alist (list (cons a 'first) (cons b 'second))))
                             (cdr (assoc 'key alist #'funcall)))
                         (fset 'funcall saved)))
                     (let ((saved (symbol-function 'string-equal)))
                       (unwind-protect
                           (progn
                             (fset 'string-equal (lambda (_a _b) t))
                             (cdr (assoc "different" '(("first" . custom)) #'string=)))
                         (fset 'string-equal saved))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((second . 2) ((old first lookup) (new second lookup)) second custom)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_errors_and_mutation_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
             (assoc 1 nil 'assoc-callback-undefined)
             (assoc 1 '(atom nil 42) 'assoc-callback-undefined)
             (condition-case e (assoc 1 '((1 . 2)) 'assoc-callback-undefined) (error e))
             (condition-case e (assoc 1 '((1 . 2)) #'car) (error e))
             (condition-case e (assoc "s" '((17 . no)) #'string=) (error e))
             (condition-case e (alist-get "s" '((17 . no)) nil nil #'string=) (error e))
             (condition-case e (assoc-default "s" '((17 . no)) #'string=) (error e))
             (condition-case e (assoc 'key '((a . 1) (b . 2))
                                      (lambda (entry key) (error "predicate %s %s" entry key)))
               (error e))
             (let* ((alist (list (cons 'a 1) (cons 'b 2) (cons 'c 3)))
                    (head alist)
                    (calls 0))
               (list (assoc 'key alist
                            (lambda (entry _key)
                              (setq calls (1+ calls))
                              (when (eq entry 'a)
                                (setq alist nil)
                                (setcdr head (list (cons 'd 4))))
                              (garbage-collect)
                              (eq entry 'd)))
                     calls))
             (catch 'assoc-done
               (assoc 'key '((a . 1) (b . 2))
                      (lambda (entry _key) (throw 'assoc-done entry)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil (void-function assoc-callback-undefined) (wrong-number-of-arguments #<subr car> 2) (wrong-type-argument stringp 17) (wrong-type-argument stringp 17) (wrong-type-argument stringp 17) (error \"predicate a key\") ((d . 4) 2) a)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_deep_equal_errors_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((left 1) (right 1))
                  (dotimes (_ 300) (setq left (list left) right (list right)))
                  (list
                   (condition-case e (assoc right (list (cons left 'deep)) #'equal)
                     (error e))
                   (condition-case e
                       (assoc right (list (cons left 'deep)) #'equal-including-properties)
                     (error e))
                   (assoc 'after '((after . restored)) #'eq)))"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"Stack overflow in equal\") (error \"Stack overflow in equal\") (after . restored))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_debugger_redefinition_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(eval '(let ((log nil) (saved (symbol-function 'funcall)))
                         (unwind-protect
                             (let ((debugger
                                    (lambda (&rest args)
                                      (push (car args) log)
                                      (if (eq (car args) 'exit)
                                          (cadr args)
                                        (fset 'funcall (lambda (_entry _key) t))
                                        nil))))
                               (let ((answer
                                      (assoc 'key
                                             (list
                                              (cons (lambda (_) (setq debug-on-next-call t) nil) 'first)
                                              (cons (lambda (_) nil) 'second)
                                              (cons (lambda (_) nil) 'third))
                                             #'funcall)))
                                 (list (cdr answer) log)))
                           (fset 'funcall saved))) t)"#;
    let expect = expect_test::expect![[r#""OK (second (exit lambda))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_position_symbols_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((key (position-symbol 'needle 7))
                         (other (position-symbol 'needle 9))
                         (alist (list (cons other 'other) (cons 'needle 'bare)))
                         (symbols-with-pos-enabled t))
                    (list
                     (cdr (assoc key alist #'eq))
                     (cdr (assoc key alist #'eql))
                     (cdr (assoc key alist #'equal))
                     (cdr (assoc key alist #'equal-including-properties))
                     (cdr (assoc key alist (position-symbol 'eq 11)))
                     (alist-get key alist nil nil #'equal-including-properties)
                     (let ((symbols-with-pos-enabled nil))
                       (list
                        (assoc key alist #'eq)
                        (assoc key alist #'eql)
                        (assoc key alist #'equal)
                        (assoc key alist #'equal-including-properties)))))"#;
    let expect =
        expect_test::expect![[r#""OK (other other other other other other (nil nil nil nil))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_assoc_callbacks_named_bytecode_and_closures_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
                   (require 'bytecomp)
                   (defalias 'assoc-callback-compiled
                     (byte-compile (lambda (entry key) (eq entry key))))
                   (defalias 'assoc-callback-compiled-alias 'assoc-callback-compiled)
                   (let ((alist '((first . 1) (second . 2) (third . 3))))
                     (dotimes (_ 100) (assoc 'second alist #'assoc-callback-compiled))
                     (list
                      (assoc 'second alist #'assoc-callback-compiled)
                      (alist-get 'second alist nil nil #'assoc-callback-compiled-alias)
                      (assoc 'second alist (symbol-function 'assoc-callback-compiled))
                      (assoc 'second alist '(lambda (entry key) (eq entry key)))
                      (let ((seen nil))
                        (defalias 'assoc-callback-frames
                          (byte-compile
                           (lambda (entry key)
                             (mapbacktrace
                              (lambda (_evaluated function args _flags)
                                (when (eq function 'assoc-callback-frames)
                                  (push (list function args) seen))))
                             (eq entry key))))
                        (let ((answer (assoc 'second alist #'assoc-callback-frames)))
                          (list answer (nreverse seen)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((second . 2) 2 (second . 2) (second . 2) ((second . 2) nil))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}
