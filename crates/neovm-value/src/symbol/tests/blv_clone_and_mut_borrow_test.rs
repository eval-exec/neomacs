//! Regression tests for soundness findings vc A9 and vc A10:
//! - A9: `LispSymbol` must not implement `Clone`, because cloning a
//!   `Localized` symbol copies its raw `BlvPtr` outside the owning `Obarray`,
//!   turning BLV reads after `Obarray::drop` into a use-after-free.
//! - A10: public `Obarray` constructors (`ensure_symbol_id`, `get_or_intern`)
//!   must not return `&mut LispSymbol`, which lets cross-crate code overwrite
//!   a slot non-atomically during a concurrent GC mark or move a `Localized`
//!   symbol out of its owning `Obarray`.

use std::marker::PhantomData;

use super::{LispSymbol, Obarray, Value, intern};
use crate::value::buffer_vocab::BufferId;

struct CloneProbe<T>(PhantomData<T>);

trait IsClone {
    fn is_clone(&self) -> bool {
        true
    }
}

impl<T: Clone> IsClone for CloneProbe<T> {}

trait NotClone {
    fn is_clone(&self) -> bool {
        false
    }
}

impl<T> NotClone for &CloneProbe<T> {}

struct ReturnProbe<T>(PhantomData<T>);

impl<T> ReturnProbe<T> {
    fn of(_value: T) -> Self {
        Self(PhantomData)
    }
}

trait ReturnsMutLispSymbol {
    fn is_mut_lisp_symbol(&self) -> bool {
        true
    }
}

impl ReturnsMutLispSymbol for ReturnProbe<&mut LispSymbol> {}

trait DoesNotReturnMutLispSymbol {
    fn is_mut_lisp_symbol(&self) -> bool {
        false
    }
}

impl<T> DoesNotReturnMutLispSymbol for &ReturnProbe<T> {}

#[test]
fn lisp_symbol_is_not_clone() {
    // Control: a Clone type must be reported as Clone by the probe.
    let control = CloneProbe::<u32>(PhantomData);
    assert!(
        (&control).is_clone(),
        "CloneProbe control must detect Clone"
    );

    let probe = CloneProbe::<LispSymbol>(PhantomData);
    assert!(
        !(&probe).is_clone(),
        "LispSymbol must not implement Clone: cloning a Localized symbol copies its raw BlvPtr"
    );
}

#[test]
fn obarray_public_constructors_do_not_return_mut_lisp_symbol() {
    // Control: `&mut LispSymbol` must be reported as true by the probe.
    let control = ReturnProbe::<&mut LispSymbol>(PhantomData);
    assert!(
        (&control).is_mut_lisp_symbol(),
        "ReturnProbe control must detect &mut LispSymbol"
    );

    let mut ob = Obarray::new();
    let id = intern("vc-a10-ensure-symbol-id");
    let ensure_probe = ReturnProbe::of(ob.ensure_symbol_id(id));
    assert!(
        !(&ensure_probe).is_mut_lisp_symbol(),
        "Obarray::ensure_symbol_id must not hand out &mut LispSymbol"
    );

    let intern_probe = ReturnProbe::of(ob.get_or_intern("vc-a10-get-or-intern"));
    assert!(
        !(&intern_probe).is_mut_lisp_symbol(),
        "Obarray::get_or_intern must not hand out &mut LispSymbol"
    );
}

#[test]
fn obarray_clone_deep_copies_localized_blv_after_source_drop() {
    crate::test_utils::init_test_tracing();
    let id = intern("vc-a9-clone-blv-deep-copy");
    let mut source = Obarray::new();
    let orig_blv = source
        .make_symbol_localized(id, Value::fixnum(42))
        .expect("localized symbol gets a BLV");

    let cloned = source.clone();
    drop(source);

    let cloned_sym = cloned
        .get_by_id(id)
        .expect("cloned obarray keeps localized symbol");
    let cloned_blv = cloned_sym
        .localized_blv()
        .expect("cloned symbol stays localized");
    assert_ne!(
        cloned_blv, orig_blv,
        "Obarray::clone must allocate a fresh BLV rather than aliasing the dropped source's BLV"
    );
    let defaults = cloned_sym
        .blv_default_cells()
        .expect("cloned localized symbol exposes its BLV default cell");
    assert_eq!(defaults.defcell().cons_cdr(), Value::fixnum(42));
    assert_eq!(
        cloned.read_localized(id, Value::NIL, Value::NIL),
        Some(Value::fixnum(42))
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "does not belong to this Obarray")]
fn read_localized_symbol_for_buffer_rejects_foreign_symbol_in_debug() {
    crate::test_utils::init_test_tracing();
    let id = intern("vc-a9-foreign-symbol-blv");
    let mut ob_a = Obarray::new();
    let mut ob_b = Obarray::new();
    ob_a.make_symbol_localized(id, Value::fixnum(1));
    ob_b.make_symbol_localized(id, Value::fixnum(2));
    let foreign_sym = ob_a.get_by_id(id).expect("localized in ob_a");
    let _ = ob_b.read_localized_symbol_for_buffer(id, foreign_sym, BufferId(1), Value::NIL);
}
