//! `:otf` candidate filtering against real, pinned font files.
//!
//! The catalog is explicit (`TestFontCatalog`), so this test never consults the
//! host's font installation: DejaVu Sans covers Arabic but has no `rtlm`
//! feature under the `arab` script, while Noto Sans Arabic does.

use super::*;
use crate::font::metrics::test_catalog::TestFontCatalog;
use neovm_core::emacs_core::fontset::{StoredOtfFeatures, StoredOtfLangsys, StoredOtfSpec};
use std::path::{Path, PathBuf};

fn fixture_files() -> (PathBuf, PathBuf, TestFontCatalog) {
    let dejavu = neomacs_test_fonts::desktop_fonts_cached()
        .directory()
        .join("DejaVuSans.ttf");
    let noto = neomacs_test_fonts::noto_sans_arabic_variable_cached().to_path_buf();
    let catalog = TestFontCatalog::from_files(&[dejavu.as_path(), noto.as_path()])
        .expect("pinned DejaVu/Noto catalog");
    (dejavu, noto, catalog)
}

fn arab_requirement(required: &[&str]) -> StoredOtfSpec {
    StoredOtfSpec {
        script: "arab".to_owned(),
        langsys: StoredOtfLangsys::Default,
        gsub: Some(StoredOtfFeatures {
            required: required.iter().map(|tag| (*tag).to_owned()).collect(),
            forbidden: Vec::new(),
        }),
        gpos: None,
    }
}

fn query(family: Option<&str>, otf: Option<StoredOtfSpec>) -> FontEntityQuery {
    let mut query = FontEntityQuery::new(family.and_then(FontFamilyName::new))
        .with_selection(FontSpecSelection::Enumerate);
    if let Some(otf) = otf {
        query = query.with_otf(otf);
    }
    query
}

fn listed_files(
    service: &crate::font::metrics::FontMetricsService,
    query: &FontEntityQuery,
) -> Vec<String> {
    service
        .list_font_entities(query, &FontEntityListConstraints::default(), None, None)
        .into_iter()
        .filter_map(|entity| entity.matched.identity.file_path)
        .collect()
}

fn resolved_file(
    service: &crate::font::metrics::FontMetricsService,
    query: &FontEntityQuery,
) -> Option<String> {
    service
        .resolve_font_entity(query)
        .and_then(|entity| entity.matched.identity.file_path)
}

#[test]
fn otf_requirement_skips_a_covering_font_that_lacks_the_feature() {
    let (dejavu, noto, catalog) = fixture_files();
    let service = catalog.metrics_service();

    let unrestricted = query(None, None);
    let all = listed_files(&service, &unrestricted);
    let dejavu_name = dejavu.to_string_lossy().into_owned();
    let noto_name = noto.to_string_lossy().into_owned();
    let dejavu_index = all
        .iter()
        .position(|file| file == &dejavu_name)
        .unwrap_or_else(|| panic!("DejaVu Sans missing from pinned catalog: {all:?}"));
    let noto_index = all
        .iter()
        .position(|file| file == &noto_name)
        .unwrap_or_else(|| panic!("Noto Sans Arabic missing from pinned catalog: {all:?}"));
    assert!(
        dejavu_index < noto_index,
        "the covering-but-incapable font must precede the capable one: {all:?}"
    );

    // DejaVu carries the Arabic codepoints but not the `rtlm` feature, so the
    // `:otf` requirement removes it from the same enumeration.
    let capable = query(None, Some(arab_requirement(&["rtlm"])));
    let filtered = listed_files(&service, &capable);
    assert!(
        !filtered.iter().any(|file| file == &dejavu_name),
        "DejaVu Sans failed the arab/rtlm requirement yet survived: {filtered:?}"
    );
    assert!(
        filtered.iter().any(|file| file == &noto_name),
        "Noto Sans Arabic satisfies arab/rtlm but was filtered out: {filtered:?}"
    );
    assert_eq!(
        resolved_file(&service, &capable).as_deref(),
        Some(noto_name.as_str()),
        "entity selection must skip the incapable covering font"
    );

    // An explicit family cannot silently fall back to a different family.
    let incapable_family = query(Some("DejaVu Sans"), Some(arab_requirement(&["rtlm"])));
    assert!(
        listed_files(&service, &incapable_family).is_empty(),
        "a family-scoped query must not substitute another family"
    );
    assert_eq!(resolved_file(&service, &incapable_family), None);
}

