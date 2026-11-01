//! Model harness helpers (the nmjit-publication-loom house pattern).
//!
//! Positives run `check` (bounded preemptions, default bound 2, overridable
//! via `N5_PREEMPTIONS`) and fail on any explored violation. Negatives run
//! the weakened scenario and require the exact single-line witness panic,
//! so a violation that stops being detectable turns the test red.
#![allow(dead_code)]
pub mod vocab;

use std::sync::atomic::{AtomicUsize, Ordering};

/// Preserve the original invariant payload if a child failure is returned by
/// JoinHandle rather than already propagated by the Loom scheduler.
pub fn join<T>(handle: loom::thread::JoinHandle<T>) -> T {
    match handle.join() {
        Ok(value) => value,
        Err(failure) => std::panic::resume_unwind(failure),
    }
}

pub fn check(threads: usize, body: impl Fn() + Send + Sync + 'static) {
    let bound = std::env::var("N5_PREEMPTIONS")
        .map(|v| v.parse::<usize>().expect("integer preemption bound"))
        .unwrap_or(2);
    assert!(bound >= 2, "qualification needs at least bound 2");
    explore(threads, Some(bound), body);
}

pub fn exhaustive(threads: usize, body: impl Fn() + Send + Sync + 'static) {
    explore(threads, None, body);
}

fn explore(threads: usize, bound: Option<usize>, body: impl Fn() + Send + Sync + 'static) {
    struct Counters {
        entered: AtomicUsize,
        completed: AtomicUsize,
    }
    struct Receipt {
        counters: std::sync::Arc<Counters>,
        bound: Option<usize>,
    }
    impl Drop for Receipt {
        fn drop(&mut self) {
            eprintln!(
                "LOOM_RECEIPT {} bound={:?} entered={} completed={}",
                std::thread::current().name().unwrap_or("unnamed"),
                self.bound,
                self.counters.entered.load(Ordering::Relaxed),
                self.counters.completed.load(Ordering::Relaxed),
            );
        }
    }
    let receipt = Receipt {
        counters: std::sync::Arc::new(Counters {
            entered: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
        }),
        bound,
    };
    let count = std::sync::Arc::clone(&receipt.counters);
    let mut builder = loom::model::Builder::new();
    // `Builder::check` runs the body on a Loom thread of its own, so the
    // ceiling is the scenario's spawned threads plus that one.
    builder.max_threads = threads + 1;
    builder.max_branches = 4096;
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.checkpoint_file = None;
    builder.location = true;
    // Deliberately override ambient preemption limits: receipts name the
    // actual bound; exhaustive() is unbounded even in a bounded parent shell.
    builder.preemption_bound = bound;
    builder.check(move || {
        count.entered.fetch_add(1, Ordering::Relaxed);
        body();
        count.completed.fetch_add(1, Ordering::Relaxed);
    });
}

pub fn negative(threads: usize, expected: &'static str, body: impl Fn() + Send + Sync + 'static) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(threads, body)));
    let failure = result.expect_err("negative control found no required witness");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("non-string panic cannot satisfy a control");
    // Loom's checked-cell heading is exact, with optional source locations
    // following it. Our own invariant witnesses are exact single-line text.
    assert_eq!(message.trim_start().lines().next(), Some(expected));
    eprintln!(
        "LOOM_WITNESS {} {}",
        std::thread::current().name().unwrap_or("unnamed"),
        expected
    );
}
