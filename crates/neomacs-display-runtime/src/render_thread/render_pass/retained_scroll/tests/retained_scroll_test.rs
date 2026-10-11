use super::*;
use neomacs_display_protocol::input_progress::InputStream;
use neomacs_display_protocol::*;

pub(in crate::render_thread::render_pass) fn fixture() -> FrameGlyphBuffer {
    fixture_with(|_| {})
}

fn fixture_with(change: impl FnOnce(&mut scroll_coverage::ScrollCoverage)) -> FrameGlyphBuffer {
    let mut state = FrameDisplayState::new(8, 4, 10.0, 10.0);
    state.presentation_id = PresentationId::new(71);
    state.background = Color::RED;
    state.fringe_bitmaps.insert(
        1,
        frame_glyphs::FringeBitmapData {
            bits: vec![0x8000, 0xc000, 0xe000],
            width: 3,
            height: 3,
            period: 0,
            align: 0,
        },
    );
    let window = DisplayWindowId::new(1);
    let viewport = Rect::new(10.0, 10.0, 60.0, 20.0);
    let bounds = Rect::new(10.0, 10.0, 60.0, 60.0);
    let mut matrix = GlyphMatrix::new(6, 6);
    let mut positions = Vec::new();
    for index in 0..6 {
        let face_id = FaceId::new(index as u32 + 1);
        let mut face = Face::new(face_id);
        face.background = if index % 2 == 0 {
            Color::GREEN
        } else {
            Color::BLUE
        };
        state.faces.insert(face_id, face);
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        row.pixel_y = index as f32 * 10.0;
        row.height_px = 10.0;
        row.ascent_px = 8.0;
        row.right_fringe_bitmap = Some(glyph_matrix::FringeBitmapInfo {
            bitmap_index: 1,
            face_id,
        });
        row.start_charpos = index * 2;
        row.end_charpos = index * 2 + 1;
        let mut glyph =
            Glyph::stretch_with_provenance(6, face_id, GlyphProvenance::buffer(index * 2));
        glyph.pixel_width = 60.0;
        glyph.pixel_height = 10.0;
        glyph.pixel_ascent = 8.0;
        row.glyphs[1].push(glyph);
        positions.push(PresentedTextPosition::new(
            window,
            FrameRect::new(10.0, 10.0 + row.pixel_y, 60.0, 10.0).unwrap(),
            index as i64 * 2 + 1,
            index as i64,
            0,
        ));
        matrix.rows[index] = MatrixRow::new(row);
    }
    let content = WindowMatrixEntry {
        window_id: window,
        matrix,
        pixel_bounds: Rect::new(0.0, 10.0, 80.0, 20.0),
        text_pixel_bounds: viewport,
        text_clip_bounds: Some(bounds),
        selected: true,
    };
    let hit_index = PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![PresentedHitRegion::new(
            Some(window),
            PresentedRegionKind::TextBody,
            FrameRect::new(10.0, 10.0, 60.0, 60.0).unwrap(),
            0,
        )],
        positions,
    )
    .unwrap();
    let live_hit_index = PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![PresentedHitRegion::new(
            Some(window),
            PresentedRegionKind::TextBody,
            FrameRect::new(viewport.x, viewport.y, viewport.width, viewport.height).unwrap(),
            0,
        )],
        hit_index
            .text_positions()
            .iter()
            .filter(|position| position.bounds().bottom() <= viewport.bottom())
            .cloned()
            .collect(),
    )
    .unwrap();
    state.window_matrices.push(content.clone());
    state
        .scroll_coverage
        .push(Arc::new(scroll_coverage::ScrollCoverage {
            epoch: 1,
            anchor_row: 0,
            predict_pixels: true,
            compositor_enabled: true,
            viewport,
            origin: 0.0,
            content,
            faces: state.faces.clone(),
            fonts: Default::default(),
            char_fonts: Default::default(),
            shaped_clusters: Default::default(),
            hit_index,
            pointer_source: Default::default(),
        }));
    change(Arc::make_mut(&mut state.scroll_coverage[0]));
    let mut frame = state.materialize();
    frame.install_presented_hit_index(live_hit_index).unwrap();
    assert_eq!(frame.scroll_surfaces.len(), 1);
    frame
}

