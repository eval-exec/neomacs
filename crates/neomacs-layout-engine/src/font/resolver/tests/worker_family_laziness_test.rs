//! Exercise the existing resolver entry point so this regression also runs
//! against the eager family-search implementation before its repair.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const FIRST_ALIAS: &str = "family-laziness-first-alias";
const FIRST_WINNER: &str = "Family Laziness First Winner";

struct FamilyVisitBackend {
    inner: CandidateBackend,
    resolved_families: Arc<AtomicUsize>,
    candidate_attempts: Arc<AtomicUsize>,
    explicit_language: Option<&'static str>,
    expects_registry_language: bool,
}

impl FontBackend for FamilyVisitBackend {
    fn kind(&self) -> FontBackendKind {
        self.inner.kind()
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        self.inner.list_families()
    }

    fn resolve_family(&self, family: &str) -> String {
        self.resolved_families.fetch_add(1, Ordering::Relaxed);
        match family {
            FIRST_ALIAS => FIRST_WINNER.to_owned(),
            other => other.to_owned(),
        }
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        self.inner.family_prefers_monospace(family)
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.candidate_attempts.fetch_add(1, Ordering::Relaxed);
        assert_eq!(query.explicit_language.as_deref(), self.explicit_language);
        if self.expects_registry_language {
            assert!(query.languages.iter().any(|language| language == "zh-cn"));
            if self.explicit_language.is_some() {
                assert!(query.languages.iter().any(|language| language == "ja"));
            }
        }

        assert_eq!(
            query.scope,
            FontCandidateScope::Family(FontFamilyName::new(FIRST_WINNER).unwrap()),
            "the resolved first alias must win before its raw spelling or any later pass"
        );
        self.inner.list_candidates(query)
    }

    fn advance_catalog_generation(&mut self) {
        self.inner.advance_catalog_generation();
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        self.inner.poll_catalog_change()
    }
}

#[test]
fn inherited_first_winner_never_resolves_unused_large_family_alternatives() {
    let families = std::iter::once(FIRST_ALIAS.to_owned())
        .chain((0..96).map(|index| format!("unused-family-{index}-{}", "W".repeat(16384))))
        .collect();
    let policy = CapturedCharacterPolicy {
        requested_family: "Family Laziness Base".to_owned(),
        ch: '好',
        families: CapturedFontFamilyPolicy::Inherited(families),
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        explicit_language: None,
        weight: 400,
        slant: FontSlant::Normal,
        width: FontWidth::Normal,
        size: selection_size(),
    };
    let winner = candidate(FIRST_WINNER, 400, FontSlant::Normal, 0);
    let expected = winner.matched.clone();
    let resolved_families = Arc::new(AtomicUsize::new(0));
    let candidate_attempts = Arc::new(AtomicUsize::new(0));
    let resolver = FontResolver::new(Box::new(FamilyVisitBackend {
        inner: CandidateBackend {
            candidates: vec![winner],
        },
        resolved_families: Arc::clone(&resolved_families),
        candidate_attempts: Arc::clone(&candidate_attempts),
        explicit_language: None,
        expects_registry_language: false,
    }));

    let selected = resolver
        .resolve_from_policy(&policy, false)
        .expect("the first inherited alias retains its selected candidate");

    assert_eq!(selected, expected);
    assert_eq!(candidate_attempts.load(Ordering::Relaxed), 1);
    assert_eq!(
        resolved_families.load(Ordering::Relaxed),
        1,
        "an early winner must not resolve or copy the unused inherited family suffix"
    );
}

#[test]
fn native_queries_preserve_explicit_language_separately_from_registry_hints() {
    for explicit_language in [None, Some("JA")] {
        let attempts = Arc::new(AtomicUsize::new(0));
        let resolver = FontResolver::new(Box::new(FamilyVisitBackend {
            inner: CandidateBackend {
                candidates: vec![candidate(FIRST_WINNER, 400, FontSlant::Normal, 0)],
            },
            resolved_families: Arc::new(AtomicUsize::new(0)),
            candidate_attempts: Arc::clone(&attempts),
            explicit_language,
            expects_registry_language: true,
        }));
        let mut query =
            FontEntityQuery::new(FontFamilyName::new(FIRST_ALIAS)).with_registry("gb2312.1980-0");
        if let Some(language) = explicit_language {
            query = query.with_language(language);
        }
        assert_eq!(
            resolver
                .list_entities(&query, &FontEntityListConstraints::default(), None, None)
                .len(),
            1
        );
        let spec = StoredFontSpec {
            family: Some(intern(FIRST_ALIAS)),
            registry: Some(intern("gb2312.1980-0")),
            lang: explicit_language.map(intern),
            weight: None,
            slant: None,
            width: None,
            definition: None,
        };
        assert!(
            resolver
                .resolve_from_spec(
                    FIRST_ALIAS,
                    '好',
                    false,
                    400,
                    FontSlant::Normal,
                    FontWidth::Normal,
                    selection_size(),
                    &spec,
                )
                .is_some()
        );
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
    }
}
