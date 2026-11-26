use super::*;
use neomacs_display_protocol::font::{ResolvedFontIdentity, ResolvedGlyphId};

fn fixture_font(file: &std::path::Path, family: &str) -> ResolvedFont {
    let identity = ResolvedFontIdentity::from_file(file.to_str().unwrap(), 0, None);
    ResolvedFont {
        id: ResolvedFontId(817),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(FontFileAsset::from_identity(&identity).unwrap()),
        },
        identity,
        family: family.into(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size: 16.0,
        ascent_px: 15.0,
        descent_px: 4.0,
        space_advance_px: 5.0,
        glyph_advance: Default::default(),
        spacing: None,
    }
}

pub(super) fn blank_font() -> ResolvedFont {
    fixture_font(
        neomacs_test_fonts::dejavu_2_37().sans_regular(),
        "DejaVu Sans",
    )
}

fn face_and_key() -> (Face, GlyphKey) {
    let mut face = Face::new(FaceId::new(817));
    face.default_resolved_font_id = Some(ResolvedFontId(817));
    face.font_size = 16.0;
    face.font_family = "DejaVu Sans".into();
    let key = GlyphKey {
        charcode: '\u{2800}' as u32,
        face_id: face.id,
        font_size_bits: 16.0f32.to_bits(),
        font_identity: glyph_font_identity(Some(&face)),
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    };
    (face, key)
}

fn lookup(
    atlas: &mut WgpuGlyphAtlas,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    key: &GlyphKey,
    face: &Face,
) -> Result<Option<GlyphAtlasHandle>, GlyphAtlasError> {
    atlas.get_or_create_atlas_result(device, queue, key, Some(face), SubpixelRequest::Disabled)
}

