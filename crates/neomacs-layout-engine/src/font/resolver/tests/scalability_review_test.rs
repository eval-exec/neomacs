use super::*;
use std::sync::Mutex;

struct ScalabilityRecordingBackend {
    kind: FontBackendKind,
    seen: Arc<Mutex<Vec<Option<bool>>>>,
    candidate: FontCandidate,
}

impl FontBackend for ScalabilityRecordingBackend {
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
    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.seen.lock().unwrap().push(query.scalable);
        vec![self.candidate.clone()]
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn recorded_listing_states(kind: FontBackendKind) -> Vec<Option<bool>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let resolver = FontResolver::new(Box::new(ScalabilityRecordingBackend {
        kind,
        seen: Arc::clone(&seen),
        candidate: candidate("Fixture Sans", 400, FontSlant::Normal, 100),
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Fixture Sans"));
    for scalable in [None, Some(true), Some(false)] {
        let constraints = FontEntityListConstraints {
            scalable,
            ..Default::default()
        };
        let result = resolver.list_entities(&query, &constraints, None, Some(1));
        assert_eq!(
            result.len(),
            1,
            "backend admission owns the native property"
        );
    }
    seen.lock().unwrap().clone()
}

#[test]
fn scalability_listing_forwards_three_states_to_fontconfig() {
    assert_eq!(
        recorded_listing_states(FontBackendKind::Fontconfig),
        vec![None, Some(true), Some(false)]
    );
}

#[test]
fn scalability_listing_leaves_coretext_and_directwrite_unconstrained() {
    for kind in [FontBackendKind::CoreText, FontBackendKind::DirectWrite] {
        assert_eq!(recorded_listing_states(kind), vec![None, None, None]);
    }
}

#[test]
fn scalability_preference_does_not_constrain_the_listing_query() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let resolver = FontResolver::new(Box::new(ScalabilityRecordingBackend {
        kind: FontBackendKind::Fontconfig,
        seen: Arc::clone(&seen),
        candidate: candidate("Fixture Sans", 400, FontSlant::Normal, 100),
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Fixture Sans"));
    let prefer = FontEntityListConstraints {
        scalable: Some(false),
        ..Default::default()
    };
    assert_eq!(
        resolver
            .list_entities(
                &query,
                &Default::default(),
                Some((&query, &prefer)),
                Some(1)
            )
            .len(),
        1
    );
    assert_eq!(*seen.lock().unwrap(), vec![None]);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn scalability_listing_keeps_fixed_spleen_when_average_width_is_unknown() {
    let path = neomacs_test_fonts::spleen_2_2_0().otb();
    let mut fixed = candidate("Spleen", 400, FontSlant::Normal, 100);
    replace_file_identity(
        &mut fixed,
        ResolvedFontIdentity::from_file(path.to_str().unwrap(), 0, None),
    );
    fixed.matched.metadata.size = PlatformFontSize::Unknown;
    let resolver = FontResolver::new(Box::new(ScalabilityRecordingBackend {
        kind: FontBackendKind::Fontconfig,
        seen: Arc::new(Mutex::new(Vec::new())),
        candidate: fixed,
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Spleen"));
    for (scalable, average_width) in [(None, 0), (None, 120), (Some(false), 0), (Some(false), 120)]
    {
        let constraints = FontEntityListConstraints {
            scalable,
            average_width: Some(average_width),
            ..Default::default()
        };
        let listed = resolver.list_entities(&query, &constraints, None, Some(1));
        assert_eq!(
            listed.len(),
            1,
            "GNU admits unknown average width: {scalable:?}, requested {average_width}"
        );
        assert_eq!(
            listed[0].candidate.metadata.size,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 16 * 64,
            }
        );
    }
}
