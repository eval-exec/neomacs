use super::{PROGRAM, SelfPolicy, compile_named};
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use neovm_eval::test_support::authority::{
    TestDirectSelfHeat, TestLeafAbi, TestRegallocPolicy, force_direct_self_heat_token_for_test,
    leaf_abi_for_test, set_heat_for_test,
};
use neovm_value::intern::intern;

#[test]
fn self_heat_gate_keeps_cold_sources_on_the_shim_and_admits_dispatch_hot_sources() {
    let _policy = SelfPolicy::enter(false, false);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("defined");
    let function = ev
        .obarray()
        .symbol_function_id(intern("neovm--self-rec"))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("bytecode");
    set_heat_for_test(bc, 0);
    for mode in [TestDirectSelfHeat::Seen, TestDirectSelfHeat::Hot] {
        force_direct_self_heat_token_for_test(Some(mode));
        let (cold, sites) = compile_named(&ev, "neovm--self-rec", TestRegallocPolicy::Auto);
        assert_eq!(
            leaf_abi_for_test(&cold),
            TestLeafAbi::Memory,
            "{mode:?}: cold ABI stays on the shim"
        );
        assert_eq!(sites, 0, "{mode:?}: cold source emits no expanded site");
    }
    set_heat_for_test(bc, crate::emacs_core::jit::hot_threshold());
    let (hot, sites) = compile_named(&ev, "neovm--self-rec", TestRegallocPolicy::Auto);
    assert_eq!(leaf_abi_for_test(&hot), TestLeafAbi::Register { arity: 1 });
    assert_eq!(sites, 1, "dispatch-hot recursion keeps its direct site");
    set_heat_for_test(bc, 0);
    force_direct_self_heat_token_for_test(Some(TestDirectSelfHeat::Off));
    let (original, sites) = compile_named(&ev, "neovm--self-rec", TestRegallocPolicy::Auto);
    assert_eq!(
        leaf_abi_for_test(&original),
        TestLeafAbi::Register { arity: 1 }
    );
    assert_eq!(sites, 1, "the gate off restores original cold emission");
}
