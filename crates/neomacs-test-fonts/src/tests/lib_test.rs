use crate::*;

#[test]
fn font_fixture_cache_root_prefers_the_explicit_directory() {
    let workspace = std::path::PathBuf::from("/work");
    let resolved = resolve_font_fixture_cache_root(
        Some("/shared/fonts".into()),
        Some("/xdg".into()),
        Some("/home/tester".into()),
        workspace,
    );
    assert_eq!(resolved, std::path::PathBuf::from("/shared/fonts"));
}

#[test]
fn font_fixture_cache_root_uses_xdg_then_home_then_the_workspace() {
    let workspace = std::path::PathBuf::from("/work");
    let xdg = resolve_font_fixture_cache_root(
        None,
        Some("/xdg".into()),
        Some("/home/tester".into()),
        workspace.clone(),
    );
    assert_eq!(xdg, std::path::PathBuf::from("/xdg/neomacs/font-fixtures"));
    let home =
        resolve_font_fixture_cache_root(None, None, Some("/home/tester".into()), workspace.clone());
    assert_eq!(
        home,
        std::path::PathBuf::from("/home/tester/.cache/neomacs/font-fixtures")
    );
    let fallback = resolve_font_fixture_cache_root(None, Some("".into()), None, workspace.clone());
    assert_eq!(
        fallback,
        std::path::PathBuf::from("/work/tmp/font-fixtures")
    );
}

#[test]
fn pins_that_share_a_stem_keep_distinct_partial_paths() {
    let cache = std::path::Path::new("/cache");
    let names = [
        "spleen-8x16.bdf",
        "spleen-8x16.pcf",
        "spleen-8x16.otb",
        "spleen-8x16.woff",
        "spleen-8x16.woff2",
    ];
    let partials: Vec<_> = names
        .iter()
        .map(|name| partial_download_path(&cache.join(name)))
        .collect();
    for (name, partial) in names.iter().zip(&partials) {
        assert_eq!(partial, &cache.join(format!("{name}.partial")), "{name}");
    }
    for index in 0..partials.len() {
        for other in (index + 1)..partials.len() {
            assert_ne!(partials[index], partials[other]);
        }
    }
}

#[test]
fn fixture_fetch_message_names_the_url_the_pin_and_the_io_error() {
    let message = fixture_fetch_message(
        "https://example.invalid/font.zip",
        "abc123",
        &"unexpected end of file",
    );
    assert_eq!(
        message,
        "FIXTURE-FETCH: url=https://example.invalid/font.zip sha256=abc123 error=unexpected end of file"
    );
}

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
