//! The in-process O-suite: every program prints the same under the tree
//! walker and under Tier-I at threshold 1 (every body compiled at its first
//! call), in `verify` mode (a balance check after every compiled form) and in
//! `on` mode, and compiled code must actually have run.

use crate::emacs_core::eval::{Context, TierIEvent, TierIMode};
use crate::emacs_core::format_eval_result;
use crate::emacs_core::value::Value;

/// Evaluate every form of SRC in a fresh runtime context in MODE (lazy
/// leaf frames on); the formatted results and the context (for its
/// statistics).
fn run(mode: TierIMode, src: &str) -> (Vec<String>, Context) {
    run_with(mode, true, src)
}

fn run_with(mode: TierIMode, lazy_frames: bool, src: &str) -> (Vec<String>, Context) {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.tier_i.set_mode(mode);
    eval.tier_i.set_threshold(1);
    eval.tier_i.set_lazy_frames(lazy_frames);
    eval.tier_i.clear_for_test();
    let forms =
        crate::emacs_core::value_reader::read_all(src, &eval.obarray).expect("parse the program");
    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    let results = forms
        .into_iter()
        .map(|form| format_eval_result(&eval.eval_form(form)))
        .collect();
    eval.restore_specpdl_roots(roots);
    (results, eval)
}

/// SRC prints the same under every mode, and compiled code ran.
#[track_caller]
fn assert_same(src: &str) -> Context {
    crate::test_utils::init_test_tracing();
    let (off, _) = run(TierIMode::Off, src);
    if std::env::var("TIER_I_TEST_DUMP").is_ok() {
        tracing::info!(target: "neovm::tier_i", "tree walker results:\n{}", off.join("\n"));
    }
    let (verify, eval) = run(TierIMode::Verify, src);
    assert_eq!(
        off, verify,
        "verify differs from the tree walker for:\n{src}"
    );
    let (on, _) = run(TierIMode::On, src);
    assert_eq!(off, on, "on differs from the tree walker for:\n{src}");
    let (eager, _) = run_with(TierIMode::Verify, false, src);
    assert_eq!(
        off, eager,
        "eager frames differ from the tree walker for:\n{src}"
    );
    assert!(
        eval.tier_i.stats().count(TierIEvent::Run) > 0,
        "no compiled body ran: {}",
        eval.tier_i.stats().report()
    );
    eval
}

fn last(results: &[String]) -> &str {
    results.last().map(String::as_str).unwrap_or("")
}

