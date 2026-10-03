use super::*;

#[test]
fn self_kernel_gate_declines_call_heavy_fallbacks_and_keeps_arithmetic_recursion() {
    let _policy = SelfPolicy::enter(false, false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("arithmetic recursion defined");
    ev.eval_str(
        "(progn
           (defun neovm--self-lookup (key table)
             (if (gethash key table)
                 (gethash key table)
               (neovm--self-lookup key table)))
           (byte-compile 'neovm--self-lookup))",
    )
    .expect("lookup fallback defined");
    for enabled in [false, true, false] {
        force_direct_self_kernel_for_test(Some(enabled));
        let (kernel, sites) = compile_named(&ev, "neovm--self-rec", lowering::RegallocPolicy::Auto);
        assert_eq!(kernel.abi, LeafAbi::Register { arity: 1 });
        assert_eq!(sites, 1, "arithmetic recursion keeps its direct site");
        let (fallback, sites) =
            compile_named(&ev, "neovm--self-lookup", lowering::RegallocPolicy::Auto);
        if enabled {
            assert_eq!(
                fallback.abi,
                LeafAbi::Memory,
                "call-heavy ABI stays on the shim"
            );
            assert_eq!(sites, 0, "call-heavy fallback emits no expanded site");
        } else {
            assert_eq!(fallback.abi, LeafAbi::Register { arity: 2 });
            assert_eq!(sites, 1, "off restores original self emission");
        }
    }
    force_direct_self_kernel_for_test(Some(true));
    force_register_abi_for_test(Some(true));
    let (explicit, sites) =
        compile_named(&ev, "neovm--self-lookup", lowering::RegallocPolicy::Auto);
    assert_eq!(explicit.abi, LeafAbi::Register { arity: 2 });
    assert_eq!(
        sites, 0,
        "explicit register ABI does not admit a declined self site"
    );
}

#[test]
fn self_kernel_recursion_executes_directly_and_declined_lookup_keeps_its_result() {
    let _policy = SelfPolicy::enter(false, false);
    force_direct_self_kernel_for_test(Some(true));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    ev.eval_str("(dotimes (_ 1500) (neovm--self-rec 4))")
        .expect("warm arithmetic recursion");
    let leaf = cached(&ev, "neovm--self-rec");
    assert_eq!(leaf.abi, LeafAbi::Register { arity: 1 });
    let slot = leaf
        .bytecode_spec_slots()
        .find(|slot| std::ptr::eq(slot.leaf_ptr(), leaf))
        .expect("recursive slot");
    assert_eq!(slot.direct_entry(), leaf.entry);
    let depth = ev.depth;
    let bindings = ev.specpdl.len();
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
    assert_eq!(ev.depth, depth);
    assert_eq!(ev.specpdl.len(), bindings);
    let result = ev
        .eval_str(
            "(progn
           (defun neovm--self-lookup-run (key table)
             (if (gethash key table)
                 (gethash key table)
               (neovm--self-lookup-run key table)))
           (byte-compile 'neovm--self-lookup-run)
           (let ((table (make-hash-table)))
             (puthash 'key 7 table)
             (dotimes (_ 1500) (neovm--self-lookup-run 'key table))
             (neovm--self-lookup-run 'key table)))",
        )
        .expect("declined lookup executes");
    assert_eq!(crate::emacs_core::print::print_value(&result), "7");
    let lookup = cached(&ev, "neovm--self-lookup-run");
    assert_eq!(lookup.abi, LeafAbi::Memory);
    assert!(
        lookup
            .bytecode_spec_slots()
            .all(|slot| slot.direct_entry().is_null())
    );
    assert_eq!(ev.depth, depth);
    assert_eq!(ev.specpdl.len(), bindings);
}
