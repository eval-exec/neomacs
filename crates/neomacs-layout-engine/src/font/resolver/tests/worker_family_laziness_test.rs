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
