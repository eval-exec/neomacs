//! An executed native interior store must retain the hull's growth rejection.
//!
//! All owners stay rooted in this executing Context. The certificate and
//! journal are mutator-local; this does not claim cross-mutator freshness.

use super::*;
use crate::tagged::collection_reads::{
    CompiledJournalMode, capture, force_compiled_journal_for_test, is_observed,
};
use crate::tagged::mutate::LispCollectionRevision;
use crate::tagged::value::TAG_MASK;

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
    }
}

#[test]
fn gen0_native_interior_gap_skips_repeat_shims_without_admitting_growth() {
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut context = context(false);
    let mut other_context = super::context(false);
    crate::tagged::gc::set_tagged_heap(&mut context.tagged_heap);
    let roots = context.save_specpdl_roots();
    let mut owners = std::array::from_fn::<_, 3, _>(|_| {
        context
            .tagged_heap
            .alloc_cons(Value::make_int(97), Value::NIL)
    });
    owners.sort_unstable_by_key(|owner| owner.bits() & !TAG_MASK);
    let [left, target, right] = owners;
    assert!(left.bits() < target.bits() && target.bits() < right.bits());
    for owner in owners {
        context.push_specpdl_root(owner);
    }
    let (_, endpoints) = capture(|| (left.cons_car(), right.cons_car()));
    let endpoints = endpoints.expect("the observed endpoints are retained");
    let growth = context
        .tagged_heap
        .alloc_cons(Value::make_int(31), Value::NIL);
    context.push_specpdl_root(growth);
    let hull = context.tagged_heap.jit_barrier_window_for_test();
    assert!(hull.covers(target.bits() & !TAG_MASK));
    assert!(!hull.covers(growth.bits() & !TAG_MASK));
    assert!(!is_observed(target.bits()) && !is_observed(growth.bits()));
    let setcar = compile_bytecode_function(&store_function(Op::Setcar))
        .expect("the interior setcar leaf compiles");
    let setcdr = compile_bytecode_function(&store_function(Op::Setcdr))
        .expect("the growth setcdr leaf compiles");
    let revision = LispCollectionRevision::current();
    let calls = cons_shims();
    assert_eq!(
        native(&mut context, &setcar, &[target, Value::make_int(12)]),
        Value::make_int(12)
    );
    assert_eq!(
        cons_shims() - calls,
        1,
        "the first interior miss completes cold"
    );
    assert_eq!(LispCollectionRevision::current(), revision);

    let calls = cons_shims();
    let (_, setter_reads) = capture(|| {
        for value in 1..=16 {
            let supplied = Value::make_int(value);
            assert_eq!(native(&mut context, &setcar, &[target, supplied]), supplied);
            assert_eq!(native(&mut context, &setcdr, &[growth, supplied]), supplied);
        }
    });
    assert_eq!(
        cons_shims() - calls,
        0,
        "a retained interior exclusion and the exterior growth rejection must both stay native"
    );
    assert_eq!(LispCollectionRevision::current(), revision);
    assert!(endpoints.unchanged());
    assert!(
        setter_reads
            .expect("supplied-value setters add no dependency")
            .unchanged()
    );
    assert!(!is_observed(target.bits()) && !is_observed(growth.bits()));
    assert_eq!(target.cons_car(), Value::make_int(16));
    assert_eq!(growth.cons_cdr(), Value::make_int(16));

    // Off clears this heap's published secondary words while retaining the
    // sticky ledger. Restore Observed on a different installed heap, then
    // reinstall the original heap: both native predicates must be rederived.
    force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
    crate::tagged::gc::set_tagged_heap(&mut other_context.tagged_heap);
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    crate::tagged::gc::set_tagged_heap(&mut context.tagged_heap);
    let calls = cons_shims();
    native(&mut context, &setcar, &[target, Value::make_int(17)]);
    native(&mut context, &setcdr, &[growth, Value::make_int(17)]);
    assert_eq!(
        cons_shims() - calls,
        0,
        "reinstallation restores the interior exemption and the growth rejection"
    );
    assert_eq!(LispCollectionRevision::current(), revision);
    assert!(endpoints.unchanged());

    let (_, target_reads) = capture(|| target.cons_car());
    let target_reads = target_reads.expect("the first actual target read closes its exclusion");
    let calls = cons_shims();
    native(&mut context, &setcar, &[target, Value::make_int(23)]);
    assert_eq!(
        cons_shims() - calls,
        1,
        "the newly observed store journals cold"
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );
    assert!(!target_reads.unchanged());
    assert!(
        endpoints.unchanged(),
        "the target write leaves both endpoint reads valid"
    );
    context.restore_specpdl_roots(roots);
}

