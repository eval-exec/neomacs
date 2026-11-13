use super::*;
use crate::font::metrics::test_catalog::TestFontCatalog;

fn native_named_catalog() -> crate::font::metrics::FontMetricsService {
    TestFontCatalog::from_files(&[neomacs_test_fonts::noto_sans_cjk_variable()])
        .expect("pinned CJK collection")
        .to_native_fontconfig()
        .metrics_service()
}

// Noto CJK2.004's named instance3 of face0 is DemiLight/wght350.
// Its native FC_WEIGHT is55, which GNU's font.c weight table calls semi-light.
const DEMILIGHT_SELECTOR: u32 = 0x0003_0000;

#[test]
fn named_listing_catalog_and_finalized_styles_match_the_gnu_semilight_category() {
    let service = native_named_catalog();
    let mut previews = Vec::new();
    let mut emitted = Vec::new();
    service
        .try_visit_font_entities::<()>(
            &FontEntityQuery::new(FontFamilyName::new("Noto Sans CJK JP")),
            &FontEntityListConstraints::default(),
            None,
            Some(1),
            Some(&mut |listed| {
                let selector = listed.candidate.identity.freetype_selector();
                if selector != Some(DEMILIGHT_SELECTOR) {
                    return Ok(false);
                }
                previews.push(listed.candidate.weight().map(FontWeight::from_css_weight));
                Ok(true)
            }),
            &mut |entity| {
                assert_eq!(
                    entity.matched.identity.freetype_selector(),
                    Some(DEMILIGHT_SELECTOR)
                );
                assert!(
                    entity
                        .matched
                        .identity
                        .variation_coords
                        .iter()
                        .any(|coordinate| {
                            coordinate.tag() == u32::from_be_bytes(*b"wght")
                                && coordinate.value() == 350.0
                        })
                );
                emitted.push(entity.matched.weight().map(FontWeight::from_css_weight));
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(previews, [Some(FontWeight::SemiLight)]);
    assert_eq!(
        emitted, previews,
        "materialization must not change the accepted catalog style"
    );
}

#[test]
fn named_listing_ignored_semilight_cannot_be_published_or_consume_maximum() {
    let service = native_named_catalog();
    let mut inspected_named = 0;
    let mut emitted = Vec::new();
    service
        .try_visit_font_entities::<()>(
            &FontEntityQuery::new(FontFamilyName::new("Noto Sans CJK JP")),
            &FontEntityListConstraints::default(),
            None,
            Some(1),
            Some(&mut |listed| {
                if listed.candidate.identity.freetype_selector() != Some(DEMILIGHT_SELECTOR) {
                    return Ok(false);
                }
                inspected_named += 1;
                // This is the category rendered in an ignored-font XLFD's
                // -semilight- field; no file or user font installation is used.
                Ok(listed.candidate.weight().map(FontWeight::from_css_weight)
                    != Some(FontWeight::SemiLight))
            }),
            &mut |entity| {
                emitted.push(entity.matched.weight().map(FontWeight::from_css_weight));
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(
        inspected_named, 1,
        "the exact native named instance must be enumerated"
    );
    assert!(
        emitted.is_empty(),
        "ignored semilight was published and consumed MAX1: {emitted:?}"
    );
}

struct StyleEnrichingBackend(Arc<AtomicUsize>);

impl FontBackend for StyleEnrichingBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }
    fn list_families(&self) -> Vec<FontFamilyName> {
        Vec::new()
    }
    fn resolve_family(&self, family: &str) -> String {
        family.to_owned()
    }
    fn family_prefers_monospace(&self, _: &str) -> bool {
        false
    }
    fn list_candidates(&self, _: &FontCandidateQuery) -> Vec<FontCandidate> {
        vec![candidate("Catalog Light", 300, FontSlant::Normal, 100)]
    }
    fn finalize_match(&self, candidate: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        self.0.fetch_add(1, Ordering::Relaxed);
        let mut matched = candidate.into_file_match()?;
        matched.metadata.weight = Some(350);
        Some(matched)
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[test]
fn named_listing_publication_preserves_catalog_metadata_when_finalization_enriches_style() {
    let finalizations = Arc::new(AtomicUsize::new(0));
    let resolver = FontResolver::new(Box::new(StyleEnrichingBackend(Arc::clone(&finalizations))));
    let mut inspected = Vec::new();
    let mut emitted = Vec::new();
    resolver
        .try_visit_entities::<()>(
            &FontEntityQuery::default(),
            &FontEntityListConstraints::default(),
            None,
            Some(1),
            Some(&mut |listed| {
                inspected.push(listed.candidate.metadata.clone());
                Ok(listed.candidate.weight().map(FontWeight::from_css_weight)
                    != Some(FontWeight::SemiLight))
            }),
            &mut |entity| {
                emitted.push(entity.matched.metadata);
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(finalizations.load(Ordering::Relaxed), 1);
    assert_eq!(inspected.len(), 1);
    assert_eq!(
        emitted, inspected,
        "a materializer must not change an already accepted XLFD style"
    );
}
