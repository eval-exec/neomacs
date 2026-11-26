use super::super::super::parallel_entry::source_entry::{
    SourceEntryRefusal, install_inactive_source_module_probe_for_test, validate_serial_source_entry,
};
use super::super::Context;
use crate::emacs_core::eval::admission::ThreadingMode;

fn source() -> Context {
    install_inactive_source_module_probe_for_test();
    let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_harness());
    source.setup_thread_locals();
    source
}

#[test]
fn pentry_e2b_source_entry_world_green_registry_reader_refuses() {
    let source = source();
    let view = source.obarray.lease.0.green.borrow();
    assert!(view.is_none());
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::GreenSlotBorrowed)
    );
    assert!(view.is_none());
}

#[test]
fn pentry_e2b_source_entry_world_green_registry_writer_refuses() {
    let source = source();
    let view = source.obarray.lease.0.green.borrow_mut();
    assert!(view.is_none());
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::GreenSlotBorrowed)
    );
    assert!(view.is_none());
}

#[test]
fn pentry_e2b_source_entry_world_existing_green_runtime_refuses() {
    install_inactive_source_module_probe_for_test();
    let mut source =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_harness());
    let baton = source
        .start_green_runtime()
        .expect("actual pre-existing Green runtime");
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::ExistingGreenRuntime)
    );
    drop(baton);
}
