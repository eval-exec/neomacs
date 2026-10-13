use super::*;
use neomacs_display_protocol::font::FontMemoryAsset;

fn standalone_spleen_sfnt() -> Vec<u8> {
    let path = neomacs_test_fonts::spleen_2_2_0().woff();
    let bytes = std::fs::read(&path).expect("read downloaded WOFF fixture");
    FontFileCache::decode_web_font_to_sfnt(&path.to_string_lossy(), 0, &bytes)
        .expect("decode fixture as standalone SFNT")
}

use std::sync::atomic::{AtomicUsize, Ordering};

struct CountedFont {
    bytes: Vec<u8>,
    reads: Arc<AtomicUsize>,
}
impl AsRef<[u8]> for CountedFont {
    fn as_ref(&self) -> &[u8] {
        self.reads.fetch_add(1, Ordering::Relaxed);
        &self.bytes
    }
}

#[test]
fn exact_face_insertion_does_not_reread_existing_font_metadata() {
    let bytes = standalone_spleen_sfnt();
    let reads = Arc::new(AtomicUsize::new(0));
    let mut db = fontdb::Database::new();
    let original = db.load_font_source(fontdb::Source::Binary(Arc::new(CountedFont {
        bytes: bytes.clone(),
        reads: Arc::clone(&reads),
    })))[0];
    let mut system = FontSystem::new_with_locale_and_db("en-US".into(), db);
    let attrs = cosmic_text::Attrs::new();
    let before = system.get_font_matches(&attrs);
    let warmed_reads = reads.load(Ordering::Relaxed);
    assert!(warmed_reads > 0);

    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new("test:append-face", Arc::new(bytes), 0).unwrap(),
    );
    let pinned = FontFileCache::new()
        .pin_exact_asset(&mut system, &asset)
        .unwrap();
    let after = system.get_font_matches(&attrs);
    assert_eq!(
        reads.load(Ordering::Relaxed),
        warmed_reads,
        "adding a face must not reread an unchanged font's weight axes"
    );
    assert!(
        !Arc::ptr_eq(&before, &after),
        "font matching must see the new face"
    );
    assert_eq!(after.len(), before.len() + 1);
    assert!(system.db().face(original).is_some());
    assert_eq!(
        system.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name(pinned.family())],
            ..fontdb::Query::default()
        }),
        Some(pinned.fontdb_id())
    );

    // Arbitrary database mutation still invalidates existing metadata.
    system.db_mut().set_sans_serif_family(pinned.family());
    system.get_font_matches(&attrs);
    assert!(reads.load(Ordering::Relaxed) > warmed_reads);
}

#[test]
fn rejected_exact_face_preserves_existing_font_matches() {
    let mut db = fontdb::Database::new();
    db.load_font_source(fontdb::Source::Binary(Arc::new(standalone_spleen_sfnt())));
    let mut system = FontSystem::new_with_locale_and_db("en-US".into(), db);
    let attrs = cosmic_text::Attrs::new();
    let before = system.get_font_matches(&attrs);
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new("test:invalid-face", Arc::new(vec![0; 16]), 0).unwrap(),
    );
    assert!(
        FontFileCache::new()
            .pin_exact_asset(&mut system, &asset)
            .is_err()
    );
    let after = system.get_font_matches(&attrs);
    assert!(
        Arc::ptr_eq(&before, &after),
        "failed opening must not invalidate matching"
    );
}

#[test]
fn file_preloading_preserves_existing_font_metadata() {
    for resolve_family in [false, true] {
        let reads = Arc::new(AtomicUsize::new(0));
        let mut db = fontdb::Database::new();
        let original = db.load_font_source(fontdb::Source::Binary(Arc::new(CountedFont {
            bytes: standalone_spleen_sfnt(),
            reads: Arc::clone(&reads),
        })))[0];
        let mut system = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let attrs = cosmic_text::Attrs::new();
        let before = system.get_font_matches(&attrs);
        let warmed_reads = reads.load(Ordering::Relaxed);
        let path = neomacs_test_fonts::spleen_2_2_0().woff();
        let path = path.to_str().unwrap();
        let mut cache = FontFileCache::new();
        if resolve_family {
            assert!(cache.resolve_family(&mut system, path).is_some());
        } else {
            assert!(cache.prime_file(&mut system, path));
        }
        let after = system.get_font_matches(&attrs);
        assert_eq!(
            reads.load(Ordering::Relaxed),
            warmed_reads,
            "preloading must not reread unchanged font metadata"
        );
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(after.len(), before.len() + 1);
        assert!(system.db().face(original).is_some());
        assert!(cache.prime_file(&mut system, path));
        assert!(cache.resolve_family(&mut system, path).is_some());
        assert!(Arc::ptr_eq(&after, &system.get_font_matches(&attrs)));
    }
}

