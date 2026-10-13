//! The thread-projection descriptor (`forward::thread_projection`): a
//! per-buffer header no buffer has, refused by every context-free accessor
//! through checks per-buffer slots already make.

use super::*;
use crate::buffer::buffer::{BUFFER_SLOT_COUNT, BufferSlot};

static_assertions::assert_not_impl_any!(ThreadProjectionDescriptor: Copy, Clone, Send, Sync);
static_assertions::assert_not_impl_any!(ThreadProjectionTable: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(CanonicalStorage: Copy, Clone, Send, Sync);
static_assertions::assert_not_impl_any!(BuiltinForwarder: Send, Sync);
static_assertions::assert_impl_all!(TlsIndex: Copy, Eq, std::hash::Hash, std::fmt::Debug);
static_assertions::assert_eq_size!(TlsIndex, Option<TlsIndex>, u32);
const _: () = {
    assert!(!std::mem::needs_drop::<ThreadProjectionDescriptor>());
    assert!(BUFFER_SLOT_COUNT < THREAD_PROJECTION_OFFSET as usize);
};

fn plain_projection(
    table: &mut ThreadProjectionTable,
    value: Value,
) -> &'static ThreadProjectionDescriptor {
    table.project_for_test(CanonicalStorage::plain(value))
}

/// The family byte is untouched: a projection reads as a per-buffer header
/// carrying the marker, so no forwarder dispatch sees a new value.
#[test]
fn projection_heads_with_a_per_buffer_header_no_buffer_has() {
    let mut table = ThreadProjectionTable::default();
    let header = plain_projection(&mut table, Value::fixnum(4242)).header();
    assert_eq!(header.ty(), LispFwdType::BufferObj);
    let buf_fwd = match header.slot() {
        ForwardSlot::BufferObj(buf_fwd) => buf_fwd,
        other => panic!("projection resolved to {other:?}"),
    };
    assert_eq!(buf_fwd.offset, THREAD_PROJECTION_OFFSET);
    assert_eq!(buf_fwd.local_flags_idx, -1);
    assert!(buf_fwd.default.is_unbound());
    assert!(BufferSlot::from_u16(buf_fwd.offset).is_none());
}

/// The only ways from a per-buffer header to a slot are the two checked
/// accessors, and both reject the marker while answering a real slot.
#[test]
fn slot_accessors_reject_the_marker_and_answer_real_slots() {
    let mut table = ThreadProjectionTable::default();
    let header = plain_projection(&mut table, Value::NIL).header();
    let projected = header.as_buffer_obj_fwd().expect("a per-buffer header");
    assert!(projected.buffer_slot().is_none());
    assert_eq!(projected.slot_index().below(usize::from(u16::MAX)), None);

    let real = alloc_buffer_objfwd(
        5,
        -1,
        crate::buffer::buffer::BufferSlotPredicate::String,
        Value::NIL,
    );
    assert_eq!(real.buffer_slot().map(BufferSlot::index), Some(5));
    assert_eq!(real.slot_index().below(BUFFER_SLOT_COUNT), Some(5));
    assert_eq!(real.slot_index().below(5), None);
}

/// Every context-free accessor refuses the header as it refuses a
/// per-buffer slot it has no buffer for, and the slot read answers UNBOUND.
#[test]
fn context_free_accessors_refuse_a_projection() {
    let mut table = ThreadProjectionTable::default();
    let header = plain_projection(&mut table, Value::fixnum(4242)).header();
    assert_eq!(header.load(), None);
    assert_eq!(header.owned_value(), None);
    assert!(header.clone_stateful().is_none());
    assert!(header.as_int_fwd().is_none());
    assert!(header.as_bool_fwd().is_none());
    assert!(header.as_obj_fwd().is_none());
    assert!(header.as_kboard_obj_fwd().is_none());
    let buf_fwd = header.as_buffer_obj_fwd().expect("a per-buffer header");
    let slots = [Value::fixnum(1); BUFFER_SLOT_COUNT];
    for (slots, flags, defaults) in [
        (Some(&slots[..]), u64::MAX, Some(&slots[..])),
        (Some(&slots[..]), 0, None),
        (None, u64::MAX, Some(&slots[..])),
        (None, 0, None),
    ] {
        assert!(buf_fwd.value_in(slots, flags, defaults).is_unbound());
    }
}

