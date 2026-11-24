use super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::{
    OwnedSourceRegistries, OwnedSourceTextRetired, SourceEntryRefusal, SourceWithdrawalFailure,
    SourceWithdrawalStep,
};
use neovm_value::heap_registry::text_retirement::{
    TextCacheRetireError, TextCacheRetirement, TextCacheSlot, heap_identity_word,
};
use neovm_value::intern::intern;
use neovm_value::tagged::gc::{CacheRootScan, HeapIdentity};
use neovm_value::value::Value;

fn ready() -> Box<super::super::super::Context> {
    install_inactive_source_module_probe_for_test();
    let mut source = Box::new(super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    if source.tagged_heap.tagged_heap_generational_enabled() {
        return source;
    }
    match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GenerationalRequired);
            panic!("these rows need a generational heap; NEOVM_GC_GENERATIONAL=0 disables it")
        }
        other => panic!("GEN0 must refuse before any effect: {:?}", other.err()),
    }
}

fn capture(
    source: Box<super::super::super::Context>,
) -> Result<OwnedSourceRegistries, SourceWithdrawalFailure> {
    // SAFETY: fixtures own fresh serial Contexts with no collector, running
    // mutator, store callback, or borrowed heap view.
    unsafe { OwnedSourceRegistries::capture(source) }
}

fn seed_canonical_tables(source: &mut super::super::super::Context) {
    let case = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    let syntax = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    let codes = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    let category = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    source.cached_standard_case_table = Some(case);
    source.standard_syntax_table = syntax;
    source.syntax_code_objects = codes;
    source.standard_category_table = category;
    source.setup_thread_locals();
}

fn visible(source: &super::super::super::Context, name: &str) -> Option<Value> {
    source.variable_value_in_current_buffer(intern(name))
}

fn slot(source: &super::super::super::Context, name: &str) -> Value {
    let info = neovm_edit::buffer::buffer::lookup_buffer_slot_by_sym_id(intern(name))
        .expect("real per-buffer descriptor");
    source
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .slots()[info.offset.index()]
}

/// The minimal harness leaves per-buffer names as ordinary cells. A default
/// binding of `truncate-lines` is the forwarded slot, the same descriptor
/// the full constructor installs.
fn install_truncate_lines_forwarder(source: &mut super::super::super::Context) {
    let info = neovm_edit::buffer::buffer::lookup_buffer_slot_by_sym_id(intern("truncate-lines"))
        .expect("real per-buffer descriptor");
    let forwarder = neovm_value::forward::alloc_buffer_objfwd(
        info.offset,
        info.local_flags_idx,
        info.predicate,
        info.default.to_value(),
    );
    source
        .obarray
        .install_buffer_objfwd(intern("truncate-lines"), forwarder);
}

fn caches(source: &super::super::super::Context) -> Vec<Value> {
    let mut roots = Vec::new();
    let heap = heap_identity_word(source.tagged_heap.heap_identity());
    let epoch = source.tagged_heap.gc_collections();
    neovm_text::emacs_core::casetab::collect_casetab_gc_roots(&mut roots, heap);
    neovm_text::emacs_core::syntax::collect_syntax_gc_roots(&mut roots, heap);
    neovm_text::emacs_core::category::collect_category_gc_roots(&mut roots, heap);
    neovm_text::emacs_core::regex::collect_regex_gc_roots(
        &mut roots,
        heap,
        epoch,
        CacheRootScan::Parked,
    );
    neovm_value::string_pos_cache::collect_string_pos_cache_gc_roots(
        &mut roots,
        heap,
        epoch,
        CacheRootScan::Parked,
    );
    roots
}

#[test]
fn pentry_e2b_source_retirement_parks_default_binding_before_withdrawal() {
    let mut source = ready();
    install_truncate_lines_forwarder(&mut source);
    let name = intern("truncate-lines");
    let original = slot(&source, "truncate-lines");
    source.try_specbind(name, Value::T).unwrap();
    assert!(matches!(
        source.specpdl.last(),
        Some(super::super::super::SpecBinding::LetDefault { .. })
    ));
    assert_eq!(slot(&source, "truncate-lines"), Value::T);
    let captured = capture(source).unwrap();
    let parked = &captured._frame.source;
    assert_eq!(slot(parked, "truncate-lines"), original);
    let mut roots = Vec::new();
    parked
        .owner
        .slab()
        .expect("actual parked slab")
        .trace_roots(&mut |value| roots.push(value));
    assert!(
        roots.contains(&Value::T),
        "the parked default binding stays rooted"
    );
    drop(captured);
}

#[test]
fn pentry_e2b_source_retirement_parks_nested_local_bindings_and_keeps_slab_roots() {
    let mut source = ready();
    let first = Value::cons(Value::fixnum(41), Value::NIL);
    let last = Value::cons(Value::fixnum(43), Value::NIL);
    let name = "buffer-undo-list";
    let original = visible(&source, name);
    source.try_specbind(intern(name), first).unwrap();
    source.try_specbind(intern(name), last).unwrap();
    let captured = capture(source).unwrap();
    let source = &captured._frame.source;
    let mut roots = Vec::new();
    source
        .owner
        .slab()
        .expect("actual parked slab")
        .trace_roots(&mut |value| roots.push(value));
    assert!(
        roots.contains(&first) && roots.contains(&last),
        "both nested parked values must stay rooted"
    );
    assert_eq!(visible(source, name), original);
}

