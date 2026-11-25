use super::*;
use neomacs_display_protocol::PresentedHitIndex;
use neovm_core::emacs_core::{Context, Value};
use std::rc::Rc;

/// Drive one window to a certified coverage export and return the publishing
/// state plus the exported epoch.
fn certified_coverage(
    eval: &mut Context,
    frame: neovm_core::window::FrameId,
    engine: &mut crate::LayoutEngine,
    window: neovm_core::window::WindowId,
    owner: DisplayWindowId,
) -> u64 {
    let current = engine.retained_window_matrices[&owner].clone();
    let body: Vec<_> = current
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let target = body[body.len() - 2].start_charpos;
    engine
        .request_scroll_coverage(eval, frame, window, CharPos0::new(target))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    engine.layout_frame_rust(eval, frame);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let coverage = state
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id == owner)
        .expect("fresh coverage export");
    assert_eq!(coverage.content.window_id, owner);
    coverage.epoch
}

#[test]
fn publish_without_fresh_surface_renews_the_certified_epoch() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let line = "ordinary offscreen text\n";
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&line.repeat(300));
    let frame = eval
        .frame_manager_mut()
        .create_frame("coverage-renewal", 800, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 1) (set-window-start nil 1 t)")
        .unwrap();
    let owner = DisplayWindowId::new(window.0 as i64);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let epoch = certified_coverage(&mut eval, frame, &mut engine, window, owner);

    // A publish that adds no prepared content (drained worker pages, or a
    // cursor-only redisplay between gestures) re-exports nothing. It must
    // renew the certified epoch instead of reading as revocation: point
    // motion does not move body rows.
    engine.prepared_viewports.entries.clear();
    eval.eval_str("(forward-char 1)").unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    assert!(
        state.scroll_coverage.is_empty(),
        "no prepared pages must not re-export a surface"
    );
    let renewal = state
        .scroll_coverage_renewals
        .iter()
        .find(|renewal| renewal.window == owner)
        .expect("cursor-only publish must renew the certified epoch");
    assert_eq!(renewal.epoch, epoch);
}

#[test]
fn content_revision_re_keys_and_stops_renewing_the_old_epoch() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let line = "ordinary offscreen text\n";
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&line.repeat(300));
    let frame = eval
        .frame_manager_mut()
        .create_frame("coverage-revocation", 800, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 1) (set-window-start nil 1 t)")
        .unwrap();
    let owner = DisplayWindowId::new(window.0 as i64);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let epoch = certified_coverage(&mut eval, frame, &mut engine, window, owner);

    // An edit invalidates the rows the certified epoch describes: the
    // bookkeeping re-keys, the new epoch is not yet certified, and the next
    // surface-less publish must stay silent so the compositor expires.
    engine.prepared_viewports.entries.clear();
    eval.eval_str("(goto-char 2) (insert \"edited\")").unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    assert!(
        state
            .scroll_coverage_renewals
            .iter()
            .all(|renewal| renewal.window != owner),
        "a re-keyed epoch must not be renewed"
    );
    assert!(
        engine
            .prepared_viewports
            .exports
            .iter()
            .any(|export| export.window == owner && export.epoch != epoch && !export.certified)
    );
}

#[test]
fn conflicting_worker_paragraph_does_not_revoke_other_prepared_rows() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let line = "ordinary offscreen text\n";
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&line.repeat(300));
    let frame = eval
        .frame_manager_mut()
        .create_frame("coverage-conflict", 800, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 1) (set-window-start nil 1 t)")
        .unwrap();
    let owner = DisplayWindowId::new(window.0 as i64);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let current = engine.retained_window_matrices[&owner].clone();
    let body: Vec<_> = current
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let target = body[body.len() - 2].start_charpos;
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(target))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage
            .len(),
        1
    );
    let worker = engine
        .prepared_viewports
        .entries
        .iter_mut()
        .find(|entry| entry.computed)
        .unwrap();
    let index = worker
        .retained
        .matrix
        .rows
        .iter()
        .position(|row| row.start_charpos == target)
        .unwrap();
    // Model the real conflict: the same source anchor has a different wrap
    // end in a worker captured from a different start within its paragraph.
    let mut conflicting = worker.retained.matrix.rows[index].as_ref().clone();
    conflicting.end_charpos -= 1;
    conflicting.continued = true;
    Rc::make_mut(&mut worker.payload).retained.matrix.rows[index] = MatrixRow::new(conflicting);
    engine.layout_frame_rust(&mut eval, frame);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let coverage = state
        .scroll_coverage
        .first()
        .expect("compatible paragraphs must remain available");
    let chosen = coverage
        .content
        .matrix
        .rows
        .iter()
        .find(|row| row.start_charpos == target)
        .unwrap();
    let expected = body.iter().find(|row| row.start_charpos == target).unwrap();
    assert_eq!(chosen.end_charpos, expected.end_charpos);
    assert_eq!(chosen.continued, expected.continued);
    assert!(
        coverage.content.matrix.rows.last().unwrap().start_charpos
            > body.last().unwrap().start_charpos
    );
}

