//! Each heap has its own collector thread (`tagged/gc/gc_thread.rs`
//! `COLLECTORS`): Worlds on one OS thread no longer queue behind each
//! other's concurrent marks, and a World's collector retires with it.

use super::Context;
use crate::emacs_core::print::print_value;
use std::sync::mpsc;
use std::time::Duration;

const KEEP: &str = "(setq tph-keep
  (let ((l nil) (i 0))
    (while (< i 2000) (setq l (cons (cons i (make-string 3 ?a)) l) i (1+ i)))
    l))";
const KEEP_SUM: &str =
    "(let ((s 0) (l tph-keep)) (while l (setq s (+ s (car (car l))) l (cdr l))) s)";
const CHURN: &str = "(let ((i 0)) (while (< i 2000) (make-list 8 i) (setq i (1+ i))))";

fn eval(ev: &mut Context, src: &str) -> String {
    ev.setup_thread_locals();
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// Allocate and collect until `ev` has a concurrent mark in flight, and
/// leave it running. A generational heap runs up to
/// `NEOVM_GC_MAJOR_MAX_MINORS` (64) synchronous minors, each with its
/// deferred sweep, before the concurrent major.
fn launch_concurrent_mark(ev: &mut Context) {
    for _ in 0..4096 {
        eval(ev, CHURN);
        ev.gc_collect_from_current_roots_impl(false);
        if ev.tagged_heap.concurrent_mark_running() {
            return;
        }
    }
    panic!("the World never launched a concurrent mark");
}

/// How long a body may run before it counts as stuck. A stuck collector never
/// finishes, so this only needs to exceed the slowest healthy run: about 55 s
/// for `two_worlds_on_one_thread_collect_while_both_mark` alone under TSan on
/// arm64 (GEN1), and over 60 s with eight tests in parallel there.
const STUCK_AFTER: Duration = Duration::from_secs(300);

/// Run `body` on its own thread, failing (rather than hanging the test
/// binary) if it does not finish in time.
fn within_timeout(body: impl FnOnce() + Send + 'static) {
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        body();
        let _ = done_tx.send(());
    });
    match done_rx.recv_timeout(STUCK_AFTER) {
        Ok(()) => worker.join().expect("the body panicked after finishing"),
        // The body panicked: surface its message.
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            std::panic::resume_unwind(worker.join().expect_err("the body did not finish"))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("a World's collection is stuck behind another World's collector")
        }
    }
}

#[test]
fn two_worlds_on_one_thread_collect_while_both_mark() {
    within_timeout(|| {
        #[cfg(debug_assertions)]
        neovm_value::test_support::authority::set_verify_marked_objects_for_test(true);
        let mut a = Context::new(crate::test_utils::unit_runtime());
        let kept = eval(&mut a, KEEP);
        assert!(!kept.starts_with("ERR"), "{kept}");
        let mut parked_a = a.park_for_test();
        let mut b = Context::new(crate::test_utils::unit_runtime());
        let kept = eval(&mut b, KEEP);
        assert!(!kept.starts_with("ERR"), "{kept}");
        let mut parked_b = b.park_for_test();
        for round in 0..4 {
            let mut a = parked_a.resume_for_test().expect("resume a");
            launch_concurrent_mark(&mut a);
            parked_a = a.park_for_test();
            let mut b = parked_b.resume_for_test().expect("resume b");
            launch_concurrent_mark(&mut b);
            parked_b = b.park_for_test();
            // Each World collects while the other's mark is in flight.
            // Only one heap is installed; the other stays parked.
            if round % 2 == 0 {
                let mut a = parked_a.resume_for_test().expect("resume a");
                a.gc_collect_from_current_roots_impl(true);
                assert!(parked_b.tagged_heap().concurrent_mark_running());
                launch_concurrent_mark(&mut a);
                parked_a = a.park_for_test();
                let mut b = parked_b.resume_for_test().expect("resume b");
                b.gc_collect_from_current_roots_impl(true);
                assert!(parked_a.tagged_heap().concurrent_mark_running());
                parked_b = b.park_for_test();
            } else {
                let mut b = parked_b.resume_for_test().expect("resume b");
                b.gc_collect_from_current_roots_impl(true);
                assert!(parked_a.tagged_heap().concurrent_mark_running());
                launch_concurrent_mark(&mut b);
                parked_b = b.park_for_test();
                let mut a = parked_a.resume_for_test().expect("resume a");
                a.gc_collect_from_current_roots_impl(true);
                assert!(parked_b.tagged_heap().concurrent_mark_running());
                parked_a = a.park_for_test();
            }
            let mut a = parked_a.resume_for_test().expect("resume a");
            assert_eq!(eval(&mut a, KEEP_SUM), "1999000", "round {round}");
            parked_a = a.park_for_test();
            let mut b = parked_b.resume_for_test().expect("resume b");
            assert_eq!(eval(&mut b, KEEP_SUM), "1999000", "round {round}");
            parked_b = b.park_for_test();
        }
        let mut a = parked_a.resume_for_test().expect("resume a");
        a.finish_concurrent_mark()
            .expect("finish the concurrent mark");
        a.gc_collect_from_current_roots_impl(true);
        assert_eq!(eval(&mut a, KEEP_SUM), "1999000");
        drop(a);
        let mut b = parked_b.resume_for_test().expect("resume b");
        b.finish_concurrent_mark()
            .expect("finish the concurrent mark");
        b.gc_collect_from_current_roots_impl(true);
        assert_eq!(eval(&mut b, KEEP_SUM), "1999000");
        #[cfg(debug_assertions)]
        neovm_value::test_support::authority::set_verify_marked_objects_for_test(false);
    });
}

