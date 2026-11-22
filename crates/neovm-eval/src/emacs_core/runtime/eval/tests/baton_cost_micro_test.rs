//! Public-API Baton thread-cost rows: the S3 thread-cost baseline.
//!
//! These rows exercise only the public thread API: `make-thread`,
//! `thread-yield`, `thread-join`, mutexes, condition variables, waits and
//! `let` bindings. None calls `green_wait` directly. Each row does a fixed
//! amount of semantic work, joins every thread, and asserts an exact result.
//! The measured section (the timed `eval_str`) is separate from the setup.
//!
//! The unit runtime loads no `subr.el`, so the forms use only primitives and
//! special forms: `while` loops instead of `dotimes`, `if` instead of
//! `when`/`unless`, and `null` instead of `not`. A missing function would
//! signal inside a worker that holds a mutex, and the row would then stop on
//! the deadlock verdict rather than on the real error. Forms that build per-thread closures are evaluated with
//! `(eval FORM t)` so their lambdas close over their arguments.
//!
//! Test-driver parameters (never a product knob):
//! - `TBC_MICRO_N`: the iteration count (default 200).
//! - `TBC_ROUTE`: `green` (default) is the pre-switch Green route, built
//!   through the explicit `new_with_threading` test seam. `default` is
//!   `Context::new`, the route the default switch will make Baton-serial.
//!   Before that switch `default` is Sim, which runs a new thread to
//!   completion inside `make-thread`. Rows that need a live peer refuse
//!   that route at once instead of spinning until the test timeout.
//!
//! Each row prints one `baton_row=` line with the timed section's elapsed
//! nanoseconds. Instruction-count slopes come from the measurement harness
//! running two values of `TBC_MICRO_N`.

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    PreSwitchGreen,
    Default,
}

impl Route {
    fn from_driver() -> Self {
        match std::env::var("TBC_ROUTE").as_deref() {
            Ok("default") => Route::Default,
            Ok("green") | Err(_) => Route::PreSwitchGreen,
            Ok(other) => panic!("TBC_ROUTE must be green or default, got {other:?}"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Route::PreSwitchGreen => "green",
            Route::Default => "default",
        }
    }

    fn context(self) -> Context {
        crate::test_utils::init_test_tracing();
        #[cfg(feature = "jit")]
        crate::emacs_core::jit::force_jit_off_for_test(true);
        match self {
            Route::PreSwitchGreen => {
                Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime())
            }
            Route::Default => Context::new(crate::test_utils::unit_runtime()),
        }
    }

    /// A Context for a row whose workload needs a second thread that runs
    /// while the first one waits.
    fn peer_context(self) -> Context {
        let ev = self.context();
        assert!(
            ev.threading_mode_is_green_for_test(),
            "route {} is not Baton-serial (pre-switch Sim): this row needs a live peer; \
             use TBC_ROUTE=green until the default switch",
            self.name()
        );
        ev
    }
}

fn iterations() -> usize {
    std::env::var("TBC_MICRO_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(200)
}

/// `(let ((VAR 0)) (while (< VAR N) BODY (setq VAR (1+ VAR))))`.
fn repeat(n: usize, var: &str, body: &str) -> String {
    format!("(let (({var} 0)) (while (< {var} {n}) {body} (setq {var} (1+ {var}))))")
}

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// Evaluate the measured section with lexical binding and report its wall
/// time.
fn timed(row: &str, route: Route, n: usize, ev: &mut Context, form: &str) -> String {
    let src = format!("(eval '{form} t)");
    let start = std::time::Instant::now();
    let printed = eval(ev, &src);
    let elapsed = start.elapsed();
    eprintln!(
        "baton_row={row} route={} n={n} elapsed_ns={}",
        route.name(),
        elapsed.as_nanos()
    );
    printed
}

/// The cold row's control: construct and drop a Context with no thread.
#[test]
fn baton_context_control() {
    let (route, n) = (Route::from_driver(), iterations());
    let start = std::time::Instant::now();
    for _ in 0..n {
        let mut ev = route.context();
        assert_eq!(eval(&mut ev, "'cold"), "cold");
    }
    eprintln!(
        "baton_row=baton_context_control route={} n={n} elapsed_ns={}",
        route.name(),
        start.elapsed().as_nanos()
    );
}

/// The first thread of each fresh Context: seating the runtime plus one
/// spawn and join. Subtract `baton_context_control` for the thread part.
#[test]
fn baton_spawn_join_cold() {
    let (route, n) = (Route::from_driver(), iterations());
    let start = std::time::Instant::now();
    for _ in 0..n {
        let mut ev = route.context();
        assert_eq!(
            eval(&mut ev, "(thread-join (make-thread (lambda () 'cold)))"),
            "cold"
        );
    }
    eprintln!(
        "baton_row=baton_spawn_join_cold route={} n={n} elapsed_ns={}",
        route.name(),
        start.elapsed().as_nanos()
    );
}

/// Spawn and join N threads in a Context whose thread runtime is already
/// seated.
#[test]
fn baton_spawn_join_warm() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.context();
    assert_eq!(
        eval(&mut ev, "(thread-join (make-thread (lambda () 'seated)))"),
        "seated"
    );
    let measured = format!(
        "(let ((ok 0)) {} ok)",
        repeat(
            n,
            "i",
            "(if (eq (thread-join (make-thread (lambda () 'warm))) 'warm) (setq ok (1+ ok)))"
        )
    );
    assert_eq!(
        timed("baton_spawn_join_warm", route, n, &mut ev, &measured),
        n.to_string()
    );
}

