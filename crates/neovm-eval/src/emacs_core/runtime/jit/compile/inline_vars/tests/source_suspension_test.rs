use super::{CompileEnv, CompileEnvState, ENV, reset_carrier_tls, validate_source_compile_scope};
use crate::emacs_core::Context;
use crate::emacs_core::jit::source_suspension::SourceScopeRefusal;

#[test]
fn pentry_e2b_source_compile_environment_accepts_absent() {
    reset_carrier_tls();
    assert!(validate_source_compile_scope().is_ok());
}

#[test]
fn pentry_e2b_source_compile_environment_preserves_actual_sim_and_green_pointers() {
    reset_carrier_tls();
    let mut source = Box::new(Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    let env = CompileEnv {
        obarray: std::ptr::from_ref(&source.obarray),
        projection: std::ptr::from_ref(source.runtime_projection_mask()),
    };
    for state in [CompileEnvState::Sim(env), CompileEnvState::Green(env)] {
        let scope = neovm_value::tls_scope::TlsScope::new(&ENV, state);
        assert_eq!(
            validate_source_compile_scope(),
            Err(SourceScopeRefusal::CompileEnvironment)
        );
        let observed = match ENV.with(std::cell::Cell::get) {
            CompileEnvState::Sim(env) | CompileEnvState::Green(env) => env,
            CompileEnvState::Absent => panic!("live environment was cleared"),
        };
        assert_eq!(observed.obarray, env.obarray);
        assert!(std::ptr::eq(observed.projection, env.projection));
        drop(scope);
        assert!(validate_source_compile_scope().is_ok());
    }
}
