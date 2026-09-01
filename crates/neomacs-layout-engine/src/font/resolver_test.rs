use super::*;
use crate::font_backend::{
    FontFamilyName, PlatformFontCandidate, PlatformFontCandidateLocator, PlatformFontDesignMetrics,
    PlatformFontMetadata, PlatformFontSize,
};
use neomacs_display_protocol::font::{FontFileAsset, FontMemoryAsset, ResolvedFontIdentity};
use neomacs_display_protocol::geometry::DeviceScale;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct CandidateBackend {
    candidates: Vec<FontCandidate>,
}

struct FamilyListingBackend;

struct CatalogGenerationBackend {
    advances: Arc<AtomicUsize>,
}

impl FontBackend for FamilyListingBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        ["Zed Sans", "Alpha Mono", "Zed Sans"]
            .into_iter()
            .map(|family| FontFamilyName::new(family).expect("non-empty fixture family"))
            .collect()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_string()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        false
    }

    fn list_candidates(&self, _query: &FontCandidateQuery) -> Vec<FontCandidate> {
        Vec::new()
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

impl FontBackend for CatalogGenerationBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::CoreText
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_owned()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        false
    }

    fn list_candidates(&self, _query: &FontCandidateQuery) -> Vec<FontCandidate> {
        Vec::new()
    }

    fn advance_catalog_generation(&mut self) {
        self.advances.fetch_add(1, Ordering::Relaxed);
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[test]
fn clearing_resolver_caches_advances_the_backend_catalog_generation() {
    let advances = Arc::new(AtomicUsize::new(0));
    let mut resolver = FontResolver::new(Box::new(CatalogGenerationBackend {
        advances: Arc::clone(&advances),
    }));

    resolver.clear_caches();

    assert_eq!(advances.load(Ordering::Relaxed), 1);
}

#[test]
fn family_listing_preserves_native_order_and_removes_duplicates() {
    let resolver = FontResolver::new(Box::new(FamilyListingBackend));

    assert_eq!(
        resolver.list_families(),
        vec![
            FontFamilyName::new("Zed Sans").expect("fixture family"),
            FontFamilyName::new("Alpha Mono").expect("fixture family"),
        ]
    );
}

#[test]
fn entity_query_uses_the_active_platform_backend_and_requested_style() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![
            candidate("Fixture Sans", 400, FontSlant::Normal, 0),
            candidate("Fixture Sans", 700, FontSlant::Italic, 0),
        ],
    }));
    let query = FontEntityQuery::new(Some(
        FontFamilyName::new("Fixture Sans").expect("fixture family"),
    ))
    .with_weight(700)
    .with_slant(FontSlant::Italic)
    .with_width(FontWidth::Normal);

    let entity = resolver.resolve_entity(&query).expect("matching entity");

    assert_eq!(entity.matched.family(), "Fixture Sans");
    assert_eq!(entity.matched.weight(), Some(700));
    assert_eq!(entity.matched.slant(), FontSlant::Italic);
    assert_eq!(
        entity.matched.file_path(),
        Some("/fixture/Fixture Sans-700.ttf")
    );
}

#[test]
fn entity_query_rejects_a_different_explicit_width() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![candidate("Fixture Sans", 400, FontSlant::Normal, 0)],
    }));
    let query = FontEntityQuery::new(Some(
        FontFamilyName::new("Fixture Sans").expect("fixture family"),
    ))
    .with_width(FontWidth::Expanded);

    assert!(
        resolver.resolve_entity(&query).is_none(),
        "GNU list-fonts filtering rejects an entity whose explicit width differs"
    );
}

#[test]
fn windows_entity_policy_keeps_gnus_relaxed_weight_match() {
    let candidate = candidate("Fixture Sans", 400, FontSlant::Normal, 0);
    let query = FontEntityQuery::new(Some(
        FontFamilyName::new("Fixture Sans").expect("fixture family"),
    ))
    .with_weight(500);

    assert!(!entity_matches_query(
        &candidate,
        &query,
        FontEntityMatchPolicy::Exact,
    ));
    assert!(entity_matches_query(
        &candidate,
        &query,
        FontEntityMatchPolicy::WindowsNtGui,
    ));
}

