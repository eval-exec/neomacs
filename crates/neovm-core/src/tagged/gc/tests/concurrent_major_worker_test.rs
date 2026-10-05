//! Worker-local P-all and symbol handoff, including bounded early-stop exits.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::intern_uninterned;
use crate::emacs_core::value::{HashTableTest, LambdaParams, LispHashTable};
use crate::heap_types::LispString;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};

struct ScratchRoots(usize);
impl ScratchRoots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }
    fn keep(&self, value: TaggedValue) -> TaggedValue {
        push_scratch_gc_root(value);
        value
    }
}
impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaimKind {
    String,
    Float,
    Vector,
    ByteCode,
}
impl ClaimKind {
    const ALL: [Self; 4] = [Self::String, Self::Float, Self::Vector, Self::ByteCode];

    fn allocate(self, heap: &mut TaggedHeap) -> TaggedValue {
        match self {
            Self::String => heap.alloc_string(LispString::from_utf8("major worker claim")),
            Self::Float => heap.alloc_float(4.25),
            Self::Vector => heap.alloc_vector(vec![TaggedValue::fixnum(7)]),
            Self::ByteCode => {
                heap.alloc_bytecode(ByteCodeFunction::new(LambdaParams::simple(vec![])))
            }
        }
    }

    fn counter(self, job: &ConcurrentClaimJob) -> usize {
        match self {
            Self::String => job.str_claimed.load(Ordering::Relaxed),
            Self::Float => job.float_claimed.load(Ordering::Relaxed),
            Self::Vector => job.vec_claimed.load(Ordering::Relaxed),
            Self::ByteCode => job.bc_claimed.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Age {
    Young,
    Old,
    Permanent,
}
impl Age {
    const ALL: [Self; 3] = [Self::Young, Self::Old, Self::Permanent];
}

fn header(value: TaggedValue) -> *mut GcHeader {
    TaggedHeap::value_heap_addr(value).unwrap() as *mut GcHeader
}

fn set_age(value: TaggedValue, age: Age) {
    // Unit claim fixtures are exclusively owned here, before worker dispatch.
    unsafe {
        match age {
            Age::Young => {}
            Age::Old => (*header(value)).tenured = true,
            Age::Permanent => (*header(value)).make_permanent(),
        }
    }
}

fn claim_job(heap: &TaggedHeap, major: bool) -> ConcurrentClaimJob {
    ConcurrentClaimJob {
        major,
        parity: heap.mark_parity.flip(),
        pages: PageSnapshot::BaseSets {
            cons: heap.cons_blocks.iter().map(ConsBlock::base_addr).collect(),
            string: heap
                .string_arena
                .pages
                .iter()
                .map(|page| page.base_addr())
                .collect(),
            float: heap
                .float_arena
                .pages
                .iter()
                .map(|page| page.base_addr())
                .collect(),
            vector: heap
                .vector_arena
                .pages
                .iter()
                .map(|page| page.base_addr())
                .collect(),
            bytecode: heap
                .bytecode_arena
                .pages
                .iter()
                .map(|page| page.base_addr())
                .collect(),
        },
        dump_lo: heap.dump_addr_lo,
        dump_hi: heap.dump_addr_hi,
        drop_dump_children: false,
        str_claimed: Arc::new(AtomicUsize::new(0)),
        float_claimed: Arc::new(AtomicUsize::new(0)),
        vec_claimed: Arc::new(AtomicUsize::new(0)),
        bc_claimed: Arc::new(AtomicUsize::new(0)),
        subr_dropped: Arc::new(AtomicUsize::new(0)),
    }
}

struct WorkerHarness {
    job: ConcurrentMarkJob,
    result: std::sync::mpsc::Receiver<ConcurrentMarkResult>,
    deferred: Arc<Mutex<Vec<TaggedValue>>>,
    satb: Arc<Mutex<Vec<TaggedValue>>>,
}
impl WorkerHarness {
    fn new(heap: &TaggedHeap, major: bool) -> Self {
        let (exited, result) = std::sync::mpsc::channel();
        let deferred = Arc::new(Mutex::new(Vec::new()));
        let satb = Arc::new(Mutex::new(Vec::new()));
        Self {
            job: ConcurrentMarkJob {
                gray: Vec::new(),
                claims: claim_job(heap, major),
                satb: satb.clone(),
                deferred: deferred.clone(),
                done: Arc::new(AtomicBool::new(false)),
                // Deterministic stop exercises the actual loop and epilogue.
                stop: Arc::new(AtomicBool::new(true)),
                wake: Arc::new((Mutex::new(()), Condvar::new())),
                exited,
                obarray: None,
                vectors: None,
                mapped_cons_ranges: None,
                mapped_veclikes: None,
            },
            result,
            deferred,
            satb,
        }
    }

    fn run(self) -> (ConcurrentMarkResult, Vec<TaggedValue>) {
        run_concurrent_mark(self.job);
        let result = self.result.recv().expect("worker result missing");
        assert!(
            self.result.try_recv().is_err(),
            "worker must send only once"
        );
        let deferred = std::mem::take(&mut *self.deferred.lock().unwrap());
        (result, deferred)
    }
}

fn snapshot(
    heap: &mut TaggedHeap,
    vector: TaggedValue,
) -> crate::tagged::header::VectorScanSnapshot {
    // SAFETY: the test owns the live vector and admits its only heap writer.
    let object = unsafe { &*(vector.as_veclike_ptr().unwrap() as *const VectorObj) };
    // SAFETY: capture occurs before the test starts its marker job.
    let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(heap) };
    let mut snapshot = crate::tagged::header::VectorScanSnapshot::with_capacity(1, &world);
    // SAFETY: the test owns this vector in the admitted heap until its
    // worker finishes; no backing mutation or replacement occurs.
    unsafe { snapshot.push(object.data.scan_entry()) };
    snapshot
}

fn symbol(label: &str) -> TaggedValue {
    TaggedValue::from_sym_id(intern_uninterned(label))
}

fn assert_symbol_once(result: &ConcurrentMarkResult, symbol: TaggedValue) {
    let crate::tagged::value::ValueKind::Symbol(id) = symbol.kind() else {
        panic!("expected a bare symbol");
    };
    assert_eq!(result.symbols.iter().filter(|&&seen| seen == id).count(), 1);
}

#[test]
fn generational_worker_major_claims_promote_only_young_headers() {
    for kind in ClaimKind::ALL {
        for age in Age::ALL {
            let mut heap = TaggedHeap::new();
            heap.generational.enabled = true;
            set_tagged_heap(&mut heap);
            let roots = ScratchRoots::new();
            let value = roots.keep(kind.allocate(&mut heap));
            heap.close_alloc_regions();
            set_age(value, age);
            let job = claim_job(&heap, true);
            let before = unsafe { (*header(value)).raw_mark() };
            let mut gray = Vec::new();
            let mut logs = WorkerMarkLogs::default();
            assert!(concurrent_try_mark_owned_logged::<true>(
                value, &job, &mut gray, &mut logs
            ));
            assert_eq!(
                kind.counter(&job),
                usize::from(age != Age::Permanent),
                "{kind:?} {age:?}"
            );
            let expected = if age == Age::Young {
                vec![header(value) as usize]
            } else {
                vec![]
            };
            assert_eq!(logs.result.promo, expected, "{kind:?} {age:?}");
            if age == Age::Permanent {
                assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
            } else {
                assert!(unsafe { (*header(value)).is_marked_at(job.parity) });
            }
            logs.result.symbols.clear();
            gray.clear();
            assert!(concurrent_try_mark_owned_logged::<true>(
                value, &job, &mut gray, &mut logs
            ));
            assert_eq!(kind.counter(&job), usize::from(age != Age::Permanent));
            assert_eq!(
                logs.result.promo, expected,
                "rediscovery must not promote twice"
            );
            assert!(gray.is_empty());
        }
    }
}

#[test]
fn generational_worker_legacy_claim_parity_and_tenured_rules_are_unchanged() {
    for kind in ClaimKind::ALL {
        for age in Age::ALL {
            let mut heap = TaggedHeap::new();
            heap.generational.enabled = false;
            set_tagged_heap(&mut heap);
            let roots = ScratchRoots::new();
            let value = roots.keep(kind.allocate(&mut heap));
            heap.close_alloc_regions();
            set_age(value, age);
            let job = claim_job(&heap, false);
            let before = unsafe { (*header(value)).raw_mark() };
            let mut gray = Vec::new();
            let mut logs = WorkerMarkLogs::default();
            assert!(concurrent_try_mark_owned_logged::<false>(
                value, &job, &mut gray, &mut logs
            ));
            let claims = usize::from(kind == ClaimKind::String || age == Age::Young);
            assert_eq!(kind.counter(&job), claims, "{kind:?} {age:?}");
            if claims == 0 {
                assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
            } else {
                assert!(unsafe { (*header(value)).is_marked_at(job.parity) });
            }
            assert!(logs.result.promo.is_empty());
            assert!(logs.result.symbols.is_empty());
        }
    }
}

#[test]
fn generational_worker_born_black_claims_have_no_promo_or_child_handoff() {
    for kind in ClaimKind::ALL {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let value = roots.keep(kind.allocate(&mut heap));
        heap.close_alloc_regions();
        let job = claim_job(&heap, true);
        unsafe { (*header(value)).set_marked(job.parity) };
        let mut gray = Vec::new();
        let mut logs = WorkerMarkLogs::default();
        assert!(concurrent_try_mark_owned_logged::<true>(
            value, &job, &mut gray, &mut logs
        ));
        assert_eq!(kind.counter(&job), 0);
        assert!(logs.result.promo.is_empty());
        assert!(logs.result.symbols.is_empty());
        assert!(gray.is_empty());
    }
}

#[test]
fn generational_worker_born_black_bytecode_does_not_read_its_value_children() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let child = roots.keep(heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::fixnum(2)));
    let key = symbol("u34-worker-unpublished-bc-child");
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.constants = vec![child, key].into();
    function.arglist = key;
    let value = roots.keep(heap.alloc_bytecode(function));
    heap.close_alloc_regions();
    let job = claim_job(&heap, true);
    unsafe { (*header(value)).set_marked(job.parity) };
    let mut logs = WorkerMarkLogs::default();
    let mut gray = Vec::new();
    assert!(concurrent_try_mark_owned_logged::<true>(
        value, &job, &mut gray, &mut logs
    ));
    assert_eq!(job.bc_claimed.load(Ordering::Relaxed), 0);
    assert!(gray.is_empty());
    assert!(logs.result.promo.is_empty());
    assert!(logs.result.symbols.is_empty());
}

