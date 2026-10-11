//! Local worker protocol tests: no thread startup or process-global fault knobs.

use super::hooks::BatchEvent;
use super::*;
use crate::emacs_core::jit::bg::{BackendOut, JobCell, JobClass};
use crate::emacs_core::jit::compile::lowering::{RegallocChoice, jit_isa_for};
use crate::emacs_core::jit::compile::shared::split::JobPayload;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::Linkage;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

fn constant(value: i64) -> JobPayload {
    let config = jit_isa_for(RegallocChoice::Full)
        .expect("host ISA")
        .frontend_config();
    let mut sig = Signature::new(config.default_call_conv);
    sig.returns.push(AbiParam::new(types::I64));
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
    let mut context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut func, &mut context);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let value = builder.ins().iconst(types::I64, value);
        builder.ins().return_(&[value]);
        builder.finalize(config);
    }
    JobPayload {
        func,
        name: "worker-protocol-test".into(),
        linkage: Linkage::Local,
        named: true,
        imports: Box::default(),
        portable: true,
        regalloc: RegallocChoice::Full,
        disasm: false,
    }
}

fn job(seq: u64, payload: JobPayload, class: JobClass) -> (BackendJob, Arc<JobCell>) {
    let cell = JobCell::new();
    let insts = payload.func.dfg.num_insts() as u64;
    (
        BackendJob {
            payload,
            class,
            seq,
            enqueued_at: Instant::now(),
            cell: cell.clone(),
            insts,
        },
        cell,
    )
}

fn fixture(count: usize) -> (Arc<Pool>, Vec<Arc<JobCell>>) {
    let (jobs, cells) = (0..count)
        .map(|seq| job(seq as u64, constant(seq as i64 + 100), JobClass::Entry))
        .unzip();
    (Arc::new(Pool::from_jobs_for_worker_test(jobs)), cells)
}

fn start<'a>(pool: &'a Pool) -> PoppedJob<'a> {
    PoppedJob::new(pool, pool.try_pop().expect("local ready job"))
}

fn invoke(entry: usize) -> i64 {
    assert_ne!(entry, 0);
    // SAFETY: only successful published entries from constant, with its
    // native C ()->i64 signature; their sealed mappings remain live.
    unsafe { std::mem::transmute::<usize, unsafe extern "C" fn() -> i64>(entry)() }
}

fn success(cell: &JobCell, expected: i64) -> BackendOut {
    assert!(cell.is_done());
    let out = cell.take_out().expect("one published result");
    assert!(!out.dropped);
    assert_eq!(
        invoke(*out.result.as_ref().expect("successful sealed code")),
        expected
    );
    assert!(cell.take_out().is_none(), "publication is consumed once");
    out
}

fn failed(cell: &JobCell) {
    assert!(cell.is_done());
    let out = cell.take_out().expect("one failed result");
    assert!(out.result.is_err());
    assert!(!out.dropped, "backend failure differs from admission drop");
    assert!(out.asm.is_none(), "failed code has no dumpable entry");
    assert!(cell.take_out().is_none());
}

fn drained(pool: &Pool) {
    assert_eq!(pool.running_for_worker_test(), 0);
    assert!(pool.try_pop().is_none());
    assert!(pool.quiesce(Duration::ZERO));
}

#[test]
fn jit_worker_protocol_eight_publish_only_after_whole_batch_seals() {
    let (pool, cells) = fixture(8);
    let observed = Rc::new(Cell::new(0));
    let seen = observed.clone();
    let observed_cells = cells.clone();
    let observed_pool = pool.clone();
    let mut hooks = BatchHooks {
        callback: Some(Box::new(move |event| {
            assert!(observed_cells.iter().all(|cell| !cell.is_done()));
            assert!(!observed_pool.quiesce(Duration::ZERO));
            match event {
                BatchEvent::Prepared { seq, index } => {
                    assert_eq!(seq as usize, index);
                    assert_eq!(observed_pool.running_for_worker_test(), index + 1);
                    seen.set(seen.get() + 1);
                }
                BatchEvent::BeforeFinalize { members } => assert_eq!(members, 8),
                BatchEvent::BeforePublish { members, success } => {
                    assert_eq!(members, 8);
                    assert!(success);
                    assert_eq!(observed_pool.running_for_worker_test(), 8);
                }
            }
        })),
    };
    let mut backend = WorkerBackend::new();
    assert!(serve_batch(&mut backend, &pool, start(&pool), &mut hooks).is_none());
    assert_eq!(observed.get(), 8);
    for (index, cell) in cells.iter().enumerate() {
        let out = success(cell, index as i64 + 100);
        assert_eq!(
            out.backend_cpu_us, 0,
            "entry never charges the upgrade budget"
        );
    }
    drained(&pool);
}

