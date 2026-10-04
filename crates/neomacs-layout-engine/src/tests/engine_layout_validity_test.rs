//! Freshness must agree with the geometry produced by a full layout.
use super::*;

/// A single cached file whose replacement has a different decoded extent.
/// Only explicit cache invalidation exposes the replacement to layout.
#[derive(Clone, Default)]
struct ReloadingImageHost(std::rc::Rc<std::cell::Cell<bool>>);

impl DisplayHost for ReloadingImageHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(self)
    }

    fn image_catalog_shared(&self) -> Option<std::rc::Rc<dyn ImageCatalog>> {
        Some(std::rc::Rc::new(self.clone()))
    }
}

impl ImageCatalog for ReloadingImageHost {
    fn lookup(&self, _: ImageResolveRequest, _limit: ImageSizeLimit) -> ImageLookup {
        ImageLookup::Ready(ReadyImage {
            load: test_image_load(if self.0.get() { 78 } else { 77 }),
            metadata:
                neovm_core::emacs_core::image_catalog::ResolvedImageMetadata::layout_is_image_pixels(
                    if self.0.get() { 60 } else { 20 },
                    24,
                    0,
                    false,
                    Default::default(),
                ),
        })
    }

    fn invalidate(
        &self,
        _: neovm_core::emacs_core::image_catalog::ImageInvalidation,
    ) -> neovm_core::emacs_core::image_catalog::ImageInvalidationResult {
        use neovm_core::emacs_core::image_catalog::ImageInvalidationResult;
        if self.0.replace(true) {
            ImageInvalidationResult::Unchanged
        } else {
            ImageInvalidationResult::Changed
        }
    }
}

/// Host boundary fixture: pending images reserve their explicitly requested
/// extent. Pixel arithmetic and ordinary layout receive the same catalog.
struct ExplicitExtentImageHost;

impl DisplayHost for ExplicitExtentImageHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(self)
    }

    fn image_catalog_shared(&self) -> Option<std::rc::Rc<dyn ImageCatalog>> {
        Some(std::rc::Rc::new(Self))
    }
}

impl ImageCatalog for ExplicitExtentImageHost {
    fn lookup(&self, request: ImageResolveRequest, _limit: ImageSizeLimit) -> ImageLookup {
        let (width, height) = request.size.placeholder_extent().expect("explicit extent");
        ImageLookup::Pending(PendingImage::new(
            test_image_load(77),
            neomacs_display_protocol::ImageLayoutExtent::new(width, height),
        ))
    }
}

