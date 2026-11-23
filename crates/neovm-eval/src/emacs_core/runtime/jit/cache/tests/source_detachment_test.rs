use super::super::{
    COMPILED, COMPILED_HEAP, COMPILED_OBARRAY, CacheEntry, CompiledHeapPin, INLINE_DEPS,
    INLINE_EVICTIONS, LEAF_SLOT_EPOCH, NativeDepthGuard, OSR_CACHE, OSR_PENDING, REDEFINED_PINS,
    clear_with_serial_receipt, collect_jit_reloc_gc_roots_for_heap, prepare_entry_for_test,
    reset_carrier_tls,
};
use super::{DetachedSourceCache, SourceCacheError, SourceCacheOwner, SourceCacheSlot};
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use neovm_value::value::{LambdaParams, Value};
use std::cell::Cell;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::atomic::Ordering;

fn context() -> Box<Context> {
    reset_carrier_tls();
    let mut source = Box::new(Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    source
}

fn function(constant: Value) -> crate::emacs_core::bytecode::ByteCodeFunction {
    let mut function = crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    function.set_ops(vec![Op::Constant(0), Op::Return]);
    function.set_constants(vec![constant].into());
    function.set_max_stack(crate::emacs_core::bytecode::StackDepth::for_test(16));
    function
}

fn metadata_fixture() -> (ManuallyDrop<Box<Context>>, SourceCacheOwner) {
    let source = context();
    let owner = SourceCacheOwner::for_context(&source);
    COMPILED.with(|cache| cache.borrow_mut().insert(11, CacheEntry::NotCompilable));
    COMPILED_HEAP.with(|slot| slot.set(CompiledHeapPin::Pinned(owner.heap)));
    COMPILED_OBARRAY.with(|slot| slot.set(Some(owner.obarray.0)));
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    (ManuallyDrop::new(source), owner)
}

fn withdraw(owner: SourceCacheOwner) -> Result<DetachedSourceCache, SourceCacheError> {
    // SAFETY: isolated sole-source fixtures retain their actual Context in a
    // non-destructing shell, with no active native, VM, compile, heap, or
    // store frame. The metadata and relocation census is complete before this
    // call, and no collecting operation follows removal. No transport is made.
    unsafe { DetachedSourceCache::withdraw(owner) }
}

#[test]
fn pentry_e2b_source_jit_cache_real_leaf_relocation_and_executable_retention() {
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let mut source = context();
    let value = Value::string("source-cache-relocation");
    let mut function = function(value);
    function.jit_runtime().set_hot_for_test();
    assert_eq!(
        crate::emacs_core::jit::test_support::run_compiled(
            std::ptr::from_mut(&mut *source),
            &function,
            Value::NIL,
            &[],
        )
        .unwrap(),
        Some(value.bits())
    );
    let id = function.jit_runtime().compiled_id().unwrap();
    let weak =
        COMPILED.with(|cache| Rc::downgrade(cache.borrow().get(id).unwrap().live_leaf().unwrap()));
    let mut census = Vec::new();
    collect_jit_reloc_gc_roots_for_heap(&mut census, source.tagged_heap.tagged_heap_identity());
    assert!(census.contains(&value));
    let owner = SourceCacheOwner::for_context(&source);
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    let _source = ManuallyDrop::new(source);
    let _census = ManuallyDrop::new(census);
    let epoch = LEAF_SLOT_EPOCH.load(Ordering::Relaxed);
    let receipt = withdraw(owner).unwrap();
    assert!(COMPILED.with(|cache| cache.borrow().get(id).is_none()));
    assert!(matches!(
        COMPILED_HEAP.with(Cell::get),
        CompiledHeapPin::Unpinned
    ));
    assert!(COMPILED_OBARRAY.with(|slot| slot.get().is_none()));
    assert_ne!(LEAF_SLOT_EPOCH.load(Ordering::Relaxed), epoch);
    assert!(
        receipt
            .compiled
            .get(id)
            .unwrap()
            .live_leaf()
            .unwrap()
            .reloc_values()
            .contains(&value)
    );
    drop(receipt);
    assert!(
        weak.upgrade().is_some(),
        "abandonment must not free executable or relocation backing"
    );
}

#[test]
fn pentry_e2b_source_jit_cache_every_slot_loan_refuses_before_any_mutation() {
    let (_source, owner) = metadata_fixture();
    let epoch = LEAF_SLOT_EPOCH.load(Ordering::Relaxed);
    macro_rules! loan {
        ($slot:ident, $kind:ident) => {
            $slot.with(|slot| {
                let reader = slot.borrow();
                assert_eq!(
                    withdraw(owner).unwrap_err(),
                    SourceCacheError::Borrowed(SourceCacheSlot::$kind)
                );
                drop(reader);
            });
        };
    }
    loan!(COMPILED, Compiled);
    loan!(INLINE_DEPS, InlineDeps);
    loan!(INLINE_EVICTIONS, InlineEvictions);
    loan!(OSR_CACHE, Osr);
    loan!(OSR_PENDING, PendingOsr);
    loan!(REDEFINED_PINS, RedefinedPins);
    assert_eq!(LEAF_SLOT_EPOCH.load(Ordering::Relaxed), epoch);
    assert!(
        COMPILED.with(|cache| matches!(cache.borrow().get(11), Some(CacheEntry::NotCompilable)))
    );
    assert_eq!(
        COMPILED_HEAP.with(Cell::get),
        CompiledHeapPin::Pinned(owner.heap)
    );
}

#[test]
fn pentry_e2b_source_jit_cache_foreign_or_unbound_identity_preserves_original_slots() {
    // A new Context initializes the legacy cache. Obtain the foreign identity
    // before installing the slots whose refusal we observe.
    let foreign = context();
    let other = SourceCacheOwner::for_context(&foreign);
    drop(foreign);
    let (_source, owner) = metadata_fixture();
    let epoch = LEAF_SLOT_EPOCH.load(Ordering::Relaxed);
    COMPILED_HEAP.with(|slot| slot.set(CompiledHeapPin::Pinned(other.heap)));
    assert_eq!(withdraw(owner).unwrap_err(), SourceCacheError::WrongHeap);
    assert_eq!(
        COMPILED_HEAP.with(Cell::get),
        CompiledHeapPin::Pinned(other.heap)
    );
    COMPILED_HEAP.with(|slot| slot.set(CompiledHeapPin::Unpinned));
    assert_eq!(
        withdraw(owner).unwrap_err(),
        SourceCacheError::MissingHeapOwner
    );
    COMPILED_HEAP.with(|slot| slot.set(CompiledHeapPin::Pinned(owner.heap)));
    COMPILED_OBARRAY.with(|slot| slot.set(Some(other.obarray.0)));
    assert_eq!(withdraw(owner).unwrap_err(), SourceCacheError::WrongObarray);
    assert_eq!(LEAF_SLOT_EPOCH.load(Ordering::Relaxed), epoch);
    assert!(
        COMPILED.with(|cache| matches!(cache.borrow().get(11), Some(CacheEntry::NotCompilable)))
    );
}

#[test]
fn pentry_e2b_source_jit_cache_installed_foreign_heap_is_not_cleared() {
    let (_source, owner) = metadata_fixture();
    let foreign = context();
    assert_eq!(
        withdraw(owner).unwrap_err(),
        SourceCacheError::HeapInstalled
    );
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
}

#[test]
fn pentry_e2b_source_jit_cache_native_extent_observation_refuses_before_mutation() {
    let (_source, owner) = metadata_fixture();
    // Counter-only scope control, without executing a native machine frame.
    let guard = NativeDepthGuard::enter();
    assert_eq!(
        withdraw(owner).unwrap_err(),
        SourceCacheError::NativeActivation
    );
    drop(guard);
    assert!(
        COMPILED.with(|cache| matches!(cache.borrow().get(11), Some(CacheEntry::NotCompilable)))
    );
}

#[test]
fn pentry_e2b_source_jit_cache_real_pending_job_is_not_cancelled_by_refusal() {
    use crate::emacs_core::jit::bg::{self, BgMode, TestModeScope};
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            clear_with_serial_receipt().expect("serial mint while idle");
            bg::hold_publish_for_test(false);
            bg::publish_held_for_test();
            bg::force_deferred_install_for_test(false);
        }
    }
    let _mode = TestModeScope::enter(BgMode::Sync);
    let _reset = Reset;
    let mut source = context();
    bg::force_deferred_install_for_test(true);
    bg::hold_publish_for_test(true);
    let mut function = function(Value::fixnum(53));
    function.jit_runtime().set_hot_for_test();
    prepare_entry_for_test(std::ptr::from_mut(&mut *source), &function);
    let pending = bg::pending_count();
    assert!(
        pending > 0,
        "real completed backend is held before cache install"
    );
    let owner = SourceCacheOwner::for_context(&source);
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    let _source = ManuallyDrop::new(source);
    assert_eq!(withdraw(owner).unwrap_err(), SourceCacheError::PendingJobs);
    assert_eq!(
        bg::pending_count(),
        pending,
        "refusal must not cancel or consume a job"
    );
}
