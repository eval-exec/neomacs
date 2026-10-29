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

#[test]
fn downloads_and_verifies_the_desktop_font_set() {
    let directory = desktop_fonts().directory();
    for name in [
        "UbuntuMono[wght].ttf",
        "UbuntuMono-Italic[wght].ttf",
        "Ubuntu[wdth,wght].ttf",
        "Ubuntu-Italic[wdth,wght].ttf",
        "DejaVuSansMono.ttf",
        "DejaVuSansMono-Bold.ttf",
        "DejaVuSans.ttf",
        "DejaVuSans-Bold.ttf",
    ] {
        let path = directory.join(name);
        assert!(path.is_file(), "{}", path.display());
        assert!(std::fs::metadata(&path).unwrap().len() > 0, "{name}");
    }
    // The suite's fontconfig points at this directory alone; a stray face
    // would change family selection behind the fixtures' backs.  Ignore
    // non-font leftovers (a crash can leave a `.partial`), which fontconfig
    // does not scan.
    let mut faces: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("ttf" | "otf")
            )
        })
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    faces.sort();
    assert_eq!(
        faces,
        [
            "DejaVuSans-Bold.ttf",
            "DejaVuSans.ttf",
            "DejaVuSansMono-Bold.ttf",
            "DejaVuSansMono.ttf",
            "Ubuntu-Italic[wdth,wght].ttf",
            "UbuntuMono-Italic[wght].ttf",
            "UbuntuMono[wght].ttf",
            "Ubuntu[wdth,wght].ttf",
        ]
    );
}
