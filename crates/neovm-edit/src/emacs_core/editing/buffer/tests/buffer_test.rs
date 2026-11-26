use neovm_value::value::Value;

#[cfg(test)]
mod conversion_byte_boundary;

#[cfg(test)]
mod insert_buffer_substring_recount;

fn check_overlay_put_evaporate_owner_boundary(empty: bool, shared_snapshot: bool) {
    use super::{builtin_make_overlay_in_buffers, builtin_overlay_buffer_in_buffers};
    use super::{builtin_overlay_put_in_buffers, resolve_overlay_buffer_id};
    use crate::buffer::{BufferManager, EmacsBytePos};

    crate::test_utils::init_test_tracing();
    let mut buffers = BufferManager::new();
    let buffer = buffers.create_buffer("evaporate-owner-boundary");
    buffers.get_mut(buffer).unwrap().insert("abcd");
    let overlay = builtin_make_overlay_in_buffers(
        &mut buffers,
        vec![
            Value::fixnum(2),
            Value::fixnum(if empty { 2 } else { 3 }),
            Value::make_buffer(buffer),
        ],
    )
    .unwrap();
    let evaporate = Value::symbol("evaporate");
    let list = &buffers.get(buffer).unwrap().overlays;
    let before_digest = list.content_digest();
    let before = list.snapshot();
    assert_eq!(before.len(), 1);
    assert!(!before.may_contain_property(evaporate));
    // Cover both the unshared cached-copy mirror and an observer retaining
    // the previous snapshot while the actual outer builtin mutates its owner.
    let old = if shared_snapshot {
        Some(before)
    } else {
        drop(before);
        None
    };
    assert_eq!(
        builtin_overlay_put_in_buffers(&mut buffers, vec![overlay, evaporate, Value::T]).unwrap(),
        Value::T
    );

    let list = &buffers.get(buffer).unwrap().overlays;
    assert_eq!(list.len(), usize::from(!empty));
    assert_eq!(list.may_contain_property(evaporate), !empty);
    // This recomputes the exact direct-key pair counts from owner membership,
    // including the decrement performed by BufferManager's delete path.
    list.assert_index_invariants_for_test();
    assert_eq!(
        list.content_digest(),
        list.content_digest_from_scratch_for_test()
    );
    let cached = list.snapshot();
    let fresh = list.snapshot_clone();
    cached.assert_index_invariants_for_test();
    fresh.assert_index_invariants_for_test();
    assert_eq!(cached.len(), fresh.len());
    assert_eq!(
        cached.may_contain_property(evaporate),
        fresh.may_contain_property(evaporate)
    );
    assert_eq!(cached.content_digest(), fresh.content_digest());
    assert_eq!(cached.content_digest(), list.content_digest());
    let state = |view: &crate::buffer::OverlayList| {
        view.overlays_in_gnu_lists_order()
            .into_iter()
            .map(|object| {
                (
                    view.overlay_start_emacs_byte_pos(object),
                    view.overlay_end_emacs_byte_pos(object),
                    view.overlay_get_named(object, evaporate),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(state(&cached), state(&fresh));
    assert_eq!(state(&cached), state(list));
    if empty {
        assert!(resolve_overlay_buffer_id(overlay).is_none());
        assert!(
            builtin_overlay_buffer_in_buffers(&buffers, vec![overlay])
                .unwrap()
                .is_nil()
        );
        assert!(cached.is_empty());
    } else {
        assert_eq!(resolve_overlay_buffer_id(overlay), Some(buffer));
        assert_eq!(
            state(list),
            vec![(
                Some(EmacsBytePos::new(1)),
                Some(EmacsBytePos::new(2)),
                Some(Value::T)
            )]
        );
        assert_eq!(
            list.overlays_at_emacs_byte_pos(EmacsBytePos::new(1)),
            vec![overlay]
        );
    }
    if let Some(old) = old {
        assert_eq!(
            old.len(),
            1,
            "the previous observer keeps its original member"
        );
        assert!(!old.may_contain_property(evaporate));
        old.assert_index_invariants_for_test();
        assert_eq!(old.content_digest(), before_digest);
        assert_eq!(
            old.content_digest(),
            old.content_digest_from_scratch_for_test()
        );
        assert_eq!(
            state(&old),
            vec![(
                Some(EmacsBytePos::new(1)),
                Some(EmacsBytePos::new(if empty { 1 } else { 2 })),
                None,
            )]
        );
    }
}

#[test]
fn overlay_put_evaporate_empty_deletes_through_buffer_owner() {
    for shared_snapshot in [false, true] {
        check_overlay_put_evaporate_owner_boundary(true, shared_snapshot);
    }
}

#[test]
fn overlay_put_evaporate_nonempty_preserves_buffer_owner() {
    for shared_snapshot in [false, true] {
        check_overlay_put_evaporate_owner_boundary(false, shared_snapshot);
    }
}

fn check_changed_non_evaporating_put_owner_queries(update: bool) {
    use super::{builtin_make_overlay_in_buffers, builtin_overlay_put_in_buffers};
    use crate::buffer::{BufferManager, EmacsBytePos};

    crate::test_utils::init_test_tracing();
    let mut buffers = BufferManager::new();
    let buffer = buffers.create_buffer("non-evaporating-put-owner-queries");
    buffers.get_mut(buffer).unwrap().insert("abcd");
    let overlay = builtin_make_overlay_in_buffers(
        &mut buffers,
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::make_buffer(buffer),
        ],
    )
    .unwrap();
    let property = Value::symbol("priority");
    if update {
        assert_eq!(
            builtin_overlay_put_in_buffers(
                &mut buffers,
                vec![overlay, property, Value::fixnum(1)],
            )
            .unwrap(),
            Value::fixnum(1)
        );
    }
    let list = &buffers.get(buffer).unwrap().overlays;
    let before_digest = list.content_digest();
    assert_eq!(list.may_contain_property(property), update);
    // BufferManager membership is unchanged through this builtin call, so
    // this live owner's index keeps the same counter target address.
    list.start_owner_query_count_for_test();
    let result =
        builtin_overlay_put_in_buffers(&mut buffers, vec![overlay, property, Value::fixnum(2)]);
    let list = &buffers.get(buffer).unwrap().overlays;
    let owner_queries = list.finish_owner_query_count_for_test();

    // Keep semantic and consistency witnesses outside the measured interval.
    assert_eq!(result.unwrap(), Value::fixnum(2));
    assert_eq!(super::resolve_overlay_buffer_id(overlay), Some(buffer));
    assert_eq!(list.len(), 1);
    assert_eq!(
        list.overlay_get_named(overlay, property),
        Some(Value::fixnum(2))
    );
    assert_eq!(
        list.overlay_start_emacs_byte_pos(overlay),
        Some(EmacsBytePos::new(1))
    );
    assert_eq!(
        list.overlay_end_emacs_byte_pos(overlay),
        Some(EmacsBytePos::new(2))
    );
    assert!(list.may_contain_property(property));
    list.assert_index_invariants_for_test();
    assert_eq!(
        list.content_digest(),
        list.content_digest_from_scratch_for_test()
    );
    assert_ne!(list.content_digest(), before_digest);
    assert_eq!(
        owner_queries, 1,
        "a changed non-evaporate put resolves its live owner only for the plist write"
    );
}

#[test]
fn changed_non_evaporating_update_resolves_live_owner_once() {
    check_changed_non_evaporating_put_owner_queries(true);
}

#[test]
fn changed_non_evaporating_insert_resolves_live_owner_once() {
    check_changed_non_evaporating_put_owner_queries(false);
}