const COVERAGE_REUSE_LINE: &str = "ordinary offscreen text\n";

struct CoverageReuseFixture {
    // Retire worker-owned roots before the evaluator that owns their heap.
    engine: crate::LayoutEngine,
    eval: Context,
    frame: neovm_core::window::FrameId,
    buffer: neovm_core::buffer::BufferId,
    window: neovm_core::window::WindowId,
}

impl CoverageReuseFixture {
    fn new() -> Self {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert(&COVERAGE_REUSE_LINE.repeat(300));
        let frame =
            eval.frame_manager_mut()
                .create_frame("coverage-product-reuse", 800, 600, buffer);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str("(setq truncate-lines t) (goto-char 8) (set-window-start nil 1 t)")
            .unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        Self {
            engine,
            eval,
            frame,
            buffer,
            window,
        }
    }

    fn layout(&mut self) {
        self.engine.layout_frame_rust(&mut self.eval, self.frame);
    }

    fn certify(
        &mut self,
        window: neovm_core::window::WindowId,
    ) -> Arc<neomacs_display_protocol::scroll_coverage::ScrollSurface> {
        certified_coverage(
            &mut self.eval,
            self.frame,
            &mut self.engine,
            window,
            DisplayWindowId::new(window.0 as i64),
        );
        let surface = self.surface(window);
        self.activate_latest();
        surface
    }

    fn activate_latest(&mut self) {
        let presentation = neovm_core::window::geometry::PresentationId::new(
            self.engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .presentation_id
                .get(),
        );
        self.eval
            .frame_manager_mut()
            .get_mut(self.frame)
            .unwrap()
            .activate_display_presentation(presentation)
            .expect("renderer confirms the full exported presentation");
    }

