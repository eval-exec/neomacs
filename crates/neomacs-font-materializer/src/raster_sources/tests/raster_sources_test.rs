use super::*;

const TRUE_TYPE: u32 = 0x0001_0000;

/// Serialize a standalone face whose directory holds `tables` (tag, length).
///
/// Classification reads only the directory, so the table payloads are never
/// emitted; a nonzero length is what marks a table as present.
fn face_with_tables(tables: &[(&[u8; 4], u32)]) -> Vec<u8> {
    let mut bytes = vec![0u8; 12 + tables.len() * 16];
    bytes[..4].copy_from_slice(&TRUE_TYPE.to_be_bytes());
    bytes[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    for (index, (tag, length)) in tables.iter().enumerate() {
        let record = 12 + index * 16;
        bytes[record..record + 4].copy_from_slice(&tag[..]);
        bytes[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
    }
    bytes
}

#[test]
fn colr_with_cpal_is_a_layered_color_source() {
    let bytes = face_with_tables(&[(b"COLR", 14), (b"CPAL", 12), (b"glyf", 128)]);
    let sources = classify_sfnt_face(&bytes, 0).expect("standalone face");
    assert!(sources.has_color_outlines());
    assert!(sources.has_outline());
    assert!(sources.swash_owned());
    assert_eq!(
        sources.color_glyph_sources().collect::<Vec<_>>(),
        vec![ColorGlyphSource::Colr]
    );
}

#[test]
fn colr_without_cpal_is_not_a_color_source() {
    let bytes = face_with_tables(&[(b"COLR", 14), (b"glyf", 128)]);
    let sources = classify_sfnt_face(&bytes, 0).expect("standalone face");
    assert!(!sources.has_color_outlines());
    assert!(sources.has_outline());
    assert_eq!(sources.color_glyph_sources().count(), 0);
}

#[test]
fn empty_tables_do_not_supply_a_source() {
    let bytes = face_with_tables(&[
        (b"COLR", 0),
        (b"CPAL", 0),
        (b"glyf", 0),
        (b"CBDT", 0),
        (b"CBLC", 0),
        (b"SVG ", 0),
    ]);
    let sources = classify_sfnt_face(&bytes, 0).expect("standalone face");
    assert_eq!(sources, SfntFaceRasterSources::default());
    assert!(!sources.swash_owned());
    assert_eq!(sources.color_glyph_sources().count(), 0);
}

#[test]
fn color_bitmap_strikes_need_their_location_table() {
    let without_location = face_with_tables(&[(b"CBDT", 4096)]);
    let sources = classify_sfnt_face(&without_location, 0).expect("standalone face");
    assert!(!sources.color_bitmap_strikes());
    assert!(!sources.swash_owned());

    let with_location = face_with_tables(&[(b"CBDT", 4096), (b"CBLC", 32)]);
    let sources = classify_sfnt_face(&with_location, 0).expect("standalone face");
    assert!(sources.color_bitmap_strikes());
    assert!(sources.swash_owned());
    assert_eq!(
        sources.color_glyph_sources().collect::<Vec<_>>(),
        vec![ColorGlyphSource::EmbeddedColorBitmaps]
    );

    let sbix = face_with_tables(&[(b"sbix", 4096)]);
    let sources = classify_sfnt_face(&sbix, 0).expect("standalone face");
    assert!(sources.color_bitmap_strikes());
    assert!(sources.swash_owned());
}

#[test]
fn monochrome_strikes_stay_on_the_freetype_replay_path() {
    let bytes = face_with_tables(&[(b"EBDT", 4096), (b"EBLC", 32)]);
    let sources = classify_sfnt_face(&bytes, 0).expect("standalone face");
    assert!(sources.monochrome_bitmap_strikes());
    assert!(
        !sources.swash_owned(),
        "Swash cannot materialize fixed strikes"
    );
    assert_eq!(sources.color_glyph_sources().count(), 0);
}

#[test]
fn svg_documents_are_the_last_color_source() {
    let bytes = face_with_tables(&[(b"SVG ", 8192), (b"glyf", 128)]);
    let sources = classify_sfnt_face(&bytes, 0).expect("standalone face");
    assert!(sources.svg_documents());
    assert_eq!(
        sources.color_glyph_sources().collect::<Vec<_>>(),
        vec![ColorGlyphSource::SvgDocuments]
    );
}

#[test]
fn color_sources_are_reported_in_rasterization_preference_order() {
    let bytes = face_with_tables(&[
        (b"SVG ", 8192),
        (b"sbix", 4096),
        (b"COLR", 16),
        (b"CPAL", 12),
        (b"glyf", 128),
    ]);
    let sources = classify_sfnt_face(&bytes, 0).expect("standalone face");
    assert_eq!(
        sources.color_glyph_sources().collect::<Vec<_>>(),
        vec![
            ColorGlyphSource::Colr,
            ColorGlyphSource::EmbeddedColorBitmaps,
            ColorGlyphSource::SvgDocuments,
        ]
    );
}

#[test]
fn a_collection_classifies_each_face_from_its_own_directory() {
    let first = face_with_tables(&[(b"glyf", 128)]);
    let second = face_with_tables(&[(b"COLR", 16), (b"CPAL", 12), (b"glyf", 64)]);
    let header = 12 + 4 + 2 * 4;
    let mut collection = vec![0u8; header];
    collection[..4].copy_from_slice(b"ttcf");
    collection[8..12].copy_from_slice(&2u32.to_be_bytes());
    let first_offset = header;
    collection[12..16].copy_from_slice(&(first_offset as u32).to_be_bytes());
    let second_offset = first_offset + first.len();
    collection[16..20].copy_from_slice(&(second_offset as u32).to_be_bytes());
    collection.extend_from_slice(&first);
    collection.extend_from_slice(&second);

    let first_sources = classify_sfnt_face(&collection, 0).expect("first collection face");
    assert!(!first_sources.has_color_outlines());
    assert!(first_sources.has_outline());

    let second_sources = classify_sfnt_face(&collection, 1).expect("second collection face");
    assert!(second_sources.has_color_outlines());
    assert!(second_sources.has_outline());

    assert_eq!(classify_sfnt_face(&collection, 2), None);
    assert_eq!(classify_sfnt_face(b"not a font", 0), None);
}

#[test]
fn real_color_emoji_fixtures_classify_as_their_own_source() {
    let cbdt = std::fs::read(neomacs_test_fonts::noto_color_emoji_2_051()).expect("CBDT fixture");
    let sources = classify_sfnt_face(&cbdt, 0).expect("CBDT fixture is an SFNT face");
    assert!(sources.color_bitmap_strikes());
    assert!(!sources.has_color_outlines());

    let colrv1 =
        std::fs::read(neomacs_test_fonts::noto_color_emoji_colrv1()).expect("COLRv1 fixture");
    let sources = classify_sfnt_face(&colrv1, 0).expect("COLRv1 fixture is an SFNT face");
    assert!(sources.has_color_outlines());
    assert!(sources.svg_documents());
    assert!(sources.has_outline());
    assert_eq!(
        sources.color_glyph_sources().collect::<Vec<_>>(),
        vec![ColorGlyphSource::Colr, ColorGlyphSource::SvgDocuments,]
    );
}
