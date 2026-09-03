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

#[cfg(not(target_family = "wasm"))]
#[test]
fn native_producer_excludes_target_only_subrs_from_the_portable_contract() {
    let producer = Context::new();
    let target_only_subr = std::cfg_select! {
        any(target_os = "linux", target_os = "android") => { "inotify-add-watch" }
        target_os = "macos" => { "kqueue-add-watch" }
        windows => { "w32-short-file-name" }
        _ => { return }
    };
    assert!(
        producer
            .obarray()
            .symbol_function(target_only_subr)
            .is_some(),
        "the fixture must be compiled into this native producer",
    );

    let image = encode_portable_snapshot(&producer).expect("encode native-produced image");
    let requirements = portable_required_subr_names(&image);

    assert!(
        !requirements.iter().any(|name| name == target_only_subr),
        "portable ABI contract leaked compile-target-only subr {target_only_subr}",
    );
}

#[test]
fn portable_restore_discards_a_producer_only_subr_cell() {
    let mut producer = Context::new();
    let spec =
        crate::emacs_core::subr::SubrSpec::fixed0("portable-target-only-subr", producer_only_subr);
    producer.register_subrs_with_portability(
        &[spec],
        crate::emacs_core::subr::SubrPortability::TargetSpecific,
    );
    let image = encode_portable_snapshot(&producer).expect("encode producer image");

    // Model a consumer binary that did not compile the producer's primitive.
    crate::emacs_core::eval::clear_global_subr_table();
    let loaded = load_from_portable_snapshot(&image).expect("load portable image");

    assert!(
        !loaded.obarray().fboundp("portable-target-only-subr"),
        "the consumer must not retain an uncallable producer subr object",
    );
}

#[test]
fn portable_restore_rebinds_compiled_target_identity() {
    let unavailable_feature = crate::emacs_core::c_features::gnu_c_features()
        .into_iter()
        .find(|feature| !feature.here.provided())
        .expect("the test build should omit at least one GNU C feature")
        .name;
    let user_feature = "portable-user-feature";
    let mut producer = Context::new();
    producer.set_variable("system-type", Value::symbol("producer-system"));
    producer.set_variable(
        "features",
        Value::list(vec![
            Value::symbol(unavailable_feature),
            Value::symbol(user_feature),
        ]),
    );
    producer.refresh_features_from_variable();

    let image = encode_portable_snapshot(&producer).expect("encode producer image");
    let loaded = load_from_portable_snapshot(&image).expect("load portable image");
    let expected_system_type = std::cfg_select! {
        target_family = "wasm" => { "wasm" }
        target_os = "android" => { "android" }
        target_os = "windows" => { "windows-nt" }
        target_os = "macos" => { "darwin" }
        target_os = "linux" => { "gnu/linux" }
        _ => { std::env::consts::OS }
    };

    assert_eq!(
        loaded.obarray().symbol_value("system-type"),
        Some(&Value::symbol(expected_system_type)),
    );
    assert!(
        loaded
            .features
            .contains(&crate::emacs_core::intern::intern(user_feature))
    );
    assert!(
        !loaded
            .features
            .contains(&crate::emacs_core::intern::intern(unavailable_feature)),
        "the producer's unavailable C feature must not survive restore",
    );
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
