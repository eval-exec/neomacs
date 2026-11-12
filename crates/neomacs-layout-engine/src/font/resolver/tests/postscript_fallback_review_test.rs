use super::*;

struct FinalizedPostscriptBackend {
    candidate: FontCandidate,
    finalized_name: Option<String>,
}

impl FontBackend for FinalizedPostscriptBackend {
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
        vec![self.candidate.clone()]
    }

    fn match_font_spec(&self, _: &FontCandidateQuery) -> crate::font_backend::FontDriverMatch {
        crate::font_backend::FontDriverMatch::Native(Some(self.candidate.clone()))
    }

    fn finalize_match(&self, mut matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        matched.identity.postscript_name = self.finalized_name.clone();
        matched.into_file_match()
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn resolver_with_postscript_names(
    discovered: Option<&str>,
    finalized: Option<&str>,
) -> FontResolver {
    let mut candidate = candidate("Fixture Sans", 400, FontSlant::Normal, 0);
    candidate.matched.identity.postscript_name = discovered.map(str::to_owned);
    FontResolver::new(Box::new(FinalizedPostscriptBackend {
        candidate,
        finalized_name: finalized.map(str::to_owned),
    }))
}

fn postscript_opening_query() -> FontEntityQuery {
    FontEntityQuery::new(FontFamilyName::new("Fixture Sans"))
        .with_postscript_name("FixtureSans-Requested")
        .with_selection(FontSpecSelection::OpenBySpec)
}

#[test]
fn postscript_fallback_rejects_a_different_finalized_name() {
    let resolver = resolver_with_postscript_names(None, Some("FixtureSans-Other"));
    assert!(
        resolver
            .resolve_entity(&postscript_opening_query())
            .is_none()
    );
}

#[test]
fn postscript_fallback_rejects_a_missing_finalized_name() {
    let resolver = resolver_with_postscript_names(None, None);
    assert!(
        resolver
            .resolve_entity(&postscript_opening_query())
            .is_none()
    );
}

#[test]
fn postscript_fallback_accepts_the_name_supplied_during_finalization() {
    for finalized in ["FixtureSans-Requested", "fixturesans-requested"] {
        let resolver = resolver_with_postscript_names(None, Some(finalized));
        let entity = resolver
            .resolve_entity(&postscript_opening_query())
            .expect("finalized identity must satisfy the request independent of ASCII case");
        assert_eq!(
            entity.matched.identity.postscript_name.as_deref(),
            Some(finalized)
        );
    }
}

#[test]
fn postscript_fallback_keeps_style_relaxation_for_matching_or_unconstrained_names() {
    for requested in [true, false] {
        let resolver = resolver_with_postscript_names(
            Some("FixtureSans-Requested"),
            Some("FixtureSans-Requested"),
        );
        let query = if requested {
            postscript_opening_query()
        } else {
            FontEntityQuery::new(FontFamilyName::new("Fixture Sans"))
                .with_selection(FontSpecSelection::OpenBySpec)
        }
        .with_weight(900);
        let entity = resolver
            .resolve_entity(&query)
            .expect("normal native fallback remains available when exact style is absent");
        assert_eq!(entity.matched.weight(), Some(400));
    }
}
