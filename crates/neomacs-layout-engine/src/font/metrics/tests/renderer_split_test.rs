//! The layout engine's half of the renderer tests split at the protocol
//! (MILESTONES M4, A6): the renderer no longer links this engine, so its
//! tests consume `ResolvedFont` and `ResolvedMenuLabel` values built from the
//! same pinned fixtures, and these tests pin that the engine publishes
//! exactly those values. Each names the renderer test it backs; a change
//! here must be mirrored there.

use neomacs_display_protocol::font::{
    FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, FontVariationCoord, ResolvedFont,
    ResolvedFontAdvance, ResolvedFontId, ResolvedFontIdentity, ResolvedGlyph, ResolvedGlyphId,
};
use std::path::Path;

/// The published font for a pinned fixture file, as the renderer's halves
/// build it.
struct PublishedFont<'a> {
    file: &'a Path,
    selector: u32,
    postscript_name: &'a str,
    variations: Vec<FontVariationCoord>,
    family: &'a str,
    weight: u16,
    pixel_size: f32,
    ascent_px: f32,
    descent_px: f32,
    space_advance_px: f32,
    glyph_advance: ResolvedFontAdvance,
    spacing: Option<i32>,
}

impl PublishedFont<'_> {
    fn resolved(self, id: ResolvedFontId) -> ResolvedFont {
        let identity = ResolvedFontIdentity::from_file_with_variations(
            &self.file.to_string_lossy(),
            self.selector,
            Some(self.postscript_name.to_owned()),
            self.variations,
        );
        ResolvedFont {
            id,
            replay: FontReplay::Swash {
                asset: FontOutlineAsset::File(
                    FontFileAsset::from_identity(&identity).expect("file-backed fixture"),
                ),
            },
            identity,
            family: self.family.to_owned(),
            full_name: None,
            postscript_name: Some(self.postscript_name.to_owned()),
            weight: self.weight,
            slant: FontSlantKind::Normal,
            width: 5,
            pixel_size: self.pixel_size,
            ascent_px: self.ascent_px,
            descent_px: self.descent_px,
            space_advance_px: self.space_advance_px,
            glyph_advance: self.glyph_advance,
            spacing: self.spacing,
        }
    }
}

fn glyph(
    font: ResolvedFontId,
    glyph_id: u32,
    x: f32,
    x_advance: f32,
    cluster: (u32, u32),
) -> ResolvedGlyph {
    ResolvedGlyph {
        resolved_font_id: font,
        glyph_id: ResolvedGlyphId::new(glyph_id),
        x,
        y: 0.0,
        x_advance,
        cluster_start: cluster.0,
        cluster_end: cluster.1,
    }
}

/// Backs `glyph_atlas::tests::renderer_keeps_missing_ascii_on_primary_font`.
#[test]
fn layout_publishes_the_symbols_nerd_fixture_as_the_primary_face() {
    let mut fonts = crate::font::metrics::test_catalog::metrics_service();
    let resolved = fonts
        .resolved_font_for_face("Symbols Nerd Font Mono", 400, false, 10.0)
        .expect("the pinned symbols-only face resolves");
    let expected = PublishedFont {
        file: neomacs_test_fonts::symbols_nerd_mono_3_4_0(),
        selector: 0,
        postscript_name: "SymbolsNFM",
        variations: Vec::new(),
        family: "Symbols Nerd Font Mono",
        weight: 400,
        pixel_size: 10.0,
        ascent_px: 8.0,
        descent_px: 2.0,
        space_advance_px: 10.0,
        glyph_advance: ResolvedFontAdvance::monospace_cells(10.0),
        spacing: Some(100),
    }
    .resolved(resolved.id);
    assert_eq!(resolved, expected);
}

/// Backs `glyph_atlas::tests::renderer_uses_layouts_published_fixed_cell_advance`.
#[test]
fn layout_publishes_jetbrains_mono_fixed_cell_advance() {
    let mut fonts = crate::font::metrics::test_catalog::metrics_service();
    let resolved = fonts
        .resolved_font_for_face("JetBrains Mono", 400, false, 14.0)
        .expect("the pinned JetBrains Mono face resolves");
    let expected = PublishedFont {
        file: neomacs_test_fonts::jetbrains_mono_regular_2_304(),
        selector: 0,
        postscript_name: "JetBrainsMono-Regular",
        variations: Vec::new(),
        family: "JetBrains Mono",
        weight: 400,
        pixel_size: 14.0,
        ascent_px: 15.0,
        descent_px: 5.0,
        space_advance_px: 8.0,
        glyph_advance: ResolvedFontAdvance::monospace_cells(8.0),
        spacing: Some(100),
    }
    .resolved(resolved.id);
    assert_eq!(resolved, expected);
}

