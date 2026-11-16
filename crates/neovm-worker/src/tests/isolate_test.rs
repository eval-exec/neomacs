//! The Lisp face of worker isolates, driven from a main isolate booted on
//! the test thread exactly as the host boots its own.
//!
//! Every test is its own process under nextest, so each installs the pool
//! once with two owners. Nothing here asserts on wall-clock time: a
//! timeout test only checks that a wait shorter than its task reports the
//! timeout condition, and cancellation is observed through the status
//! protocol, not through timing.

use crate::isolate::{self, IsolateBoot, IsolatePoolConfig};
use crate::lisp;
use crate::transfer::{self, Transfer, TransferError};
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::load::apply_runtime_startup_state;
use neovm_core::emacs_core::load_boot::create_bootstrap_evaluator_cached;
use neovm_core::emacs_core::print::format_eval_result_with_eval;

fn main_isolate() -> Box<Context> {
    let _ = isolate::install(IsolatePoolConfig {
        threads: 2,
        queue_capacity: 64,
        boot: IsolateBoot::CachedBootstrap,
    });
    let mut ctx = Box::new(create_bootstrap_evaluator_cached().expect("bootstrap evaluator"));
    ctx.setup_thread_locals();
    apply_runtime_startup_state(&mut ctx).expect("runtime startup state");
    lisp::register_subrs(&mut ctx);
    ctx
}

/// `OK <printed value>` or `ERR (<condition> <data>)`.
fn eval(ctx: &mut Context, source: &str) -> String {
    let result = ctx.eval_str(source);
    format_eval_result_with_eval(ctx, &result)
}

/// Run FORM's value through a worker and back: `(neomacs-worker-wait
/// (neomacs-worker-run #'identity FORM))`.
fn round_trip(ctx: &mut Context, form: &str) -> String {
    eval(
        ctx,
        &format!("(neomacs-worker-wait (neomacs-worker-run #'identity {form}))"),
    )
}

#[test]
fn wire_format_round_trips_every_transfer_kind() {
    let value = Transfer::List {
        items: vec![
            Transfer::Nil,
            Transfer::T,
            Transfer::Fixnum(-42),
            Transfer::Bignum("123456789012345678901234567890".to_string()),
            Transfer::Float(1.5f64.to_bits()),
            Transfer::Str {
                bytes: b"caf\xc3\xa9".to_vec(),
                multibyte: true,
            },
            Transfer::Str {
                bytes: vec![0xff, 0x00],
                multibyte: false,
            },
            Transfer::Symbol(":key".to_string()),
            Transfer::Vector(vec![Transfer::Fixnum(1)]),
            Transfer::Record(vec![Transfer::Symbol("point".to_string())]),
            Transfer::BoolVector {
                nbits: 3,
                words: vec![0b101],
            },
            Transfer::HashTable {
                test: neovm_core::emacs_core::value::HashTableTest::Equal,
                weakness: Some(neovm_core::emacs_core::value::HashTableWeakness::Key),
                entries: vec![(Transfer::Symbol("a".to_string()), Transfer::Fixnum(1))],
            },
            Transfer::Closure(vec![Transfer::Nil; 6]),
            Transfer::ByteCode("#[0 \"\\300\\207\" [1] 1]".to_string()),
            Transfer::Subr("car".to_string()),
        ],
        tail: Some(Box::new(Transfer::Fixnum(7))),
    };
    let bytes = transfer::encode(&value);
    assert_eq!(transfer::decode(&bytes).expect("decodes"), value);

    let call = transfer::Call {
        function: Transfer::Symbol("+".to_string()),
        args: vec![Transfer::Fixnum(1), Transfer::Fixnum(2)],
    };
    let bytes = transfer::encode_call(&call);
    assert_eq!(transfer::decode_call(&bytes).expect("decodes"), call);
    assert!(matches!(
        transfer::decode_call(&bytes[..bytes.len() - 1]),
        Err(TransferError::Decode(_))
    ));
    assert!(matches!(
        transfer::decode(&[200]),
        Err(TransferError::Decode(_))
    ));
}

