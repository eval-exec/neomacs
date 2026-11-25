//! R9 verification controls for native auxiliary scroll-coverage publication.
//!
//! Covers:
//! - differential content oracle vs forced-redisplay export,
//! - command-boundary service (`service_gui_command_boundary_at`, H2/R2),
//! - base-not-latest refusal then activation-triggered native publish (R3),
//! - bounded non-spinning idle maintenance on refused bases (R8),
//! - all R7 invalidators (source, point, overlay, fontset, media, topology,
//!   placement, display var/policy, and v1 offscreen new-face refusal),
//! - unconditional withdrawal on in-place display-spec invalidation (R5),
//! - #832 own-late-chrome base refusal & withdrawal without dropping pages.

use super::*;
use neomacs_display_protocol::scroll_coverage::{AuxiliaryScrollCoverage, ScrollCoverage};
use neomacs_display_protocol::{DisplayWindowId, PresentationId};
use neovm_core::emacs_core::eval::keyboard::KeyEvent;
use neovm_core::emacs_core::eval::{
    DisplayMaintenanceOutcome, DisplayMaintenancePublication, RedisplayHookPolicyGuard,
};
use neovm_core::window::{DisplayFrameExt, DisplayFrameManagerExt, FrameId, WindowId};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

struct AuxFixture {
    engine: Rc<RefCell<LayoutEngine>>,
    eval: Context,
    frame: FrameId,
    owner: DisplayWindowId,
    producer_calls: Rc<Cell<usize>>,
    delivered: Rc<RefCell<Vec<AuxiliaryScrollCoverage>>>,
    refusals: Rc<RefCell<Vec<AuxiliaryPublicationRefusal>>>,
}

impl Drop for AuxFixture {
    fn drop(&mut self) {
        self.eval.display_idle_maintenance_fn = None;
        self.eval.redisplay_fn = None;
    }
}

