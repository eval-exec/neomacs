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

/// Structural framing extracted from a GIF's header and image descriptors.
pub(super) struct GifStructure {
    pub(super) extensions: Vec<Vec<(u8, Vec<u8>)>>,
    /// True iff every image descriptor fits within the logical screen
    /// (`left + width <= screen_width && top + height <= screen_height`),
    /// matching GNU `gif_load`'s up-front check (`src/image.c:10018-10035`).
    pub(super) all_subimages_fit_screen: bool,
    /// True iff every image's LZW minimum code size byte is `<= 8`.
    /// GNU `gif_load` runs `DGifSlurp` up front (`src/image.c:9992`), and
    /// giflib 6.1.3 `DGifSetupDecompress` (`dgif_lib.c:827`) rejects only
    /// `BitsPerPixel > 8` with `D_GIF_ERR_READ_FAILED`. Code size `0` is
    /// accepted up front to match giflib 6.1.3 so earlier valid frames remain
    /// decodable; `gif` 0.14.2 rejects code size `0` per-frame as a known
    /// decoder limitation.
    pub(super) all_code_sizes_valid: bool,
}

/// Parse the GIF record structure, returning each frame's preceding extension
/// blocks and up-front structural validity flags matching GNU `gif_load`.
pub(super) fn parse_structure(data: &[u8]) -> Option<GifStructure> {
    let mut remaining = data;
    match take(&mut remaining, 6)? {
        b"GIF87a" | b"GIF89a" => {}
        _ => return None,
    }
    let screen = take(&mut remaining, 7)?;
    let screen_width = u32::from(u16::from_le_bytes([screen[0], screen[1]]));
    let screen_height = u32::from(u16::from_le_bytes([screen[2], screen[3]]));
    take(&mut remaining, color_table_length(screen[4]))?;

    let mut frames = Vec::new();
    let mut extensions = Vec::new();
    let mut all_subimages_fit_screen = true;
    let mut all_code_sizes_valid = true;
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
                let left = u32::from(u16::from_le_bytes([descriptor[0], descriptor[1]]));
                let top = u32::from(u16::from_le_bytes([descriptor[2], descriptor[3]]));
                let width = u32::from(u16::from_le_bytes([descriptor[4], descriptor[5]]));
                let height = u32::from(u16::from_le_bytes([descriptor[6], descriptor[7]]));
                if left.saturating_add(width) > screen_width
                    || top.saturating_add(height) > screen_height
                {
                    all_subimages_fit_screen = false;
                }
                take(&mut remaining, color_table_length(descriptor[8]))?;
                let min_code_size = byte(&mut remaining)?; // LZW minimum code size.
                if min_code_size > 8 {
                    all_code_sizes_valid = false;
                }
                while sub_block(&mut remaining)?.is_some() {}
                frames.push(std::mem::take(&mut extensions));
            }
            0x3b => {
                return Some(GifStructure {
                    extensions: frames,
                    all_subimages_fit_screen,
                    all_code_sizes_valid,
                });
            }
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
    use super::parse_structure;

    fn frame_extensions(data: &[u8]) -> Option<Vec<Vec<(u8, Vec<u8>)>>> {
        parse_structure(data).map(|structure| structure.extensions)
    }

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
    fn parse_structure_flags_out_of_screen_subimages_and_invalid_lzw_code_sizes() {
        let valid = parse_structure(&gif(&[GCE, IMAGE])).expect("valid 1x1 GIF");
        assert!(valid.all_subimages_fit_screen);
        assert!(valid.all_code_sizes_valid);

        // Second frame extends past 1x1 screen (left=1, width=1 -> right=2 > 1).
        let oob_image = b"\x2c\x01\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00";
        let oob = parse_structure(&gif(&[IMAGE, oob_image])).expect("parses structure");
        assert!(!oob.all_subimages_fit_screen);
        assert!(oob.all_code_sizes_valid);

        // Second frame has LZW min_code_size > 8 (9, 42), matching giflib 6.1.3
        // `DGifSetupDecompress` (`dgif_lib.c:827`, `D_GIF_ERR_READ_FAILED`).
        for bad_cs in [9u8, 42] {
            let mut bad_image = IMAGE.to_vec();
            bad_image[10] = bad_cs;
            let parsed = parse_structure(&gif(&[IMAGE, &bad_image])).expect("parses structure");
            assert!(parsed.all_subimages_fit_screen);
            assert!(
                !parsed.all_code_sizes_valid,
                "min_code_size={bad_cs} must fail up-front validation"
            );
        }
        // Code sizes 0..=8 (including 0, accepted by giflib 6.1.3) pass up-front
        // validation.
        for ok_cs in [0u8, 1, 8] {
            let mut ok_image = IMAGE.to_vec();
            ok_image[10] = ok_cs;
            let parsed = parse_structure(&gif(&[IMAGE, &ok_image])).expect("parses structure");
            assert!(
                parsed.all_code_sizes_valid,
                "min_code_size={ok_cs} must pass up-front validation like giflib 6.1.3"
            );
        }
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
