//! GEN0 native stores journal only owners published by actual observations.
//!
//! Certificates and revisions remain local to the executing mutator. These
//! tests switch Contexts on one thread and retain object storage explicitly;
//! they do not claim cross-mutator certificate coherence or race reclamation.

use super::*;
use crate::heap_types::LispString;
use crate::tagged::collection_reads::{
    CompiledJournalMode, capture, force_compiled_journal_for_test, is_observed,
};
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
use crate::tagged::mutate::LispCollectionRevision;

struct JournalMode;

impl JournalMode {
    fn observed() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for JournalMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

#[derive(Clone, Copy, Debug)]
enum StoreKind {
    Setcar,
    Setcdr,
    ConstantSetcar,
    VectorAset,
    RecordAset,
    UnibyteStringAset,
    MultibyteStringAset,
    BlvDefault,
    BlvLocal,
}

impl StoreKind {
    fn allocate(self, heap: &mut TaggedHeap) -> Value {
        match self {
            Self::Setcar | Self::Setcdr | Self::ConstantSetcar => {
                heap.alloc_cons(Value::NIL, Value::NIL)
            }
            Self::VectorAset => heap.alloc_vector(vec![Value::NIL]),
            Self::RecordAset => {
                heap.alloc_record(vec![Value::symbol("observed-journal"), Value::NIL])
            }
            Self::UnibyteStringAset | Self::MultibyteStringAset => heap.alloc_string(
                LispString::new("a".into(), matches!(self, Self::MultibyteStringAset)),
            ),
            Self::BlvDefault | Self::BlvLocal => {
                panic!("BLV owners come from their localized symbol")
            }
        }
    }

    fn read(self, owner: Value) -> Value {
        match self {
            Self::Setcar | Self::ConstantSetcar => owner.cons_car(),
            Self::Setcdr | Self::BlvDefault | Self::BlvLocal => owner.cons_cdr(),
            Self::VectorAset => owner.as_vector_data().expect("vector")[0],
            Self::RecordAset => owner.as_record_data().expect("record")[1],
            Self::UnibyteStringAset | Self::MultibyteStringAset => Value::make_int(i64::from(
                owner.as_str_owned().expect("string").as_bytes()[0],
            )),
        }
    }

    fn index(self) -> usize {
        usize::from(matches!(self, Self::RecordAset))
    }

    fn expected(self, supplied: Value) -> Value {
        if matches!(self, Self::ConstantSetcar) {
            Value::make_int(12)
        } else {
            supplied
        }
    }

    fn args(self, owner: Value, supplied: Value) -> Vec<Value> {
        match self {
            Self::Setcar | Self::Setcdr => vec![owner, supplied],
            Self::ConstantSetcar => vec![owner],
            Self::BlvDefault | Self::BlvLocal => vec![supplied],
            _ => vec![owner, Value::make_int(self.index() as i64), supplied],
        }
    }

    fn shim_calls(self) -> usize {
        match self {
            Self::Setcar | Self::Setcdr | Self::ConstantSetcar => cons_shims(),
            Self::BlvDefault | Self::BlvLocal => {
                super::super::shims::VARSET_SHIM_CALLS.with(|count| count.get())
            }
            _ => super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get()),
        }
    }
}

const STORE_KINDS: [StoreKind; 9] = [
    StoreKind::Setcar,
    StoreKind::Setcdr,
    StoreKind::ConstantSetcar,
    StoreKind::VectorAset,
    StoreKind::RecordAset,
    StoreKind::UnibyteStringAset,
    StoreKind::MultibyteStringAset,
    StoreKind::BlvDefault,
    StoreKind::BlvLocal,
];