impl AuxFixture {
    fn new(setup_elisp: Option<&str>) -> Self {
        let line = "ordinary offscreen text\n";
        let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
        assert!(eval.frame_manager_mut().select_frame(frame));
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(
            "(progn
               (setq aux-raise-spec (list 'raise 0.25)
                     aux-pre-count 0
                     aux-mode-line-count 0
                     timer-list nil timer-idle-list nil
                     mode-line-format
                     '((:eval (progn
                                (setq aux-mode-line-count (1+ aux-mode-line-count))
                                \"aux-ml\")))
                     pre-redisplay-function
                     (lambda (_windows)
                       (setq aux-pre-count (1+ aux-pre-count))))
               (put-text-property 1 (point-max) 'display aux-raise-spec))",
        )
        .unwrap();
        if let Some(elisp) = setup_elisp {
            eval.eval_str(elisp).unwrap();
        }
        let engine = Rc::new(RefCell::new(
            crate::font::metrics::test_catalog::layout_engine(),
        ));
        let producer_calls = Rc::new(Cell::new(0));
        let delivered = Rc::new(RefCell::new(Vec::new()));
        let refusals = Rc::new(RefCell::new(Vec::new()));

        let prod_count = Rc::clone(&producer_calls);
        let prod_engine = Rc::clone(&engine);
        eval.redisplay_fn = Some(Box::new(move |eval| {
            prod_count.set(prod_count.get() + 1);
            let mut eng = prod_engine.borrow_mut();
            eng.layout_frame_rust(eval, frame);
            activate_last_engine_presentation(eval, &eng, frame);
        }));
        eval.redisplay().expect("initial redisplay");

        let maint_engine = Rc::clone(&engine);
        let maint_delivered = Rc::clone(&delivered);
        let maint_refusals = Rc::clone(&refusals);
        eval.display_idle_maintenance_fn = Some(Box::new(move |eval| {
            let mut eng = maint_engine.borrow_mut();
            let progress = eng.maintain_scroll_coverage(eval);
            let publication = if eng.has_actionable_scroll_coverage_publication(eval) {
                match eng.prepare_auxiliary_scroll_coverage(eval) {
                    AuxiliaryPreparation::Ready(prepared) => {
                        let was_refusal_withdrawal = prepared.refusal_reason().is_some();
                        if let Some(refusal) = prepared.refusal_reason() {
                            maint_refusals.borrow_mut().push(refusal);
                        }
                        maint_delivered.borrow_mut().push(prepared.clone_update());
                        eng.commit_auxiliary_scroll_coverage(prepared);
                        if was_refusal_withdrawal {
                            DisplayMaintenancePublication::Deferred
                        } else {
                            DisplayMaintenancePublication::PublishedNatively
                        }
                    }
                    AuxiliaryPreparation::NoChange => DisplayMaintenancePublication::Idle,
                    AuxiliaryPreparation::Refused(refusal) => {
                        maint_refusals.borrow_mut().push(refusal);
                        DisplayMaintenancePublication::Deferred
                    }
                }
            } else {
                DisplayMaintenancePublication::Idle
            };
            DisplayMaintenanceOutcome {
                next_wake: progress.map(|p| p.next_wake()),
                publication,
            }
        }));

        Self {
            engine,
            eval,
            frame,
            owner: DisplayWindowId::new(window.0 as i64),
            producer_calls,
            delivered,
            refusals,
        }
    }

    fn pre_count(&self) -> i64 {
        self.eval
            .builtin_var_value_by_name("aux-pre-count")
            .unwrap()
            .as_fixnum()
            .unwrap()
    }

    fn mode_line_count(&self) -> i64 {
        self.eval
            .builtin_var_value_by_name("aux-mode-line-count")
            .unwrap()
            .as_fixnum()
            .unwrap()
    }

    /// Drive the background worker until a closed prefix of `target_rows` has
    /// drained into `prepared_viewports` (leaving `publication_pending = true`).
    fn drain_worker_prefix(&mut self, target_rows: usize) {
        for _ in 0..target_rows {
            let captured = self
                .engine
                .borrow()
                .scroll_coverage
                .active_capture_progress_for_test()
                .map_or(0, |(_, _, rows)| rows);
            if captured >= target_rows {
                break;
            }
            assert!(
                self.engine
                    .borrow_mut()
                    .maintain_scroll_coverage(&self.eval)
                    .is_some()
            );
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let ready = {
                let mut eng = self.engine.borrow_mut();
                let eng = &mut *eng;
                eng.scroll_coverage
                    .drain(&mut eng.prepared_viewports)
                    .unwrap()
            };
            if ready {
                break;
            }
            assert!(Instant::now() < deadline, "worker prefix timed out");
            std::thread::yield_now();
        }
        assert!(
            self.engine
                .borrow()
                .prepared_viewports
                .has_computed(self.frame, self.owner)
        );
    }
}

fn assert_coverage_content_equivalent(aux: &ScrollCoverage, forced: &ScrollCoverage) {
    assert_eq!(aux.anchor_row, forced.anchor_row, "anchor_row");
    assert_eq!(aux.viewport, forced.viewport, "viewport");
    assert_eq!(aux.origin, forced.origin, "origin");
    assert_eq!(aux.predict_pixels, forced.predict_pixels, "predict_pixels");
    assert_eq!(
        aux.compositor_enabled, forced.compositor_enabled,
        "compositor_enabled"
    );
    assert_eq!(aux.content.window_id, forced.content.window_id, "window_id");
    assert_eq!(
        aux.content.pixel_bounds, forced.content.pixel_bounds,
        "pixel_bounds"
    );
    assert_eq!(
        aux.content.text_pixel_bounds, forced.content.text_pixel_bounds,
        "text_pixel_bounds"
    );
    assert_eq!(
        aux.content.text_clip_bounds, forced.content.text_clip_bounds,
        "text_clip_bounds"
    );
    assert_eq!(aux.content.selected, forced.content.selected, "selected");
    assert_eq!(
        aux.content.matrix.rows.len(),
        forced.content.matrix.rows.len(),
        "matrix row count"
    );
    for (idx, (a, b)) in aux
        .content
        .matrix
        .rows
        .iter()
        .zip(forced.content.matrix.rows.iter())
        .enumerate()
    {
        assert_eq!(a.enabled, b.enabled, "row {idx} enabled");
        assert_eq!(a.role, b.role, "row {idx} role");
        assert_eq!(a.pixel_y, b.pixel_y, "row {idx} pixel_y");
        assert_eq!(a.height_px, b.height_px, "row {idx} height_px");
        assert_eq!(a.ascent_px, b.ascent_px, "row {idx} ascent_px");
        assert_eq!(a.start_charpos, b.start_charpos, "row {idx} start_charpos");
        assert_eq!(a.end_charpos, b.end_charpos, "row {idx} end_charpos");
        assert_eq!(a.continued, b.continued, "row {idx} continued");
        assert_eq!(a.glyphs, b.glyphs, "row {idx} glyphs");
    }
    assert_eq!(aux.faces, forced.faces, "faces");
    assert_eq!(aux.fonts, forced.fonts, "fonts");
    assert_eq!(aux.char_fonts, forced.char_fonts, "char_fonts");
    assert_eq!(
        aux.shaped_clusters, forced.shaped_clusters,
        "shaped_clusters"
    );
    assert_eq!(
        aux.hit_index.regions(),
        forced.hit_index.regions(),
        "hit_index regions"
    );
    assert_eq!(
        aux.hit_index.text_positions(),
        forced.hit_index.text_positions(),
        "hit_index positions"
    );
    assert_eq!(aux.pointer_source, forced.pointer_source, "pointer_source");
}

#[test]
fn auxiliary_prefix_matches_forced_redisplay_export_content() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    // Run the twin fixtures sequentially so only one Context heap is installed
    // on this thread at a time.
    let aux_snapshots: Vec<ScrollCoverage> = {
        let mut aux_fix = AuxFixture::new(None);
        let mut out = Vec::new();
        for target_rows in [4_usize, 8] {
            aux_fix.drain_worker_prefix(target_rows);
            aux_fix
                .eval
                .display_idle_maintenance_deadline(true, false)
                .unwrap();
            let cov = aux_fix
                .engine
                .borrow()
                .published_scroll_coverage(aux_fix.frame, aux_fix.owner)
                .expect("aux coverage published")
                .as_ref()
                .clone();
            out.push(cov);
        }
        assert_eq!(aux_fix.producer_calls.get(), 1);
        out
    };
    let forced_snapshots: Vec<ScrollCoverage> = {
        let mut forced_fix = AuxFixture::new(None);
        let mut out = Vec::new();
        for target_rows in [4_usize, 8] {
            forced_fix.drain_worker_prefix(target_rows);
            forced_fix.eval.invalidate_redisplay();
            forced_fix.eval.redisplay().unwrap();
            let cov = forced_fix
                .engine
                .borrow()
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .scroll_coverage
                .iter()
                .find(|c| c.content.window_id == forced_fix.owner)
                .expect("forced redisplay coverage")
                .as_ref()
                .clone();
            out.push(cov);
        }
        assert_eq!(forced_fix.producer_calls.get(), 3);
        out
    };
    for (aux_cov, forced_cov) in aux_snapshots.iter().zip(forced_snapshots.iter()) {
        assert_coverage_content_equivalent(aux_cov, forced_cov);
    }
}

#[test]
fn command_boundary_service_publishes_auxiliary_coverage_without_logical_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut fix = AuxFixture::new(None);
    let initial_producers = fix.producer_calls.get();
    let initial_pre = fix.pre_count();
    let initial_ml = fix.mode_line_count();
    let initial_gen = fix.eval.redisplay_generation();

