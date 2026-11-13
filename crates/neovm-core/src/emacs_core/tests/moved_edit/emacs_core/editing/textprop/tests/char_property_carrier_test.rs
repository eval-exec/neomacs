use neovm_edit::buffer::{
    Buffer, BufferId, BufferManager, CharLen, CharPos0, CharRange, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
};
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_edit::emacs_core::edit_host::{EditHost, EditVariableRead};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_max_args, expect_min_args};
use neovm_value::intern::SymId;
use neovm_value::intern::WellKnown;
use neovm_value::intern::{NIL_SYM_ID, T_SYM_ID};
use neovm_value::symbol::CanonicalAccess;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::value::SymbolMode;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use strum::IntoStaticStr;

use super::{
    CharPropertyCarrier, TextPropertyControlVariable, Value, lookup_char_property_from_direct,
    plist_slice_get_value,
};

use crate::emacs_core::eval::Context;

use strum::IntoEnumIterator;

#[test]
fn gde_char_property_carrier_defaults_only_apply_to_text() {
    crate::test_utils::init_test_tracing();
    let property = Value::symbol("gde-carrier-probe");
    let default = Value::fixnum(11);
    let eval = context_with_default_property(property, default);

    // GNU intervals.c:1741-1742 gates default-text-properties on textprop.
    for carrier in CharPropertyCarrier::iter() {
        let actual = lookup_char_property_from_direct(
            eval.variables(),
            &eval.edit.buffers,
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
            eval.variables(),
            &eval.edit.buffers,
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
    eval.obarray_mut().set_symbol_value(
        TextPropertyControlVariable::CharPropertyAliasAlist.name(),
        Value::list(vec![Value::cons(property, Value::list(vec![alias]))]),
    );
    eval.obarray_mut()
        .put_property("gde-carrier-category", "gde-carrier-probe", category_value)
        .expect("the new category has a valid plist");
    let plist = [(Value::symbol("category"), category), (alias, alias_value)];

    // GNU intervals.c:1729-1742 shares category/alias precedence across carriers.
    for carrier in CharPropertyCarrier::iter() {
        let actual = lookup_char_property_from_direct(
            eval.variables(),
            &eval.edit.buffers,
            |name| plist_slice_get_value(&plist, name),
            property,
            carrier,
        );
        assert_eq!(actual, category_value, "{carrier:?}");
    }

    eval.obarray_mut()
        .put_property("gde-carrier-category", "gde-carrier-probe", Value::NIL)
        .expect("updating the valid category plist preserves its shape");
    for carrier in CharPropertyCarrier::iter() {
        let actual = lookup_char_property_from_direct(
            eval.variables(),
            &eval.edit.buffers,
            |name| plist_slice_get_value(&plist, name),
            property,
            carrier,
        );
        assert_eq!(actual, alias_value, "{carrier:?}");
    }
}

fn context_with_default_property(property: Value, default: Value) -> Context {
    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        TextPropertyControlVariable::DefaultTextProperties.name(),
        Value::list(vec![property, default]),
    );
    eval.obarray_mut().set_symbol_value(
        TextPropertyControlVariable::CharPropertyAliasAlist.name(),
        Value::NIL,
    );
    eval
}
