use super::super::super::shims::{
    clear_current_leaf_bases, enter_armed_live_extent, is_inside_native_extent, publish_leaf_bases,
};
use super::super::{PERSISTENT_TEST_OVERRIDE, SHARED_JIT, SharedJit};
use super::{
    DetachedSourceBackend, PENDING_FLOW, PENDING_ROOT_SWEEP_FLOOR, PENDING_SHIM_PANIC,
    SourceBackendError, SourceBackendSlot,
};
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use crate::emacs_core::jit::cache::source_detachment::{DetachedSourceCache, SourceCacheOwner};
use neovm_value::value::{LambdaParams, Value};
use std::mem::ManuallyDrop;

fn function() -> crate::emacs_core::bytecode::ByteCodeFunction {
    let mut function = crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    function.set_ops(vec![
        crate::emacs_core::bytecode::opcode::Op::Constant(0),
        crate::emacs_core::bytecode::opcode::Op::Return,
    ]);
    function.set_constants(vec![Value::fixnum(73)].into());
    function.set_max_stack(crate::emacs_core::bytecode::StackDepth::for_test(16));
    function
}

fn fixture() -> DetachedSourceCache {
    crate::emacs_core::jit::cache::reset_carrier_tls();
    PENDING_FLOW.with(|slot| *slot.borrow_mut() = None);
    PENDING_SHIM_PANIC.with(|slot| *slot.borrow_mut() = None);
    clear_current_leaf_bases();
    PENDING_ROOT_SWEEP_FLOOR.with(|slot| slot.set(None));
    let mut source = Box::new(Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    let prior = PERSISTENT_TEST_OVERRIDE.with(|slot| slot.replace(Some(true)));
    let mut function = function();
    function.jit_runtime().set_hot_for_test();
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let compiled = crate::emacs_core::jit::test_support::run_compiled(
        std::ptr::from_mut(&mut *source),
        &function,
        Value::NIL,
        &[],
    );
    PERSISTENT_TEST_OVERRIDE.with(|slot| slot.set(prior));
    assert_eq!(compiled.unwrap(), Some(Value::fixnum(73).bits()));
    assert!(SHARED_JIT.with(|cell| cell.borrow().is_some()));
    let owner = SourceCacheOwner::for_context(&source);
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    let _source = ManuallyDrop::new(source);
    // SAFETY: isolated sole-source fixture. Execution, store, and collector
    // frames have returned, the original Context is retained, and no
    // collection follows.
    unsafe { DetachedSourceCache::withdraw(owner) }.unwrap()
}

fn withdraw(cache: &DetachedSourceCache) -> Result<DetachedSourceBackend, SourceBackendError> {
    // SAFETY: the fixture retained the actual source and withdrawn cache
    // without a live execution, collector, or store frame. No collection or
    // transport follows.
    unsafe { DetachedSourceBackend::withdraw(cache) }
}

fn address() -> *const SharedJit {
    SHARED_JIT.with(|cell| std::ptr::from_ref(&**cell.borrow().as_ref().unwrap()))
}

#[test]
fn pentry_e2b_source_jit_backend_retains_actual_backend_allocation_and_modules() {
    let cache = fixture();
    let original = address();
    let receipt = withdraw(&cache).unwrap();
    assert!(SHARED_JIT.with(|cell| cell.borrow().is_none()));
    let backend = receipt.backend.as_ref().unwrap();
    assert_eq!(std::ptr::from_ref(&**backend), original);
    assert!(
        backend
            .modules
            .iter()
            .flatten()
            .any(|module| module.leaves > 0)
    );
    assert_eq!(receipt.thread, std::thread::current().id());
    drop(receipt);
    assert!(SHARED_JIT.with(|cell| cell.borrow().is_none()));
}

#[test]
fn pentry_e2b_source_jit_backend_every_slot_loan_refuses_without_taking_backend() {
    let cache = fixture();
    let original = address();
    macro_rules! loan {
        ($slot:ident, $kind:ident) => {
            $slot.with(|slot| {
                let reader = slot.borrow();
                assert_eq!(
                    withdraw(&cache).unwrap_err(),
                    SourceBackendError::Borrowed(SourceBackendSlot::$kind)
                );
                drop(reader);
            });
            assert_eq!(address(), original);
        };
    }
    loan!(PENDING_FLOW, PendingFlow);
    loan!(PENDING_SHIM_PANIC, PendingPanic);
    loan!(SHARED_JIT, Backend);
}

#[test]
fn pentry_e2b_source_jit_backend_pending_flow_remains_unconsumed() {
    let cache = fixture();
    let original = address();
    let flow = neovm_value::error::Flow::shutdown(crate::emacs_core::eval::ShutdownRequest {
        exit_code: 29,
        restart: false,
    });
    PENDING_FLOW.with(|slot| *slot.borrow_mut() = Some(flow));
    assert_eq!(
        withdraw(&cache).unwrap_err(),
        SourceBackendError::PendingFlow
    );
    assert_eq!(address(), original);
    let flow = PENDING_FLOW.with(|slot| slot.borrow_mut().take()).unwrap();
    assert_eq!(flow.shutdown_request().unwrap().exit_code, 29);
}

#[test]
fn pentry_e2b_source_jit_backend_pending_panic_remains_unmaterialized() {
    let cache = fixture();
    let original = address();
    PENDING_SHIM_PANIC.with(|slot| *slot.borrow_mut() = Some("retained-shim-panic".to_owned()));
    assert_eq!(
        withdraw(&cache).unwrap_err(),
        SourceBackendError::PendingPanic
    );
    assert_eq!(address(), original);
    assert_eq!(
        PENDING_SHIM_PANIC
            .with(|slot| slot.borrow_mut().take())
            .as_deref(),
        Some("retained-shim-panic")
    );
}

#[test]
fn pentry_e2b_source_jit_backend_native_frame_publication_is_not_cleared() {
    let cache = fixture();
    let original = address();
    // Refusal only examines the published marker and never dereferences it.
    {
        // SAFETY: `withdraw` only inspects `is_inside_native_extent` and never
        // dereferences the dangling pointer before `_framed` restores `OUTSIDE`.
        let _framed = unsafe { publish_leaf_bases(Some(std::ptr::NonNull::dangling())) };
        assert_eq!(
            withdraw(&cache).unwrap_err(),
            SourceBackendError::NativeFramePublished
        );
        assert!(is_inside_native_extent());
    }
    {
        let _live = enter_armed_live_extent();
        assert_eq!(
            withdraw(&cache).unwrap_err(),
            SourceBackendError::NativeFramePublished
        );
        assert!(is_inside_native_extent());
    }
    assert!(!is_inside_native_extent());
    assert_eq!(address(), original);
}

#[test]
fn pentry_e2b_source_jit_backend_pending_root_sweep_is_not_healed() {
    let cache = fixture();
    let original = address();
    PENDING_ROOT_SWEEP_FLOOR.with(|slot| slot.set(Some(17)));
    assert_eq!(
        withdraw(&cache).unwrap_err(),
        SourceBackendError::RootSweepPending
    );
    assert_eq!(
        PENDING_ROOT_SWEEP_FLOOR.with(|slot| slot.replace(None)),
        Some(17)
    );
    assert_eq!(address(), original);
}

#[test]
fn pentry_e2b_source_jit_backend_unreturned_frontend_scratch_refuses() {
    let cache = fixture();
    let original = address();
    let scratch = SHARED_JIT.with(|cell| cell.borrow_mut().as_mut().unwrap().fbctx.take());
    assert!(scratch.is_some());
    assert_eq!(
        withdraw(&cache).unwrap_err(),
        SourceBackendError::FrontendBusy
    );
    assert_eq!(address(), original);
    SHARED_JIT.with(|cell| cell.borrow_mut().as_mut().unwrap().fbctx = scratch);
}