#[test]
fn generational_worker_refuses_interval_strings_and_snapshot_misses_before_claiming() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let string = roots.keep(heap.alloc_string(LispString::from_utf8("has interval")));
    crate::tagged::mutate::with_string_text_props_mut(string, |_| {});
    let vector = roots.keep(heap.alloc_vector(vec![TaggedValue::fixnum(8)]));
    let boxed = roots.keep(heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq)));
    heap.close_alloc_regions();
    let mut job = claim_job(&heap, true);
    if let PageSnapshot::BaseSets { vector, .. } = &mut job.pages {
        vector.clear();
    }
    let mut gray = Vec::new();
    let mut logs = WorkerMarkLogs::default();
    for value in [string, vector, boxed] {
        let before = unsafe { (*header(value)).raw_mark() };
        assert!(!concurrent_try_mark_owned_logged::<true>(
            value, &job, &mut gray, &mut logs
        ));
        assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
    }
    assert!(gray.is_empty());
    assert!(logs.result.promo.is_empty());
    assert!(logs.result.symbols.is_empty());
    assert_eq!(job.str_claimed.load(Ordering::Relaxed), 0);
    assert_eq!(job.vec_claimed.load(Ordering::Relaxed), 0);
}

#[test]
fn generational_worker_snapshot_misses_never_claim_or_promote_any_header_class() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let mut values = Vec::new();
    for kind in ClaimKind::ALL {
        values.push(roots.keep(kind.allocate(&mut heap)));
    }
    heap.close_alloc_regions();
    let mut job = claim_job(&heap, true);
    job.pages = PageSnapshot::BaseSets {
        cons: FxHashSet::default(),
        string: FxHashSet::default(),
        float: FxHashSet::default(),
        vector: FxHashSet::default(),
        bytecode: FxHashSet::default(),
    };
    let mut gray = Vec::new();
    let mut logs = WorkerMarkLogs::default();
    for value in values {
        let before = unsafe { (*header(value)).raw_mark() };
        assert!(!concurrent_try_mark_owned_logged::<true>(
            value, &job, &mut gray, &mut logs
        ));
        assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
    }
    assert!(gray.is_empty());
    assert!(logs.result.promo.is_empty());
    assert!(logs.result.symbols.is_empty());
    for kind in ClaimKind::ALL {
        assert_eq!(kind.counter(&job), 0);
    }
}

