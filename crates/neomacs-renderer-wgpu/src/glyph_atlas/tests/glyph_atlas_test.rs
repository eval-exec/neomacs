use super::{
    AnyAtlasEntry, BitmapFontReplayCache, ComposedGlyphKey, FontconfigSubpixelOrder,
    GlyphAtlasError, GlyphKey, GlyphPixelKind, RasterizeResult, SampledSubGlyph, SingleCharGlyph,
    SubGlyph, SubpixelBin, SubpixelRequest, WgpuGlyphAtlas, effective_font_size,
    frame_font_bindings_identity, glyph_font_identity, key_uses_default_font_metrics,
    normalize_subpixel_mask, rasterize_missing_glyph_box, resolved_glyph_stream_identity,
};
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::font::{
    CharFontTable, FontFileAsset, FontOutlineAsset, FontReplay, GlyphSampling, ResolvedCharGlyph,
    ResolvedFont, ResolvedFontId, ResolvedFontIdentity, ResolvedGlyph, ResolvedGlyphId,
};
use neomacs_display_protocol::types::FaceId;

fn test_font_path(path: std::path::PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn file_replay_for(identity: &ResolvedFontIdentity) -> FontReplay {
    FontReplay::Swash {
        asset: FontOutlineAsset::File(
            FontFileAsset::from_identity(identity).expect("file-backed fixture identity"),
        ),
    }
}

fn resolved_glyph(font: u32, glyph: u32, x: f32) -> ResolvedGlyph {
    ResolvedGlyph {
        resolved_font_id: ResolvedFontId(font),
        glyph_id: ResolvedGlyphId::new(glyph),
        x,
        y: 0.0,
        x_advance: 8.0,
        cluster_start: 0,
        cluster_end: 1,
    }
}

#[test]
fn composed_stream_identity_includes_exact_font_glyph_and_position() {
    let original = resolved_glyph_stream_identity(&[resolved_glyph(1, 2, 0.0)]);

    assert_ne!(
        original,
        resolved_glyph_stream_identity(&[resolved_glyph(3, 2, 0.0)])
    );
    assert_ne!(
        original,
        resolved_glyph_stream_identity(&[resolved_glyph(1, 4, 0.0)])
    );
    assert_ne!(
        original,
        resolved_glyph_stream_identity(&[resolved_glyph(1, 2, 0.25)])
    );
}

#[test]
fn composed_atlas_identity_classifies_the_published_glyph_stream() {
    let stream_a = resolved_glyph_stream_identity(&[resolved_glyph(1, 2, 0.0)]);
    let stream_b = resolved_glyph_stream_identity(&[resolved_glyph(3, 4, 0.0)]);
    let key = |stream| ComposedGlyphKey {
        text: "A©".into(),
        face_id: FaceId::new(7),
        font_size_bits: 16.0f32.to_bits(),
        font_identity: 11,
        glyph_stream_identity: Some(stream),
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    };

    assert_ne!(key(stream_a).identity(), key(stream_b).identity());
}

#[test]
fn mixed_composition_keeps_sampling_homogeneous_atlas_parts() {
    let mask = |x, sampling| SampledSubGlyph {
        glyph: SubGlyph {
            bearing_x: x,
            bearing_y: 8.0,
            width: 1,
            height: 1,
            pixel_data: vec![255],
            pixel_kind: GlyphPixelKind::AlphaMask,
            advance_width: 1.0,
        },
        sampling,
    };
    let parts = WgpuGlyphAtlas::composite_sampled_sub_glyphs(vec![
        mask(0.0, GlyphSampling::Nearest),
        mask(1.0, GlyphSampling::Linear),
    ])
    .expect("two drawable sampling runs");

    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].sampling, GlyphSampling::Nearest);
    assert_eq!(parts[1].sampling, GlyphSampling::Linear);
}

#[test]
fn color_bitmap_pixels_always_keep_linear_sampling() {
    use neomacs_font_materializer::RasterPixels;

    assert_eq!(
        super::bitmap_fonts::bitmap_pixel_sampling(
            &RasterPixels::Mask8(vec![255]),
            GlyphSampling::Nearest,
        ),
        GlyphSampling::Nearest
    );
    assert_eq!(
        super::bitmap_fonts::bitmap_pixel_sampling(
            &RasterPixels::Bgra8(vec![0, 0, 0, 255]),
            GlyphSampling::Nearest,
        ),
        GlyphSampling::Linear
    );
}

#[test]
fn frame_font_binding_identity_changes_with_an_exact_char_binding() {
    let mut original = CharFontTable::default();
    original.entry(FaceId::new(7)).or_default().insert(
        '©',
        ResolvedCharGlyph {
            resolved_font_id: ResolvedFontId(1),
            glyph_id: ResolvedGlyphId::new(2),
            advance_px: 8.0,
        },
    );
    let mut changed = original.clone();
    changed.get_mut(&FaceId::new(7)).unwrap().insert(
        '©',
        ResolvedCharGlyph {
            resolved_font_id: ResolvedFontId(3),
            glyph_id: ResolvedGlyphId::new(4),
            advance_px: 8.0,
        },
    );

    assert_ne!(
        frame_font_bindings_identity(
            &Default::default(),
            &Default::default(),
            &original,
            &Default::default()
        ),
        frame_font_bindings_identity(
            &Default::default(),
            &Default::default(),
            &changed,
            &Default::default()
        )
    );
}