/// Start directly, without evaluator safe points that could terminate the
/// worker before the fixture hands out a black cell. Worker scheduling cannot
/// change the mutator-owned running flag or the region's used prefix.
fn start_mark_without_safe_points(ev: &mut Context) {
    ev.setup_thread_locals();
    ev.gc_collect_from_current_roots_impl(true);
    ev.tagged_heap.concurrent_begin();
    let heap = ev.tagged_heap.heap_root().as_ptr();
    // SAFETY: this thread owns the stopped World; no worker has launched yet.
    unsafe { ev.seed_registered_mutator_roots_world_stopped(heap) };
    ev.tagged_heap.launch_concurrent_mark();
    ev.setup_thread_locals();
    ev.tagged_heap.alloc_cons(
        neovm_value::tagged::value::TaggedValue::fixnum(17),
        neovm_value::tagged::value::TaggedValue::NIL,
    );
    assert!(ev.tagged_heap.alloc_regions_open());
}

enum MarkerJoin {
    Explicit,
    ByCollection,
}

fn forced_collection_after_two_world_marks(join: MarkerJoin) {
    use neovm_value::tagged::gc::MarkPhase;
    let a = Context::new(crate::test_utils::unit_runtime());
    let mut parked_a = a.park_for_test();
    let b = Context::new(crate::test_utils::unit_runtime());
    let mut parked_b = b.park_for_test();
    {
        let mut a = parked_a.resume_for_test().expect("resume a");
        start_mark_without_safe_points(&mut a);
        parked_a = a.park_for_test();
    }
    let mut b = parked_b.resume_for_test().expect("resume b");
    start_mark_without_safe_points(&mut b);
    // B is installed. A's marker is still running on the parked heap.
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &b.tagged_heap
    ));
    assert_eq!(
        parked_a.tagged_heap().mark_phase(),
        MarkPhase::ConcurrentRunning
    );
    match join {
        MarkerJoin::Explicit => {
            parked_a.finish_concurrent_mark().expect("join A");
            assert!(!parked_a.tagged_heap().alloc_regions_open());
            assert_eq!(
                parked_a.tagged_heap().mark_phase(),
                MarkPhase::StoppedMarking
            );
        }
        MarkerJoin::ByCollection => {}
    }
    // Catch the regression so both real workers are joined on the red path.
    let mut parked_a = Some(parked_a);
    let mut b = Some(b);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let parked_b = b.take().unwrap().park_for_test();
        let mut a = parked_a
            .take()
            .unwrap()
            .resume_for_test()
            .expect("resume a");
        // Clear A's heap slot so A's collection enters the
        // `!tagged_heap_is_current` arm on an empty slot while B's concurrent
        // marker is still running on `parked_b`.
        neovm_value::tagged::gc::clear_tagged_heap_if_installed(&a.tagged_heap);
        a.gc_collect_from_current_roots_impl(true);
        assert_eq!(a.tagged_heap.mark_phase(), MarkPhase::Idle);
        assert_eq!(
            parked_b.tagged_heap().mark_phase(),
            MarkPhase::ConcurrentRunning
        );
        parked_a = Some(a.park_for_test());
        let mut b_resumed = parked_b.resume_for_test().expect("resume b");
        neovm_value::tagged::gc::clear_tagged_heap_if_installed(&b_resumed.tagged_heap);
        b_resumed.gc_collect_from_current_roots_impl(true);
        b = Some(b_resumed);
    }));
    let mut parked_b = b.take().map(|active| active.park_for_test());
    if let Some(parked) = parked_a.take() {
        let mut a = parked.resume_for_test().expect("resume a before teardown");
        a.finish_concurrent_mark().expect("join before teardown");
        parked_a = Some(a.park_for_test());
    }
    if let Some(parked) = parked_b.take() {
        let mut active = parked.resume_for_test().expect("resume b before teardown");
        active
            .finish_concurrent_mark()
            .expect("join before teardown");
        parked_b = Some(active.park_for_test());
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    // Prove another collection is safe after both cycles were consumed.
    if let Some(parked) = parked_a.take() {
        let mut a = parked.resume_for_test().expect("resume a");
        a.gc_collect_from_current_roots_impl(true);
        drop(a);
    }
    if let Some(parked) = parked_b.take() {
        let mut b = parked.resume_for_test().expect("resume b");
        b.gc_collect_from_current_roots_impl(true);
    }
}

