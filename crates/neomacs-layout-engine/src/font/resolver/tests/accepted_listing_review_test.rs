use super::*;
use std::sync::Mutex;

struct AcceptedListingBackend {
    candidates: Vec<FontCandidate>,
    enumerations: Arc<AtomicUsize>,
    finalized: Arc<Mutex<Vec<String>>>,
}

impl FontBackend for AcceptedListingBackend {
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
        self.enumerations.fetch_add(1, Ordering::Relaxed);
        self.candidates.clone()
    }
    fn finalize_match(&self, candidate: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        self.finalized
            .lock()
            .unwrap()
            .push(candidate.family().to_owned());
        candidate.into_file_match()
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn resolver(
    candidates: Vec<FontCandidate>,
) -> (FontResolver, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let enumerations = Arc::new(AtomicUsize::new(0));
    let finalized = Arc::new(Mutex::new(Vec::new()));
    let resolver = FontResolver::new(Box::new(AcceptedListingBackend {
        candidates,
        enumerations: Arc::clone(&enumerations),
        finalized: Arc::clone(&finalized),
    }));
    (resolver, enumerations, finalized)
}

#[test]
fn accepted_listing_zero_maximum_does_not_enumerate() {
    let (resolver, enumerations, finalized) =
        resolver(vec![candidate("Fixture Sans", 400, FontSlant::Normal, 100)]);
    assert!(
        resolver
            .list_entities(
                &FontEntityQuery::default(),
                &FontEntityListConstraints::default(),
                None,
                Some(0),
            )
            .is_empty()
    );
    assert_eq!(enumerations.load(Ordering::Relaxed), 0);
    assert!(finalized.lock().unwrap().is_empty());
}

#[test]
fn accepted_listing_unknown_average_width_does_not_imply_scalability() {
    let mut fixed = candidate("Bitmap Fixture", 400, FontSlant::Normal, 100);
    fixed.matched.metadata.size = PlatformFontSize::Fixed {
        device_ppem_26_6: 16 * 64,
    };
    let (resolver, _, _) = resolver(vec![fixed]);
    // GNU font_delete_unmatched compares an entity's average width only
    // when it is known and nonzero. A fixed strike does not imply that width.
    for average_width in [0, 120] {
        let listed = resolver.list_entities(
            &FontEntityQuery::default(),
            &FontEntityListConstraints {
                average_width: Some(average_width),
                ..Default::default()
            },
            None,
            Some(1),
        );
        assert_eq!(listed.len(), 1, "requested average width {average_width}");
    }
}

#[test]
fn accepted_listing_explicit_zero_preference_ranks_scalable_before_fixed() {
    let mut fixed = candidate("Fixed Fixture", 400, FontSlant::Normal, 100);
    fixed.matched.metadata.size = PlatformFontSize::Fixed {
        device_ppem_26_6: 16 * 64,
    };
    let (resolver, _, _) = resolver(vec![
        fixed,
        candidate("Scalable Fixture", 400, FontSlant::Normal, 100),
    ]);
    let query = FontEntityQuery::default();
    let unconstrained = FontEntityListConstraints::default();
    // GNU font_score distinguishes a nil size from a numeric zero even
    // though font_delete_unmatched imposes no primary size restriction.
    for (preferred, first) in [
        (unconstrained.clone(), "Fixed Fixture"),
        (
            FontEntityListConstraints {
                pixel_size: Some(0),
                ..Default::default()
            },
            "Scalable Fixture",
        ),
    ] {
        let listed =
            resolver.list_entities(&query, &unconstrained, Some((&query, &preferred)), Some(1));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].matched.family(), first);
    }
}