    fn surface(
        &self,
        window: neovm_core::window::WindowId,
    ) -> Arc<neomacs_display_protocol::scroll_coverage::ScrollSurface> {
        let state = self.engine.last_frame_display_state.as_ref().unwrap();
        state
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id == DisplayWindowId::new(window.0 as i64))
            .expect("this window published fresh coverage")
            .materialize(state)
            .expect("exported coverage passes real transport admission")
    }

    fn assert_renewed(&self, window: neovm_core::window::WindowId, epoch: u64) {
        let owner = DisplayWindowId::new(window.0 as i64);
        assert_eq!(
            self.engine.last_layout_stats().window_classes.get(&owner),
            Some(&crate::incremental_layout::LayoutClass::CursorOnly),
            "renewal requires this window's accepted body reuse"
        );
        assert_eq!(
            self.engine
                .last_layout_stats()
                .coverage_export_calls
                .get(&owner)
                .copied()
                .unwrap_or(0),
            0,
            "renewal must avoid the actual export constructor"
        );
        let state = self.engine.last_frame_display_state.as_ref().unwrap();
        assert!(
            state
                .scroll_coverage
                .iter()
                .all(|coverage| coverage.content.window_id != owner),
            "an unchanged product must not be rebuilt alongside its renewal"
        );
        assert!(
            state
                .scroll_coverage_renewals
                .iter()
                .any(|renewal| { renewal.window == owner && renewal.epoch == epoch })
        );
    }

    fn assert_old_product_not_renewed(&self, window: neovm_core::window::WindowId, epoch: u64) {
        let owner = DisplayWindowId::new(window.0 as i64);
        assert_ne!(
            *self
                .engine
                .last_layout_stats()
                .window_classes
                .get(&owner)
                .expect("the changed window has an accepted layout class"),
            crate::incremental_layout::LayoutClass::CursorOnly,
            "changed body/placement must not borrow a sibling's CursorOnly class"
        );
        assert!(
            self.engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .scroll_coverage_renewals
                .iter()
                .all(|renewal| renewal.window != owner),
            "changed source or placement must expire product {epoch} without any renewal"
        );
        assert_eq!(
            self.engine
                .last_layout_stats()
                .coverage_export_calls
                .get(&owner),
            Some(&1),
            "the immediate post-mutation frame must attempt the actual exporter"
        );
    }

    fn assert_hits_match_forced_export(
        &mut self,
        window: neovm_core::window::WindowId,
        retained: &neomacs_display_protocol::scroll_coverage::ScrollSurface,
    ) {
        // Keep the accepted layout and eligible prepared rows. Removing only
        // export products forces the actual exporter to rebuild its geometry.
        self.engine.prepared_viewports.exports.clear();
        self.layout();
        let fresh = self.surface(window);
        assert_surface_hits_equal(retained, &fresh);
    }

    fn assert_visible_hits_match_forced_export(&mut self, window: neovm_core::window::WindowId) {
        let owner = DisplayWindowId::new(window.0 as i64);
        let bounds = self.engine.retained_window_matrices[&owner]
            .display_snapshot
            .regions
            .text_body;
        let accepted = self
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .presented_hit_index
            .clone();
        self.engine.prepared_viewports.exports.clear();
        self.layout();
        let state = self.engine.last_frame_display_state.as_ref().unwrap();
        assert!(
            state
                .scroll_coverage
                .iter()
                .all(|coverage| coverage.content.window_id != owner)
        );
        let query = |index: &PresentedHitIndex, x, y| {
            let point =
                neomacs_display_protocol::InteractionProjection::settled(index.presentation())
                    .map(
                        neomacs_display_protocol::GeometryPoint::<
                            neomacs_display_protocol::RootSurfaceSpace,
                            neomacs_display_protocol::LogicalPixels,
                        >::from_px(x, y)
                        .unwrap(),
                    )
                    .unwrap();
            index
                .resolve(neomacs_display_protocol::PresentedHitQuery::new(point))
                .unwrap()
        };
        let mut source_hits = 0;
        for y in (0..bounds.height.ceil() as usize).step_by(3) {
            for x in [0.5, 7.5, 15.5, bounds.width / 2.0, bounds.width - 0.5] {
                let x = bounds.x + x;
                let y = bounds.y + (y as f32 + 0.5).min((bounds.height - 0.5).max(0.0));
                let actual = query(&accepted, x, y);
                let expected = query(&state.presented_hit_index, x, y);
                source_hits += usize::from(expected.and_then(|hit| hit.text_position()).is_some());
                assert_eq!(actual, expected, "visible x={x}, y={y}");
            }
        }
        assert!(
            source_hits > 0,
            "visible comparison must exercise actual source positions"
        );
    }

    fn assert_mutation_rebuilds(&mut self, mutation: &str) {
        let window = self.window;
        let old = self.certify(window);
        let old_epoch = old.coverage().epoch;
        self.eval.eval_str(mutation).unwrap();
        self.layout();
        self.assert_old_product_not_renewed(window, old_epoch);

        // The mutation invalidates old worker pages too. Prepare compatible
        // rows before demanding a replacement surface; invalid rows confer no
        // authority to export merely because the previous epoch was certified.
        let replacement = self.certify(window);
        assert_ne!(replacement.coverage().epoch, old_epoch);
        self.assert_hits_match_forced_export(window, &replacement);
    }

    fn assert_receipt_only_frame_change(&mut self, change: impl FnOnce(&mut FrameDisplayState)) {
        let window = self.window;
        let owner = DisplayWindowId::new(window.0 as i64);
        let activated = self.certify(window);
        self.eval.eval_str("(forward-char 1)").unwrap();
        self.layout();
        self.assert_renewed(window, activated.coverage().epoch);
        assert!(
            self.engine
                .prepared_viewports
                .exports
                .iter()
                .find(|export| export.window == owner)
                .unwrap()
                .last_export
                .is_some()
        );

        let retained = self.engine.retained_window_matrices.clone();
        let key = retained[&owner].key.clone();
        let pages: Vec<_> = self
            .engine
            .prepared_viewports
            .entries
            .iter()
            .filter(|page| page.window == owner && page.computed)
            .map(|page| {
                assert!(page.dependencies_valid());
                assert!(RetainedWindowKey::row_content_eligible(
                    &page.retained.key,
                    &key
                ));
                Rc::clone(&page.payload)
            })
            .collect();
        assert!(!pages.is_empty());
        let mut state = self
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .clone()
            .into_state();
        state.scroll_coverage.clear();
        state.scroll_coverage_renewals.clear();
        let mut stats = self.engine.last_layout_stats().clone();
        stats.coverage_export_calls.clear();
        // Mutate only exporter receipt inputs. The accepted per-window class,
        // complete layout key and eligible prepared payloads stay unchanged.
        change(&mut state);
        let mut fresh_state = state.clone();
        let active = Some(activated.coverage().hit_index.presentation());
        self.engine.prepared_viewports.export(
            self.frame,
            &retained,
            &mut stats,
            active,
            &mut state,
            &self.engine.frame_face_arenas[&self.frame],
            &mut self.engine.font_metrics,
            |_| (false, false),
        );
        assert_eq!(
            stats.window_classes.get(&owner),
            Some(&crate::incremental_layout::LayoutClass::CursorOnly)
        );
        assert_eq!(stats.coverage_export_calls.get(&owner), Some(&1));
        assert!(
            state
                .scroll_coverage_renewals
                .iter()
                .all(|renewal| renewal.window != owner)
        );
        let rebuilt = state
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id == owner)
            .expect("a receipt-only invalidator must reconstruct the eligible surface")
            .materialize(&state)
            .expect("receipt-only invalidation still has valid source and resources");
        let current_pages: Vec<_> = self
            .engine
            .prepared_viewports
            .entries
            .iter()
            .filter(|page| page.window == owner && page.computed)
            .collect();
        assert_eq!(pages.len(), current_pages.len());
        assert!(
            pages
                .iter()
                .zip(current_pages)
                .all(|(old, page)| Rc::ptr_eq(old, &page.payload))
        );
        assert_eq!(self.engine.retained_window_matrices[&owner].key, key);

        self.engine.prepared_viewports.exports.clear();
        stats.coverage_export_calls.clear();
        self.engine.prepared_viewports.export(
            self.frame,
            &retained,
            &mut stats,
            active,
            &mut fresh_state,
            &self.engine.frame_face_arenas[&self.frame],
            &mut self.engine.font_metrics,
            |_| (false, false),
        );
        assert_eq!(stats.coverage_export_calls.get(&owner), Some(&1));
        let fresh = fresh_state
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id == owner)
            .expect("forced fresh exporter constructs the same changed-frame surface")
            .materialize(&fresh_state)
            .unwrap();
        assert_surface_hits_equal(&rebuilt, &fresh);
    }
}

