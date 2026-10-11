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
fn entity_query_preserves_an_exact_postscript_identity() {
    let mut regular = candidate("Fixture Sans", 400, FontSlant::Normal, 0);
    replace_file_identity(
        &mut regular,
        ResolvedFontIdentity::from_file(
            "/fixture/FixtureSans-Regular.ttf",
            0,
            Some("FixtureSans-Regular".to_owned()),
        ),
    );
    let mut alternate = candidate("Fixture Sans", 400, FontSlant::Normal, 0);
    replace_file_identity(
        &mut alternate,
        ResolvedFontIdentity::from_file(
            "/fixture/FixtureSans-Alternate.ttf",
            0,
            Some("FixtureSans-Alternate".to_owned()),
        ),
    );
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![regular, alternate],
    }));
    let query = FontEntityQuery::new(Some(
        FontFamilyName::new("Fixture Sans").expect("fixture family"),
    ))
    .with_postscript_name("FixtureSans-Alternate");

    let entity = resolver.resolve_entity(&query).expect("matching entity");

    assert_eq!(
        entity.matched.identity.postscript_name.as_deref(),
        Some("FixtureSans-Alternate")
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
    fn match_font_spec(&self, _query: &FontCandidateQuery) -> crate::font_backend::FontDriverMatch {
        crate::font_backend::FontDriverMatch::Native(self.candidates.first().cloned())
    }

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

#[test]
fn native_driver_match_is_not_rejected_by_enumeration_style_filters() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![
            candidate("Fixture Sans", 400, FontSlant::Normal, 0),
            candidate("Fixture Sans", 700, FontSlant::Italic, 0),
        ],
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Fixture Sans"))
        .with_weight(700)
        .with_slant(FontSlant::Italic)
        .with_selection(FontSpecSelection::DriverMatch);
    let entity = resolver
        .resolve_entity(&query)
        .expect("native driver winner");
    assert_eq!(entity.matched.weight(), Some(400));
    assert_eq!(entity.matched.slant(), FontSlant::Normal);
    assert_eq!(
        entity.matched.file_path(),
        Some("/fixture/Fixture Sans-400.ttf")
    );
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

#[test]
fn primary_weight_selection_uses_gnu_distance_before_discovery_order() {
    // GNU font.c:font_score compares weight-table values, not CSS weights.
    // semi-light=55 is nearer light=50 than regular=80. The CSS distances
    // are both 50, which incorrectly lets the first Regular entity win.
    // Medium=100 likewise prefers regular=80 over semi-bold=180. Distances
    // saturate at 127, so thin=0 ties black=210 with bold=200 in native order.
    for (requested, weights, expected) in [
        (350, [400, 300], 300),
        (500, [600, 400], 400),
        (100, [900, 700], 900),
    ] {
        let resolver = FontResolver::new(Box::new(CandidateBackend {
            candidates: weights
                .into_iter()
                .map(|weight| candidate("Fixture Sans", weight, FontSlant::Normal, 0))
                .collect(),
        }));
        let selected = resolver
            .resolve_primary(
                "Fixture Sans",
                requested,
                FontSlant::Normal,
                FontWidth::Normal,
                selection_size(),
            )
            .expect("available family");
        assert_eq!(selected.weight(), Some(expected), "requested {requested}");
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

#[cfg(any(windows, target_os = "linux"))]
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

#[test]
fn character_fallback_prefers_the_requested_face_width() {
    // A collection such as Iosevka lists its Extended face (width 125) before
    // the Regular one. GNU font_select_entity takes the preferred width from
    // the face when the fontset spec leaves it unset, so the face width must
    // decide, not discovery order.
    let collection_face = |index: u32, width: FontWidth| {
        let mut face = candidate("Fixture Mono", 400, FontSlant::Normal, 100);
        face.matched.metadata.width = Some(width);
        replace_file_identity(
            &mut face,
            ResolvedFontIdentity::from_file("/fixture/FixtureMono.ttc", index, None),
        );
        face
    };
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![
            collection_face(3, FontWidth::Expanded),
            collection_face(0, FontWidth::Normal),
        ],
    }));
    for (requested, expected_index) in [(FontWidth::Normal, 0), (FontWidth::Expanded, 3)] {
        let selected = resolver
            .resolve_for_char(
                "Fixture Mono",
                'α',
                400,
                FontSlant::Normal,
                requested,
                selection_size(),
            )
            .expect("covering collection face");
        assert_eq!(
            (selected.metadata.width, selected.identity.file_face_index()),
            (Some(requested), expected_index),
            "requested {requested:?}"
        );
    }
}

