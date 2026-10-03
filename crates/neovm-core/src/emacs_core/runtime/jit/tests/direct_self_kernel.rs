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
}
