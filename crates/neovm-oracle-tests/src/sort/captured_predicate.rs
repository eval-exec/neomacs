//! GNU `sort.c:resolve_fun` predicate capture, including builtin aliases.
//! Refresh expectations from GNU only with
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const BOTH: &[&[(&str, &str)]] = &[&[("NEOVM_JIT", "0")], &[]];
const CAPTURED: &[&[(&str, &str)]] = BOTH;

#[test]
fn oracle_sort_capture_aliases_and_string_representations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((alias (make-symbol "cl1-sort-alias"))
      (outer (make-symbol "cl1-sort-outer")))
  (fset alias #'string<)
  (fset outer alias)
  (list
   (sort (list "c" "a" "b") #'string<)
   (sort (list "c" "a" "b") #'string-lessp)
   (sort (list "c" "a" "b") (indirect-function #'string<))
   (sort (list 'c 'a 'b) outer)
   (sort (list "é" "a" "λ" (unibyte-string 255)
               (string-to-multibyte (unibyte-string 128))) outer)
   (sort (list "éx" "éa" "é" "éx") #'string<)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\") (\"a\" \"b\" \"c\") (\"a\" \"b\" \"c\") (a b c) (\"a\" \"é\" \"�\" \"λ\" \"\\200\") (\"é\" \"éa\" \"éx\" \"éx\"))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_sort_capture_errors_and_uncalled_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((autoloaded (make-symbol "cl1-sort-autoload")))
  (fset autoloaded '(autoload "cl1-sort-no-such-file" nil nil))
  (list
   (sort nil #'cl1-sort-void)
   (sort (list "only") #'cl1-sort-void)
   (sort (list "only") autoloaded)
   (sort (list "only") 42)
   (condition-case e (sort (list "b" "a") #'cl1-sort-void) (error e))
   (condition-case e (sort (list "b" "a") 42) (error e))
   (condition-case e (sort (list "a" 3) #'string<) (error e))
   (condition-case e (sort (list "a" 3) #'string-lessp) (error e))
   (let ((xs (list "c" "a" "b")))
     (list (condition-case e
               (sort xs (lambda (_a _b) (error "cl1-sort-predicate-error")))
             (error e))
           xs))))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil (\"only\") (\"only\") (\"only\") (void-function cl1-sort-void) (invalid-function 42) (wrong-type-argument stringp 3) (wrong-type-argument stringp 3) ((error \"cl1-sort-predicate-error\") (\"c\" \"a\" \"b\")))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_sort_capture_stability_copy_and_list_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let* ((xs (list '("b" . b1) '("a" . a1) '("b" . b2) '("a" . a2)))
       (tail (cdr xs))
       (copy (sort xs :key #'car :lessp #'string<))
       (reverse (sort xs :key #'car :lessp #'string< :reverse t))
       (original (copy-sequence xs))
       (sorted (sort xs (lambda (a b) (string< (car a) (car b))))))
  (list copy reverse original xs
        (eq xs sorted) (eq tail (cdr xs))
        (let* ((vec (vector "c" "a" "b"))
               (copied (sort vec :lessp #'string<))
               (mutated (sort vec #'string<)))
          (list copied vec (eq copied vec) (eq mutated vec)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" . a1) (\"a\" . a2) (\"b\" . b1) (\"b\" . b2)) ((\"b\" . b1) (\"b\" . b2) (\"a\" . a1) (\"a\" . a2)) ((\"b\" . b1) (\"a\" . a1) (\"b\" . b2) (\"a\" . a2)) ((\"a\" . a1) (\"a\" . a2) (\"b\" . b1) (\"b\" . b2)) t t ([\"a\" \"b\" \"c\"] [\"a\" \"b\" \"c\"] nil t))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_sort_capture_redefinition_and_advice_before_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar cl1-sort-advice-calls 0)
  (let ((saved (symbol-function 'string<))
        (cl1-sort-advice-calls 0)
        (advice (lambda (original a b)
                  (setq cl1-sort-advice-calls (1+ cl1-sort-advice-calls))
                  (funcall original a b))))
    (unwind-protect
        (list
         (progn (fset 'string< (lambda (a b) (string-lessp b a)))
                (sort (list "d" "b" "c" "a") #'string<))
         (progn (fset 'string< saved)
                (advice-add 'string< :around advice)
                (list (sort (list "d" "b" "c" "a") #'string<)
                      cl1-sort-advice-calls)))
      (advice-remove 'string< advice)
      (fset 'string< saved))))
"#;
    let expect =
        expect_test::expect![[r#""OK ((\"d\" \"c\" \"b\" \"a\") ((\"a\" \"b\" \"c\" \"d\") 6))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, BOTH, expect);
}

#[test]
fn oracle_sort_capture_predicate_before_keys_and_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar cl1-sort-predicate nil)
  (defvar cl1-sort-calls 0)
  (let ((cl1-sort-predicate (make-symbol "cl1-sort-before-key"))
        (cl1-sort-calls 0))
    (fset cl1-sort-predicate
          (lambda (a b) (setq cl1-sort-calls (1+ cl1-sort-calls)) (< a b)))
    (list
     (sort (list 4 2 3 1) :lessp cl1-sort-predicate
           :key (lambda (x)
                  (fset cl1-sort-predicate (lambda (a b) (> a b)))
                  (garbage-collect)
                  x))
     cl1-sort-calls
     (let ((alias (make-symbol "cl1-sort-builtin-key")))
       (fset alias #'string<)
       (sort (list "d" "b" "c" "a") :lessp alias
             :key (lambda (x)
                    (fset alias (lambda (a b) (string-lessp b a)))
                    (garbage-collect)
                    x))))))
"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) 6 (\"a\" \"b\" \"c\" \"d\"))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, CAPTURED, expect);
}

#[test]
fn oracle_sort_capture_redefinition_during_comparisons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar cl1-sort-predicate nil)
  (defvar cl1-sort-old-calls 0)
  (defvar cl1-sort-new-calls 0)
  (let ((cl1-sort-predicate (make-symbol "cl1-sort-during-comparison"))
        (cl1-sort-old-calls 0)
        (cl1-sort-new-calls 0))
    (fset cl1-sort-predicate
          (lambda (a b)
            (setq cl1-sort-old-calls (1+ cl1-sort-old-calls))
            (fset cl1-sort-predicate
                  (lambda (a b)
                    (setq cl1-sort-new-calls (1+ cl1-sort-new-calls))
                    (> a b)))
            (garbage-collect)
            (< a b)))
    (list (sort (list 4 2 3 1) cl1-sort-predicate)
          cl1-sort-old-calls cl1-sort-new-calls)))
"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) 6 0)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, CAPTURED, expect);
}

#[test]
fn oracle_sort_capture_advice_removed_during_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar cl1-sort-advice-calls 0)
  (let ((cl1-sort-advice-calls 0)
        (advice (lambda (original a b)
                  (setq cl1-sort-advice-calls (1+ cl1-sort-advice-calls))
                  (funcall original a b))))
    (defalias 'cl1-sort-advised #'string<)
    (unwind-protect
        (progn
          (advice-add 'cl1-sort-advised :around advice)
          (list (sort (list "d" "b" "c" "a") :lessp #'cl1-sort-advised
                      :key (lambda (x) (advice-remove 'cl1-sort-advised advice) x))
                cl1-sort-advice-calls))
      (advice-remove 'cl1-sort-advised advice)
      (fmakunbound 'cl1-sort-advised))))
"#;
    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\" \"d\") 6)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, CAPTURED, expect);
}

#[test]
fn oracle_sort_capture_debugger_redefinition_in_funcall_prologue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar cl1-sort-predicate nil)
  (defvar cl1-sort-key-count 0)
  (defvar cl1-sort-debug-count 0)
  (let ((cl1-sort-predicate (make-symbol "cl1-sort-debugger"))
        (cl1-sort-key-count 0)
        (cl1-sort-debug-count 0)
        (debug-on-next-call nil)
        (debugger (lambda (&rest _ignored)
                    (setq debug-on-next-call nil)
                    (setq cl1-sort-debug-count (1+ cl1-sort-debug-count))
                    (fset cl1-sort-predicate (lambda (a b) (> a b)))
                    nil)))
    (fset cl1-sort-predicate #'<)
    (list (sort (list 4 2 3 1) :lessp cl1-sort-predicate
                :key (lambda (x)
                       (setq cl1-sort-key-count (1+ cl1-sort-key-count))
                       (if (= cl1-sort-key-count 4) (setq debug-on-next-call t))
                       x))
          cl1-sort-debug-count)))
"#;
    let expect = expect_test::expect![[r#""OK ((1 4 2 3) 2)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, CAPTURED, expect);
}