impl FontBackend for CandidateBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_string()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(&self, _query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.candidates.clone()
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn candidate(family: &str, weight: u16, slant: FontSlant, spacing: i32) -> FontCandidate {
    let path = format!("/fixture/{family}-{weight}.ttf");
    FontCandidate {
        matched: PlatformFontCandidate {
            identity: ResolvedFontIdentity::from_file(&path, 0, None),
            locator: PlatformFontCandidateLocator::File(
                FontFileAsset::new(path, 0).expect("fixture path"),
            ),
            metadata: PlatformFontMetadata {
                foundry: None,
                family: family.to_string(),
                weight: Some(weight),
                slant,
                width: Some(FontWidth::Normal),
                spacing: Some(spacing),
                design_metrics: Some(PlatformFontDesignMetrics::default()),
                size: PlatformFontSize::Scalable,
            },
        },
    }
}

fn selection_size() -> FontSelectionSize {
    FontSelectionSize::new(13.0, DeviceScale::new(1.0).expect("unit scale"))
}

fn replace_file_identity(candidate: &mut FontCandidate, identity: ResolvedFontIdentity) {
    candidate.matched.locator = PlatformFontCandidateLocator::File(
        FontFileAsset::from_identity(&identity).expect("file-backed fixture identity"),
    );
    candidate.matched.identity = identity;
}

#[test]
fn candidate_finalization_rejects_an_asset_from_another_identity() {
    let mut file = candidate("Fixture", 400, FontSlant::Normal, 0).matched;
    file.identity = ResolvedFontIdentity::from_file("/fixture/other.ttf", 0, None);
    assert!(file.into_file_match().is_none());

    let mut native = candidate("Fixture", 400, FontSlant::Normal, 0).matched;
    native.identity = ResolvedFontIdentity::from_memory(
        FontBackendKind::CoreText,
        "coretext:fixture".to_owned(),
        0,
        Some("Fixture".to_owned()),
    );
    native.locator = PlatformFontCandidateLocator::Native;
    let wrong_asset = FontMemoryAsset::new("coretext:other", Arc::new(vec![1]), 0)
        .expect("non-empty memory asset");

    assert!(native.into_memory_match(wrong_asset).is_none());
}

#[test]
fn native_candidate_finalization_rejects_another_collection_face() {
    let mut native = candidate("Fixture", 400, FontSlant::Normal, 0).matched;
    native.identity = ResolvedFontIdentity::from_memory(
        FontBackendKind::DirectWrite,
        "directwrite:Fixture".to_owned(),
        3,
        Some("Fixture".to_owned()),
    );
    native.locator = PlatformFontCandidateLocator::Native;
    let wrong_face = FontMemoryAsset::new("directwrite:Fixture", Arc::new(vec![1]), 2)
        .expect("non-empty memory asset");

    assert!(native.into_memory_match(wrong_face).is_none());
}

fn fixed_size_candidate(layout_px: u32) -> FontCandidate {
    let mut candidate = candidate("Fixture", 400, FontSlant::Normal, 100);
    replace_file_identity(
        &mut candidate,
        ResolvedFontIdentity::from_file(&format!("/fixture/Fixture-{layout_px}px.pcf"), 0, None),
    );
    candidate.matched.metadata.size = PlatformFontSize::Fixed {
        device_ppem_26_6: layout_px * 64,
    };
    candidate
}

#[test]
fn fixed_bitmap_entity_selection_and_cache_are_requested_size_aware() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![fixed_size_candidate(13), fixed_size_candidate(26)],
    }));
    let unit_scale = DeviceScale::new(1.0).expect("unit scale");

    let selected_13 = resolver
        .resolve_primary(
            "Fixture",
            400,
            FontSlant::Normal,
            FontWidth::Normal,
            FontSelectionSize::new(13.0, unit_scale),
        )
        .expect("13px candidate");
    let selected_26 = resolver
        .resolve_primary(
            "Fixture",
            400,
            FontSlant::Normal,
            FontWidth::Normal,
            FontSelectionSize::new(26.0, unit_scale),
        )
        .expect("26px candidate");

    assert_eq!(selected_13.file_path(), Some("/fixture/Fixture-13px.pcf"));
    assert_eq!(selected_26.file_path(), Some("/fixture/Fixture-26px.pcf"));
}

#[cfg(any(unix, windows))]
#[test]
fn unknown_native_size_is_classified_into_concrete_strikes_before_scoring() {
    let path = neomacs_test_fonts::spleen_2_2_0()
        .otb()
        .to_string_lossy()
        .into_owned();
    let mut unknown = candidate("Spleen", 400, FontSlant::Normal, 100);
    replace_file_identity(
        &mut unknown,
        ResolvedFontIdentity::from_file(&path, 0, None),
    );
    unknown.matched.metadata.size = PlatformFontSize::Unknown;
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![unknown],
    }));

    let selected = resolver
        .resolve_primary(
            "Spleen",
            400,
            FontSlant::Normal,
            FontWidth::Normal,
            FontSelectionSize::new(16.0, DeviceScale::new(1.0).expect("unit device scale")),
        )
        .expect("FreeType strike metadata must complete native discovery");

    assert_eq!(
        selected.metadata.size,
        PlatformFontSize::Fixed {
            device_ppem_26_6: 16 * 64,
        }
    );
}

#[test]
fn fixed_bitmap_entity_more_than_two_x_from_the_request_is_ineligible() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![fixed_size_candidate(13)],
    }));

    assert!(
        resolver
            .resolve_primary(
                "Fixture",
                400,
                FontSlant::Normal,
                FontWidth::Normal,
                FontSelectionSize::new(100.0, DeviceScale::new(1.0).expect("unit scale")),
            )
            .is_none(),
        "GNU rejects fixed entities whose pixel size differs by more than 2x"
    );
}