#[test]
fn values_round_trip_through_a_worker() {
    let mut ctx = main_isolate();
    assert_eq!(round_trip(&mut ctx, "42"), "OK 42");
    assert_eq!(round_trip(&mut ctx, "-7"), "OK -7");
    assert_eq!(round_trip(&mut ctx, "nil"), "OK nil");
    assert_eq!(round_trip(&mut ctx, "t"), "OK t");
    assert_eq!(round_trip(&mut ctx, "1.5"), "OK 1.5");
    assert_eq!(
        round_trip(&mut ctx, "(expt 2 70)"),
        "OK 1180591620717411303424"
    );
    assert_eq!(round_trip(&mut ctx, "?a"), "OK 97");
    assert_eq!(round_trip(&mut ctx, "\"plain\""), "OK \"plain\"");
    assert_eq!(
        round_trip(&mut ctx, "\"caf\\u00e9 \\u4e2d\""),
        "OK \"café 中\""
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(multibyte-string-p (neomacs-worker-wait (neomacs-worker-run #'identity \"caf\\u00e9\")))"
        ),
        "OK t"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(multibyte-string-p (neomacs-worker-wait (neomacs-worker-run #'identity (string-to-unibyte \"abc\"))))"
        ),
        "OK nil"
    );
    assert_eq!(round_trip(&mut ctx, "'sym"), "OK sym");
    assert_eq!(round_trip(&mut ctx, ":kw"), "OK :kw");
    assert_eq!(round_trip(&mut ctx, "'(1 (2 3) . 4)"), "OK (1 (2 3) . 4)");
    assert_eq!(
        round_trip(&mut ctx, "[1 \"two\" (3)]"),
        "OK [1 \"two\" (3)]"
    );
    assert_eq!(round_trip(&mut ctx, "(record 'pt 1 2)"), "OK #s(pt 1 2)");
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((bv (make-bool-vector 5 nil))) (aset bv 1 t) (aset bv 4 t) \
             (equal bv (neomacs-worker-wait (neomacs-worker-run #'identity bv))))"
        ),
        "OK t"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(let* ((h (make-hash-table :test 'equal)) (_ (puthash \"k\" '(v) h)) (_ (puthash 2 3 h)) \
             (back (neomacs-worker-wait (neomacs-worker-run #'identity h)))) \
             (list (hash-table-test back) (hash-table-count back) (gethash \"k\" back) (gethash 2 back)))"
        ),
        "OK (equal 2 (v) 3)"
    );
    // The copy is a copy: mutating it in the worker leaves the original alone.
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((v (vector 1 2))) \
             (neomacs-worker-wait (neomacs-worker-run (lambda (x) (aset x 0 'changed) x) v)) \
             v)"
        ),
        "OK [1 2]"
    );
}

#[test]
fn functions_cross_as_closures_symbols_and_subrs() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run #'+ 1 2 3))"
        ),
        "OK 6"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run (symbol-function 'car) '(1 2)))"
        ),
        "OK 1"
    );
    // A lexical closure carries its captured environment across.
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run (eval '(let ((base 40)) (lambda (n) (+ base n))) t) 2))"
        ),
        "OK 42"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run (lambda (&rest xs) (apply #'max xs)) 3 9 4))"
        ),
        "OK 9"
    );
    // A function the worker's image does not know is a void-function there,
    // because definitions are not shared between isolates.
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (defun isolate-test-main-only () 'here) \
             (neomacs-worker-wait (neomacs-worker-run 'isolate-test-main-only)))"
        ),
        "ERR (void-function (isolate-test-main-only))"
    );
}

#[test]
fn worker_errors_are_resignaled_with_their_data() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run #'/ 1 0))"
        ),
        "ERR (arith-error nil)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (error \"boom %d\" 7)))) \
             (error (list 'caught (car err) (cadr err))))"
        ),
        "OK (caught error \"boom 7\")"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (define-error 'isolate-test-custom \"custom\") (signal 'isolate-test-custom '(1 two))))) \
               (t (list (car err) (cdr err))))"
        ),
        "OK (isolate-test-custom (1 two))"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run #'car 1))"
        ),
        "ERR (wrong-type-argument (listp 1))"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (define-error 'foo \"foo\") (signal 'foo '(1 . 2))))) (foo (cadr err)))"
        ),
        "OK (1 . 2)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (signal 'error (list \"line1\\n\\\"quoted\\\"\"))))) (error (cadr err)))"
        ),
        "OK \"line1\n\\\"quoted\\\"\""
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (signal 'error (list (expt 2 100)))))) (error (cadr err)))"
        ),
        "OK 1267650600228229401496703205376"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (signal 'error '((a (b . c)) d))))) (error (cdr err)))"
        ),
        "OK ((a (b . c)) d)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (signal 'error (list (intern \"symbol with space\")))))) (error (cadr err)))"
        ),
        "OK symbol\\ with\\ space"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (signal 'error (list (current-buffer)))))) (error (list (car err) (stringp (cadr err)) (not (null (string-match-p \"buffer\" (cadr err)))))))"
        ),
        "OK (error t t)"
    );
    // The worker's condition definition is private to its isolate.
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run (lambda () (define-error 'worker-only-condition \"worker-only message\") (signal 'worker-only-condition '(x))))) (t (error-message-string err)))"
        ),
        "OK \"peculiar error: x\""
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run 7)) (invalid-function (car err)))"
        ),
        "OK invalid-function"
    );
}