fn assert_prefix_mutation_invalidates_geometry(variable: &str, mutation: &str) {
    assert_prefix_spec_mutation_invalidates_geometry(variable, r#"(copy-sequence "  ")"#, mutation);
}

fn assert_prefix_spec_mutation_invalidates_geometry(variable: &str, initial: &str, mutation: &str) {
    assert_layout_mutation_invalidates_geometry(&format!("(setq {variable} {initial})"), mutation);
}

fn assert_layout_mutation_invalidates_geometry(setup: &str, mutation: &str) {
    assert_layout_mutation_with_measurement(setup, mutation, false);
}

fn assert_layout_mutation_with_measurement(setup: &str, mutation: &str, graphical: bool) {
    assert_layout_mutation_with_host(setup, mutation, graphical, None);
}

fn assert_layout_mutation_with_host(
    setup: &str,
    mutation: &str,
    graphical: bool,
    host: Option<Box<dyn DisplayHost>>,
) {
    let mut eval = Context::new();
    if let Some(host) = host {
        eval.set_display_host(host);
    }
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"abcdefghij".repeat(20));
    let frame = eval
        .frame_manager_mut()
        .create_frame("prefix-freshness", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    if graphical {
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
    }
    eval.eval_str(&format!(r#"(progn (goto-char 1) {setup})"#))
        .unwrap();
    let mut engine = if graphical {
        LayoutEngine::new()
    } else {
        LayoutEngine::new_without_font_metrics()
    };
    engine.layout_frame_rust(&mut eval, frame);
    let original = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .unwrap()
        .iter_points()
        .collect::<Vec<_>>();

    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        engine.last_layout_stats().reused_rows > 0,
        "unchanged inputs retain row reuse"
    );
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str(mutation).unwrap();
    let after = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    let stale_accepted = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .is_some();
    let mut query_engine = if graphical {
        WindowLayoutQueryEngine::new()
    } else {
        WindowLayoutQueryEngine::new_without_font_metrics()
    };
    let measured = query_engine
        .query_window_layout(
            &mut eval,
            frame,
            window,
            neovm_core::window::WindowLayoutQueryScope::Viewport,
        )
        .unwrap()
        .into_geometry()
        .unwrap()
        .iter_points()
        .collect::<Vec<_>>();
    engine.layout_frame_rust(&mut eval, frame);
    let full_windows = engine.last_layout_stats().full_windows;
    let incremental = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .unwrap()
        .iter_points()
        .collect::<Vec<_>>();
    let mut full = if graphical {
        LayoutEngine::new()
    } else {
        LayoutEngine::new_without_font_metrics()
    };
    full.layout_frame_rust(&mut eval, frame);
    let reference = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .unwrap()
        .iter_points()
        .collect::<Vec<_>>();
    assert_ne!(
        &original, &reference,
        "the mutation must change actual layout geometry"
    );
    assert_eq!(
        &incremental, &reference,
        "retained engine agrees with a fresh full layout"
    );
    assert_ne!(
        before, after,
        "in-flight attempts detect the same input mutation"
    );
    assert!(full_windows > 0, "input changes invalidate row reuse");
    assert_eq!(
        &measured, &reference,
        "synchronous query agrees with redisplay"
    );
    assert!(
        !stale_accepted,
        "geometry from before a layout input change must not be reported fresh"
    );
}

fn assert_decoration_mutation_invalidates_presentation(setup: &str, mutation: &str) {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("abc");
    let frame = eval
        .frame_manager_mut()
        .create_frame("box-inputs", 160, 160, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(&format!(
        r##"(progn (goto-char 1) (setq line-prefix (copy-sequence "xx")) {setup})"##
    ))
    .unwrap();
    let render = |engine: &mut LayoutEngine, eval: &mut Context| {
        let FrameLayoutAttempt::Prepared(state) = engine.redisplay_frame_attempt(eval, frame)
        else {
            panic!("presentation aborted")
        };
        let glyph = state
            .window_matrices
            .iter()
            .flat_map(|window| &window.matrix.rows)
            .flat_map(|row| &row.glyphs[GlyphArea::Text.index()])
            .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: 'x' }))
            .expect("prefix glyph");
        let face = &state.faces[&glyph.face_id];
        (
            face.box_line_width,
            face.box_color,
            face.underline_style,
            face.underline_placement,
            face.underline_color,
            face.stipple.clone(),
        )
    };
    let mut engine = LayoutEngine::new();
    let before = render(&mut engine, &mut eval);
    assert_eq!(before, render(&mut engine, &mut eval));
    assert!(engine.last_layout_stats().reused_rows > 0);
    eval.eval_str(mutation).unwrap();
    let incremental = render(&mut engine, &mut eval);
    let reference = render(&mut LayoutEngine::new(), &mut eval);
    assert_ne!(
        before, reference,
        "mutation must change the rendered decoration"
    );
    assert_eq!(
        incremental, reference,
        "retained decoration agrees with fresh layout"
    );
}

#[test]
fn retained_geometry_tracks_explicit_prefix_stretch_height() {
    assert_layout_mutation_with_measurement(
        "(setq prefix-height (list 24) line-prefix (list 'space :width 1 :height prefix-height))",
        "(setcar prefix-height 60)",
        true,
    );
}

#[test]
fn tall_stretch_prefix_reserves_vertical_space_before_next_line() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("a\nb\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("tall-prefix", 160, 300, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq line-prefix '(space :width 1 :height (60) :ascent 80))")
        .unwrap();
    LayoutEngine::new().layout_frame_rust(&mut eval, frame);
    let snapshot = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .unwrap();
    let first = snapshot
        .iter_points()
        .find(|point| point.buffer_pos.as_i64() == 1)
        .unwrap();
    let second = snapshot
        .iter_points()
        .find(|point| point.buffer_pos.as_i64() == 3)
        .unwrap();
    assert!(
        second.y - first.y >= 60,
        "the 60px prefix must fit before the next line"
    );
}