#[test]
fn rejected_file_preloading_preserves_existing_font_matches() {
    let mut db = fontdb::Database::new();
    db.load_font_source(fontdb::Source::Binary(Arc::new(standalone_spleen_sfnt())));
    let mut system = FontSystem::new_with_locale_and_db("en-US".into(), db);
    let attrs = cosmic_text::Attrs::new();
    let before = system.get_font_matches(&attrs);
    let mut cache = FontFileCache::new();
    assert!(!cache.prime_file(&mut system, ""));
    assert!(cache.resolve_family(&mut system, "").is_none());
    assert!(
        Arc::ptr_eq(&before, &system.get_font_matches(&attrs)),
        "failed preloading must not invalidate matching"
    );
}

#[test]
fn native_memory_asset_replays_in_independent_font_systems() {
    let sfnt = Arc::new(standalone_spleen_sfnt());
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new("coretext:test:Spleen", Arc::clone(&sfnt), 0)
            .expect("valid native-memory fixture"),
    );

    for _ in 0..2 {
        let mut font_system = FontSystem::new();
        let mut cache = FontFileCache::new();
        let pinned = cache
            .pin_exact_asset(&mut font_system, &asset)
            .expect("pin the exact memory asset");
        let face = font_system
            .db()
            .face(pinned.fontdb_id())
            .expect("pinned fontdb face");

        assert_eq!(face.index, 0);
        assert!(matches!(face.source, fontdb::Source::Binary(_)));
        assert!(
            face.families
                .iter()
                .any(|(family, _)| family == pinned.family())
        );
        assert_eq!(
            cache
                .pin_exact_asset(&mut font_system, &asset)
                .expect("reuse cached memory pin")
                .fontdb_id(),
            pinned.fontdb_id()
        );
    }
}

#[test]
fn native_table_serializer_builds_a_valid_checksummed_sfnt() {
    let source = standalone_spleen_sfnt();
    let provider = ReadScope::new(&source)
        .read::<FontData<'_>>()
        .expect("parse decoded fixture")
        .table_provider(0)
        .expect("fixture face");
    let tables = provider
        .table_tags()
        .expect("fixture table tags")
        .into_iter()
        .map(|tag| {
            let data = provider
                .table_data(tag)
                .expect("read fixture table")
                .expect("fixture table bytes")
                .into_owned();
            (tag, data)
        })
        .collect();

    let rebuilt =
        FontFileCache::standalone_sfnt_from_tables(tables).expect("serialize native table payload");

    ttf_parser::Face::parse(&rebuilt, 0).expect("rebuilt font is a standalone face");
    assert_eq!(FontFileCache::checksum(&rebuilt), 0xB1B0_AFBA);
}

#[test]
fn rebuilding_font_caches_reuses_process_lifetime_selectors() {
    let asset = FontOutlineAsset::Memory(
        FontMemoryAsset::new(
            "test:selector-lifetime",
            Arc::new(standalone_spleen_sfnt()),
            0,
        )
        .unwrap(),
    );
    let mut previous: Option<&'static str> = None;
    for _ in 0..3 {
        let mut system = FontSystem::new();
        let mut cache = FontFileCache::new();
        let family = cache.pin_exact_asset(&mut system, &asset).unwrap().family();
        if let Some(previous) = previous {
            assert!(
                std::ptr::eq(previous, family),
                "rebuilding a worker cache must not leak another identical selector"
            );
        }
        previous = Some(family);
    }
}

#[test]
fn collection_container_detection_reads_only_the_selected_directory() {
    struct Counted {
        data: std::io::Cursor<Vec<u8>>,
        read: usize,
    }
    impl Read for Counted {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let count = self.data.read(buf)?;
            self.read += count;
            Ok(count)
        }
    }
    impl Seek for Counted {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.data.seek(pos)
        }
    }
    for tags in [
        [*b"glyf", *b"head"],
        [*b"EBDT", *b"EBLC"],
        [*b"CBDT", *b"CBLC"],
    ] {
        let mut bytes = vec![0; 8 * 1024 * 1024];
        bytes[..4].copy_from_slice(b"ttcf");
        bytes[8..12].copy_from_slice(&2u32.to_be_bytes());
        let directory = 4 * 1024 * 1024;
        bytes[16..20].copy_from_slice(&(directory as u32).to_be_bytes());
        bytes[directory..directory + 4].copy_from_slice(&TRUE_TYPE_TAG.to_be_bytes());
        bytes[directory + 4..directory + 6].copy_from_slice(&2u16.to_be_bytes());
        for (i, tag) in tags.iter().enumerate() {
            let record = directory + 12 + i * 16;
            bytes[record..record + 4].copy_from_slice(tag);
            bytes[record + 12..record + 16].copy_from_slice(&100u32.to_be_bytes());
        }
        let path = Path::new("collection.ttc");
        let expected = FontContainer::detect(path, &bytes, 1);
        let mut source = Counted {
            data: std::io::Cursor::new(bytes),
            read: 0,
        };
        let (actual, _) = FontContainer::read_source(&mut source, path, 1).unwrap();
        assert_eq!(actual, expected);
        assert!(
            source.read <= 64,
            "classification read {} bytes",
            source.read
        );
    }
}

