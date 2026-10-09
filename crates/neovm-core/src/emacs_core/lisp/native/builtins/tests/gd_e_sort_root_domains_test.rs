//! Root guards restore one mutator's storage on every Rust exit path.
use super::{SortRootGuard, SortRuntime, Value, ValueRooting};
use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::eval::Context;

#[test]
fn sort_root_guard_restores_nested_scopes_in_order() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let baseline = context.specpdl.len();
    {
        let mut outer = SortRootGuard::new(&mut context, ValueRooting::Required);
        outer.runtime().root_sort_slot(Value::fixnum(11));
        let outer_len = outer.runtime().specpdl.len();
        {
            let mut inner = SortRootGuard::new(outer.runtime(), ValueRooting::Required);
            inner.runtime().root_sort_slot(Value::fixnum(22));
            assert_eq!(inner.runtime().specpdl.len(), outer_len + 1);
        }
        assert_eq!(outer.runtime().specpdl.len(), outer_len);
    }
    assert_eq!(context.specpdl.len(), baseline);
}

#[test]
fn sort_root_guard_restores_on_flow_propagation() {
    fn fail(context: &mut Context) -> Result<(), Flow> {
        let mut roots = SortRootGuard::new(context, ValueRooting::Required);
        roots.runtime().root_sort_slot(Value::fixnum(33));
        Err(signal(LispCondition::Error, vec![]))?;
        Ok(())
    }
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let baseline = context.specpdl.len();
    assert!(fail(&mut context).is_err());
    assert_eq!(context.specpdl.len(), baseline);
}

#[test]
fn sort_root_guard_restores_on_rust_unwind() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let baseline = context.specpdl.len();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut roots = SortRootGuard::new(&mut context, ValueRooting::Required);
        roots.runtime().root_sort_slot(Value::fixnum(44));
        panic!("exercise guard cleanup during a Rust unwind");
    }));
    assert!(result.is_err());
    assert_eq!(context.specpdl.len(), baseline);
}