fn coverage_point(
    surface: &neomacs_display_protocol::scroll_coverage::ScrollSurface,
    x: f32,
    y: f32,
) -> neomacs_display_protocol::PresentationFramePoint {
    neomacs_display_protocol::InteractionProjection::settled(
        surface.coverage().hit_index.presentation(),
    )
    .map(
        neomacs_display_protocol::GeometryPoint::<
            neomacs_display_protocol::RootSurfaceSpace,
            neomacs_display_protocol::LogicalPixels,
        >::from_px(x, y)
        .unwrap(),
    )
    .unwrap()
}

fn assert_surface_hits_equal(
    retained: &neomacs_display_protocol::scroll_coverage::ScrollSurface,
    fresh: &neomacs_display_protocol::scroll_coverage::ScrollSurface,
) {
    let viewport = retained.coverage().viewport;
    assert_eq!(viewport, fresh.coverage().viewport);
    assert_eq!(retained.coverage_glyphs(), fresh.coverage_glyphs());
    assert_eq!(retained.coverage().faces, fresh.coverage().faces);
    assert_eq!(retained.coverage().fonts, fresh.coverage().fonts);
    assert_eq!(retained.coverage().char_fonts, fresh.coverage().char_fonts);
    assert_eq!(
        retained.coverage().shaped_clusters,
        fresh.coverage().shaped_clusters
    );
    assert_eq!(
        retained.coverage().pointer_source,
        fresh.coverage().pointer_source,
        "coverage must retain the fresh source's mouse-face ownership too"
    );
    let mut source_hits = 0;
    // Include glyph interiors, inter-glyph edges, trailing/newline fallback,
    // clipping boundaries, fractional motion and the far prepared frontier.
    let xs = [
        -0.5,
        0.5,
        7.5,
        15.5,
        viewport.width / 2.0,
        viewport.width - 0.5,
        viewport.width + 0.5,
    ];
    let ys = std::iter::once(-0.5)
        .chain(
            (0..viewport.height.ceil() as usize)
                .step_by(3)
                .map(|y| y as f32 + 0.5),
        )
        .chain([viewport.height - 0.5, viewport.height + 0.5]);
    let offsets = [0.0, 0.5, retained.clamp_offset(f32::MAX)];
    for y in ys {
        for x in xs {
            for offset in offsets {
                assert_eq!(retained.clamp_offset(offset), fresh.clamp_offset(offset));
                let x = viewport.x + x;
                let y = viewport.y + y;
                let actual = retained
                    .hit(coverage_point(retained, x, y), offset)
                    .unwrap();
                let expected = fresh.hit(coverage_point(fresh, x, y), offset).unwrap();
                source_hits += usize::from(expected.and_then(|hit| hit.text_position()).is_some());
                assert_eq!(actual, expected, "x={x}, y={y}, offset={offset}");
            }
        }
    }
    assert!(
        source_hits > 0,
        "comparison must exercise source hit positions"
    );
}