#[test]
fn gen0_cons_shim_rejects_outside_mutator_observations_before_exact_query() {
    let _journal = JournalMode::observed();
    let mut context = context(false);
    let observed = context
        .tagged_heap
        .alloc_cons(Value::make_int(1), Value::NIL);
    let old_child = context
        .tagged_heap
        .alloc_cons(Value::make_int(2), Value::NIL);
    let target = context.tagged_heap.alloc_cons(old_child, old_child);
    for owner in [observed, old_child, target] {
        context.push_specpdl_root(owner);
    }
    let (_, certificate) = capture(|| observed.cons_car());
    let certificate = certificate.expect("one retained observation");
    let (lo, hi) = crate::tagged::collection_reads::compiled_observation_window();
    let address = target.bits() & !crate::tagged::value::TAG_MASK;
    assert!(address < lo || address >= hi);
    assert!(!is_observed(target.bits()));
    // A concurrent mark still makes the ordinary native gate ALL. Its SATB
    // work must run even when this mutator's read envelope rejects the owner.
    context.tagged_heap.set_concurrent_active_for_test(true);
    let queries = super::super::dispatch::CONS_OBSERVATION_QUERIES.with(|count| count.get());
    let revision = LispCollectionRevision::current();
    let vmctx = (&mut context as *mut Context).cast::<u8>();
    assert_eq!(
        super::super::dispatch::neovm_jit_setcar(
            vmctx,
            target.bits() as i64,
            Value::NIL.bits() as i64
        ),
        Value::NIL.bits() as i64,
    );
    assert_eq!(
        super::super::dispatch::neovm_jit_setcdr(
            vmctx,
            target.bits() as i64,
            Value::NIL.bits() as i64
        ),
        Value::NIL.bits() as i64,
    );
    assert!(
        context
            .tagged_heap
            .take_satb_shared_for_test()
            .contains(&old_child),
        "the ordinary SATB barrier retains the overwritten child",
    );
    context.tagged_heap.set_concurrent_active_for_test(false);
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        0
    );
    assert!(certificate.unchanged());
    assert_eq!(target.cons_car(), Value::NIL);
    assert_eq!(target.cons_cdr(), Value::NIL);
    assert_eq!(
        super::super::dispatch::CONS_OBSERVATION_QUERIES.with(|count| count.get()) - queries,
        0,
        "ordinary GC hits outside local read owners skip exact metadata",
    );
}

fn check_mapped_blv_binding() {
    let _journal = JournalMode::observed();
    let mut source = blv_context(false);
    let source_owner = blv_cell(&source, false);
    source
        .obarray_mut()
        .set_symbol_value("fx1-mapped-default-owner", source_owner);
    let directory = tempfile::tempdir().expect("private dump fixture");
    let image = directory.path().join("mapped-blv.pdump");
    crate::emacs_core::pdump::dump_to_file(&source, &image).expect("dump the localized symbol");
    drop(source);
    let mut context = crate::test_utils::with_legacy_gc(|| {
        crate::emacs_core::pdump::load_from_dump(&image).expect("map the localized symbol")
    });
    set_tagged_heap(&mut context.tagged_heap);
    context.specpdl.reserve(16);
    context.jit_bind_stack.reserve(16);
    let owner = context
        .obarray()
        .symbol_value_copied("fx1-mapped-default-owner")
        .expect("mapped owner root");
    let symbol = context
        .obarray()
        .get_by_id(intern("u34-inline-blv"))
        .expect("dumped BLV");
    assert_eq!(
        symbol.redirect(),
        crate::emacs_core::symbol::SymbolRedirect::Localized
    );
    // The image reconstructs ordinary BLV records with fresh defcells. Install
    // the same coherent (SYMBOL . DEFAULT) pair retained explicitly in this
    // image, so the remembered mapped-cell proof is exercised rather than
    // silently testing a newly allocated owner outside the dump window.
    // SAFETY: this exclusive Context owns the checked BLV record; both cells
    // are live and have identical symbol/default contents.
    let blv = unsafe { &mut *symbol.localized_blv().expect("localized").as_ptr() };
    if blv.valcell == blv.defcell {
        blv.valcell = owner;
    }
    blv.defcell = owner;
    assert!(
        context
            .tagged_heap
            .remember_mapped_cons_ahead_of_writes(owner)
    );
    assert!(!is_observed(owner.bits()));
    let inline_bindings = super::super::inline_vars::blv_bind_layout_available_for_test();
    super::super::inline_vars::reset_inline_var_sites();
    let leaf = compile_blv(
        &context,
        &[
            Op::StackRef(0),
            Op::VarBind(0),
            Op::Unbind(1),
            Op::StackRef(0),
            Op::Return,
        ],
        &[Value::symbol("u34-inline-blv")],
        1,
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Bind),
        u32::from(inline_bindings)
    );
    assert_eq!(
        super::super::inline_vars::inline_var_sites(super::super::inline_vars::InlineVarOp::Unbind),
        u32::from(inline_bindings)
    );
    for _ in 0..3 {
        assert_eq!(native(&mut context, &leaf, &[Value::T]), Value::T);
    }
    let binds = super::super::shims::VARBIND_SHIM_CALLS.with(|count| count.get());
    let unbinds = super::super::shims::UNBIND_SHIM_CALLS.with(|count| count.get());
    assert_eq!(native(&mut context, &leaf, &[Value::T]), Value::T);
    assert_eq!(
        super::super::shims::VARBIND_SHIM_CALLS.with(|count| count.get()) - binds,
        usize::from(!inline_bindings),
        "an admitted binding stays inline; a missing layout uses the compiled shim",
    );
    assert_eq!(
        super::super::shims::UNBIND_SHIM_CALLS.with(|count| count.get()) - unbinds,
        usize::from(!inline_bindings)
    );
    assert!(
        !is_observed(owner.bits()),
        "plain stores do not publish reads"
    );
    let (_, reads) = capture(|| owner.cons_cdr());
    let reads = reads.expect("retained default-cell read");
    let revision = LispCollectionRevision::current();
    assert_eq!(native(&mut context, &leaf, &[Value::T]), Value::T);
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        2
    );
    assert!(
        !reads.unchanged(),
        "bind and restore each journal their observed cell"
    );
}

