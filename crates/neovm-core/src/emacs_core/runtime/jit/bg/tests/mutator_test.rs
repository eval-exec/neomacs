//! Independent mutators share only backend jobs, never their Lisp roots.

use super::*;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::{OptMode, force_deopt_for_test, opt_mode_scope_for_test};
use crate::emacs_core::jit::try_run_compiled;
use crate::emacs_core::value::Value;

const WAIT: Duration = Duration::from_secs(60);

// Channels carry only commands and attestations. Every Context, function
// and Value is created and destroyed on its owning mutator thread.
enum Command {
    CheckForeignHeap(usize),
    CancelAndExit,
    Install,
}

enum Report {
    Ready {
        role: usize,
        heap: usize,
        collections: usize,
    },
    ForeignHeapExcluded(usize),
    Cancelled {
        role: usize,
        pending: usize,
    },
    Installed {
        role: usize,
        value: String,
        pending: usize,
    },
}

fn mutator(role: usize, commands: Receiver<Command>, reports: Sender<Report>) {
    let _backend = opt_mode_scope_for_test(OptMode::Legacy);
    force_mode_for_test(Some(BgMode::Threaded));
    force_deferred_install_for_test(false);
    force_deopt_for_test(false);
    let mut ctx = Context::new();
    let payload = ctx
        .eval_str(if role == 0 {
            "(list (make-string 3 ?a) (cons 7 9))"
        } else {
            "(list (make-string 3 ?b) (cons 11 13))"
        })
        .expect("private payload");
    let expected_bits = payload.bits();
    let f = function(vec![Op::Constant(0), Op::Return], vec![payload], 0);
    assert_eq!(
        try_run_compiled(&mut ctx, &f, Value::NIL, &[]).unwrap(),
        None
    );
    let id = f.jit_runtime().compiled_id().expect("pending id");
    assert_eq!(cache::cache_entry_kind_for_test(id), "pending");
    assert_eq!(pending_count(), 1);
    // Neither Rust-local f.constants nor payload is an owned Context root.
    // The pending leaf's immutable reloc vector must preserve the object.
    let before = ctx.tagged_heap.gc_collections();
    ctx.gc_collect_exact();
    ctx.gc_collect_exact();
    let mut roots = Vec::new();
    let heap = ctx.tagged_heap.identity();
    cache::collect_jit_reloc_gc_roots_for_heap(&mut roots, heap);
    assert!(roots.iter().any(|value| value.bits() == expected_bits));
    assert!(cache::compiled_cache_probe().1 >= 1);
    reports
        .send(Report::Ready {
            role,
            heap,
            collections: ctx.tagged_heap.gc_collections() - before,
        })
        .expect("parent is receiving");

    // Timed channel waits form a barrier without an uninterruptible Barrier:
    // a parent panic drops senders, so a child exits instead of deadlocking.
    let Ok(Command::CheckForeignHeap(other_heap)) = commands.recv_timeout(WAIT) else {
        return;
    };
    assert_ne!(heap, other_heap);
    roots.clear();
    cache::collect_jit_reloc_gc_roots_for_heap(&mut roots, other_heap);
    assert!(
        roots.is_empty(),
        "never export this mutator's roots to another heap"
    );
    roots.clear();
    cache::collect_jit_reloc_gc_roots_for_heap(&mut roots, heap);
    assert!(roots.iter().any(|value| value.bits() == expected_bits));
    reports
        .send(Report::ForeignHeapExcluded(role))
        .expect("parent is receiving");

    match commands.recv_timeout(WAIT) {
        Ok(Command::CancelAndExit) => {
            cache::evict_compiled(id);
            assert_eq!(pending_count(), 0);
            roots.clear();
            cache::collect_jit_reloc_gc_roots_for_heap(&mut roots, heap);
            assert!(
                roots.is_empty(),
                "cancelled private constant is no longer rooted"
            );
            // Do not dereference the cancelled payload after its last root
            // disappears. Drop its Rust-only holders before collecting.
            drop(roots);
            drop(f);
            ctx.gc_collect_exact();
            drop(ctx);
            reports
                .send(Report::Cancelled {
                    role,
                    pending: pending_count(),
                })
                .expect("parent is receiving");
            // Return without clearing another thread's caches or joining a
            // worker. Its queued cancelled job is drained after parent unhold.
        }
        Ok(Command::Install) => {
            assert_eq!(cache::cache_entry_kind_for_test(id), "pending");
            ctx.gc_collect_exact();
            let bits = try_run_compiled(&mut ctx, &f, Value::NIL, &[])
                .expect("no Lisp signal")
                .expect("ready private leaf installs");
            assert_eq!(bits, expected_bits);
            assert_eq!(cache::cache_entry_kind_for_test(id), "compiled");
            let value = crate::emacs_core::print::print_value(&Value::from_bits(bits));
            reports
                .send(Report::Installed {
                    role,
                    value,
                    pending: pending_count(),
                })
                .expect("parent is receiving");
        }
        Ok(Command::CheckForeignHeap(_)) => {
            panic!("foreign-heap check belongs before install/cancel")
        }
        Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {}
    }
}