#[test]
fn frame_font_binding_identity_includes_each_faces_primary_font() {
    use neomacs_display_protocol::face::Face;

    let mut first_face = Face::new(FaceId::new(7));
    first_face.default_resolved_font_id = Some(ResolvedFontId(1));
    let mut second_face = first_face.clone();
    second_face.default_resolved_font_id = Some(ResolvedFontId(2));
    let first = [(first_face.id, first_face)].into_iter().collect();
    let second = [(second_face.id, second_face)].into_iter().collect();

    assert_ne!(
        frame_font_bindings_identity(
            &first,
            &Default::default(),
            &Default::default(),
            &Default::default()
        ),
        frame_font_bindings_identity(
            &second,
            &Default::default(),
            &Default::default(),
            &Default::default()
        )
    );
}

#[test]
fn normalize_subpixel_mask_preserves_rgb_order() {
    let out = normalize_subpixel_mask(&[10, 20, 30], 1, FontconfigSubpixelOrder::Rgb);
    assert_eq!(out, vec![10, 20, 30, 30]);
}

#[test]
fn normalize_subpixel_mask_swaps_bgr_order() {
    let out = normalize_subpixel_mask(&[10, 20, 30], 1, FontconfigSubpixelOrder::Bgr);
    assert_eq!(out, vec![30, 20, 10, 30]);
}

#[test]
fn missing_glyph_box_uses_layout_advance_and_face_line_metrics() {
    let result = rasterize_missing_glyph_box(5.0, 10.0, 7.0, 2.0);

    assert_eq!(result.width, 10);
    assert_eq!(result.height, 20);
    assert_eq!(result.advance_width, 10.0);
    assert_eq!(result.bearing_y, 14.0);
    assert_eq!(result.pixel_data.len(), 200);
    for y in 0..result.height {
        for x in 0..result.width {
            let expected = if x == 0 || x + 1 == result.width || y == 0 || y + 1 == result.height {
                255
            } else {
                0
            };
            assert_eq!(result.pixel_data[(y * result.width + x) as usize], expected);
        }
    }
}

#[test]
fn default_metrics_ignore_nondefault_face_zero_font_size() {
    let key = GlyphKey {
        charcode: 'F' as u32,
        face_id: FaceId::new(0),
        font_size_bits: 27.0_f32.to_bits(),
        font_identity: 0,
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    };

    assert!(!key_uses_default_font_metrics(&key, 13.0));
}

#[test]
fn default_metrics_accept_unspecified_default_font_size() {
    let key = GlyphKey {
        charcode: 'F' as u32,
        face_id: FaceId::new(0),
        font_size_bits: 0.0_f32.to_bits(),
        font_identity: 0,
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    };

    assert!(key_uses_default_font_metrics(&key, 13.0));
}

#[test]
fn default_metrics_accept_explicit_default_font_size() {
    let key = GlyphKey {
        charcode: 'F' as u32,
        face_id: FaceId::new(0),
        font_size_bits: 13.05_f32.to_bits(),
        font_identity: 0,
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    };

    assert!(key_uses_default_font_metrics(&key, 13.0));
}

#[cfg(target_os = "linux")]
#[test]
fn renderer_reopens_the_exact_physical_bitmap_strike_without_rescaling() {
    use neomacs_display_protocol::font::{
        FontSlantKind, ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
    };
    use neomacs_display_protocol::geometry::DeviceScale;
    use neomacs_font_materializer::{FixedFontSpacing, FontMaterializer, FontOpenRequest};

    let path = test_font_path(neomacs_test_fonts::spleen_2_2_0().pcf_gz());
    let identity = ResolvedFontIdentity::from_file(&path, 0, None);
    let asset = FontFileAsset::from_identity(&identity).expect("fixture asset");
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let opened = materializer
        .open(FontOpenRequest {
            asset: &asset,
            requested_layout_px: 16.0,
            device_scale: DeviceScale::new(1.0).unwrap(),
            selected_device_ppem_26_6: None,
            line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
            spacing: FixedFontSpacing::MonospaceOrCharacterCell,
        })
        .expect("layout-side fixed strike");
    let metrics = opened.metrics();
    let font = ResolvedFont {
        id: ResolvedFontId(41),
        identity,
        replay: opened.replay(),
        family: "Spleen".to_owned(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: metrics.height_px,
        ascent_px: metrics.ascent_px,
        descent_px: metrics.descent_px,
        space_advance_px: metrics.space_advance_px,
        glyph_advance: Default::default(),
    };

    let mut cache = BitmapFontReplayCache::new().expect("renderer bitmap replay cache");
    let rendered = cache
        .rasterize_char(&font, 'A')
        .expect("renderer must replay the exact bitmap face")
        .expect("fixture contains A");

    assert_eq!((rendered.width, rendered.height), (8, 16));
    assert_eq!(rendered.pixel_data.len(), 8 * 16);
    assert_eq!(rendered.advance_width, 8.0);
    assert_eq!(rendered.bearing_x, 0.0);
    assert_eq!(rendered.bearing_y, 12.0);
    assert_eq!(
        rendered.sampling,
        neomacs_display_protocol::font::GlyphSampling::Nearest
    );
    assert!(rendered.pixel_data.contains(&255));
}

#[test]
fn effective_font_size_resolves_zero_sentinel_to_default() {
    // An explicit, positive size is honored verbatim.
    assert_eq!(effective_font_size(Some(27.0), 13.0), 27.0);
    // font_size 0.0 is the "unspecified" sentinel (see
    // key_uses_default_font_metrics): a face that inherits the frame default
    // font (minibuffer/echo-area) carries it, and it MUST resolve to the
    // default. Feeding 0 into cosmic-text's Metrics panics ("line height
    // cannot be 0").
    assert_eq!(effective_font_size(Some(0.0), 13.0), 13.0);
    // A degenerate (negative / non-finite) size is equally unusable and falls
    // back to the default.
    assert_eq!(effective_font_size(Some(-5.0), 13.0), 13.0);
    assert_eq!(effective_font_size(Some(f32::NAN), 13.0), 13.0);
    // A missing face resolves to the default as before.
    assert_eq!(effective_font_size(None, 13.0), 13.0);
}

#[test]
fn rasterize_result_to_pixels_rejects_mismatched_alpha_length() {
    let result = RasterizeResult {
        width: 2,
        height: 2,
        pixel_data: vec![255],
        bearing_x: 0.0,
        bearing_y: 0.0,
        pixel_kind: GlyphPixelKind::AlphaMask,
        advance_width: 0.0,
        sampling: neomacs_display_protocol::font::GlyphSampling::Linear,
    };

    let err = WgpuGlyphAtlas::rasterize_result_to_pixels(&result).unwrap_err();
    assert!(matches!(
        err,
        GlyphAtlasError::PixelDataLength {
            expected: 4,
            actual: 1,
            ..
        }
    ));
}

#[test]
fn rasterize_result_to_pixels_rejects_zero_size() {
    let result = RasterizeResult {
        width: 0,
        height: 2,
        pixel_data: Vec::new(),
        bearing_x: 0.0,
        bearing_y: 0.0,
        pixel_kind: GlyphPixelKind::AlphaMask,
        advance_width: 0.0,
        sampling: neomacs_display_protocol::font::GlyphSampling::Linear,
    };

    let err = WgpuGlyphAtlas::rasterize_result_to_pixels(&result).unwrap_err();
    assert_eq!(err, GlyphAtlasError::ZeroSize);
}

#[test]
fn glyph_font_identity_discriminates_resolved_font_id() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::font::ResolvedFontId;

    let mut a = Face::new(FaceId::new(5));
    a.font_family = "Mono".to_string();
    let mut b = a.clone();

    // Same request fields, different realized fonts -> different identity.
    a.default_resolved_font_id = Some(ResolvedFontId(1));
    b.default_resolved_font_id = Some(ResolvedFontId(2));
    assert_ne!(glyph_font_identity(Some(&a)), glyph_font_identity(Some(&b)));

    // Same realized font -> same identity.
    b.default_resolved_font_id = Some(ResolvedFontId(1));
    assert_eq!(glyph_font_identity(Some(&a)), glyph_font_identity(Some(&b)));

    // Unresolved differs from resolved.
    b.default_resolved_font_id = None;
    assert_ne!(glyph_font_identity(Some(&a)), glyph_font_identity(Some(&b)));

    // Emergency non-ASCII fallback semantics are also part of an unresolved
    // face's raster identity.
    a.default_resolved_font_id = None;
    b.font_family = "Different Effective Family".to_string();
    assert_ne!(glyph_font_identity(Some(&a)), glyph_font_identity(Some(&b)));
}