#[test]
fn gen0_mapped_blv_keeps_remembered_proof_until_its_default_cell_is_observed() {
    check_mapped_blv_binding();
}

#[test]
fn gen0_mapped_blv_keeps_remembered_proof_until_its_default_cell_is_observed_without_let_layout() {
    super::super::jit_layout::with_unavailable_let_layout_for_test(|| {
        assert!(!super::super::inline_vars::blv_bind_layout_available_for_test());
        check_mapped_blv_binding();
    });
}

fn blv_context(local: bool) -> Context {
    let mut context = context(false);
    context.specpdl.reserve(16);
    context.jit_bind_stack.reserve(16);
    let setup = if local {
        "(progn (defvar u34-inline-blv 43)
                (make-local-variable 'u34-inline-blv)
                (setq u34-inline-blv 47))"
    } else {
        "(progn (defvar u34-inline-blv 43)
                (save-current-buffer
                  (set-buffer (get-buffer-create \" u34-inline-other\"))
                  (make-local-variable 'u34-inline-blv))
                u34-inline-blv)"
    };
    context.eval_str(setup).expect("GEN0 BLV fixture");
    context
}

fn aset_leaf() -> CompiledLeaf {
    lower_leaf(
        &[
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        &[],
        3,
    )
    .expect("aset compiles")
}

fn fixture(kind: StoreKind) -> (Context, Value, CompiledLeaf) {
    if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) {
        let local = matches!(kind, StoreKind::BlvLocal);
        let mut context = blv_context(local);
        let owner = blv_cell(&context, local);
        // Return the supplied value directly: a VarRef after VarSet would
        // intentionally observe the BLV cell inside setter-only captures.
        let leaf = compile_blv(
            &context,
            &[Op::StackRef(0), Op::VarSet(0), Op::StackRef(0), Op::Return],
            &[Value::symbol("u34-inline-blv")],
            1,
        );
        context.push_specpdl_root(owner);
        return (context, owner, leaf);
    }
    let mut context = context(false);
    let leaf = match kind {
        StoreKind::Setcar | StoreKind::Setcdr => {
            compile_bytecode_function(&store_function(if matches!(kind, StoreKind::Setcar) {
                Op::Setcar
            } else {
                Op::Setcdr
            }))
            .expect("cons setter compiles")
        }
        StoreKind::ConstantSetcar => lower_leaf(
            &[Op::StackRef(0), Op::Constant(0), Op::Setcar, Op::Return],
            &[Value::make_int(12)],
            1,
        )
        .expect("constant setter compiles"),
        _ => aset_leaf(),
    };
    if !matches!(
        kind,
        StoreKind::Setcar | StoreKind::Setcdr | StoreKind::ConstantSetcar
    ) {
        // Epoch validation may outline the first aset. Warm a separate owner
        // before allocating the never-read owner whose revision we measure.
        let warm = kind.allocate(&mut context.tagged_heap);
        context.push_specpdl_root(warm);
        let args = kind.args(warm, Value::make_int(64));
        native(&mut context, &leaf, &args);
        native(&mut context, &leaf, &args);
    }
    let owner = kind.allocate(&mut context.tagged_heap);
    context.push_specpdl_root(owner);
    (context, owner, leaf)
}

fn check_first_store(kind: StoreKind) {
    let _journal = JournalMode::observed();
    let (mut context, owner, leaf) = fixture(kind);
    assert!(!is_observed(owner.bits()), "fresh owner: {kind:?}");
    let before = LispCollectionRevision::current();
    let shims_before = kind.shim_calls();
    let (lo, hi) = crate::tagged::collection_reads::compiled_observation_window();
    let address = owner.bits() & !crate::tagged::value::TAG_MASK;
    let outside_window = address < lo || address >= hi;
    let supplied = Value::make_int(65);
    assert_eq!(
        native(&mut context, &leaf, &kind.args(owner, supplied)),
        kind.expected(supplied)
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        0,
        "never-read native owner skips the journal: {kind:?}"
    );
    assert!(!is_observed(owner.bits()), "the store is not a read");
    if outside_window {
        assert_eq!(
            kind.shim_calls(),
            shims_before,
            "the outside-window unobserved store stays inline: {kind:?}"
        );
    }
    let (value, reads) = capture(|| {
        let value = kind.read(owner);
        assert!(
            is_observed(owner.bits()),
            "published before returning the read"
        );
        value
    });
    assert_eq!(value, kind.expected(supplied));
    let reads = reads.expect("a read after the unobserved store is coherent");
    assert!(reads.unchanged());
    let before = LispCollectionRevision::current();
    let supplied = Value::make_int(66);
    native(&mut context, &leaf, &kind.args(owner, supplied));
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        1,
        "observed native owner journals exactly once: {kind:?}"
    );
    assert!(
        !reads.unchanged(),
        "the completed read rejects reuse: {kind:?}"
    );
    drop(reads);
    assert!(
        is_observed(owner.bits()),
        "scope/certificate exit is not reclamation"
    );
    let (_, incoherent) = capture(|| {
        kind.read(owner);
        native(&mut context, &leaf, &kind.args(owner, Value::make_int(67)))
    });
    assert!(incoherent.is_none(), "read before native write: {kind:?}");
}

#[test]
fn gen0_observed_setcar_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::Setcar);
}

