//! Live session-bus checks. Skipped when the bus is not available.

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn session_bus_unique_name_has_gnu_shape() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let refs = match super::super::connection::init_bus(
        &mut ctx,
        vec![Value::keyword_by_name(":session")],
    ) {
        Ok(value) => value,
        Err(Flow::Signal(signal)) if signal.symbol_name() == "dbus-error" => return,
        Err(err) => panic!("unexpected init-bus failure: {err:?}"),
    };
    assert_eq!(refs, Value::fixnum(1));
    let name = super::super::connection::get_unique_name(vec![Value::keyword_by_name(":session")])
        .expect("unique name");
    let text = name.as_utf8_str().expect("unique name string");
    assert!(
        text.starts_with(':') && text.contains('.'),
        "GNU unique names look like :1.54, got {text}"
    );
}