#[test]
fn gen0_native_exact_cons_and_large_gap_both_survive_alternating_stores() {
    use crate::tagged::collection_reads::compiled_observation_gate;
    use crate::tagged::gc::{
        HEAP_JIT_EXCLUSION_LEN, HEAP_JIT_EXCLUSION_LO, HEAP_JIT_SINGLE_CONS, TaggedHeap,
        has_collection_observations,
    };
    use crate::tagged::header::ConsCell;

    struct RestoreGeneration(Option<std::ffi::OsString>);
    impl Drop for RestoreGeneration {
        fn drop(&mut self) {
            // Nextest owns this isolated process; no other heap constructor
            // can race this temporary immutable GEN0 selection.
            unsafe {
                match self.0.take() {
                    Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }

    let published_word = |context: &Context, offset: usize| {
        // SAFETY: these constants name the contiguous byte-Cell protocol
        // words in this live heap. This test's mutator owns the Context, and
        // no callback, collection or native execution interleaves this read.
        unsafe {
            let heap = (&*context.tagged_heap as *const TaggedHeap).cast::<u8>();
            heap.add(offset).cast::<usize>().read_unaligned()
        }
    };
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    // The shared generational helper collects immediately, leaving recycled
    // cells whose sorted addresses do not establish a tiny first interval.
    // This private Context retains its untouched bump-allocation sequence.
    let mut context = {
        let _restore = RestoreGeneration(std::env::var_os("NEOVM_GC_GENERATIONAL"));
        unsafe { std::env::remove_var("NEOVM_GC_GENERATIONAL") };
        Context::new()
    };
    context.gc_stress = false;
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert!(!context.tagged_heap.generational_enabled());
    crate::tagged::gc::set_tagged_heap(&mut context.tagged_heap);
    let roots = context.save_specpdl_roots();
    let mut owners = std::array::from_fn::<_, 9, _>(|_| {
        context
            .tagged_heap
            .alloc_cons(Value::make_int(97), Value::NIL)
    });
    owners.sort_unstable_by_key(|owner| owner.bits() & !TAG_MASK);
    for pair in owners.windows(2) {
        assert_eq!(
            (pair[1].bits() & !TAG_MASK) - (pair[0].bits() & !TAG_MASK),
            size_of::<ConsCell>(),
            "the native fixture must own nine genuinely contiguous cells",
        );
    }
    let small_len = (owners[2].bits() & !TAG_MASK) - (owners[0].bits() & !TAG_MASK) - 1;
    let large_len = (owners[8].bits() & !TAG_MASK) - (owners[2].bits() & !TAG_MASK) - 1;
    assert!(small_len <= 2 * size_of::<ConsCell>());
    assert!(large_len > 2 * size_of::<ConsCell>());
    for owner in owners {
        context.push_specpdl_root(owner);
    }
    let (_, endpoints) = capture(|| {
        (
            owners[0].cons_car(),
            owners[2].cons_car(),
            owners[8].cons_car(),
        )
    });
    let endpoints = endpoints.expect("live endpoints bound both empty intervals");
    assert!(
        has_collection_observations(),
        "the native compiler sees real published observations"
    );
    let setcar = compile_bytecode_function(&store_function(Op::Setcar)).expect("setcar compiles");
    let setcdr = compile_bytecode_function(&store_function(Op::Setcdr)).expect("setcdr compiles");
    let revision = LispCollectionRevision::current();
    native(&mut context, &setcar, &[owners[1], Value::make_int(12)]);
    let old_address = owners[1].bits() & !TAG_MASK;
    let initial = compiled_observation_gate(context.tagged_heap.collection_dump_window());
    assert_eq!(
        initial.single_cons, old_address,
        "the first tiny-gap miss prepares an exact exemption"
    );
    assert_eq!(
        published_word(&context, HEAP_JIT_SINGLE_CONS),
        old_address,
        "the installed heap publishes that exact native word"
    );
    native(&mut context, &setcdr, &[owners[5], Value::make_int(12)]);
    let learned = compiled_observation_gate(context.tagged_heap.collection_dump_window());
    assert_eq!(
        learned.single_cons, old_address,
        "the large-gap miss preserves the exact exemption"
    );
    assert_eq!(
        published_word(&context, HEAP_JIT_SINGLE_CONS),
        old_address,
        "the second publication retains the exact native word"
    );
    let gap = learned
        .excluded
        .expect("the larger empty interval is retained");
    assert!(gap.covers(owners[5].bits() & !TAG_MASK));
    assert!(!gap.covers(old_address));
    assert_eq!(
        published_word(&context, HEAP_JIT_EXCLUSION_LO),
        gap.lo(),
        "native gap start matches the prepared gate"
    );
    assert_eq!(
        published_word(&context, HEAP_JIT_EXCLUSION_LEN),
        gap.len(),
        "native gap length matches the prepared gate"
    );
    let calls = cons_shims();
    for value in 1..=16 {
        let value = Value::make_int(value);
        native(&mut context, &setcar, &[owners[1], value]);
        native(&mut context, &setcdr, &[owners[5], value]);
    }
    assert_eq!(
        cons_shims() - calls,
        0,
        "both negative proofs remain native"
    );
    assert_eq!(LispCollectionRevision::current(), revision);
    assert!(endpoints.unchanged());
    let (_, newly_read) = capture(|| owners[1].cons_car());
    let newly_read = newly_read.expect("reading the exact owner closes its exemption");
    let calls = cons_shims();
    native(&mut context, &setcar, &[owners[1], Value::make_int(23)]);
    assert_eq!(cons_shims() - calls, 1);
    assert!(!newly_read.unchanged());
    assert!(endpoints.unchanged());
    context.restore_specpdl_roots(roots);
}

#[test]
fn gen0_native_two_store_leaf_closes_only_the_first_read_owner() {
    use crate::emacs_core::value::LambdaParams;
    use crate::tagged::collection_reads::compiled_observation_gate;
    use crate::tagged::header::ConsCell;

    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    // No initial collection: both physical empty intervals come from this
    // retained bump sequence rather than unrelated recycled cells.
    let mut context = crate::test_utils::with_legacy_gc(Context::new);
    context.gc_stress = false;
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert!(!context.tagged_heap.generational_enabled());
    crate::tagged::gc::set_tagged_heap(&mut context.tagged_heap);
    let roots = context.save_specpdl_roots();
    let mut owners = std::array::from_fn::<_, 9, _>(|_| {
        context
            .tagged_heap
            .alloc_cons(Value::make_int(97), Value::NIL)
    });
    owners.sort_unstable_by_key(|owner| owner.bits() & !TAG_MASK);
    for pair in owners.windows(2) {
        assert_eq!(
            (pair[1].bits() & !TAG_MASK) - (pair[0].bits() & !TAG_MASK),
            size_of::<ConsCell>(),
            "the exact and range owners must occupy real separate empty intervals",
        );
    }
    for owner in owners {
        context.push_specpdl_root(owner);
    }
    let (_, endpoints) = capture(|| {
        (
            owners[0].cons_car(),
            owners[2].cons_car(),
            owners[8].cons_car(),
        )
    });
    let endpoints = endpoints.expect("all endpoint owners remain rooted");
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (1..=3).map(crate::emacs_core::intern::SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.ops = vec![
        Op::StackRef(2), // [exact range value exact]
        Op::StackRef(1), // [exact range value exact value]
        Op::Setcar,
        Op::DiscardN(1),
        Op::StackRef(1), // [exact range value range]
        Op::StackRef(1), // [exact range value range value]
        Op::Setcdr,
        Op::Return,
    ];
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    let leaf = compile_bytecode_function(&function).expect("both explicit stores compile together");
    let revision = LispCollectionRevision::current();
    assert_eq!(
        native(
            &mut context,
            &leaf,
            &[owners[1], owners[5], Value::make_int(12)]
        ),
        Value::make_int(12)
    );
    let learned = compiled_observation_gate(context.tagged_heap.collection_dump_window());
    assert_eq!(learned.single_cons, owners[1].bits() & !TAG_MASK);
    assert!(
        learned
            .excluded
            .expect("the second owner has a range proof")
            .covers(owners[5].bits() & !TAG_MASK)
    );
    assert!(!is_observed(owners[1].bits()) && !is_observed(owners[5].bits()));
    let (_, setter_reads) = capture(|| {
        for value in 1..=16 {
            let supplied = Value::make_int(value);
            assert_eq!(
                native(&mut context, &leaf, &[owners[1], owners[5], supplied]),
                supplied
            );
        }
    });
    assert_eq!(LispCollectionRevision::current(), revision);
    assert!(endpoints.unchanged());
    assert!(
        setter_reads
            .expect("supplied results add no owner dependency")
            .unchanged()
    );

    let (read, newly_read) = capture(|| owners[1].cons_car());
    assert_eq!(read, Value::make_int(16));
    let newly_read = newly_read.expect("a real read closes the exact exemption");
    assert!(is_observed(owners[1].bits()) && !is_observed(owners[5].bits()));
    assert_eq!(
        native(
            &mut context,
            &leaf,
            &[owners[1], owners[5], Value::make_int(23)]
        ),
        Value::make_int(23)
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );
    assert!(!newly_read.unchanged());
    assert!(endpoints.unchanged());
    assert!(
        !is_observed(owners[5].bits()),
        "the other store still adds no read dependency"
    );
    assert_eq!(owners[1].cons_car(), Value::make_int(23));
    assert_eq!(owners[5].cons_cdr(), Value::make_int(23));
    context.restore_specpdl_roots(roots);
}