fn mapping(frame: &FrameGlyphBuffer, scale: f32) -> PresentMapping {
    let SurfaceState::Drawable(surface) = SurfaceState::from_device_size(
        (frame.width * scale) as u32,
        (frame.height * scale) as u32,
        DeviceScale::new(scale).unwrap(),
    )
    .unwrap() else {
        unreachable!()
    };
    PresentMapping::top_left_clip(
        surface,
        PresentationExtent::new(
            frame.presentation_id,
            GeometrySize::<LogicalPixels>::from_px(frame.width, frame.height).unwrap(),
        ),
    )
}

fn pixels(renderer: &WgpuRenderer, texture: &wgpu::Texture) -> Vec<u8> {
    let width = texture.width();
    let height = texture.height();
    let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained-scroll-pixel-test"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device()
        .create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit(std::iter::once(encoder.finish()));
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    renderer
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(3)),
        })
        .unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    (0..height)
        .flat_map(|y| data[(y * row) as usize..(y * row + width * 4) as usize].to_vec())
        .collect()
}

#[test]
fn retained_scroll_pixels_match_full_render_and_reuse_coverage_during_reversal() {
    let Ok(mut renderer) = WgpuRenderer::new(None, 80, 40) else {
        assert!(std::env::var_os("NEOMACS_REQUIRE_GPU_TESTS").is_none());
        return;
    };
    for scale in [1.0, 1.5, 2.0] {
        let mut render = GuiFrameRenderState::new(
            1,
            renderer.device(),
            scale as f64,
            false,
            frame_time::observe_platform_now(),
        );
        let original = fixture();
        let stream = InputStream::default();
        let size = SnapshotSize::new((80.0 * scale) as u32, (40.0 * scale) as u32).unwrap();
        let expected = renderer.acquire_snapshot(size).unwrap();
        let actual = renderer.acquire_snapshot(size).unwrap();
        let mut deliveries = Vec::new();
        let mut cached_id = None;
        let mut held = None;
        for (step, delta) in [4.0, 8.0, -8.0].into_iter().enumerate() {
            // Every authoritative scene owns a fresh coverage Arc, even when
            // the bounded coverage still paints exactly the same picture.
            let refreshed = fixture_with(|coverage| coverage.origin = step as f32 * 4.0);
            render.compositor.current_scene_generation += 1;
            render.compositor.input_scroll.reconcile(Some(&refreshed));
            let delivery = stream.issue().unwrap();
            assert!(render.compositor.input_scroll.push(
                &refreshed,
                11.0,
                11.0,
                delta,
                delivery.receipt(),
                None
            ));
            deliveries.push(delivery);
            let mut frame = original.clone();
            render.compositor.input_scroll.paint(&mut frame);
            let map = mapping(&frame, scale);
            let atlas = render.compositor.glyph_atlas.as_mut().unwrap();
            atlas.set_current_frame_fonts(frame.font_bindings());
            renderer.render_frame_glyphs(
                expected.view(),
                &frame,
                atlas,
                map,
                false,
                None,
                None,
                None,
                None,
                None,
            );
            super::super::scene::render_frame_root_glyphs(
                &mut renderer,
                &mut render,
                actual.view(),
                &frame,
                map,
                false,
                None,
                None,
                true,
            );
            let cache = render
                .compositor
                .retained_scroll
                .as_ref()
                .expect("retained body must run");
            if let Some(id) = cached_id {
                assert_eq!(cache.texture.id(), id);
            }
            cached_id = Some(cache.texture.id());
            held = Some(cache.texture.clone());
            let expected_pixels = pixels(&renderer, expected.view().texture());
            let actual_pixels = pixels(&renderer, actual.view().texture());
            assert!(
                actual_pixels
                    .iter()
                    .zip(&expected_pixels)
                    .all(|(&a, &b)| a.abs_diff(b) <= 1),
                "retained crop differs from canonical projection at scale={scale}, delta={delta}"
            );
            render.compositor.input_scroll.submit();
        }
        let mut frame = original.clone();
        render.compositor.input_scroll.paint(&mut frame);
        render.compositor.current_scene_generation += 1;
        let rebuilt = prepare(
            &mut renderer,
            &mut render,
            &frame,
            mapping(&frame, scale),
            false,
        )
        .unwrap();
        assert_eq!(
            rebuilt.texture.id(),
            held.as_ref().unwrap().id(),
            "unchanged paint survives a new scene"
        );
        frame.background = Color::new(0.25, 0.5, 0.75, 1.0);
        for _ in 0..2 {
            assert!(
                prepare(
                    &mut renderer,
                    &mut render,
                    &frame,
                    mapping(&frame, scale),
                    false
                )
                .is_none()
            );
        }
        let changed = prepare(
            &mut renderer,
            &mut render,
            &frame,
            mapping(&frame, scale),
            false,
        )
        .unwrap();
        assert_ne!(
            changed.texture.id(),
            held.as_ref().unwrap().id(),
            "background changes must rebuild without overwriting a live lease"
        );
        renderer.effects.line_highlight.enabled = true;
        assert!(
            prepare(
                &mut renderer,
                &mut render,
                &frame,
                mapping(&frame, scale),
                false
            )
            .is_none()
        );
        renderer.effects.line_highlight.enabled = false;
        renderer.effects.cursor_color_cycle.enabled = false;
        renderer.effects.scroll_bar.width = 0;
        assert!(
            prepare(
                &mut renderer,
                &mut render,
                &frame,
                mapping(&frame, scale),
                false
            )
            .is_some(),
            "the software-adapter profile must retain static body pixels too"
        );
        renderer.effects = Default::default();
        assert!(
            prepare(
                &mut renderer,
                &mut render,
                &frame,
                mapping(&frame, scale),
                true
            )
            .is_none(),
            "extra spacing and gradients require the full glyph path"
        );
        let cached = prepare(
            &mut renderer,
            &mut render,
            &frame,
            mapping(&frame, scale),
            false,
        )
        .unwrap();
        render.compositor.input_scroll = Default::default();
        assert!(
            prepare(
                &mut renderer,
                &mut render,
                &frame,
                mapping(&frame, scale),
                false
            )
            .is_none()
        );
        assert_eq!(
            render
                .compositor
                .retained_scroll
                .as_ref()
                .unwrap()
                .texture
                .id(),
            cached.texture.id(),
            "acknowledging input must not discard an unchanged picture"
        );
        frame.scroll_surfaces.clear();
        assert!(
            prepare(
                &mut renderer,
                &mut render,
                &frame,
                mapping(&frame, scale),
                false
            )
            .is_none()
        );
        assert!(
            render.compositor.retained_scroll.is_none(),
            "withdrawn coverage releases its lease"
        );
    }
}