#[test]
fn jit_worker_protocol_ninth_ready_job_is_left_for_next_batch() {
    let (pool, cells) = fixture(9);
    let mut backend = WorkerBackend::new();
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    assert!(cells[..8].iter().all(|cell| cell.is_done()));
    assert!(!cells[8].is_done());
    assert_eq!(pool.running_for_worker_test(), 0);
    assert!(!pool.quiesce(Duration::ZERO), "one job is still queued");
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    for (index, cell) in cells.iter().enumerate() {
        success(cell, index as i64 + 100);
    }
    drained(&pool);
}

#[test]
fn jit_worker_protocol_module_boundary_carries_job_without_reprioritizing_it() {
    let (old_job, old_cell) = job(0, constant(77), JobClass::Entry);
    let old_pool = Pool::from_jobs_for_worker_test(vec![old_job]);
    let mut backend = WorkerBackend::new();
    assert!(
        serve_batch(
            &mut backend,
            &old_pool,
            start(&old_pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    let old_entry = success(&old_cell, 77).result.expect("published old code");
    backend.approach_module_limit_for_worker_test(RegallocChoice::Full);
    let (pool, cells) = fixture(3);
    let carry = serve_batch(
        &mut backend,
        &pool,
        start(&pool),
        &mut BatchHooks::default(),
    )
    .expect("second job crosses the module boundary");
    assert_eq!(carry.job.seq, 1);
    success(&cells[0], 100);
    assert!(!cells[1].is_done());
    assert!(!cells[2].is_done());
    assert_eq!(
        pool.running_for_worker_test(),
        1,
        "carry retains its single dequeue lease"
    );
    assert!(!pool.quiesce(Duration::ZERO));
    let (urgent, urgent_cell) = job(3, constant(999), JobClass::Osr);
    pool.push_ready_for_worker_test(urgent);
    let served = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = served.clone();
    let mut hooks = BatchHooks {
        callback: Some(Box::new(move |event| {
            if let BatchEvent::Prepared { seq, .. } = event {
                observed.borrow_mut().push(seq);
            }
        })),
    };
    assert!(serve_batch(&mut backend, &pool, carry, &mut hooks).is_none());
    assert_eq!(
        *served.borrow(),
        vec![1, 3, 2],
        "carried job stays ahead of new priority work"
    );
    success(&cells[1], 101);
    success(&cells[2], 102);
    success(&urgent_cell, 999);
    assert_eq!(
        invoke(old_entry),
        77,
        "module retirement keeps earlier code live"
    );
    drained(&pool);
}

#[test]
fn jit_worker_protocol_cancelled_jobs_release_exactly_their_dequeue_lease() {
    let (pool, cells) = fixture(3);
    cells[0].cancel();
    cells[2].cancel();
    let mut backend = WorkerBackend::new();
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    assert!(!cells[0].is_done());
    assert!(!cells[2].is_done());
    success(&cells[1], 101);
    drained(&pool);
}

#[test]
fn jit_worker_protocol_all_cancelled_batch_never_finalizes_or_publishes() {
    let (pool, cells) = fixture(8);
    for cell in &cells {
        cell.cancel();
    }
    let mut hooks = BatchHooks {
        callback: Some(Box::new(|_| {
            panic!("cancelled jobs must never reach a compiler/publication hook");
        })),
    };
    let mut backend = WorkerBackend::new();
    assert!(serve_batch(&mut backend, &pool, start(&pool), &mut hooks).is_none());
    assert!(cells.iter().all(|cell| !cell.is_done()));
    drained(&pool);
}

#[test]
fn jit_worker_protocol_cancel_after_prepare_keeps_other_members_usable() {
    let (pool, cells) = fixture(3);
    let cancel = cells[1].clone();
    let mut hooks = BatchHooks {
        callback: Some(Box::new(move |event| {
            if matches!(event, BatchEvent::Prepared { seq: 1, .. }) {
                cancel.cancel();
            }
        })),
    };
    let mut backend = WorkerBackend::new();
    assert!(serve_batch(&mut backend, &pool, start(&pool), &mut hooks).is_none());
    assert!(
        cells[1].is_cancelled(),
        "advisory cancellation persists through publication"
    );
    for (index, cell) in cells.iter().enumerate() {
        success(cell, index as i64 + 100);
    }
    drained(&pool);
    // This layer cannot authorize installation: the mutator's existing
    // token/invalidation regressions test rejection of cancelled results.
}

#[test]
fn jit_worker_protocol_prepare_panic_aborts_prefix_and_next_job_recovers() {
    let (pool, cells) = fixture(3);
    let mut hooks = BatchHooks {
        callback: Some(Box::new(|event| {
            if matches!(event, BatchEvent::Prepared { index: 1, .. }) {
                panic!("owned fault after two real definitions");
            }
        })),
    };
    let mut backend = WorkerBackend::new();
    assert!(serve_batch(&mut backend, &pool, start(&pool), &mut hooks).is_none());
    failed(&cells[0]);
    failed(&cells[1]);
    assert!(!cells[2].is_done());
    assert_eq!(pool.running_for_worker_test(), 0);
    assert!(!pool.quiesce(Duration::ZERO));
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    success(&cells[2], 102);
    drained(&pool);
}

#[test]
fn jit_worker_protocol_nonportable_prepare_error_aborts_successful_prefix() {
    let (first, first_cell) = job(0, constant(200), JobClass::Entry);
    let mut bad = constant(201);
    bad.portable = false;
    let (second, second_cell) = job(1, bad, JobClass::Entry);
    let (third, third_cell) = job(2, constant(202), JobClass::Entry);
    let pool = Pool::from_jobs_for_worker_test(vec![first, second, third]);
    let mut backend = WorkerBackend::new();
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    failed(&first_cell);
    failed(&second_cell);
    assert!(!third_cell.is_done());
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    success(&third_cell, 202);
    drained(&pool);
}

#[test]
fn jit_worker_protocol_finalize_panic_aborts_every_member_and_preserves_old_code() {
    let (old_job, old_cell) = job(0, constant(77), JobClass::Entry);
    let old_pool = Pool::from_jobs_for_worker_test(vec![old_job]);
    let mut backend = WorkerBackend::new();
    assert!(
        serve_batch(
            &mut backend,
            &old_pool,
            start(&old_pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    let old_entry = success(&old_cell, 77).result.expect("old entry");
    let (pool, cells) = fixture(2);
    let mut hooks = BatchHooks {
        callback: Some(Box::new(|event| {
            if matches!(event, BatchEvent::BeforeFinalize { .. }) {
                panic!("owned seal fault");
            }
        })),
    };
    assert!(serve_batch(&mut backend, &pool, start(&pool), &mut hooks).is_none());
    for cell in &cells {
        failed(cell);
    }
    drained(&pool);
    assert_eq!(invoke(old_entry), 77);
    let (later, later_cells) = fixture(1);
    assert!(
        serve_batch(
            &mut backend,
            &later,
            start(&later),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    success(&later_cells[0], 100);
    assert_eq!(
        invoke(old_entry),
        77,
        "reset never overwrites earlier published code"
    );
    drained(&later);
}

#[test]
fn jit_worker_protocol_aborted_disassembly_cannot_leak_into_later_job() {
    let mut payload = constant(50);
    payload.disasm = true;
    let (first, first_cell) = job(0, payload, JobClass::Entry);
    let (second, second_cell) = job(1, constant(51), JobClass::Entry);
    let pool = Pool::from_jobs_for_worker_test(vec![first, second]);
    let mut hooks = BatchHooks {
        callback: Some(Box::new(|event| {
            if matches!(event, BatchEvent::Prepared { .. }) {
                let pending =
                    asm_dump::take_stashed().expect("real disassembly from prepared leaf");
                asm_dump::restash(pending);
                panic!("abort while a real dump is stashed");
            }
        })),
    };
    let mut backend = WorkerBackend::new();
    assert!(serve_batch(&mut backend, &pool, start(&pool), &mut hooks).is_none());
    failed(&first_cell);
    assert!(asm_dump::take_stashed().is_none());
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    assert!(success(&second_cell, 51).asm.is_none());
    drained(&pool);
}

#[test]
fn jit_worker_protocol_interval_shares_never_multiply_the_compile_budget() {
    for total in [0, 1, 7, 8, 9, 1_000_001, u64::MAX] {
        for count in 1..=8 {
            let shares: Vec<_> = (0..count)
                .map(|index| completion::share(total, index, count))
                .collect();
            assert_eq!(shares.iter().sum::<u64>(), total);
            assert!(shares.iter().max().unwrap() - shares.iter().min().unwrap() <= 1);
        }
    }
    let (entry, entry_cell) = job(0, constant(21), JobClass::Entry);
    let (upgrade, upgrade_cell) = job(1, constant(22), JobClass::Upgrade);
    let pool = Pool::from_jobs_for_worker_test(vec![entry, upgrade]);
    let mut backend = WorkerBackend::new();
    assert!(
        serve_batch(
            &mut backend,
            &pool,
            start(&pool),
            &mut BatchHooks::default()
        )
        .is_none()
    );
    assert_eq!(success(&entry_cell, 21).backend_cpu_us, 0);
    success(&upgrade_cell, 22);
    drained(&pool);
}
