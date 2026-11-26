use super::{
    FormatOutput, FormatResultStorage, FormatStorageBytes, FormatStringEncoding,
    allocate_format_bytes, format_source_bytes,
};
use crate::emacs_core::eval::AllocationFailureContextExt;
use crate::emacs_core::eval::Context;
use neovm_value::alloc::AllocationFailure;
use neovm_value::error::{FlowKind, SignalDelivery};
use neovm_value::value::Value;

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
            let storage = FormatResultStorage::new(bytes, encoding).unwrap();
            let bytes = match (encoding, storage) {
                (FormatStringEncoding::Unibyte, FormatResultStorage::Unibyte(bytes))
                | (FormatStringEncoding::Multibyte, FormatResultStorage::Multibyte(bytes)) => bytes,
                _ => panic!("final storage must retain the validated encoding"),
            };
            assert_eq!(bytes.as_slice(), expected);
            assert_eq!(
                bytes.capacity(),
                bytes.len() + 1,
                "final storage is an exact len + NUL payload"
            );
        }
    }

    let mut context = Context::new(crate::test_utils::unit_runtime());
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

#[test]
fn format_result_storage_copies_the_working_buffer_to_an_exact_payload() {
    crate::test_utils::init_test_tracing();

    // The builder reserves the control string's bound for an unknown output
    // length; final storage copies the finished bytes once into a payload of
    // exactly `len + 1` (GNU's make_specified_string copy, editfns.c:4285),
    // so a retained result never keeps the growth slack.
    let mut output = FormatOutput::new(116, FormatStringEncoding::Unibyte).unwrap();
    output
        .append(&[b'x'; 94])
        .expect("append a candidate-sized field");
    let working_ptr = output.bytes.as_ptr();
    assert_eq!(output.bytes.capacity(), 116);
    let bytes = output.into_bytes();
    assert_eq!(
        bytes.as_ptr(),
        working_ptr,
        "the builder returns its buffer"
    );
    assert_eq!(bytes.capacity(), 116, "the working buffer keeps its bound");

    let storage = FormatResultStorage::new(bytes, FormatStringEncoding::Unibyte).unwrap();
    let payload = match storage {
        FormatResultStorage::Unibyte(bytes) => bytes,
        _ => panic!("final storage must retain the validated encoding"),
    };
    assert_eq!(payload.len(), 94);
    assert_eq!(payload.capacity(), 95, "payload must be len + NUL");

    // A large width or precision is one reserve and one fill, not a loop of
    // small appends.
    let mut padded = FormatOutput::new(0, FormatStringEncoding::Unibyte).unwrap();
    padded.append_fill(b' ', 1_000_000).expect("fill the field");
    assert_eq!(padded.bytes.len(), 1_000_000);
    assert!(padded.bytes.iter().all(|&byte| byte == b' '));
    let storage = FormatResultStorage::new(padded.into_bytes(), FormatStringEncoding::Unibyte)
        .expect("validated padded storage");
    let payload = match storage {
        FormatResultStorage::Unibyte(bytes) => bytes,
        _ => panic!("final storage must retain the validated encoding"),
    };
    assert_eq!((payload.len(), payload.capacity()), (1_000_000, 1_000_001));
}
