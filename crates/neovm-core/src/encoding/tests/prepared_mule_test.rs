use super::*;
use crate::emacs_core::charset::{
    CharsetMethodSnapshot, restore_charset_registry, snapshot_charset_registry,
};
use crate::emacs_core::intern::intern;
use crate::heap_types::LispString;

// Exercise the enabled body directly, without changing process environment or
// racing the once-only runtime knob.
fn encode_prepared(source: &LispString) -> Vec<u8> {
    encode_via_emacs_mule_prepared(source)
}

fn install_overlapping_charsets() -> (
    crate::emacs_core::intern::SymId,
    crate::emacs_core::intern::SymId,
) {
    let mut snapshot = snapshot_charset_registry();
    let template = snapshot
        .charsets
        .iter()
        .find(|info| info.name == intern("latin-iso8859-1"))
        .expect("preseeded Latin charset")
        .clone();
    let mut names = Vec::new();
    for (index, first_code) in [33, 34].into_iter().enumerate() {
        let mut info = template.clone();
        info.name = intern(if index == 0 {
            "neovm-mule-first"
        } else {
            "neovm-mule-second"
        });
        info.id = 900 + index as i64;
        info.emacs_mule_id = Some(150 + index as i64);
        info.code_space = [first_code, 126, 0, 0, 0, 0, 0, 0];
        info.min_code = first_code;
        info.max_code = 126;
        info.iso_final_char = None;
        info.unified_p = false;
        info.unify_map = Value::NIL;
        info.method = CharsetMethodSnapshot::Offset(0x1f300);
        info.plist = vec![(intern(":name"), Value::from_sym_id(info.name))];
        names.push(info.name);
        snapshot.priority.push(info.name);
        snapshot
            .priority_identities
            .as_mut()
            .expect("modern snapshot")
            .push(info.name);
        snapshot.emacs_mule_order.push(info.name);
        snapshot.charsets.push(info);
    }
    restore_charset_registry(snapshot);
    (names[0], names[1])
}

fn bytes_for_registered_char(name: crate::emacs_core::intern::SymId, ch: i64) -> Vec<u8> {
    let snapshot = snapshot_charset_registry();
    let info = snapshot
        .charsets
        .iter()
        .find(|info| info.name == name)
        .expect("registered fixture");
    let code =
        crate::emacs_core::charset::charset_encode_char(name, ch).expect("encodable fixture");
    // Fixtures use direct one-dimensional Mule ids. The assertion is derived
    // from their registry metadata, rather than a hand-written GNU expectation.
    vec![
        info.emacs_mule_id.expect("Mule fixture id") as u8,
        (code | 0x80) as u8,
    ]
}

#[test]
fn prepared_mule_encode_observes_priority_reordering() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::Context::new();
    let (first, second) = install_overlapping_charsets();
    let source = LispString::from_utf8("\u{1f300}\u{1f300}");
    let first_bytes = bytes_for_registered_char(first, 0x1f300).repeat(2);
    let second_bytes = bytes_for_registered_char(second, 0x1f300).repeat(2);
    assert_eq!(encode_prepared(&source), first_bytes);
    crate::emacs_core::charset::builtin_set_charset_priority(vec![Value::from_sym_id(second)])
        .expect("preferred fixture charset");
    assert_eq!(encode_prepared(&source), second_bytes);
    crate::emacs_core::charset::builtin_set_charset_priority(vec![Value::from_sym_id(first)])
        .expect("restore fixture priority");
    assert_eq!(encode_prepared(&source), first_bytes);
}

#[test]
fn prepared_mule_encode_observes_redefinition_between_calls() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::Context::new();
    let (first, _) = install_overlapping_charsets();
    let source = LispString::from_utf8("\u{1f300}");
    let before = bytes_for_registered_char(first, 0x1f300);
    assert_eq!(encode_prepared(&source), before);
    let mut snapshot = snapshot_charset_registry();
    snapshot
        .charsets
        .retain(|info| info.name != intern("neovm-mule-second"));
    snapshot
        .priority
        .retain(|&name| name != intern("neovm-mule-second"));
    snapshot
        .priority_identities
        .as_mut()
        .expect("modern snapshot")
        .retain(|&name| name != intern("neovm-mule-second"));
    snapshot
        .emacs_mule_order
        .retain(|&name| name != intern("neovm-mule-second"));
    let info = snapshot
        .charsets
        .iter_mut()
        .find(|info| info.name == first)
        .expect("fixture");
    info.method = CharsetMethodSnapshot::Offset(0x1f400);
    restore_charset_registry(snapshot);
    assert_eq!(encode_prepared(&LispString::from_utf8("\u{1f400}")), before);
    assert_eq!(encode_prepared(&source), b" ");
}

#[test]
fn prepared_mule_encode_preserves_ascii_unibyte_and_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::Context::new();
    assert_eq!(
        encode_prepared(&LispString::from_utf8("ASCII\n")),
        b"ASCII\n"
    );
    let wire = vec![b'A', 0x80, 0xe9, 0xff];
    assert_eq!(
        encode_prepared(&LispString::from_unibyte(wire.clone())),
        wire
    );
    let mut internal = vec![b'A'];
    for byte in [0x80, 0xe9, 0xff] {
        let mut scratch = [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = crate::emacs_core::emacs_char::char_string(
            crate::emacs_core::emacs_char::byte8_to_char(byte),
            &mut scratch,
        );
        internal.extend_from_slice(&scratch[..len]);
    }
    assert_eq!(
        encode_prepared(&LispString::from_emacs_bytes(internal)),
        wire
    );
}