    fix.drain_worker_prefix(4);
    for ch in ['a', 'b'] {
        fix.eval.command_loop_mut().unread_key(KeyEvent::char(ch));
    }
    let now = Instant::now();
    fix.eval.service_gui_command_boundary_at(now).unwrap();
    assert!(fix.delivered.borrow().is_empty(), "deadline not yet due");
    fix.eval
        .service_gui_command_boundary_at(now + Duration::from_millis(20))
        .unwrap();

    assert_eq!(fix.delivered.borrow().len(), 1);
    assert!(
        fix.engine
            .borrow()
            .published_scroll_coverage(fix.frame, fix.owner)
            .is_some(),
        "command boundary published native coverage"
    );
    assert_eq!(fix.producer_calls.get(), initial_producers);
    assert_eq!(fix.pre_count(), initial_pre);
    assert_eq!(fix.mode_line_count(), initial_ml);
    assert_eq!(fix.eval.redisplay_generation(), initial_gen);
    assert_eq!(
        fix.eval.command_loop_mut().read_key_event(),
        Some(Value::fixnum('a' as i64))
    );
    assert_eq!(
        fix.eval.command_loop_mut().read_key_event(),
        Some(Value::fixnum('b' as i64))
    );
}

#[test]
fn auxiliary_refuses_unactivated_latest_seal_then_publishes_on_activation() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut fix = AuxFixture::new(None);
    let first_active = fix
        .eval
        .frame_manager()
        .get(fix.frame)
        .unwrap()
        .active_presentation()
        .unwrap();

    // Seal a newer presentation N+1 WITHOUT activating it (N remains active).
    let unactivated_id = {
        let mut eng = fix.engine.borrow_mut();
        eng.layout_frame_rust(&mut fix.eval, fix.frame);
        eng.last_frame_display_state
            .as_ref()
            .unwrap()
            .presentation()
    };
    assert_ne!(unactivated_id.get(), first_active.get());
    assert_eq!(
        fix.eval
            .frame_manager()
            .get(fix.frame)
            .unwrap()
            .active_presentation(),
        Some(first_active)
    );

    fix.drain_worker_prefix(4);
    let producers_before = fix.producer_calls.get();
    let pre_before = fix.pre_count();

    // While N is active and N+1 is latest sealed, aux MUST refuse with BaseNotActive.
    let outcome = fix
        .eval
        .display_idle_maintenance_deadline(true, false)
        .unwrap();
    let _ = outcome;
    assert!(fix.delivered.borrow().is_empty());
    assert_eq!(
        fix.refusals.borrow().as_slice(),
        &[AuxiliaryPublicationRefusal::BaseNotActive {
            latest_sealed: unactivated_id,
            active: Some(PresentationId::new(first_active.get())),
        }]
    );
    assert!(
        !fix.engine
            .borrow()
            .has_actionable_scroll_coverage_publication(&fix.eval),
        "R8: refused BaseNotActive must not stay actionable while base is unchanged"
    );

    // Once N+1 activates, the pending prefix becomes actionable and publishes
    // natively without any logical redisplay.
    fix.eval
        .frame_manager_mut()
        .get_mut(fix.frame)
        .unwrap()
        .activate_display_presentation(neovm_core::window::geometry::PresentationId::new(
            unactivated_id.get(),
        ))
        .expect("activate N+1");
    assert!(
        fix.engine
            .borrow()
            .has_actionable_scroll_coverage_publication(&fix.eval)
    );
    fix.eval
        .display_idle_maintenance_deadline(true, false)
        .unwrap();
    assert_eq!(fix.delivered.borrow().len(), 1);
    assert_eq!(fix.delivered.borrow()[0].base, unactivated_id);
    assert!(
        fix.engine
            .borrow()
            .published_scroll_coverage(fix.frame, fix.owner)
            .is_some()
    );
    assert_eq!(fix.producer_calls.get(), producers_before);
    assert_eq!(fix.pre_count(), pre_before);
}

