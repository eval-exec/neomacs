//! Actual terminal realization entry tests. No cache hit is admitted here.
use super::*;
use neovm_core::emacs_core::terminal::pure;
use std::cell::Cell;
use std::sync::Arc;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        pure::reset_terminal_host();
        pure::reset_terminal_runtime();
    }
}

fn fixture() -> (Context, FrameId, FaceResolver, RestoreTerminal) {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let restore = RestoreTerminal;
    context.set_gc_threshold(usize::MAX);
    pure::configure_terminal_runtime(pure::TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let source = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frame_manager_mut()
        .create_frame("rows-face-witness", 120, 40, source);
    assert!(context.frame_is_tty_face_target(frame));
    context.sync_runtime_faces_for_frame(frame);
    let mut resolver = FaceResolver::new(context.face_table(), 0x00ffffff, 0, 14.0, None);
    resolver.prepare_tty_realization(frame, &mut context, None);
    assert!(resolver.has_tty_realization());
    context
        .eval_str(
            r#"(setq rows-face-calls 0 tty-defined-color-alist nil)
      (fset 'tty-color-desc (lambda (&rest ignored)
        (setq rows-face-calls (1+ rows-face-calls))
        (list "rows-face-color" 4 12345 23456 34567)))"#,
        )
        .unwrap();
    (context, frame, resolver, restore)
}

fn custom_face(resolver: &FaceResolver, color: &str) -> ResolvedFace {
    let mut attributes = NeoFace::new("rows-observed-cold");
    attributes.color_specifications.foreground =
        Some(Arc::new(neovm_core::face::SpecifiedColor::parse(color)));
    let source = Arc::new(RootedFace::new(attributes).expect("installed source owner"));
    let mut face = resolver.realize_face(source.attributes());
    face.source_attributes = Some(source);
    face.canonical_tty_role = None;
    face
}

fn calls(context: &mut Context) -> i64 {
    context
        .eval_str("rows-face-calls")
        .unwrap()
        .as_int()
        .unwrap()
}

#[test]
fn rows_face_witness_actual_warm_basic_role_stays_clear() {
    let (mut context, _, resolver, _restore) = fixture();
    let cold = Cell::new(false);
    let services = ObservedFaceServices::new(&resolver, &cold);
    let mut face = resolver.default_face().clone();
    services.realize_for_display(&mut face, &mut context);
    assert!(!cold.get());
    assert_eq!(calls(&mut context), 0);
}

#[test]
fn rows_face_witness_actual_core_warm_and_local_warm_paths_stay_clear() {
    let (mut context, frame, resolver, _restore) = fixture();
    let mut face = custom_face(&resolver, "rows-core-warm-color");
    let source = face.source_attributes.as_ref().unwrap();
    context.realize_display_face_for_frame(frame, source.attributes());
    assert_eq!(
        calls(&mut context),
        1,
        "real canonical Core warmup called Lisp"
    );
    let cold = Cell::new(false);
    let services = ObservedFaceServices::new(&resolver, &cold);
    let mut repeated = face.clone();
    services.realize_for_display(&mut face, &mut context);
    services.realize_for_display(&mut repeated, &mut context);
    assert!(!cold.get());
    assert_eq!(calls(&mut context), 1);
    assert_eq!(face.terminal_fg, repeated.terminal_fg);
}

#[test]
fn rows_face_witness_actual_cold_callback_is_sticky_after_a_warm_hit() {
    let (mut context, _, resolver, _restore) = fixture();
    let mut face = custom_face(&resolver, "rows-actual-cold-color");
    let mut repeated = face.clone();
    let cold = Cell::new(false);
    let services = ObservedFaceServices::new(&resolver, &cold);
    services.realize_for_display(&mut face, &mut context);
    assert!(
        cold.get(),
        "the actual demand gateway marked before entering Core"
    );
    assert_eq!(calls(&mut context), 1);
    services.realize_for_display(&mut repeated, &mut context);
    assert!(
        cold.get(),
        "a subsequent warm result cannot erase the entry"
    );
    assert_eq!(calls(&mut context), 1);
}

