//! Small actual-backing provenance fixtures, usable without a full Context.
use super::*;
use crate::emacs_core::intern::intern;
use crate::emacs_core::symbol::ArmMut;
use crate::emacs_core::value::Value;

fn project(
    ob: &mut Obarray,
    id: SymId,
    projection: &'static crate::emacs_core::forward::ThreadProjectionDescriptor,
) {
    let mut write = ob.cell_write_ensure(id);
    let ArmMut::Plain(plain) = write.arm() else {
        panic!("fresh plain cell")
    };
    plain.conceal_projection(projection);
}

#[test]
fn p3sym_projection_admission_uses_own_world_table_and_plain_arm() {
    let mut ob = Obarray::new();
    let mut table = ThreadProjectionTable::default();
    let mut foreign = ThreadProjectionTable::default();
    let owned = intern("p3sym-c2-owned-projection");
    let void = intern("p3sym-c2-void-projection");
    let wrong = intern("p3sym-c2-foreign-projection");
    let host = intern("p3sym-c2-host-projection");
    project(
        &mut ob,
        owned,
        table.issue(CanonicalArm::Plain, Value::fixnum(17), false),
    );
    project(&mut ob, void, table.void_descriptor());
    project(
        &mut ob,
        wrong,
        foreign.issue(CanonicalArm::Plain, Value::NIL, false),
    );
    project(
        &mut ob,
        host,
        table.issue(CanonicalArm::Plain, Value::NIL, true),
    );
    // Root derivation happens after setup. All references are short-lived and
    // no mutable Obarray aggregate is reborrowed after these roots are issued.
    let ob = Box::new(std::cell::UnsafeCell::new(ob));
    let table = Box::new(RefCell::new(table));
    let backing =
        unsafe { SymbolBacking::new(NonNull::new(ob.get()).unwrap(), NonNull::from(&*table), 7) };
    assert_eq!(
        unsafe { backing.classify(owned) },
        Some(PlainSymbolBacking::Projected)
    );
    assert_eq!(
        unsafe { backing.classify(void) },
        Some(PlainSymbolBacking::VoidProjection)
    );
    assert_eq!(unsafe { backing.classify(wrong) }, None);
    assert_eq!(unsafe { backing.classify(host) }, None);
    assert_eq!(unsafe { backing.probe(owned) }, Value::fixnum(17).bits());
    assert_eq!(unsafe { backing.probe(void) }, Value::UNBOUND.bits());
}

#[test]
fn p3sym_original_allocation_root_survives_thread_handoff() {
    let mut ob = Obarray::new();
    let id = intern("p3sym-c2-provenance-handoff");
    ob.seed_value_id(id, Value::fixnum(29));
    let ob = Box::new(std::cell::UnsafeCell::new(ob));
    let table = Box::new(RefCell::new(ThreadProjectionTable::default()));
    let backing =
        unsafe { SymbolBacking::new(NonNull::new(ob.get()).unwrap(), NonNull::from(&*table), 9) };
    // A mutex models EXCLUSIVE backing admission, not the counted-reader
    // algorithm (the real-RunAccess test exercises that word separately).
    let backing = std::sync::Mutex::new(backing);
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let backing = backing.lock().unwrap();
                assert_eq!(
                    unsafe { backing.classify(id) },
                    Some(PlainSymbolBacking::Unprojected)
                );
                assert_eq!(unsafe { backing.probe(id) }, Value::fixnum(29).bits());
            })
            .join()
            .unwrap();
    });
    let backing = backing.lock().unwrap();
    assert_eq!(
        unsafe { backing.classify(id) },
        Some(PlainSymbolBacking::Unprojected)
    );
    assert_eq!(unsafe { backing.probe(id) }, Value::fixnum(29).bits());
}

#[test]
fn p3sym_symbol_layout_stays_at_b135_offsets() {
    use super::super::{
        LISP_SYMBOL_FLAGS_OFFSET, LISP_SYMBOL_INTERNED_GLOBAL_OFFSET, LISP_SYMBOL_SIZE,
        LISP_SYMBOL_VAL_OFFSET, LispSymbol,
    };
    assert_eq!(LISP_SYMBOL_SIZE, 32);
    assert_eq!(LISP_SYMBOL_VAL_OFFSET, 0);
    assert_eq!(std::mem::offset_of!(LispSymbol, function), 8);
    assert_eq!(std::mem::offset_of!(LispSymbol, plist), 16);
    assert_eq!(LISP_SYMBOL_FLAGS_OFFSET, 28);
    assert_eq!(LISP_SYMBOL_INTERNED_GLOBAL_OFFSET, 29);
}

/// Deliberate provenance violations run only as isolated expected-failure
/// Miri jobs. Never execute these under an ordinary test runner.
#[cfg(miri)]
#[test]
fn p3sym_miri_negative_exclusive_aggregate_reborrow() {
    fn violate(ob: &mut Obarray, backing: &SymbolBacking, id: SymId) {
        let _ = unsafe { backing.classify(id) };
        std::hint::black_box(ob.generation());
    }
    let mut ob = Obarray::new();
    let id = intern("p3sym-miri-negative-alias");
    ob.seed_value_id(id, Value::NIL);
    let ob = Box::new(std::cell::UnsafeCell::new(ob));
    let table = RefCell::new(ThreadProjectionTable::default());
    let root = NonNull::new(ob.get()).unwrap();
    let backing = unsafe { SymbolBacking::new(root, NonNull::from(&table), 19) };
    // Forbidden: a live exclusive aggregate loan overlaps an independent
    // root-derived access. Correct C2 never issues this aggregate reference.
    unsafe { violate(&mut *root.as_ptr(), &backing, id) };
}

#[cfg(miri)]
#[test]
fn p3sym_miri_negative_backing_outlives_owner() {
    let mut ob = Obarray::new();
    let id = intern("p3sym-miri-negative-owner");
    ob.seed_value_id(id, Value::NIL);
    let ob = Box::new(std::cell::UnsafeCell::new(ob));
    let table = RefCell::new(ThreadProjectionTable::default());
    let backing =
        unsafe { SymbolBacking::new(NonNull::new(ob.get()).unwrap(), NonNull::from(&table), 23) };
    drop(ob); // Forbidden: era teardown must retain this allocation.
    let _ = unsafe { backing.classify(id) };
}
