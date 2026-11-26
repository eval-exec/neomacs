use super::{
    ACTIVE_CALL_SOURCE, ACTIVE_FEEDBACK_HOLDS, CallSourceScope, FeedbackHolds, hold_for_leaf,
    reset_carrier_tls, validate_source_compile_scope,
};
use crate::emacs_core::jit::Runtime;
use crate::emacs_core::jit::feedback::RuntimeFeedbackExt;
use crate::emacs_core::jit::source_suspension::{SourceScopeRefusal, SourceScopeSlot};
use std::sync::Arc;

#[test]
fn pentry_e2b_source_compile_call_source_retains_actual_runtime_owner() {
    reset_carrier_tls();
    let state = Runtime::new().share_state();
    let scope = CallSourceScope::enter(Some(Arc::clone(&state)), true);
    let owners = Arc::strong_count(&state);
    assert_eq!(
        validate_source_compile_scope(),
        Err(SourceScopeRefusal::CallSource)
    );
    assert_eq!(Arc::strong_count(&state), owners);
    drop(scope);
    assert!(validate_source_compile_scope().is_ok());
}

#[test]
fn pentry_e2b_source_compile_feedback_holds_are_not_consumed() {
    reset_carrier_tls();
    let state = Runtime::new().share_state();
    let holds = FeedbackHolds::enter();
    hold_for_leaf(&state);
    let owners = Arc::strong_count(&state);
    assert_eq!(
        validate_source_compile_scope(),
        Err(SourceScopeRefusal::FeedbackHolds)
    );
    assert_eq!(Arc::strong_count(&state), owners);
    let finished = holds.finish();
    assert!(Arc::ptr_eq(&finished[0], &state));
    assert!(validate_source_compile_scope().is_ok());
}

#[test]
fn pentry_e2b_source_compile_feedback_every_slot_loan_refuses() {
    reset_carrier_tls();
    ACTIVE_CALL_SOURCE.with(|slot| {
        let reader = slot.borrow();
        assert_eq!(
            validate_source_compile_scope(),
            Err(SourceScopeRefusal::Borrowed(SourceScopeSlot::CallSource))
        );
        drop(reader);
    });
    ACTIVE_FEEDBACK_HOLDS.with(|slot| {
        let reader = slot.borrow();
        assert_eq!(
            validate_source_compile_scope(),
            Err(SourceScopeRefusal::Borrowed(SourceScopeSlot::FeedbackHolds))
        );
        drop(reader);
    });
    assert!(validate_source_compile_scope().is_ok());
}