#[test]
fn coverage_export_keeps_hit_geometry_deferred_through_admission_and_hits() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    certified_coverage(
        &mut fixture.eval,
        fixture.frame,
        &mut fixture.engine,
        window,
        owner,
    );
    assert_eq!(
        fixture
            .engine
            .last_layout_stats()
            .coverage_export_calls
            .get(&owner),
        Some(&1),
        "the cost assertion must observe the actual coverage exporter"
    );
    let state = fixture.engine.last_frame_display_state.as_ref().unwrap();
    let coverage = state
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id == owner)
        .expect("the real prepared-row producer must export coverage");
    // This observes whether the full position vector was constructed, not an
    // estimate of allocations from the number of rows or source characters.
    assert!(
        coverage.hit_index.text_deferred(),
        "exporting row-backed coverage must not flatten all source positions"
    );
    let surface = fixture.surface(window);
    assert!(
        coverage.hit_index.text_deferred(),
        "real transport admission must validate coverage without flattening its hit geometry"
    );
    assert!(surface.coverage().hit_index.text_deferred());

    // Force the legacy flat index on an independent clone. Cloning preserves
    // the immutable source but gives its full-vector cache a separate owner,
    // so the reference cannot silently materialize the measured index.
    let mut eager_coverage = (**coverage).clone();
    eager_coverage.hit_index = PresentedHitIndex::from_parts(
        eager_coverage.hit_index.presentation(),
        eager_coverage.hit_index.regions().to_vec(),
        eager_coverage.hit_index.text_positions().to_vec(),
    )
    .expect("the same coverage geometry must admit the legacy flat index");
    assert!(!eager_coverage.hit_index.text_deferred());
    assert!(coverage.hit_index.text_deferred());
    let eager_surface = Arc::new(eager_coverage)
        .materialize(state)
        .expect("the legacy flat reference must pass real transport admission");
    assert_surface_hits_equal(&surface, &eager_surface);
    assert!(
        surface.coverage().hit_index.text_deferred(),
        "direct row hits must preserve the deferred full position vector"
    );
}

#[test]
fn cursor_motion_renews_export_with_eligible_prepared_rows_and_exact_hits() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let surface = fixture.certify(window);
    assert!(
        fixture
            .engine
            .prepared_viewports
            .entries
            .iter()
            .any(|entry| {
                entry.window == DisplayWindowId::new(window.0 as i64) && entry.computed
            })
    );
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    fixture.assert_renewed(window, surface.coverage().epoch);
    fixture.assert_hits_match_forced_export(window, &surface);
}

#[test]
fn unactivated_coverage_repeats_full_export_before_renderer_confirmed_renewal() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    let epoch = certified_coverage(
        &mut fixture.eval,
        fixture.frame,
        &mut fixture.engine,
        window,
        owner,
    );
    let initial = fixture.surface(window);
    // Materializing is not acknowledgement: the full presentation could be
    // coalesced away before the renderer activates it.
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    let repeated = fixture.surface(window);
    assert_eq!(
        fixture
            .engine
            .last_layout_stats()
            .coverage_export_calls
            .get(&owner),
        Some(&1)
    );
    assert_eq!(repeated.coverage().epoch, epoch);
    assert_eq!(
        initial.coverage().publication,
        repeated.coverage().publication,
        "identical activation-family retries keep the held surface's coordinate authority"
    );
    assert!(
        fixture
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage_renewals
            .iter()
            .all(|renewal| renewal.window != owner),
        "unactivated full coverage must not be replaced with a renewal"
    );
    assert_surface_hits_equal(&initial, &repeated);

    // A delayed ACK for the earlier equivalent full publish must also arm
    // renewal; requiring only the latest ID can keep chasing a busy renderer.
    fixture
        .eval
        .frame_manager_mut()
        .get_mut(fixture.frame)
        .unwrap()
        .activate_display_presentation(neovm_core::window::geometry::PresentationId::new(
            initial.coverage().hit_index.presentation().get(),
        ))
        .expect("renderer acquired the earlier equivalent full surface");
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    fixture.assert_renewed(window, epoch);
    fixture.assert_hits_match_forced_export(window, &repeated);
}

