use super::*;
use std::{fs, path::PathBuf};

struct CatalogConfig {
    config: NonNull<fc::FcConfig>,
    directory: tempfile::TempDir,
}

impl CatalogConfig {
    fn new(collection: bool, rules: &str, include_directory: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let fonts = directory.path().join("fonts");
        let cache = directory.path().join("cache");
        fs::create_dir(&fonts).unwrap();
        fs::create_dir(&cache).unwrap();
        let pinned = neomacs_test_fonts::dejavu_2_37();
        if collection {
            fs::write(
                fonts.join("fixture.ttc"),
                collection_bytes(&[
                    fs::read(pinned.mono_regular()).unwrap(),
                    fs::read(pinned.sans_regular()).unwrap(),
                ]),
            )
            .unwrap();
        } else {
            fs::copy(pinned.mono_regular(), fonts.join("mono.ttf")).unwrap();
            fs::copy(pinned.sans_regular(), fonts.join("sans.ttf")).unwrap();
        }
        let dir = if include_directory {
            format!("<dir>{}</dir>", xml(&fonts))
        } else {
            String::new()
        };
        let aliases: String = [
            ("monospace", "DejaVu Sans Mono"), ("sans-serif", "DejaVu Sans"),
            ("serif", "DejaVu Sans"), ("cursive", "DejaVu Sans"), ("fantasy", "DejaVu Sans"),
        ].into_iter().map(|(generic, family)| format!(
            "<alias><family>{generic}</family><prefer><family>{family}</family></prefer></alias>"
        )).collect();
        let text = CString::new(format!(
            "<fontconfig>{dir}<cachedir>{}</cachedir>{aliases}{rules}</fontconfig>",
            xml(&cache)
        ))
        .unwrap();
        unsafe extern "C" {
            fn FcConfigParseAndLoadFromMemory(
                config: *mut fc::FcConfig,
                buffer: *const fc::FcChar8,
                complain: fc::FcBool,
            ) -> fc::FcBool;
        }
        // SAFETY: this fixture exclusively owns the configuration; XML is
        // copied by Fontconfig. BuildFonts loads its private directory caches,
        // so selectfont is exercised rather than bypassed with application fonts.
        let config = NonNull::new(unsafe { fc::FcConfigCreate() }).unwrap();
        unsafe {
            assert_ne!(
                FcConfigParseAndLoadFromMemory(config.as_ptr(), text.as_ptr().cast(), 1),
                0
            );
            assert_ne!(fc::FcConfigBuildFonts(config.as_ptr()), 0);
        }
        Self { config, directory }
    }

    fn context(&self) -> FontconfigContext<'_> {
        // SAFETY: no mutation is exposed; the borrowed owner outlives all queries.
        unsafe { FontconfigContext::from_raw(self.config) }
    }
}

impl Drop for CatalogConfig {
    fn drop(&mut self) {
        // SAFETY: this fixture owns the config; its directory remains live.
        unsafe { fc::FcConfigDestroy(self.config.as_ptr()) };
    }
}

fn xml(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
}

fn reject_family(family: &str) -> String {
    format!(
        "<selectfont><rejectfont><pattern><patelt name=\"family\"><string>{family}</string></patelt></pattern></rejectfont></selectfont>"
    )
}

fn paths(database: &fontdb::Database) -> Vec<PathBuf> {
    database
        .faces()
        .map(|face| match &face.source {
            fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => path.clone(),
            fontdb::Source::Binary(_) => panic!("catalog sources retain file identity"),
        })
        .collect()
}

#[test]
fn native_catalog_excludes_rejected_fonts_before_fontdb_materialization() {
    let fixture = CatalogConfig::new(false, &reject_family("DejaVu Sans"), true);
    let (database, opened) = fixture.context().font_database_with_count().unwrap();
    assert_eq!(opened, 1, "only native-approved files may be opened");
    assert_eq!(
        paths(&database),
        vec![fixture.directory.path().join("fonts/mono.ttf")]
    );
    assert_eq!(
        database.family_name(&fontdb::Family::Monospace),
        "DejaVu Sans Mono"
    );
}

#[test]
fn native_catalog_preserves_configured_generic_family_defaults() {
    let fixture = CatalogConfig::new(false, "", true);
    let database = fixture.context().font_database().unwrap();
    for (family, expected) in [
        (fontdb::Family::Monospace, "DejaVu Sans Mono"),
        (fontdb::Family::SansSerif, "DejaVu Sans"),
        (fontdb::Family::Serif, "DejaVu Sans"),
        (fontdb::Family::Cursive, "DejaVu Sans"),
        (fontdb::Family::Fantasy, "DejaVu Sans"),
    ] {
        assert_eq!(database.family_name(&family), expected);
    }
}

