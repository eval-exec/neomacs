use crate::emacs_core::value::Value;

#[test]
fn disabled_build_exposes_only_gnu_capability_probes() {
    let mut eval = crate::emacs_core::Context::new();
    let surface = eval
        .eval_str(
            "(list (sqlite-available-p)
                   (sqlitep nil)
                   (fboundp 'sqlite-open)
                   (fboundp 'sqlite-version))",
        )
        .unwrap();

    assert_eq!(
        surface,
        Value::list(vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL])
    );
}