#[test]
fn pentry_e2b_source_retirement_removes_real_caches_keeps_original_canonical_tables() {
    let mut source = ready();
    seed_canonical_tables(&mut source);
    let original_box = std::ptr::from_ref(&*source);
    let canonical = [
        source.standard_syntax_table,
        source.syntax_code_objects,
        source.standard_category_table,
    ];
    assert!(canonical.iter().all(|value| value.is_heap_object()));
    assert!(!caches(&source).is_empty());
    let retired = capture(source).unwrap().retire_text().unwrap();
    assert_eq!(std::ptr::from_ref(&**retired._frame.source), original_box);
    assert!(caches(&retired._frame.source).is_empty());
    assert_eq!(
        [
            retired._frame.source.standard_syntax_table,
            retired._frame.source.syntax_code_objects,
            retired._frame.source.standard_category_table
        ],
        canonical
    );
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &retired._frame.source.tagged_heap
    ));
}

#[test]
fn pentry_e2b_source_retirement_actual_cache_reader_retains_prior_payloads() {
    let mut source = ready();
    seed_canonical_tables(&mut source);
    let alive = source.world_lifetime_probe_for_test();
    let registry = source.ccl_registry.downgrade();
    let captured = capture(source).unwrap();
    let failure =
        neovm_text::emacs_core::casetab::retirement::with_source_retirement_reader_for_test(|| {
            let error = captured
                .retire_text()
                .err()
                .expect("actual matching TLS reader refuses");
            assert!(format!("{error:?}").contains(&format!(
                "{:?}",
                TextCacheRetireError::Borrowed(TextCacheSlot::CaseTable)
            )));
            match error {
                SourceWithdrawalFailure::Retained(failure) => {
                    assert_eq!(failure.step, SourceWithdrawalStep::CaseTable);
                    failure
                }
                other => panic!("partial withdrawal must stay retained: {other:?}"),
            }
        });
    drop(failure);
    assert!(alive());
    assert!(registry.upgrade().is_some());
}

#[test]
fn pentry_e2b_source_retirement_foreign_reactivation_refuses_before_cache_effects() {
    let mut source = ready();
    seed_canonical_tables(&mut source);
    let alive = source.world_lifetime_probe_for_test();
    let captured = capture(source).unwrap();
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&captured._frame.source.tagged_heap);
    let mut foreign = ready();
    seed_canonical_tables(&mut foreign);
    let foreign_roots = caches(&foreign);
    assert!(!foreign_roots.is_empty());
    let failure = captured.retire_text().err().expect("fresh heap validation");
    assert!(format!("{failure:?}").contains("HeapTlsNotCurrent"));
    match &failure {
        SourceWithdrawalFailure::Retained(retained) => {
            assert_eq!(retained.step, SourceWithdrawalStep::TextEntry);
        }
        other => panic!("already armed: {other:?}"),
    }
    drop(failure);
    assert_eq!(caches(&foreign), foreign_roots);
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
    assert!(alive());
}

#[test]
fn pentry_e2b_source_retirement_drop_and_forget_keep_original_worlds() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let retired = capture(source).unwrap().retire_text().unwrap();
    // `SourceFrame` holds `ManuallyDrop<Box<Context>>`, so dropping it leaves
    // the retired world's heap slot installed. Clear that slot before building
    // the second test world.
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&retired._frame.source.tagged_heap);
    drop(retired);
    assert!(alive());
    let source = ready();
    let second = source.world_lifetime_probe_for_test();
    std::mem::forget(capture(source).unwrap().retire_text().unwrap());
    assert!(alive() && second());
}

#[test]
fn pentry_e2b_source_retirement_catch_unwind_keeps_parked_source_closed() {
    let mut source = ready();
    source
        .try_specbind(intern("truncate-lines"), Value::T)
        .unwrap();
    let alive = source.world_lifetime_probe_for_test();
    let retired = capture(source).unwrap().retire_text().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _retained: OwnedSourceTextRetired = retired;
        panic!("OWNED_SOURCE_TEXT_RETIREMENT_UNWIND");
    }));
    assert!(result.is_err());
    assert!(alive());
}

#[test]
fn pentry_e2b_source_retirement_already_empty_caches_complete_without_saved_words() {
    let mut source = ready();
    seed_canonical_tables(&mut source);
    let heap: HeapIdentity = source.tagged_heap.heap_identity();
    neovm_text::emacs_core::casetab::retirement::retire_casetab_for_heap(heap).unwrap();
    neovm_text::emacs_core::syntax::retirement::retire_syntax_for_heap(heap).unwrap();
    neovm_text::emacs_core::category::retirement::retire_category_for_heap(heap).unwrap();
    neovm_text::emacs_core::regex::retirement::retire_regex_for_heap(heap).unwrap();
    neovm_value::string_pos_cache::retire_string_pos_cache_for_heap(heap);
    let retired = capture(source).unwrap().retire_text().unwrap();
    assert!(caches(&retired._frame.source).is_empty());
    assert_eq!(
        retired._frame.effects._receipt.0,
        TextCacheRetirement::AlreadyInactive(heap)
    );
}