#[test]
fn coalesced_same_input_reexport_keeps_resolved_preview_on_the_held_publication() {
    let mut fixture = CoverageReuseFixture::new();
    fixture
        .eval
        .eval_str("(setq neomacs-compositor-scrolling t)")
        .unwrap();
    fixture.layout();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    let epoch = certified_coverage(
        &mut fixture.eval,
        fixture.frame,
        &mut fixture.engine,
        window,
        owner,
    );
    // The renderer acquires I, but its activation is delayed until after R has
    // been exported. R can be coalesced away while the evaluator mirrors it.
    let held = fixture.surface(window);
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    let repeated = fixture.surface(window);
    assert_eq!(held.coverage().epoch, repeated.coverage().epoch);
    assert_eq!(held.coverage().origin, repeated.coverage().origin);
    assert_eq!(held.coverage().anchor_row, repeated.coverage().anchor_row);
    assert_eq!(
        held.coverage().content.text_clip_bounds,
        repeated.coverage().content.text_clip_bounds
    );
    assert_surface_hits_equal(&held, &repeated);
    fixture
        .eval
        .frame_manager_mut()
        .get_mut(fixture.frame)
        .unwrap()
        .activate_display_presentation(neovm_core::window::geometry::PresentationId::new(
            held.coverage().hit_index.presentation().get(),
        ))
        .expect("renderer acquired I; equivalent R was coalesced away");
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    fixture.assert_renewed(window, epoch);
    fixture.activate_latest();

    // The renewal carries no surface. The renderer still owns I, whereas the
    // evaluator's latest accepted coverage mirror was refreshed by R.
    let destination = COVERAGE_REUSE_LINE.len() + 1;
    fixture
        .eval
        .eval_str(&format!(
            "(goto-char {destination}) (set-window-start nil {destination} t)",
        ))
        .unwrap();
    let stream = neomacs_display_protocol::input_progress::InputStream::new(
        neomacs_display_protocol::input_progress::InputStreamId::FIRST,
    );
    let delivery = stream.issue().unwrap();
    let intent = fixture
        .engine
        .resolved_scroll_preview(
            &fixture.eval,
            fixture.frame,
            window,
            vec![delivery.receipt()],
        )
        .expect("the renewed rows must resolve a real committed scroll destination");
    assert_eq!(intent.epoch, held.coverage().epoch);
    assert!(
        intent.offset > 0.0,
        "exercise a real scroll destination, not an idle zero offset"
    );
    assert_eq!(
        intent.publication,
        held.coverage().publication,
        "same-input re-export must keep preview authority usable after earlier activation and renewal"
    );
}

#[test]
fn unactivated_reexport_with_a_changed_origin_gets_new_coordinate_authority() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    certified_coverage(
        &mut fixture.eval,
        fixture.frame,
        &mut fixture.engine,
        window,
        owner,
    );
    let initial = fixture.surface(window);
    if let neovm_core::window::Window::Leaf {
        vscroll,
        force_start,
        ..
    } = fixture
        .eval
        .frame_manager_mut()
        .get_mut(fixture.frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *vscroll = -4;
        *force_start = true;
    }
    fixture.layout();
    let replacement = fixture.surface(window);
    assert_eq!(initial.coverage().epoch, replacement.coverage().epoch);
    assert_ne!(
        initial.coverage().origin,
        replacement.coverage().origin,
        "the control must actually change the offset coordinate system"
    );
    assert_ne!(
        initial.coverage().publication,
        replacement.coverage().publication,
        "same lease cannot reuse offset authority after an origin change"
    );
}

#[test]
fn replacing_an_eligible_prepared_payload_forces_full_coverage_export() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    let old = fixture.certify(window);
    let worker = fixture
        .engine
        .prepared_viewports
        .entries
        .iter_mut()
        .find(|entry| entry.window == owner && entry.computed)
        .expect("a real worker page contributes to the certified product");
    let original_payload = Rc::clone(&worker.payload);
    worker.payload = Rc::new((*original_payload).clone());
    assert!(!Rc::ptr_eq(&worker.payload, &original_payload));

    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    assert_eq!(
        fixture
            .engine
            .last_layout_stats()
            .window_classes
            .get(&owner),
        Some(&crate::incremental_layout::LayoutClass::CursorOnly)
    );
    let replacement = fixture.surface(window);
    assert_eq!(
        fixture
            .engine
            .last_layout_stats()
            .coverage_export_calls
            .get(&owner),
        Some(&1)
    );
    assert!(
        fixture
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage_renewals
            .iter()
            .all(|renewal| renewal.window != owner),
        "row identity replacement must invalidate the derived-product receipt"
    );
    assert_surface_hits_equal(&old, &replacement);
    fixture.assert_hits_match_forced_export(window, &replacement);
}

