//! GNU w32font.c admission/publication exercised through the shared resolver.
//! This is a portable DirectWrite stand-in, not a native Windows runtime test.
use super::*;
use crate::font_backend::FontSignatureRequirement;
use std::sync::Mutex;

struct SignatureBackend {
    kind: FontBackendKind,
    queries: Arc<Mutex<Vec<FontCandidateQuery>>>,
}

impl FontBackend for SignatureBackend {
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
        self.queries.lock().unwrap().push(query.clone());
        // Two physical faces with identical public family/style metadata:
        // the first has BMP coverage, the second also has a SIP signature.
        // Native signature-bit interpretation is tested in windows/query_policy.
        [false, true]
            .into_iter()
            .filter(|sip| {
                query.signature_requirement != Some(FontSignatureRequirement::Sip) || *sip
            })
            .map(|sip| {
                let mut candidate = candidate("Registry Boundary", 400, FontSlant::Normal, 100);
                let path = if sip {
                    "/fixture/sip.ttf"
                } else {
                    "/fixture/bmp.ttf"
                };
                candidate.matched.identity = ResolvedFontIdentity::from_file(path, 0, None);
                candidate.matched.locator =
                    PlatformFontCandidateLocator::File(FontFileAsset::new(path, 0).unwrap());
                candidate
            })
            .collect()
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn resolver(kind: FontBackendKind) -> (FontResolver, Arc<Mutex<Vec<FontCandidateQuery>>>) {
    let queries = Arc::new(Mutex::new(Vec::new()));
    (
        FontResolver::new(Box::new(SignatureBackend {
            kind,
            queries: Arc::clone(&queries),
        })),
        queries,
    )
}

#[test]
fn windows_signature_listing_filters_bmp_before_maximum_one() {
    let (resolver, queries) = resolver(FontBackendKind::DirectWrite);
    let query =
        FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry("unicode-sip");
    let listed = resolver.list_entities(&query, &Default::default(), None, Some(1));
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].candidate.identity.file_path.as_deref(),
        Some("/fixture/sip.ttf")
    );
    let queries = queries.lock().unwrap();
    assert_eq!(
        queries[0].signature_requirement,
        Some(FontSignatureRequirement::Sip)
    );
    assert!(queries[0].charset_ranges.is_empty());
    assert!(queries[0].languages.is_empty());
    assert!(matches!(queries[0].required, RequiredFontCoverage::Any));
}

#[test]
fn windows_signature_results_preserve_the_explicit_registry_without_changing_fontconfig() {
    for kind in [FontBackendKind::DirectWrite, FontBackendKind::Fontconfig] {
        for registry in [
            None,
            Some("unknown"),
            Some("ascii-0"),
            Some("iso10646-1"),
            Some("unicode-bmp"),
            Some("unicode-sip"),
            Some("ISO8859-2"),
        ] {
            if kind == FontBackendKind::Fontconfig
                && matches!(registry, Some("unknown" | "unicode-sip"))
            {
                continue;
            }
            let (resolver, _) = resolver(kind);
            let mut query = FontEntityQuery::new(FontFamilyName::new("Registry Boundary"));
            if let Some(registry) = registry {
                query = query.with_registry(registry);
            }
            let expected = if kind == FontBackendKind::DirectWrite {
                registry.unwrap_or("iso10646-1")
            } else {
                "iso10646-1"
            };
            let listed = resolver.list_entities(&query, &Default::default(), None, Some(1));
            assert_eq!(listed.len(), 1, "{kind:?} {registry:?}");
            assert_eq!(
                listed[0].registry.as_deref(),
                Some(expected),
                "{kind:?} {registry:?}"
            );
            let matched = resolver
                .resolve_entity(&query.with_selection(FontSpecSelection::Enumerate))
                .unwrap();
            assert_eq!(
                matched.registry.as_deref(),
                Some(expected),
                "resolved {kind:?} {registry:?}"
            );
        }
    }
}

#[test]
fn windows_signature_shortcuts_are_typed_only_for_directwrite() {
    for kind in [
        FontBackendKind::DirectWrite,
        FontBackendKind::CoreText,
        FontBackendKind::Fontconfig,
    ] {
        for (registry, expected) in [
            ("iso10646-1", Some(FontSignatureRequirement::Unicode)),
            ("unicode-bmp", Some(FontSignatureRequirement::Bmp)),
            ("unicode-sip", Some(FontSignatureRequirement::Sip)),
            ("ascii-0", None),
            ("unknown", None),
            ("iso8859-2", None),
        ] {
            if kind == FontBackendKind::Fontconfig && matches!(registry, "unknown" | "unicode-sip")
            {
                continue;
            }
            let (resolver, queries) = resolver(kind);
            let query = FontEntityQuery::new(FontFamilyName::new("Registry Boundary"))
                .with_registry(registry);
            assert!(
                !resolver
                    .list_entities(&query, &Default::default(), None, Some(1))
                    .is_empty()
            );
            assert_eq!(
                queries.lock().unwrap()[0].signature_requirement,
                if kind == FontBackendKind::DirectWrite {
                    expected
                } else {
                    None
                },
                "{kind:?} {registry}"
            );
        }
    }
}

#[test]
fn windows_signature_survives_live_and_detached_character_policies() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(r#"(set-fontset-font t ?好 (font-spec :family "Registry Boundary" :registry "unicode-sip"))"#).unwrap();
    let size = FontSelectionSize::new(13.0, DeviceScale::new(1.0).unwrap());
    for detached in [false, true] {
        let (mut resolver, queries) = resolver(FontBackendKind::DirectWrite);
        if detached {
            resolver.install_worker_policy(Arc::new(
                FrozenCharacterPolicies::capture(
                    &[("Registry Boundary", '好', 400, false, size)],
                    4096,
                )
                .unwrap(),
            ));
        }
        let matched = resolver
            .resolve_for_char(
                "Registry Boundary",
                '好',
                400,
                FontSlant::Normal,
                FontWidth::Normal,
                size,
            )
            .unwrap();
        assert_eq!(
            matched.identity.file_path.as_deref(),
            Some("/fixture/sip.ttf"),
            "detached={detached}"
        );
        let queries = queries.lock().unwrap();
        assert_eq!(
            queries[0].signature_requirement,
            Some(FontSignatureRequirement::Sip),
            "detached={detached}"
        );
        assert!(queries[0].charset_ranges.is_empty());
        assert!(matches!(
            queries[0].required,
            RequiredFontCoverage::Character('好')
        ));
    }
}
