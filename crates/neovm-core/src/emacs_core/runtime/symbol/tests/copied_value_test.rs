//! Copied reads retain their value while the owning cell changes.

use super::{Obarray, intern};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;

// These signatures reject a borrowed-value return at the copying boundary.
const _: fn(&Obarray, &str) -> Option<Value> = Obarray::symbol_value_copied;
const _: fn(&Obarray, SymId) -> Option<Value> = Obarray::symbol_value_id_copied;
const _: fn(&Obarray, SymId) -> Option<Value> = Obarray::default_value_id_copied;

#[test]
fn copied_plain_read_retains_its_word_across_overwrite() {
    let mut obarray = Obarray::new();
    obarray.set_symbol_value("p74-copy-plain", Value::fixnum(41));
    let old = obarray.symbol_value_copied("p74-copy-plain");

    obarray.set_symbol_value("p74-copy-plain", Value::fixnum(42));

    assert_eq!(old, Some(Value::fixnum(41)));
    assert_eq!(
        obarray.symbol_value_copied("p74-copy-plain"),
        Some(Value::fixnum(42))
    );
}

#[test]
fn copied_default_follows_alias_and_filters_unbound() {
    let mut obarray = Obarray::new();
    let target = intern("p74-copy-target");
    let alias = intern("p74-copy-alias");
    obarray.set_symbol_value_id(target, Value::fixnum(51));
    obarray.make_alias(alias, target).unwrap();
    let old = obarray.default_value_id_copied(alias);

    obarray.set_symbol_value_id(target, Value::UNBOUND);

    assert_eq!(old, Some(Value::fixnum(51)));
    assert_eq!(obarray.default_value_id_copied(alias), None);
    assert_eq!(obarray.symbol_value_id_copied(alias), None);
}

#[test]
fn copied_forwarded_read_retains_its_word_across_overwrite() {
    let mut obarray = Obarray::new();
    obarray.define_int_variable("p74-copy-forwarded", 61);
    let id = intern("p74-copy-forwarded");
    let old = obarray.symbol_value_copied("p74-copy-forwarded");
    let old_default = obarray.default_value_id_copied(id);

    obarray.set_symbol_value_id(id, Value::fixnum(62));

    assert_eq!(old, Some(Value::fixnum(61)));
    assert_eq!(old_default, Some(Value::fixnum(61)));
    assert_eq!(obarray.default_value_id_copied(id), Some(Value::fixnum(62)));
}
