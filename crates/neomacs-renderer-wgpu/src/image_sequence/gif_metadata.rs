//! GIF extension blocks retained for GNU's `image-metadata` contract.
//!
//! GNU `src/image.c:10276-10314` exposes giflib's selected SavedImage
//! extension blocks, including their original sub-block boundaries. The
//! first sub-block has the extension's function label; subsequent sub-blocks
//! have function zero. Keep file order here; the Lisp adapter owns GNU's
//! reverse-order list construction.
//!
//! Record framing follows GIF89a sections 15-16 and 18-27:
//! <https://www.w3.org/Graphics/GIF/spec-gif89a.txt>.

/// Return the extension blocks preceding each image descriptor.
///
/// This only extracts framing and metadata; the pixel decoder remains
/// responsible for validating image dimensions, palettes, and LZW contents.
/// Extensions after the last image belong to the file rather than a frame,
/// so they are discarded when the trailer is reached.
pub(super) fn frame_extensions(data: &[u8]) -> Option<Vec<Vec<(u8, Vec<u8>)>>> {
    let mut remaining = data;
    match take(&mut remaining, 6)? {
        b"GIF87a" | b"GIF89a" => {}
        _ => return None,
    }
    let screen = take(&mut remaining, 7)?;
    take(&mut remaining, color_table_length(screen[4]))?;

    let mut frames = Vec::new();
    let mut extensions = Vec::new();
    loop {
        match byte(&mut remaining)? {
            0x21 => {
                let mut function = byte(&mut remaining)?;
                while let Some(block) = sub_block(&mut remaining)? {
                    extensions.push((function, block.to_vec()));
                    function = 0;
                }
            }
            0x2c => {
                let descriptor = take(&mut remaining, 9)?;
                take(&mut remaining, color_table_length(descriptor[8]))?;
                byte(&mut remaining)?; // LZW minimum code size.
                while sub_block(&mut remaining)?.is_some() {}
                frames.push(std::mem::take(&mut extensions));
            }
            0x3b => return Some(frames),
            _ => return None,
        }
    }
}

fn take<'a>(remaining: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
    let (head, tail) = remaining.split_at_checked(length)?;
    *remaining = tail;
    Some(head)
}

fn byte(remaining: &mut &[u8]) -> Option<u8> {
    take(remaining, 1)?.first().copied()
}

fn sub_block<'a>(remaining: &mut &'a [u8]) -> Option<Option<&'a [u8]>> {
    let length = usize::from(byte(remaining)?);
    if length == 0 {
        Some(None)
    } else {
        take(remaining, length).map(Some)
    }
}

fn color_table_length(packed: u8) -> usize {
    if packed & 0x80 == 0 {
        0
    } else {
        // The three-bit size field encodes 2..=256 RGB entries, at most
        // 768 bytes. The bounded shift and multiplication cannot overflow.
        3 * (1_usize << ((packed & 7) + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::frame_extensions;

    const HEADER: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff";
    const IMAGE: &[u8] = b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00";
    const GCE: &[u8] = b"\x21\xf9\x04\x01\x00\x00\x00\x00";

    fn gif(records: &[&[u8]]) -> Vec<u8> {
        let mut data = HEADER.to_vec();
        for record in records {
            data.extend_from_slice(record);
        }
        data.push(0x3b);
        data
    }

    #[test]
    fn single_gif_retains_graphics_control_extension_bytes() {
        assert_eq!(
            frame_extensions(&gif(&[GCE, IMAGE])),
            Some(vec![vec![(249, vec![1, 0, 0, 0])]])
        );
    }

    #[test]
    fn gif_without_extensions_has_an_empty_list_for_each_frame() {
        assert_eq!(
            frame_extensions(&gif(&[IMAGE, IMAGE])),
            Some(vec![vec![], vec![]])
        );
    }

    #[test]
    fn gif_extensions_belong_to_the_following_frame_only() {
        let second_gce = b"\x21\xf9\x04\x01\x07\x00\x00\x00";
        assert_eq!(
            frame_extensions(&gif(&[GCE, IMAGE, second_gce, IMAGE, IMAGE, GCE])),
            Some(vec![
                vec![(249, vec![1, 0, 0, 0])],
                vec![(249, vec![1, 7, 0, 0])],
                vec![],
            ])
        );
    }

    #[test]
    fn gif_comment_continuations_preserve_boundaries_and_file_order() {
        let comment = b"\x21\xfe\x02hi\x03bye\x00";
        assert_eq!(
            frame_extensions(&gif(&[comment, GCE, IMAGE])),
            Some(vec![vec![
                (254, b"hi".to_vec()),
                (0, b"bye".to_vec()),
                (249, vec![1, 0, 0, 0]),
            ]])
        );
    }

    #[test]
    fn gif_local_palette_and_compressed_sub_blocks_are_skipped() {
        let local_image = b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x80\x21\xf9\x2c\x3b\x00\x00\x02\x02\x21\xf9\x02\x2c\x3b\x00";
        assert_eq!(
            frame_extensions(&gif(&[GCE, local_image, IMAGE])),
            Some(vec![vec![(249, vec![1, 0, 0, 0])], vec![]])
        );
    }

    #[test]
    fn truncated_gif_records_never_return_partial_metadata() {
        let comment = b"\x21\xfe\x02hi\x03bye\x00";
        let data = gif(&[comment, GCE, IMAGE]);
        for end in 0..data.len() {
            assert!(frame_extensions(&data[..end]).is_none(), "prefix {end}");
        }
        assert!(frame_extensions(&data).is_some());
    }

    #[test]
    fn invalid_header_and_record_marker_are_rejected() {
        let mut data = gif(&[IMAGE]);
        data[0] = b'B';
        assert!(frame_extensions(&data).is_none());
        assert!(frame_extensions(&gif(&[b"\x22"])).is_none());
    }
}