#[test]
fn accepted_listing_filters_metadata_before_finalizing_and_stops_at_accepted_maximum() {
    let (resolver, _, finalized) = resolver(
        ["Rejected", "Accepted", "Unvisited"]
            .into_iter()
            .map(|name| candidate(name, 400, FontSlant::Normal, 100))
            .collect(),
    );
    let mut inspected = Vec::new();
    let mut visited = Vec::new();
    resolver
        .try_visit_entities::<()>(
            &FontEntityQuery::default(),
            &FontEntityListConstraints::default(),
            None,
            Some(1),
            Some(&mut |listed| {
                inspected.push(listed.candidate.family().to_owned());
                assert_eq!(listed.driver, Some(intern("ftcrhb")));
                assert_eq!(listed.registry.as_deref(), Some("iso10646-1"));
                Ok(listed.candidate.family() != "Rejected")
            }),
            &mut |entity| {
                visited.push(entity.matched.family().to_owned());
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(inspected, ["Rejected", "Accepted"]);
    assert_eq!(visited, ["Accepted"]);
    assert_eq!(*finalized.lock().unwrap(), ["Accepted"]);
}

#[test]
fn accepted_listing_preserves_filter_error_before_primary_numeric_rejection() {
    let (resolver, _, finalized) =
        resolver(vec![candidate("Regular", 400, FontSlant::Normal, 100)]);
    let result = resolver.try_visit_entities(
        &FontEntityQuery::default().with_weight(700),
        &FontEntityListConstraints {
            pixel_size: Some(999),
            ..Default::default()
        },
        None,
        Some(1),
        Some(&mut |_| Err("original invalid-regexp")),
        &mut |_| panic!("a failed filter cannot publish an entity"),
    );
    assert_eq!(result, Err("original invalid-regexp"));
    assert!(finalized.lock().unwrap().is_empty());
}

#[test]
fn accepted_listing_preference_ranks_only_survivors_and_finalizes_only_selected() {
    let (resolver, _, finalized) = resolver(vec![
        candidate("Regular", 400, FontSlant::Normal, 100),
        candidate("Rejected Bold", 700, FontSlant::Normal, 100),
        candidate("Bold", 700, FontSlant::Normal, 100),
    ]);
    let mut inspected = Vec::new();
    let mut visited = Vec::new();
    resolver
        .try_visit_entities::<()>(
            &FontEntityQuery::default(),
            &FontEntityListConstraints::default(),
            Some((
                &FontEntityQuery::default().with_weight(700),
                &FontEntityListConstraints::default(),
            )),
            Some(1),
            Some(&mut |listed| {
                inspected.push(listed.candidate.family().to_owned());
                Ok(listed.candidate.family() != "Rejected Bold")
            }),
            &mut |entity| {
                visited.push(entity.matched.family().to_owned());
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(inspected, ["Regular", "Rejected Bold", "Bold"]);
    assert_eq!(visited, ["Bold"]);
    assert_eq!(*finalized.lock().unwrap(), ["Bold"]);
}

#[test]
fn accepted_listing_visitor_break_skips_remaining_filters_and_finalizations() {
    let (resolver, _, finalized) = resolver(vec![
        candidate("First", 400, FontSlant::Normal, 100),
        candidate("Unvisited", 400, FontSlant::Normal, 100),
    ]);
    let mut inspected = 0;
    resolver
        .try_visit_entities::<()>(
            &FontEntityQuery::default(),
            &FontEntityListConstraints::default(),
            None,
            None,
            Some(&mut |_| {
                inspected += 1;
                Ok(true)
            }),
            &mut |_| std::ops::ControlFlow::Break(()),
        )
        .unwrap();
    assert_eq!(inspected, 1);
    assert_eq!(*finalized.lock().unwrap(), ["First"]);
}

#[test]
fn accepted_listing_failed_finalization_does_not_consume_the_result_limit() {
    let mut unavailable = candidate("Unavailable", 400, FontSlant::Normal, 100);
    unavailable.matched.identity = ResolvedFontIdentity::from_file("/different.ttf", 0, None);
    let mut exact = candidate("Exact", 400, FontSlant::Normal, 100);
    replace_file_identity(
        &mut exact,
        ResolvedFontIdentity::from_file("/fixture/collection.ttc", 7, None),
    );
    let identity = exact.matched.identity.clone();
    let (resolver, _, finalized) = resolver(vec![unavailable, exact]);
    let mut visited = Vec::new();
    resolver
        .try_visit_entities::<()>(
            &FontEntityQuery::default(),
            &FontEntityListConstraints::default(),
            None,
            Some(1),
            Some(&mut |_| Ok(true)),
            &mut |entity| {
                visited.push(entity.matched.identity);
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(visited, [identity]);
    assert_eq!(*finalized.lock().unwrap(), ["Unavailable", "Exact"]);
}
