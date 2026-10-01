//! Function-cell changes while compiled callers are live, including the
//! per-symbol resync case. GNU's Bcall records the called symbol before
//! resolving its current definition (src/bytecode.c:792-796). Ffset and
//! Ffmakunbound write that cell synchronously (src/data.c), and mapcar1
//! calls a symbol again for every element (src/fns.c).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const JIT_ENV: &[(&str, &str)] = &[("NEOVM_JIT_THRESHOLD", "1")];

#[test]
fn oracle_per_symbol_unrelated_fset_preserves_result_and_call_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--sr-f
    (byte-compile
     (lambda (x)
       (if (eq x 'show)
           (list (backtrace-frame 0 'neovm--sr-f)
                 (backtrace-frame 1 'neovm--sr-f))
         (1+ x)))))
  (defalias 'neovm--sr-unrelated
    (byte-compile
     (lambda (n)
       (let ((i 0) (sum 0))
         (while (< i n)
           (when (= (% i 10) 0)
             (fset 'neovm--sr-other (if (= (% i 20) 0) #'ignore #'identity)))
           (setq sum (+ sum (neovm--sr-f i)) i (1+ i)))
         (list sum (neovm--sr-f 'show))))))
  (neovm--sr-unrelated 2000))"#;
    let expect = expect_test::expect![[
        r#""OK (2001000 ((t neovm--sr-f show) (t neovm--sr-unrelated 2000)))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// GNU's gv setter compiles cl-letf's function-cell writes to Bfset, which
/// bypasses the fset function cell but must still invalidate g's call site.
#[test]
fn oracle_per_symbol_compiled_cl_letf_restores_only_its_callee() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (require 'cl-lib)
  (defalias 'neovm--sr-cl-f (byte-compile (lambda (x) (1+ x))))
  (defalias 'neovm--sr-cl-g (byte-compile (lambda (x) (* 2 x))))
  (defalias 'neovm--sr-cl-call
    (byte-compile
     (lambda (n)
       (let ((i 0) (sum 0))
         (while (< i n)
           (setq sum
                 (+ sum
                    (if (= (% i 10) 0)
                        (cl-letf (((symbol-function 'neovm--sr-cl-g)
                                   (lambda (x) (+ x 100))))
                          (+ (neovm--sr-cl-g i) (neovm--sr-cl-f i)))
                      (+ (neovm--sr-cl-g i) (neovm--sr-cl-f i)))
                    (neovm--sr-cl-g i))
                 i (1+ i)))
         sum))))
  (list (neovm--sr-cl-call 2000) (neovm--sr-cl-g 7)))"#;
    let expect = expect_test::expect![[r#""OK (9818000 14)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// The old activation finishes after either cell writer; the next call
/// from that same caller resolves the new definition.
#[test]
fn oracle_per_symbol_fset_and_defalias_inside_a_running_caller() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--sr-writer nil)
  (defvar neovm--sr-new (byte-compile (lambda (x) (+ x 10))))
  (defvar neovm--sr-old
    (byte-compile
     (lambda (x)
       (if (eq x 'change)
           (progn
             (if (eq neovm--sr-writer 'fset)
                 (fset 'neovm--sr-live-f neovm--sr-new)
               (defalias 'neovm--sr-live-f neovm--sr-new))
             (list 'old (backtrace-frame 0 'neovm--sr-live-f)
                   (backtrace-frame 1 'neovm--sr-live-f)))
         (1+ x)))))
  (fset 'neovm--sr-live-f neovm--sr-old)
  (defalias 'neovm--sr-live-call
    (byte-compile (lambda (x) (list (neovm--sr-live-f x) (neovm--sr-live-f 1)))))
  (dotimes (i 2000) (neovm--sr-live-call i))
  (list (let ((neovm--sr-writer 'fset)) (neovm--sr-live-call 'change))
        (progn
          (fset 'neovm--sr-live-f neovm--sr-old)
          (let ((neovm--sr-writer 'defalias)) (neovm--sr-live-call 'change)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((old (t neovm--sr-live-f change) (t neovm--sr-live-call change)) 11) ((old (t neovm--sr-live-f change) (t neovm--sr-live-call change)) 11))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// advice-add writes the function cell immediately; mapcar's following
/// elements must use the advice even though the first callback is live.
#[test]
fn oracle_per_symbol_advice_added_by_a_mapcar_callback_affects_later_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--sr-map-frames nil)
  (defalias 'neovm--sr-map-advice
    (byte-compile (lambda (x) (if (numberp x) (+ x 100) x))))
  (defalias 'neovm--sr-map-f
    (byte-compile
     (lambda (x)
       (when (eq x 'arm)
         (advice-add 'neovm--sr-map-f :filter-return #'neovm--sr-map-advice)
         (setq neovm--sr-map-frames
               (list (backtrace-frame 0 'neovm--sr-map-f)
                     (backtrace-frame 1 'neovm--sr-map-f))))
       x)))
  (dotimes (_ 2000) (mapcar #'neovm--sr-map-f '(1 2)))
  (list (mapcar #'neovm--sr-map-f '(arm 1 2 3)) neovm--sr-map-frames))"#;
    let expect = expect_test::expect![[
        r#""OK ((arm 101 102 103) ((t neovm--sr-map-f arm) (t mapcar neovm--sr-map-f (arm 1 2 3))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

#[test]
fn oracle_per_symbol_fmakunbound_mid_loop_signals_with_the_current_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--sr-unbound-frames nil)
  (defvar neovm--sr-unbound-index nil)
  (defalias 'neovm--sr-unbound-f
    (byte-compile
     (lambda (x)
       (when (= x 2000) (fmakunbound 'neovm--sr-unbound-f))
       (1+ x))))
  (defalias 'neovm--sr-unbound-call
    (byte-compile
     (lambda (n)
       (let ((i 0) (sum 0))
         (while (< i n)
           (setq neovm--sr-unbound-index i)
           (setq sum (+ sum (neovm--sr-unbound-f i)) i (1+ i)))
         sum))))
  (dotimes (_ 2000) (neovm--sr-unbound-f 0))
  (list
   (condition-case err
       (handler-bind
           ((void-function
             (lambda (_err)
               (setq neovm--sr-unbound-frames
                     (list (backtrace-frame 0 'neovm--sr-unbound-f)
                           (backtrace-frame 1 'neovm--sr-unbound-f))))))
         (neovm--sr-unbound-call 2002))
     (error (list err (error-message-string err))))
   neovm--sr-unbound-index neovm--sr-unbound-frames))"#;
    let expect = expect_test::expect![[
        r#""OK (((void-function neovm--sr-unbound-f) \"Symbol’s function definition is void: neovm--sr-unbound-f\") 2001 ((t neovm--sr-unbound-f 2001) (t neovm--sr-unbound-call 2002)))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// Fautoload replaces another autoload, and Fautoload_do_load resolves
/// the real cell again after loading the file (src/eval.c:2411,2494).
#[test]
fn oracle_per_symbol_autoload_replaces_a_warmed_callee() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--autoload-match-data-probe
    (byte-compile (lambda () 'warm)))
  (defalias 'neovm--sr-autoload-call
    (byte-compile (lambda () (neovm--autoload-match-data-probe))))
  (dotimes (_ 2000) (neovm--sr-autoload-call))
  (fmakunbound 'neovm--autoload-match-data-probe)
  (autoload 'neovm--autoload-match-data-probe "ignored-first-file")
  (autoload 'neovm--autoload-match-data-probe "autoload-match-data-probe")
  (list (autoloadp (symbol-function 'neovm--autoload-match-data-probe))
        (neovm--sr-autoload-call)
        (autoloadp (symbol-function 'neovm--autoload-match-data-probe))
        (neovm--sr-autoload-call)))"#;
    let expect = expect_test::expect![[r#""OK (t autoload-loaded nil autoload-loaded)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &neomacs_infra::crate_root!().join("fixtures/autoload"),
        expect,
    );
}
