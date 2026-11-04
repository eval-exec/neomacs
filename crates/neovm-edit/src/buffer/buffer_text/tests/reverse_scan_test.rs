use super::super::{BACKWARD_SCAN_BYTE_VISITS, TextPositionAnchor, scan_backward};
use super::*;

#[test]
fn backward_gap_scan_reads_only_the_bytes_between_positions() {
    let source = "ordinary café 好🙂 text\n".repeat(200);
    let positions: Vec<_> = source
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([source.len()])
        .collect();
    for gap_char in [0, 3, positions.len() / 2, positions.len() - 1] {
        let mut text = BufferText::from_str(&source);
        let gap_byte = positions[gap_char];
        // Place the storage gap without changing the logical text.
        insert_storage_string(&mut text, EmacsBytePos::new(gap_byte), " ");
        delete_emacs_byte_range(&mut text, emacs_byte_range(gap_byte, gap_byte + 1));
        let storage = text.storage.borrow();
        for anchor_char in [1, positions.len() / 2, positions.len() - 1] {
            for distance in [0, 1, 7, 32, 1000] {
                let target = anchor_char.saturating_sub(distance);
                BACKWARD_SCAN_BYTE_VISITS.with(|visits| visits.set(0));
                let actual = scan_backward(
                    &storage.backend,
                    TextPositionAnchor::new(
                        CharPos0::new(anchor_char),
                        EmacsBytePos::new(positions[anchor_char]),
                    ),
                    CharPos0::new(target),
                );
                assert_eq!(actual.get(), positions[target]);
                assert_eq!(
                    BACKWARD_SCAN_BYTE_VISITS.with(|visits| visits.get()),
                    positions[anchor_char] - positions[target],
                    "gap={gap_char}, anchor={anchor_char}, target={target}"
                );
            }
        }
    }
}