#[test]
fn gen0_observed_setcdr_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::Setcdr);
}

#[test]
fn gen0_observed_constant_store_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::ConstantSetcar);
}

#[test]
fn gen0_observed_vector_aset_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::VectorAset);
}

#[test]
fn gen0_observed_record_aset_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::RecordAset);
}

#[test]
fn gen0_observed_string_aset_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::UnibyteStringAset);
    check_first_store(StoreKind::MultibyteStringAset);
}

#[test]
fn gen0_observed_blv_store_skips_until_the_first_owner_read() {
    check_first_store(StoreKind::BlvDefault);
    check_first_store(StoreKind::BlvLocal);
}

#[test]
fn gen0_observed_store_before_read_in_the_same_capture_is_coherent() {
    let _journal = JournalMode::observed();
    for kind in STORE_KINDS {
        let (mut context, owner, leaf) = fixture(kind);
        assert!(!is_observed(owner.bits()));
        let before = LispCollectionRevision::current();
        let shims_before = kind.shim_calls();
        let supplied = Value::make_int(65);
        let (result, reads) = capture(|| {
            native(&mut context, &leaf, &kind.args(owner, supplied));
            let shim_delta = kind.shim_calls() - shims_before;
            // A compiled cached BLV fallback uses the same unobserved
            // owner contract as the native store, even after earlier captures
            // place this cell inside the conservative envelope.
            if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) && shim_delta != 0 {
                assert_eq!(shim_delta, 1);
            }
            assert!(!is_observed(owner.bits()), "native setter alone: {kind:?}");
            assert_eq!(
                LispCollectionRevision::current().steps_since_for_test(before),
                0
            );
            tracing::info!(target: "fx1_collection",
                "FX1_CAPTURE case=store-before-read kind={kind:?} shim_delta={shim_delta} owner_mark={}",
                usize::from(is_observed(owner.bits())));
            kind.read(owner)
        });
        assert_eq!(result, kind.expected(supplied));
        let reads = reads.expect("the first actual read follows the unobserved store");
        assert!(is_observed(owner.bits()));
        assert!(reads.unchanged());
        native(&mut context, &leaf, &kind.args(owner, Value::make_int(66)));
        assert!(!reads.unchanged(), "later mutation: {kind:?}");
    }
}