#[test]
fn rejected_coverage_retries_full_export_until_admissible_presentation_is_activated() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    fixture.certify(window);
    let last_visible = fixture.engine.retained_window_matrices[&owner]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .map(|row| row.start_charpos)
        .max()
        .unwrap();
    let (page_index, row_index) = fixture
        .engine
        .prepared_viewports
        .entries
        .iter()
        .enumerate()
        .filter(|(_, page)| page.window == owner && page.computed)
        .find_map(|(page_index, page)| {
            page.retained
                .matrix
                .rows
                .iter()
                .position(|row| {
                    row.enabled
                        && row.role == GlyphRowRole::Text
                        && row.start_charpos > last_visible
                })
                .map(|row_index| (page_index, row_index))
        })
        .expect("the real computed coverage includes an offscreen text row");
    let page = &mut fixture.engine.prepared_viewports.entries[page_index];
    let original_payload = Rc::clone(&page.payload);
    let mut broken = page.retained.matrix.rows[row_index].as_ref().clone();
    let face_id = broken.referenced_face_ids().next().unwrap();
    let missing_bitmap = u16::MAX;
    // Face admission still succeeds; only the offscreen product references a
    // missing bitmap. The ordinary visible frame remains valid/activatable.
    broken.left_fringe_bitmap = Some(neomacs_display_protocol::glyph_matrix::FringeBitmapInfo {
        bitmap_index: missing_bitmap,
        face_id,
    });
    Rc::make_mut(&mut page.payload).retained.matrix.rows[row_index] = MatrixRow::new(broken);
    let rejected_payload = Rc::clone(&page.payload);

    for _ in 0..2 {
        fixture.eval.eval_str("(forward-char 1)").unwrap();
        fixture.layout();
        let state = fixture.engine.last_frame_display_state.as_ref().unwrap();
        let attempted = state
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id == owner)
            .expect("rejected coverage must keep attempting a full export");
        assert_eq!(
            fixture
                .engine
                .last_layout_stats()
                .window_classes
                .get(&owner),
            Some(&crate::incremental_layout::LayoutClass::CursorOnly)
        );
        assert_eq!(
            fixture
                .engine
                .last_layout_stats()
                .coverage_export_calls
                .get(&owner),
            Some(&1)
        );
        assert!(attempted.content.matrix.rows.iter().any(|row| {
            row.left_fringe_bitmap
                .is_some_and(|bitmap| bitmap.bitmap_index == missing_bitmap)
        }));
        assert!(!state.fringe_bitmaps.contains_key(&missing_bitmap));
        assert!(
            attempted.materialize(state).is_none(),
            "missing fringe resource must fail actual ScrollCoverage admission"
        );
        assert!(
            state
                .scroll_coverage_renewals
                .iter()
                .all(|renewal| renewal.window != owner)
        );
        assert!(
            fixture
                .engine
                .prepared_viewports
                .exports
                .iter()
                .find(|export| export.window == owner)
                .unwrap()
                .last_export
                .is_none(),
            "a rejected product must not leave a reusable receipt"
        );
        fixture.activate_latest();
    }

    let page = fixture
        .engine
        .prepared_viewports
        .entries
        .iter_mut()
        .find(|page| Rc::ptr_eq(&page.payload, &rejected_payload))
        .expect("the rejected page remains available for resource recovery");
    page.payload = original_payload;
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    let recovered = fixture.surface(window);
    assert!(
        fixture
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage_renewals
            .iter()
            .all(|renewal| renewal.window != owner)
    );
    fixture.activate_latest();
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    fixture.assert_renewed(window, recovered.coverage().epoch);
    fixture.assert_hits_match_forced_export(window, &recovered);
}

