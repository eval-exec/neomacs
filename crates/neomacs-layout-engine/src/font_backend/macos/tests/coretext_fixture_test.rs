use super::CoreTextBackend;
use crate::font::resolver::{FontEntityQuery, FontResolver};
use crate::font_backend::FontFamilyName;
use neovm_core::emacs_core::eval::FontSpecSelection;
use neovm_core::face::{FontSlant, FontWeight, FontWidth};
use objc2_core_foundation::{CFRetained, CFURL};
use objc2_core_text::{
    CTFontManagerRegisterFontsForURL, CTFontManagerScope, CTFontManagerUnregisterFontsForURL,
};
use std::path::Path;
use std::ptr;

/// Only unregister a registration successfully owned by this test. Retaining
/// its URL and leaving the pinned file in place satisfies CoreText's lifetime
/// contract; process scope cannot change another application's font catalog.
struct ProcessFontRegistration(CFRetained<CFURL>);

impl ProcessFontRegistration {
    fn new(path: &Path) -> Self {
        let url = CFURL::from_file_path(path).expect("pinned fixture file URL");
        // SAFETY: the URL is retained throughout registration, the fixture is
        // not moved, and the optional CFError output pointer is documented null.
        assert!(
            unsafe {
                CTFontManagerRegisterFontsForURL(&url, CTFontManagerScope::Process, ptr::null_mut())
            },
            "CoreText must register the pinned Thin fixture for this process"
        );
        Self(url)
    }
}

impl Drop for ProcessFontRegistration {
    fn drop(&mut self) {
        // SAFETY: this URL registered successfully with the same process scope;
        // the retained URL is live and a null error-output pointer is permitted.
        // Avoid a second panic during unwinding; process scope also bounds the
        // registration lifetime if the OS reports an unregistration failure.
        let _ = unsafe {
            CTFontManagerUnregisterFontsForURL(
                &self.0,
                CTFontManagerScope::Process,
                ptr::null_mut(),
            )
        };
    }
}

fn fixture_name(face: &ttf_parser::Face<'_>, name_id: u16) -> Option<String> {
    face.names()
        .into_iter()
        .find(|name| name.name_id == name_id && name.is_unicode())
        .and_then(|name| name.to_string())
}

#[test]
fn registered_coretext_thin_fixture_enumerates_and_opens_with_normal_preference() {
    let path = neomacs_test_fonts::mplus_1_code_thin();
    let bytes = std::fs::read(path).expect("pinned Thin fixture bytes");
    let face = ttf_parser::Face::parse(&bytes, 0).expect("pinned Thin SFNT");
    let postscript = fixture_name(&face, ttf_parser::name_id::POST_SCRIPT_NAME)
        .expect("fixture PostScript identity");
    let _registration = ProcessFontRegistration::new(path);
    let resolver = FontResolver::new(Box::new(CoreTextBackend::default()));

    // CoreText may expose the legacy family instead of the SFNT typographic
    // family. Discover its actual catalog name through the public resolver;
    // the fixture's exact PostScript identity excludes installed siblings.
    let discovered = resolver
        .resolve_entity(&FontEntityQuery::new(None).with_postscript_name(&postscript))
        .expect("registered fixture appears in the CoreText catalog");
    assert_eq!(
        Path::new(
            discovered
                .matched
                .file_path()
                .expect("registered fixture URL")
        )
        .canonicalize()
        .expect("selected fixture file"),
        path.canonicalize().expect("pinned fixture file")
    );
    let thin = FontEntityQuery::new(FontFamilyName::new(discovered.matched.family()))
        .with_postscript_name(&postscript)
        .with_weight(100)
        .with_slant(FontSlant::Normal)
        .with_width(FontWidth::Normal);
    let entity = resolver
        .resolve_entity(&thin)
        .expect("CoreText Thin matches GNU's Thin weight category");
    assert_eq!(
        entity.matched.identity.postscript_name.as_deref(),
        Some(postscript.as_str())
    );
    assert_eq!(
        FontWeight::from_css_weight(entity.matched.weight().expect("CoreText weight"))
            .gnu_numeric(),
        FontWeight::THIN.gnu_numeric()
    );
    assert_eq!(entity.matched.identity, discovered.matched.identity);

    let normal = thin.with_weight(400);
    assert!(
        resolver.resolve_entity(&normal).is_none(),
        "enumeration must keep Normal distinct from the pinned Thin face"
    );
    let opened = resolver
        .open_entity(&normal.with_selection(FontSpecSelection::OpenBySpec), 20)
        .expect("normal opening preference can fall back to the available Thin face");
    assert_eq!(opened.entity.matched.identity, entity.matched.identity);
    assert_eq!(opened.metrics.pixel_size, 20);
    assert!(opened.metrics.ascent > 0);
    assert!(opened.metrics.descent >= 0);
    assert!(opened.metrics.height > 0);
    assert!(opened.metrics.max_width > 0);
    assert!(opened.metrics.space_width > 0);

    // fontdb and CoreText can attach different diagnostic names to the same
    // file identity. Neither a different name nor an absent one may hide it.
    use crate::font_backend::{
        FontBackend, FontCandidateQuery, FontCandidateScope, RequiredFontCoverage, TextDirection,
    };
    let query = FontCandidateQuery {
        scope: FontCandidateScope::Family(
            FontFamilyName::new(discovered.matched.family()).unwrap(),
        ),
        required: RequiredFontCoverage::Any,
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        requested_weight: 400,
        requested_slant: FontSlant::Normal,
        requested_width: FontWidth::Normal,
        direction: TextDirection::LeftToRight,
    };
    for postscript_name in [Some("DifferentFontdbPostScriptName".to_owned()), None] {
        let mut identity = discovered.matched.identity.clone();
        identity.postscript_name = postscript_name;
        assert_eq!(identity, discovered.matched.identity);
        let backend = CoreTextBackend::default();
        let matches = backend.list_candidates_for_identity(&query, &identity);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matched.identity, identity);
        assert_eq!(
            matches[0].matched.identity.postscript_name.as_deref(),
            Some(postscript.as_str())
        );
        let files = backend.file_faces.lock().unwrap();
        assert_eq!(files.len(), 1, "exact lookup only reads the selected file");
        assert!(files.contains_key(Path::new(identity.file_path.as_deref().unwrap())));
    }
}