#[test]
fn gen0_observed_setter_capture_preserves_inline_and_outlined_dependencies() {
    let _journal = JournalMode::observed();
    for kind in STORE_KINDS {
        let (mut context, owner, leaf) = fixture(kind);
        let supplied = Value::make_int(65);
        let before = LispCollectionRevision::current();
        let shims_before = kind.shim_calls();
        let (result, empty) = capture(|| native(&mut context, &leaf, &kind.args(owner, supplied)));
        assert_eq!(result, kind.expected(supplied));
        let shim_delta = kind.shim_calls() - shims_before;
        let reads = empty.expect("setter capture is coherent");
        if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) && shim_delta != 0 {
            assert_eq!(shim_delta, 1);
        }
        assert!(
            !is_observed(owner.bits()),
            "a compiled setter is not a read: {kind:?}"
        );
        native(&mut context, &leaf, &kind.args(owner, Value::make_int(66)));
        assert!(
            reads.unchanged(),
            "an empty dependency set is safe: {kind:?}"
        );
        assert_eq!(
            LispCollectionRevision::current().steps_since_for_test(before),
            0
        );
        tracing::info!(target: "fx1_collection",
            "FX1_CAPTURE case=setter-only kind={kind:?} shim_delta={shim_delta} owner_mark={}",
            usize::from(is_observed(owner.bits())));
        let (_, actual) = capture(|| kind.read(owner));
        native(&mut context, &leaf, &kind.args(owner, Value::make_int(67)));
        assert!(!actual.expect("actual owner read").unchanged());
    }
}

#[test]
fn gen0_observed_nested_and_transitive_reads_publish_sticky_owners() {
    let _journal = JournalMode::observed();
    let (mut context, owner, leaf) = fixture(StoreKind::Setcar);
    let (_, outer) = capture(|| {
        let (_, inner) = capture(|| {
            owner.cons_car();
            assert!(is_observed(owner.bits()));
        });
        assert!(inner.expect("nested read").unchanged_and_observe());
    });
    let outer = outer.expect("nested dependencies propagate");
    let (_, transitive) = capture(|| assert!(outer.unchanged_and_observe()));
    native(&mut context, &leaf, &[owner, Value::make_int(65)]);
    assert!(!outer.unchanged());
    assert!(!transitive.expect("cache hit retains the owner").unchanged());
    assert!(is_observed(owner.bits()));
}

#[test]
fn gen0_observed_envelope_does_not_journal_an_unobserved_middle_owner() {
    let _journal = JournalMode::observed();
    let mut context = context(false);
    let leaf = compile_bytecode_function(&store_function(Op::Setcar)).expect("setcar");
    let mut owners = [
        Value::cons(Value::NIL, Value::NIL),
        Value::cons(Value::NIL, Value::NIL),
        Value::cons(Value::NIL, Value::NIL),
    ];
    owners.sort_unstable_by_key(|owner| owner.bits());
    let [first, middle, last] = owners;
    for owner in [first, middle, last] {
        context.push_specpdl_root(owner);
    }
    assert!(first.bits() < middle.bits() && middle.bits() < last.bits());
    let (_, ends) = capture(|| {
        first.cons_car();
        last.cons_car();
    });
    assert!(is_observed(first.bits()) && is_observed(last.bits()));
    assert!(!is_observed(middle.bits()));
    let before = LispCollectionRevision::current();
    native(&mut context, &leaf, &[middle, Value::make_int(65)]);
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        0,
        "the envelope is conservative; its exact owner filter is not"
    );
    assert!(!is_observed(middle.bits()));
    assert!(ends.expect("only the endpoints were read").unchanged());
    assert_eq!(middle.cons_car(), Value::make_int(65));
}

#[test]
fn gen0_observed_certificate_survives_context_switch_until_its_owner_is_stored() {
    let _journal = JournalMode::observed();
    let mut first_context = context(false);
    let owner = Value::cons(Value::NIL, Value::NIL);
    first_context.push_specpdl_root(owner);
    let (_, reads) = capture(|| owner.cons_car());
    let reads = reads.expect("Context A owner");
    assert!(is_observed(owner.bits()));
    let mut second_context = context(false);
    let leaf = compile_bytecode_function(&store_function(Op::Setcar)).expect("Context B setter");
    assert!(reads.unchanged(), "creating B does not mutate A's owner");
    let before = LispCollectionRevision::current();
    native(&mut second_context, &leaf, &[owner, Value::make_int(65)]);
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        1
    );
    assert!(
        !reads.unchanged(),
        "B's native store invalidates this mutator's A read"
    );
    assert_eq!(owner.cons_car(), Value::make_int(65));
}

