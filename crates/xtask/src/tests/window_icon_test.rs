use super::*;

/// Which pixel sizes the PNG at PATH declares, from its IHDR chunk.
fn png_dimensions(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).expect("rendered PNG");
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "{}: PNG magic",
        path.display()
    );
    assert_eq!(&bytes[12..16], b"IHDR", "{}: IHDR first", path.display());
    (
        u32::from_be_bytes(bytes[16..20].try_into().expect("width bytes")),
        u32::from_be_bytes(bytes[20..24].try_into().expect("height bytes")),
    )
}

#[test]
fn render_window_icon_writes_the_exact_macos_icon_family() {
    let repo_root = crate::repository_root();
    let out_dir = tempfile::tempdir().expect("tempdir for iconset");
    render_iconset(&repo_root.join(CANONICAL_ICON), out_dir.path()).expect("render iconset");

    for &(name, size) in ICONSET {
        let path = out_dir.path().join(name);
        assert!(path.is_file(), "{name} must be written");
        assert_eq!(
            png_dimensions(&path),
            (size, size),
            "{name} must be {size}x{size}"
        );
    }

    // `iconutil` rejects an iconset with extra entries, so the family is
    // exactly the table above.
    let entries: Vec<String> = fs::read_dir(out_dir.path())
        .expect("read iconset")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(entries.len(), ICONSET.len());
}

#[test]
fn render_window_icon_refuses_arguments_it_does_not_understand() {
    let repo_root = crate::repository_root();
    let error = run(&repo_root, [std::ffi::OsString::from("--nope")])
        .expect_err("unknown argument must be refused");
    assert!(error.to_string().contains("--nope"), "{error}");
    let error = run(&repo_root, std::iter::empty()).expect_err("missing --out-dir must be refused");
    assert!(error.to_string().contains("--out-dir"), "{error}");
}
