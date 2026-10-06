use super::*;

#[test]
fn transpose_gap_positions_match_across_storage_backends() {
    for kind in implemented_text_backends() {
        for (initial, inserted, expected_gap) in [("ab", "中", 1), ("中", "ab", 4), ("b", "a", 3)]
        {
            let mut buf = buf_with_text_backend(initial, kind);
            buf.insert(inserted);
            let last = initial.chars().count() + inserted.chars().count();
            let transposition = buf.text_transposition_for_char_ranges(
                CharRange::from_usize(0, 1),
                CharRange::from_usize(last - 1, last),
            );
            buf.transpose_regions(
                transposition,
                crate::buffer::TranspositionAnchorPolicy::FollowText,
            );
            assert_eq!(
                buf.gap_position_lisp(),
                expected_gap,
                "{kind:?}: {initial:?} + {inserted:?}"
            );
        }
    }
}

#[test]
fn transpose_storage_moves_gap_outside_rearranged_character_boundaries() {
    for kind in implemented_text_backends() {
        let mut buf = buf_with_text_backend("b", kind);
        buf.insert("é");
        let transposition = buf.text_transposition_for_char_ranges(
            CharRange::from_usize(0, 1),
            CharRange::from_usize(1, 2),
        );
        buf.transpose_regions(
            transposition,
            crate::buffer::TranspositionAnchorPolicy::FollowText,
        );
        assert_eq!(
            buf.char_code_at_emacs_byte_pos(EmacsBytePos::new(1)),
            Some(233),
            "{kind:?}"
        );
        assert_eq!(byte_pos_for_char(&buf, 1), 1, "{kind:?}");
        assert_eq!(byte_pos_for_char(&buf, 2), 3, "{kind:?}");
    }
}

#[test]
fn transpose_preserved_marker_anchor_has_consistent_coordinates() {
    for kind in implemented_text_backends() {
        let mut buf = buf_with_text_backend("a中", kind);
        let _marker = register_marker_for_test(&mut buf, 701, 1, InsertionType::Before);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(1));
        let transposition = buf.text_transposition_for_char_ranges(
            CharRange::from_usize(0, 1),
            CharRange::from_usize(1, 2),
        );
        buf.transpose_regions(
            transposition,
            crate::buffer::TranspositionAnchorPolicy::PreserveCharacterPositions,
        );
        assert_eq!(
            marker_chain_anchor_for_test(&buf, 701),
            Some(TextPositionAnchor::from_usize(1, 3)),
            "{kind:?}"
        );
        assert_eq!(
            buf.point_anchor(),
            TextPositionAnchor::from_usize(1, 3),
            "{kind:?}"
        );
    }
}

/// GNU buffer.c:795-802 fetches state markers independently, even when
/// transpose_markers (editfns.c:4500-4523) puts point below saved BEGV.
#[test]
fn transpose_indirect_point_restores_full_text_anchor_outside_accessible_region() {
    use crate::buffer::TranspositionAnchorPolicy::{FollowText, PreserveCharacterPositions};
    for kind in implemented_text_backends() {
        for (text, preserved_point, moved_begv) in [
            (
                "abcdefgh",
                TextPositionAnchor::from_usize(5, 5),
                TextPositionAnchor::from_usize(4, 4),
            ),
            (
                "é中😀abcdx",
                TextPositionAnchor::from_usize(5, 9),
                TextPositionAnchor::from_usize(4, 7),
            ),
        ] {
            for policy in [FollowText, PreserveCharacterPositions] {
                let mut manager = manager_with_text_backend(kind);
                let base = manager.current_buffer_id().unwrap();
                manager.insert_into_buffer(base, text).unwrap();
                let base_point = manager
                    .get(base)
                    .unwrap()
                    .text
                    .char_pos_to_emacs_byte_pos(CharPos0::new(1));
                manager
                    .goto_buffer_emacs_byte_pos(base, base_point)
                    .unwrap();
                let sibling = manager
                    .create_indirect_buffer(base, "transpose-state-sibling", false)
                    .unwrap();
                assert!(manager.switch_current(sibling));
                let sibling_point = manager
                    .get(sibling)
                    .unwrap()
                    .text
                    .char_pos_to_emacs_byte_pos(CharPos0::new(5));
                manager
                    .goto_buffer_emacs_byte_pos(sibling, sibling_point)
                    .unwrap();
                let point_marker = manager
                    .get(sibling)
                    .unwrap()
                    .state_markers
                    .unwrap()
                    .pt_marker;
                assert!(manager.switch_current(base));
                let transposition = manager
                    .get(base)
                    .unwrap()
                    .text_transposition_for_char_ranges(
                        CharRange::from_usize(0, 2),
                        CharRange::from_usize(4, 6),
                    );
                manager
                    .transpose_buffer_regions(base, transposition, policy)
                    .unwrap();
                let (expected_point, expected_begv) = match policy {
                    FollowText => (TextPositionAnchor::from_usize(1, 1), moved_begv),
                    PreserveCharacterPositions => (preserved_point, TextPositionAnchor::ZERO),
                };
                assert_eq!(
                    manager.marker_anchor_position(sibling, point_marker),
                    Some(expected_point),
                    "{kind:?} {text:?} {policy:?}"
                );
                assert_eq!(
                    manager.get(sibling).unwrap().point_anchor(),
                    expected_point,
                    "{kind:?} {text:?} {policy:?}"
                );
                assert_eq!(
                    manager.get(sibling).unwrap().point_min_anchor(),
                    expected_begv,
                    "{kind:?} {text:?} {policy:?}"
                );
                // Switching must fetch the same full-text state without
                // clipping point to the independently saved narrowing.
                assert!(manager.switch_current(sibling));
                assert_eq!(
                    manager.get(sibling).unwrap().point_anchor(),
                    expected_point,
                    "{kind:?} {text:?} {policy:?}"
                );
            }
        }
    }
}