#[test]
fn software_quality_profile_allows_retained_scroll() {
    use crate::render_thread::render_quality::{RenderBackendProfile, RenderQualityPolicy};
    let policy = RenderQualityPolicy::negotiate(
        RenderBackendProfile::software(),
        &neomacs_display_protocol::VisualConfig::default(),
    );
    assert!(static_body_effects(
        &policy.effective_visual_config().effects
    ));
}

#[test]
fn gallery_roundtrip_allows_retained_scroll() {
    use neomacs_display_protocol::{EffectOperation, EffectsConfig};
    let defaults = EffectsConfig::default();
    let operations = defaults
        .effect_names()
        .into_iter()
        .map(|name| EffectOperation::set(name.clone(), defaults.effect_values(&name).unwrap()))
        .collect::<Vec<_>>();
    let actual = defaults.apply_effects(&operations).unwrap();
    assert!(static_body_effects(&actual));
    use crate::render_thread::render_quality::{RenderBackendProfile, RenderQualityPolicy};
    let mut requested = neomacs_display_protocol::VisualConfig::default();
    requested.effects = actual;
    let policy = RenderQualityPolicy::negotiate(RenderBackendProfile::software(), &requested);
    assert!(static_body_effects(
        &policy.effective_visual_config().effects
    ));
}

#[test]
fn disabled_effect_parameters_do_not_veto_static_body_rasters() {
    let mut effects = neomacs_display_protocol::EffectsConfig::default();
    effects.argyle_pattern.color = (0.2, 0.4, 0.6);
    effects.bg_pattern.spacing = 37.0;
    assert!(
        static_body_effects(&effects),
        "inactive properties cannot change body pixels"
    );
}