fn try_test_device_and_atlas() -> Option<(wgpu::Device, wgpu::Queue, WgpuGlyphAtlas)> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("glyph-atlas-font-boundary-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .ok()?;
    let atlas = WgpuGlyphAtlas::new(&device);
    Some((device, queue, atlas))
}

fn try_test_atlas() -> Option<WgpuGlyphAtlas> {
    try_test_device_and_atlas().map(|(_, _, mut atlas)| {
        let frame = neomacs_display_protocol::FrameGlyphBuffer::default();
        atlas.set_current_frame_fonts(frame.font_bindings());
        atlas
    })
}

#[test]
fn frame_font_installation_invalidates_atlas_on_catalog_generation_change() {
    let Some((_, _, mut atlas)) = try_test_device_and_atlas() else {
        return;
    };
    let mut frame = neomacs_display_protocol::FrameGlyphBuffer::default();
    let initial = neomacs_display_protocol::font::FontCatalogGeneration::default();

    let before_first_frame = atlas.eviction_generation();
    atlas.set_current_frame_fonts(frame.font_bindings());
    let established = atlas.eviction_generation();
    assert!(established > before_first_frame);
    atlas.set_current_frame_fonts(frame.font_bindings());
    assert_eq!(atlas.eviction_generation(), established);

    frame.font_catalog_generation = initial.next();
    atlas.set_current_frame_fonts(frame.font_bindings());
    assert!(atlas.eviction_generation() > established);
}

