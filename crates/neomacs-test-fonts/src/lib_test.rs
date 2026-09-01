use super::*;

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
    let bytes = std::fs::read(super::woff2_collection()).expect("downloaded WOFF2 collection");
    assert_eq!(&bytes[..8], b"wOF2ttcf");
}