#[test]
fn retained_geometry_tracks_prefix_image_operand_height() {
    assert_layout_mutation_with_host(
        r##"(setq image-dimensions (list :height 24 :width 20)
                   image-spec (cons 'image (cons :file (cons "prefix.png" image-dimensions)))
                   line-prefix (list 'space :width 1 :height image-spec))"##,
        "(setcar (cdr image-dimensions) 60)",
        true,
        Some(Box::new(ExplicitExtentImageHost)),
    );
}

#[test]
fn retained_geometry_tracks_wrap_prefix_string_stretch_height() {
    assert_layout_mutation_with_measurement(
        r##"(setq prefix-height (list 24) wrap-prefix (copy-sequence " "))
             (put-text-property 0 1 'display
               (list 'space :width 1 :height prefix-height) wrap-prefix)"##,
        "(setcar prefix-height 60)",
        true,
    );
}

#[test]
fn image_cache_flush_rebuilds_retained_prefix_geometry() {
    for flush in [
        "(image-flush image-spec t)",
        "(clear-image-cache t)",
        r##"(clear-image-cache "prefix.png")"##,
    ] {
        assert_layout_mutation_with_host(
            r##"(setq image-spec (list 'image :type 'png :file "prefix.png")
                       line-prefix (list 'space :width image-spec))"##,
            flush,
            true,
            Some(Box::new(ReloadingImageHost::default())),
        );
    }
}

#[test]
fn retained_geometry_rejects_mutated_pixel_arithmetic_image_width() {
    assert_layout_mutation_with_host(
        r##"(setq image-dimensions (list :width 20 :height 24)
                   image-spec (cons 'image (cons :file (cons "prefix.png" image-dimensions)))
                   line-prefix (list 'space :width (list '+ (list 5) (cons 0.5 image-spec))))"##,
        "(setcar (cdr image-dimensions) 60)",
        true,
        Some(Box::new(ExplicitExtentImageHost)),
    );
}

#[test]
fn retained_geometry_rejects_mutated_direct_pixel_image_width() {
    assert_layout_mutation_with_host(
        r##"(setq image-dimensions (list :width 24 :height 20)
                   image-spec (cons 'image (cons :file (cons "prefix.png" image-dimensions)))
                   line-prefix (list 'space :width image-spec))"##,
        "(setcar (cdr image-dimensions) 48)",
        true,
        Some(Box::new(ExplicitExtentImageHost)),
    );
}

#[test]
fn retained_geometry_rejects_mutated_image_operand_in_wrap_prefix_string() {
    assert_layout_mutation_with_host(
        r##"(setq image-dimensions (list :width 20 :height 24)
                   image-spec (cons 'image (cons :file (cons "prefix.png" image-dimensions)))
                   wrap-prefix (copy-sequence " "))
             (put-text-property 0 1 'display
               (list 'space :width (list '- (list 80) (cons 0.5 image-spec))) wrap-prefix)"##,
        "(setcar (cdr image-dimensions) 60)",
        true,
        Some(Box::new(ExplicitExtentImageHost)),
    );
}

#[test]
fn retained_geometry_rejects_mutated_literal_when_payload() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq prefix-space (list 'space :width 2) prefix-text (copy-sequence " "))
             (put-text-property 0 1 'display (cons 'when (cons t prefix-space)) prefix-text)
             prefix-text)"##,
        "(setcar (cdr (cdr prefix-space)) 6)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_literal_when_condition() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq conditional (list 'when t 'space :width 6)
                           prefix-text (copy-sequence " "))
             (put-text-property 0 1 'display conditional prefix-text) prefix-text)"##,
        "(setcar (cdr conditional) nil)",
    );
}

#[test]
fn retained_geometry_tracks_literal_when_payload_under_disable_eval() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "wrap-prefix",
        r##"(progn (setq prefix-space (list 'space :width 2) prefix-text (copy-sequence " "))
             (put-text-property 0 1 'display
               (list 'disable-eval (vector (cons 'when (cons t prefix-space)))) prefix-text)
             prefix-text)"##,
        "(setcar (cdr (cdr prefix-space)) 6)",
    );
}

#[test]
fn retained_geometry_tracks_prefix_image_vertical_margin() {
    for variable in ["line-prefix", "wrap-prefix"] {
        assert_layout_mutation_with_host(
            &format!(
                r##"(setq image-margin (cons 0 0) {variable} (copy-sequence " "))
                     (put-text-property 0 1 'display
                       (list 'image :type 'png :file "prefix.png" :margin image-margin)
                       {variable})"##
            ),
            "(setcdr image-margin 10)",
            true,
            Some(Box::new(RecordingImageDisplayHost::default())),
        );
    }
}

