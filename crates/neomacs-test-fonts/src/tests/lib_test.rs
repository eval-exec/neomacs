use crate::*;

#[test]
fn downloads_and_verifies_every_public_fixture() {
    let fixtures = spleen_2_2_0();
    for path in [
        fixtures.bdf(),
        fixtures.pcf(),
        fixtures.pcf_gz(),
        fixtures.otb(),
        fixtures.woff(),
        fixtures.woff2(),
    ] {
        assert!(path.is_file(), "{}", path.display());
    }
    assert!(gzip_expands_to(&fixtures.pcf_gz(), &fixtures.pcf()).unwrap());
}

#[test]
fn downloads_the_nonzero_face_woff2_collection_fixture() {
    let bytes = std::fs::read(crate::woff2_collection()).expect("downloaded WOFF2 collection");
    assert_eq!(&bytes[..8], b"wOF2ttcf");
}

#[test]
fn downloads_and_verifies_pinned_arabic_hebrew_and_serif_faces() {
    for (path, expected) in [
        (
            noto_sans_arabic_variable(),
            NOTO_SANS_ARABIC_VARIABLE_SHA256,
        ),
        (
            noto_sans_hebrew_variable(),
            NOTO_SANS_HEBREW_VARIABLE_SHA256,
        ),
        (noto_serif_variable(), NOTO_SERIF_VARIABLE_SHA256),
    ] {
        verify_sha256(path, expected).expect("pinned shaping face checksum");
    }
}