#[test]
fn gen0_native_refined_gap_closes_when_owner_is_read_after_store() {
    let _journal = JournalMode::observed();
    for (op, kind) in [
        (Op::Setcar, StoreKind::Setcar),
        (Op::Setcdr, StoreKind::Setcdr),
    ] {
        let mut context = context(false);
        let leaf = compile_bytecode_function(&store_function(op)).expect("native cons setter");
        let mut owners = std::array::from_fn::<_, 3, _>(|_| {
            context.tagged_heap.alloc_cons(Value::NIL, Value::NIL)
        });
        owners.sort_unstable_by_key(|owner| owner.bits());
        let [first, target, last] = owners;
        for owner in owners {
            context.push_specpdl_root(owner);
        }
        let (_, endpoints) = capture(|| (first.cons_car(), last.cons_car()));
        let endpoints = endpoints.expect("the endpoints bound a real empty interval");
        let before = LispCollectionRevision::current();
        let shims = cons_shims();
        for value in [12, 23, 34] {
            native(
                &mut context,
                &leaf,
                &kind.args(target, Value::make_int(value)),
            );
        }
        assert_eq!(LispCollectionRevision::current(), before);
        assert_eq!(
            cons_shims(),
            shims + 1,
            "the first refusal completes in the shim; later stores stay inline"
        );
        assert!(endpoints.unchanged());
        let address = target.bits() & !crate::tagged::value::TAG_MASK;
        assert!(
            !context
                .tagged_heap
                .jit_barrier_window_for_test()
                .covers(address)
        );

        let (value, reads) = capture(|| kind.read(target));
        assert_eq!(value, Value::make_int(34));
        let reads = reads.expect("a certificate taken after the inline mutation");
        assert!(reads.unchanged());
        assert!(
            context
                .tagged_heap
                .jit_barrier_window_for_test()
                .covers(address)
        );
        native(&mut context, &leaf, &kind.args(target, Value::make_int(45)));
        assert_eq!(
            LispCollectionRevision::current().steps_since_for_test(before),
            1
        );
        assert_eq!(cons_shims(), shims + 2);
        assert!(
            !reads.unchanged(),
            "the same leaf journals the newly observed owner"
        );
        assert!(endpoints.unchanged());
    }
}

fn standalone_heap() -> TaggedHeap {
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                match self.0.take() {
                    Some(previous) => std::env::set_var("NEOVM_GC_GENERATIONAL", previous),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }
    let _restore = Restore(std::env::var_os("NEOVM_GC_GENERATIONAL"));
    unsafe { std::env::remove_var("NEOVM_GC_GENERATIONAL") };
    TaggedHeap::new()
}

const GC_KINDS: [StoreKind; 4] = [
    StoreKind::Setcar,
    StoreKind::VectorAset,
    StoreKind::RecordAset,
    StoreKind::UnibyteStringAset,
];

fn concurrent_cycle(heap: &mut TaggedHeap, roots: &[Value]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    let before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}

#[test]
fn gen0_observed_mark_survives_live_stw_and_concurrent_gc_cycles() {
    let _journal = JournalMode::observed();
    for kind in GC_KINDS {
        let mut heap = standalone_heap();
        set_tagged_heap(&mut heap);
        let owner = kind.allocate(&mut heap);
        assert!(!is_observed(owner.bits()));
        let (_, reads) = capture(|| kind.read(owner));
        assert!(reads.expect("live owner read").unchanged());
        assert!(is_observed(owner.bits()));
        for _ in 0..2 {
            heap.collect_exact(std::iter::once(owner));
            assert!(is_observed(owner.bits()), "live STW survivor: {kind:?}");
        }
        concurrent_cycle(&mut heap, &[owner]);
        assert!(
            is_observed(owner.bits()),
            "live concurrent survivor: {kind:?}"
        );
    }
}

#[test]
fn gen0_observed_mark_clears_when_gc_frees_and_reuses_the_exact_slot() {
    let _journal = JournalMode::observed();
    for kind in GC_KINDS {
        let mut heap = standalone_heap();
        set_tagged_heap(&mut heap);
        let keep = kind.allocate(&mut heap);
        let doomed = kind.allocate(&mut heap);
        let keep2 = kind.allocate(&mut heap);
        let old_bits = doomed.bits();
        let (_, reads) = capture(|| kind.read(doomed));
        let old_reads = reads.expect("observed owner before reclamation");
        assert!(is_observed(old_bits));
        heap.collect_exact([keep, keep2].into_iter());
        assert!(!heap.owns_heap_value_for_test(doomed));
        // The same-class neighbors retain this block/page's storage. The
        // non-observing query can inspect its cleared free-slot metadata.
        assert!(!is_observed(old_bits), "freed slot: {kind:?}");
        let reused = kind.allocate(&mut heap);
        assert_eq!(reused.bits(), old_bits, "exact slot reuse: {kind:?}");
        assert!(!is_observed(reused.bits()), "a new owner is unobserved");
        // Readsets do not root owners. Retain this old certificate across
        // reclamation/reuse, but do not validate it after its owner's lifetime.
        drop(old_reads);
        let (_, reads) = capture(|| kind.read(reused));
        assert!(is_observed(reused.bits()));
        assert!(reads.expect("fresh reused-owner certificate").unchanged());
    }
}

