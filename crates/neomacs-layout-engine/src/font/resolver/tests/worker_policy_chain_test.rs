//! Regression cases use only the pre-existing capture/resolve interface, so
//! they can first run against the old bounded implementation as genuine reds.
use super::*;
use crate::font_backend::{
    FontFamilyName, PlatformFontCandidateLocator, PlatformFontDesignMetrics, PlatformFontMetadata,
};
use neomacs_display_protocol::DeviceScale;
use neomacs_display_protocol::font::{FontFileAsset, ResolvedFontIdentity};
use std::sync::{Arc, Mutex};

const BASE_FAMILY: &str = "font-policy-base";
const FIRST_WINNER: &str = "font-policy-first-winner";
const LAST_WINNER: &str = "font-policy-fallback-39";

#[derive(Clone, Copy)]
enum FontsetScenario {
    LongSpecificAndFallback,
    FirstWinnerThenLongName,
    ExplicitStop,
}

fn installed_fontset(scenario: FontsetScenario) -> neovm_core::emacs_core::Context {
    let mut eval = neovm_core::emacs_core::Context::new();
    match scenario {
        FontsetScenario::LongSpecificAndFallback | FontsetScenario::ExplicitStop => {
            eval.eval_str(
                r#"(progn
                   (set-fontset-font t ?好 "font-policy-specific-0")
                   (let ((i 0))
                     (while (< i 39)
                       (set-fontset-font t ?好 (format "font-policy-specific-%d" (1+ i)) nil 'append)
                       (setq i (1+ i))))
                   (set-fontset-font t nil "font-policy-fallback-0")
                   (let ((i 0))
                     (while (< i 39)
                       (set-fontset-font t nil (format "font-policy-fallback-%d" (1+ i)) nil 'append)
                       (setq i (1+ i)))))"#,
            ).unwrap();
        }
        FontsetScenario::FirstWinnerThenLongName => {
            eval.eval_str(
                r#"(progn
                   (set-fontset-font t ?好 "font-policy-first-winner")
                   (set-fontset-font t ?好 (make-string 16384 ?W) nil 'append)
                   (set-fontset-font t nil nil))"#,
            )
            .unwrap();
        }
    }
    match scenario {
        FontsetScenario::ExplicitStop => {
            eval.eval_str("(set-fontset-font t ?好 nil nil 'append)")
                .unwrap();
        }
        FontsetScenario::LongSpecificAndFallback | FontsetScenario::FirstWinnerThenLongName => {}
    }
    eval
}

fn size() -> FontSelectionSize {
    FontSelectionSize::new(13.0, DeviceScale::new(1.0).unwrap())
}

fn capture() -> FrozenCharacterPolicies {
    FrozenCharacterPolicies::capture(&[(BASE_FAMILY, '好', 400, false, size())], 4096)
        .expect("valid long chains and unused later names must remain capturable")
}

/// Every query is recorded with its typed scope. Only the designated explicit
/// family wins. Generic fallback deliberately succeeds, exposing any accidental
/// fallback past ExplicitNone instead of making that error look harmless.
struct RecordingBackend {
    winner: FontFamilyName,
    scopes: Arc<Mutex<Vec<FontCandidateScope>>>,
}

impl RecordingBackend {
    fn candidate(&self) -> FontCandidate {
        let path = "/fixture/font-policy-winner.ttf";
        FontCandidate {
            matched: PlatformFontCandidate {
                identity: ResolvedFontIdentity::from_file(path, 0, None),
                locator: PlatformFontCandidateLocator::File(FontFileAsset::new(path, 0).unwrap()),
                metadata: PlatformFontMetadata {
                    foundry: None,
                    family: self.winner.as_str().to_owned(),
                    weight: Some(400),
                    slant: FontSlant::Normal,
                    width: Some(FontWidth::Normal),
                    spacing: Some(0),
                    design_metrics: Some(PlatformFontDesignMetrics::default()),
                    size: PlatformFontSize::Scalable,
                },
            },
        }
    }
}

impl FontBackend for RecordingBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }
    fn list_families(&self) -> Vec<FontFamilyName> {
        vec![self.winner.clone()]
    }
    fn resolve_family(&self, family: &str) -> String {
        family.to_owned()
    }
    fn family_prefers_monospace(&self, _family: &str) -> bool {
        false
    }
    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.scopes.lock().unwrap().push(query.scope.clone());
        match &query.scope {
            FontCandidateScope::Family(family) => {
                if family == &self.winner {
                    vec![self.candidate()]
                } else {
                    Vec::new()
                }
            }
            FontCandidateScope::All | FontCandidateScope::NativeFallback { .. } => {
                vec![self.candidate()]
            }
        }
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn resolver(winner: &str) -> (FontResolver, Arc<Mutex<Vec<FontCandidateScope>>>) {
    let scopes = Arc::new(Mutex::new(Vec::new()));
    let resolver = FontResolver::new(Box::new(RecordingBackend {
        winner: FontFamilyName::new(winner).unwrap(),
        scopes: Arc::clone(&scopes),
    }));
    (resolver, scopes)
}