#[test]
fn explicit_fontset_width_beats_the_face_width_on_live_and_frozen_paths() {
    // The face width only fills an unset fontset width.  An explicit
    // `:width' in the fontset spec still wins, on the live path and on the
    // frozen worker path, both on a fresh lookup and on a cache hit.  The
    // Normal face is discovered first, so only scoring can choose Expanded.
    let collection_face = |index: u32, width: FontWidth| {
        let mut face = candidate("Fixture Mono", 400, FontSlant::Normal, 100);
        face.matched.metadata.width = Some(width);
        replace_file_identity(
            &mut face,
            ResolvedFontIdentity::from_file("/fixture/FixtureMono.ttc", index, None),
        );
        face
    };
    let candidates = vec![
        collection_face(0, FontWidth::Normal),
        collection_face(3, FontWidth::Expanded),
    ];
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.eval_str(
        "(let ((font-encoding-alist '((\".*\" unicode)))) \
           (set-fontset-font t ?α (font-spec :family \"Fixture Mono\" :width 'expanded \
                                             :registry \"iso10646-1\")))",
    )
    .unwrap();
    let policies =
        FrozenCharacterPolicies::capture(&[("Base Mono", 'α', 400, false, selection_size())], 4096)
            .unwrap();
    let selected_face = |resolver: &FontResolver| {
        resolver
            .resolve_for_char(
                "Base Mono",
                'α',
                400,
                FontSlant::Normal,
                FontWidth::Normal,
                selection_size(),
            )
            .map(|selected| (selected.metadata.width, selected.identity.file_face_index()))
    };
    let expanded = Some((Some(FontWidth::Expanded), 3));

    let live = FontResolver::new(Box::new(CandidateBackend {
        candidates: candidates.clone(),
    }));
    assert_eq!(selected_face(&live), expanded, "live lookup");
    assert_eq!(selected_face(&live), expanded, "live cache hit");
    drop(eval);

    std::thread::spawn(move || {
        let mut frozen = FontResolver::new(Box::new(CandidateBackend { candidates }));
        frozen.install_worker_policy(Arc::new(policies));
        assert_eq!(selected_face(&frozen), expanded, "frozen lookup");
        assert_eq!(selected_face(&frozen), expanded, "frozen cache hit");
        assert!(!frozen.worker_policy_missing());
    })
    .join()
    .unwrap();
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

#[test]
fn native_entity_open_is_completed_with_backend_metrics() {
    let probes = Arc::new(AtomicUsize::new(0));
    let mut candidate = candidate("Fixture", 400, FontSlant::Normal, 100);
    candidate.matched.metadata.design_metrics = None;
    let resolver = FontResolver::new(Box::new(MetricBackend {
        candidates: vec![candidate],
        probes: Arc::clone(&probes),
    }));

    let opened = resolver
        .open_entity(
            &FontEntityQuery::new(Some(
                FontFamilyName::new("Fixture").expect("fixture family"),
            )),
            20,
        )
        .expect("selected entity");

    assert_eq!(opened.metrics.ascent, 16);
    assert_eq!(probes.load(Ordering::Relaxed), 1);
}

#[test]
fn exact_native_observations_are_cached_until_the_catalog_advances() {
    let probes = Arc::new(AtomicUsize::new(0));
    let mut selected = candidate("Fixture", 400, FontSlant::Normal, 100);
    selected.matched.metadata.design_metrics = None;
    let identity = selected.matched.identity.clone();
    let mut resolver = FontResolver::new(Box::new(MetricBackend {
        candidates: vec![selected],
        probes: Arc::clone(&probes),
    }));

    let first = resolver
        .observe_exact_font(&identity, "Fixture")
        .expect("native face");
    let again = resolver
        .observe_exact_font(&identity, "Fixture")
        .expect("cached native face");
    assert_eq!(first, again);
    assert_eq!(
        probes.load(Ordering::Relaxed),
        1,
        "do not reopen on a cache hit"
    );

    resolver.clear_caches();
    let refreshed = resolver
        .observe_exact_font(&identity, "Fixture")
        .expect("fresh native face");
    assert_eq!(refreshed.identity, identity);
    assert_eq!(
        probes.load(Ordering::Relaxed),
        2,
        "catalog refresh reopens the exact face"
    );
}

#[test]
fn an_exact_native_miss_does_not_survive_backend_replacement() {
    let selected = candidate("Fixture", 400, FontSlant::Normal, 100);
    let identity = selected.matched.identity.clone();
    let mut resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: Vec::new(),
    }));
    assert!(resolver.observe_exact_font(&identity, "Fixture").is_none());
    resolver.replace_backend(Box::new(CandidateBackend {
        candidates: vec![selected],
    }));
    assert_eq!(
        resolver
            .observe_exact_font(&identity, "Fixture")
            .expect("new catalog face")
            .identity,
        identity
    );
}

