use super::*;

fn producer_only_subr(_eval: &mut Context) -> crate::emacs_core::error::EvalResult {
    Ok(crate::emacs_core::value::Value::NIL)
}

fn one_argument_subr(
    _eval: &mut Context,
    value: crate::emacs_core::value::Value,
) -> crate::emacs_core::error::EvalResult {
    Ok(value)
}

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
fn portable_snapshot_preserves_an_integer_beyond_the_wasm32_fixnum_range() {
    const WASM32_MOST_POSITIVE_FIXNUM: i64 = (1_i64 << 29) - 1;
    const INTEGER: i64 = WASM32_MOST_POSITIVE_FIXNUM + 1;

    let mut eval = Context::new();
    eval.set_variable("portable-cross-width-integer", Value::fixnum(INTEGER));

    let image = encode_portable_snapshot(&eval).expect("encode portable snapshot");
    let loaded = load_from_portable_snapshot(&image).expect("load portable snapshot");
    let value = *loaded
        .obarray()
        .symbol_value("portable-cross-width-integer")
        .expect("restored integer");

    assert_eq!(value.as_fixnum(), Some(INTEGER));
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

#[test]
fn portable_snapshot_rejects_a_missing_required_native_subr() {
    let mut producer = Context::new();
    producer.register_subr(crate::emacs_core::subr::SubrSpec::fixed0(
        "portable-producer-only-subr",
        producer_only_subr,
    ));
    let image = encode_portable_snapshot(&producer).expect("encode producer image");

    // Model a different compiled consumer. Portable restore must rebuild the
    // primitive table from this binary, not inherit the producer's process.
    crate::emacs_core::eval::clear_global_subr_table();

    assert!(matches!(
        load_from_portable_snapshot(&image),
        Err(DumpError::PortableRuntimeContractMismatch(message))
            if message.contains("portable-producer-only-subr")
    ));
}

#[test]
fn portable_snapshot_rejects_an_incompatible_native_subr_abi() {
    let mut producer = Context::new();
    producer.register_subr(crate::emacs_core::subr::SubrSpec::fixed0(
        "portable-changed-subr",
        producer_only_subr,
    ));
    let image = encode_portable_snapshot(&producer).expect("encode producer image");

    crate::emacs_core::eval::clear_global_subr_table();
    let mut consumer = Context::new();
    consumer.register_subr(crate::emacs_core::subr::SubrSpec::fixed1(
        "portable-changed-subr",
        one_argument_subr,
        crate::emacs_core::subr::FixedMin1::One,
    ));

    assert!(matches!(
        load_from_portable_snapshot(&image),
        Err(DumpError::PortableRuntimeContractMismatch(message))
            if message.contains("portable-changed-subr")
                && message.contains("incompatible ABI")
    ));
}

#[test]
fn portable_snapshot_accepts_a_consumer_with_additional_native_subrs() {
    let producer = Context::new();
    let image = encode_portable_snapshot(&producer).expect("encode producer image");

    let mut consumer = Context::new();
    consumer.register_subr(crate::emacs_core::subr::SubrSpec::fixed0(
        "portable-consumer-only-subr",
        producer_only_subr,
    ));

    load_from_portable_snapshot(&image).expect("a primitive superset is compatible");
}

#[test]
fn portable_snapshot_file_publish_is_reloadable_and_replaces_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("neomacs.portable");
    std::fs::write(&path, b"stale partial image").unwrap();
    let mut source = Context::new();
    source.set_variable("portable-file-value", Value::fixnum(91));

    dump_portable_snapshot_to_file(&source, &path).expect("publish portable image");
    let bytes = std::fs::read(&path).unwrap();
    let loaded = load_from_portable_snapshot(&bytes).expect("reload portable image");

    assert_eq!(
        loaded.obarray().symbol_value("portable-file-value"),
        Some(&Value::fixnum(91)),
    );
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "publishing must not leave temporary files behind",
    );
}