#[test]
fn two_worlds_forced_collection_after_explicit_marker_join() {
    forced_collection_after_two_world_marks(MarkerJoin::Explicit);
}

#[test]
fn two_worlds_forced_collection_without_explicit_marker_join_control() {
    forced_collection_after_two_world_marks(MarkerJoin::ByCollection);
}

/// How many of this process's threads are collectors. Nextest runs each
/// test in its own process, so only the test's own heaps count.
#[cfg(target_os = "linux")]
fn collector_threads() -> usize {
    std::fs::read_dir("/proc/self/task")
        .expect("read /proc/self/task")
        .filter_map(|task| std::fs::read_to_string(task.ok()?.path().join("comm")).ok())
        .filter(|comm| comm.trim_end() == neovm_value::tagged::gc::COLLECTOR_THREAD_NAME)
        .count()
}

/// Wait until `expected` collectors are listed. A joined thread has finished
/// running, but the kernel unlists it a moment after `pthread_join` returns.
#[cfg(target_os = "linux")]
fn await_collector_threads(expected: usize, limit: Duration) {
    let start = std::time::Instant::now();
    while collector_threads() != expected {
        assert!(
            start.elapsed() < limit,
            "{} collector threads, expected {expected}",
            collector_threads()
        );
        std::thread::sleep(Duration::from_micros(100));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn dropping_a_world_joins_its_collector() {
    let before = collector_threads();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.gc_collect_from_current_roots_impl(true);
    let heap = ev.tagged_heap.heap_identity();
    assert!(neovm_value::tagged::gc::collector_is_registered(heap));
    assert_eq!(collector_threads(), before + 1, "one collector per heap");
    drop(ev);
    assert!(!neovm_value::tagged::gc::collector_is_registered(heap));
    await_collector_threads(before, Duration::from_secs(1));
}

#[cfg(target_os = "linux")]
#[test]
fn a_world_dropped_mid_mark_detaches_a_collector_that_then_exits() {
    within_timeout(|| {
        let before = collector_threads();
        let mut ev = Context::new(crate::test_utils::unit_runtime());
        launch_concurrent_mark(&mut ev);
        let heap = ev.tagged_heap.heap_identity();
        assert!(neovm_value::tagged::gc::collector_is_registered(heap));
        assert_eq!(collector_threads(), before + 1, "one collector per heap");
        // Drop abandons the running job without waiting for it.
        drop(ev);
        assert!(!neovm_value::tagged::gc::collector_is_registered(heap));
        await_collector_threads(before, Duration::from_secs(30));
    });
}
