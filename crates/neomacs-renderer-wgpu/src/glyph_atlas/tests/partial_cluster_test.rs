use super::*;
use neomacs_display_protocol::font::{ResolvedFontIdentity, ResolvedGlyphId};
use std::{path::PathBuf, sync::Arc};

struct PartialFixture {
    face: Face,
    good: ResolvedGlyph,
    failed: ResolvedGlyph,
    blank: ResolvedGlyph,
}

fn fixture(atlas: &mut WgpuGlyphAtlas) -> PartialFixture {
    let good = super::raster_outcome_tests::blank_font();
    let missing = PathBuf::from("/__neomacs_unavailable__/partial-colrv1.ttf");
    assert!(!missing.exists());
    let mut database = fontdb::Database::new();
    database
        .load_font_file(neomacs_test_fonts::dejavu_2_37().sans_regular())
        .unwrap();
    let bad_ids = database.load_font_source(fontdb::Source::SharedFile(
        missing.clone(),
        Arc::new(std::fs::read(neomacs_test_fonts::noto_color_emoji_colrv1()).unwrap()),
    ));
    assert!(!bad_ids.is_empty());
    database.set_sans_serif_family("DejaVu Sans");
    database.set_monospace_family("DejaVu Sans");
    atlas.font_system = FontSystem::new_with_locale_and_db("en-US".into(), database);
    let mut failed = good.clone();
    failed.id = ResolvedFontId(818);
    failed.identity = ResolvedFontIdentity::from_file(missing.to_str().unwrap(), 0, None);
    failed.replay = FontReplay::Swash {
        asset: FontOutlineAsset::File(FontFileAsset::from_identity(&failed.identity).unwrap()),
    };
    failed.family = "Noto Color Emoji".into();
    atlas.install_resolved_fonts([&good, &failed].into_iter());
    let local = atlas.local_fontdb_id_for(good.id).unwrap();
    let opened = atlas
        .font_system
        .get_font(local, fontdb::Weight::NORMAL)
        .unwrap();
    let good_id = opened.as_swash().charmap().map('A');
    let blank_id = opened.as_swash().charmap().map('\u{2800}');
    let bad = atlas
        .font_system
        .get_font(bad_ids[0], fontdb::Weight::NORMAL)
        .unwrap();
    let bad_id = bad.as_swash().charmap().map('\u{1f347}');
    assert!(good_id != 0 && blank_id != 0 && bad_id != 0);
    let glyph = |font, id, advance| ResolvedGlyph {
        resolved_font_id: font,
        glyph_id: ResolvedGlyphId::new(u32::from(id)),
        x: 0.0,
        y: 0.0,
        x_advance: advance,
        cluster_start: 0,
        cluster_end: 1,
    };
    let mut face = Face::new(FaceId::new(817));
    face.font_family = "DejaVu Sans".into();
    face.font_size = 16.0;
    PartialFixture {
        face,
        good: glyph(good.id, good_id, 12.0),
        failed: glyph(failed.id, bad_id, 16.0),
        blank: glyph(good.id, blank_id, 12.0),
    }
}