#[test]
fn atlas_sampling_policy_selects_distinct_wgpu_bind_groups() {
    use neomacs_display_protocol::font::GlyphSampling;

    let Some((device, queue, mut atlas)) = try_test_device_and_atlas() else {
        return;
    };
    let result = |sampling| RasterizeResult {
        width: 2,
        height: 2,
        pixel_data: vec![0, 85, 170, 255],
        bearing_x: 0.0,
        bearing_y: 2.0,
        pixel_kind: GlyphPixelKind::AlphaMask,
        advance_width: 2.0,
        sampling,
    };
    let linear = atlas
        .rasterize_result_to_atlas_entry(&device, &queue, &result(GlyphSampling::Linear))
        .expect("linear entry");
    let nearest = atlas
        .rasterize_result_to_atlas_entry(&device, &queue, &result(GlyphSampling::Nearest))
        .expect("nearest entry");

    assert_eq!(linear.sampling(), GlyphSampling::Linear);
    assert_eq!(nearest.sampling(), GlyphSampling::Nearest);
    assert!(
        !std::ptr::eq(
            atlas.atlas_bind_group(linear).expect("linear bind group"),
            atlas.atlas_bind_group(nearest).expect("nearest bind group"),
        ),
        "the GPU sampling boundary must not blur fixed bitmap masks with the linear sampler"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn renderer_keeps_missing_ascii_on_primary_font() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::font::{FontSlantKind, ResolvedFont, ResolvedFontId};
    use neomacs_layout_engine::font::metrics::FontMetricsService;

    let requested_family = "Symbols Nerd Font Mono";
    let Some(platform) = neomacs_layout_engine::font::fontconfig::find_font_for_spec(
        Some(requested_family),
        None,
        None,
        None,
        None,
        None,
    ) else {
        return;
    };
    // `ResolvedFont::family` deliberately preserves the requested family,
    // even when Fontconfig substitutes another face.  This test needs the
    // actual symbols-only font because a substituted text font normally has
    // an ASCII space glyph.
    if !platform.family.eq_ignore_ascii_case(requested_family) {
        return;
    }

    let Some(resolved) =
        FontMetricsService::new().resolved_font_for_face(requested_family, 400, false, 10.0)
    else {
        return;
    };
    assert_eq!(resolved.identity.file_path, platform.file);
    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let family = resolved.family;
    let weight = resolved.weight;
    let identity = resolved.identity;
    let id = ResolvedFontId(1);
    let font = ResolvedFont {
        id,
        identity: identity.clone(),
        replay: file_replay_for(&identity),
        family,
        full_name: None,
        postscript_name: identity.postscript_name.clone(),
        weight,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 10.0,
        ascent_px: 8.0,
        descent_px: 2.0,
        space_advance_px: 5.0,
        glyph_advance: Default::default(),
    };
    atlas.install_frame_fonts(
        &Default::default(),
        &[(id, font)].into_iter().collect(),
        &Default::default(),
        &Default::default(),
    );
    let mut face = Face::new(FaceId::new(7));
    face.font_family = requested_family.to_string();
    face.font_size = 10.0;
    face.font_ascent = 8;
    face.font_descent = 2;
    face.default_resolved_font_id = Some(id);

    assert!(matches!(
        atlas.try_fast_single_char_glyph(' ', Some(&face)),
        Some(SingleCharGlyph::MissingPrimaryAscii { advance_width: 5.0 })
    ));
    let result = atlas
        .rasterize_glyph(
            ' ',
            Some(&face),
            SubpixelBin::Zero,
            SubpixelBin::Zero,
            false,
        )
        .expect("missing ASCII renders GNU's empty box");
    assert_eq!(result.width, 5);
    assert_eq!(result.advance_width, 5.0);
    assert_eq!(result.height, 10);
    assert!(result.pixel_data.contains(&255));
}

#[cfg(unix)]
#[test]
fn renderer_uses_layouts_published_fixed_cell_advance() {
    use neomacs_display_protocol::face::Face;
    use neomacs_layout_engine::font::metrics::FontMetricsService;

    let Some(resolved) =
        FontMetricsService::new().resolved_font_for_face("JetBrains Mono", 400, false, 14.0)
    else {
        return;
    };
    let Some(cell) = resolved.glyph_advance.cell_advance_px() else {
        return;
    };
    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let id = resolved.id;
    atlas.install_frame_fonts(
        &Default::default(),
        &[(id, resolved)].into_iter().collect(),
        &Default::default(),
        &Default::default(),
    );
    let mut face = Face::new(FaceId::new(8));
    face.font_size = 14.0;
    face.default_resolved_font_id = Some(id);

    let Some(SingleCharGlyph::Resolved(glyph)) = atlas.try_fast_single_char_glyph('!', Some(&face))
    else {
        panic!("the exact primary face must cover ASCII punctuation");
    };
    assert_eq!(glyph.x_advance, cell);
}

#[cfg(unix)]
#[test]
#[tracing_test::traced_test]
fn renderer_replays_named_instance_weight_on_the_exact_raw_face() {
    use cosmic_text::{Buffer, Metrics, Shaping};
    use neomacs_display_protocol::font::{FontSlantKind, ResolvedFont, ResolvedFontId};
    use neomacs_layout_engine::font::metrics::FontMetricsService;

    let Some(resolved) =
        FontMetricsService::new().resolved_font_for_face("Noto Sans", 700, false, 18.0)
    else {
        tracing::info!("skipping: Noto Sans Bold is not installed");
        return;
    };
    if resolved.identity.file_path.is_none() {
        tracing::info!("skipping: Fontconfig match has no file");
        return;
    }
    let Some(mut atlas) = try_test_atlas() else {
        tracing::info!("skipping: no headless wgpu adapter");
        return;
    };
    let family = resolved.family;
    let weight = resolved.weight;
    let identity = resolved.identity;
    let font = ResolvedFont {
        id: ResolvedFontId(1),
        identity: identity.clone(),
        replay: file_replay_for(&identity),
        family,
        full_name: None,
        postscript_name: identity.postscript_name.clone(),
        weight,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 18.0,
        ascent_px: 0.0,
        descent_px: 0.0,
        space_advance_px: 0.0,
        glyph_advance: Default::default(),
    };

    let attrs = atlas
        .exact_attrs_for_resolved_font(&font)
        .expect("renderer must open the layout-resolved face");
    let mut buffer = Buffer::new(&mut atlas.font_system, Metrics::new(18.0, 24.0));
    buffer.set_size(&mut atlas.font_system, Some(72.0), Some(36.0));
    buffer.set_text(&mut atlas.font_system, "M", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut atlas.font_system, false);
    let cache_key = buffer
        .layout_runs()
        .find_map(|run| run.glyphs.first())
        .expect("shaped glyph")
        .physical((0.0, 0.0), 1.0)
        .cache_key;
    let face = atlas
        .font_system
        .db()
        .face(cache_key.font_id)
        .expect("renderer fontdb face");

    assert_eq!(face.index, identity.file_face_index());
    assert_eq!(cache_key.font_weight.0, 700);
    assert!(atlas.render_cache_key_image(cache_key, false).is_some());
}

#[test]
fn renderer_exact_attrs_reject_an_unopenable_identity() {
    use neomacs_display_protocol::font::{
        FontSlantKind, ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
    };

    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let identity = ResolvedFontIdentity::from_file("/neomacs/missing/font.ttf", 0, None);
    let font = ResolvedFont {
        id: ResolvedFontId(1),
        identity: identity.clone(),
        replay: file_replay_for(&identity),
        family: "missing".to_string(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 14.0,
        ascent_px: 0.0,
        descent_px: 0.0,
        space_advance_px: 0.0,
        glyph_advance: Default::default(),
    };

    assert!(atlas.exact_attrs_for_resolved_font(&font).is_none());
}

#[test]
fn renderer_replays_the_same_decoded_woff_face_as_layout() {
    use neomacs_display_protocol::font::{
        FontSlantKind, ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
    };

    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let path = test_font_path(neomacs_test_fonts::spleen_2_2_0().woff());
    let id = ResolvedFontId(73);
    let identity = ResolvedFontIdentity::from_file(&path, 0, None);
    let font = ResolvedFont {
        id,
        identity: identity.clone(),
        replay: file_replay_for(&identity),
        family: "Spleen 8x16".to_owned(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 12.0,
        descent_px: 4.0,
        space_advance_px: 8.0,
        glyph_advance: Default::default(),
    };
    atlas.install_frame_fonts(
        &Default::default(),
        &[(id, font.clone())].into_iter().collect(),
        &Default::default(),
        &Default::default(),
    );

    assert!(atlas.exact_attrs_for_resolved_font(&font).is_some());
    let local_id = atlas
        .local_fontdb_id_for(id)
        .expect("renderer keeps the decoded exact face id");
    let source = &atlas
        .font_system
        .db()
        .face(local_id)
        .expect("renderer keeps the decoded exact face")
        .source;
    assert!(matches!(
        source,
        fontdb::Source::SharedFile(source_path, _) if source_path == &path
    ));
}

#[test]
fn renderer_replays_a_native_memory_asset_in_its_own_font_system() {
    use cosmic_text::{Buffer, Metrics, Shaping};
    use neomacs_display_protocol::font::{
        FontBackendKind, FontMemoryAsset, FontSlantKind, ResolvedFont, ResolvedFontId,
    };
    use std::sync::Arc;

    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let path = neomacs_test_fonts::spleen_2_2_0().woff();
    let mut source_db = fontdb::Database::new();
    let source_ids = neomacs_font_materializer::FontFileCache::open_file(
        &mut source_db,
        &path.to_string_lossy(),
        0,
    )
    .expect("decode downloaded WOFF fixture");
    let bytes = source_ids
        .into_iter()
        .find_map(|id| match &source_db.face(id)?.source {
            fontdb::Source::SharedFile(_, bytes) => Some(bytes.as_ref().as_ref().to_vec()),
            fontdb::Source::File(_) | fontdb::Source::Binary(_) => None,
        })
        .expect("decoded standalone SFNT bytes");
    let identity = ResolvedFontIdentity::from_memory(
        FontBackendKind::CoreText,
        "coretext:test:Spleen".to_owned(),
        0,
        Some("Spleen-8x16".to_owned()),
    );
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new(identity.stable_key.clone(), Arc::new(bytes), 0)
            .expect("native-memory fixture"),
    );
    let id = ResolvedFontId(74);
    let font = ResolvedFont {
        id,
        identity,
        replay: FontReplay::Swash { asset },
        family: "Spleen 8x16".to_owned(),
        full_name: None,
        postscript_name: Some("Spleen-8x16".to_owned()),
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 12.0,
        descent_px: 4.0,
        space_advance_px: 8.0,
        glyph_advance: Default::default(),
    };
    atlas.install_frame_fonts(
        &Default::default(),
        &[(id, font.clone())].into_iter().collect(),
        &Default::default(),
        &Default::default(),
    );

    let attrs = atlas
        .exact_attrs_for_resolved_font(&font)
        .expect("renderer pins native-memory font");
    let local_id = atlas
        .local_fontdb_id_for(id)
        .expect("renderer records its local native-memory face id");
    let mut buffer = Buffer::new(&mut atlas.font_system, Metrics::new(16.0, 20.0));
    buffer.set_size(&mut atlas.font_system, Some(64.0), Some(32.0));
    buffer.set_text(&mut atlas.font_system, "A", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut atlas.font_system, false);
    let cache_key = buffer
        .layout_runs()
        .find_map(|run| run.glyphs.first())
        .expect("shape native-memory glyph")
        .physical((0.0, 0.0), 1.0)
        .cache_key;

    assert_eq!(cache_key.font_id, local_id);
    assert!(matches!(
        atlas
            .font_system
            .db()
            .face(local_id)
            .map(|face| &face.source),
        Some(fontdb::Source::Binary(_))
    ));
    assert!(atlas.render_cache_key_image(cache_key, false).is_some());
}

#[test]
fn reused_resolved_font_id_invalidates_renderer_identity_caches() {
    use neomacs_display_protocol::font::{
        FontSlantKind, ResolvedFont, ResolvedFontId, ResolvedFontIdentity, ResolvedFontTable,
    };

    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let id = ResolvedFontId(9);
    let font = |path: &str| {
        let identity = ResolvedFontIdentity::from_file(path, 0, None);
        ResolvedFont {
            id,
            identity: identity.clone(),
            replay: file_replay_for(&identity),
            family: "Fixture".to_string(),
            full_name: None,
            postscript_name: None,
            weight: 400,
            slant: FontSlantKind::Normal,
            width: 5,
            pixel_size: 14.0,
            ascent_px: 0.0,
            descent_px: 0.0,
            space_advance_px: 0.0,
            glyph_advance: Default::default(),
        }
    };
    let mut first = ResolvedFontTable::default();
    first.insert(id, font("/fonts/first.ttf"));
    atlas.install_frame_fonts(
        &Default::default(),
        &first,
        &Default::default(),
        &Default::default(),
    );
    atlas.resolved_fontdb_ids.insert(id, None);

    let mut replacement = ResolvedFontTable::default();
    replacement.insert(id, font("/fonts/replacement.ttf"));
    atlas.install_frame_fonts(
        &Default::default(),
        &replacement,
        &Default::default(),
        &Default::default(),
    );

    assert!(!atlas.resolved_fontdb_ids.contains_key(&id));
    assert_eq!(
        atlas.frame_fonts.get(&id).unwrap().identity,
        replacement.get(&id).unwrap().identity
    );
}

/// Install the reporter's COLRv1 face as an exact memory font.
fn install_colrv1_memory_face(atlas: &mut WgpuGlyphAtlas) -> (ResolvedFontId, ResolvedFont) {
    use neomacs_display_protocol::font::{
        FontBackendKind, FontMemoryAsset, FontSlantKind, ResolvedFontTable,
    };
    use std::sync::Arc;

    let bytes = std::fs::read(neomacs_test_fonts::noto_color_emoji_colrv1())
        .expect("downloaded COLRv1 fixture");
    let identity = ResolvedFontIdentity::from_memory(
        FontBackendKind::Fontconfig,
        "freeTypeFontconfig:test:Noto Color Emoji".to_owned(),
        0,
        Some("NotoColorEmoji".to_owned()),
    );
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new(identity.stable_key.clone(), Arc::new(bytes), 0)
            .expect("COLRv1 memory fixture"),
    );
    let id = ResolvedFontId(542);
    let font = ResolvedFont {
        id,
        identity,
        replay: FontReplay::Swash { asset },
        family: "Noto Color Emoji".to_owned(),
        full_name: Some("Noto Color Emoji".to_owned()),
        postscript_name: Some("NotoColorEmoji".to_owned()),
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 12.0,
        descent_px: 4.0,
        space_advance_px: 8.0,
        glyph_advance: Default::default(),
    };
    let mut fonts = ResolvedFontTable::default();
    fonts.insert(id, font.clone());
    atlas.install_frame_fonts(
        &Default::default(),
        &fonts,
        &Default::default(),
        &Default::default(),
    );
    (id, font)
}

/// Shape one emoji with `attrs` and return the leading glyph's cache key.
fn shape_emoji(
    atlas: &mut WgpuGlyphAtlas,
    attrs: &cosmic_text::Attrs<'static>,
) -> cosmic_text::CacheKey {
    shape_single(atlas, attrs, "\u{1F347}")
}

/// Shape `text` with `attrs` and return the leading glyph's cache key.
fn shape_single(
    atlas: &mut WgpuGlyphAtlas,
    attrs: &cosmic_text::Attrs<'static>,
    text: &str,
) -> cosmic_text::CacheKey {
    use cosmic_text::{Buffer, Metrics, Shaping};
    let mut buffer = Buffer::new(&mut atlas.font_system, Metrics::new(16.0, 20.0));
    buffer.set_size(&mut atlas.font_system, Some(64.0), Some(32.0));
    buffer.set_text(&mut atlas.font_system, text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut atlas.font_system, false);
    buffer
        .layout_runs()
        .find_map(|run| run.glyphs.first())
        .expect("shape the glyph")
        .physical((0.0, 0.0), 1.0)
        .cache_key
}

/// Install one variation instance of the variable COLRv1 face.
fn install_nabla_memory_face(
    atlas: &mut WgpuGlyphAtlas,
    id: u32,
    coords: &[(&[u8; 4], f32)],
) -> (ResolvedFontId, ResolvedFont) {
    use neomacs_display_protocol::font::{
        FontBackendKind, FontMemoryAsset, FontSlantKind, FontVariationCoord, ResolvedFontTable,
    };
    use std::sync::Arc;

    let bytes =
        std::fs::read(neomacs_test_fonts::nabla_color_colrv1()).expect("downloaded Nabla fixture");
    let variations = coords
        .iter()
        .map(|(tag, value)| {
            FontVariationCoord::try_new(u32::from_be_bytes(**tag), *value).expect("finite value")
        })
        .collect::<Vec<_>>();
    let identity = ResolvedFontIdentity::from_native_with_variations(
        FontBackendKind::Fontconfig,
        format!("freeTypeFontconfig:test:Nabla-{id}"),
        0,
        Some("Nabla".to_owned()),
        variations,
    );
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new(identity.stable_key.clone(), Arc::new(bytes), 0)
            .expect("Nabla memory fixture"),
    );
    let resolved_id = ResolvedFontId(id);
    let font = ResolvedFont {
        id: resolved_id,
        identity,
        replay: FontReplay::Swash { asset },
        family: "Nabla".to_owned(),
        full_name: Some("Nabla".to_owned()),
        postscript_name: Some("Nabla".to_owned()),
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 12.0,
        descent_px: 4.0,
        space_advance_px: 8.0,
        glyph_advance: Default::default(),
    };
    let mut fonts = ResolvedFontTable::default();
    fonts.insert(resolved_id, font.clone());
    atlas.install_frame_fonts(
        &Default::default(),
        &fonts,
        &Default::default(),
        &Default::default(),
    );
    (resolved_id, font)
}