#[test]
fn gen0_observed_reused_address_republishes_before_recent_read_dedup() {
    let _journal = JournalMode::observed();
    for kind in GC_KINDS {
        let mut heap = standalone_heap();
        set_tagged_heap(&mut heap);
        let keep = kind.allocate(&mut heap);
        let doomed = kind.allocate(&mut heap);
        let keep2 = kind.allocate(&mut heap);
        let old_bits = doomed.bits();
        let (_, reads) = capture(|| {
            kind.read(doomed);
            assert!(is_observed(old_bits));
            heap.collect_exact([keep, keep2].into_iter());
            assert!(!is_observed(old_bits));
            let reused = kind.allocate(&mut heap);
            assert_eq!(reused.bits(), old_bits);
            assert!(!is_observed(reused.bits()));
            // This capture's recent-identity cache still names old_bits.
            // Actual observation must republish before its dedup shortcut.
            kind.read(reused);
            assert!(is_observed(reused.bits()), "recent identity hit: {kind:?}");
        });
        assert!(reads.expect("no writes after either read").unchanged());
    }
}

fn probe_kind(name: &str) -> StoreKind {
    match name {
        "setcar" => StoreKind::Setcar,
        "setcdr" => StoreKind::Setcdr,
        "vector-aset" => StoreKind::VectorAset,
        "record-aset" => StoreKind::RecordAset,
        "string-aset" => StoreKind::UnibyteStringAset,
        "blv-local" => StoreKind::BlvLocal,
        "blv-default" => StoreKind::BlvDefault,
        other => panic!("unsupported FX1_STORE_KIND: {other}"),
    }
}

fn probe_leaf(kind: StoreKind, context: &Context) -> CompiledLeaf {
    // Parameters are [owner-or-unused n]. Each iteration has exactly one
    // lowered heap store and no Lisp allocation or Lisp function call. Its
    // observed owner or conservative envelope can select the existing shim.
    let mut ops = vec![Op::StackRef(0), Op::Constant(0), Op::Gtr, Op::GotoIfNil(0)];
    match kind {
        StoreKind::Setcar | StoreKind::Setcdr => {
            ops.extend([Op::StackRef(1), Op::Constant(1)]);
            ops.push(if matches!(kind, StoreKind::Setcar) {
                Op::Setcar
            } else {
                Op::Setcdr
            });
            ops.push(Op::Pop);
        }
        StoreKind::BlvDefault | StoreKind::BlvLocal => {
            ops.extend([Op::Constant(1), Op::VarSet(3)]);
        }
        _ => ops.extend([
            Op::StackRef(1),
            Op::Constant(2),
            Op::Constant(1),
            Op::Aset,
            Op::Pop,
        ]),
    }
    ops.extend([Op::StackRef(0), Op::Sub1, Op::StackSet(1), Op::Goto(0)]);
    let exit = ops.len() as u32;
    ops[3] = Op::GotoIfNil(exit);
    ops.extend([Op::Constant(1), Op::Return]);
    let constants = [
        Value::make_int(0),
        Value::make_int(65),
        Value::make_int(kind.index() as i64),
        Value::symbol("u34-inline-blv"),
    ];
    if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) {
        compile_blv(context, &ops, &constants, 2)
    } else {
        lower_leaf(&ops, &constants, 2).expect("one-store bytecode loop compiles")
    }
}