#[test]
fn refused_base_quiesces_bounded_idle_maintenance_calls() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut fix = AuxFixture::new(None);
    fix.drain_worker_prefix(4);
    // Exhaust further worker targets for this window without erasing its
    // tracking entry (erasing the entry would legitimately reschedule capture).
    {
        let mut eng = fix.engine.borrow_mut();
        for entry in eng.scroll_coverage.windows.values_mut() {
            entry.targets.clear();
        }
        eng.scroll_coverage.cancel_active();
    }
    // Create evaluator debt so auxiliary publication refuses.
    fix.eval.invalidate_redisplay();

    let mut wakes = 0usize;
    for _ in 0..20 {
        if fix
            .eval
            .display_idle_maintenance_deadline(true, false)
            .unwrap()
            .is_some()
        {
            wakes += 1;
        }
    }
    assert_eq!(
        fix.refusals.borrow().len(),
        1,
        "R8: refused base must be evaluated once, not re-attempted on every idle tick"
    );
    assert!(
        wakes <= 1,
        "R8: a refused base with no worker progress must not spin (got {wakes} wakes)"
    );
}

#[test]
fn in_place_display_spec_mutation_withdraws_published_auxiliary_coverage() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut fix = AuxFixture::new(None);
    fix.drain_worker_prefix(4);
    fix.eval
        .display_idle_maintenance_deadline(true, false)
        .unwrap();
    assert_eq!(fix.delivered.borrow().len(), 1);
    assert_eq!(fix.delivered.borrow()[0].surfaces.len(), 1);
    assert!(
        fix.engine
            .borrow()
            .published_scroll_coverage(fix.frame, fix.owner)
            .is_some()
    );

    let producers_before = fix.producer_calls.get();
    let pre_before = fix.pre_count();
    fix.eval
        .eval_str("(setcar (cdr aux-raise-spec) 0.75)")
        .unwrap();
    fix.eval
        .display_idle_maintenance_deadline(true, false)
        .unwrap();

    assert_eq!(fix.delivered.borrow().len(), 2);
    assert!(
        fix.delivered.borrow()[1].surfaces.is_empty(),
        "in-place display-spec invalidation must deliver an unconditional withdrawal"
    );
    assert!(
        fix.engine
            .borrow()
            .published_scroll_coverage(fix.frame, fix.owner)
            .is_none()
    );
    assert_eq!(fix.producer_calls.get(), producers_before);
    assert_eq!(fix.pre_count(), pre_before);
}