#[test]
fn published_prefix_image_reserves_both_vertical_margins() {
    let hosts: Vec<Box<dyn DisplayHost>> = vec![
        Box::new(RecordingImageDisplayHost::default()),
        Box::new(ReloadingImageHost::default()),
    ];
    for host in hosts {
        let mut eval = Context::new();
        eval.set_display_host(host);
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert("a\nb\n");
        let frame = eval
            .frame_manager_mut()
            .create_frame("image-margins", 160, 300, buffer);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(
            r##"(progn (setq line-prefix (copy-sequence " "))
          (put-text-property 0 1 'display
            '(image :type png :file "prefix.png" :margin (0 . 10)) line-prefix))"##,
        )
        .unwrap();
        let FrameLayoutAttempt::Prepared(state) =
            LayoutEngine::new().redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("image prefix must produce a presentation");
        };
        let matrix = &state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix;
        let row_for = |ch| {
            matrix
                .rows
                .iter()
                .find(|row| {
                    row.glyphs[GlyphArea::Text.index()].iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: actual } if actual == ch))
                })
                .unwrap()
        };
        let first = row_for('a');
        let second = row_for('b');
        assert!(first.height_px >= 44.0, "24px image plus two 10px margins");
        assert!(
            second.pixel_y - first.pixel_y >= first.height_px,
            "published rows must not overlap"
        );
    }
}

#[test]
fn retained_geometry_rejects_mutated_prefix_image_margin() {
    assert_layout_mutation_with_host(
        r##"(setq image-margin (cons 0 0)
                   line-prefix (copy-sequence " "))
             (put-text-property 0 1 'display
               (list 'image :type 'png :file "prefix.png" :margin image-margin)
               line-prefix)"##,
        "(setcar image-margin 20)",
        true,
        Some(Box::new(RecordingImageDisplayHost::default())),
    );
}

#[test]
fn retained_geometry_rejects_mutated_wrap_prefix_image_margin() {
    assert_layout_mutation_with_host(
        r##"(setq image-margin (cons 0 0)
                   wrap-prefix (copy-sequence " "))
             (put-text-property 0 1 'display
               (vector (list 'image :type 'png :file "prefix.png" :margin image-margin))
               wrap-prefix)"##,
        "(setcar image-margin 10)",
        true,
        Some(Box::new(RecordingImageDisplayHost::default())),
    );
}

#[test]
fn retained_geometry_rejects_mutation_late_in_long_image_spec() {
    assert_layout_mutation_with_host(
        r##"(setq image-margin (cons 0 0)
                   image-properties (list :type 'png :file "prefix.png" :margin image-margin)
                   n 0 line-prefix (copy-sequence " "))
             (while (< n 256)
               (setq image-properties (cons :unused (cons nil image-properties)) n (+ n 1)))
             (put-text-property 0 1 'display (cons 'image image-properties) line-prefix)"##,
        "(setcar image-margin 20)",
        true,
        Some(Box::new(RecordingImageDisplayHost::default())),
    );
}

#[test]
fn forced_window_update_rebuilds_a_mutated_buffer_image_spec() {
    assert_layout_mutation_with_host(
        r#"(setq mutable-avatar (list 'image :type 'png :file "avatar.png" :width 20 :height 24))
           (put-text-property 1 2 'display mutable-avatar)"#,
        r#"(setcdr mutable-avatar (list :type 'png :file "delivered-avatar.png" :width 60 :height 24))
           (image-flush mutable-avatar t)
           (force-window-update)"#,
        true,
        Some(Box::new(ExplicitExtentImageHost)),
    );
}

/// A presentation-only redisplay invalidation (`redisplay_generation` moves:
/// messages, chrome, mode lines) must NOT relayout body text.  If retained
/// bodies were keyed on the generic generation this window would rebuild.
#[test]
fn presentation_only_redisplay_invalidation_retains_body_rows() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"abcdefghij".repeat(20));
    let frame = eval
        .frame_manager_mut()
        .create_frame("presentation-only", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame);
    let original = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .unwrap()
        .iter_points()
        .collect::<Vec<_>>();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        engine.last_layout_stats().reused_rows > 0,
        "unchanged inputs retain row reuse"
    );

    let generation_before = eval.redisplay_generation();
    // A real public presentation-only entry point: GNU's global
    // `force-mode-line-update` raises update_mode_lines and rebuilds chrome,
    // moving the generic redisplay generation without touching body inputs.
    eval.eval_str("(force-mode-line-update t)")
        .expect("force-mode-line-update");
    assert!(
        eval.redisplay_generation() > generation_before,
        "the generic redisplay generation moves"
    );
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        engine.last_layout_stats().reused_rows > 0,
        "a presentation-only invalidation must not relayout body text"
    );
    let after = eval
        .fresh_window_display_snapshot(frame, window, buffer)
        .unwrap()
        .iter_points()
        .collect::<Vec<_>>();
    assert_eq!(
        original, after,
        "presentation-only invalidation keeps geometry"
    );
}

