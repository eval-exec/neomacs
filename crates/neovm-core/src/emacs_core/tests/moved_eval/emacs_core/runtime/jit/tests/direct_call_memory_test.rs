//! Memory-ABI direct entries retain the reference entry shape while
//! bypassing the spec shim. The shared differential harness also runs
//! every exact-call observable through this entry and its forced fallback.

use super::{ARMING, cached_leaf_id};
use neovm_eval::test_support::authority::{
    TestDirectShapes, TestDirectSitesMode, TestLeafAbi, bytecode_slot_clear_leaf_for_test,
    bytecode_slot_direct_entry_identity_for_test, bytecode_slot_index_for_callee_for_test,
    bytecode_slot_register_key_for_test, eval_fixture_program, force_direct_call_for_test,
    force_direct_memory_for_test, force_direct_shapes_token_for_test,
    force_direct_sites_token_for_test, force_inline_for_test, force_profit_defer_for_test,
    force_profit_gate_for_test, force_register_abi_for_test, leaf_abi_for_test,
    leaf_entry_identity_for_test, null_identity_for_test, unlink_spec_slots_for_test,
    with_cached_leaf_for_test,
};

fn setup(register: bool) {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    force_inline_for_test(Some(false));
    force_profit_defer_for_test(Some(1));
    force_direct_call_for_test(Some(true));
    force_direct_memory_for_test(Some(true));
    force_register_abi_for_test(Some(register));
    force_direct_sites_token_for_test(Some(TestDirectSitesMode::All));
    force_direct_shapes_token_for_test(Some(TestDirectShapes::OFF));
}

fn restore() {
    force_direct_call_for_test(None);
    force_direct_memory_for_test(None);
    force_register_abi_for_test(None);
    force_direct_sites_token_for_test(None);
    force_direct_shapes_token_for_test(None);
    force_inline_for_test(None);
    force_profit_defer_for_test(None);
}

#[test]
fn memory_direct_calls_keep_the_memory_abi_and_bypass_the_shim() {
    setup(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    eval_fixture_program(&mut ev, ARMING).expect("warm");
    let caller_id = cached_leaf_id(&ev, "neovm--dc-caller").expect("caller");
    let callee_id = cached_leaf_id(&ev, "neovm--dc-exact").expect("callee");
    with_cached_leaf_for_test(caller_id, |caller| {
        with_cached_leaf_for_test(callee_id, |callee| {
            assert_eq!(leaf_abi_for_test(callee), TestLeafAbi::Memory);
            let slot = bytecode_slot_index_for_callee_for_test(caller, callee).expect("slot");
            assert_eq!(
                bytecode_slot_direct_entry_identity_for_test(caller, slot),
                leaf_entry_identity_for_test(callee)
            );
            assert_eq!(bytecode_slot_register_key_for_test(caller, slot), 0);
        })
        .expect("callee");
    })
    .expect("caller");

    // Only the exact callee is direct in this program. Test an exact-only
    // caller to distinguish an engaged direct call from a declined site.
    eval_fixture_program(
        &mut ev,
        r#"(progn
  (defun neovm--memory-caller (n) (if n (neovm--dc-exact n 1) 0))
  (byte-compile 'neovm--memory-caller)
  (dotimes (n 1500) (neovm--memory-caller n)))"#,
    )
    .expect("warm exact caller");
    let calls = neovm_eval::test_support::spec_call_count_for_test();
    let value = ev.eval_str("(neovm--memory-caller 8)").expect("run");
    assert_eq!(crate::emacs_core::print::print_value(&value), "7");
    assert_eq!(neovm_eval::test_support::spec_call_count_for_test(), calls);
    restore();
}

#[test]
fn an_explicit_register_abi_restores_register_direct_entries() {
    setup(true);
    let mut ev = crate::test_utils::runtime_startup_context();
    eval_fixture_program(&mut ev, ARMING).expect("warm");
    let caller_id = cached_leaf_id(&ev, "neovm--dc-caller").expect("caller");
    let callee_id = cached_leaf_id(&ev, "neovm--dc-exact").expect("callee");
    with_cached_leaf_for_test(caller_id, |caller| {
        with_cached_leaf_for_test(callee_id, |callee| {
            assert_eq!(
                leaf_abi_for_test(callee),
                TestLeafAbi::Register { arity: 2 }
            );
            let slot = bytecode_slot_index_for_callee_for_test(caller, callee).expect("slot");
            assert_eq!(
                bytecode_slot_direct_entry_identity_for_test(caller, slot),
                leaf_entry_identity_for_test(callee)
            );
            assert_ne!(bytecode_slot_register_key_for_test(caller, slot), 0);
        })
        .expect("callee");
    })
    .expect("caller");
    restore();
}

#[test]
fn memory_direct_slots_clear_unlink_and_rearm_the_current_entry() {
    setup(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    eval_fixture_program(&mut ev, ARMING).expect("warm");
    let caller_id = cached_leaf_id(&ev, "neovm--dc-caller").expect("caller");
    let callee_id = cached_leaf_id(&ev, "neovm--dc-exact").expect("callee");
    with_cached_leaf_for_test(caller_id, |caller| {
        with_cached_leaf_for_test(callee_id, |callee| {
            let slot = bytecode_slot_index_for_callee_for_test(caller, callee).expect("slot");
            assert_eq!(leaf_abi_for_test(callee), TestLeafAbi::Memory);
            bytecode_slot_clear_leaf_for_test(caller, slot);
            assert!(
                bytecode_slot_direct_entry_identity_for_test(caller, slot)
                    == null_identity_for_test()
            );
            ev.eval_str("(neovm--dc-caller 5)").expect("rearm");
            assert_eq!(
                bytecode_slot_direct_entry_identity_for_test(caller, slot),
                leaf_entry_identity_for_test(callee)
            );
            assert_eq!(unlink_spec_slots_for_test(callee), 1);
            assert!(
                bytecode_slot_direct_entry_identity_for_test(caller, slot)
                    == null_identity_for_test()
            );
            ev.eval_str("(neovm--dc-caller 6)")
                .expect("rearm after unlink");
            assert_eq!(
                bytecode_slot_direct_entry_identity_for_test(caller, slot),
                leaf_entry_identity_for_test(callee)
            );
        })
        .expect("callee");
    })
    .expect("caller");
    restore();
}