/// A resolved variation instance must reach the color painter: the variable
/// COLRv1 face paints different layers for different `EDPT` coordinates, and
/// the two instances lose their distinction only if the request drops them.
#[test]
fn colrv1_variation_instances_paint_differently() {
    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let (_, thin) = install_nabla_memory_face(&mut atlas, 553, &[(b"EDPT", 0.0)]);
    let (_, thick) = install_nabla_memory_face(&mut atlas, 554, &[(b"EDPT", 100.0)]);

    let thin_attrs = atlas
        .exact_attrs_for_resolved_font(&thin)
        .expect("renderer pins the thin instance");
    let thick_attrs = atlas
        .exact_attrs_for_resolved_font(&thick)
        .expect("renderer pins the thick instance");
    let thin_key = shape_single(&mut atlas, &thin_attrs, "A");
    let thick_key = shape_single(&mut atlas, &thick_attrs, "A");
    assert_ne!(thin_key, thick_key, "instances must not share a key");

    let thin_image = atlas
        .glyph_image(thin_key, None, 16.0, false)
        .expect("the thin instance paints");
    let thick_image = atlas
        .glyph_image(thick_key, None, 16.0, false)
        .expect("the thick instance paints");
    assert_eq!(thin_image.content, super::RasterContent::Color);
    assert!(
        thin_image.data.chunks_exact(4).any(|pixel| pixel[3] > 0),
        "the thin instance has no ink"
    );
    assert!(
        thick_image.data.chunks_exact(4).any(|pixel| pixel[3] > 0),
        "the thick instance has no ink"
    );
    assert_ne!(
        thin_image.data, thick_image.data,
        "the two EDPT instances painted identically: the request dropped the variation"
    );
}

