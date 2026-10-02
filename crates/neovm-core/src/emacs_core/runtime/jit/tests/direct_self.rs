//! The self-only direct policy keeps register entries only where a named
//! exact self-call can enter them. Other named and object calls retain their
//! reference entry and frame protocols.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;

/// Test configuration only; restores scalar compiler overrides on drop.
/// Threading: owns only this test's compiler-thread configuration, no Lisp
/// values or shared runtime state.
struct SelfPolicy;

impl SelfPolicy {
    fn enter(register: bool, memory: bool) -> Self {
        crate::test_utils::init_test_tracing();
        force_profit_gate_for_test(false);
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
        force_direct_call_for_test(Some(true));
        force_direct_sites_for_test(Some(DirectSitesMode::SelfOnly));
        force_direct_shapes_for_test(Some(DirectShapesKnob::ALL));
        force_direct_memory_for_test(Some(memory));
        force_register_abi_for_test(Some(register));
        Self
    }
}

impl Drop for SelfPolicy {
    fn drop(&mut self) {
        force_direct_call_for_test(None);
        force_direct_sites_for_test(None);
        force_direct_shapes_for_test(None);
        force_direct_memory_for_test(None);
        force_register_abi_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        crate::emacs_core::jit::force_profit_defer_for_test(None);
    }
}

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

fn compile_named(
    ev: &Context,
    name: &str,
    policy: lowering::RegallocPolicy,
) -> (CompiledLeaf, usize) {
    let function = ev
        .obarray
        .symbol_function_id(intern(name))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    let before = direct_call::direct_sites_emitted_for_test();
    let leaf = compile_bytecode_function_tiered(bc, Some(&ev.obarray), policy).expect("compiles");
    (leaf, direct_call::direct_sites_emitted_for_test() - before)
}

fn cached(ev: &Context, name: &str) -> &'static CompiledLeaf {
    let function = ev
        .obarray
        .symbol_function_id(intern(name))
        .expect("defined");
    let id = function
        .get_bytecode_data()
        .expect("byte-compiled")
        .jit_runtime()
        .compiled_id_or_assign();
    let ptr = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id).expect("cached");
    // SAFETY: the cache retains this leaf for the whole test; no clear runs
    // while the test reads it.
    unsafe { &*ptr }
}

#[test]
fn self_policy_converts_actual_self_calls_but_not_self_symbols_used_as_data() {
    let _policy = SelfPolicy::enter(false, false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    let (recursive, sites) = compile_named(&ev, "neovm--self-rec", lowering::RegallocPolicy::Auto);
    assert_eq!(recursive.abi, LeafAbi::Register { arity: 1 });
    assert_eq!(sites, 1, "one actual recursive call emits one direct site");
    for name in [
        "neovm--self-plain",
        "neovm--self-data",
        "neovm--self-loop",
        "neovm--self-opt",
        "neovm--self-object",
    ] {
        let (leaf, sites) = compile_named(&ev, name, lowering::RegallocPolicy::Full);
        assert_eq!(
            leaf.abi,
            LeafAbi::Memory,
            "{name}: a full request is no self proof"
        );
        assert_eq!(sites, 0, "{name}: no self-only direct site");
        if name == "neovm--self-object" {
            assert_eq!(
                leaf.source_spec_slots().count(),
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
    ev.eval_str("(dotimes (_ 1500) (neovm--self-rec 4))")
        .expect("warm");
    let leaf = cached(&ev, "neovm--self-rec");
    assert_eq!(leaf.abi, LeafAbi::Register { arity: 1 });
    let slot = leaf
        .bytecode_spec_slots()
        .find(|slot| std::ptr::eq(slot.leaf_ptr(), leaf))
        .expect("recursive slot");
    assert_eq!(slot.direct_entry(), leaf.entry);
    let depth = ev.depth;
    let spec = ev.specpdl.len();
    let calls = SPEC_CALL_COUNT.load(Ordering::Relaxed);
    let result = ev
        .eval_str("(neovm--self-rec 20)")
        .expect("direct recursion");
    assert_eq!(crate::emacs_core::print::print_value(&result), "20");
    assert_eq!(
        SPEC_CALL_COUNT.load(Ordering::Relaxed),
        calls,
        "self calls bypass the shim"
    );

    assert!(crate::emacs_core::jit::cache::unlink_spec_slots(leaf) >= 1);
    assert!(slot.direct_entry().is_null());
    let result = ev
        .eval_str("(neovm--self-rec 20)")
        .expect("rearmed recursion");
    assert_eq!(crate::emacs_core::print::print_value(&result), "20");
    assert_eq!(slot.direct_entry(), leaf.entry);
    let calls = SPEC_CALL_COUNT.load(Ordering::Relaxed);
    ev.eval_str("(neovm--self-rec 20)").expect("direct again");
    assert_eq!(SPEC_CALL_COUNT.load(Ordering::Relaxed), calls);
    assert_eq!(ev.depth, depth);
    assert_eq!(ev.specpdl.len(), spec);
}

#[test]
fn explicit_register_abi_keeps_global_entries_but_only_self_sites_are_direct() {
    let _policy = SelfPolicy::enter(true, true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    for name in ["neovm--self-plain", "neovm--self-data", "neovm--self-loop"] {
        let (leaf, sites) = compile_named(&ev, name, lowering::RegallocPolicy::Full);
        assert_eq!(
            leaf.abi,
            LeafAbi::Register { arity: 1 },
            "{name}: explicit register ABI"
        );
        assert_eq!(
            sites, 0,
            "{name}: explicit ABI never widens self-site reach"
        );
    }
    let (recursive, sites) = compile_named(&ev, "neovm--self-rec", lowering::RegallocPolicy::Auto);
    assert_eq!(recursive.abi, LeafAbi::Register { arity: 1 });
    assert_eq!(sites, 1);
}

#[test]
fn self_policy_baseline_requires_a_scoped_source_and_actual_call() {
    let _policy = SelfPolicy::enter(false, true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    let function = ev
        .obarray
        .symbol_function_id(intern("neovm--self-rec"))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    let build = || {
        lower_leaf_full(
            bc.executable_ops(),
            &bc.constants,
            1,
            bc.executable_gnu_byte_offset_map(),
            Some(&ev.obarray),
            0,
        )
    };
    let before = direct_call::direct_sites_emitted_for_test();
    let standalone = build().expect("standalone baseline");
    assert_eq!(
        standalone.abi,
        LeafAbi::Memory,
        "no parent source outside a request"
    );
    assert_eq!(direct_call::direct_sites_emitted_for_test(), before);
    {
        let _source = direct_call::SelfSourceScope::enter_for(bc, true);
        let baseline = build().expect("scoped baseline");
        assert_eq!(baseline.abi, LeafAbi::Register { arity: 1 });
        assert_eq!(direct_call::direct_sites_emitted_for_test(), before + 1);
    }
    let standalone = build().expect("source scope restored");
    assert_eq!(standalone.abi, LeafAbi::Memory);
}
