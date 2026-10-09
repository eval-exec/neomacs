use super::{
    CharPropertyCarrier, TextPropertyControlVariable, Value, lookup_char_property_from_direct,
    plist_slice_get_value,
};
use crate::emacs_core::eval::Context;
use strum::IntoEnumIterator;

fn context_with_default_property(property: Value, default: Value) -> Context {
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        TextPropertyControlVariable::DefaultTextProperties.name(),
        Value::list(vec![property, default]),
    );
    eval.obarray.set_symbol_value(
        TextPropertyControlVariable::CharPropertyAliasAlist.name(),
        Value::NIL,
    );
    eval
}

#[test]
fn gde_char_property_carrier_defaults_only_apply_to_text() {
    crate::test_utils::init_test_tracing();
    let property = Value::symbol("gde-carrier-probe");
    let default = Value::fixnum(11);
    let eval = context_with_default_property(property, default);

    // GNU intervals.c:1741-1742 gates default-text-properties on textprop.
    for carrier in CharPropertyCarrier::iter() {
        let actual = lookup_char_property_from_direct(
            &eval.obarray,
            &eval.buffers,
            |_| None,
            property,
            carrier,
        );
        let expected = match carrier {
            CharPropertyCarrier::Text => default,
            CharPropertyCarrier::Overlay => Value::NIL,
        };
        assert_eq!(actual, expected, "{carrier:?}");
    }
}

#[test]
fn gde_char_property_carrier_direct_nil_suppresses_fallbacks() {
    crate::test_utils::init_test_tracing();
    let property = Value::symbol("gde-carrier-probe");
    let eval = context_with_default_property(property, Value::fixnum(11));

    // GNU intervals.c:1720-1721 returns a directly present nil before category,
    // aliases, or defaults. Only the canonical property may be probed here.
    for carrier in CharPropertyCarrier::iter() {
        let mut probes = 0;
        let actual = lookup_char_property_from_direct(
            &eval.obarray,
            &eval.buffers,
            |name| {
                probes += 1;
                assert_eq!(name, property);
                Some(Value::NIL)
            },
            property,
            carrier,
        );
        assert_eq!(actual, Value::NIL, "{carrier:?}");
        assert_eq!(probes, 1, "{carrier:?}");
    }
}

#[test]
fn gde_char_property_carrier_category_and_alias_precede_defaults() {
    crate::test_utils::init_test_tracing();
    let property = Value::symbol("gde-carrier-probe");
    let category = Value::symbol("gde-carrier-category");
    let alias = Value::symbol("gde-carrier-alias");
    let category_value = Value::fixnum(22);
    let alias_value = Value::fixnum(33);
    let mut eval = context_with_default_property(property, Value::fixnum(11));
    eval.obarray.set_symbol_value(
        TextPropertyControlVariable::CharPropertyAliasAlist.name(),
        Value::list(vec![Value::cons(property, Value::list(vec![alias]))]),
    );
    eval.obarray
        .put_property("gde-carrier-category", "gde-carrier-probe", category_value)
        .expect("the new category has a valid plist");
    let plist = [(Value::symbol("category"), category), (alias, alias_value)];

    // GNU intervals.c:1729-1742 shares category/alias precedence across carriers.
    for carrier in CharPropertyCarrier::iter() {
        let actual = lookup_char_property_from_direct(
            &eval.obarray,
            &eval.buffers,
            |name| plist_slice_get_value(&plist, name),
            property,
            carrier,
        );
        assert_eq!(actual, category_value, "{carrier:?}");
    }

    eval.obarray
        .put_property("gde-carrier-category", "gde-carrier-probe", Value::NIL)
        .expect("updating the valid category plist preserves its shape");
    for carrier in CharPropertyCarrier::iter() {
        let actual = lookup_char_property_from_direct(
            &eval.obarray,
            &eval.buffers,
            |name| plist_slice_get_value(&plist, name),
            property,
            carrier,
        );
        assert_eq!(actual, alias_value, "{carrier:?}");
    }
}