/// Backs `glyph_atlas::tests::renderer_replays_named_instance_weight_on_the_exact_raw_face`:
/// the native catalog's Noto Sans Bold is the variable fixture's seventh
/// named instance (selector `7 << 16`) at `wght` 700.
#[cfg(target_os = "linux")]
#[test]
fn layout_publishes_the_noto_sans_bold_named_instance() {
    let catalog = crate::font::metrics::test_catalog::standard_catalog().to_native_fontconfig();
    let mut fonts = catalog.metrics_service();
    let resolved = fonts
        .resolved_font_for_face("Noto Sans", 700, false, 18.0)
        .expect("the pinned Noto Sans Bold instance resolves");
    let expected = PublishedFont {
        file: neomacs_test_fonts::noto_sans_variable(),
        selector: 7 << 16,
        postscript_name: "NotoSans-Bold",
        variations: vec![
            FontVariationCoord::try_new(u32::from_be_bytes(*b"wght"), 700.0).expect("finite"),
        ],
        family: "Noto Sans",
        weight: 700,
        pixel_size: 18.0,
        ascent_px: 20.0,
        descent_px: 6.0,
        space_advance_px: 5.0,
        glyph_advance: ResolvedFontAdvance::PerGlyph,
        spacing: None,
    }
    .resolved(resolved.id);
    assert_eq!(resolved, expected);
}

/// Backs `offscreen_frame::menu_test::menu_bar_initial_cjk_glyphs_do_not_overlap`,
/// `compact_bar_initial_cjk_glyphs_do_not_overlap` and
/// `menu_bar_text_origin_is_device_pixel_aligned_at_fractional_scales`: the
/// glyphs and fonts of a monospace menu label at the atlas's default 13 px.
#[test]
fn layout_publishes_monospace_menu_label_glyphs() {
    let mut fonts = crate::font::metrics::test_catalog::metrics_service();

    let (glyphs, resolved) = fonts
        .resolved_glyphs_for_cluster("中中", "monospace", 400, false, 13.0)
        .expect("the CJK label resolves");
    let [font] = resolved.as_slice() else {
        panic!("one font publishes the CJK label: {resolved:?}");
    };
    let id = font.id;
    assert_eq!(
        glyphs,
        vec![
            glyph(id, 997, 0.0, 14.0, (0, 3)),
            glyph(id, 997, 14.0, 14.0, (3, 6))
        ]
    );
    assert_eq!(
        *font,
        PublishedFont {
            file: neomacs_test_fonts::mplus_1_code_thin(),
            selector: 0,
            postscript_name: "MPLUS1Code-Thin",
            variations: Vec::new(),
            family: "M PLUS 1 Code",
            weight: 100,
            pixel_size: 13.0,
            ascent_px: 13.0,
            descent_px: 4.0,
            space_advance_px: 7.0,
            glyph_advance: ResolvedFontAdvance::monospace_cells(7.0),
            spacing: Some(100),
        }
        .resolved(id)
    );

    let (glyphs, resolved) = fonts
        .resolved_glyphs_for_cluster("HHH", "monospace", 400, false, 13.0)
        .expect("the ASCII label resolves");
    let [font] = resolved.as_slice() else {
        panic!("one font publishes the ASCII label: {resolved:?}");
    };
    let id = font.id;
    assert_eq!(
        glyphs,
        vec![
            glyph(id, 43, 0.0, 7.8, (0, 1)),
            glyph(id, 43, 7.0, 7.8, (1, 2)),
            glyph(id, 43, 15.0, 7.8, (2, 3)),
        ]
    );
    assert_eq!(
        *font,
        PublishedFont {
            file: neomacs_test_fonts::noto_sans_mono_variable(),
            selector: 0,
            postscript_name: "NotoSansMono-Regular",
            variations: Vec::new(),
            family: "Noto Sans Mono",
            weight: 400,
            pixel_size: 13.0,
            ascent_px: 14.0,
            descent_px: 4.0,
            space_advance_px: 8.0,
            glyph_advance: ResolvedFontAdvance::PerGlyph,
            spacing: Some(0),
        }
        .resolved(id)
    );
}
