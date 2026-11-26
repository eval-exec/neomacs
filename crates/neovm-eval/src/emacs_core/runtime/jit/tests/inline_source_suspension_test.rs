use super::{
    ACTIVE_FUSED, FusedBody, FusedScope, reset_carrier_tls, validate_source_compile_scope,
};
use crate::emacs_core::jit::source_suspension::{SourceScopeRefusal, SourceScopeSlot};
use neovm_value::value::Value;

#[test]
fn pentry_e2b_source_compile_fused_scope_retains_actual_constants() {
    reset_carrier_tls();
    let mut source = Box::new(crate::emacs_core::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    let value = Value::string("fused-source-constant");
    let body = std::rc::Rc::new(FusedBody {
        v2: None,
        ops: Vec::new(),
        constants: vec![value],
        feedback: Vec::new(),
        offset_map: None,
        regions: Vec::new(),
        region_of: Vec::new(),
        caller_of_fused: Vec::new(),
    });
    let scope = FusedScope::enter(std::rc::Rc::clone(&body));
    let owners = std::rc::Rc::strong_count(&body);
    assert_eq!(
        validate_source_compile_scope(),
        Err(SourceScopeRefusal::FusedBody)
    );
    assert_eq!(std::rc::Rc::strong_count(&body), owners);
    ACTIVE_FUSED.with(|slot| {
        assert_eq!(slot.borrow().as_ref().unwrap().constants, vec![value]);
    });
    drop(scope);
    assert!(validate_source_compile_scope().is_ok());
}

#[test]
fn pentry_e2b_source_compile_fused_slot_loan_refuses() {
    reset_carrier_tls();
    ACTIVE_FUSED.with(|slot| {
        let reader = slot.borrow();
        assert_eq!(
            validate_source_compile_scope(),
            Err(SourceScopeRefusal::Borrowed(SourceScopeSlot::FusedBody))
        );
        drop(reader);
    });
    assert!(validate_source_compile_scope().is_ok());
}