#[test]
fn every_enabled_effect_has_an_explicit_static_body_decision() {
    use neomacs_display_protocol::{EffectOperation, EffectValue, EffectsConfig};
    let defaults = EffectsConfig::default();
    for name in defaults.effect_names() {
        let values = defaults.effect_values(&name).unwrap();
        if !values.iter().any(|(property, _)| property == "enabled") {
            continue;
        }
        let enabled = defaults
            .apply_effects(&[EffectOperation::set(
                name.clone(),
                [("enabled", EffectValue::Bool(true))],
            )])
            .unwrap();
        assert_eq!(
            static_body_effects(&enabled),
            name == "cursor-color-cycle",
            "{name}"
        );
    }
    let mut patterned = defaults.clone();
    patterned.bg_pattern.style = 1;
    assert!(!static_body_effects(&patterned));
    patterned = defaults;
    patterned.mode_line_separator.style = 1;
    assert!(!static_body_effects(&patterned));
}

#[test]
fn changed_coverage_pixels_and_font_catalog_rebuild_the_raster() {
    let Ok(mut renderer) = WgpuRenderer::new(None, 80, 40) else {
        assert!(std::env::var_os("NEOMACS_REQUIRE_GPU_TESTS").is_none());
        return;
    };
    let mut render = GuiFrameRenderState::new(
        1,
        renderer.device(),
        1.0,
        false,
        frame_time::observe_platform_now(),
    );
    let stream = InputStream::default();
    let mut deliveries = Vec::new();
    let mut previous = None;
    for change in 0..4 {
        let mut frame = fixture_with(|coverage| {
            if change >= 1 {
                coverage.faces.get_mut(&FaceId::new(1)).unwrap().foreground = Color::BLACK;
            }
            if change >= 2 {
                let mut row = coverage.content.matrix.rows[0].as_ref().clone();
                row.glyphs[1][0].pixel_width = 59.0;
                coverage.content.matrix.rows[0] = MatrixRow::new(row);
            }
        });
        if change == 3 {
            frame.font_catalog_generation = frame.font_catalog_generation.next();
        }
        render.compositor.input_scroll = Default::default();
        let delivery = stream.issue().unwrap();
        assert!(render.compositor.input_scroll.push(
            &frame,
            11.0,
            11.0,
            4.0,
            delivery.receipt(),
            None
        ));
        deliveries.push(delivery);
        render.compositor.input_scroll.paint(&mut frame);
        if previous.is_some() {
            for _ in 0..2 {
                assert!(
                    prepare(
                        &mut renderer,
                        &mut render,
                        &frame,
                        mapping(&frame, 1.0),
                        false
                    )
                    .is_none()
                );
            }
        }
        let raster = prepare(
            &mut renderer,
            &mut render,
            &frame,
            mapping(&frame, 1.0),
            false,
        )
        .unwrap();
        if let Some(old) = previous.as_ref() {
            let old: &SnapshotLease = old;
            assert_ne!(
                old.id(),
                raster.texture.id(),
                "changed paint dependency {change} reused old pixels"
            );
        }
        previous = Some(raster.texture);
    }
}

#[test]
fn changing_coverage_defers_raster_work_until_paint_is_stable() {
    let Ok(mut renderer) = WgpuRenderer::new(None, 80, 40) else {
        assert!(std::env::var_os("NEOMACS_REQUIRE_GPU_TESTS").is_none());
        return;
    };
    let mut render = GuiFrameRenderState::new(
        1,
        renderer.device(),
        1.0,
        false,
        frame_time::observe_platform_now(),
    );
    let stream = InputStream::default();
    let mut deliveries = Vec::new();
    let mut original = None;
    for change in 0..5 {
        let mut frame = fixture_with(|coverage| {
            coverage.faces.get_mut(&FaceId::new(1)).unwrap().foreground =
                Color::new(change as f32 / 8.0, 0.0, 0.0, 1.0);
        });
        render.compositor.input_scroll = Default::default();
        let delivery = stream.issue().unwrap();
        assert!(render.compositor.input_scroll.push(
            &frame,
            11.0,
            11.0,
            4.0,
            delivery.receipt(),
            None,
        ));
        deliveries.push(delivery);
        render.compositor.input_scroll.paint(&mut frame);
        let raster = prepare(
            &mut renderer,
            &mut render,
            &frame,
            mapping(&frame, 1.0),
            false,
        );
        if change == 0 {
            original = Some(raster.unwrap().texture);
        } else {
            assert!(
                raster.is_none(),
                "one-off coverage must use the regular draw path"
            );
            assert_eq!(
                render
                    .compositor
                    .retained_scroll
                    .as_ref()
                    .unwrap()
                    .texture
                    .id(),
                original.as_ref().unwrap().id(),
                "deferred work must not allocate a raster"
            );
        }
        if change == 4 {
            // Three viewport areas of coverage require three observations.
            assert!(
                prepare(
                    &mut renderer,
                    &mut render,
                    &frame,
                    mapping(&frame, 1.0),
                    false
                )
                .is_none()
            );
            let stable = prepare(
                &mut renderer,
                &mut render,
                &frame,
                mapping(&frame, 1.0),
                false,
            )
            .unwrap();
            assert_ne!(stable.texture.id(), original.as_ref().unwrap().id());
        }
    }
}