#[test]
fn auxiliary_refuses_source_point_overlay_fontset_media_topology_placement_and_display_var() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let cases: &[(&str, fn(&mut AuxFixture))] = &[
        ("source", |fix| {
            fix.eval.eval_str("(insert \"X\")").unwrap();
        }),
        ("point", |fix| {
            fix.eval.eval_str("(forward-char 1)").unwrap();
        }),
        ("overlay", |fix| {
            fix.eval
                .eval_str("(let ((o (make-overlay 1 5))) (overlay-put o 'face 'bold))")
                .unwrap();
        }),
        ("media", |fix| {
            fix.eval.invalidate_media();
        }),
        ("topology", |fix| {
            let selected = fix
                .eval
                .frame_manager()
                .get(fix.frame)
                .unwrap()
                .selected_window;
            let buf = fix.eval.buffer_manager().current_buffer().unwrap().id();
            fix.eval
                .frame_manager_mut()
                .split_window(
                    fix.frame,
                    selected,
                    neovm_core::window::SplitDirection::Horizontal,
                    buf,
                    None,
                    neovm_core::window::SplitPlacement::AfterTarget,
                )
                .unwrap();
        }),
        ("placement", |fix| {
            fix.eval.eval_str("(set-window-start nil 25 t)").unwrap();
        }),
        ("display-var", |fix| {
            fix.eval.eval_str("(setq truncate-lines t)").unwrap();
        }),
    ];

    for (label, mutate) in cases {
        let mut fix = AuxFixture::new(None);
        fix.drain_worker_prefix(4);
        mutate(&mut fix);
        let prep = fix
            .engine
            .borrow_mut()
            .prepare_auxiliary_scroll_coverage(&fix.eval);
        assert!(
            matches!(prep, AuxiliaryPreparation::Refused(_)),
            "invalidator {label} must refuse auxiliary installation"
        );
    }

    // Fontset generation bump retires worker pages via dependencies_valid() and
    // must never install stale fontset rows.
    let mut fix = AuxFixture::new(None);
    fix.drain_worker_prefix(4);
    fix.eval
        .eval_str("(set-fontset-font t #x25cb '(nil . \"iso10646-1\"))")
        .unwrap();
    let prep = fix
        .engine
        .borrow_mut()
        .prepare_auxiliary_scroll_coverage(&fix.eval);
    assert!(
        matches!(
            prep,
            AuxiliaryPreparation::NoChange | AuxiliaryPreparation::Refused(_)
        ),
        "fontset generation bump must retire stale pages rather than install them"
    );
    assert!(
        fix.engine
            .borrow()
            .published_scroll_coverage(fix.frame, fix.owner)
            .is_none()
    );
}

