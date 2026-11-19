//! Carrier pool (Green S-track): OS-thread reuse, fresh Lisp identity,
//! teardown drain, and the park TLS snapshot.

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green(with_jit: bool) -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(!with_jit);
    Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime())
}

#[test]
fn carrier_pool_reuses_the_os_thread_with_fresh_lisp_identity() {
    // The carrier-job log is process-global and earlier tests may have run
    // jobs in this test binary; compare only this test's two entries.
    let before = super::switch::carrier_job_threads_for_test().len();
    let mut main = green(false);
    assert_eq!(
        eval(
            &mut main,
            "(progn (setq cp-t1 (make-thread (lambda () 41) \"cp-one\")) (thread-join cp-t1))"
        ),
        "41"
    );
    assert_eq!(
        eval(
            &mut main,
            "(progn (setq cp-t2 (make-thread (lambda () 42) \"cp-two\")) (thread-join cp-t2))"
        ),
        "42"
    );
    // Lisp identity is fresh per make-thread even though the OS executor is
    // reused; thread-name returns the stored object; join is repeatable.
    assert_eq!(
        eval(
            &mut main,
            "(list (eq cp-t1 cp-t2) (thread-name cp-t1) (thread-name cp-t2) \
             (thread-join cp-t1) (thread-join cp-t2) (thread-live-p cp-t1))"
        ),
        "(nil \"cp-one\" \"cp-two\" 41 42 nil)"
    );
    let all = super::switch::carrier_job_threads_for_test();
    let jobs = &all[before..];
    assert_eq!(jobs.len(), 2, "two Lisp threads ran");
    assert_eq!(
        jobs[0].1, jobs[1].1,
        "the second Lisp thread must reuse the first carrier's OS thread"
    );
    assert_eq!(
        super::carrier::live_carriers_for_test(),
        1,
        "one bounded idle carrier serves the second job"
    );
}

/// More than [`MAX_IDLE_CARRIERS`] jobs overlap, for several rounds. Joining
/// every Lisp thread parks each carrier first, so nothing is in flight: the
/// pool may keep only the idle carriers. Exited carriers used to stay
/// `Draining` with their `JoinHandle`s, which retains the pthread stack.
#[test]
fn carrier_pool_bounds_slots_and_handles_after_oversubscribed_rounds() {
    let mut main = green(false);
    let max = super::carrier::max_idle_carriers_for_test();
    let width = max + 16;
    let rounds = 4;
    for round in 0..rounds {
        let src = format!(
            "(condition-case err \
               (progn \
                 (defvar cp-ex-n 0) \
                 (defvar cp-ex-started 0) \
                 (defvar cp-ex-lock nil) \
                 (setq cp-ex-n {width}) \
                 (setq cp-ex-started 0) \
                 (setq cp-ex-lock (make-mutex)) \
                 (let ((i 0) (threads nil)) \
                   (while (< i cp-ex-n) \
                     (setq threads \
                       (cons (make-thread \
                               (lambda () \
                                 (mutex-lock cp-ex-lock) \
                                 (setq cp-ex-started (1+ cp-ex-started)) \
                                 (mutex-unlock cp-ex-lock) \
                                 (let ((done nil)) \
                                   (while (not done) \
                                     (mutex-lock cp-ex-lock) \
                                     (setq done (>= cp-ex-started cp-ex-n)) \
                                     (mutex-unlock cp-ex-lock) \
                                     (if done nil (thread-yield)))) \
                                 1)) \
                             threads)) \
                     (setq i (1+ i))) \
                   (while threads \
                     (thread-join (car threads)) \
                     (setq threads (cdr threads)))) \
                 cp-ex-started) \
               (error (prin1-to-string err)))"
        );
        assert_eq!(
            eval(&mut main, &src),
            width.to_string(),
            "round {round} did not release every worker"
        );
        let (slots, handles) = main.carrier_pool_counts_for_test();
        assert!(
            slots <= max && handles <= max,
            "round {round}: exited carriers stayed in the pool: slots={slots} handles={handles} bound={max}"
        );
    }
}

#[test]
fn carrier_pool_drains_every_carrier_at_teardown() {
    let before = super::carrier::live_carriers_for_test();
    {
        let mut main = green(false);
        assert_eq!(
            eval(
                &mut main,
                "(progn (setq cp-l1 (make-thread (lambda () 1))) (thread-join cp-l1))"
            ),
            "1"
        );
        assert_eq!(
            eval(
                &mut main,
                "(progn (setq cp-l2 (make-thread (lambda () 2))) (thread-join cp-l2))"
            ),
            "2"
        );
        assert_eq!(super::carrier::live_carriers_for_test(), 1);
    }
    let mut reached = None;
    for _ in 0..400 {
        let live = super::carrier::live_carriers_for_test();
        if live == before {
            reached = Some(live);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        reached,
        Some(before),
        "carrier OS threads leaked past Context teardown"
    );
}

#[cfg(debug_assertions)]
#[test]
fn carrier_reuse_restores_the_fresh_tls_snapshot() {
    // The worker-side hook captures the fresh-carrier TLS fingerprint before
    // the first job's prologue and asserts it again after each reuse reset.
    // Park nulls the heap-pointer records; the deferred reset clears the
    // remaining caches at reuse, so the pair must reproduce the fresh state.
    super::carrier_tls::arm_carrier_tls_snapshot();
    let mut main = green(true);
    let seen = eval(
        &mut main,
        "(progn (setq cp-s1 (make-thread (lambda () (progn (string-match \"a+\" \"aaab\") \
         (prin1-to-string '(1 2)) (parse-partial-sexp 1 3) nil)) \"cp-snap1\")) \
         (thread-join cp-s1))",
    );
    assert_eq!(seen, "nil");
    // A second job on the same carrier exercises the retained reference; the
    // compiled loop also populates the JIT caches before the park.
    let count = eval(
        &mut main,
        "(progn (setq cp-s2 (make-thread (lambda () (let ((i 0)) (while (< i 4000) (setq i (1+ i))) i)) \"cp-snap2\")) \
         (thread-join cp-s2))",
    );
    assert_eq!(count, "4000");
}

#[cfg(debug_assertions)]
#[test]
fn carrier_reuse_reads_through_caches_after_a_gc_while_idle() {
    super::carrier_tls::arm_carrier_tls_snapshot();
    let mut main = green(true);
    assert_eq!(
        eval(
            &mut main,
            "(progn (setq cp-g1 (make-thread (lambda () (string-match \"a+\" \"xaaay\")) \"cp-gc1\")) \
             (thread-join cp-g1))"
        ),
        "1"
    );
    // The carrier is idle; a full collection runs between the two jobs.
    assert_eq!(
        eval(
            &mut main,
            "(progn (garbage-collect) \
             (setq cp-g2 (make-thread (lambda () (list (string-match \"a+\" \"xaaay\") (substring \"xaaay\" 1 4))) \"cp-gc2\")) \
             (thread-join cp-g2))"
        ),
        "(1 \"aaa\")"
    );
}