#[test]
fn captured_character_policy_selects_on_a_thread_without_evaluator_state() {
    let mut spec = StoredFontSpec {
        family: Some(intern("Fixture Sans")),
        registry: None,
        lang: Some(intern("ja")),
        weight: Some(FontWeight::from_css_weight(700)),
        slant: Some(FontSlant::Italic),
        width: Some(FontWidth::Normal),
        definition: Some(neovm_core::emacs_core::fontset::FontDefinitionMetadata {
            encoding: intern("unicode"),
            repertory: Some(
                neovm_core::emacs_core::fontset::FontRepertory::CharTableRanges(vec![(
                    0x3040, 0x309f,
                )]),
            ),
        }),
    };
    let policy = CapturedCharacterPolicy::capture(
        "Base Mono",
        'あ',
        400,
        FontSlant::Normal,
        FontWidth::Expanded,
        selection_size(),
        &spec,
    );
    let candidates = vec![
        candidate("Fixture Sans", 400, FontSlant::Normal, 0),
        candidate("Fixture Sans", 700, FontSlant::Italic, 0),
    ];
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: candidates.clone(),
    }));
    let expected = resolver
        .resolve_from_spec(
            "Base Mono",
            'あ',
            true,
            400,
            FontSlant::Normal,
            FontWidth::Expanded,
            selection_size(),
            &spec,
        )
        .expect("live selection");
    spec.family = Some(intern("Changed Family"));
    spec.weight = Some(FontWeight::from_css_weight(400));
    spec.lang = None;
    spec.definition = None;
    drop(spec);
    drop(resolver);

    let selected = std::thread::spawn(move || {
        // No evaluator, obarray binding, or fontset read on this thread.
        assert_eq!(policy.languages, vec!["ja"]);
        assert!(
            policy.charset_ranges.is_empty(),
            "fontset repertory is not native glyph coverage"
        );
        assert_eq!(
            policy.families.search_order(str::to_owned),
            vec![Some("Fixture Sans".into())]
        );
        let resolver = FontResolver::new(Box::new(CandidateBackend { candidates }));
        resolver
            .resolve_from_policy(&policy, true)
            .expect("captured selection")
    })
    .join()
    .expect("native font selection worker");
    assert_eq!(selected, expected);
    assert_eq!(selected.weight(), Some(700));
    assert_eq!(selected.slant(), FontSlant::Italic);
}