/// Regression for issue #542: a COLRv1 face materializes through fontdb, but
/// nothing in the Swash source chain can paint it — its emoji glyphs have
/// paint graphs in `BaseGlyphList` and empty `glyf` outlines, so the Swash
/// chain returns an empty bitmap and the cell stays blank. The color stage
/// must paint it before Swash is consulted.
#[test]
fn colrv1_memory_asset_paints_through_the_color_stage() {
    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let (id, font) = install_colrv1_memory_face(&mut atlas);
    let attrs = atlas
        .exact_attrs_for_resolved_font(&font)
        .expect("renderer pins the COLRv1 face");
    let local_id = atlas
        .local_fontdb_id_for(id)
        .expect("renderer records its local COLRv1 face id");
    let cache_key = shape_emoji(&mut atlas, &attrs);

    assert_eq!(
        cache_key.font_id, local_id,
        "shaping must use the pinned face"
    );

    // The bug: Swash cannot paint this face's emoji glyphs at all.
    let swash_only = atlas.render_cache_key_image(cache_key, false);
    assert!(
        swash_only
            .as_ref()
            .is_none_or(|image| image.width == 0 || image.height == 0),
        "the Swash chain is expected to produce nothing for a COLRv1 emoji glyph"
    );

    let image = atlas
        .glyph_image(cache_key, None, 16.0, false)
        .expect("the color stage paints the emoji glyph");
    assert_eq!(image.content, super::RasterContent::Color);
    assert!(
        (12..=20).contains(&image.width) && (12..=20).contains(&image.height),
        "16 px emoji rasterized {}x{}",
        image.width,
        image.height
    );
    let painted = image
        .data
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(painted > 0, "the emoji raster has no visible pixels");
}

