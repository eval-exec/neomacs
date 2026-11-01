//! P10-a1: every production plist-head store routes through
//! `store_plist_head` and advances the universal plist clock; construction
//! and pdump reconstruction take the seam too.  The architecture guard
//! pins the one-site rule; these tests pin the observable bump.
//!
//! The clock is process-global, so a test only asserts that it moved
//! forward: other tests in the same binary may bump it concurrently.

use crate::emacs_core::intern::intern;
use crate::emacs_core::symbol::{
    DumpedCell, DumpedSymbolFlags, LispSymbol, Obarray, SymbolInterned, SymbolTrappedWrite,
    plist_epoch, store_plist_head,
};
use crate::emacs_core::value::Value;

fn advanced(last: u64) -> u64 {
    let now = plist_epoch();
    assert!(now > last, "the plist epoch must advance: {last} -> {now}");
    now
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
    // An existing-key put stores the same head again and bumps; harmless.
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
    store_plist_head(&mut symbol, Value::fixnum(9));
    assert_eq!(symbol.plist.get(), Value::fixnum(9));
    let _ = advanced(before);
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
    let after_dump = advanced(before);
    assert_eq!(dumped.plist.get(), Value::fixnum(11));

    let mut slot = LispSymbol::empty();
    slot.publish_fill(dumped);
    let _ = advanced(after_dump);
    assert_eq!(slot.plist.get(), Value::fixnum(11));
}
