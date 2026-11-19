use super::{
    ACTIVE_BUILD, ActiveBuild, T2Origin, reset_carrier_tls, validate_source_compile_scope,
};
use crate::emacs_core::jit::feedback::RuntimeFeedbackExt;
use crate::emacs_core::jit::source_suspension::{SourceScopeRefusal, SourceScopeSlot};
use std::sync::Arc;

#[test]
fn pentry_e2b_source_compile_tier_build_preserves_actual_runtime_owner() {
    reset_carrier_tls();
    let runtime = super::super::Runtime::new().share_state();
    let build = ActiveBuild {
        origin: T2Origin::Profiling,
        source: Some(Arc::clone(&runtime)),
        window: 8,
        ops_len: 0,
        loop_or_recursive: false,
    };
    let scope = neovm_value::tls_scope::TlsScope::new(&ACTIVE_BUILD, Some(build));
    let owners = Arc::strong_count(&runtime);
    assert_eq!(
        validate_source_compile_scope(),
        Err(SourceScopeRefusal::TierBuild)
    );
    assert_eq!(Arc::strong_count(&runtime), owners);
    drop(scope);
    assert!(validate_source_compile_scope().is_ok());
}

#[test]
fn pentry_e2b_source_compile_tier_build_slot_loan_refuses() {
    reset_carrier_tls();
    ACTIVE_BUILD.with(|slot| {
        let reader = slot.borrow();
        assert_eq!(
            validate_source_compile_scope(),
            Err(SourceScopeRefusal::Borrowed(SourceScopeSlot::TierBuild))
        );
        drop(reader);
    });
    assert!(validate_source_compile_scope().is_ok());
}