fn resolve(resolver: &FontResolver) -> Option<PlatformFontMatch> {
    resolver.resolve_for_char(
        BASE_FAMILY,
        '好',
        400,
        FontSlant::Normal,
        FontWidth::Normal,
        size(),
    )
}

fn expected_long_chain() -> Vec<FontCandidateScope> {
    ["specific", "fallback"]
        .into_iter()
        .flat_map(|part| {
            (0..40).map(move |index| {
                FontCandidateScope::Family(
                    FontFamilyName::new(format!("font-policy-{part}-{index}")).unwrap(),
                )
            })
        })
        .collect()
}

#[test]
fn frozen_long_chain_reaches_specific_then_fallback_winner_after_thirty_two_entries() {
    let _eval = installed_fontset(FontsetScenario::LongSpecificAndFallback);
    let policy = Arc::new(capture());
    let (live, live_scopes) = resolver(LAST_WINNER);
    let expected = resolve(&live).expect("live long-chain winner");
    assert_eq!(*live_scopes.lock().unwrap(), expected_long_chain());
    let (mut worker, scopes) = resolver(LAST_WINNER);
    worker.install_worker_policy(policy);
    assert_eq!(resolve(&worker), Some(expected));
    assert_eq!(*scopes.lock().unwrap(), expected_long_chain());
    assert!(!worker.worker_policy_missing());
}

#[test]
fn frozen_first_winner_does_not_materialize_or_query_later_oversized_names() {
    let _eval = installed_fontset(FontsetScenario::FirstWinnerThenLongName);
    let policy = capture();
    assert!(policy.bytes() <= 4096);
    let (mut worker, scopes) = resolver(FIRST_WINNER);
    worker.install_worker_policy(Arc::new(policy));
    assert_eq!(resolve(&worker).unwrap().family(), FIRST_WINNER);
    assert_eq!(
        *scopes.lock().unwrap(),
        vec![FontCandidateScope::Family(
            FontFamilyName::new(FIRST_WINNER).unwrap()
        )]
    );
    assert!(!worker.worker_policy_missing());
}

#[test]
fn frozen_explicit_stop_after_forty_entries_never_tries_fallback_or_generic_scope() {
    let _eval = installed_fontset(FontsetScenario::ExplicitStop);
    let policy = capture();
    let (mut worker, scopes) = resolver(LAST_WINNER);
    worker.install_worker_policy(Arc::new(policy));
    assert!(resolve(&worker).is_none());
    let expected = expected_long_chain();
    assert_eq!(*scopes.lock().unwrap(), expected[..40]);
    assert!(!worker.worker_policy_missing());
}

#[test]
fn frozen_long_chain_survives_live_mutation_and_evaluator_drop_on_a_detached_thread() {
    let mut eval = installed_fontset(FontsetScenario::LongSpecificAndFallback);
    let policy = capture();
    eval.eval_str("(progn (set-fontset-font t ?好 nil) (set-fontset-font t nil nil))")
        .unwrap();
    let (live, _) = resolver(LAST_WINNER);
    assert!(
        resolve(&live).is_none(),
        "live state must actually differ from captured policy"
    );
    drop(eval);
    std::thread::spawn(move || {
        let (mut worker, scopes) = resolver(LAST_WINNER);
        worker.install_worker_policy(Arc::new(policy));
        assert_eq!(resolve(&worker).unwrap().family(), LAST_WINNER);
        assert_eq!(*scopes.lock().unwrap(), expected_long_chain());
        assert!(!worker.worker_policy_missing());
    })
    .join()
    .unwrap();
}
