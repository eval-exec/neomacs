use super::*;
use crate::emacs_core::eval::{Context, ThreadingMode};

#[test]
fn mixed_positions_preserve_immediates_aliases_and_symbols() {
    let _owner = Context::new_with_threading(ThreadingMode::Green);
    let object = TaggedValue::string("registered payload alias");
    let symbol = TaggedValue::symbol("registered-payload-symbol");
    let values = [
        TaggedValue::NIL,
        object,
        TaggedValue::fixnum(17),
        symbol,
        TaggedValue::T,
        object,
        TaggedValue::fixnum(-29),
        symbol,
        TaggedValue::NIL,
    ];
    let payload = RegisteredPayload::new(&values);
    let traced = [object, symbol, object, symbol];
    let mut destination = [TaggedValue::T; 9];
    assert_eq!(payload.len(), values.len());
    payload.copy_into(&traced, &mut destination).unwrap();
    assert_eq!(destination, values);
}

#[test]
fn immediate_only_and_empty_payloads_need_no_traced_words() {
    let values = [
        TaggedValue::NIL,
        TaggedValue::T,
        TaggedValue::fixnum(TaggedValue::MOST_NEGATIVE_FIXNUM),
        TaggedValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM),
    ];
    let payload = RegisteredPayload::new(&values);
    let mut destination = [TaggedValue::NIL; 4];
    payload.copy_into(&[], &mut destination).unwrap();
    assert_eq!(destination, values);

    let empty = RegisteredPayload::new(&[]);
    assert_eq!(empty.len(), 0);
    empty.copy_into(&[], &mut []).unwrap();
}

#[test]
fn mismatched_sizes_are_rejected_before_any_destination_write() {
    let _owner = Context::new_with_threading(ThreadingMode::Green);
    let object = TaggedValue::string("size rejection root");
    let payload = RegisteredPayload::new(&[TaggedValue::fixnum(17), object]);
    let unchanged = [TaggedValue::T; 2];

    for traced in [&[][..], &[object, object][..]] {
        let mut destination = unchanged;
        assert!(matches!(
            payload.copy_into(traced, &mut destination),
            Err(HostRootError::ForeignRecord)
        ));
        assert_eq!(destination, unchanged);
    }

    let mut short = [TaggedValue::T];
    assert!(matches!(
        payload.copy_into(&[object], &mut short),
        Err(HostRootError::ForeignRecord)
    ));
    assert_eq!(short, [TaggedValue::T]);

    let mut long = [TaggedValue::T; 3];
    assert!(matches!(
        payload.copy_into(&[object], &mut long),
        Err(HostRootError::ForeignRecord)
    ));
    assert_eq!(long, [TaggedValue::T; 3]);
}
