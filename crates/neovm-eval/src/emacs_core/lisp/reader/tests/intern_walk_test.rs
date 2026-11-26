//! The reader's obarray-intern pass walks a value whose size is the
//! caller's data: a loaded file, a library cache, a keymap. It must not
//! consume Rust stack in proportion to that size (issue #590: an ETAF
//! library cache drove the mount walk thousands of frames deep before any
//! printer ran).

use crate::emacs_core::eval::Context;
use crate::emacs_core::reader::ObarrayReadSymbolsExt;
use neovm_value::value::Value;

#[test]
fn interning_a_million_element_read_value_uses_a_worklist() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    // The name is canonical but not a member until a publication funnel
    // puts it in the global obarray; build the value directly so the test
    // observes only what the walk does.
    let id = neovm_value::intern::intern("neovm--deep-intern");
    assert!(
        !eval.obarray_view().is_global_member(id),
        "the raw canonical id starts as a non-member"
    );
    let symbol = Value::from_sym_id(id);
    let value = Value::list(vec![symbol; 1_000_000]);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(value);
    eval.obarray_mut().intern_member_symbols_in(value);
    eval.restore_specpdl_roots(roots);
    assert!(
        eval.obarray_view().is_global_member(id),
        "the walk must reach and re-intern the leaf symbol"
    );
}

#[test]
fn interning_a_self_linked_symbol_with_pos_terminates() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let symbol = Value::symbol("neovm-swp-cycle");
    // The reader cannot build this from Lisp (position-symbol always carries
    // a bare symbol), but pdump and fixtures can: an SWP whose symbol slot is
    // the SWP itself. The walk must memoise the SWP by identity.
    let positioned =
        unsafe { eval.tagged_heap_mut() }.alloc_symbol_with_pos(symbol, Value::fixnum(0));
    unsafe {
        let object = positioned.as_symbol_with_pos().expect("fresh SWP")
            as *const neovm_value::tagged::header::SymbolWithPosObj
            as *mut neovm_value::tagged::header::SymbolWithPosObj;
        (*object).sym = positioned;
    }
    eval.push_specpdl_root(positioned);
    let roots = eval.save_specpdl_roots();
    eval.obarray_mut().intern_member_symbols_in(positioned);
    eval.restore_specpdl_roots(roots);
}
