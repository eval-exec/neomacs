use super::*;

fn request(filters: impl IntoIterator<Item = W32Filter>) -> W32Request {
    W32Request::new(filters.into_iter().collect())
}

#[test]
fn w32_filters_map_to_exact_read_directory_changes_bits() {
    assert_eq!(
        request([W32Filter::FileName, W32Filter::Attributes]).native_filter_bits(),
        0x0000_0001 | 0x0000_0004
    );
    assert_eq!(
        request([W32Filter::LastAccessTime, W32Filter::SecurityDescriptor]).native_filter_bits(),
        0x0000_0020 | 0x0000_0100
    );
    assert_eq!(request([W32Filter::Subtree]).native_filter_bits(), 0);
}

#[test]
fn w32_decoder_preserves_the_ordered_old_and_new_rename_halves() {
    let mut old = encoded_record(4, "old.txt");
    let old_len = old.len().next_multiple_of(4);
    old.resize(old_len, 0);
    old[0..4].copy_from_slice(&(old_len as u32).to_le_bytes());
    old.extend(encoded_record(5, "new.txt"));

    assert_eq!(
        codec::decode(&old).expect("decode rename records"),
        [
            (W32Action::RenamedFrom, PathBuf::from("old.txt")),
            (W32Action::RenamedTo, PathBuf::from("new.txt")),
        ]
    );
}

#[test]
fn w32_decoder_rejects_a_next_record_offset_inside_the_current_name() {
    let mut record = encoded_record(1, "name.txt");
    record[0..4].copy_from_slice(&12_u32.to_le_bytes());

    assert_eq!(
        codec::decode(&record).expect_err("overlapping records must be rejected"),
        "invalid FILE_NOTIFY_INFORMATION next offset"
    );
}

#[test]
fn w32_request_parsing_and_lisp_event_shape_match_gnu() {
    assert_eq!(
        W32Filter::from_lisp_name("security-desc"),
        Some(W32Filter::SecurityDescriptor)
    );
    assert_eq!(W32Filter::from_lisp_name("unknown-filter"), None);
    assert!(request([W32Filter::Subtree]).recursive());

    let event = W32Event {
        watch_id: WatchId::new(42, 0),
        action: W32Action::RenamedTo,
        path: PathBuf::from(r"nested\new.txt"),
    };
    let eval = crate::test_utils::runtime_startup_context();
    let fields = crate::emacs_core::value::list_to_vec(
        &event.into_lisp(&eval, WatchRegistration::new(Value::NIL, Value::NIL)),
    )
    .expect("w32 event is a proper list");
    assert_eq!(
        fields,
        [
            Value::fixnum(42),
            Value::symbol("renamed-to"),
            Value::string(r"nested\new.txt")
        ]
    );
}

fn encoded_record(action: u32, name: &str) -> Vec<u8> {
    let name = name.encode_utf16().collect::<Vec<_>>();
    let mut record = Vec::with_capacity(12 + name.len() * 2);
    record.extend(0_u32.to_le_bytes());
    record.extend(action.to_le_bytes());
    record.extend(((name.len() * 2) as u32).to_le_bytes());
    for code_unit in name {
        record.extend(code_unit.to_le_bytes());
    }
    record
}
