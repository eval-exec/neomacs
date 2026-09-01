use super::*;
use crate::heap_types::LispString;

fn signal_payload(flow: Flow) -> Vec<Value> {
    match flow {
        Flow::Signal(signal) => signal.data,
        other => panic!("expected signal flow, got {other:?}"),
    }
}

#[test]
fn invalid_read_syntax_in_unibyte_string_counts_raw_bytes_by_line_and_column() {
    crate::test_utils::init_test_tracing();
    let payload = signal_payload(signal_invalid_read_syntax_in_lisp_string(
        &LispString::from_unibyte(b"a\n\xffz".to_vec()),
        3,
        "bad read".to_string(),
    ));

    assert_eq!(payload[0].as_utf8_str(), Some("bad read"));
    assert_eq!(payload[1].as_fixnum(), Some(2));
    assert_eq!(payload[2].as_fixnum(), Some(1));
}

#[test]
fn invalid_read_syntax_in_multibyte_string_counts_chars_not_storage_bytes() {
    crate::test_utils::init_test_tracing();
    let payload = signal_payload(signal_invalid_read_syntax_in_lisp_string(
        &LispString::from_utf8("é\nλx"),
        "é\nλ".len(),
        "bad read".to_string(),
    ));

    assert_eq!(payload[0].as_utf8_str(), Some("bad read"));
    assert_eq!(payload[1].as_fixnum(), Some(2));
    assert_eq!(payload[2].as_fixnum(), Some(1));
}