/// N public `thread-yield` calls from main with one live peer that yields
/// back: two Baton hand-offs per round trip.
#[test]
fn baton_public_yield_roundtrip() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.peer_context();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq bcy-stop nil bcy-turns 0)
               (setq bcy-th (make-thread
                             (lambda ()
                               (while (null bcy-stop)
                                 (setq bcy-turns (1+ bcy-turns))
                                 (thread-yield)))))
               (thread-yield)
               (list (thread-live-p bcy-th) (thread-last-error bcy-th)))"
        ),
        "(t nil)",
        "the yield row needs a live peer"
    );
    let measured = format!(
        "(progn (setq bcy-turns 0) {} bcy-turns)",
        repeat(n, "i", "(thread-yield)")
    );
    let printed = timed("baton_public_yield_roundtrip", route, n, &mut ev, &measured);
    let turns: usize = printed
        .parse()
        .unwrap_or_else(|_| panic!("peer turn count, got {printed}"));
    assert!(
        turns >= n,
        "every main yield must give the peer a turn: {turns} < {n}"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(progn (setq bcy-stop t)
                    (list (thread-join bcy-th) (thread-live-p bcy-th) (thread-last-error bcy-th)))"
        ),
        "(nil nil nil)"
    );
}

/// Two workers take one mutex recursively (depth 2) and yield while they
/// hold it, so every round contends: a parked `mutex-lock` and a hand-off.
#[test]
fn baton_recursive_mutex_ping_pong() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.peer_context();
    let body = repeat(
        n,
        "i",
        "(mutex-lock bcm-m) (mutex-lock bcm-m)
         (setq bcm-count (1+ bcm-count))
         (thread-yield)
         (mutex-unlock bcm-m) (mutex-unlock bcm-m)
         (thread-yield)",
    );
    let measured = format!(
        "(progn
           (setq bcm-m (make-mutex) bcm-count 0)
           (let* ((a (make-thread (lambda () {body})))
                  (b (make-thread (lambda () {body}))))
             (thread-join a)
             (thread-join b)
             (list bcm-count (thread-last-error a) (thread-last-error b)
                   (mutex-name bcm-m))))"
    );
    assert_eq!(
        timed(
            "baton_recursive_mutex_ping_pong",
            route,
            n,
            &mut ev,
            &measured
        ),
        format!("({} nil nil nil)", 2 * n)
    );
}

/// Two threads alternate strictly through one condition variable with
/// `condition-notify` waking one waiter.
#[test]
fn baton_condition_handoff_one() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.peer_context();
    let side = repeat(
        n,
        "i",
        "(mutex-lock bco-m)
         (while (null (eq bco-turn me)) (condition-wait bco-c))
         (setq bco-turn next bco-hops (1+ bco-hops))
         (condition-notify bco-c)
         (mutex-unlock bco-m)",
    );
    let measured = format!(
        "(progn
           (setq bco-m (make-mutex) bco-c (make-condition-variable bco-m)
                 bco-turn 'a bco-hops 0)
           (let* ((side (lambda (me next) (lambda () {side})))
                  (a (make-thread (funcall side 'a 'b)))
                  (b (make-thread (funcall side 'b 'a))))
             (thread-join a)
             (thread-join b)
             (list bco-hops bco-turn (thread-last-error a) (thread-last-error b))))"
    );
    assert_eq!(
        timed("baton_condition_handoff_one", route, n, &mut ev, &measured),
        format!("({} a nil nil)", 2 * n)
    );
}

