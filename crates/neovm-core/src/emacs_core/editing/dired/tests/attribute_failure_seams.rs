use super::*;
#[test]
fn vanished_entries_do_not_consume_count_and_the_scan_stops_after_the_next_accepted_entry() {
    crate::test_utils::init_test_tracing();
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    std::fs::write(dir.path().join("live"), b"").unwrap();
    std::fs::write(dir.path().join("extra"), b"").unwrap();
    let path = LispString::from_unibyte(dir.path().to_str().unwrap().as_bytes().to_vec());
    let mut ctx = Context::new();
    let time = LispTimeOutput::from_context(&ctx).unwrap();
    let syntax =
        crate::emacs_core::builtins::search::FastStringMatchSyntax::for_current_buffer(&ctx);
    let args = vec![
        Value::heap_string(path.clone()),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::fixnum(1),
    ];
    let mut entries = ["gone", "live", "extra", "never-read"].into_iter();
    let mut reads = 0;
    let mut metadata_reads = 0;
    let result = directory_files_and_attributes_from_reader(
        &mut ctx,
        &args,
        &path,
        time,
        syntax,
        |_, bytes| LispString::from_unibyte(bytes.to_vec()),
        |_| {
            reads += 1;
            Ok(entries
                .next()
                .map(|s| LispString::from_unibyte(s.as_bytes().to_vec())))
        },
        |filename| {
            metadata_reads += 1;
            if filename.file_name().unwrap() == "gone" {
                Err(std::io::Error::from_raw_os_error(libc::ENOENT))
            } else {
                std::fs::symlink_metadata(filename)
            }
        },
    )
    .unwrap();
    let rows = list_to_vec(&result).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cons_car().as_str_owned().as_deref(), Some("live"));
    assert!(rows[0].cons_cdr().is_cons());
    assert_eq!(reads, 3); // GNU checks COUNT after inspecting the next eligible entry.
    assert_eq!(metadata_reads, 3);
}
#[test]
fn metadata_errors_preserve_gnu_error_categories_and_entry_name() {
    crate::test_utils::init_test_tracing();
    let ctx = Context::new();
    let time = LispTimeOutput::from_context(&ctx).unwrap();
    let path = LispString::from_unibyte(b"/not-accessed/entry".to_vec());
    let name = LispString::from_unibyte(b"entry".to_vec());
    for errno in [libc::ENOENT, libc::ENOTDIR] {
        assert!(
            build_file_attributes_with_metadata(
                &path,
                FileIdFormat::Integer,
                time,
                &name,
                |_| Err(std::io::Error::from_raw_os_error(errno))
            )
            .unwrap()
            .is_none()
        );
    }
    for (errno, symbol) in [
        (libc::EACCES, "permission-denied"),
        (libc::EIO, "file-error"),
        (libc::ELOOP, "file-error"),
    ] {
        let error =
            build_file_attributes_with_metadata(&path, FileIdFormat::Integer, time, &name, |_| {
                Err(std::io::Error::from_raw_os_error(errno))
            })
            .unwrap_err();
        match error.into_kind() {
            FlowKind::Signal(signal) => {
                assert_eq!(signal.symbol_name(), symbol);
                assert_eq!(
                    signal.data[0].as_str_owned().as_deref(),
                    Some("Getting attributes")
                );
                assert_eq!(signal.data[2].as_str_owned().as_deref(), Some("entry"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
