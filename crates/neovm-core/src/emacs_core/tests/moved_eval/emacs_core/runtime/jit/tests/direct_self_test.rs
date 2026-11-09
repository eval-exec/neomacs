//! The self-only direct policy keeps register entries only where a named
//! exact self-call can enter them. Other named and object calls retain their
//! reference entry and frame protocols.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{CompiledLeaf, lower_leaf_full};
use neovm_eval::test_support::authority::{
    TestDirectSelfHeat, TestDirectShapes, TestDirectSitesMode, TestLeafAbi, TestRegallocPolicy,
    bytecode_slot_direct_entry_identity_for_test, bytecode_slot_index_for_callee_for_test,
    compile_with_regalloc_for_test, direct_sites_emitted_for_test,
    executable_gnu_byte_offset_map_for_test, force_direct_call_for_test,
    force_direct_memory_for_test, force_direct_self_heat_token_for_test,
    force_direct_self_kernel_for_test, force_direct_shapes_token_for_test,
    force_direct_sites_token_for_test, force_inline_for_test, force_profit_defer_for_test,
    force_profit_gate_for_test, force_register_abi_for_test, jit_compiled_id_or_assign_for_test,
    leaf_abi_for_test, leaf_entry_identity_for_test, null_identity_for_test,
    prepare_named_for_test, self_source_scope_for_test, source_spec_slot_count_for_test,
    unlink_spec_slots_for_test, with_cached_leaf_for_test,
};
use neovm_value::intern::intern;

/// Test configuration only; restores scalar compiler overrides on drop.
/// Threading: owns only this test's compiler-thread configuration, no Lisp
/// values or shared runtime state.
struct SelfPolicy;

impl SelfPolicy {
    #[doc(hidden)]
    pub fn enter(register: bool, memory: bool) -> Self {
        crate::test_utils::init_test_tracing();
        force_profit_gate_for_test(false);
        force_inline_for_test(Some(false));
        force_profit_defer_for_test(Some(1));
        force_direct_call_for_test(Some(true));
        force_direct_sites_token_for_test(Some(TestDirectSitesMode::SelfOnly));
        force_direct_self_heat_token_for_test(Some(TestDirectSelfHeat::Off));
        force_direct_self_kernel_for_test(Some(false));
        force_direct_shapes_token_for_test(Some(TestDirectShapes::ALL));
        force_direct_memory_for_test(Some(memory));
        force_register_abi_for_test(Some(register));
        Self
    }
}

impl Drop for SelfPolicy {
    fn drop(&mut self) {
        force_direct_call_for_test(None);
        force_direct_sites_token_for_test(None);
        force_direct_self_heat_token_for_test(None);
        force_direct_self_kernel_for_test(None);
        force_direct_shapes_token_for_test(None);
        force_direct_memory_for_test(None);
        force_register_abi_for_test(None);
        force_inline_for_test(None);
        force_profit_defer_for_test(None);
    }
}

#[cfg(test)]
#[path = "direct_self_heat_test.rs"]
mod heat_tests;

#[cfg(test)]
#[path = "direct_self_kernel_test.rs"]
mod kernel_tests;

const PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defun neovm--self-rec (n)
    (if (= n 0) 0 (1+ (neovm--self-rec (1- n)))))
  (defun neovm--self-plain (n) (if (> n 0) n 0))
  (defun neovm--self-data (n)
    (list 'neovm--self-data (neovm--self-plain n)))
  (defun neovm--self-loop (n)
    (while (> n 0) (setq n (neovm--self-plain (1- n)))) n)
  (defun neovm--self-opt (n &optional acc)
    (if (= n 0) acc (neovm--self-opt (1- n) acc)))
  (defun neovm--self-object (n)
    (cl-flet ((local (x) (list (neovm--self-plain x))))
      (list (local n) (local (1+ n)))))
  (dolist (f '(neovm--self-rec neovm--self-plain neovm--self-data
               neovm--self-loop neovm--self-opt neovm--self-object))
    (byte-compile f)))"#;

fn compile_named(ev: &Context, name: &str, policy: TestRegallocPolicy) -> (CompiledLeaf, usize) {
    let function = ev
        .obarray()
        .symbol_function_id(intern(name))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    let before = direct_sites_emitted_for_test();
    let leaf = compile_with_regalloc_for_test(bc, Some(ev.obarray()), policy).expect("compiles");
    (leaf, direct_sites_emitted_for_test() - before)
}

fn cached(ev: &Context, name: &str) -> u64 {
    let function = ev
        .obarray()
        .symbol_function_id(intern(name))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    jit_compiled_id_or_assign_for_test(&bc)
}

