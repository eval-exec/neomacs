//! Memory-ABI direct entries retain the reference entry shape while
//! bypassing the spec shim. The shared differential harness also runs
//! every exact-call observable through this entry and its forced fallback.

use super::*;

fn setup(register: bool) {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
    force_direct_call_for_test(Some(true));
    force_direct_memory_for_test(Some(true));
    force_register_abi_for_test(Some(register));
    force_direct_sites_for_test(Some(DirectSitesMode::All));
    force_direct_shapes_for_test(Some(DirectShapesKnob::OFF));
}

fn restore() {
    force_direct_call_for_test(None);
    force_direct_memory_for_test(None);
    force_register_abi_for_test(None);
    force_direct_sites_for_test(None);
    force_direct_shapes_for_test(None);
    crate::emacs_core::jit::inline::force_inline_for_test(None);
    crate::emacs_core::jit::force_profit_defer_for_test(None);
}

#[test]
fn memory_direct_calls_keep_the_memory_abi_and_bypass_the_shim() {
    setup(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(ARMING).expect("warm");
    let caller = cached_leaf(&ev, "neovm--dc-caller").expect("caller");
    let callee = cached_leaf(&ev, "neovm--dc-exact").expect("callee");
    assert_eq!(callee.abi, LeafAbi::Memory);
    let slot = slot_calling(caller, callee).expect("slot");
    assert_eq!(slot.direct_entry(), callee.entry);
    assert_eq!(
        slot.direct_consts.load(Ordering::Relaxed) & SpecSlot::KEY_REGISTER,
        0
    );

    // Only the exact callee is direct in this program. Test an exact-only
    // caller to distinguish an engaged direct call from a declined site.
    ev.eval_str(
        r#"(progn
  (defun neovm--memory-caller (n) (if n (neovm--dc-exact n 1) 0))
  (byte-compile 'neovm--memory-caller)
  (dotimes (n 1500) (neovm--memory-caller n)))"#,
    )
    .expect("warm exact caller");
    let calls = SPEC_CALL_COUNT.load(Ordering::Relaxed);
    let value = ev.eval_str("(neovm--memory-caller 8)").expect("run");
    assert_eq!(crate::emacs_core::print::print_value(&value), "7");
    assert_eq!(SPEC_CALL_COUNT.load(Ordering::Relaxed), calls);
    restore();
}

#[test]
fn an_explicit_register_abi_restores_register_direct_entries() {
    setup(true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(ARMING).expect("warm");
    let caller = cached_leaf(&ev, "neovm--dc-caller").expect("caller");
    let callee = cached_leaf(&ev, "neovm--dc-exact").expect("callee");
    assert_eq!(callee.abi, LeafAbi::Register { arity: 2 });
    let slot = slot_calling(caller, callee).expect("slot");
    assert_eq!(slot.direct_entry(), callee.entry);
    assert_ne!(
        slot.direct_consts.load(Ordering::Relaxed) & SpecSlot::KEY_REGISTER,
        0
    );
    restore();
}

#[test]
fn memory_direct_slots_clear_unlink_and_rearm_the_current_entry() {
    setup(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(ARMING).expect("warm");
    let caller = cached_leaf(&ev, "neovm--dc-caller").expect("caller");
    let callee = cached_leaf(&ev, "neovm--dc-exact").expect("callee");
    let slot = slot_calling(caller, callee).expect("slot");
    assert_eq!(callee.abi, LeafAbi::Memory);
    slot.clear_leaf();
    assert!(slot.direct_entry().is_null());
    ev.eval_str("(neovm--dc-caller 5)").expect("rearm");
    assert_eq!(slot.direct_entry(), callee.entry);
    assert_eq!(crate::emacs_core::jit::cache::unlink_spec_slots(callee), 1);
    assert!(slot.direct_entry().is_null());
    ev.eval_str("(neovm--dc-caller 6)")
        .expect("rearm after unlink");
    assert_eq!(slot.direct_entry(), callee.entry);
    restore();
}
