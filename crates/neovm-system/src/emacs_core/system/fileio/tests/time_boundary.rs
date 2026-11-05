use super::*;

#[cfg(unix)]
#[test]
fn file_timestamp_before_epoch_matches_gnu() {
    // GNU 31.1 sandboxed oracle: tmp/tsb-time/file-time.gnu.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pre-epoch");
    fs::write(&path, b"x").unwrap();
    let mut eval = Context::new();
    assert_eq!(
        builtin_set_file_times(
            &mut eval,
            vec![Value::string(path.to_string_lossy()), Value::fixnum(-1)]
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        std::time::UNIX_EPOCH - std::time::Duration::from_secs(1)
    );
    builtin_insert_file_contents(
        &mut eval,
        vec![Value::string(path.to_string_lossy()), Value::T],
    )
    .unwrap();
    assert_eq!(
        eval.buffers
            .current_buffer()
            .unwrap()
            .visited_file_modtime(),
        VisitedFileModtime::Known { sec: -1, nsec: 0 }
    );
    assert_eq!(
        builtin_verify_visited_file_modtime(&mut eval, vec![]).unwrap(),
        Value::T
    );
}

#[cfg(unix)]
#[test]
fn directory_timestamp_before_epoch_matches_gnu() {
    let dir = tempfile::tempdir().unwrap();
    let mut eval = Context::new();
    assert_eq!(
        builtin_set_file_times(
            &mut eval,
            vec![
                Value::string(dir.path().to_string_lossy()),
                Value::fixnum(-1)
            ]
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        fs::metadata(dir.path()).unwrap().modified().unwrap(),
        std::time::UNIX_EPOCH - std::time::Duration::from_secs(1)
    );
}