#[test]
fn render_shaped_partial_cluster_keeps_successful_ink_when_one_glyph_fails() {
    let (_, _, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let fixture = fixture(&mut atlas);
    let render = |atlas: &mut WgpuGlyphAtlas, text: &str| {
        atlas.rasterize_text(
            text,
            Some(&fixture.face),
            SubpixelBin::Zero,
            SubpixelBin::Zero,
            false,
        )
    };
    assert!(
        matches!(render(&mut atlas, "A"), Ok(Rasterized::Ink(_))),
        "valid outline control"
    );
    assert!(
        matches!(
            render(&mut atlas, "\u{1f347}"),
            Err(GlyphAtlasError::RasterizeFailed)
        ),
        "the selected COLRv1 glyph truly cannot reopen its paint source"
    );
    for text in ["A\u{1f347}", "\u{1f347}A"] {
        let outcome = render(&mut atlas, text)
            .expect("one failed glyph must not discard its painted neighbor");
        let Rasterized::Ink(result) = outcome else {
            panic!("successful A ink was lost");
        };
        assert!(result.width > 0 && result.height > 0);
        assert!(result.pixel_data.iter().any(|&coverage| coverage != 0));
    }
}

#[test]
fn resolved_partial_cluster_keeps_ink_but_blank_neighbors_cannot_hide_failure() {
    let (_, _, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let fixture = fixture(&mut atlas);
    let render = |atlas: &mut WgpuGlyphAtlas, glyphs: &[ResolvedGlyph]| {
        atlas.rasterize_resolved_cluster(
            glyphs,
            Some(&fixture.face),
            16.0,
            SubpixelBin::Zero,
            SubpixelBin::Zero,
            false,
        )
    };
    assert!(matches!(
        render(&mut atlas, std::slice::from_ref(&fixture.good)),
        Ok(Rasterized::Ink(_))
    ));
    assert!(matches!(
        render(&mut atlas, std::slice::from_ref(&fixture.failed)),
        Err(GlyphAtlasError::RasterizeFailed)
    ));
    assert!(
        matches!(
            render(&mut atlas, &[fixture.blank.clone(), fixture.failed.clone()]),
            Err(GlyphAtlasError::RasterizeFailed)
        ),
        "an empty joiner/blank cannot certify failed paint as valid empty success"
    );
    for mut glyphs in [
        vec![fixture.good.clone(), fixture.failed.clone()],
        vec![fixture.failed.clone(), fixture.good.clone()],
    ] {
        glyphs[1].x = glyphs[0].x_advance;
        let expected_advance = glyphs.iter().map(|g| g.x_advance).sum::<f32>();
        let Rasterized::Ink(parts) =
            render(&mut atlas, &glyphs).expect("surviving glyph must paint")
        else {
            panic!("partial cluster was incorrectly empty");
        };
        assert!(!parts.is_empty());
        assert!(
            parts
                .iter()
                .all(|part| part.pixel_data.iter().any(|&v| v != 0))
        );
        assert!(
            parts
                .iter()
                .all(|part| part.advance_width == expected_advance)
        );
    }
}

#[test]
fn composed_empty_lookup_preserves_real_advance_on_first_and_cached_results() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let mut fixture = fixture(&mut atlas);
    let local = atlas
        .local_fontdb_id_for(fixture.blank.resolved_font_id)
        .unwrap();
    let opened = atlas
        .font_system
        .get_font(local, fontdb::Weight::NORMAL)
        .unwrap();
    let advance = opened
        .as_swash()
        .glyph_metrics(&[])
        .scale(16.0)
        .advance_width(fixture.blank.glyph_id.as_u16().unwrap());
    assert!(
        advance > 0.0,
        "the pinned font supplies a real blank advance"
    );
    fixture.blank.x_advance = advance;
    fixture.face.default_resolved_font_id = Some(fixture.blank.resolved_font_id);
    let mut second = fixture.blank.clone();
    second.x = advance;
    let text = "\u{2800}\u{2800}";
    atlas
        .frame_shaped_clusters
        .entry(fixture.face.id)
        .or_default()
        .insert(text.into(), vec![fixture.blank, second]);
    let before = atlas.resolved_rasterization_attempts_total();
    for _ in 0..2 {
        let lookup = atlas
            .get_or_create_composed_atlas_lookup(
                &device,
                &queue,
                text,
                fixture.face.id,
                16.0f32.to_bits(),
                Some(&fixture.face),
                SubpixelBin::Zero,
                SubpixelBin::Zero,
                SubpixelRequest::Disabled,
            )
            .unwrap();
        assert!(
            matches!(lookup, ComposedGlyphLookup::Empty { advance_width }
            if advance_width == advance * 2.0)
        );
    }
    assert!(
        atlas
            .get_or_create_composed_atlas(
                &device,
                &queue,
                text,
                fixture.face.id,
                16.0f32.to_bits(),
                Some(&fixture.face),
                SubpixelBin::Zero,
                SubpixelBin::Zero,
                SubpixelRequest::Disabled
            )
            .unwrap()
            .is_empty(),
        "legacy wrapper remains compatible"
    );
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 1);
}

#[test]
fn oversized_menu_raster_is_memoized_and_retries_after_atlas_reset() {
    use neomacs_display_protocol::frame_chrome::ResolvedMenuLabel;
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let fixture = fixture(&mut atlas);
    let original = atlas.atlas_config;
    atlas.atlas_config.page_size = 1;
    atlas.atlas_pages = GlyphAtlasPages::new(atlas.atlas_config);
    let label = ResolvedMenuLabel::new(vec![fixture.good], Vec::new(), 16.0);
    let before = atlas.resolved_rasterization_attempts_total();
    for _ in 0..20 {
        assert_eq!(
            atlas.menu_label_atlas(&label, &device, &queue).err(),
            Some(GlyphAtlasError::GlyphTooLarge)
        );
    }
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 1);
    atlas.clear();
    atlas.atlas_config = original;
    atlas.atlas_pages = GlyphAtlasPages::new(original);
    assert!(
        !atlas
            .menu_label_atlas(&label, &device, &queue)
            .unwrap()
            .is_empty()
    );
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 2);
    let malformed = GlyphAtlasError::PixelDataLength {
        material: GlyphMaterialKind::AlphaMask,
        expected: 1,
        actual: 0,
    };
    assert!(malformed.is_stable_raster_failure());
    for transient in [
        GlyphAtlasError::PageBudgetExhausted {
            material: GlyphMaterialKind::AlphaMask,
        },
        GlyphAtlasError::AllPagesPinned {
            material: GlyphMaterialKind::AlphaMask,
        },
    ] {
        assert!(!transient.is_stable_raster_failure());
    }
}