#[test]
fn waits_accept_lists_share_timeout_and_keep_unwaited_tasks() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((a (neomacs-worker-run #'+ 1 2)) (b (neomacs-worker-run #'+ 3 4))) (neomacs-worker-wait (list a b)))"
        ),
        "OK (3 7)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(let* ((bad (neomacs-worker-run (lambda () (error \"first\")))) (later (neomacs-worker-run #'+ 40 2))) (list (condition-case err (neomacs-worker-wait (list bad later)) (error 'first-error)) (neomacs-worker-wait later)))"
        ),
        "OK (first-error 42)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait nil -0.1) (args-out-of-range (car err)))"
        ),
        "OK args-out-of-range"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait nil 'bad) (wrong-type-argument (cadr err)))"
        ),
        "OK numberp"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait nil 0.0e+NaN) (args-out-of-range (car err)))"
        ),
        "OK args-out-of-range"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-run) (wrong-number-of-arguments (car err)))"
        ),
        "OK wrong-number-of-arguments"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (setq neomacs-worker-pool-size 0) (condition-case err (neomacs-worker-run #'identity 1) (neomacs-worker-error (car err))))"
        ),
        "OK neomacs-worker-error"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (setq neomacs-worker-pool-size nil) (stringp (documentation 'neomacs-worker-run)))"
        ),
        "OK t"
    );
    assert_eq!(
        eval(&mut ctx, "(special-variable-p 'neomacs-worker-pool-size)"),
        "OK t"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(and (stringp (documentation 'neomacs-worker-run)) (stringp (documentation 'neomacs-worker-wait)) (stringp (documentation 'neomacs-worker-cancel)) (stringp (documentation 'neomacs-worker-status)) (stringp (documentation 'neomacs-worker-pool-size)) (stringp (documentation 'neomacs-worker-stats)))"
        ),
        "OK t"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((a (neomacs-worker-run (lambda () (sleep-for 0.2) 'done))) (b (neomacs-worker-run (lambda () (sleep-for 0.4) 'done)))) (condition-case err (neomacs-worker-wait (list a b) 0.3) (neomacs-worker-timeout 'timed-out)))"
        ),
        "OK timed-out"
    );
}

