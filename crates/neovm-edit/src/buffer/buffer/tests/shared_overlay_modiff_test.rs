//! GNU overlay modiff invalidates every live member of a shared text group,
//! while overlay lists and character/property modification state stay private.
use super::*;
use crate::emacs_core::Context;

fn fixture() -> (Context, BufferId, BufferId) {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let base = eval.buffers.create_buffer("shared-overlay-base");
    eval.buffers.set_current(base);
    eval.eval_str("(insert \"abcdef\\n\")").expect("base text");
    let indirect = eval
        .buffers
        .create_indirect_buffer(base, "shared-overlay-indirect", false)
        .expect("indirect peer");
    eval.buffers
        .get(base)
        .expect("base")
        .reset_unchanged_region();
    (eval, base, indirect)
}

fn make_overlay(eval: &mut Context, owner: BufferId) -> Value {
    eval.buffers.set_current(owner);
    eval.eval_str("(setq shared-overlay-test-object (make-overlay 1 2))")
        .expect("empty overlay creation")
}

fn tick(eval: &Context, buffer: BufferId) -> i64 {
    eval.buffers
        .get(buffer)
        .expect("buffer")
        .overlay_modified_tick()
}

#[test]
fn shared_overlay_modiff_invalidates_peers_without_sharing_overlay_lists() {
    for owner_is_base in [true, false] {
        let (mut eval, base, indirect) = fixture();
        let (owner, peer) = if owner_is_base {
            (base, indirect)
        } else {
            (indirect, base)
        };
        let overlay = make_overlay(&mut eval, owner);
        let before = tick(&eval, peer);
        eval.buffers
            .put_buffer_overlay_property(
                owner,
                overlay,
                Value::symbol("face"),
                Value::symbol("bold"),
            )
            .expect("actual overlay property change");
        assert_eq!(tick(&eval, peer), before.wrapping_add(1));
        assert_eq!(eval.buffers.get(owner).expect("owner").overlays.len(), 1);
        assert!(eval.buffers.get(peer).expect("peer").overlays.is_empty());
        let before = tick(&eval, peer);
        eval.buffers
            .move_buffer_overlay_to_emacs_byte_range(
                owner,
                overlay,
                EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(3)),
            )
            .expect("actual overlay relocation");
        assert_eq!(tick(&eval, peer), before.wrapping_add(1));
        let before = tick(&eval, peer);
        eval.buffers
            .delete_buffer_overlay(owner, overlay)
            .expect("overlay deletion");
        assert_eq!(tick(&eval, peer), before.wrapping_add(1));
        let overlay = make_overlay(&mut eval, owner);
        eval.buffers
            .put_buffer_overlay_property(
                owner,
                overlay,
                Value::symbol("face"),
                Value::symbol("bold"),
            )
            .expect("bulk-delete setup");
        let before = tick(&eval, peer);
        eval.buffers
            .delete_all_buffer_overlays(owner)
            .expect("bulk overlay deletion");
        assert_eq!(tick(&eval, peer), before.wrapping_add(1));
        assert!(eval.buffers.get(owner).expect("owner").overlays.is_empty());
        assert!(eval.buffers.get(peer).expect("peer").overlays.is_empty());
    }
}

#[test]
fn shared_overlay_modiff_noop_properties_and_empty_creation_do_not_invalidate_peers() {
    let (mut eval, base, indirect) = fixture();
    let before = tick(&eval, indirect);
    let overlay = make_overlay(&mut eval, base);
    assert_eq!(
        tick(&eval, indirect),
        before,
        "bare empty creation is not GNU modify_overlay"
    );
    eval.eval_str("(overlay-put shared-overlay-test-object 'face nil)")
        .expect("native nil no-op");
    assert_eq!(tick(&eval, indirect), before);
    let owner_before = tick(&eval, base);
    eval.buffers
        .put_buffer_overlay_property(base, overlay, Value::symbol("face"), Value::NIL)
        .expect("manager nil no-op");
    assert_eq!(
        tick(&eval, base),
        owner_before.wrapping_add(1),
        "preserve existing owner-only Manager behavior"
    );
    assert_eq!(
        tick(&eval, indirect),
        before,
        "do not extend owner-only no-op invalidation"
    );
    eval.eval_str("(overlay-put shared-overlay-test-object 'face 'bold)")
        .expect("actual native property setup");
    let before = tick(&eval, indirect);
    eval.eval_str("(overlay-put shared-overlay-test-object 'face 'bold)")
        .expect("native identical property no-op");
    assert_eq!(tick(&eval, indirect), before);
}

#[test]
fn shared_overlay_modiff_leaves_char_property_ticks_and_dirty_spans_unchanged() {
    let (mut eval, base, indirect) = fixture();
    let state = |eval: &Context, id| {
        let buffer = eval.buffers.get(id).expect("buffer");
        (
            buffer.modified_tick(),
            buffer.chars_modified_tick(),
            buffer.props_modified_tick(),
            buffer.changed_char_range(),
            buffer.gnu_beg_unchanged(),
        )
    };
    let before = [state(&eval, base), state(&eval, indirect)];
    make_overlay(&mut eval, indirect);
    eval.eval_str(
        "(progn (overlay-put shared-overlay-test-object 'face 'bold)
                           (move-overlay shared-overlay-test-object 2 4)
                           (delete-overlay shared-overlay-test-object))",
    )
    .expect("real overlay mutation sequence");
    assert_eq!([state(&eval, base), state(&eval, indirect)], before);
}
