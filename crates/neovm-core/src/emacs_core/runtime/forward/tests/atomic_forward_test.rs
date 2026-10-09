//! Native slot publication tests. Only immediate Values are constructed on
//! worker threads; no unrooted heap object crosses into a non-mutator.

use super::*;
use std::sync::Barrier;

#[test]
fn atomic_forward_slot_publishes_prior_payload_writes() {
    let slot = AtomicValue::new(Value::fixnum(0));
    let payload = AtomicUsize::new(0);
    let start = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            start.wait();
            payload.store(0x5a17, Ordering::Relaxed);
            slot.store(Value::fixnum(1));
        });
        start.wait();
        // The writer owns no heap values or TLS heap. The acquire load of
        // its release publication must observe the preceding payload store.
        while slot.load().as_fixnum() == Some(0) {
            std::thread::yield_now();
        }
        assert_eq!(payload.load(Ordering::Relaxed), 0x5a17);
    });
}

#[test]
fn atomic_forward_slot_never_tears_shared_immediate_words() {
    let first = 0x1234_5678_i64;
    let second = -0x2345_6789_i64;
    let slot = AtomicValue::new(Value::fixnum(first));
    let start = Barrier::new(3);
    std::thread::scope(|scope| {
        for number in [first, second] {
            let slot = &slot;
            let start = &start;
            scope.spawn(move || {
                start.wait();
                for _ in 0..20_000 {
                    slot.store(Value::fixnum(number));
                }
            });
        }
        start.wait();
        for _ in 0..40_000 {
            let number = slot.load().as_fixnum();
            assert!(number == Some(first) || number == Some(second));
        }
    });
}

#[test]
fn forward_load_copy_survives_replacement_and_clone_has_independent_slot() {
    let descriptor = alloc_objfwd(Value::fixnum(3));
    let header = descriptor.header();
    let old = header.load().expect("object slot");
    let cloned = header.clone_stateful().expect("object state is copied");
    descriptor.set(Value::fixnum(9));
    assert_eq!(old.as_fixnum(), Some(3));
    assert_eq!(cloned.load().and_then(Value::as_fixnum), Some(3));
    assert_eq!(header.load().and_then(Value::as_fixnum), Some(9));
    assert!(!std::ptr::eq(header, cloned));
}

#[test]
fn atomic_forward_descriptors_keep_gnu_store_rules() {
    let integer = alloc_intfwd(LispInteger::from_i64(7));
    assert_eq!(
        integer.header().store(Value::T).unwrap_err(),
        ForwardStoreError::WrongType("integerp")
    );
    assert_eq!(integer.get_i64(), 7);
    let accepted = integer
        .header()
        .store(Value::fixnum(-12))
        .expect("fixnum passes integer rule");
    integer.header().commit(accepted);
    assert_eq!(integer.get_i64(), -12);

    let boolean = alloc_boolfwd(false);
    // GNU Boolean forwarding uses NILP; UNBOUND is truthy. Symbol APIs may
    // reject unbinding before reaching this rule, but must not alter it.
    let truthy = boolean
        .header()
        .store(Value::UNBOUND)
        .expect("bool coerces");
    assert!(boolean.header().commit(truthy).is_truthy());
    assert!(boolean.get());
    let nil = boolean.header().store(Value::NIL).expect("bool coerces");
    assert!(boolean.header().commit(nil).is_nil());
    assert!(!boolean.get());

    let keyboard = alloc_kboard_objfwd(Value::fixnum(23));
    let value = keyboard
        .header()
        .store(Value::fixnum(-24))
        .expect("keyboard slots accept objects");
    assert_eq!(keyboard.header().commit(value).as_fixnum(), Some(-24));
    assert_eq!(keyboard.get().as_fixnum(), Some(-24));
}

#[test]
fn atomic_forward_layout_offsets_address_atomic_storage() {
    // Use the same offsets as generated code and concurrent scan code,
    // without introducing a plain read of an atomic slot in a layout probe.
    let object = alloc_objfwd(Value::fixnum(31));
    let object_base = std::ptr::from_ref(object).cast::<u8>();
    // SAFETY: the descriptor is live, the offset is pinned to its aligned
    // AtomicValue word and repr(transparent) puts AtomicUsize at offset zero.
    let object_word = unsafe {
        &*object_base
            .add(LISP_OBJ_FWD_VALUE_OFFSET)
            .cast::<AtomicUsize>()
    };
    assert_eq!(
        object_word.load(Ordering::Acquire),
        Value::fixnum(31).bits()
    );
    let boolean = alloc_boolfwd(true);
    let boolean_base = std::ptr::from_ref(boolean).cast::<u8>();
    // SAFETY: the pinned byte offset addresses this live AtomicBool field.
    let boolean_word = unsafe {
        &*boolean_base
            .add(LISP_BOOL_FWD_VALUE_OFFSET)
            .cast::<AtomicBool>()
    };
    assert!(boolean_word.load(Ordering::Acquire));
}
