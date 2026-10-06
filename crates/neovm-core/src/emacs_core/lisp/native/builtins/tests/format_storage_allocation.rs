use super::{FormatOutput, FormatStorageBytes, FormatStringEncoding, allocate_format_bytes};
use crate::emacs_core::alloc::AllocationFailure;
use crate::emacs_core::error::{FlowKind, SignalDelivery};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn format_initial_storage_retains_vector_ownership_and_live_null_failure() {
    crate::test_utils::init_test_tracing();

    let empty = allocate_format_bytes(FormatStorageBytes::try_from(0).unwrap()).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.capacity(), 0);

    let mut bytes = allocate_format_bytes(FormatStorageBytes::try_from(7).unwrap()).unwrap();
    assert!(bytes.is_empty());
    assert_eq!(bytes.capacity(), 7);
    bytes.extend_from_slice(b"initial");
    bytes.try_reserve_exact(8).unwrap();
    bytes.extend_from_slice(b" storage");
    assert_eq!(bytes, b"initial storage");
    // Drop after ordinary Vec reallocation checks allocation-layout ownership.
    drop(bytes);
    assert!(FormatStorageBytes::try_from(isize::MAX as usize + 1).is_err());

    for encoding in [
        FormatStringEncoding::Unibyte,
        FormatStringEncoding::Multibyte,
    ] {
        let mut output = FormatOutput::new(7, encoding).unwrap();
        output.append(b"initial").unwrap();
        output.append(b"").unwrap();
        assert_eq!(output.bytes, b"initial");
        output.append(b" storage").unwrap();
        output.append(b"!").unwrap();
        output.append(b" again").unwrap();
        assert_eq!(output.bytes, b"initial storage! again");
    }

    let mut context = Context::new();
    let original = context
        .eval_str("(setq memory-signal-data '(error . format-null-allocation))")
        .expect("live original memory-signal-data");
    let flow = AllocationFailure::NullAllocation.into_flow_in_context(&context);
    context
        .eval_str("(setq memory-signal-data nil)")
        .expect("remove context's original error root");
    context
        .eval_str("(garbage-collect)")
        .expect("collect with in-flight failure");
    let FlowKind::Signal(signal) = flow.into_kind() else {
        panic!("allocation failure must signal");
    };
    assert_eq!(signal.symbol_name(), "error");
    assert_eq!(signal.data, vec![Value::symbol("format-null-allocation")]);
    let SignalDelivery::MemoryExhausted(binding) = signal.delivery() else {
        panic!("null allocation must retain GNU OOM delivery policy");
    };
    assert_eq!(binding.original(), original);
}
