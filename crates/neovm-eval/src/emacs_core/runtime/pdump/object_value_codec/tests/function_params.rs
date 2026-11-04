use super::*;

#[test]
fn p6_pdump_parameter_codec_rejects_unknown_kind() {
    assert!(matches!(
        Cursor::new(&[u8::MAX]).read_function_params(),
        Err(DumpError::BytecodeParameterKind(_))
    ));
}

#[test]
fn p6_pdump_parameter_codec_preserves_signed_template() {
    for raw in [-1, 383, 1 << 40, i64::MAX] {
        let mut bytes = Vec::new();
        write_function_params(&mut bytes, &DumpFunctionParams::Stack(raw)).unwrap();
        let decoded = Cursor::new(&bytes).read_function_params().unwrap();
        assert!(matches!(decoded, DumpFunctionParams::Stack(value) if value == raw));
    }
}

#[test]
fn p6_pdump_parameter_codec_round_trips_all_modes() {
    let mut bytes = Vec::new();
    write_function_params(&mut bytes, &DumpFunctionParams::Dynamic).unwrap();
    assert!(matches!(
        Cursor::new(&bytes).read_function_params().unwrap(),
        DumpFunctionParams::Dynamic
    ));
    bytes.clear();
    write_function_params(
        &mut bytes,
        &DumpFunctionParams::Named(DumpLambdaParams {
            required: vec![DumpSymId(7)],
            optional: vec![DumpSymId(9)],
            rest: Some(DumpSymId(11)),
        }),
    )
    .unwrap();
    let DumpFunctionParams::Named(named) = Cursor::new(&bytes).read_function_params().unwrap()
    else {
        panic!("named shape round-trips");
    };
    assert_eq!(named.required, vec![DumpSymId(7)]);
    assert_eq!(named.optional, vec![DumpSymId(9)]);
    assert_eq!(named.rest, Some(DumpSymId(11)));
}
