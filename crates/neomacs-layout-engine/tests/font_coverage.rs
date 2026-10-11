//! Glyph coverage uses one exact file, memory face, or bitmap font.
use neomacs_display_protocol::font::{
    FontBackendKind, FontFileAsset, FontMemoryAsset, FontOutlineAsset, ResolvedFontIdentity,
};
use neomacs_layout_engine::{
    font::metrics::FontMetricsService,
    font_backend::{PlatformFontMatch, PlatformFontMetadata, PlatformFontSize},
};
use neovm_core::face::FontSlant;
use std::sync::Arc;

fn matched(
    identity: ResolvedFontIdentity,
    asset: FontOutlineAsset,
    size: PlatformFontSize,
) -> PlatformFontMatch {
    PlatformFontMatch {
        identity,
        asset,
        metadata: PlatformFontMetadata {
            family: "Coverage fixture".into(),
            foundry: None,
            weight: None,
            slant: FontSlant::Normal,
            width: None,
            spacing: None,
            design_metrics: None,
            size,
        },
    }
}

#[test]
fn exact_outline_identity_reports_space_and_ascii_without_emoji_fallback() {
    let mut service = FontMetricsService::new();
    let path = neomacs_test_fonts::mplus_1_code_thin().to_str().unwrap();
    let identity = ResolvedFontIdentity::from_file(path, 0, None);
    assert_eq!(
        service.font_has_char_in_identity(&identity, 'A'),
        Some(true)
    );
    assert_eq!(
        service.font_has_char_in_identity(&identity, ' '),
        Some(true)
    );
    assert_eq!(
        service.font_has_char_in_identity(&identity, '\u{10ffff}'),
        Some(false)
    );
    // The pinned Noto emoji fixture contains this glyph (also exercised by
    // the GUI icon test); M PLUS does not. A fallback lookup would be wrong.
    assert_eq!(
        service.font_has_char_in_identity(&identity, '🔽'),
        Some(false)
    );
    // The fixture is a single-face font. Do not silently query face zero if
    // the supplied collection selector cannot be opened.
    let invalid_face = ResolvedFontIdentity::from_file(path, 1, None);
    assert_eq!(service.font_has_char_in_identity(&invalid_face, 'A'), None);
}

#[test]
fn owned_memory_asset_reports_coverage_without_a_file_or_family_selection() {
    let mut service = FontMetricsService::new();
    let bytes = Arc::new(std::fs::read(neomacs_test_fonts::mplus_1_code_thin()).unwrap());
    let identity = ResolvedFontIdentity::from_memory(
        FontBackendKind::CoreText,
        "coverage-memory".into(),
        0,
        None,
    );
    let asset =
        FontOutlineAsset::Memory(FontMemoryAsset::new("coverage-memory", bytes, 0).unwrap());
    let font = matched(identity, asset, PlatformFontSize::Scalable);
    assert_eq!(service.font_has_char_in_match(&font, 'A'), Some(true));
    assert_eq!(
        service.font_has_char_in_match(&font, '\u{10ffff}'),
        Some(false)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bitmap_font_reports_block_coverage_without_outline_parsing() {
    let mut service = FontMetricsService::new();
    let path = neomacs_test_fonts::spleen_2_2_0().pcf();
    let path = path.to_str().unwrap();
    let font = matched(
        ResolvedFontIdentity::from_file(path, 0, None),
        FontOutlineAsset::File(FontFileAsset::new(path, 0).unwrap()),
        PlatformFontSize::Fixed {
            device_ppem_26_6: 16 * 64,
        },
    );
    assert_eq!(service.font_has_char_in_match(&font, '█'), Some(true));
    assert_eq!(
        service.font_has_char_in_match(&font, '\u{10ffff}'),
        Some(false)
    );
}
