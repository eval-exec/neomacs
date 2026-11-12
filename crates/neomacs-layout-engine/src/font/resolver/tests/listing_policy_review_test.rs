use super::*;

fn fixed_candidates() -> Vec<FontCandidate> {
    [14, 24]
        .into_iter()
        .map(|pixels| {
            let mut candidate = candidate("Rescale Trade", 400, FontSlant::Normal, 100);
            candidate.matched.metadata.size = PlatformFontSize::Fixed {
                device_ppem_26_6: pixels * 64,
            };
            candidate
        })
        .collect()
}

#[test]
fn listing_rescale_is_evaluated_per_fixed_candidate_before_preference_ranking() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: fixed_candidates(),
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Rescale Trade"));
    let primary = FontEntityListConstraints::default();
    let preferred = FontEntityListConstraints {
        pixel_size: Some(12),
        ..Default::default()
    };
    let mut seen = Vec::new();
    let mut rescale = |candidate: &ListedFontCandidate| -> Result<f64, &'static str> {
        Ok(
            if matches!(
                candidate.candidate.metadata.size,
                PlatformFontSize::Fixed {
                    device_ppem_26_6: 1536
                }
            ) {
                2.0
            } else {
                1.0
            },
        )
    };
    resolver
        .try_visit_entities(
            &query,
            &primary,
            Some((&query, &preferred)),
            None,
            Some(&mut rescale),
            |candidate| {
                seen.push(candidate.candidate.metadata.size);
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(
        seen[0],
        PlatformFontSize::Fixed {
            device_ppem_26_6: 24 * 64
        }
    );
}

#[test]
fn listing_rescale_propagates_a_reached_policy_error() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: fixed_candidates(),
    }));
    let query = FontEntityQuery::new(None);
    let primary = FontEntityListConstraints::default();
    let preferred = FontEntityListConstraints {
        pixel_size: Some(12),
        ..Default::default()
    };
    let mut rescale =
        |_: &ListedFontCandidate| -> Result<f64, &'static str> { Err("invalid rescale") };
    let mut visited = 0;
    assert_eq!(
        resolver.try_visit_entities(
            &query,
            &primary,
            Some((&query, &preferred)),
            None,
            Some(&mut rescale),
            |_| {
                visited += 1;
                std::ops::ControlFlow::Continue(())
            }
        ),
        Err("invalid rescale")
    );
    assert_eq!(visited, 0);
}

#[test]
fn listing_score_uses_configured_scalar_shifts_instead_of_a_fixed_order() {
    let normal_wide = candidate_listing_score(
        Some(FontWeight::Normal),
        None,
        Some(FontWidth::Normal),
        Some(FontWeight::Normal),
        FontSlant::Normal,
        Some(FontWidth::Expanded),
        None,
    )
    .unwrap();
    let bold_normal = candidate_listing_score(
        Some(FontWeight::Normal),
        None,
        Some(FontWidth::Normal),
        Some(FontWeight::Bold),
        FontSlant::Normal,
        Some(FontWidth::Normal),
        None,
    )
    .unwrap();
    assert!(bold_normal.listing_value([23, 16, 9, 2]) < normal_wide.listing_value([23, 16, 9, 2]));
    assert!(normal_wide.listing_value([16, 9, 23, 2]) < bold_normal.listing_value([16, 9, 23, 2]));
}

#[test]
fn listing_filter_error_precedes_primary_numeric_rejection() {
    for prefer_size in [None, Some(12)] {
        let resolver = FontResolver::new(Box::new(CandidateBackend {
            candidates: fixed_candidates(),
        }));
        let query = FontEntityQuery::new(None).with_weight(700);
        let primary = FontEntityListConstraints {
            pixel_size: Some(200),
            ..Default::default()
        };
        let preferred = FontEntityListConstraints {
            pixel_size: Some(12),
            ..Default::default()
        };
        let preference = prefer_size.map(|_| (&query, &preferred));
        let mut filter = |_: &ListedFontCandidate| -> Result<bool, &'static str> {
            Err("invalid ignored regexp")
        };
        let mut visited = false;
        assert_eq!(
            resolver.try_visit_entities(
                &query,
                &primary,
                preference,
                Some(&mut filter),
                None,
                |_| {
                    visited = true;
                    std::ops::ControlFlow::Continue(())
                }
            ),
            Err("invalid ignored regexp")
        );
        assert!(!visited);
    }
}

#[test]
fn listing_single_survivor_skips_rescaling_after_ignored_filtering() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: fixed_candidates(),
    }));
    let query = FontEntityQuery::new(None);
    let primary = FontEntityListConstraints::default();
    let preferred = FontEntityListConstraints {
        pixel_size: Some(12),
        ..Default::default()
    };
    let mut filter = |candidate: &ListedFontCandidate| -> Result<bool, &'static str> {
        Ok(matches!(
            candidate.candidate.metadata.size,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 896
            }
        ))
    };
    let mut scale = |_: &ListedFontCandidate| -> Result<f64, &'static str> {
        panic!("one survivor is never scored")
    };
    let mut seen = 0;
    resolver
        .try_visit_entities(
            &query,
            &primary,
            Some((&query, &preferred)),
            Some(&mut filter),
            Some(&mut scale),
            |_| {
                seen += 1;
                std::ops::ControlFlow::Break(())
            },
        )
        .unwrap();
    assert_eq!(seen, 1);
}

#[test]
fn listing_factor_two_failure_retains_the_entity_at_maximum_score() {
    let resolver = FontResolver::new(Box::new(CandidateBackend {
        candidates: fixed_candidates(),
    }));
    let query = FontEntityQuery::new(None);
    let primary = FontEntityListConstraints::default();
    let preferred = FontEntityListConstraints {
        pixel_size: Some(12),
        ..Default::default()
    };
    let mut scale = |candidate: &ListedFontCandidate| -> Result<f64, &'static str> {
        Ok(
            if matches!(
                candidate.candidate.metadata.size,
                PlatformFontSize::Fixed {
                    device_ppem_26_6: 1536
                }
            ) {
                0.5
            } else {
                1.0
            },
        )
    };
    let mut seen = Vec::new();
    resolver
        .try_visit_entities(
            &query,
            &primary,
            Some((&query, &preferred)),
            None,
            Some(&mut scale),
            |candidate| {
                seen.push(candidate.candidate.metadata.size);
                std::ops::ControlFlow::Continue(())
            },
        )
        .unwrap();
    assert_eq!(
        seen,
        vec![
            PlatformFontSize::Fixed {
                device_ppem_26_6: 896
            },
            PlatformFontSize::Fixed {
                device_ppem_26_6: 1536
            }
        ]
    );
}
