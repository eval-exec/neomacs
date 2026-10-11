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
}