#[test]
fn generational_worker_bytecode_routes_each_symbol_field_and_keeps_legacy_heap_filter() {
    for major in [false, true] {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let child = roots.keep(heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(4)));
        let symbols: Vec<TaggedValue> = (0..6)
            .map(|i| symbol(&format!("u34-worker-bc-{i}")))
            .collect();
        let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        function.arglist = symbols[0];
        function.constants = vec![symbols[1], child, TaggedValue::fixnum(7), symbols[0]].into();
        function.env = Some(symbols[2]);
        function.doc_form = Some(symbols[3]);
        function.interactive = Some(symbols[4]);
        function.extra_slots = vec![symbols[5], symbols[0]];
        let value = roots.keep(heap.alloc_bytecode(function));
        heap.close_alloc_regions();
        let job = claim_job(&heap, major);
        let mut gray = Vec::new();
        let mut logs = WorkerMarkLogs::default();
        let handled = if major {
            concurrent_try_mark_owned_logged::<true>(value, &job, &mut gray, &mut logs)
        } else {
            concurrent_try_mark_owned_logged::<false>(value, &job, &mut gray, &mut logs)
        };
        assert!(handled);
        assert_eq!(gray, [child]);
        if major {
            assert_eq!(logs.result.promo, [header(value) as usize]);
            for symbol in &symbols {
                assert_symbol_once(&logs.result, *symbol);
            }
        } else {
            assert!(logs.result.promo.is_empty());
            assert!(logs.result.symbols.is_empty());
        }
    }
}