#[test]
fn rows_face_witness_actual_failed_callback_without_cache_insert_is_cold() {
    let (mut context, frame, resolver, _restore) = fixture();
    context
        .eval_str(
            r#"(fset 'tty-color-desc (lambda (&rest ignored)
      (setq rows-face-calls (1+ rows-face-calls))
      (signal 'error (list "rows face callback failed"))))"#,
        )
        .unwrap();
    let mut face = custom_face(&resolver, "rows-error-color");
    let source = Arc::clone(face.source_attributes.as_ref().unwrap());
    let cold = Cell::new(false);
    ObservedFaceServices::new(&resolver, &cold).realize_for_display(&mut face, &mut context);
    assert!(cold.get());
    assert!(context.redisplay_hook_flow_pending());
    assert!(
        context
            .cached_display_face_for_frame(frame, source.attributes())
            .is_none()
    );
    assert!(
        source
            .cached_tty_paint(frame, context.face_change_count(), &face)
            .is_none()
    );
}

#[test]
fn rows_face_witness_pending_flow_preserves_placeholder_instead_of_core_warm_paint() {
    let (mut context, frame, resolver, _restore) = fixture();
    let mut face = custom_face(&resolver, "rows-pending-warm-color");
    let source = Arc::clone(face.source_attributes.as_ref().unwrap());
    context.realize_display_face_for_frame(frame, source.attributes());
    assert_eq!(calls(&mut context), 1);
    // This is the real production pending-flow seam, not a synthetic cache
    // witness. The old method returns source colors and does not seed paint.
    let form = context
        .eval_str("'(signal 'error (list \"pending face flow\"))")
        .unwrap();
    let pending = context.eval_value(&form).unwrap_err();
    context.defer_redisplay_hook_flow(pending);
    let mut legacy = face.clone();
    resolver.realize_for_display(&mut legacy, &mut context);
    let cold = Cell::new(false);
    ObservedFaceServices::new(&resolver, &cold).realize_for_display(&mut face, &mut context);
    assert!(cold.get(), "pending demand still enters the real Core API");
    assert_eq!(face.fg, legacy.fg);
    assert_eq!(face.terminal_fg, legacy.terminal_fg);
    assert!(context.redisplay_hook_flow_pending());
    assert!(
        source
            .cached_tty_paint(frame, context.face_change_count(), &face)
            .is_none()
    );
}

#[test]
fn rows_face_witness_noop_and_observed_realization_keep_callback_and_paint_parity() {
    fn run(observed: bool) -> (i64, u32, Option<TerminalColor>) {
        let (mut context, _, resolver, _restore) = fixture();
        let mut face = custom_face(&resolver, "rows-parity-color");
        if observed {
            let cold = Cell::new(false);
            ObservedFaceServices::new(&resolver, &cold)
                .realize_for_display(&mut face, &mut context);
            assert!(cold.get());
        } else {
            FaceServices::realize_for_display(&resolver, &mut face, &mut context);
        }
        (calls(&mut context), face.fg, face.terminal_fg)
    }
    assert_eq!(run(false), run(true));
}

#[test]
fn rows_face_witness_speculation_rewind_cannot_erase_actual_cold_entry() {
    let (mut context, _, resolver, _restore) = fixture();
    let mut face = custom_face(&resolver, "rows-rewind-color");
    let cold = Cell::new(false);
    let mark = resolver.arm_face_cache_speculation();
    ObservedFaceServices::new(&resolver, &cold).realize_for_display(&mut face, &mut context);
    assert!(cold.get());
    resolver.rewind_face_cache_speculation(mark);
    assert!(cold.get(), "the witness is not an undo-journal length");
    assert_eq!(calls(&mut context), 1);
}