#[test]
fn every_mirrored_special_form() {
    let eval = assert_same(
        r#"
(defvar ti-dyn 10)
(defun ti-sf (n)
  (let ((acc nil) (i 0))
    (while (< i n)
      (setq acc (cons (cond ((= i 0) 'zero)
                            ((and (> i 1) (< i 3)) (list 'two i))
                            ((or (= i 3) (= i 4)) (if (= i 3) 'three 'four 'ignored))
                            (t (let* ((a i) (b (* a 2)) (c (+ a b))) (list a b c))))
                      acc))
      (setq i (1+ i)))
    (list (nreverse acc)
          (prog1 (progn 1 2 3) 4)
          (catch 'done (dotimes (k 5) (when (= k 2) (throw 'done k))) 'never)
          (let ((x 0)) (unwind-protect (setq x 1) (setq x 2)) x)
          (condition-case e (car 1) (wrong-type-argument (list 'caught (car e))))
          (let ((ti-dyn 20)) (ti-sf-read-dyn))
          (save-excursion 'se)
          (save-restriction 'sr)
          (save-current-buffer 'scb)
          (and) (or) (progn) (cond) (setq))))
(defun ti-sf-read-dyn () ti-dyn)
(list (ti-sf 7) (ti-sf 7) (ti-sf 0))
"#,
    );
    assert!(eval.tier_i.stats().count(TierIEvent::Compiled) >= 2);
}

#[test]
fn lexical_slots_shadowing_and_closures() {
    assert_same(
        r#"
(defun ti-shadow (x)
  (let ((f (lambda () x)))
    (let ((x (1+ x)))
      (let* ((x (* x 10)) (g (lambda () x)))
        (setq x (1+ x))
        (list x (funcall f) (funcall g))))))
(defun ti-counter ()
  (let ((n 0))
    (list (lambda () (setq n (1+ n))) (lambda () n))))
(let* ((c (ti-counter)) (inc (car c)) (get (cadr c)))
  (funcall inc) (funcall inc)
  (list (ti-shadow 1) (ti-shadow 2) (funcall get)
        (prin1-to-string inc)))
(defun ti-dup (a) (let ((a (+ a 1)) (a (+ a 2))) a))
(list (ti-dup 1) (ti-dup 5))
(defun ti-optrest (a &optional b &rest c) (list a b c (let ((b (or b 'none))) b)))
(list (ti-optrest 1) (ti-optrest 1 2) (ti-optrest 1 2 3 4))
"#,
    );
}

#[test]
fn dynamic_binding_quirks() {
    assert_same(
        r#"
(defvar ti-special 'global)
(defun ti-q1 ()
  ;; A dynamic let of a special variable, read by a callee.
  (let ((ti-special 'let)) (ti-q-read)))
(defun ti-q-read () ti-special)
(defun ti-q2 (y)
  ;; `defvar' with no value inside the body: later lets are dynamic.
  (let ((r1 (let ((ti-local-dyn 1)) (ti-q-read-local))))
    (defvar ti-local-dyn)
    (let ((ti-local-dyn 2)) (list r1 (ti-q-read-local) y))))
(defun ti-q-read-local () (if (boundp 'ti-local-dyn) ti-local-dyn 'unbound))
(list (ti-q1) (ti-q1) (ti-q2 'a) (ti-q2 'b))
(defun ti-shadow-quirk (x)
  ;; x becomes special after the outer lexical binding: the dynamic inner
  ;; let does not shadow the lexical one.
  (let ((x2 x))
    (defvar x)
    (let ((x (* 100 x2))) (list x (symbol-value 'x)))))
(condition-case e (list (ti-shadow-quirk 1) (ti-shadow-quirk 2)) (error e))
"#,
    );
}

#[test]
fn environment_facts_free_variables_and_special_declarations() {
    assert_same(
        r#"
(defvar ti-free 1)
(defvar ti-watch-log nil)
(defun ti-incr () (setq ti-free (1+ ti-free)) ti-free)
(add-variable-watcher 'ti-free (lambda (sym new op where) (push (list sym new op) ti-watch-log)))
(list (ti-incr) (ti-incr) (reverse ti-watch-log))
(defun ti-read-mk-dyn () (if (boundp 'ti-mk-dyn) ti-mk-dyn 'unbound))
(defun ti-mk-marker ()
  (let ((a 1))
    (defvar ti-mk-dyn)
    (lambda (v) (let ((ti-mk-dyn v)) (list a (ti-read-mk-dyn))))))
(let ((f (ti-mk-marker))) (list (funcall f 1) (funcall f 2) (prin1-to-string f)))
(defmacro ti-declare-dyn (v) `(defvar ,v))
(defun ti-im-read () (if (boundp 'ti-im-var) ti-im-var 'unbound))
(let ((internal-make-interpreted-closure-function nil))
  (defun ti-island-marker (x)
    (let ((before (let ((ti-im-var 'lexical)) (ti-im-read))))
      (ti-declare-dyn ti-im-var)
      (let ((ti-im-var x)) (list before (ti-im-read))))))
(list (ti-island-marker 1) (ti-island-marker 2))
(defun ti-captured (x)
  (let ((y (* x 2)))
    (lambda (z) (list x y z ti-free (let ((y 5)) y) y))))
(let ((g (ti-captured 3))) (list (funcall g 1) (funcall g 2)))
(defun ti-many-binders (a)
  (let ((a (1+ a))) (let ((a (1+ a))) (let ((a (1+ a))) (let ((a (1+ a))) (let ((a (1+ a))) (list a)))))))
(list (ti-many-binders 0) (ti-many-binders 10))
"#,
    );
}

#[test]
fn calls_through_function_aliases() {
    let eval = assert_same(
        r#"
(defun ti-al-target (x) (list 'target x))
(defalias 'ti-al-lambda 'ti-al-target)
(defalias 'ti-al-chain 'ti-al-lambda)
(defalias 'ti-al-last 'last)
(defun ti-aliases (x)
  (list (not x) (null x) (string= "a" "a") (string< "a" "b")
        (ti-al-lambda x) (ti-al-chain x) (ti-al-last (list 1 2 x))
        (condition-case e (eval '(not)) (error e))
        (condition-case e (funcall (lambda () (not 1 2))) (error e))
        (condition-case e (ti-al-lambda) (error (car e)))))
(list (ti-aliases 1) (ti-aliases nil))
(defalias 'ti-al-lambda (lambda (x) (list 'redefined x)))
(list (ti-aliases 2))
(defalias 'ti-al-target (lambda (x) (list 'retargeted x)))
(defalias 'ti-al-lambda 'ti-al-target)
(list (ti-aliases 3))
(defmacro ti-al-mac (x) `(list 'mac ,x))
(defalias 'ti-al-to-mac 'ti-al-mac)
(let ((internal-make-interpreted-closure-function nil))
  (defun ti-al-use-mac (x) (ti-al-to-mac x)))
(list (ti-al-use-mac 1) (ti-al-use-mac 2))
(defalias 'ti-al-to-mac 'ti-al-target)
(list (ti-al-use-mac 3))
(fset 'ti-al-target nil)
(condition-case e (ti-aliases 4) (error e))
"#,
    );
    assert!(
        eval.tier_i.stats().count(TierIEvent::AliasCall) > 10,
        "{}",
        eval.tier_i.stats().report()
    );
}

#[test]
fn lazy_leaf_frames_appear_when_a_leaf_signals() {
    let eval = assert_same(
        r#"
(defvar ti-lz-log nil)
(defun ti-lz-frames ()
  (mapcar (lambda (f) (list (car f) (if (symbolp (cadr f)) (cadr f) 'fn) (car (cddr f))))
          (seq-take (nthcdr 5 (backtrace-frames)) 12)))
(defun ti-lz (x k)
  (condition-case e
      (handler-bind ((error (lambda (_e) (push (ti-lz-frames) ti-lz-log))))
        (cond ((= k 0) (car x))
              ((= k 1) (+ x 1))
              ((= k 2) (aref x 10))
              ((= k 3) (car ti-lz-unbound))
              ((= k 4) (length x))
              ((= k 5) (memq 'a x))
              (t (list (car-safe x) (nth 1 x) (1+ k) (eq x x)))))
    (error (list 'err e))))
(list (ti-lz 5 0) (ti-lz 'a 1) (ti-lz [1 2] 2) (ti-lz nil 3) (ti-lz 7 4) (ti-lz 8 5) (ti-lz '(1 2) 6))
(reverse ti-lz-log)
(let ((debugger (lambda (&rest args) (push (list 'dbg (car args) (ti-lz-frames)) ti-lz-log) nil))
      (debug-on-error t) (debug-ignored-errors nil) (ti-lz-log nil))
  (list (ti-lz 5 0) (ti-lz nil 3) (reverse ti-lz-log)))
(let ((debugger (lambda (&rest args) (push (list 'next (car args)) ti-lz-log) nil)) (ti-lz-log nil))
  (list (progn (setq debug-on-next-call t) (ti-lz '(1 2) 6)) (reverse ti-lz-log)))
(defvar ti-res-var nil)
(defun ti-res-make (tbl) (let ((o (list 1 2))) (puthash o t tbl) o))
(defun ti-res (tbl leaf)
  (setq ti-res-var (ti-res-make tbl))
  (if leaf (car ti-res-var) (ignore ti-res-var))
  (setq ti-res-var nil)
  (garbage-collect)
  (hash-table-count tbl))
(list (ti-res (make-hash-table :weakness 'key) t) (ti-res (make-hash-table :weakness 'key) nil)
      (ti-res (make-hash-table :weakness 'key) t))
"#,
    );
    let stats = eval.tier_i.stats();
    assert!(stats.count(TierIEvent::LazyLeaf) > 10, "{}", stats.report());
    assert!(
        stats.count(TierIEvent::LazyLeafSignal) >= 6,
        "{}",
        stats.report()
    );
}

#[test]
fn errors_and_their_data() {
    assert_same(
        r#"
(defun ti-err (k)
  (condition-case e
      (cond ((= k 0) (car 'x))
            ((= k 1) (undefined-ti-function 1 2))
            ((= k 2) ti-unbound-variable)
            ((= k 3) (funcall (lambda (a) a)))
            ((= k 4) (signal 'my-error '(1 2)))
            ((= k 5) (throw 'no-such-tag 1))
            ((= k 6) (let ((nil 1)) nil))
            ((= k 7) (eval '(if)))
            ((= k 8) (eval '(setq a)))
            ((= k 9) (eval '(let ((a 1 2)) a)))
            ((= k 10) (+ 1 (car (cdr 5))))
            ((= k 11) (1+ 'a))
            (t (list 'ok k)))
    (error (list 'err e))))
(let (out) (dotimes (k 13) (push (ti-err k) out)) (nreverse out))
(let (out) (dotimes (k 13) (push (ti-err k) out)) (nreverse out))
"#,
    );
}

#[test]
fn backtrace_frames_inside_compiled_forms() {
    let eval = assert_same(
        r#"
(defun ti-bt-probe ()
  (let (frames)
    (mapbacktrace (lambda (evald fun args flags)
                    (push (list evald fun (if (listp args) (length args) args)) frames))
                  #'ti-bt-probe)
    (let ((n 0) (keep nil))
      (dolist (f (nreverse frames))
        (when (< n 12) (push f keep))
        (setq n (1+ n)))
      (nreverse keep))))
(defun ti-bt (x)
  (let ((y (1+ x)))
    (if (> y 0)
        (progn (list (ti-bt-probe) y))
      'neg)))
(list (ti-bt 1) (ti-bt 2))
(defun ti-bt-frames ()
  (let ((fr (backtrace-frames #'ti-bt-frames)))
    (mapcar (lambda (f) (list (car f) (cadr f))) (seq-take fr 6))))
(defun ti-bt2 (a) (when a (let ((b a)) (and b (ti-bt-frames)))))
(list (ti-bt2 1) (ti-bt2 2))
(defun ti-bt-frame (a) (let ((z a)) (list (backtrace-frame 0 #'ti-bt-frame) (backtrace-frame 1 #'ti-bt-frame) z)))
(list (ti-bt-frame 5) (ti-bt-frame 6))
"#,
    );
    assert!(eval.tier_i.stats().count(TierIEvent::Run) >= 4);
}

#[test]
fn handler_bind_sees_the_frames_of_a_leaf_signal() {
    assert_same(
        r#"
(defun ti-hb (x)
  (let ((seen nil))
    (condition-case nil
        (handler-bind ((wrong-type-argument
                        (lambda (_e)
                          (setq seen (mapcar (lambda (f) (list (car f) (cadr f)))
                                             (seq-take (backtrace-frames) 8))))))
          (let ((y x)) (if y (car y) 'none)))
      (error nil))
    seen))
(list (ti-hb 1) (ti-hb 2))
"#,
    );
}

#[test]
fn debugger_and_debug_on_next_call() {
    assert_same(
        r#"
(defvar ti-dbg-log nil)
(defun ti-dbg-body (x) (let ((y (* x 2))) (list x y)))
(defun ti-dbg ()
  (let ((debugger (lambda (&rest args) (push (car args) ti-dbg-log) nil))
        (debug-on-error t)
        (debug-ignored-errors nil))
    (list (condition-case e (ti-dbg-body 'a) (error (car e)))
          (progn (setq debug-on-next-call t) (ti-dbg-body 3)))))
(list (ti-dbg) (ti-dbg) (reverse ti-dbg-log))
"#,
    );
}

#[test]
fn eval_depth_overflow_point_and_data() {
    assert_same(
        r#"
(defun ti-rec (k) (if (> k 0) (let ((r (ti-rec (1- k)))) (1+ r)) 0))
(ti-rec 5)
(let ((max-lisp-eval-depth 400) (found nil))
  (dolist (k '(50 100 120 130 140 150 200 400))
    (push (list k (condition-case e (ti-rec k) (error (car e)))) found))
  (nreverse found))
(let ((max-lisp-eval-depth 99))
  (condition-case e (ti-rec 200) (error e)))
"#,
    );
}

#[test]
fn quit_flag_set_by_lisp_fires_at_the_next_form() {
    assert_same(
        r#"
(defun ti-quit (x)
  (condition-case e
      (let ((a x))
        (setq quit-flag t)
        (list 'after a))
    (quit (list 'quit-caught e))))
(list (ti-quit 1) (ti-quit 2) quit-flag)
"#,
    );
}

#[test]
fn macros_expand_at_every_evaluation() {
    assert_same(
        r#"
(defvar ti-expansions 0)
(defmacro ti-mac (x) (setq ti-expansions (1+ ti-expansions)) `(list ,x ,ti-expansions))
(defun ti-use-mac (n) (let ((r nil)) (dotimes (i n) (push (ti-mac i) r)) r))
(list (ti-use-mac 3) (ti-use-mac 2) ti-expansions)
(defun ti-later (x) (ti-becomes-macro x))
(defun ti-becomes-macro (x) (list 'fn x))
(list (ti-later 1) (ti-later 2))
(defmacro ti-becomes-macro (x) `(list 'mac ,x))
(list (ti-later 3) (ti-later 4))
(fset 'ti-becomes-macro (lambda (x) (list 'fn-again x)))
(list (ti-later 5))
"#,
    );
}

#[test]
fn redefined_and_advised_functions_are_honoured() {
    assert_same(
        r#"
(defun ti-callee (x) (list 'v1 x))
(defun ti-caller (x) (let ((y x)) (ti-callee (car (list y)))))
(list (ti-caller 1) (ti-caller 2))
(defun ti-callee (x) (list 'v2 x))
(list (ti-caller 3))
(defun ti-around (f &rest args) (list 'advised (apply f args)))
(advice-add 'ti-callee :around #'ti-around)
(list (ti-caller 4))
(advice-remove 'ti-callee #'ti-around)
(list (ti-caller 5))
(defun ti-uses-car (x) (let ((c (car x))) c))
(ti-uses-car '(1 2))
(advice-add 'car :around (lambda (f &rest args) (apply f args)))
(ti-uses-car '(3 4))
"#,
    );
}

#[test]
fn self_modifying_code() {
    assert_same(
        r#"
(defun ti-mut () (let ((a 1) (b 2)) (list a b 'orig)))
(ti-mut)
(ti-mut)
(let* ((body (aref (symbol-function 'ti-mut) 1))
       (letform (car body))
       (listform (nth 2 letform)))
  ;; rewrite the quoted tag, then the variable reference, then a binder
  (setcar (cdr (nth 3 listform)) 'changed)
  (list (ti-mut)
        (progn (setcar (cdr listform) 'b) (ti-mut))
        (progn (setcar (car (nth 1 letform)) 'b) (ti-mut))))
(defun ti-mut2 (x) (if x 'yes 'no))
(ti-mut2 t)
(setcar (cdr (car (aref (symbol-function 'ti-mut2) 1))) 'x2)
(defvar x2 nil)
(list (ti-mut2 t) (ti-mut2 nil))
"#,
    );
}

#[test]
fn unwind_forms_and_non_local_exits() {
    assert_same(
        r#"
(defvar ti-uw-log nil)
(defun ti-uw (mode)
  (let ((x 0))
    (catch 'out
      (unwind-protect
          (progn (setq x 1)
                 (cond ((eq mode 'throw) (throw 'out (list 'thrown x)))
                       ((eq mode 'error) (car 'bad))
                       (t (setq x 2) (list 'normal x))))
        (push (list mode x) ti-uw-log)))))
(list (ti-uw 'throw) (ti-uw 'normal) (condition-case e (ti-uw 'error) (error (car e)))
      (reverse ti-uw-log))
"#,
    );
}

#[test]
fn condition_case_variable_binding() {
    assert_same(
        r#"
(defvar ti-cc-dyn nil)
(defun ti-cc (k)
  (list
   (condition-case err (if (= k 0) (car 1) 'fine)
     (error (list 'lex err (let ((g (lambda () err))) (car (funcall g)))))
     (:success (list 'ok err)))
   (condition-case ti-cc-dyn (/ 1 k)
     (arith-error (list 'dyn ti-cc-dyn (symbol-value 'ti-cc-dyn)))
     (:success (list 'ok ti-cc-dyn)))
   (condition-case nil (signal 'quit nil) (quit 'quit-handled))))
(list (ti-cc 0) (ti-cc 1) (ti-cc 0))
"#,
    );
}

#[test]
fn closures_identity_and_printing() {
    assert_same(
        r#"
(defun ti-mk (a b) (lambda (c) (list a b c)))
(let ((f (ti-mk 1 2)) (g (ti-mk 3 4)))
  (list (interpreted-function-p (symbol-function 'ti-mk))
        (byte-code-function-p (symbol-function 'ti-mk))
        (func-arity 'ti-mk)
        (prin1-to-string f)
        (funcall f 5) (funcall g 6)
        (eq (aref f 1) (aref g 1))
        (equal (aref f 2) (aref g 2))))
(defun ti-arity (a b) (list a b))
(list (condition-case e (ti-arity 1) (error (car e)))
      (condition-case e (funcall #'ti-arity 1 2 3) (error (list (car e) (length (cdr e))))))
(let ((internal-make-interpreted-closure-function nil))
  (defun ti-untrimmed (x) (let ((y 2)) (lambda () (list x y))))
  (list (prin1-to-string (ti-untrimmed 1)) (prin1-to-string (ti-untrimmed 1))))
"#,
    );
}

#[test]
fn threads_turn_tiered_code_off() {
    let eval = assert_same(
        r#"
(defun ti-thr (x) (let ((y x)) (* y 2)))
(ti-thr 1)
(ti-thr 2)
(thread-join (make-thread (lambda () (ti-thr 3))))
(list (ti-thr 4) (ti-thr 5))
"#,
    );
    assert!(eval.tier_i.stats().count(TierIEvent::RefuseThreads) > 0);
}

#[test]
fn verify_catches_nothing_on_a_loop_heavy_body() {
    let (results, eval) = run(
        TierIMode::Verify,
        r#"
(defun ti-loop (n)
  (let ((sum 0) (i 0) (v (make-vector 10 1)))
    (while (< i n)
      (let ((j (% i 10)))
        (setq sum (+ sum (aref v j) (if (= 0 (% i 2)) 1 0))))
      (setq i (1+ i)))
    sum))
(list (ti-loop 1000) (ti-loop 1000))
"#,
    );
    assert_eq!(last(&results), "OK (1500 1500)");
    assert!(eval.tier_i.stats().count(TierIEvent::Run) >= 2);
    let _ = Value::NIL;
}
