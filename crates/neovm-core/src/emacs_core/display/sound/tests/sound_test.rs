use super::*;

#[test]
fn sound_spec_head_domain_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        SoundSpecHead::from_lisp_value(&Value::symbol("sound")),
        Some(SoundSpecHead::Sound)
    );
    assert_eq!(SoundSpecHead::Sound.name(), "sound");
    assert_eq!(
        SoundSpecHead::from_lisp_value(&Value::symbol("not-sound")),
        None
    );
}

#[test]
fn parse_sound_spec_odd_plist_signals_malformed_keyword_arg_list() {
    crate::test_utils::init_test_tracing();

    let invalid = Value::list(vec![Value::symbol("sound"), Value::symbol(":data")]);
    match parse_sound_spec(invalid) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "malformed-keyword-arg-list");
            assert!(sig.data.is_empty());
        }
        Err(other) => panic!("unexpected flow: {other:?}"),
        Ok(_) => panic!("expected malformed keyword arg list"),
    }
}

#[test]
fn parse_sound_spec_validates_device_like_gnu() {
    crate::test_utils::init_test_tracing();

    let invalid = Value::list(vec![
        Value::symbol("sound"),
        Value::symbol(":data"),
        Value::string(""),
        Value::symbol(":device"),
        Value::fixnum(1),
    ]);
    match parse_sound_spec(invalid) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Invalid sound specification")]);
        }
        Err(other) => panic!("unexpected flow: {other:?}"),
        Ok(_) => panic!("expected invalid sound specification"),
    }

    let valid = Value::list(vec![
        Value::symbol("sound"),
        Value::symbol(":data"),
        Value::string(""),
        Value::symbol(":device"),
        Value::string("default"),
    ]);
    let spec = parse_sound_spec(valid).unwrap();
    assert_eq!(spec.device.as_deref(), Some("default"));
}