#[test]
fn generational_worker_vector_and_obarray_snapshots_deduplicate_bare_symbols() {
    for major in [false, true] {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let symbols = [
            symbol("u34-worker-value"),
            symbol("u34-worker-function"),
            symbol("u34-worker-plist"),
        ];
        let child = roots.keep(heap.alloc_float(6.5));
        let vector = roots
            .keep(heap.alloc_vector(vec![symbols[0], symbols[1], symbols[2], symbols[0], child]));
        let mut obarray = crate::emacs_core::symbol::Obarray::new();
        obarray.set_symbol_value("u34-worker-owner", symbols[0]);
        obarray.set_symbol_function("u34-worker-owner", symbols[1]);
        let owner = crate::emacs_core::intern::intern("u34-worker-owner");
        obarray.set_symbol_plist_id(owner, symbols[2]);
        heap.close_alloc_regions();
        let mut harness = WorkerHarness::new(&heap, major);
        harness.job.vectors = Some(snapshot(&mut heap, vector));
        // SAFETY: the test owns the matching obarray and captures before launch.
        let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
        harness.job.obarray = Some(obarray.scan_snapshot(&world));
        let (result, deferred) = harness.run();
        assert!(deferred.is_empty());
        if major {
            assert_eq!(result.promo, [header(child) as usize]);
            for symbol in symbols {
                assert_symbol_once(&result, symbol);
            }
        } else {
            assert!(result.promo.is_empty());
            assert!(result.symbols.is_empty());
        }
    }
}

