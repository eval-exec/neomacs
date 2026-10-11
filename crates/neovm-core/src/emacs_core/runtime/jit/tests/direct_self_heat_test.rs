use super::*;
use direct_call::DirectSelfHeat;

#[test]
fn self_heat_gate_keeps_cold_sources_on_the_shim_and_admits_dispatch_hot_sources() {
    let _policy = SelfPolicy::enter(false, false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    let function = ev
        .obarray
        .symbol_function_id(intern("neovm--self-rec"))
        .expect("defined");
    let runtime = function
        .get_bytecode_data()
        .expect("bytecode")
        .jit_runtime();
    runtime.set_heat_for_test(0);
    for mode in [DirectSelfHeat::Seen, DirectSelfHeat::Hot] {
        force_direct_self_heat_for_test(Some(mode));
        let (cold, sites) = compile_named(&ev, "neovm--self-rec", lowering::RegallocPolicy::Auto);
        assert_eq!(
            cold.abi,
            LeafAbi::Memory,
            "{mode:?}: cold ABI stays on the shim"
        );
        assert_eq!(sites, 0, "{mode:?}: cold source emits no expanded site");
    }
    runtime.set_heat_for_test(crate::emacs_core::jit::hot_threshold());
    let (hot, sites) = compile_named(&ev, "neovm--self-rec", lowering::RegallocPolicy::Auto);
    assert_eq!(hot.abi, LeafAbi::Register { arity: 1 });
    assert_eq!(sites, 1, "dispatch-hot recursion keeps its direct site");
    runtime.set_heat_for_test(0);
    force_direct_self_heat_for_test(Some(DirectSelfHeat::Off));
    let (original, sites) = compile_named(&ev, "neovm--self-rec", lowering::RegallocPolicy::Auto);
    assert_eq!(original.abi, LeafAbi::Register { arity: 1 });
    assert_eq!(sites, 1, "the gate off restores original cold emission");
}