#[test]
fn quit_during_wait_does_not_consume_the_task() {
    let mut ctx = main_isolate();
    let request = ctx.quit_requested.clone();
    let helper = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        request.request();
    });
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((task (neomacs-worker-run (lambda () (sleep-for 0.2) 42)))) (list (condition-case err (neomacs-worker-wait task) (quit 'quit)) (neomacs-worker-wait task)))"
        ),
        "OK (quit 42)"
    );
    helper.join().expect("quit helper thread");
}

#[test]
fn editor_objects_and_cycles_are_refused_at_the_boundary() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-run #'identity (current-buffer)) \
             (neomacs-worker-transfer-error (cdr err)))"
        ),
        "OK (unsupported-object buffer)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-run #'identity (point-marker)) \
             (neomacs-worker-transfer-error (cdr err)))"
        ),
        "OK (unsupported-object marker)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-run #'identity (vector 1 (current-buffer))) \
             (neomacs-worker-error (cdr err)))"
        ),
        "OK (unsupported-object buffer)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (let ((l (list 1 2))) (setcdr (cdr l) l) (neomacs-worker-run #'identity l)) \
             (neomacs-worker-transfer-error (cdr err)))"
        ),
        "OK (circular-object)"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (let ((v (vector nil))) (aset v 0 v) (neomacs-worker-run #'identity v)) \
             (neomacs-worker-transfer-error (cdr err)))"
        ),
        "OK (circular-object)"
    );
    // Shared substructure is not a cycle.
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((shared (list 1))) (neomacs-worker-wait (neomacs-worker-run #'identity (list shared shared))))"
        ),
        "OK ((1) (1))"
    );
    // A worker returning an object of its own editor state is refused on
    // the way back, with the condition raised in the waiter.
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait (neomacs-worker-run #'current-buffer)) \
             (neomacs-worker-transfer-error (car (cdr err))))"
        ),
        "OK unsupported-object"
    );
}

#[test]
fn workers_do_not_share_state_with_the_main_isolate() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (setq isolate-test-main-var 'main) \
             (neomacs-worker-wait (neomacs-worker-run (lambda () (boundp 'isolate-test-main-var)))))"
        ),
        "OK nil"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (neomacs-worker-wait (neomacs-worker-run (lambda () (setq isolate-test-worker-var 'worker) t))) \
             (boundp 'isolate-test-worker-var))"
        ),
        "OK nil"
    );
    // A worker's buffers are its own: it may use them, and the main
    // isolate's buffer text is untouched.
    assert_eq!(
        eval(
            &mut ctx,
            "(with-temp-buffer (insert \"main text\") \
             (list (neomacs-worker-wait (neomacs-worker-run (lambda () (with-temp-buffer (insert \"worker\") (buffer-string))))) \
                   (buffer-string)))"
        ),
        "OK (\"worker\" \"main text\")"
    );
}

#[test]
fn many_tasks_complete_on_two_owners() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((tasks (mapcar (lambda (i) (neomacs-worker-run (lambda (n) (let ((s 0)) (dotimes (k n) (setq s (+ s k))) s)) i)) \
                                 (number-sequence 1 40)))) \
             (equal (mapcar #'neomacs-worker-wait tasks) \
                    (mapcar (lambda (i) (/ (* i (1- i)) 2)) (number-sequence 1 40))))"
        ),
        "OK t"
    );
    assert_eq!(eval(&mut ctx, "(neomacs-worker-pool-size)"), "OK 2");
    assert_eq!(
        eval(
            &mut ctx,
            "(plist-get (neomacs-worker-stats) :startup-error)"
        ),
        "OK nil"
    );
    // The owners boot concurrently with the first tasks; both report.
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (let ((attempts 0)) (while (and (< (plist-get (neomacs-worker-stats) :ready) 2) (< attempts 100000)) (setq attempts (1+ attempts))) \
                    (unless (= (plist-get (neomacs-worker-stats) :ready) 2) (error \"worker startup poll exceeded 100000 iterations\"))) \
             (plist-get (neomacs-worker-stats) :ready))"
        ),
        "OK 2"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(>= (plist-get (neomacs-worker-stats) :completed) 40)"
        ),
        "OK t"
    );
}