#[test]
fn auxiliary_refuses_offscreen_new_face_in_v1() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    // Apply a face with a distinct background/height ONLY on offscreen lines
    // (lines 20..120), never visible in the initial base viewport.
    let line_len = "ordinary offscreen text\n".len();
    let offscreen_start = 45 * line_len + 1;
    let offscreen_end = 120 * line_len + 1;
    let mut fix = AuxFixture::new(Some(&format!(
        "(put-text-property {offscreen_start} {offscreen_end} 'face '(:foreground \"#ff0088\" :height 140))"
    )));
    // Directly request a worker page inside the offscreen region that introduces
    // the new face not present in the base frame's sealed FrameFaceMap.
    let target = neovm_core::buffer::CharPos0::new(
        fix.engine.borrow().retained_window_matrices[&fix.owner]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .rev()
            .nth(1)
            .unwrap()
            .start_charpos,
    );
    // Move the face start right below the initial viewport so the very first
    // connected worker page introduces the unseen face.
    let first_offscreen_lisp = target.get() + 2 * line_len + 1;
    fix.eval
        .eval_str(&format!(
            "(put-text-property {first_offscreen_lisp} {} 'face '(:foreground \"#ff0088\" :height 140))",
            first_offscreen_lisp + 20 * line_len
        ))
        .unwrap();
    // Re-accept the clean base frame (visible viewport still does NOT contain
    // the offscreen face, while buffer ticks match the base).
    fix.eval.redisplay().unwrap();
    fix.drain_worker_prefix(8);
    let prep = fix
        .engine
        .borrow_mut()
        .prepare_auxiliary_scroll_coverage(&fix.eval);
    assert!(
        matches!(
            prep,
            AuxiliaryPreparation::Refused(AuxiliaryPublicationRefusal::ResourceExtensionRequired(
                owner
            )) if owner == fix.owner
        ),
        "v1 must refuse an offscreen new face that is not a subset of the base frame resources"
    );
}

#[test]
fn own_late_chrome_base_refuses_auxiliary_and_withdraws_without_dropping_pages() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let line = "agent shell body line\n";
    let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(200), 800, 600);
    let other_buffer = eval.buffer_manager_mut().create_buffer("aux-sibling-clean");
    eval.buffer_manager_mut()
        .get_mut(other_buffer)
        .unwrap()
        .insert(&line.repeat(200));
    let sibling = eval
        .frame_manager_mut()
        .split_window(
            frame,
            window,
            neovm_core::window::SplitDirection::Vertical,
            other_buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    assert!(eval.frame_manager_mut().select_frame(frame));
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        r#"(progn
             (setq neo-aux-chrome-evals 0
                   neo-aux-chrome-overlay (make-overlay 1 6)
                   timer-list nil timer-idle-list nil
                   mode-line-format nil tab-line-format nil
                   header-line-format
                   '((:eval (progn
                              (setq neo-aux-chrome-evals (1+ neo-aux-chrome-evals))
                              (overlay-put neo-aux-chrome-overlay 'face
                                (if (= (% neo-aux-chrome-evals 2) 0) 'bold 'italic))
                              "Agent shell")))))"#,
    )
    .unwrap();
    let engine = Rc::new(RefCell::new(
        crate::font::metrics::test_catalog::layout_engine(),
    ));
    let prod_engine = Rc::clone(&engine);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        let mut eng = prod_engine.borrow_mut();
        eng.layout_frame_rust(eval, frame);
        activate_last_engine_presentation(eval, &eng, frame);
    }));
    eval.redisplay().expect("initial own-late-chrome redisplay");

    // Prepare a valid worker page on the unmodified sibling window; meanwhile
    // the selected window's own late-chrome header mutated its overlay tick
    // after body layout, making its original body freshness stale (#832).
    let owner = DisplayWindowId::new(window.0 as i64);
    let sibling_owner = DisplayWindowId::new(sibling.0 as i64);
    let target = {
        let eng = engine.borrow();
        let body: Vec<_> = eng.retained_window_matrices[&sibling_owner]
            .matrix
            .rows
            .iter()
            .filter(|r| r.enabled && r.role == GlyphRowRole::Text)
            .collect();
        neovm_core::buffer::CharPos0::new(body[body.len() - 2].start_charpos)
    };
    engine
        .borrow_mut()
        .request_scroll_coverage(&eval, frame, sibling, target)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let ready = {
            let mut eng = engine.borrow_mut();
            let eng = &mut *eng;
            eng.scroll_coverage
                .drain(&mut eng.prepared_viewports)
                .unwrap()
        };
        if ready {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        engine
            .borrow()
            .prepared_viewports
            .has_computed(frame, sibling_owner)
    );

    // Proof 3 (#832 original body evidence) must refuse the own-late-chrome
    // base on `owner` without dropping the sibling's valid prepared pages!
    let prep = engine.borrow_mut().prepare_auxiliary_scroll_coverage(&eval);
    assert!(
        matches!(
            prep,
            AuxiliaryPreparation::Refused(
                AuxiliaryPublicationRefusal::WindowFreshnessChanged(refused_owner)
            ) if refused_owner == owner
        ),
        "own-late-chrome base must refuse via exact original body freshness (#832)"
    );
    assert!(
        engine
            .borrow()
            .prepared_viewports
            .has_computed(frame, sibling_owner),
        "refusing own-late-chrome base must NOT drop prepared pages"
    );

    // Next real redisplay still sees those preserved pages and exports them
    // cleanly for the sibling window.
    eval.invalidate_redisplay();
    eval.redisplay().unwrap();
    assert!(
        engine
            .borrow()
            .published_scroll_coverage(frame, sibling_owner)
            .is_some(),
        "next real redisplay exports preserved sibling pages"
    );
    // Now that coverage IS published on the own-late-chrome base, a new
    // auxiliary publication attempt must deliver an unconditional withdrawal
    // while STILL keeping prepared pages intact!
    engine
        .borrow_mut()
        .scroll_coverage
        .note_publication_pending();
    let withdrawal = match engine.borrow_mut().prepare_auxiliary_scroll_coverage(&eval) {
        AuxiliaryPreparation::Ready(prepared) => prepared,
        AuxiliaryPreparation::NoChange => panic!("expected unconditional withdrawal, got NoChange"),
        AuxiliaryPreparation::Refused(r) => {
            panic!("expected withdrawal for published own-chrome-stale base, got Refused({r:?})")
        }
    };
    assert!(
        withdrawal.is_withdrawal_only(),
        "own-chrome-stale base withdrawal must carry zero surfaces"
    );
    assert_eq!(
        withdrawal.refusal_reason(),
        Some(AuxiliaryPublicationRefusal::WindowFreshnessChanged(owner))
    );
    engine
        .borrow_mut()
        .commit_auxiliary_scroll_coverage(withdrawal);
    assert!(
        engine
            .borrow()
            .published_scroll_coverage(frame, sibling_owner)
            .is_none(),
        "withdrawal must clear published coverage"
    );
    assert!(
        engine
            .borrow()
            .prepared_viewports
            .has_computed(frame, sibling_owner),
        "withdrawal on own-chrome-stale base must NOT drop prepared pages"
    );
    eval.redisplay_fn = None;
}