#[test]
fn acceptfont_protects_an_allowed_face_from_a_broader_rejection() {
    let rules = "<selectfont><rejectfont><pattern><patelt name=\"scalable\"><bool>true</bool></patelt></pattern></rejectfont><acceptfont><pattern><patelt name=\"family\"><string>DejaVu Sans Mono</string></patelt></pattern></acceptfont></selectfont>";
    let fixture = CatalogConfig::new(false, rules, true);
    let (database, opened) = fixture.context().font_database_with_count().unwrap();
    assert_eq!(opened, 1);
    assert_eq!(
        paths(&database),
        vec![fixture.directory.path().join("fonts/mono.ttf")]
    );
}

#[test]
fn empty_and_all_rejected_native_catalogs_never_open_directory_fallback_fonts() {
    let reject_all = "<selectfont><rejectfont><pattern><patelt name=\"scalable\"><bool>true</bool></patelt></pattern></rejectfont></selectfont>";
    for (rules, include) in [("", false), (reject_all, true)] {
        let fixture = CatalogConfig::new(false, rules, include);
        let (database, opened) = fixture.context().font_database_with_count().unwrap();
        assert!(database.is_empty());
        assert_eq!(opened, 0);
    }
    for error in [FontCatalogError::Unavailable, FontCatalogError::Query] {
        assert!(crate::fontdb::font_database_from_native_result(Err(error)).is_empty());
    }
}

#[test]
fn vanished_approved_file_does_not_trigger_other_font_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = crate::fontdb::NativeFontCatalog {
        files: [(
            directory.path().join("vanished.ttf"),
            [0].into_iter().collect(),
        )]
        .into_iter()
        .collect(),
        generics: Vec::new(),
    };
    let (database, opened) = crate::fontdb::database_from_native_catalog(catalog);
    assert!(database.is_empty());
    assert_eq!(opened, 1, "only the approved source was attempted");
}

#[test]
fn approved_collection_file_does_not_publish_rejected_sibling_faces() {
    let fixture = CatalogConfig::new(true, &reject_family("DejaVu Sans"), true);
    let (database, opened) = fixture.context().font_database_with_count().unwrap();
    assert_eq!(opened, 1);
    assert_eq!(database.len(), 1);
    let face = database.faces().next().unwrap();
    assert_eq!(face.index, 0);
    assert!(
        face.families
            .iter()
            .any(|(name, _)| name == "DejaVu Sans Mono")
    );
}

#[test]
fn named_instance_only_authorization_does_not_broaden_to_the_default_face() {
    let file = neomacs_test_fonts::dejavu_2_37()
        .mono_regular()
        .to_path_buf();
    let catalog = crate::fontdb::NativeFontCatalog {
        files: [(file, [1 << 16].into_iter().collect())]
            .into_iter()
            .collect(),
        generics: Vec::new(),
    };
    let (database, opened) = crate::fontdb::database_from_native_catalog(catalog);
    assert!(database.is_empty());
    assert_eq!(opened, 0);
}

#[test]
fn named_generic_match_maps_an_independently_approved_physical_face() {
    let file = neomacs_test_fonts::dejavu_2_37()
        .mono_regular()
        .to_path_buf();
    let catalog = crate::fontdb::NativeFontCatalog {
        files: [(file.clone(), [0].into_iter().collect())]
            .into_iter()
            .collect(),
        generics: vec![(crate::fontdb::GenericFamily::Monospace, (file, 1 << 16))],
    };
    let (database, opened) = crate::fontdb::database_from_native_catalog(catalog);
    assert_eq!(opened, 1);
    assert_eq!(database.len(), 1);
    assert_eq!(
        database.family_name(&fontdb::Family::Monospace),
        "DejaVu Sans Mono"
    );
}

// Build a valid collection from pinned SFNTs. TTC table offsets are absolute
// from the collection start, unlike the source files' standalone offsets.
fn collection_bytes(fonts: &[Vec<u8>]) -> Vec<u8> {
    let mut result = b"ttcf\0\x01\0\0".to_vec();
    result.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    result.resize(12 + fonts.len() * 4, 0);
    for (index, font) in fonts.iter().enumerate() {
        while result.len() % 4 != 0 {
            result.push(0);
        }
        let start = result.len();
        result[12 + index * 4..16 + index * 4].copy_from_slice(&(start as u32).to_be_bytes());
        let mut relocated = font.clone();
        let count = u16::from_be_bytes(font[4..6].try_into().unwrap()) as usize;
        for table in 0..count {
            let record = 12 + table * 16;
            let offset = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap());
            relocated[record + 8..record + 12]
                .copy_from_slice(&(offset + start as u32).to_be_bytes());
            if &font[record..record + 4] == b"head" {
                // TTC files do not use the standalone font checksum adjustment.
                relocated[offset as usize + 8..offset as usize + 12].fill(0);
            }
        }
        result.extend_from_slice(&relocated);
    }
    result
}
