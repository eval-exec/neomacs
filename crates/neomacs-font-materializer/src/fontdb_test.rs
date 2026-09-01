use super::*;
use neomacs_display_protocol::font::FontMemoryAsset;

fn standalone_spleen_sfnt() -> Vec<u8> {
    let path = neomacs_test_fonts::spleen_2_2_0().woff();
    let bytes = std::fs::read(&path).expect("read downloaded WOFF fixture");
    FontFileCache::decode_web_font_to_sfnt(&path.to_string_lossy(), 0, &bytes)
        .expect("decode fixture as standalone SFNT")
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