/// A forced body redisplay names exactly one window: the other frame's window
/// keeps reusing its retained rows, and only the targeted window escalates to
/// a full rebuild (GNU `Fforce_window_update`, src/window.c:4492).
#[test]
fn forced_body_redisplay_rebuilds_only_the_targeted_window() {
    use neovm_core::window::ForcedBodyRedisplay;

    let mut eval = Context::new();
    let buffer_a = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer_a)
        .unwrap()
        .insert(&"a-line\n".repeat(40));
    let frame_a = eval
        .frame_manager_mut()
        .create_frame("forced-a", 160, 160, buffer_a);
    let buffer_b = eval.buffer_manager_mut().create_buffer("forced-b");
    eval.buffer_manager_mut()
        .get_mut(buffer_b)
        .unwrap()
        .insert(&"b-line\n".repeat(40));
    let frame_b = eval
        .frame_manager_mut()
        .create_frame("forced-b-frame", 160, 160, buffer_b);
    let window_a = eval.frame_manager().get(frame_a).unwrap().selected_window;

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_a);
    engine.layout_frame_rust(&mut eval, frame_b);
    engine.layout_frame_rust(&mut eval, frame_b);
    assert!(
        engine.last_layout_stats().reused_rows > 0,
        "the unchanged second frame reuses its body rows"
    );

    eval.force_body_redisplay(ForcedBodyRedisplay::Window(window_a));
    engine.layout_frame_rust(&mut eval, frame_b);
    assert!(
        engine.last_layout_stats().reused_rows > 0,
        "forcing another window's body must not relayout this window"
    );
    engine.layout_frame_rust(&mut eval, frame_a);
    assert!(
        engine.last_layout_stats().full_windows > 0,
        "the targeted window must rebuild its body"
    );
}

#[test]
fn retained_presentation_rejects_mutated_prefix_stipple_bytes() {
    assert_decoration_mutation_invalidates_presentation(
        r##"(setq bitmap-data (copy-sequence "A"))
             (put-text-property 0 2 'face
               (list :stipple (list 8 1 bitmap-data)) line-prefix)"##,
        "(aset bitmap-data 0 66)",
    );
}