#[test]
fn self_policy_converts_actual_self_calls_but_not_self_symbols_used_as_data() {
    let _policy = SelfPolicy::enter(false, false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    let (recursive, sites) = compile_named(&ev, "neovm--self-rec", TestRegallocPolicy::Auto);
    assert_eq!(
        leaf_abi_for_test(&recursive),
        TestLeafAbi::Register { arity: 1 }
    );
    assert_eq!(sites, 1, "one actual recursive call emits one direct site");
    for name in [
        "neovm--self-plain",
        "neovm--self-data",
        "neovm--self-loop",
        "neovm--self-opt",
        "neovm--self-object",
    ] {
        let (leaf, sites) = compile_named(&ev, name, TestRegallocPolicy::Full);
        assert_eq!(
            leaf_abi_for_test(&leaf),
            TestLeafAbi::Memory,
            "{name}: a full request is no self proof"
        );
        assert_eq!(sites, 0, "{name}: no self-only direct site");
        if name == "neovm--self-object" {
            assert_eq!(
                source_spec_slot_count_for_test(&leaf),
                0,
                "constant reach declines"
            );
        }
    }
}

#[test]
fn self_policy_recursion_bypasses_the_shim_and_rearms_after_unlink() {
    // Memory opt-in must not change the self site's original register call.
    let _policy = SelfPolicy::enter(false, true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    prepare_named_for_test(&mut ev, &["neovm--self-rec"]);
    ev.eval_str("(dotimes (_ 1500) (neovm--self-rec 4))")
        .expect("warm");
    with_cached_leaf_for_test(cached(&ev, "neovm--self-rec"), |leaf| {
        assert_eq!(leaf_abi_for_test(leaf), TestLeafAbi::Register { arity: 1 });
        let slot = bytecode_slot_index_for_callee_for_test(leaf, leaf).expect("recursive slot");
        assert_eq!(
            bytecode_slot_direct_entry_identity_for_test(leaf, slot),
            leaf_entry_identity_for_test(leaf)
        );
        let depth = ev.depth_for_test();
        let spec = ev.specpdl().len();
        let calls = neovm_eval::test_support::spec_call_count_for_test();
        let result = ev
            .eval_str("(neovm--self-rec 20)")
            .expect("direct recursion");
        assert_eq!(crate::emacs_core::print::print_value(&result), "20");
        assert_eq!(
            neovm_eval::test_support::spec_call_count_for_test(),
            calls,
            "self calls bypass the shim"
        );

        assert!(unlink_spec_slots_for_test(leaf) >= 1);
        assert_eq!(
            bytecode_slot_direct_entry_identity_for_test(leaf, slot),
            null_identity_for_test()
        );
        let result = ev
            .eval_str("(neovm--self-rec 20)")
            .expect("rearmed recursion");
        assert_eq!(crate::emacs_core::print::print_value(&result), "20");
        assert_eq!(
            bytecode_slot_direct_entry_identity_for_test(leaf, slot),
            leaf_entry_identity_for_test(leaf)
        );
        let calls = neovm_eval::test_support::spec_call_count_for_test();
        ev.eval_str("(neovm--self-rec 20)").expect("direct again");
        assert_eq!(neovm_eval::test_support::spec_call_count_for_test(), calls);
        assert_eq!(ev.depth_for_test(), depth);
        assert_eq!(ev.specpdl().len(), spec);
    })
    .expect("cached");
}

#[test]
fn explicit_register_abi_keeps_global_entries_but_only_self_sites_are_direct() {
    let _policy = SelfPolicy::enter(true, true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    for name in ["neovm--self-plain", "neovm--self-data", "neovm--self-loop"] {
        let (leaf, sites) = compile_named(&ev, name, TestRegallocPolicy::Full);
        assert_eq!(
            leaf_abi_for_test(&leaf),
            TestLeafAbi::Register { arity: 1 },
            "{name}: explicit register ABI"
        );
        assert_eq!(
            sites, 0,
            "{name}: explicit ABI never widens self-site reach"
        );
    }
    let (recursive, sites) = compile_named(&ev, "neovm--self-rec", TestRegallocPolicy::Auto);
    assert_eq!(
        leaf_abi_for_test(&recursive),
        TestLeafAbi::Register { arity: 1 }
    );
    assert_eq!(sites, 1);
}

#[test]
fn self_policy_baseline_requires_a_scoped_source_and_actual_call() {
    let _policy = SelfPolicy::enter(false, true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    let function = ev
        .obarray()
        .symbol_function_id(intern("neovm--self-rec"))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    let build = || {
        let ops = crate::emacs_core::bytecode::executable_ops_for_test(bc);
        let map = executable_gnu_byte_offset_map_for_test(bc);
        lower_leaf_full(
            &ops,
            &bc.constants,
            1,
            map.as_deref(),
            Some(ev.obarray()),
            0,
        )
    };
    let before = direct_sites_emitted_for_test();
    let standalone = build().expect("standalone baseline");
    assert_eq!(
        leaf_abi_for_test(&standalone),
        TestLeafAbi::Memory,
        "no parent source outside a request"
    );
    assert_eq!(direct_sites_emitted_for_test(), before);
    {
        let _source = self_source_scope_for_test(bc, true, false);
        let baseline = build().expect("scoped baseline");
        assert_eq!(
            leaf_abi_for_test(&baseline),
            TestLeafAbi::Register { arity: 1 }
        );
        assert_eq!(direct_sites_emitted_for_test(), before + 1);
    }
    let standalone = build().expect("source scope restored");
    assert_eq!(leaf_abi_for_test(&standalone), TestLeafAbi::Memory);
}