#[test]
fn otf_requirement_absent_feature_rejects_the_only_capable_family() {
    let (_dejavu, _noto, catalog) = fixture_files();
    let service = catalog.metrics_service();
    let absent = query(Some("Noto Sans Arabic"), Some(arab_requirement(&["zzzz"])));
    assert_eq!(resolved_file(&service, &absent), None);

    let absent_script = query(
        Some("Noto Sans Arabic"),
        Some(StoredOtfSpec {
            script: "zzzz".to_owned(),
            langsys: StoredOtfLangsys::Default,
            gsub: Some(StoredOtfFeatures {
                required: vec!["init".to_owned()],
                forbidden: Vec::new(),
            }),
            gpos: None,
        }),
    );
    assert_eq!(resolved_file(&service, &absent_script), None);
}

#[test]
fn otf_requirement_finds_the_satisfying_family() {
    let (_dejavu, noto, catalog) = fixture_files();
    let service = catalog.metrics_service();
    let capable = query(
        Some("Noto Sans Arabic"),
        Some(arab_requirement(&["init", "medi", "fina"])),
    );
    assert_eq!(
        resolved_file(&service, &capable).as_deref(),
        Some(noto.to_string_lossy().as_ref())
    );
    assert_eq!(
        Path::new(noto.as_path()).canonicalize().unwrap(),
        Path::new(noto.as_path())
    );
}

#[test]
fn otf_script_only_uses_gnu_script_representative_chars() {
    let (dejavu, noto, catalog) = fixture_files();
    let service = catalog.metrics_service();
    // DejaVu Sans covers Arabic letters but not U+06C1 (ARABIC LETTER HEH
    // GOAL), which GNU's `script-representative-chars` lists for `arabic`
    // alongside U+0628. GNU's ftfont adds both to the Fontconfig charset, so
    // even a featureless `(arab)` requirement must not list DejaVu.
    let script_only = query(
        None,
        Some(StoredOtfSpec {
            script: "arab".to_owned(),
            langsys: StoredOtfLangsys::Default,
            gsub: None,
            gpos: None,
        }),
    );
    let listed = listed_files(&service, &script_only);
    let dejavu_name = dejavu.to_string_lossy();
    let noto_name = noto.to_string_lossy();
    assert!(
        !listed.iter().any(|file| file == dejavu_name.as_ref()),
        "the arab representative charset must exclude DejaVu Sans: {listed:?}"
    );
    assert!(
        listed.iter().any(|file| file == noto_name.as_ref()),
        "Noto Sans Arabic covers both representative characters: {listed:?}"
    );
    assert_eq!(
        resolved_file(&service, &script_only).as_deref(),
        Some(noto_name.as_ref())
    );
}

#[test]
fn captured_character_policy_adds_the_otf_representative_charset() {
    let spec = StoredFontSpec {
        family: Some(intern("DejaVu Sans")),
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        otf: Some(StoredOtfSpec {
            script: "arab".to_owned(),
            langsys: StoredOtfLangsys::Default,
            gsub: None,
            gpos: None,
        }),
        definition: None,
    };
    let size = FontSelectionSize::new(
        13.0,
        neomacs_display_protocol::geometry::DeviceScale::new(1.0).expect("unit scale"),
    );
    let policy = CapturedCharacterPolicy::capture(
        "Base Mono",
        'س',
        400,
        FontSlant::Normal,
        FontWidth::Normal,
        size,
        &spec,
    );
    assert!(
        policy.charset_ranges.contains(&(0x628, 0x628)),
        "the char path must require U+0628: {:?}",
        policy.charset_ranges
    );
    assert!(
        policy.charset_ranges.contains(&(0x6c1, 0x6c1)),
        "the char path must require U+06C1: {:?}",
        policy.charset_ranges
    );
}
