use super::{PROGRAM, SelfPolicy, cached, compile_named};
use neovm_eval::test_support::authority::{
    TestLeafAbi, TestRegallocPolicy, bytecode_slot_direct_entry_identity_for_test,
    bytecode_slot_index_for_callee_for_test, bytecode_spec_slots_all_direct_entries_null_for_test,
    eval_fixture_program, force_direct_self_kernel_for_test, force_register_abi_for_test,
    leaf_abi_for_test, leaf_entry_identity_for_test, prepare_named_for_test,
    with_cached_leaf_for_test,
};

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
        let (kernel, sites) = compile_named(&ev, "neovm--self-rec", TestRegallocPolicy::Auto);
        assert_eq!(
            leaf_abi_for_test(&kernel),
            TestLeafAbi::Register { arity: 1 }
        );
        assert_eq!(sites, 1, "arithmetic recursion keeps its direct site");
        let (fallback, sites) = compile_named(&ev, "neovm--self-lookup", TestRegallocPolicy::Auto);
        if enabled {
            assert_eq!(
                leaf_abi_for_test(&fallback),
                TestLeafAbi::Memory,
                "call-heavy ABI stays on the shim"
            );
            assert_eq!(sites, 0, "call-heavy fallback emits no expanded site");
        } else {
            assert_eq!(
                leaf_abi_for_test(&fallback),
                TestLeafAbi::Register { arity: 2 }
            );
            assert_eq!(sites, 1, "off restores original self emission");
        }
    }
    force_direct_self_kernel_for_test(Some(true));
    force_register_abi_for_test(Some(true));
    let (explicit, sites) = compile_named(&ev, "neovm--self-lookup", TestRegallocPolicy::Auto);
    assert_eq!(
        leaf_abi_for_test(&explicit),
        TestLeafAbi::Register { arity: 2 }
    );
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
    prepare_named_for_test(&mut ev, &["neovm--self-rec"]);
    ev.eval_str("(dotimes (_ 1500) (neovm--self-rec 4))")
        .expect("warm arithmetic recursion");
    with_cached_leaf_for_test(cached(&ev, "neovm--self-rec"), |leaf| {
        assert_eq!(leaf_abi_for_test(leaf), TestLeafAbi::Register { arity: 1 });
        let slot = bytecode_slot_index_for_callee_for_test(leaf, leaf).expect("recursive slot");
        assert_eq!(
            bytecode_slot_direct_entry_identity_for_test(leaf, slot),
            leaf_entry_identity_for_test(leaf)
        );
        let depth = ev.depth_for_test();
        let bindings = ev.specpdl().len();
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
        assert_eq!(ev.depth_for_test(), depth);
        assert_eq!(ev.specpdl().len(), bindings);
        let result = eval_fixture_program(
            &mut ev,
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
        with_cached_leaf_for_test(cached(&ev, "neovm--self-lookup-run"), |lookup| {
            assert_eq!(leaf_abi_for_test(lookup), TestLeafAbi::Memory);
            assert!(bytecode_spec_slots_all_direct_entries_null_for_test(lookup));
        })
        .expect("cached");
        assert_eq!(ev.depth_for_test(), depth);
        assert_eq!(ev.specpdl().len(), bindings);
    })
    .expect("cached");
}