/// Four threads pass a turn around a ring through one condition variable
/// with `condition-notify` waking all waiters.
#[test]
fn baton_condition_handoff_all() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.peer_context();
    let member = repeat(
        n,
        "i",
        "(mutex-lock bca-m)
         (while (/= bca-turn me) (condition-wait bca-c))
         (setq bca-turn (% (1+ me) 4) bca-hops (1+ bca-hops))
         (condition-notify bca-c t)
         (mutex-unlock bca-m)",
    );
    let measured = format!(
        "(progn
           (setq bca-m (make-mutex) bca-c (make-condition-variable bca-m)
                 bca-turn 0 bca-hops 0)
           (let* ((member (lambda (me) (lambda () {member})))
                  (ths (mapcar (lambda (k) (make-thread (funcall member k))) '(0 1 2 3))))
             (mapc #'thread-join ths)
             (list bca-hops bca-turn (delq nil (mapcar #'thread-last-error ths)))))"
    );
    assert_eq!(
        timed("baton_condition_handoff_all", route, n, &mut ev, &measured),
        format!("({} 0 nil)", 4 * n)
    );
}

/// Main makes N short native waits while a peer runs: each wait releases
/// the Baton and the peer makes progress during it.
///
/// This is the native-wait shape of `baton_blocking_io_peer_progress`. The
/// pipe-readiness shape needs `make-process`, which is defined in Lisp and
/// is not loaded in this crate's unit runtime; it is a follow-up row at the
/// level of the bootstrapped runtime.
#[test]
fn baton_blocking_io_peer_progress() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.peer_context();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq bci-stop nil bci-turns 0)
               (setq bci-th (make-thread
                             (lambda ()
                               (while (null bci-stop)
                                 (setq bci-turns (1+ bci-turns))
                                 (thread-yield)))))
               (thread-yield)
               (list (thread-live-p bci-th) (thread-last-error bci-th)))"
        ),
        "(t nil)",
        "the native-wait row needs a live peer"
    );
    let measured = format!(
        "(let ((progressed 0)) {} progressed)",
        repeat(
            n,
            "i",
            "(let ((before bci-turns))
               (sleep-for 0.001)
               (if (> bci-turns before) (setq progressed (1+ progressed))))"
        )
    );
    assert_eq!(
        timed(
            "baton_blocking_io_peer_progress",
            route,
            n,
            &mut ev,
            &measured
        ),
        n.to_string(),
        "the peer must run during every native wait"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(progn (setq bci-stop t) (list (thread-join bci-th) (thread-last-error bci-th)))"
        ),
        "(nil nil)"
    );
}

/// Two threads each keep a special `let` binding and a buffer-local value
/// in their own current buffer across N yields, with periodic collections
/// while the other thread is parked.
#[test]
fn baton_bindings_gc_handoff() {
    let (route, n) = (Route::from_driver(), iterations());
    let mut ev = route.peer_context();
    assert_eq!(
        eval(
            &mut ev,
            "(progn (defvar bcb-special 'global) (defvar bcb-local 'default) t)"
        ),
        "t"
    );
    let rounds = repeat(
        n,
        "i",
        "(thread-yield)
         (if (= 0 (% i 32)) (garbage-collect))
         (if (null (and (eq bcb-special me) (eq bcb-local me)))
           (error \"bindings lost: %S %S\" bcb-special bcb-local))",
    );
    let measured = format!(
        "(let* ((body (lambda (me buffer-name)
                        (let ((bcb-special me))
                          (set-buffer (get-buffer-create buffer-name))
                          (set (make-local-variable 'bcb-local) me)
                          {rounds}
                          me)))
                (worker (make-thread (lambda () (funcall body 'worker \" bcb-worker\")))))
           (list (funcall body 'main \" bcb-main\")
                 (thread-join worker)
                 (thread-last-error worker)
                 bcb-special))"
    );
    assert_eq!(
        timed("baton_bindings_gc_handoff", route, n, &mut ev, &measured),
        "(main worker nil global)"
    );
}
