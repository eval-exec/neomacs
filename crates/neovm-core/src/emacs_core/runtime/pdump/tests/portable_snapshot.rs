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