#[test]
fn braille_blank_is_a_successful_empty_raster_and_never_retried() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let font = blank_font();
    atlas.install_resolved_fonts(std::iter::once(&font));
    let (face, key) = face_and_key();
    let local = atlas.local_fontdb_id_for(font.id).unwrap();
    let opened = atlas
        .font_system
        .get_font(local, fontdb::Weight::NORMAL)
        .unwrap();
    let glyph_id = opened.as_swash().charmap().map('\u{2800}');
    assert_ne!(glyph_id, 0, "blank is covered by the exact pinned font");
    let glyph = ResolvedGlyph {
        resolved_font_id: font.id,
        glyph_id: ResolvedGlyphId::new(u32::from(glyph_id)),
        x: 0.0,
        y: 0.0,
        x_advance: 12.0,
        cluster_start: 0,
        cluster_end: 3,
    };
    let outcome = atlas
        .rasterize_resolved_cluster(
            std::slice::from_ref(&glyph),
            Some(&face),
            16.0,
            SubpixelBin::Zero,
            SubpixelBin::Zero,
            false,
        )
        .unwrap();
    assert!(matches!(outcome, Rasterized::Empty { advance_width } if advance_width == 12.0));
    let before = atlas.rasterization_attempts_total();
    let fallback = atlas.fallback_shape_attempts_total();
    for _ in 0..20 {
        assert!(
            lookup(&mut atlas, &device, &queue, &key, &face)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(atlas.rasterization_attempts_total() - before, 1);
    assert_eq!(atlas.fallback_shape_attempts_total(), fallback);
    assert!(atlas.atlas_cache.is_empty(), "no zero-sized GPU texture");
    assert!(matches!(atlas.non_drawing_cache.values().next(),
        Some(NonDrawingOutcome::Empty { advance_width, .. }) if *advance_width > 0.0));
    atlas
        .frame_shaped_clusters
        .entry(face.id)
        .or_default()
        .insert("\u{2800}\u{2800}".into(), vec![glyph.clone(), glyph]);
    for _ in 0..20 {
        assert!(
            atlas
                .get_or_create_composed_atlas_result(
                    &device,
                    &queue,
                    "\u{2800}\u{2800}",
                    face.id,
                    16.0f32.to_bits(),
                    Some(&face),
                    SubpixelBin::Zero,
                    SubpixelBin::Zero,
                    SubpixelRequest::Disabled
                )
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        atlas.fallback_shape_attempts_total(),
        fallback,
        "published blanks never re-shape"
    );
    for _ in 0..20 {
        assert!(
            atlas
                .get_or_create_composed_atlas_result(
                    &device,
                    &queue,
                    "\u{2800}\u{2800}\u{2800}",
                    face.id,
                    16.0f32.to_bits(),
                    Some(&face),
                    SubpixelBin::Zero,
                    SubpixelBin::Zero,
                    SubpixelRequest::Disabled
                )
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(atlas.fallback_shape_attempts_total() - fallback, 1);
}

#[test]
fn repeated_failed_lookup_walks_fallback_once_and_published_font_heals_it() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    // Shaping opens valid immutable bytes, but the claimed COLRv1 glyph's
    // durable paint source cannot be reopened. This is a real paint failure
    // after an actual fallback walk, rather than an empty catalog shortcut.
    let file = neomacs_test_fonts::noto_color_emoji_colrv1();
    let missing = std::path::PathBuf::from("/__neomacs_unavailable__/issue817.ttf");
    assert!(!missing.exists());
    let mut database = fontdb::Database::new();
    assert!(
        !database
            .load_font_source(fontdb::Source::SharedFile(
                missing,
                std::sync::Arc::new(std::fs::read(file).unwrap())
            ))
            .is_empty()
    );
    database.set_sans_serif_family("Noto Color Emoji");
    database.set_monospace_family("Noto Color Emoji");
    atlas.font_system = FontSystem::new_with_locale_and_db("en-US".into(), database);
    let (mut face, mut key) = face_and_key();
    face.font_family = "Noto Color Emoji".into();
    key.charcode = '\u{1f347}' as u32;
    let mut faces = FrameFaceMap::default();
    faces.insert(face.id, face.clone());
    let empty_fonts = ResolvedFontTable::default();
    let chars = CharFontTable::default();
    let clusters = ShapedClusterTable::default();
    atlas.install_frame_fonts(&faces, &empty_fonts, &chars, &clusters);
    let before = atlas.fallback_shape_attempts_total();
    for _ in 0..20 {
        assert_eq!(
            lookup(&mut atlas, &device, &queue, &key, &face).err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    assert_eq!(atlas.fallback_shape_attempts_total() - before, 1);
    atlas.set_scale_factor(2.0);
    for _ in 0..2 {
        assert_eq!(
            lookup(&mut atlas, &device, &queue, &key, &face).err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    assert_eq!(
        atlas.fallback_shape_attempts_total() - before,
        2,
        "scale invalidates failure"
    );
    atlas.clear();
    assert_eq!(
        lookup(&mut atlas, &device, &queue, &key, &face).err(),
        Some(GlyphAtlasError::RasterizeFailed)
    );
    assert_eq!(
        atlas.fallback_shape_attempts_total() - before,
        3,
        "clear retries once"
    );
    for suffix in 0..20 {
        let other = FaceId::new(818 + suffix % 2);
        faces.insert(other, Face::new(other));
        let mut unrelated = ShapedClusterTable::default();
        unrelated
            .entry(other)
            .or_default()
            .insert(format!("suffix-{suffix}").into(), Vec::new());
        atlas.begin_frame_fonts();
        atlas.install_frame_fonts(&faces, &empty_fonts, &chars, &unrelated);
        assert_eq!(
            lookup(&mut atlas, &device, &queue, &key, &face).err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    assert_eq!(
        atlas.fallback_shape_attempts_total() - before,
        3,
        "unrelated projections keep the cached failure"
    );
    let font = fixture_font(file, "Noto Color Emoji");
    let mut fonts = ResolvedFontTable::default();
    fonts.insert(font.id, font);
    atlas.install_frame_fonts(&faces, &fonts, &chars, &clusters);
    assert!(
        lookup(&mut atlas, &device, &queue, &key, &face)
            .unwrap()
            .is_some()
    );
}

#[test]
fn empty_native_catalog_fails_without_shaping_or_repeated_attempts() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    atlas.font_system = FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
    let (face, key) = face_and_key();
    for _ in 0..20 {
        assert_eq!(
            lookup(&mut atlas, &device, &queue, &key, &face).err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    assert_eq!(atlas.fallback_shape_attempts_total(), 0);
    assert_eq!(atlas.rasterization_attempts_total(), 1);
    let font = blank_font();
    atlas.install_resolved_fonts(std::iter::once(&font));
    assert!(
        lookup(&mut atlas, &device, &queue, &key, &face)
            .unwrap()
            .is_none()
    );
}

#[test]
fn retained_monochrome_bytes_keep_blank_valid_when_the_path_is_unavailable() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let file = neomacs_test_fonts::dejavu_2_37().sans_regular();
    let missing = std::path::PathBuf::from("/__neomacs_unavailable__/issue817-mono.ttf");
    assert!(!missing.exists());
    let mut database = fontdb::Database::new();
    assert!(
        !database
            .load_font_source(fontdb::Source::SharedFile(
                missing,
                std::sync::Arc::new(std::fs::read(file).unwrap())
            ))
            .is_empty()
    );
    database.set_sans_serif_family("DejaVu Sans");
    database.set_monospace_family("DejaVu Sans");
    atlas.font_system = FontSystem::new_with_locale_and_db("en-US".into(), database);
    let (face, key) = face_and_key();
    for _ in 0..20 {
        assert!(
            lookup(&mut atlas, &device, &queue, &key, &face)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(atlas.fallback_shape_attempts_total(), 1);
    assert_eq!(atlas.rasterization_attempts_total(), 1);
}

#[test]
fn empty_and_failed_outcomes_reset_with_scale_and_font_catalog() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let font = blank_font();
    atlas.install_resolved_fonts(std::iter::once(&font));
    let (face, key) = face_and_key();
    lookup(&mut atlas, &device, &queue, &key, &face).unwrap();
    assert_eq!(atlas.non_drawing_cache.len(), 1);
    atlas.set_scale_factor(2.0);
    assert!(atlas.non_drawing_cache.is_empty());
    lookup(&mut atlas, &device, &queue, &key, &face).unwrap();
    assert_eq!(atlas.non_drawing_cache.len(), 1);
    atlas.synchronize_font_catalog_generation(
        neomacs_display_protocol::font::FontCatalogGeneration::from_raw(817),
    );
    assert!(atlas.non_drawing_cache.is_empty());
    assert!(atlas.non_drawing_composed_cache.is_empty());
}