/// Shaping pins a face under a synthetic family without recording a fontdb id
/// in the atlas's resolved-id table, so glyphs shaped through it carry a
/// DIFFERENT fontdb id than the one `local_fontdb_id_for` returns.  That is the
/// path `rasterize_text` takes (via `face_to_attrs_for_text`), and the color
/// stage must resolve the face's asset from the shaping pin as well — otherwise
/// a COLRv1 face paints on the fast path and stays blank here.
#[test]
fn colrv1_face_pinned_for_shaping_still_paints_in_color() {
    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let (_id, font) = install_colrv1_memory_face(&mut atlas);
    let attrs = atlas
        .exact_attrs_for_resolved_font(&font)
        .expect("renderer pins the COLRv1 face for shaping");
    let cache_key = shape_emoji(&mut atlas, &attrs);

    let image = atlas
        .glyph_image(cache_key, None, 16.0, false)
        .expect("a shaping-pinned color face paints");
    assert_eq!(image.content, super::RasterContent::Color);
    assert!(image.data.chunks_exact(4).any(|pixel| pixel[3] > 0));
}

/// A face that arrived through semantic fallback (`prime_file`) carries no
/// recorded asset, so the color stage derives one from fontdb's own source: a
/// face fontdb loaded from a file still names that file, which is all the
/// rasterizer needs to classify and open it.
#[test]
fn colrv1_file_face_paints_through_a_derived_asset() {
    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let path = neomacs_test_fonts::noto_color_emoji_colrv1();
    let ids = atlas
        .font_system
        .db_mut()
        .load_font_source(fontdb::Source::File(path.into()));
    let fontdb_id = *ids.first().expect("fontdb loads the fixture");
    let glyph = atlas
        .font_system
        .get_font(fontdb_id, fontdb::Weight::NORMAL)
        .map(|font| font.as_swash().charmap().map('\u{1F347}'))
        .expect("the fixture face is openable");

    let image = atlas
        .color_glyph_image(
            fontdb_id,
            glyph,
            16.0,
            None,
            cosmic_text::SubpixelBin::Zero,
            cosmic_text::SubpixelBin::Zero,
        )
        .expect("a file face paints through a derived asset");
    assert_eq!(image.content, super::RasterContent::Color);
}