#[test]
fn status_timeout_cancel_and_handles() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((task (neomacs-worker-run #'identity 1))) \
             (neomacs-worker-wait task) \
             (list (neomacs-worker-status task) \
                   (condition-case err (neomacs-worker-wait task) (neomacs-worker-error (cadr err)))))"
        ),
        "OK (nil \"No such worker task\")"
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-wait 'not-a-task) (wrong-type-argument (cdr err)))"
        ),
        "OK (neomacs-worker-task-p not-a-task)"
    );
    // A spinner never returns; a wait shorter than forever times out. The
    // spinner is then cancelled so its owner is free again.
    assert_eq!(
        eval(
            &mut ctx,
            "(progn (defun isolate-test-spin () (let ((i 0)) (while t (setq i (1+ i))))) \
             (let ((spinner (neomacs-worker-run (symbol-function 'isolate-test-spin)))) \
               (prog1 (condition-case err (neomacs-worker-wait spinner 0.01) (neomacs-worker-timeout 'timed-out)) \
                 (neomacs-worker-cancel spinner))))"
        ),
        "OK timed-out"
    );
    // Two spinners occupy both owners, so a third task stays queued: its
    // cancellation is observed before it ever runs, and its wait reports it.
    assert_eq!(
        eval(
            &mut ctx,
            "(let* ((spin (symbol-function 'isolate-test-spin)) \
                    (blockers (list (neomacs-worker-run spin) (neomacs-worker-run spin))) \
                    (task (neomacs-worker-run #'identity 'queued))) \
             (prog1 (list (neomacs-worker-status task) \
                          (neomacs-worker-cancel task) \
                          (neomacs-worker-status task) \
                          (neomacs-worker-cancel task) \
                          (condition-case err (neomacs-worker-wait task) (neomacs-worker-cancelled 'cancelled))) \
               (mapc #'neomacs-worker-cancel blockers)))"
        ),
        "OK (queued t cancelled nil cancelled)"
    );
    // Cancelling a running task interrupts its evaluator (a quit at the
    // next safepoint); afterwards its owner takes new work.
    assert_eq!(
        eval(
            &mut ctx,
            "(let ((spinner (neomacs-worker-run (symbol-function 'isolate-test-spin)))) \
             (let ((attempts 0)) (while (and (not (eq (neomacs-worker-status spinner) 'running)) (< attempts 100000)) (setq attempts (1+ attempts))) \
               (unless (eq (neomacs-worker-status spinner) 'running) (error \"spinner did not start within 100000 polls\"))) \
             (neomacs-worker-cancel spinner) \
             (list (condition-case err (neomacs-worker-wait spinner) (neomacs-worker-cancelled 'cancelled)) \
                   (neomacs-worker-wait (neomacs-worker-run #'+ 1 1)) \
                   (neomacs-worker-wait (neomacs-worker-run #'+ 2 2))))"
        ),
        "OK (cancelled 2 4)"
    );
}

#[test]
fn a_process_without_a_pool_signals_instead_of_panicking() {
    // Only the primitives, no pool: every entry reports the missing pool.
    if isolate::installed().is_some() {
        return;
    }
    let mut ctx = Box::new(create_bootstrap_evaluator_cached().expect("bootstrap evaluator"));
    ctx.setup_thread_locals();
    apply_runtime_startup_state(&mut ctx).expect("runtime startup state");
    lisp::register_subrs(&mut ctx);
    assert_eq!(eval(&mut ctx, "(neomacs-worker-pool-size)"), "OK 0");
    assert_eq!(eval(&mut ctx, "(neomacs-worker-stats)"), "OK nil");
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (neomacs-worker-run #'identity 1) (neomacs-worker-error (car err)))"
        ),
        "OK neomacs-worker-error"
    );
}
