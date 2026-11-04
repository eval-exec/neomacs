use super::*;

#[test]
fn casify_expansion_constructor_excludes_empty_and_single_char_outputs() {
    assert!(CasifyExpansion::new(CharPos0::ZERO, CharLen::ZERO).is_none());
    assert!(CasifyExpansion::new(CharPos0::ZERO, CharLen::new(1)).is_none());
    let expansion = CasifyExpansion::new(CharPos0::new(3), CharLen::new(2)).unwrap();
    assert_eq!(expansion.source, CharPos0::new(3));
    assert_eq!(expansion.replacement_len, CharLen::new(2));
}

#[test]
fn casify_same_byte_extent_can_grow_character_extent() {
    let mut buffers = BufferManager::new();
    let id = buffers.current_buffer_id().unwrap();
    buffers.insert_into_buffer(id, "ßx").unwrap();
    buffers.get(id).unwrap().text.reset_unchanged_region();
    let expansion = CasifyExpansion::new(CharPos0::ZERO, CharLen::new(2)).unwrap();
    buffers
        .casify_replace_buffer_region_with_expansions(
            id,
            EmacsByteRange::from_usize(0, 3),
            &LispString::from_utf8("SSX"),
            &[expansion],
            CasifyStorageShape::Changed,
            &crate::buffer::text_props::CasingPropertyMode::Unneeded,
        )
        .unwrap();
    let buffer = buffers.get(id).unwrap();
    assert_eq!(buffer.text.char_count(), CharLen::new(3));
    assert_eq!(buffer.point_max_anchor().char_pos(), CharPos0::new(3));
    assert_eq!(buffer.point_anchor().char_pos(), CharPos0::new(3));
    assert_eq!(buffer.text.changed_char_range(3), Some((0, 3)));
    buffers.insert_into_buffer(id, "!").unwrap();
    assert_eq!(buffers.get(id).unwrap().buffer_string(), "SSX!");
}

#[test]
fn casify_point_uses_expansion_site_instead_of_region_end() {
    let mut buffers = BufferManager::new();
    let id = buffers.current_buffer_id().unwrap();
    buffers.insert_into_buffer(id, "aßbßc").unwrap();
    buffers
        .goto_buffer_emacs_byte_pos(id, EmacsBytePos::new(3))
        .unwrap();
    let expansions = [
        CasifyExpansion::new(CharPos0::new(1), CharLen::new(2)).unwrap(),
        CasifyExpansion::new(CharPos0::new(3), CharLen::new(2)).unwrap(),
    ];
    buffers
        .casify_replace_buffer_region_with_expansions(
            id,
            EmacsByteRange::from_usize(0, 7),
            &LispString::from_utf8("ASSBSSC"),
            &expansions,
            CasifyStorageShape::Changed,
            &crate::buffer::text_props::CasingPropertyMode::Unneeded,
        )
        .unwrap();
    assert_eq!(
        buffers.get(id).unwrap().point_anchor().char_pos(),
        CharPos0::new(3)
    );
}

#[test]
fn casify_growth_beyond_narrowed_end_keeps_coordinate_pairs_coherent() {
    let mut buffers = BufferManager::new();
    let id = buffers.current_buffer_id().unwrap();
    buffers.insert_into_buffer(id, "aßz").unwrap();
    buffers
        .get_mut(id)
        .unwrap()
        .narrow_to_emacs_byte_range(EmacsByteRange::from_usize(0, 1));
    let expansion = CasifyExpansion::new(CharPos0::new(1), CharLen::new(2)).unwrap();
    buffers
        .casify_replace_buffer_region_with_expansions(
            id,
            EmacsByteRange::from_usize(0, 4),
            &LispString::from_utf8("ASSZ"),
            &[expansion],
            CasifyStorageShape::Changed,
            &crate::buffer::text_props::CasingPropertyMode::Unneeded,
        )
        .unwrap();
    let buffer = buffers.get(id).unwrap();
    assert_eq!(buffer.point_min_anchor().char_pos(), CharPos0::ZERO);
    assert_eq!(buffer.point_max_anchor().char_pos(), CharPos0::new(2));
    assert_eq!(
        buffer.point_max_anchor().emacs_byte_pos(),
        EmacsBytePos::new(2)
    );
    assert_eq!(buffer.buffer_string(), "AS");
}