#[test]
fn retained_presentation_rejects_mutated_stipple_dimensions() {
    for mutation in [
        "(setcar bitmap 4)",
        "(setcar (cdr bitmap) 2)",
        "(setcdr bitmap (list 2 (copy-sequence \"CD\")))",
    ] {
        assert_decoration_mutation_invalidates_presentation(
            r##"(setq bitmap (list 8 1 (copy-sequence "AB")))
                 (put-text-property 0 2 'face (list :stipple bitmap) line-prefix)"##,
            mutation,
        );
    }
}

#[test]
fn retained_presentation_rejects_mutated_replacement_stipple_bytes() {
    assert_decoration_mutation_invalidates_presentation(
        r##"(setq bitmap-data (copy-sequence "A")
                   replacement (copy-sequence "xx")
                   line-prefix (copy-sequence " "))
             (put-text-property 0 2 'face
               (list :stipple (list 8 1 bitmap-data)) replacement)
             (put-text-property 0 1 'display replacement line-prefix)"##,
        "(aset bitmap-data 0 66)",
    );
}

#[test]
fn retained_presentation_rejects_mutated_prefix_box_line_width() {
    assert_decoration_mutation_invalidates_presentation(
        "(setq box-spec (list :line-width 1)) (put-text-property 0 2 'face (list :box box-spec) line-prefix)",
        "(setcar (cdr box-spec) 6)",
    );
}

#[test]
fn retained_presentation_rejects_mutated_box_width_pair() {
    assert_decoration_mutation_invalidates_presentation(
        "(setq widths (cons 1 1)) (put-text-property 0 2 'face (list :box (list :line-width widths)) line-prefix)",
        "(setcar widths 6)",
    );
}

#[test]
fn retained_presentation_rejects_mutated_underline_style() {
    assert_decoration_mutation_invalidates_presentation(
        "(setq underline-spec (list :style 'line)) (put-text-property 0 2 'face (list :underline underline-spec) line-prefix)",
        "(setcar (cdr underline-spec) 'wave)",
    );
}

#[test]
fn retained_presentation_rejects_mutated_underline_position() {
    assert_decoration_mutation_invalidates_presentation(
        "(setq underline-spec (list :position 0)) (put-text-property 0 2 'face (list :underline underline-spec) line-prefix)",
        "(setcar (cdr underline-spec) 4)",
    );
}

#[test]
fn retained_presentation_rejects_mutated_decoration_color_string() {
    assert_decoration_mutation_invalidates_presentation(
        r##"(setq color (copy-sequence "#ff0000")) (put-text-property 0 2 'face (list :underline (list :color color)) line-prefix)"##,
        "(aset color 1 48)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_filtered_prefix_face_height() {
    assert_layout_mutation_with_measurement(
        r##"(setq prefix-face (list :height 1.0)
                   line-prefix (copy-sequence "xx"))
             (put-text-property 0 2 'face
               (list :filtered (list :window-system 'neomacs) prefix-face)
               line-prefix)"##,
        "(setcar (cdr prefix-face) 2.0)",
        true,
    );
}

#[test]
fn retained_geometry_rejects_mutated_prefix_window_filter_operand() {
    assert_layout_mutation_with_measurement(
        r##"(set-window-parameter nil 'prefix-scale 'large)
             (setq prefix-filter (list :window 'prefix-scale 'large)
                   line-prefix (copy-sequence "xx"))
             (put-text-property 0 2 'face
               (list :filtered prefix-filter (list :height 2.0)) line-prefix)"##,
        "(setcar (cdr (cdr prefix-filter)) 'small)",
        true,
    );
}

#[test]
fn retained_geometry_rejects_mutated_replacement_window_system_filter() {
    assert_layout_mutation_with_measurement(
        r##"(setq prefix-filter (list :window-system 'neomacs)
                   replacement (copy-sequence "xx")
                   line-prefix (copy-sequence " "))
             (put-text-property 0 2 'face
               (list :filtered prefix-filter (list :height 2.0)) replacement)
             (put-text-property 0 1 'display replacement line-prefix)"##,
        "(setcar (cdr prefix-filter) nil)",
        true,
    );
}

#[test]
fn retained_geometry_rejects_mutated_prefix_face_height() {
    assert_layout_mutation_with_measurement(
        r##"(setq prefix-face (list :height 1.0) line-prefix (copy-sequence "xx")) (put-text-property 0 2 'face prefix-face line-prefix)"##,
        "(setcar (cdr prefix-face) 2.0)",
        true,
    );
}

#[test]
fn retained_geometry_rejects_mutated_inherited_prefix_face_height() {
    assert_layout_mutation_with_measurement(
        r##"(setq parent-face (list :height 1.0) wrap-prefix (copy-sequence "xx")) (put-text-property 0 2 'face (list (list :inherit parent-face)) wrap-prefix)"##,
        "(setcar (cdr parent-face) 2.0)",
        true,
    );
}

#[test]
fn retained_geometry_rejects_mutated_replacement_face_height() {
    assert_layout_mutation_with_measurement(
        r##"(setq replacement-face (list :height 1.0) replacement (copy-sequence "xx") line-prefix (copy-sequence " ")) (put-text-property 0 2 'face replacement-face replacement) (put-text-property 0 1 'display replacement line-prefix)"##,
        "(setcar (cdr replacement-face) 2.0)",
        true,
    );
}

#[test]
fn cyclic_face_inheritance_capture_tracks_other_attributes() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("face-cycle", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r##"(progn (setq height-tail (list :height 1) cyclic-face (cons :inherit (cons nil height-tail)) line-prefix (copy-sequence "x")) (setcar (cdr cyclic-face) cyclic-face) (put-text-property 0 1 'face cyclic-face line-prefix))"##).unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr height-tail) 2)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr height-tail) 1)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn string_face_freshness_tracks_mutable_color_bytes() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("face-inputs", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r##"(progn (setq color (copy-sequence "#ff0000") line-prefix (copy-sequence "x")) (put-text-property 0 1 'face (list :foreground color) line-prefix))"##).unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(aset color 1 48)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(aset color 1 102)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_mutated_prefix_replacement_string() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq replacement (copy-sequence " ") prefix-text (copy-sequence " ")) (put-text-property 0 1 'display replacement prefix-text) prefix-text)"##,
        "(aset replacement 0 9)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_wrapped_replacement_string() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "wrap-prefix",
        r##"(progn (setq replacement (copy-sequence " ") prefix-text (copy-sequence " ")) (put-text-property 0 1 'display (list 'disable-eval (vector replacement)) prefix-text) prefix-text)"##,
        "(aset replacement 0 9)",
    );
}

#[test]
fn replacement_string_freshness_does_not_recurse_through_display_properties() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("replacement-inputs", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r##"(progn (setq replacement (copy-sequence " ") line-prefix (copy-sequence " ")) (put-text-property 0 1 'display replacement line-prefix) (put-text-property 0 1 'display replacement replacement))"##).unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(aset replacement 0 9)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(aset replacement 0 32)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_mutated_list_display_space() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "wrap-prefix",
        r##"(progn (setq prefix-space (list 'space :width 2) prefix-text (copy-sequence " ")) (put-text-property 0 1 'display (list '(raise 0) prefix-space) prefix-text) prefix-text)"##,
        "(setcar (cdr (cdr prefix-space)) 6)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_disabled_display_space() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq prefix-space (list 'space :width 2) prefix-text (copy-sequence " ")) (put-text-property 0 1 'display (list 'disable-eval (vector prefix-space)) prefix-text) prefix-text)"##,
        "(setcar (cdr (cdr prefix-space)) 6)",
    );
}

#[test]
fn retained_geometry_rejects_display_vector_precedence_changes() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq specs (vector '(raise 0) '(space :width 6)) prefix-text (copy-sequence " ")) (put-text-property 0 1 'display specs prefix-text) prefix-text)"##,
        r##"(aset specs 0 "x")"##,
    );
}

