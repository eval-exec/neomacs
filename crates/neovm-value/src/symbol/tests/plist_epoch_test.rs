//! P10-a1 / A2-S1: every production plist-head store routes through
//! `store_plist_head`.  A runtime store bumps the universal plist clock.
//! Construction and pdump reconstruction take the seam and do not bump.
//! The architecture guard pins the one-site rule; these tests pin the clock.
//!
//! The construction test asserts equality.  That assertion is sound because
//! nextest gives each test its own process, and nothing in that test does a
//! runtime store after `before` is read.

use crate::intern::intern;
use crate::symbol::{
    DumpedCell, DumpedSymbolFlags, LispSymbol, Obarray, PlistStore, SymbolInterned,
    SymbolTrappedWrite, plist_epoch, store_construction_plist_head, store_plist_head,
    store_runtime_plist_head,
};
use crate::value::Value;

fn advanced(last: u64) -> u64 {
    let now = plist_epoch();
    assert!(now > last, "the plist epoch must advance: {last} -> {now}");
    now
}

fn unchanged(last: u64) {
    assert_eq!(
        plist_epoch(),
        last,
        "a construction head store must not bump the plist epoch"
    );
}

#[test]
fn obarray_plist_head_stores_advance_the_epoch() {
    let mut ob = Obarray::new();
    let symbol = intern("p10-a1-epoch-symbol");
    let prop = intern("p10-a1-epoch-prop");
    let mut last = plist_epoch();

    ob.put_property("p10-a1-epoch-symbol", "p10-a1-epoch-prop", Value::fixnum(1))
        .expect("new property");
    last = advanced(last);
    // An existing-key put stores the same head again and still bumps.
    // Same-head suppression is not this rule.
    ob.put_property("p10-a1-epoch-symbol", "p10-a1-epoch-prop", Value::fixnum(2))
        .expect("existing property");
    last = advanced(last);
    ob.put_property_id(symbol, prop, Value::fixnum(3))
        .expect("property by identity");
    last = advanced(last);
    ob.set_symbol_plist_id(symbol, Value::NIL);
    last = advanced(last);
    ob.replace_symbol_plist_id(symbol, [(prop, Value::fixnum(4))]);
    last = advanced(last);

    assert_eq!(ob.get_property_id(symbol, prop), Some(Value::fixnum(4)));
}

#[test]
fn store_plist_head_publishes_the_head_and_advances_the_epoch() {
    let id = intern("p10-a1-seam-symbol");
    let mut symbol = LispSymbol::new(id);
    let before = plist_epoch();
    store_plist_head(&mut symbol, Value::fixnum(9), PlistStore::Runtime);
    assert_eq!(symbol.plist.get(), Value::fixnum(9));
    let _ = advanced(before);
    store_runtime_plist_head(&mut symbol, Value::fixnum(10));
    assert_eq!(symbol.plist.get(), Value::fixnum(10));
}

#[test]
fn construction_and_pdump_head_stores_take_the_seam() {
    let id = intern("p10-a1-dump-symbol");
    let before = plist_epoch();
    let dumped = LispSymbol::from_dump(
        id,
        DumpedSymbolFlags {
            trapped_write: SymbolTrappedWrite::Untrapped,
            interned: SymbolInterned::Interned,
            declared_special: false,
        },
        DumpedCell::Plain(Value::UNBOUND),
        Value::NIL,
        Value::fixnum(11),
    );
    unchanged(before);
    assert_eq!(dumped.plist.get(), Value::fixnum(11));

    let mut slot = LispSymbol::empty();
    slot.publish_fill(dumped);
    unchanged(before);
    assert_eq!(slot.plist.get(), Value::fixnum(11));

    let mut ob = Obarray::new();
    let fresh = ob.get_or_intern("p10-a2-fresh-intern-symbol");
    assert!(fresh.plist.get().is_nil());
    unchanged(before);
}

/// `Construction` over a non-nil pre-image traps in debug.  The runtime
/// store installs that pre-image; the construction store is the trap.
#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn construction_plist_head_store_traps_on_a_non_nil_preimage() {
    let id = intern("p10-a2-construction-trap");
    let mut symbol = LispSymbol::new(id);
    store_runtime_plist_head(&mut symbol, Value::fixnum(3));
    store_construction_plist_head(&mut symbol, Value::NIL);
}
