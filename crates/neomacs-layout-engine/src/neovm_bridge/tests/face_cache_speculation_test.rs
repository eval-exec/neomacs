//! The routed-row face-cache speculation journal.
//!
//! A routed row renders once through the real writer and rewinds on refusal
//! (`RoutedRowCheckpoint`). The caches the resolver writes during that
//! attempt — realized paint beside composed sources, folded named
//! specifications — live past the row, so the attempt journals every insert
//! and a refusal undoes them: a refused row must leave no face-cache side
//! effects visible (GNU instead lets a retried display keep its realized
//! faces, `face_change` being the only retirement, src/xfaces.c:3098-3105;
//! the rewind requirement here is the route's own invisibility contract).

use super::*;
use std::sync::Arc;

fn speculation_resolver() -> FaceResolver {
    let table = FaceTable::new();
    FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None)
}

fn speculation_source() -> Arc<RootedFace> {
    let overlay = NeoFace::new("speculation-source");
    Arc::new(RootedFace::new(overlay).expect("installed mutator heap"))
}

fn speculation_face(resolver: &FaceResolver, source: &Arc<RootedFace>) -> ResolvedFace {
    let mut face = resolver.realize_face(source.attributes());
    face.source_attributes = Some(Arc::clone(source));
    face.fg = 0x00F0E0D0;
    face.terminal_fg = Some(TerminalColor::Indexed(3));
    face.use_default_foreground = false;
    face.bg = 0x00102030;
    face.terminal_bg = None;
    face.use_default_background = true;
    face.terminal_inverse_video = false;
    face.terminal_underline_color = Some(TerminalColor::Indexed(6));
    face.underline_color = 0x00A0B0C0;
    face
}

#[test]
fn rewind_restores_an_evicted_paint_slot() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let resolver = speculation_resolver();
    let source = speculation_source();
    let frame = FrameId(7);

    // The pre-attempt paint this source carries into the routed row.
    let carried = speculation_face(&resolver, &source);
    resolver.record_tty_paint(
        TtyPaintKey::of(&carried),
        &carried,
        Arc::clone(&source),
        frame,
        41,
    );

    let mark = resolver.arm_face_cache_speculation();
    // The attempt re-realizes the same source from another entry state,
    // evicting the carried paint from the one-deep slot.
    let speculative = speculation_face(&resolver, &source);
    let mut other = speculative.clone();
    other.underline_color = 0x00112233;
    resolver.record_tty_paint(
        TtyPaintKey::of(&other),
        &other,
        Arc::clone(&source),
        frame,
        41,
    );
    assert!(
        resolver
            .cached_tty_paint(&other, &source, frame, 41)
            .is_some(),
        "the attempt's record answers before the rewind"
    );
    resolver.rewind_face_cache_speculation(mark);
    assert!(
        resolver
            .cached_tty_paint(&other, &source, frame, 41)
            .is_none(),
        "the refused attempt's paint must not answer"
    );
    assert!(
        resolver
            .cached_tty_paint(&carried, &source, frame, 41)
            .is_some(),
        "the paint the attempt evicted must be restored"
    );
}

#[test]
fn rewind_drops_a_fresh_named_spec_fold_and_accept_keeps_it() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let resolver = speculation_resolver();

    let mark = resolver.arm_face_cache_speculation();
    resolver.record_named_overlay_spec_for_alist("bold", NeoFace::new("bold"), ALIST_INDEPENDENT);
    assert!(
        resolver
            .cached_buffer_named_overlay_spec("bold", 0)
            .is_some(),
        "the fold is recorded while the attempt runs"
    );
    resolver.rewind_face_cache_speculation(mark);
    assert!(
        resolver
            .cached_buffer_named_overlay_spec("bold", 0)
            .is_none(),
        "a refused attempt's fold must not survive"
    );

    let mark = resolver.arm_face_cache_speculation();
    resolver.record_named_overlay_spec_for_alist(
        "italic",
        NeoFace::new("italic"),
        ALIST_INDEPENDENT,
    );
    resolver.accept_face_cache_speculation(mark);
    assert!(
        resolver
            .cached_buffer_named_overlay_spec("italic", 0)
            .is_some(),
        "a routed row keeps the folds it earned"
    );
}

#[test]
fn rewind_restores_a_replaced_named_spec_fold() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let resolver = speculation_resolver();

    let carried = resolver.record_named_overlay_spec_for_alist(
        "bold",
        NeoFace::new("bold-carried"),
        ALIST_INDEPENDENT,
    );
    let mark = resolver.arm_face_cache_speculation();
    resolver.record_named_overlay_spec_for_alist(
        "bold",
        NeoFace::new("bold-speculative"),
        ALIST_INDEPENDENT,
    );
    resolver.rewind_face_cache_speculation(mark);
    let restored = resolver
        .cached_buffer_named_overlay_spec("bold", 0)
        .expect("the carried fold must answer again");
    assert!(
        Arc::ptr_eq(&restored, &carried),
        "the restored fold is the exact allocation the attempt replaced"
    );
}

#[test]
fn accept_commits_permanently_against_an_outer_rewind() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let resolver = speculation_resolver();

    // The nesting rule: `accept` truncates the journal, so an inner window
    // that committed owns its entries outright — an outer refusal cannot
    // take back what a nested layout earned. Only inserts still holding
    // undo records (the outer window's own) rewind.
    let outer = resolver.arm_face_cache_speculation();
    let inner = resolver.arm_face_cache_speculation();
    resolver.record_named_overlay_spec_for_alist("inner", NeoFace::new("inner"), ALIST_INDEPENDENT);
    resolver.accept_face_cache_speculation(inner);
    resolver.record_named_overlay_spec_for_alist("outer", NeoFace::new("outer"), ALIST_INDEPENDENT);
    resolver.rewind_face_cache_speculation(outer);
    assert!(
        resolver
            .cached_buffer_named_overlay_spec("inner", 0)
            .is_some(),
        "an inner window that accepted keeps its entries"
    );
    assert!(
        resolver
            .cached_buffer_named_overlay_spec("outer", 0)
            .is_none(),
        "the outer refusal rewinds its own inserts"
    );
}