#[test]
fn streamed_container_detection_preserves_truncated_and_non_sfnt_results() {
    let mut sfnt = vec![0; 28];
    sfnt[..4].copy_from_slice(&TRUE_TYPE_TAG.to_be_bytes());
    sfnt[4..6].copy_from_slice(&1u16.to_be_bytes());
    sfnt[12..16].copy_from_slice(b"glyf");
    sfnt[24..28].copy_from_slice(&100u32.to_be_bytes());
    let mut collection = b"ttcf\0\x01\0\0\0\0\0\x01\0\0\0\x10".to_vec();
    collection.extend_from_slice(&sfnt);
    for bytes in [
        sfnt,
        collection,
        b"STARTFONT 2.1".to_vec(),
        b"wOFFpayload".to_vec(),
        vec![],
    ] {
        for len in 0..=bytes.len() {
            for index in [0, 1, u32::MAX] {
                let path = Path::new("source.font");
                let expected = FontContainer::detect(path, &bytes[..len], index);
                let (actual, _) = FontContainer::read_source(
                    &mut std::io::Cursor::new(&bytes[..len]),
                    path,
                    index,
                )
                .unwrap();
                assert_eq!(actual, expected, "length={len} index={index}");
            }
        }
    }
}

/// Font classification must not pay for a payload it does not need: the color
/// source layer asks about every face it is handed, and reading a whole font
/// to answer "no color tables" is most of a font cache's I/O.
#[test]
fn streamed_classification_reads_only_the_table_directory() {
    struct Counted {
        data: std::io::Cursor<Vec<u8>>,
        read: usize,
    }
    impl Read for Counted {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let count = self.data.read(buf)?;
            self.read += count;
            Ok(count)
        }
    }
    impl Seek for Counted {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.data.seek(pos)
        }
    }

    let mut bytes = vec![0u8; 8 * 1024 * 1024];
    bytes[..4].copy_from_slice(&TRUE_TYPE_TAG.to_be_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
    bytes[12..16].copy_from_slice(b"glyf");
    bytes[24..28].copy_from_slice(&100u32.to_be_bytes());

    let mut source = Counted {
        data: std::io::Cursor::new(bytes),
        read: 0,
    };
    let mut header = Vec::new();
    (&mut source).take(12).read_to_end(&mut header).unwrap();
    let classified = classify_sfnt_stream(&mut source, &header, 0).unwrap();
    let StreamedSfntSource::Sfnt(Some(sources)) = classified else {
        panic!("an SFNT face must classify as such");
    };
    assert!(sources.has_outline());
    assert!(
        sources.color_glyph_sources().next().is_none(),
        "an outline-only face carries no color source"
    );
    assert!(
        source.read <= 128,
        "classification read {} bytes of an 8 MiB font",
        source.read
    );
}

#[test]
fn streamed_classification_agrees_with_the_slice_classifier() {
    for path in [
        neomacs_test_fonts::noto_color_emoji_2_051(),
        neomacs_test_fonts::noto_color_emoji_colrv1(),
    ] {
        let bytes = std::fs::read(path).expect("read fixture");
        let slice = classify_sfnt_face(&bytes, 0).expect("fixture is an SFNT face");
        let mut source = std::io::Cursor::new(bytes);
        let mut header = Vec::new();
        (&mut source).take(12).read_to_end(&mut header).unwrap();
        let StreamedSfntSource::Sfnt(Some(streamed)) =
            classify_sfnt_stream(&mut source, &header, 0).unwrap()
        else {
            panic!("{} must classify as an SFNT face", path.display());
        };
        assert_eq!(
            streamed,
            slice,
            "streaming and slice classification disagree for {}",
            path.display()
        );
    }
}