/// A color face's paint graph may resolve palette index 0xFFFF to the text
/// foreground, and the resulting color raster is drawn untinted, so two faces
/// that differ only in colour must not share one cached raster.  Mask glyphs
/// are unaffected: their colour is applied when the mask is drawn.
#[test]
fn color_rasters_are_not_shared_across_foregrounds() {
    let Some((device, queue, mut atlas)) = try_test_device_and_atlas() else {
        return;
    };
    atlas.set_current_frame_fonts(
        neomacs_display_protocol::FrameGlyphBuffer::default().font_bindings(),
    );
    let (id, _font) = install_colrv1_memory_face(&mut atlas);
    assert_eq!(
        super::mix_foreground(7, 1),
        super::mix_foreground(7, 1),
        "the mixer is deterministic"
    );
    assert_ne!(
        super::mix_foreground(7, 1),
        super::mix_foreground(7, 2),
        "different foregrounds must produce different identities"
    );

    let face_with = |foreground: neomacs_display_protocol::Color| {
        let mut face = Face::new(FaceId::new(11));
        face.font_family = "Noto Color Emoji".to_owned();
        face.font_size = 16.0;
        face.default_resolved_font_id = Some(id);
        face.foreground = foreground;
        face
    };
    let key = GlyphKey {
        charcode: '\u{1F347}' as u32,
        face_id: FaceId::new(11),
        font_size_bits: 16.0f32.to_bits(),
        font_identity: 0,
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    };
    let red = atlas
        .get_or_create_atlas(
            &device,
            &queue,
            &key,
            Some(&face_with(neomacs_display_protocol::Color::rgb(
                1.0, 0.0, 0.0,
            ))),
            SubpixelRequest::Disabled,
        )
        .expect("a red-face raster");
    let blue = atlas
        .get_or_create_atlas(
            &device,
            &queue,
            &key,
            Some(&face_with(neomacs_display_protocol::Color::rgb(
                0.0, 0.0, 1.0,
            ))),
            SubpixelRequest::Disabled,
        )
        .expect("a blue-face raster");
    assert!(
        matches!(red.entry, AnyAtlasEntry::Color(_)),
        "the fast path must produce a color entry: {:?}",
        red.entry
    );
    assert!(matches!(blue.entry, AnyAtlasEntry::Color(_)));
    assert_eq!(
        atlas.len(),
        2,
        "two foregrounds must not share one cached color raster"
    );
}

/// The other half of the foreground rule: a face whose font has no color
/// source resolves no palette index to the text foreground, so its glyph
/// identities — and therefore its atlas and row-reuse hit rates — must not
/// depend on the foreground colour.
#[test]
fn mask_glyph_identities_ignore_the_foreground() {
    use neomacs_display_protocol::font::{FontSlantKind, ResolvedFontTable};

    let Some(mut atlas) = try_test_atlas() else {
        return;
    };
    let (color_id, _color_font) = install_colrv1_memory_face(&mut atlas);
    let outline_identity = ResolvedFontIdentity::from_file(
        &test_font_path(neomacs_test_fonts::mplus_1_code_thin().to_path_buf()),
        0,
        None,
    );
    let outline_id = ResolvedFontId(543);
    let outline_font = ResolvedFont {
        id: outline_id,
        replay: file_replay_for(&outline_identity),
        identity: outline_identity,
        family: "M PLUS 1 Code".to_owned(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 12.0,
        descent_px: 4.0,
        space_advance_px: 8.0,
        glyph_advance: Default::default(),
    };
    let mut fonts = ResolvedFontTable::default();
    fonts.insert(color_id, atlas.frame_fonts.get(&color_id).unwrap().clone());
    fonts.insert(outline_id, outline_font);
    atlas.install_frame_fonts(
        &Default::default(),
        &fonts,
        &Default::default(),
        &Default::default(),
    );

    let face_for = |resolved: ResolvedFontId, foreground: neomacs_display_protocol::Color| {
        let mut face = Face::new(FaceId::new(21));
        face.font_family = "Fixture".to_owned();
        face.font_size = 16.0;
        face.default_resolved_font_id = Some(resolved);
        face.foreground = foreground;
        face
    };
    let red = neomacs_display_protocol::Color::rgb(1.0, 0.0, 0.0);
    let blue = neomacs_display_protocol::Color::rgb(0.0, 0.0, 1.0);

    assert_ne!(
        atlas.glyph_font_identity_for_char(Some(&face_for(color_id, red)), '\u{1F347}'),
        atlas.glyph_font_identity_for_char(Some(&face_for(color_id, blue)), '\u{1F347}'),
        "a color face's glyph identity must separate foregrounds"
    );
    assert_eq!(
        atlas.glyph_font_identity_for_char(Some(&face_for(outline_id, red)), 'A'),
        atlas.glyph_font_identity_for_char(Some(&face_for(outline_id, blue)), 'A'),
        "a mask glyph's identity must ignore the foreground"
    );
}

/// Resolved coordinates win over the weight fallback; an instance without
/// coordinates keeps the one axis Swash scaling applies.
#[test]
fn color_variation_settings_prefer_resolved_coordinates() {
    use neomacs_display_protocol::font::{FontVariationCoord, FontVariationSet};

    let tag = |bytes: &[u8; 4]| neomacs_font_materializer::Tag::from_bytes(bytes);
    let empty = FontVariationSet::default();
    assert_eq!(
        super::color_variation_settings(&empty, 700),
        vec![(tag(b"wght"), 700.0)]
    );

    let resolved = FontVariationSet::new(vec![
        FontVariationCoord::try_new(u32::from_be_bytes(*b"EDPT"), 42.5).expect("finite value"),
    ]);
    assert_eq!(
        super::color_variation_settings(&resolved, 700),
        vec![(tag(b"EDPT"), 42.5)]
    );
}