#[test]
fn captured_family_policy_keeps_alias_order_and_fallback_boundary() {
    let families = CapturedFontFamilyPolicy::Inherited(vec!["Alias".into(), "Second".into()]);
    let resolve = |family: &str| match family {
        "Alias" => "Native".to_owned(),
        _ => family.to_owned(),
    };
    assert_eq!(
        families.search_order(resolve),
        vec![
            Some("Native".into()),
            Some("Alias".into()),
            Some("Second".into()),
            None,
        ]
    );
    assert_eq!(
        CapturedFontFamilyPolicy::Explicit("Alias".into()).search_order(resolve),
        vec![Some("Native".into())]
    );
    assert_eq!(
        CapturedFontFamilyPolicy::Inherited(Vec::new()).search_order(resolve),
        vec![None]
    );
}

#[test]
fn frozen_character_requests_survive_live_policy_mutation_and_reject_unknown_queries() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.eval_str("(let ((font-encoding-alist '((\".*\" unicode)))) (set-fontset-font t #x3042 '(\"Fixture Sans\" . \"iso10646-1\")))").unwrap();
    let policies = FrozenCharacterPolicies::capture(
        &[("Base Mono", 'あ', 400, false, selection_size())],
        4096,
    )
    .unwrap();
    let candidates = vec![candidate("Fixture Sans", 400, FontSlant::Normal, 0)];
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: candidates.clone(),
    }));
    let expected = resolver
        .resolve_for_char(
            "Base Mono",
            'あ',
            400,
            FontSlant::Normal,
            FontWidth::Normal,
            selection_size(),
        )
        .unwrap();
    eval.eval_str("(set-fontset-font t #x3042 nil)").unwrap();
    drop(eval);
    std::thread::spawn(move || {
        let mut resolver = FontResolver::new(Box::new(CandidateBackend { candidates }));
        resolver.install_worker_policy(Arc::new(policies));
        assert_eq!(
            resolver.resolve_for_char(
                "Base Mono",
                'あ',
                400,
                FontSlant::Normal,
                FontWidth::Normal,
                selection_size()
            ),
            Some(expected)
        );
        assert!(!resolver.worker_policy_missing());
        assert!(
            resolver
                .resolve_for_char(
                    "Base Mono",
                    'い',
                    400,
                    FontSlant::Normal,
                    FontWidth::Normal,
                    selection_size()
                )
                .is_none()
        );
        assert!(resolver.worker_policy_missing());
    })
    .join()
    .unwrap();
}

#[test]
fn frozen_explicit_none_does_not_enable_native_fallback() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.eval_str("(set-fontset-font t #x3042 nil)").unwrap();
    let policies = FrozenCharacterPolicies::capture(
        &[("Base Mono", 'あ', 400, false, selection_size())],
        4096,
    )
    .unwrap();
    let mut resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![candidate("Fixture Sans", 400, FontSlant::Normal, 0)],
    }));
    resolver.install_worker_policy(Arc::new(policies));
    assert!(
        resolver
            .resolve_for_char(
                "Base Mono",
                'あ',
                400,
                FontSlant::Normal,
                FontWidth::Normal,
                selection_size()
            )
            .is_none()
    );
    assert!(!resolver.worker_policy_missing());
}

