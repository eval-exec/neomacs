//! A shared BLV borrow must observe a cache refresh. `read_localized`
//! writes `found`, `where_buf`, and `alist_epoch` while `blv()` can still
//! hold the record.

use super::{NO_WHERE_BUF, Obarray, blv_alist_epoch};
use crate::intern::intern;
use crate::value::Value;

#[test]
fn shared_blv_observes_a_cache_refresh() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("blv-shared-refresh");
    ob.make_symbol_localized(id, Value::fixnum(0));
    let cell = Value::cons(Value::from_sym_id(id), Value::fixnum(99));
    let alist = Value::cons(cell, Value::NIL);
    let buf = Value::fixnum(1);
    let live = ob.blv(id).expect("localized record");
    assert!(!live.found.get());
    let loaded = ob.read_localized(id, buf, alist).expect("localized read");
    assert_eq!(loaded, Value::fixnum(99));
    assert!(live.found.get(), "shared BLV missed found");
    assert_eq!(live.where_buf.get(), buf);
    assert_eq!(
        live.where_buf_id.get(),
        NO_WHERE_BUF,
        "a fixnum is not a buffer id"
    );
    assert_eq!(live.alist_epoch.get(), blv_alist_epoch());
}