#[test]
fn table_issues_dense_indices_and_resolves_only_its_own_headers() {
    let mut table = ThreadProjectionTable::default();
    assert!(table.is_empty());
    let first = plain_projection(&mut table, Value::fixnum(1));
    let second = plain_projection(&mut table, Value::fixnum(2));
    assert_eq!(table.len(), 2);
    assert_eq!((first.index().slot(), second.index().slot()), (0, 1));
    assert!(std::ptr::eq(table.get(first.index()).unwrap(), first));
    assert!(std::ptr::eq(
        table.projection(second.header()).unwrap(),
        second
    ));

    // A real per-buffer header, another table's projection and a copy of a
    // projection's header are not this table's projections.
    let real = alloc_buffer_objfwd(
        0,
        -1,
        crate::buffer::buffer::BufferSlotPredicate::String,
        Value::NIL,
    );
    assert!(table.projection(real.header()).is_none());
    let mut other = ThreadProjectionTable::default();
    let foreign = plain_projection(&mut other, Value::NIL);
    assert!(table.projection(foreign.header()).is_none());
    let copied: &'static LispBufferObjFwd =
        Box::leak(Box::new(*first.header().as_buffer_obj_fwd().unwrap()));
    assert_eq!(copied.offset, THREAD_PROJECTION_OFFSET);
    assert!(table.projection(copied.header()).is_none());
    // Nor is anything else.
    assert!(
        table
            .projection(alloc_objfwd(Value::NIL).header())
            .is_none()
    );
}

#[test]
fn canonical_storage_keeps_the_logical_cell() {
    let mut table = ThreadProjectionTable::default();
    let plain = plain_projection(&mut table, Value::fixnum(4242));
    match plain.canonical() {
        CanonicalStorage::Plain(cell) => assert_eq!(cell.get(), Value::fixnum(4242)),
        other => panic!("canonical storage became {other:?}"),
    }
    assert_eq!(plain.canonical().owned_value(), Some(Value::fixnum(4242)));

    let builtin = alloc_objfwd(Value::fixnum(7)).header();
    let wrapped =
        table.project_for_test(CanonicalStorage::forwarded(builtin).expect("a GNU family"));
    assert_eq!(wrapped.canonical().owned_value(), None);
    match wrapped.canonical() {
        CanonicalStorage::Forwarded(canonical) => {
            assert!(std::ptr::eq(canonical.descriptor(), builtin));
        }
        other => panic!("canonical storage became {other:?}"),
    }
}

#[test]
fn a_projection_never_wraps_a_projection() {
    let mut table = ThreadProjectionTable::default();
    let inner = plain_projection(&mut table, Value::NIL).header();
    assert!(CanonicalStorage::forwarded(inner).is_none());
    for header in [
        alloc_intfwd(LispInteger::from_i64(1)).header(),
        alloc_boolfwd(true).header(),
        alloc_objfwd(Value::NIL).header(),
        alloc_kboard_objfwd(Value::NIL).header(),
        alloc_buffer_objfwd(
            3,
            -1,
            crate::buffer::buffer::BufferSlotPredicate::String,
            Value::NIL,
        )
        .header(),
    ] {
        assert!(
            CanonicalStorage::forwarded(header).is_some(),
            "{:?}",
            header.ty()
        );
    }
}

/// The marker is unreachable for a real per-buffer registration.
#[test]
#[should_panic(expected = "per-buffer slot offset")]
fn a_real_per_buffer_slot_cannot_carry_the_marker() {
    alloc_buffer_objfwd(
        THREAD_PROJECTION_OFFSET,
        -1,
        crate::buffer::buffer::BufferSlotPredicate::Unrestricted,
        Value::NIL,
    );
}
