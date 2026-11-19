use super::{request_quit_detach, request_quit_restore};
use crate::emacs_core::eval::Context;
use neovm_value::error::FlowRef;
use neovm_value::intern::intern;
use neovm_value::quit::{ASYNC_ATTENTION, quit_pending};
use neovm_value::value::Value;

fn source() -> Context {
    let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_harness());
    source.setup_thread_locals();
    source
}

#[test]
fn pentry_e2b_quit_detach_bridge_request_survives_until_real_safe_point() {
    let mut source = source();
    let receipt = request_quit_detach(&source.quit_requested).unwrap();
    let bridge = source.quit_requested.clone();
    std::thread::spawn(move || bridge.request())
        .join()
        .expect("bridge raised while detached");
    assert!(source.quit_requested.is_requested());
    assert!(!quit_pending());
    assert_ne!(ASYNC_ATTENTION.load(), 0);
    request_quit_restore(receipt).unwrap();
    assert!(quit_pending());
    let flow = source
        .maybe_quit()
        .expect_err("actual safe point delivers quit");
    assert!(matches!(
        flow.kind(),
        FlowRef::Signal(signal) if signal.symbol == intern("quit")
    ));
    assert!(!source.quit_requested.is_requested());
    source.set_quit_flag_value(Value::NIL);
}

#[test]
fn pentry_e2b_quit_detach_restore_refuses_same_owner_reactivation() {
    let mut source = source();
    let receipt = request_quit_detach(&source.quit_requested).unwrap();
    source.setup_thread_locals();
    let failure =
        request_quit_restore(receipt).expect_err("reactivated original displaced sentinel");
    assert_eq!(
        failure.reason(),
        neovm_value::quit::quit_detach::QuitDetachError::Displaced
    );
}