#[test]
fn horizontal_scroll_rebuilds_coverage_instead_of_renewing_old_hit_geometry() {
    let mut fixture = CoverageReuseFixture::new();
    let window = fixture.window;
    let owner = DisplayWindowId::new(window.0 as i64);
    let old = fixture.certify(window);
    fixture.eval.eval_str("(set-window-hscroll nil 2)").unwrap();
    fixture.layout();
    fixture.assert_old_product_not_renewed(window, old.coverage().epoch);
    assert_eq!(
        fixture.engine.retained_window_matrices[&owner].key.hscroll,
        2
    );
    // begin_scroll_coverage rejects nonzero hscroll (scroll_coverage.rs:879-889).
    // Ordinary synchronous hit geometry remains authoritative in that domain;
    // demanding a new worker page here tests an unsupported capability.
    assert!(
        fixture
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage
            .iter()
            .all(|coverage| coverage.content.window_id != owner)
    );
    fixture.assert_visible_hits_match_forced_export(window);

    fixture.eval.eval_str("(set-window-hscroll nil 0)").unwrap();
    fixture.layout();
    let recovered = fixture.certify(window);
    assert_ne!(recovered.coverage().epoch, old.coverage().epoch);
    fixture.eval.eval_str("(forward-char 1)").unwrap();
    fixture.layout();
    fixture.assert_renewed(window, recovered.coverage().epoch);
    fixture.assert_hits_match_forced_export(window, &recovered);
}

#[test]
fn changed_frame_cell_size_rebuilds_coverage_with_unchanged_layout_key_and_pages() {
    let mut fixture = CoverageReuseFixture::new();
    fixture.assert_receipt_only_frame_change(|state| state.char_width += 1.0);
}

#[test]
fn changed_window_placement_rebuilds_coverage_with_unchanged_layout_key_and_pages() {
    let mut fixture = CoverageReuseFixture::new();
    let owner = DisplayWindowId::new(fixture.window.0 as i64);
    fixture.assert_receipt_only_frame_change(|state| {
        state
            .window_matrices
            .iter_mut()
            .find(|entry| entry.window_id == owner)
            .unwrap()
            .pixel_bounds
            .x += 1.0;
    });
}

#[test]
fn text_edit_rebuilds_coverage_instead_of_renewing_old_source_positions() {
    let mut fixture = CoverageReuseFixture::new();
    fixture.assert_mutation_rebuilds("(goto-char 2) (insert \"edited\")");
}

#[test]
fn coverage_renewal_uses_each_same_buffer_windows_own_layout_class() {
    let mut fixture = CoverageReuseFixture::new();
    let selected = fixture.window;
    let other = fixture
        .eval
        .frame_manager_mut()
        .split_window(
            fixture.frame,
            selected,
            // Full-width stacked panes keep the truncating worker fixture
            // independent of reduced-width row-overflow behavior.
            neovm_core::window::SplitDirection::Vertical,
            fixture.buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    fixture.layout();
    let selected_surface = fixture.certify(selected);
    let other_surface = fixture.certify(other);
    let other_owner = DisplayWindowId::new(other.0 as i64);
    let old_start = fixture.engine.retained_window_matrices[&other_owner]
        .key
        .window_start;
    fixture
        .eval
        .set_variable("coverage-other-window", Value::make_window(other.0));
    // ASCII fixture: advance past the complete physical line, including its
    // newline, then convert the byte/character count to a Lisp position.
    let second_line = COVERAGE_REUSE_LINE.len() + 1;
    fixture
        .eval
        .eval_str(&format!(
            "(set-window-point coverage-other-window {second_line})
             (set-window-start coverage-other-window {second_line} t)"
        ))
        .unwrap();
    fixture.layout();
    fixture.assert_renewed(selected, selected_surface.coverage().epoch);
    fixture.assert_old_product_not_renewed(other, other_surface.coverage().epoch);
    assert_ne!(
        fixture.engine.retained_window_matrices[&other_owner]
            .key
            .window_start,
        old_start
    );
    // A changed start retains compatible prepared rows and their content
    // epoch, but must reconstruct this window's placement/source hit product.
    let changed = fixture.surface(other);
    assert!(!Arc::ptr_eq(&changed, &other_surface));
    fixture.assert_hits_match_forced_export(selected, &selected_surface);
    fixture.assert_hits_match_forced_export(other, &changed);
}

#[test]
fn mouse_face_overlay_change_rebuilds_certified_coverage() {
    let mut fixture = CoverageReuseFixture::new();
    fixture.assert_mutation_rebuilds(
        "(let ((overlay (make-overlay 1 12))) (overlay-put overlay 'mouse-face 'highlight))",
    );
}

#[test]
fn moving_show_paren_face_overlay_rebuilds_certified_coverage() {
    let mut fixture = CoverageReuseFixture::new();
    fixture
        .eval
        .eval_str(
            "(setq coverage-show-paren (make-overlay 2 5))
             (overlay-put coverage-show-paren 'priority 1000)
             (overlay-put coverage-show-paren 'face '(:background \"#123456\"))",
        )
        .unwrap();
    fixture.layout();
    fixture.assert_mutation_rebuilds("(move-overlay coverage-show-paren 6 10)");
}
