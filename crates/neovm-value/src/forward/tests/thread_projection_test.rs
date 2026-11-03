//! The thread-projection descriptor (`forward::thread_projection`): a
//! per-buffer header no buffer has, refused by every context-free accessor
//! through checks per-buffer slots already make.

use super::*;
use crate::buffer::buffer::{BUFFER_SLOT_COUNT, BufferSlot};

static_assertions::assert_not_impl_any!(ThreadProjectionDescriptor: Copy, Clone, Send, Sync);
static_assertions::assert_not_impl_any!(ThreadProjectionTable: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(CanonicalBlv: Send, Sync);
static_assertions::assert_impl_all!(CanonicalArm: Copy, Eq);
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
    table.project_for_test(CanonicalArm::Plain, value, false)
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
        BufferSlot::from_u16(5).unwrap(),
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
    // Slot 0 is reserved for the shared VOID descriptor.
    assert_eq!((first.index().slot(), second.index().slot()), (1, 2));
    assert!(std::ptr::eq(table.get(first.index()).unwrap(), first));
    assert!(std::ptr::eq(
        table.projection(second.header()).unwrap(),
        second
    ));

    // A real per-buffer header and another table's projection are not this
    // table's projections (a header cannot be copied out of its body).
    let real = alloc_buffer_objfwd(
        BufferSlot::from_u16(0).unwrap(),
        -1,
        crate::buffer::buffer::BufferSlotPredicate::String,
        Value::NIL,
    );
    assert!(table.projection(real.header()).is_none());
    let mut other = ThreadProjectionTable::default();
    let foreign = plain_projection(&mut other, Value::NIL);
    assert!(table.projection(foreign.header()).is_none());
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
    assert_eq!(plain.arm(), CanonicalArm::Plain);
    assert_eq!(plain.plain_value(), Value::fixnum(4242));
    // The plain word is an ordinary objfwd the obarray roots.
    assert_eq!(plain.plain_root().load(), Some(Value::fixnum(4242)));
    plain.set_plain_value(Value::fixnum(1));
    assert_eq!(plain.plain_value(), Value::fixnum(1));
    assert_eq!(plain.plain_root().load(), Some(Value::fixnum(1)));

    let builtin = alloc_objfwd(Value::fixnum(7)).header();
    let arm = CanonicalArm::Forwarded(BuiltinForwarder::new(builtin).expect("a GNU family"));
    let wrapped = table.project_for_test(arm, Value::UNBOUND, true);
    assert!(wrapped.host_projected());
    match wrapped.arm() {
        CanonicalArm::Forwarded(canonical) => {
            assert!(canonical.descriptor() == builtin);
        }
        other => panic!("canonical storage became {other:?}"),
    }
    wrapped.set_arm(CanonicalArm::Plain);
    assert_eq!(wrapped.arm(), CanonicalArm::Plain);
}

#[test]
fn a_projection_never_wraps_a_projection() {
    let mut table = ThreadProjectionTable::default();
    let inner = plain_projection(&mut table, Value::NIL).header();
    assert!(BuiltinForwarder::new(inner).is_none());
    for header in [
        alloc_intfwd(LispInteger::from_i64(1)).header(),
        alloc_boolfwd(true).header(),
        alloc_objfwd(Value::NIL).header(),
        alloc_kboard_objfwd(Value::NIL).header(),
        alloc_buffer_objfwd(
            BufferSlot::from_u16(3).unwrap(),
            -1,
            crate::buffer::buffer::BufferSlotPredicate::String,
            Value::NIL,
        )
        .header(),
    ] {
        assert!(BuiltinForwarder::new(header).is_some(), "{:?}", header.ty());
    }
}

/// The cast resolver answers exactly the projections, and nothing else.
#[test]
fn headers_resolve_to_their_projection() {
    let mut table = ThreadProjectionTable::default();
    let projection = plain_projection(&mut table, Value::fixnum(3));
    let resolved = projection
        .header()
        .thread_projection()
        .expect("a projection");
    assert!(std::ptr::eq(resolved, projection));
    assert_eq!(resolved.index(), projection.index());
    for header in [
        alloc_objfwd(Value::NIL).header(),
        alloc_buffer_objfwd(
            BufferSlot::from_u16(0).unwrap(),
            -1,
            crate::buffer::buffer::BufferSlotPredicate::Unrestricted,
            Value::NIL,
        )
        .header(),
    ] {
        assert!(header.thread_projection().is_none(), "{:?}", header.ty());
    }
}

/// Exercise the stored cell handle and table resolver without a Context or
/// bootstrap obarray. The handle must retain the whole descriptor's extent.
#[test]
fn miriub3_fwdref_resolves_whole_thread_projection() {
    let mut table = ThreadProjectionTable::default();
    let projection = plain_projection(&mut table, Value::fixnum(97));
    let handle = projection.header();
    assert!(std::ptr::eq(
        handle.thread_projection().unwrap(),
        projection
    ));
    let stored = FwdRef::from_address(handle.address());
    let resolved = stored.thread_projection().unwrap();
    assert!(std::ptr::eq(resolved, projection));
    assert_eq!(resolved.plain_value(), Value::fixnum(97));
    assert!(std::ptr::eq(table.projection(stored).unwrap(), projection));
}
