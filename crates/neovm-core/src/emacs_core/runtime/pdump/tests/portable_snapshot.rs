use super::*;

#[test]
fn portable_snapshot_round_trips_evaluator_state() {
    let mut eval = Context::new();
    eval.obarray
        .set_symbol_value("portable-snapshot-value", Value::fixnum(42));

    let image = encode_portable_snapshot(&eval).expect("encode portable snapshot");
    let mut loaded = load_from_portable_snapshot(&image).expect("load portable snapshot");

    assert_eq!(
        loaded.obarray.symbol_value("portable-snapshot-value"),
        Some(&Value::fixnum(42))
    );
    assert!(take_after_pdump_load_hook_pending(&mut loaded));
    assert!(!take_after_pdump_load_hook_pending(&mut loaded));
}

#[test]
fn portable_snapshot_rejects_corrupted_payload() {
    let image = encode_portable_snapshot(&Context::new()).expect("encode portable snapshot");
    let mut corrupted = image;
    *corrupted.last_mut().expect("snapshot payload") ^= 0x01;

    assert!(matches!(
        load_from_portable_snapshot(&corrupted),
        Err(DumpError::ChecksumMismatch)
    ));
}

#[test]
fn portable_snapshot_rejects_unknown_schema_version() {
    let image = encode_portable_snapshot(&Context::new()).expect("encode portable snapshot");
    let mut incompatible = image;
    incompatible[16..20].copy_from_slice(&u32::MAX.to_le_bytes());

    assert!(matches!(
        load_from_portable_snapshot(&incompatible),
        Err(DumpError::UnsupportedVersion(u32::MAX))
    ));
}