#[test]
fn cyclic_display_spec_capture_terminates_and_tracks_changes() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("cyclic-display", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r##"(progn (setq space (list 'space :width 2) specs (list space) line-prefix (copy-sequence " ")) (setcdr specs specs) (put-text-property 0 1 'display specs line-prefix))"##).unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr space)) 6)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr space)) 2)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_mutated_vector_display_space() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq prefix-space (list 'space :width 2) prefix-text (copy-sequence " ")) (put-text-property 0 1 'display (vector prefix-space) prefix-text) prefix-text)"##,
        "(setcar (cdr (cdr prefix-space)) 6)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_prefix_string_display_space() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        r##"(progn (setq prefix-space (list 'space :width 2) prefix-text (copy-sequence " ")) (put-text-property 0 1 'display prefix-space prefix-text) prefix-text)"##,
        "(setcar (cdr (cdr prefix-space)) 6)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_wrap_string_space_expression() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "wrap-prefix",
        r##"(progn (setq dimension (list 16) prefix-space (list 'space :width (list '+ dimension 1)) prefix-text (copy-sequence " ")) (put-text-property 0 1 'display prefix-space prefix-text) prefix-text)"##,
        "(setcar dimension 48)",
    );
}

#[test]
fn prefix_string_space_freshness_ignores_unrelated_property_payloads() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("string-space", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r##"(progn (setq space (list 'space :width 2) unrelated (list 1) line-prefix (copy-sequence " ")) (put-text-property 0 1 'display space line-prefix) (put-text-property 0 1 'help-echo unrelated line-prefix))"##).unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(setcar unrelated 6)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr space)) 6)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr space)) 2)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_mutated_display_table_glyph_vector() {
    assert_layout_mutation_invalidates_geometry(
        "(setq glyphs (vector 97) buffer-display-table (make-char-table 'display-table nil)) (aset buffer-display-table 97 glyphs)",
        "(aset glyphs 0 30028)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_standard_display_table_glyph() {
    assert_layout_mutation_invalidates_geometry(
        "(setq glyph (cons 97 0) standard-display-table (make-char-table 'display-table nil)) (aset standard-display-table 97 (vector glyph))",
        "(setcar glyph 30028)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_default_glyph_vector() {
    assert_layout_mutation_invalidates_geometry(
        "(setq glyphs (vector 97) buffer-display-table (make-char-table 'display-table nil)) (set-char-table-range buffer-display-table nil glyphs)",
        "(aset glyphs 0 30028)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_parent_table_glyph_vector() {
    assert_layout_mutation_invalidates_geometry(
        "(setq glyphs (vector 97) parent-table (make-char-table 'display-table nil) buffer-display-table (make-char-table 'display-table nil)) (aset parent-table 97 glyphs) (set-char-table-parent buffer-display-table parent-table)",
        "(aset glyphs 0 30028)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_ellipsis_glyph_vector() {
    assert_layout_mutation_invalidates_geometry(
        "(put 'display-table 'char-table-extra-slots 6) (setq glyphs (vector 97) buffer-display-table (make-char-table 'display-table nil) buffer-invisibility-spec '((hidden . t))) (set-char-table-extra-slot buffer-display-table 4 glyphs) (put-text-property 1 31 'invisible 'hidden)",
        "(aset glyphs 0 30028)",
    );
}

#[test]
fn display_table_freshness_ignores_unrelated_vector_mutations() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("glyph-inputs", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(progn (setq glyphs (vector 97) unrelated (vector 97) buffer-display-table (make-char-table 'display-table nil) standard-display-table (make-char-table 'display-table nil)) (aset buffer-display-table 97 glyphs) (aset standard-display-table 97 unrelated))").unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(aset unrelated 0 30028)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(aset glyphs 0 30028)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(aset glyphs 0 97)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_mutated_display_table_entry() {
    assert_layout_mutation_invalidates_geometry(
        "(setq buffer-display-table (make-char-table 'display-table nil)) (aset buffer-display-table 97 [97])",
        "(aset buffer-display-table 97 [97 97 97 97])",
    );
}

#[test]
fn retained_geometry_rejects_mutated_display_table_default() {
    assert_layout_mutation_invalidates_geometry(
        "(setq buffer-display-table (make-char-table 'display-table nil)) (set-char-table-range buffer-display-table nil [97])",
        "(set-char-table-range buffer-display-table nil [97 97 97 97])",
    );
}

#[test]
fn retained_geometry_rejects_mutated_invisibility_membership() {
    assert_layout_mutation_invalidates_geometry(
        "(setq buffer-invisibility-spec (list 'hidden)) (put-text-property 1 11 'invisible 'hidden)",
        "(setcar buffer-invisibility-spec 'visible)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_invisibility_ellipsis() {
    assert_layout_mutation_invalidates_geometry(
        "(setq category (cons 'hidden nil) buffer-invisibility-spec (list category)) (put-text-property 1 31 'invisible 'hidden)",
        "(setcdr category t)",
    );
}

#[test]
fn invisibility_freshness_tracks_ellipsis_truthiness_not_identity() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("invisibility", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(setq category (cons 'hidden t) buffer-invisibility-spec (list category))")
        .unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(setcdr category 'another-non-nil-value)")
        .unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcdr category nil)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcdr category t)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_in_place_line_prefix_changes() {
    assert_prefix_mutation_invalidates_geometry("line-prefix", "(aset line-prefix 0 9)");
}

#[test]
fn retained_geometry_rejects_mutated_nested_space_expression() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        "(progn (setq dimension (list 16)) (list 'space :width (list '+ dimension 1)))",
        "(setcar dimension 48)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_scaled_space_expression() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "wrap-prefix",
        "(progn (setq dimension (cons 2 1)) (list 'space :width (list '- dimension 1)))",
        "(setcdr dimension 3)",
    );
}

#[test]
fn cyclic_pixel_input_capture_terminates_and_detects_operand_changes() {
    // Only capture freshness: evaluating cyclic arithmetic is not supported.
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("cyclic-prefix", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(progn (setq dimension (list 1) line-prefix (list 'space :width (cons '+ dimension))) (setcdr dimension dimension))").unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar dimension 2)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar dimension 1)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_mutated_space_prefix_width() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "line-prefix",
        "(list 'space :width 2)",
        "(setcar (cdr (cdr line-prefix)) 6)",
    );
}

#[test]
fn retained_geometry_rejects_mutated_wrap_space_prefix_width() {
    assert_prefix_spec_mutation_invalidates_geometry(
        "wrap-prefix",
        "(list 'space :width 2)",
        "(setcar (cdr (cdr wrap-prefix)) 6)",
    );
}

#[test]
fn space_prefix_freshness_ignores_shadowed_and_unknown_operands() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("space-prefix", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(setq tail (list :width 8 :unknown 9) line-prefix (cons 'space (cons :width (cons 2 tail))))").unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(setcar (cdr tail) 20)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr (cdr tail))) 30)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr line-prefix)) 6)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(setcar (cdr (cdr line-prefix)) 2)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn retained_geometry_rejects_in_place_wrap_prefix_changes() {
    assert_prefix_mutation_invalidates_geometry("wrap-prefix", "(aset wrap-prefix 0 9)");
}

#[test]
fn retained_geometry_rejects_prefix_text_property_changes() {
    assert_prefix_mutation_invalidates_geometry(
        "line-prefix",
        r#"(put-text-property 0 1 'display '(space :width 6) line-prefix)"#,
    );
}

#[test]
fn restoring_prefix_bytes_restores_attempt_validity() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("prefix-restoration", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str(r#"(setq line-prefix (copy-sequence "  "))"#)
        .unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.eval_str("(aset line-prefix 0 9)").unwrap();
    assert_ne!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
    eval.eval_str("(aset line-prefix 0 32)").unwrap();
    assert_eq!(
        before,
        eval.window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}