#[test]
#[ignore = "supplementary instruction slopes; runner controls kind/count/observation"]
fn gen0_observed_collection_store_cost_probe() {
    let name = std::env::var("FX1_STORE_KIND").expect("FX1_STORE_KIND");
    let kind = probe_kind(&name);
    let count: i64 = std::env::var("FX1_STORE_COUNT")
        .expect("FX1_STORE_COUNT")
        .parse()
        .expect("positive integer count");
    assert!(count > 0);
    let observation = std::env::var("FX1_STORE_OBSERVATION").expect("FX1_STORE_OBSERVATION");
    assert!(matches!(observation.as_str(), "observed" | "unobserved"));
    let envelope = std::env::var("FX1_STORE_ENVELOPE").ok().as_deref() == Some("1");
    if envelope {
        assert_eq!(
            observation, "unobserved",
            "the envelope target is never read"
        );
        assert!(
            !matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal),
            "BLV envelopes use the separately tested outlined interpreter setter"
        );
    }
    // Keep the runtime mode selected by the runner: this probe is shared by
    // OFF/Observed/Eager measurements and does not force correctness mode.
    let mut context = if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) {
        blv_context(matches!(kind, StoreKind::BlvLocal))
    } else {
        context(false)
    };
    let leaf = probe_leaf(kind, &context);
    let warm = if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) {
        blv_cell(&context, matches!(kind, StoreKind::BlvLocal))
    } else {
        kind.allocate(&mut context.tagged_heap)
    };
    context.push_specpdl_root(warm);
    for _ in 0..2 {
        assert_eq!(
            native(&mut context, &leaf, &[warm, Value::make_int(8)]),
            Value::make_int(65)
        );
    }
    let (owner, neighbors) = if envelope {
        let mut owners = [
            kind.allocate(&mut context.tagged_heap),
            kind.allocate(&mut context.tagged_heap),
            kind.allocate(&mut context.tagged_heap),
        ];
        owners.sort_unstable_by_key(|owner| owner.bits());
        let [left, target, right] = owners;
        for owner in owners {
            context.push_specpdl_root(owner);
        }
        (target, Some([left, right]))
    } else if matches!(kind, StoreKind::BlvDefault | StoreKind::BlvLocal) {
        (warm, None)
    } else {
        (kind.allocate(&mut context.tagged_heap), None)
    };
    context.push_specpdl_root(owner);
    let reads = if let Some([left, right]) = neighbors {
        Some(
            capture(|| (kind.read(left), kind.read(right)))
                .1
                .expect("two live owners bracket the unobserved target"),
        )
    } else if observation == "observed" {
        Some(
            capture(|| kind.read(owner))
                .1
                .expect("one actual owner read"),
        )
    } else {
        None
    };
    let mode = crate::tagged::collection_reads::compiled_journal_mode();
    let (lo, hi) = crate::tagged::collection_reads::compiled_observation_window();
    let address = owner.bits() & !crate::tagged::value::TAG_MASK;
    let owner_in_envelope = usize::from(lo <= address && address < hi);
    let observed_neighbors = neighbors.map_or(0, |owners| {
        owners
            .into_iter()
            .filter(|owner| is_observed(owner.bits()))
            .count()
    });
    if envelope && mode == CompiledJournalMode::Observed {
        assert_eq!(owner_in_envelope, 1, "lo <= target < hi");
        assert_eq!(observed_neighbors, 2);
    }
    let owner_mark_before = usize::from(is_observed(owner.bits()));
    if observation == "unobserved" {
        assert_eq!(owner_mark_before, 0);
    }
    let gcs_before = context.gc_count;
    let revision_before = LispCollectionRevision::current();
    let result = native(&mut context, &leaf, &[owner, Value::make_int(count)]);
    let gcs_after = context.gc_count;
    let journal_steps = LispCollectionRevision::current().steps_since_for_test(revision_before);
    let owner_mark_after = usize::from(is_observed(owner.bits()));
    if observation == "unobserved" {
        assert_eq!(owner_mark_after, 0);
    }
    assert_eq!(result, Value::make_int(65));
    assert_eq!(kind.read(owner), Value::make_int(65));
    assert_eq!(gcs_after, gcs_before);
    if envelope && mode == CompiledJournalMode::Observed {
        assert_eq!(journal_steps, 0, "the exact filter excludes this owner");
        assert!(reads.expect("bracketing certificate").unchanged());
    }
    tracing::info!(target: "fx1_store",
        "FX1_STORE kind={} count={} observation={} owner_mark_before={} owner_mark_after={} bytecode=1 result={} expected=65 gcs_before={} gcs_after={} gcs_delta=0 envelope={} owner_in_envelope={} observed_neighbors={} journal_steps={}",
        name, count, observation, owner_mark_before, owner_mark_after,
        result.as_fixnum().expect("fixnum result"), gcs_before, gcs_after,
        usize::from(envelope), owner_in_envelope, observed_neighbors, journal_steps);
}