#[test]
fn generational_worker_gray_and_satb_symbols_reach_major_result_only() {
    for major in [false, true] {
        let heap = TaggedHeap::new();
        let first = symbol("u34-worker-gray");
        let second = symbol("u34-worker-satb");
        let mut harness = WorkerHarness::new(&heap, major);
        harness.job.gray.push(first);
        harness.satb.lock().unwrap().extend([first, second]);
        let (result, deferred) = harness.run();
        assert!(deferred.is_empty());
        assert!(result.promo.is_empty());
        if major {
            assert_symbol_once(&result, first);
            assert_symbol_once(&result, second);
        } else {
            assert!(result.symbols.is_empty());
        }
    }
}

#[test]
fn generational_worker_first_partition_symbols_survive_in_span_child_drops() {
    for major in [false, true] {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let image = super::super::fake_image::FakeImage::leak(false);
        let cons = roots.keep(image.register_cons(&mut heap));
        let vector = roots.keep(image.register_vector(&mut heap));
        let key = symbol("u34-worker-image-symbol");
        assert!(crate::tagged::mutate::set_cons_car(cons, key));
        assert!(crate::tagged::mutate::set_cons_cdr(cons, vector));
        assert!(crate::tagged::mutate::set_vector_slot(vector, 0, key));
        assert!(crate::tagged::mutate::set_vector_slot(vector, 1, cons));
        heap.close_alloc_regions();
        let mut harness = WorkerHarness::new(&heap, major);
        harness.job.claims.drop_dump_children = true;
        harness.job.mapped_cons_ranges = Some(vec![(cons.xcons_ptr() as usize, 1)]);
        harness.job.mapped_veclikes = Some(vec![vector.as_veclike_ptr().unwrap() as usize]);
        let (result, deferred) = harness.run();
        assert!(result.promo.is_empty());
        assert!(deferred.iter().all(|value| !value.is_heap_object()));
        if major {
            assert_symbol_once(&result, key);
            assert!(deferred.is_empty());
        } else {
            assert!(result.symbols.is_empty());
            assert!(deferred.contains(&key));
        }
    }
}

#[test]
fn generational_worker_early_cdr_stop_retains_initial_claims_symbols_and_tail() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let key = symbol("u34-worker-stop-prefix");
    let terminal = symbol("u34-worker-stop-terminal");
    let mut spine = terminal;
    for _ in 0..2048 {
        spine = roots.keep(heap.alloc_cons(key, spine));
    }
    let float = roots.keep(heap.alloc_float(1.25));
    let vector = roots.keep(heap.alloc_vector(vec![key, float]));
    heap.close_alloc_regions();
    let mut harness = WorkerHarness::new(&heap, true);
    harness.job.gray.push(spine);
    harness.job.vectors = Some(snapshot(&mut heap, vector));
    let parity = harness.job.claims.parity;
    let (result, deferred) = harness.run();
    assert_eq!(result.promo, [header(float) as usize]);
    assert_symbol_once(&result, key);
    let marked: usize = heap.cons_blocks.iter().map(ConsBlock::count_marked).sum();
    assert!(
        marked > 0 && marked < 2048,
        "cdr stop must leave a white tail"
    );
    let tail = deferred
        .iter()
        .find(|value| value.is_cons())
        .copied()
        .expect("missing residual tail");
    let base = ConsBlock::block_base_for_ptr(tail.xcons_ptr());
    let block = heap
        .cons_blocks
        .iter()
        .find(|block| block.base_addr() == base)
        .unwrap();
    assert!(!block.is_marked_ptr(tail.xcons_ptr()));
    assert!(unsafe { (*header(float)).is_marked_at(parity) });
}

#[test]
fn generational_worker_outer_stop_preserves_unprocessed_gray_and_owned_logs() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let value = roots.keep(heap.alloc_float(8.5));
    let key = symbol("u34-worker-outer-stop");
    heap.close_alloc_regions();
    let mut harness = WorkerHarness::new(&heap, true);
    harness.job.gray.extend(std::iter::repeat_n(value, 2048));
    // Both are visited before the quantum reaches the repeated float tail.
    harness.job.gray.push(key);
    let (result, deferred) = harness.run();
    assert_eq!(result.promo, [header(value) as usize]);
    assert_symbol_once(&result, key);
    assert!(!deferred.is_empty());
    assert!(deferred.iter().all(|&item| item == value));
}
