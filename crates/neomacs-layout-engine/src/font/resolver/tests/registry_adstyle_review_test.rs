use super::*;

fn style_candidate(style: Option<&str>) -> FontCandidate {
    let mut font = candidate("Registry Style Fixture", 400, FontSlant::Normal, 100);
    font.matched.metadata.size = PlatformFontSize::Fixed {
        device_ppem_26_6: 13 * 64,
    };
    font.matched.metadata.adstyle = style.map(str::to_owned);
    font
}

fn style_catalog() -> FontResolver {
    FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![
            style_candidate(Some("ja")),
            style_candidate(None),
            style_candidate(Some("ko")),
            style_candidate(Some("")),
        ],
    }))
}

fn listed_styles(fonts: Vec<ListedFontCandidate>) -> Vec<Option<String>> {
    fonts
        .into_iter()
        .map(|font| font.candidate.metadata.adstyle)
        .collect()
}

#[test]
fn registry_language_and_explicit_adstyle_are_conjunctive_before_limit() {
    let resolver = style_catalog();
    let query = FontEntityQuery::default().with_registry("gb2312.1980-0");
    let constraints = FontEntityListConstraints {
        adstyle: Some("ja".to_owned()),
        ..Default::default()
    };
    for prefer in [None, Some((&query, &constraints))] {
        assert!(
            resolver
                .list_entities(&query, &constraints, prefer, Some(1))
                .is_empty(),
            "an explicit matching style cannot override the registry language"
        );
    }
}

#[test]
fn registry_language_preserves_absent_style_but_rejects_conflicting_and_empty_candidate_styles() {
    let resolver = style_catalog();
    let query = FontEntityQuery::default().with_registry("microsoft-cp1251");
    for adstyle in [None, Some(String::new())] {
        let constraints = FontEntityListConstraints {
            adstyle,
            ..Default::default()
        };
        assert_eq!(
            listed_styles(resolver.list_entities(&query, &constraints, None, None)),
            vec![None]
        );
        assert_eq!(
            listed_styles(resolver.list_entities(&query, &constraints, None, Some(1))),
            vec![None],
            "metadata filtering precedes MAX"
        );
    }
}

#[test]
fn registry_style_language_ignores_additional_explicit_language_and_folds_style_case() {
    let ja = style_candidate(Some("JA"));
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: vec![ja, style_candidate(Some("ko")), style_candidate(None)],
    }));
    let query = FontEntityQuery::default()
        .with_registry("jisx0208.1983-0")
        .with_language("ru");
    assert_eq!(
        listed_styles(resolver.list_entities(&query, &Default::default(), None, None)),
        vec![Some("JA".to_owned()), None]
    );
    let constraints = FontEntityListConstraints {
        adstyle: Some("ja".to_owned()),
        ..Default::default()
    };
    assert_eq!(
        listed_styles(resolver.list_entities(&query, &constraints, None, None)),
        vec![Some("JA".to_owned())]
    );
}

#[test]
fn explicit_language_without_registry_does_not_become_an_adstyle_filter() {
    let resolver = style_catalog();
    let query = FontEntityQuery::default().with_language("ru");
    assert_eq!(
        listed_styles(resolver.list_entities(&query, &Default::default(), None, None)),
        vec![
            Some("ja".to_owned()),
            None,
            Some("ko".to_owned()),
            Some(String::new())
        ]
    );
}

struct OtherStyleBackend {
    kind: FontBackendKind,
}
impl FontBackend for OtherStyleBackend {
    fn kind(&self) -> FontBackendKind {
        self.kind
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
        vec![style_candidate(Some("ja"))]
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[test]
fn freetype_registry_style_restriction_does_not_leak_to_other_backends() {
    for kind in [FontBackendKind::CoreText, FontBackendKind::DirectWrite] {
        let resolver = FontResolver::new(Box::new(OtherStyleBackend { kind }));
        let query = FontEntityQuery::default().with_registry("gb2312.1980-0");
        assert_eq!(
            listed_styles(resolver.list_entities(&query, &Default::default(), None, None)),
            vec![Some("ja".to_owned())]
        );
    }
}