#[test]
fn scrolling_hover_resolves_projected_slots_and_draws_live_pixels() {
    let Ok(mut renderer) = WgpuRenderer::new(None, 80, 40) else {
        assert!(std::env::var_os("NEOMACS_REQUIRE_GPU_TESTS").is_none());
        return;
    };
    let mut frame = fixture_with(|coverage| {
        let mut regions = Vec::new();
        let mut appearances = Vec::new();
        let window = coverage.content.window_id;
        for row in 0..6 {
            let bounds = FrameRect::new(10.0, 10.0 + row as f32 * 10.0, 60.0, 10.0).unwrap();
            regions.push(PresentedPointerRegion::new_owned(
                PresentedRegionId::new(Some(window), PresentedRegionKind::TextBody),
                bounds,
                None,
                Some(PointerAppearanceId::try_from(row as usize).unwrap()),
            ));
            let mode = PointerDrawMode::Face(FaceId::new(1));
            appearances.push(PresentedPointerSourceAppearance::new(
                vec![PresentedSourcePaintSpan::new(
                    PresentedPrimitiveKind::Glyph,
                    GlyphRowRole::Text,
                    DisplaySlotId {
                        window_id: window,
                        row,
                        col: 0,
                    },
                    bounds,
                )],
                mode,
                mode,
            ));
        }
        coverage.pointer_source = PresentedPointerSourceMap::new(regions, appearances);
    });
    let mut render = GuiFrameRenderState::new(
        1,
        renderer.device(),
        1.0,
        false,
        frame_time::observe_platform_now(),
    );
    render.set_current_frame(
        Some(frame.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    render.set_surface_state(
        SurfaceState::from_device_size(80, 40, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
    render.pointer_inside = true;
    render.mouse_pos = (11.0, 11.0);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    assert!(render.compositor.input_scroll.push(
        &frame,
        11.0,
        11.0,
        10.5,
        delivery.receipt(),
        None
    ));
    render.compositor.input_scroll.paint(&mut frame);
    let selection = render
        .pointer_selection_for(&frame)
        .expect("hover follows projected pixels");
    let appearance = frame
        .presented_pointer()
        .appearance(selection.appearance())
        .unwrap();
    assert_eq!(appearance.hover(), PointerDrawMode::Face(FaceId::new(1)));
    assert!(
        prepare(
            &mut renderer,
            &mut render,
            &frame,
            mapping(&frame, 1.0),
            false
        )
        .is_none(),
        "live hover must not be covered by the ordinary cached body"
    );
    let texture = renderer
        .acquire_snapshot(SnapshotSize::new(80, 40).unwrap())
        .unwrap();
    super::super::scene::render_frame_root_glyphs(
        &mut renderer,
        &mut render,
        texture.view(),
        &frame,
        mapping(&frame, 1.0),
        false,
        None,
        None,
        true,
    );
    let data = pixels(&renderer, texture.view().texture());
    let pixel = &data[(12 * 80 + 12) * 4..(12 * 80 + 12) * 4 + 4];
    assert_eq!(
        pixel,
        &[0, 255, 0, 255],
        "mouse face replaces the blue source row with green"
    );
    render.pointer_inside = false;
    assert!(render.pointer_selection_for(&frame).is_none());
}