#[test]
fn jit_bg_independent_mutators_root_install_and_cancel_separately() {
    std::thread::scope(|scope| {
        let mut hold = Some(hold_workers_for_test());
        let (reports_tx, reports_rx) = mpsc::channel();
        let (survivor_tx, survivor_rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = mpsc::channel();
        let survivor_reports = reports_tx.clone();
        let cancel_reports = reports_tx.clone();
        let mut survivor = Some(scope.spawn(move || mutator(0, survivor_rx, survivor_reports)));
        let mut canceller = Some(scope.spawn(move || mutator(1, cancel_rx, cancel_reports)));
        drop(reports_tx);

        // Always release the worker hold and command receivers before the
        // scope joins, even if a parent assertion or a child panics.
        let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut heaps = [None; 2];
            for _ in 0..2 {
                let Report::Ready {
                    role,
                    heap,
                    collections,
                } = reports_rx
                    .recv_timeout(WAIT)
                    .expect("both mutators reach their rooted pending barrier")
                else {
                    panic!("expected a ready mutator");
                };
                assert!(collections >= 2, "real collections while workers are held");
                assert!(heaps[role].replace(heap).is_none());
            }
            let [Some(survivor_heap), Some(cancel_heap)] = heaps else {
                panic!("one private heap per mutator");
            };
            assert_ne!(survivor_heap, cancel_heap);
            survivor_tx
                .send(Command::CheckForeignHeap(cancel_heap))
                .unwrap();
            cancel_tx
                .send(Command::CheckForeignHeap(survivor_heap))
                .unwrap();
            let mut checked = [false; 2];
            for _ in 0..2 {
                let Report::ForeignHeapExcluded(role) = reports_rx
                    .recv_timeout(WAIT)
                    .expect("both mutators validate owner-specific roots")
                else {
                    panic!("expected foreign-heap exclusion");
                };
                assert!(!std::mem::replace(&mut checked[role], true));
            }
            assert_eq!(checked, [true, true]);
            cancel_tx.send(Command::CancelAndExit).unwrap();
            let Report::Cancelled { role, pending } = reports_rx
                .recv_timeout(WAIT)
                .expect("one mutator cancels before exiting")
            else {
                panic!("expected cancellation");
            };
            assert_eq!((role, pending), (1, 0));
            canceller
                .take()
                .unwrap()
                .join()
                .expect("cancelled mutator exits cleanly");
            // The other mutator is still alive with its constant rooted
            // solely by its pending leaf. Releasing workers must not make
            // the exited mutator's cancelled job interfere with its install.
            drop(hold.take());
            assert!(
                quiesce_for_test(WAIT),
                "worker drains success and cancellation"
            );
            survivor_tx.send(Command::Install).unwrap();
            let Report::Installed {
                role,
                value,
                pending,
            } = reports_rx
                .recv_timeout(WAIT)
                .expect("surviving mutator installs its own result")
            else {
                panic!("expected a surviving install");
            };
            assert_eq!((role, pending), (0, 0));
            assert_eq!(value, "(\"aaa\" (7 . 9))");
        }));

        drop(hold.take());
        drop(survivor_tx);
        drop(cancel_tx);
        let survivor_join = survivor.take().unwrap().join();
        let cancel_join = canceller.take().map(|thread| thread.join());
        if let Err(panic) = verdict {
            std::panic::resume_unwind(panic);
        }
        survivor_join.expect("surviving mutator exits cleanly");
        if let Some(joined) = cancel_join {
            joined.expect("cancelled mutator exits cleanly");
        }
    });
}