#[test]
fn tty_physical_redraw_does_not_refuse_gui_auxiliary_publication() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut fix = AuxFixture::new(None);
    // Real GUI startup leaves a TTY physical-redraw request on the initial
    // terminal frame that GUI renderers never drain via
    // `gnu_take_tty_frame_redraw`. That TTY-only state must not refuse native
    // auxiliary publication for the active GUI frame.
    let initial_tty_frame = neovm_core::window::FrameId(1);
    fix.eval.gnu_request_frame_redraw(initial_tty_frame);
    assert_eq!(
        fix.eval
            .auxiliary_redisplay_readiness(fix.eval.redisplay_generation()),
        Ok(())
    );
    fix.drain_worker_prefix(4);
    let prepared = match fix
        .engine
        .borrow_mut()
        .prepare_auxiliary_scroll_coverage(&fix.eval)
    {
        AuxiliaryPreparation::Ready(prepared) => prepared,
        AuxiliaryPreparation::NoChange => {
            panic!("expected Ready auxiliary publication, got NoChange")
        }
        AuxiliaryPreparation::Refused(refusal) => {
            panic!("TTY physical redraw must not refuse GUI auxiliary publication: {refusal:?}")
        }
    };
    assert!(
        !prepared.is_withdrawal_only() && !prepared.update().surfaces.is_empty(),
        "expected non-empty auxiliary scroll surface"
    );
    fix.engine
        .borrow_mut()
        .commit_auxiliary_scroll_coverage(prepared);
    assert!(
        fix.engine
            .borrow()
            .published_scroll_coverage(fix.frame, fix.owner)
            .is_some(),
        "GUI auxiliary publication must commit despite pending TTY physical redraw"
    );
}