#[test]
fn fixed_bitmap_size_distance_caps_before_discovery_order_tie_break() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![fixed_size_candidate(200), fixed_size_candidate(164)],
    }));

    let selected = resolver
        .resolve_primary(
            "Fixture",
            400,
            FontSlant::Normal,
            FontWidth::Normal,
            FontSelectionSize::new(100.0, DeviceScale::new(1.0).expect("unit scale")),
        )
        .expect("both candidates are within GNU's inclusive 2x boundary");

    assert_eq!(
        selected.file_path(),
        Some("/fixture/Fixture-200px.pcf"),
        "both doubled integer-pixel distances cap at 127, so the first entity wins"
    );
}

#[test]
fn shared_primary_scoring_prefers_requested_style() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![
            candidate("Fixture", 400, FontSlant::Normal, 100),
            candidate("Fixture", 700, FontSlant::Italic, 100),
        ],
    }));
    let selected = resolver
        .resolve_primary(
            "Fixture",
            700,
            FontSlant::Italic,
            FontWidth::Normal,
            selection_size(),
        )
        .expect("candidate");
    assert_eq!(selected.weight(), Some(700));
    assert_eq!(selected.slant(), FontSlant::Italic);
}

#[test]
fn equal_score_entities_keep_their_own_discovery_order() {
    let mut variable_seed = candidate("Fixture", 800, FontSlant::Italic, 100);
    replace_file_identity(
        &mut variable_seed,
        ResolvedFontIdentity::from_file(
            "/fixture/Fixture[wdth,wght].ttf",
            0x0008_0000,
            Some("Fixture-ExtraBoldItalic".to_string()),
        ),
    );
    let mut static_bold = candidate("Fixture", 700, FontSlant::Italic, 100);
    replace_file_identity(
        &mut static_bold,
        ResolvedFontIdentity::from_file(
            "/fixture/Fixture-BoldItalic.ttf",
            0,
            Some("Fixture-BoldItalic".to_string()),
        ),
    );
    let mut variable_bold = candidate("Fixture", 700, FontSlant::Italic, 100);
    replace_file_identity(
        &mut variable_bold,
        ResolvedFontIdentity::from_file(
            "/fixture/Fixture[wdth,wght].ttf",
            0x0007_0000,
            Some("Fixture-BoldItalic".to_string()),
        ),
    );

    let selected = select_best_candidate(
        vec![variable_seed, static_bold, variable_bold],
        &SelectionRequest {
            weight: 700,
            slant: FontSlant::Italic,
            width: Some(FontWidth::Normal),
            spacing: None,
            prefer_monospace: false,
            queried_family: Some("Fixture"),
            size: selection_size(),
        },
    )
    .expect("equal-score entity");

    assert_eq!(
        selected.file_path(),
        Some("/fixture/Fixture-BoldItalic.ttf"),
        "GNU scores each entity independently; a variable file's earlier, non-matching instance must not donate its ordinal to a later instance"
    );
}

struct MetricBackend {
    candidates: Vec<FontCandidate>,
    probes: Arc<AtomicUsize>,
}

impl FontBackend for MetricBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::CoreText
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_string()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(&self, _query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.candidates.clone()
    }

    fn design_metrics(&self, _matched: &PlatformFontMatch) -> Option<PlatformFontDesignMetrics> {
        self.probes.fetch_add(1, Ordering::Relaxed);
        Some(PlatformFontDesignMetrics {
            units_per_em: 1_000,
            ascent: 800,
            descent: 200,
            line_gap: 0,
            max_advance: 700,
            space_advance: 500,
            average_advance: 600,
        })
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[test]
fn native_metrics_are_probed_only_for_the_cached_winner() {
    let probes = Arc::new(AtomicUsize::new(0));
    let mut regular = candidate("Fixture", 400, FontSlant::Normal, 100);
    regular.matched.metadata.design_metrics = None;
    let mut bold = candidate("Fixture", 700, FontSlant::Normal, 100);
    bold.matched.metadata.design_metrics = None;
    let resolver = FontResolver::new(Box::new(MetricBackend {
        candidates: vec![regular, bold],
        probes: Arc::clone(&probes),
    }));

    let first = resolver
        .resolve_primary(
            "Fixture",
            700,
            FontSlant::Normal,
            FontWidth::Normal,
            selection_size(),
        )
        .expect("selected winner");
    let second = resolver
        .resolve_primary(
            "Fixture",
            700,
            FontSlant::Normal,
            FontWidth::Normal,
            selection_size(),
        )
        .expect("cached winner");

    assert_eq!(first.identity, second.identity);
    assert_eq!(
        first.pixel_metrics(20.0).expect("native metrics").ascent,
        16
    );
    assert_eq!(probes.load(Ordering::Relaxed), 1);
}
