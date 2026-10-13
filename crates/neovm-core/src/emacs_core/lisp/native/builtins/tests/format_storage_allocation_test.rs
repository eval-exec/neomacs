use super::{
    FormatOutput, FormatResultStorage, FormatStorageBytes, FormatStringEncoding,
    allocate_format_bytes, format_source_bytes,
};
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

    // Final storage distinguishes canonical byte capacity from the actual
    // unibyte extent, and guarantees a terminator without infallible growth.
    let canonical_byte = format_source_bytes(&[0xff], FormatStringEncoding::Unibyte).unwrap();
    for (encoding, canonical, expected) in [
        (FormatStringEncoding::Unibyte, &b""[..], &b""[..]),
        (FormatStringEncoding::Multibyte, &b""[..], &b""[..]),
        (FormatStringEncoding::Unibyte, &b"ASCII"[..], &b"ASCII"[..]),
        (
            FormatStringEncoding::Multibyte,
            &b"ASCII"[..],
            &b"ASCII"[..],
        ),
        (
            FormatStringEncoding::Multibyte,
            "é界".as_bytes(),
            "é界".as_bytes(),
        ),
        (
            FormatStringEncoding::Unibyte,
            canonical_byte.as_slice(),
            &[0xff][..],
        ),
    ] {
        for spare_terminator in [false, true] {
            let capacity = canonical.len() + usize::from(spare_terminator);
            let mut bytes =
                allocate_format_bytes(FormatStorageBytes::try_from(capacity).unwrap()).unwrap();
            bytes.extend_from_slice(canonical);
            let pointer = bytes.as_ptr();
            let storage = FormatResultStorage::new(bytes, encoding).unwrap();
            let bytes = match (encoding, storage) {
                (FormatStringEncoding::Unibyte, FormatResultStorage::Unibyte(bytes))
                | (FormatStringEncoding::Multibyte, FormatResultStorage::Multibyte(bytes)) => bytes,
                _ => panic!("final storage must retain the validated encoding"),
            };
            assert_eq!(bytes.as_slice(), expected);
            assert!(bytes.len() < bytes.capacity());
            if spare_terminator {
                assert_eq!(bytes.as_ptr(), pointer, "adequate storage must be retained");
                assert_eq!(bytes.capacity(), capacity);
            }
        }
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