#[test]
fn coretext_driver_match_opens_thin_only_family_with_noncanonical_weight() {
    // GNU macfont_match uses CoreText's descriptor matcher, separately from
    // font_delete_unmatched's strict list-fonts style filtering. A sole Thin
    // face may report CT weight -0.6 as CSS 150 rather than named Thin's 100.
    let probes = Arc::new(AtomicUsize::new(0));
    let mut thin = candidate("Thin Only Fixture", 150, FontSlant::Normal, 0);
    thin.matched.metadata.design_metrics = None;
    let expected_identity = thin.matched.identity.clone();
    let resolver = FontResolver::new(Box::new(MetricBackend {
        candidates: vec![thin],
        probes: Arc::clone(&probes),
    }));

    for requested_weight in [100, 400] {
        let enumeration = FontEntityQuery::new(FontFamilyName::new("Thin Only Fixture"))
            .with_weight(requested_weight)
            .with_slant(FontSlant::Normal)
            .with_width(FontWidth::Normal);
        if requested_weight == 400 {
            assert!(
                resolver.resolve_entity(&enumeration).is_none(),
                "enumeration rejects a different GNU weight category"
            );
        }

        let opened = resolver
            .open_entity(
                &enumeration.with_selection(FontSpecSelection::DriverMatch),
                20,
            )
            .expect("driver matching opens the family's available Thin face");
        assert_eq!(opened.entity.matched.identity, expected_identity);
        assert_eq!(opened.entity.matched.weight(), Some(150));
        assert_eq!(opened.metrics.ascent, 16);
    }
    assert_eq!(probes.load(Ordering::Relaxed), 2);
}

#[test]
fn entity_enumeration_matches_gnu_weight_category_for_noncanonical_css_weight() {
    let resolver = FontResolver::new(Box::new(MetricBackend {
        candidates: vec![candidate("Thin Only Fixture", 150, FontSlant::Normal, 0)],
        probes: Arc::new(AtomicUsize::new(0)),
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Thin Only Fixture"))
        .with_weight(100)
        .with_slant(FontSlant::Normal)
        .with_width(FontWidth::Normal);
    let entity = resolver
        .resolve_entity(&query)
        .expect("GNU exact style filtering compares weight-table categories");
    assert_eq!(entity.matched.weight(), Some(150));
    assert_eq!(entity.matched.family(), "Thin Only Fixture");
    assert!(
        resolver.resolve_entity(&query.with_weight(400)).is_none(),
        "Thin remains distinct from Normal during enumeration"
    );
}

#[test]
fn opening_by_spec_keeps_explicit_bold_before_native_regular_fallback() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![
            candidate("Fixture Sans", 400, FontSlant::Normal, 0),
            candidate("Fixture Sans", 700, FontSlant::Normal, 0),
        ],
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Fixture Sans"))
        .with_weight(700)
        .with_selection(FontSpecSelection::OpenBySpec);
    let opened = resolver.resolve_entity(&query).expect("listed Bold face");
    assert_eq!(opened.matched.weight(), Some(700));
    assert_eq!(
        opened.matched.file_path(),
        Some("/fixture/Fixture Sans-700.ttf")
    );
}

#[test]
fn opening_by_spec_uses_normal_as_preference_without_requiring_normal_face() {
    for weights in [vec![150, 400], vec![150]] {
        let resolver = FontResolver::new(Box::new(MetricBackend {
            candidates: weights
                .iter()
                .map(|weight| candidate("Fixture Sans", *weight, FontSlant::Normal, 0))
                .collect(),
            probes: Arc::new(AtomicUsize::new(0)),
        }));
        let query = FontEntityQuery::new(FontFamilyName::new("Fixture Sans"))
            .with_selection(FontSpecSelection::OpenBySpec);
        let opened = resolver.resolve_entity(&query).expect("available face");
        assert_eq!(
            opened.matched.weight(),
            Some(if weights.contains(&400) { 400 } else { 150 })
        );
    }
}

#[test]
fn opening_by_spec_driver_fallback_replaces_unavailable_explicit_style_with_normal_preference() {
    let resolver = FontResolver::new(Box::new(MetricBackend {
        candidates: vec![
            candidate("Fixture Sans", 700, FontSlant::Normal, 0),
            candidate("Fixture Sans", 400, FontSlant::Normal, 0),
        ],
        probes: Arc::new(AtomicUsize::new(0)),
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Fixture Sans"))
        .with_weight(900)
        .with_selection(FontSpecSelection::OpenBySpec);
    let opened = resolver.resolve_entity(&query).expect("fallback face");
    assert_eq!(opened.matched.weight(), Some(400));
}