#[test]
fn exact_identity_miss_does_not_read_unrelated_font_files() {
    use crate::font_backend::{
        FontBackend, FontCandidateQuery, FontCandidateScope, RequiredFontCoverage, TextDirection,
    };
    use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};

    let backend = CoreTextBackend::default();
    let query = FontCandidateQuery {
        scope: FontCandidateScope::Family(
            FontFamilyName::new(backend.resolve_family("monospace")).unwrap(),
        ),
        required: RequiredFontCoverage::Any,
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        requested_weight: 400,
        requested_slant: FontSlant::Normal,
        requested_width: FontWidth::Normal,
        direction: TextDirection::LeftToRight,
    };
    assert!(!super::core_text_calls::candidates(&query).is_empty());
    let identity = ResolvedFontIdentity::from_native_with_variations(
        FontBackendKind::CoreText,
        "coretext:missing-fixture#0".into(),
        0,
        Some("NeomacsMissingExactIdentityFixture".into()),
        Vec::new(),
    );
    assert!(
        backend
            .list_candidates_for_identity(&query, &identity)
            .is_empty()
    );
    assert!(
        backend.file_faces.lock().unwrap().is_empty(),
        "an exact PostScript miss must not open unrelated candidate files"
    );

    // Passing the cheap prefilter is not enough: the complete stable key
    // must still match, including the file/instance key.
    let selected = backend.list_candidates(&query).into_iter().next().unwrap();
    assert_eq!(
        backend.list_candidates_for_identity(&query, &selected.matched.identity),
        vec![selected.clone()]
    );
    let mut wrong_key = selected.matched.identity;
    wrong_key.stable_key.push_str("-different-instance");
    // The path prefilter cannot distinguish faces/instances in one file;
    // disambiguating them requires resolving the file identity. Use a
    // fresh backend so the earlier list_candidates cache cannot hide that.
    let fresh = CoreTextBackend::default();
    assert!(
        fresh
            .list_candidates_for_identity(&query, &wrong_key)
            .is_empty()
    );
}

#[test]
fn native_identity_prefilter_uses_the_stable_key_even_with_an_opaque_url() {
    use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};

    let candidate = super::core_text_calls::Candidate {
        path: Some("/opaque/font-container".into()),
        postscript_name: "NativeFixture".into(),
        family: "Fixture".into(),
        weight: 400,
        slant: FontSlant::Normal,
        width: FontWidth::Normal,
        spacing: 0,
        variation_coords: Vec::new(),
    };
    // A URL is not proof that ttf-parser can open the container. Keep this
    // candidate's native fallback even when its diagnostic name differs.
    let mut identity = ResolvedFontIdentity::from_native_with_variations(
        FontBackendKind::CoreText,
        "coretext:NativeFixture#0".into(),
        0,
        Some("DifferentDiagnosticName".into()),
        Vec::new(),
    );
    assert!(CoreTextBackend::candidate_may_match_identity(
        &candidate, &identity
    ));
    identity.postscript_name = None;
    assert!(CoreTextBackend::candidate_may_match_identity(
        &candidate, &identity
    ));
    identity.stable_key.push_str("-different-instance");
    assert!(!CoreTextBackend::candidate_may_match_identity(
        &candidate, &identity
    ));
}
