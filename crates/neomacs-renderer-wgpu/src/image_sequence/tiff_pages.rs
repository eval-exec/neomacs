//! Selected TIFF directories and GNU's count-only image metadata.
//!
//! GNU image.c:9527-9573 selects `:index` with TIFFSetDirectory, decodes only
//! that directory, and counts the main IFD chain for `image-metadata`.
//! Directory framing follows the classic/BigTIFF layouts documented by
//! <https://libtiff.gitlab.io/libtiff/specification/bigtiff.html>.

use super::ImageSequenceFrame;
use image::ImageDecoder;
use neomacs_display_protocol::{ImageEmbeddedMetadata, ImageFrameIndex, ImageNativeExtent};
use std::collections::HashSet;
use std::io::{self, BufReader, Cursor, Read, Seek, SeekFrom};

pub(super) fn decode(data: &[u8], frame: ImageFrameIndex) -> Option<ImageSequenceFrame> {
    let (mut decoder, count) = page_decoder(data, frame)?;
    decoder.set_limits(image::Limits::default()).ok()?;
    let pixels = image::DynamicImage::from_decoder(decoder)
        .ok()?
        .into_rgba8();
    Some(ImageSequenceFrame {
        width: pixels.width(),
        height: pixels.height(),
        rgba: pixels.into_raw(),
        embedded: ImageEmbeddedMetadata::pages(count),
    })
}

/// Inspect the selected directory before admitting its pixel allocation.
pub(super) fn probe(data: &[u8], frame: ImageFrameIndex) -> Option<ImageNativeExtent> {
    let (decoder, _) = page_decoder(data, frame)?;
    let (width, height) = decoder.dimensions();
    Some(ImageNativeExtent::new(width, height))
}

type PageDecoder<'a> = image::codecs::tiff::TiffDecoder<BufReader<HeaderOverlay<'a>>>;

fn page_decoder(data: &[u8], frame: ImageFrameIndex) -> Option<(PageDecoder<'_>, u32)> {
    let (header, directory, count) = selected_directory(data, frame.get())?;
    let reader = HeaderOverlay {
        data: Cursor::new(data),
        header: header.select(directory)?,
        header_len: header.layout.header_len(),
    };
    // The adapter changes only the first-IFD pointer. Strip/tile offsets and
    // every subsequent read retain their original absolute file positions.
    let decoder = image::codecs::tiff::TiffDecoder::new(BufReader::new(reader)).ok()?;
    Some((decoder, count))
}

#[derive(Clone, Copy)]
enum ByteOrder {
    Little,
    Big,
}

#[derive(Clone, Copy)]
enum Layout {
    Classic,
    Big,
}

impl Layout {
    const fn header_len(self) -> usize {
        match self {
            Self::Classic => 8,
            Self::Big => 16,
        }
    }

    const fn pointer_len(self) -> usize {
        match self {
            Self::Classic => 4,
            Self::Big => 8,
        }
    }

    const fn count_len(self) -> usize {
        match self {
            Self::Classic => 2,
            Self::Big => 8,
        }
    }

    const fn entry_len(self) -> usize {
        match self {
            Self::Classic => 12,
            Self::Big => 20,
        }
    }
}

struct Header {
    order: ByteOrder,
    layout: Layout,
    bytes: [u8; 16],
}

impl Header {
    fn parse(data: &[u8]) -> Option<Self> {
        let order = match data.get(..2)? {
            b"II" => ByteOrder::Little,
            b"MM" => ByteOrder::Big,
            _ => return None,
        };
        let layout = match integer(data, 2, 2, order)? {
            42 => Layout::Classic,
            43 if integer(data, 4, 2, order)? == 8 && integer(data, 6, 2, order)? == 0 => {
                Layout::Big
            }
            _ => return None,
        };
        let mut bytes = [0; 16];
        bytes[..layout.header_len()].copy_from_slice(data.get(..layout.header_len())?);
        Some(Self {
            order,
            layout,
            bytes,
        })
    }

    fn pointer_offset(&self) -> usize {
        self.layout.header_len() - self.layout.pointer_len()
    }

    fn select(&self, directory: u64) -> Option<[u8; 16]> {
        let mut bytes = self.bytes;
        let offset = self.pointer_offset();
        match self.layout {
            Layout::Classic => {
                let directory = u32::try_from(directory).ok()?;
                let pointer = match self.order {
                    ByteOrder::Little => directory.to_le_bytes(),
                    ByteOrder::Big => directory.to_be_bytes(),
                };
                bytes[offset..offset + 4].copy_from_slice(&pointer);
            }
            Layout::Big => {
                let pointer = match self.order {
                    ByteOrder::Little => directory.to_le_bytes(),
                    ByteOrder::Big => directory.to_be_bytes(),
                };
                bytes[offset..offset + 8].copy_from_slice(&pointer);
            }
        }
        Some(bytes)
    }
}

fn integer(data: &[u8], offset: usize, width: usize, order: ByteOrder) -> Option<u64> {
    let bytes = data.get(offset..offset.checked_add(width)?)?;
    Some(match (width, order) {
        (2, ByteOrder::Little) => u64::from(u16::from_le_bytes(bytes.try_into().ok()?)),
        (2, ByteOrder::Big) => u64::from(u16::from_be_bytes(bytes.try_into().ok()?)),
        (4, ByteOrder::Little) => u64::from(u32::from_le_bytes(bytes.try_into().ok()?)),
        (4, ByteOrder::Big) => u64::from(u32::from_be_bytes(bytes.try_into().ok()?)),
        (8, ByteOrder::Little) => u64::from_le_bytes(bytes.try_into().ok()?),
        (8, ByteOrder::Big) => u64::from_be_bytes(bytes.try_into().ok()?),
        _ => return None,
    })
}

fn selected_directory(data: &[u8], requested: u64) -> Option<(Header, u64, u32)> {
    let header = Header::parse(data)?;
    let mut directory = integer(
        data,
        header.pointer_offset(),
        header.layout.pointer_len(),
        header.order,
    )?;
    let mut seen = HashSet::new();
    let mut selected = None;
    let mut count = 0_u32;
    while directory != 0 {
        if !seen.insert(directory) {
            return None;
        }
        let offset = usize::try_from(directory).ok()?;
        if offset < header.layout.header_len() {
            return None;
        }
        let entries = usize::try_from(integer(
            data,
            offset,
            header.layout.count_len(),
            header.order,
        )?)
        .ok()?;
        let next = offset
            .checked_add(header.layout.count_len())?
            .checked_add(entries.checked_mul(header.layout.entry_len())?)?;
        if u64::from(count) == requested {
            selected = Some(directory);
        }
        count = count.checked_add(1)?;
        directory = integer(data, next, header.layout.pointer_len(), header.order)?;
    }
    Some((header, selected?, count))
}

struct HeaderOverlay<'a> {
    data: Cursor<&'a [u8]>,
    header: [u8; 16],
    header_len: usize,
}

impl Read for HeaderOverlay<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let start = self.data.position();
        let count = self.data.read(output)?;
        if start < self.header_len as u64 {
            let start = start as usize;
            let end = self.header_len.min(start + count);
            output[..end - start].copy_from_slice(&self.header[start..end]);
        }
        Ok(count)
    }
}

impl Seek for HeaderOverlay<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.data.seek(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(data: &mut Vec<u8>, value: u64, width: usize, order: ByteOrder) {
        let bytes = match order {
            ByteOrder::Little => value.to_le_bytes(),
            ByteOrder::Big => value.to_be_bytes(),
        };
        match order {
            ByteOrder::Little => data.extend_from_slice(&bytes[..width]),
            ByteOrder::Big => data.extend_from_slice(&bytes[8 - width..]),
        }
    }

    fn fixture(order: ByteOrder, layout: Layout) -> Vec<u8> {
        let mut data = match order {
            ByteOrder::Little => b"II".to_vec(),
            ByteOrder::Big => b"MM".to_vec(),
        };
        put(
            &mut data,
            match layout {
                Layout::Classic => 42,
                Layout::Big => 43,
            },
            2,
            order,
        );
        if matches!(layout, Layout::Big) {
            put(&mut data, 8, 2, order);
            put(&mut data, 0, 2, order);
        }
        put(
            &mut data,
            layout.header_len() as u64,
            layout.pointer_len(),
            order,
        );
        for (index, (width, height, color)) in [(2_u64, 1_u64, [255, 0, 0]), (3, 2, [0, 255, 0])]
            .into_iter()
            .enumerate()
        {
            let start = data.len();
            let pixels_offset =
                start + layout.count_len() + 10 * layout.entry_len() + layout.pointer_len();
            let end = pixels_offset + (width * height * 3) as usize;
            let tags = [
                (256, 4, width),
                (257, 4, height),
                // A single BitsPerSample value applies to every RGB component.
                (258, 3, 8),
                (259, 3, 1),
                (262, 3, 2),
                (273, 4, pixels_offset as u64),
                (277, 3, 3),
                (278, 4, height),
                (279, 4, width * height * 3),
                (284, 3, 1),
            ];
            put(&mut data, 10, layout.count_len(), order);
            for (tag, kind, value) in tags {
                put(&mut data, tag, 2, order);
                put(&mut data, kind, 2, order);
                put(&mut data, 1, layout.pointer_len(), order);
                let width = if kind == 3 { 2 } else { 4 };
                put(&mut data, value, width, order);
                data.resize(data.len() + layout.pointer_len() - width, 0);
            }
            put(
                &mut data,
                if index == 0 { end as u64 } else { 0 },
                layout.pointer_len(),
                order,
            );
            for _ in 0..width * height {
                data.extend_from_slice(&color);
            }
            assert_eq!(data.len(), end);
        }
        data
    }

    #[test]
    fn tiff_selected_page_decodes_its_pixels_and_count_for_each_layout() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for layout in [Layout::Classic, Layout::Big] {
                let data = fixture(order, layout);
                for (index, extent, color) in
                    [(0, (2, 1), [255, 0, 0, 255]), (1, (3, 2), [0, 255, 0, 255])]
                {
                    let frame = decode(&data, ImageFrameIndex::new(index)).expect("TIFF page");
                    assert_eq!(frame.dimensions(), extent);
                    assert_eq!(
                        probe(&data, ImageFrameIndex::new(index)),
                        Some(ImageNativeExtent::new(extent.0, extent.1))
                    );
                    let (pixels, metadata) = frame.into_parts();
                    assert!(pixels.chunks_exact(4).all(|pixel| pixel == color));
                    assert_eq!(metadata.frame_count(), Some(2));
                    assert_eq!(metadata.frame_delay(), None);
                }
                assert!(decode(&data, ImageFrameIndex::new(2)).is_none());
            }
        }
    }

    #[test]
    fn tiff_selected_page_drives_provisional_geometry_and_admission() {
        use crate::image_probe::{ImageProbeSource, admit_frame, probe_image_layout_for_frame};
        use neomacs_display_protocol::{
            ImageRealization, ImageRotation, ImageSizeLimit, ImageSizeSpec,
        };
        let data = fixture(ByteOrder::Little, Layout::Classic);
        let source = ImageProbeSource::Data(&data);
        let limit = ImageSizeLimit::from_axis_pixels(2);
        assert!(admit_frame(source, ImageFrameIndex::new(0), limit).is_ok());
        assert!(admit_frame(source, ImageFrameIndex::new(1), limit).is_err());
        let layout = probe_image_layout_for_frame(
            source,
            ImageFrameIndex::new(1),
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageRealization::default(),
        )
        .unwrap();
        assert_eq!(layout.dimensions(), (3, 2));
    }

    #[test]
    fn tiff_single_directory_has_no_animation_metadata() {
        let mut data = fixture(ByteOrder::Little, Layout::Classic);
        let next_pointer = 8 + 2 + 10 * 12;
        data[next_pointer..next_pointer + 4].fill(0);
        let frame = decode(&data, ImageFrameIndex::new(0)).expect("single TIFF page");
        let (_, metadata) = frame.into_parts();
        assert_eq!(metadata.frame_count(), None);
        assert_eq!(metadata.frame_delay(), None);
        assert!(decode(&data, ImageFrameIndex::new(1)).is_none());
    }

    #[test]
    fn tiff_directory_cycles_and_truncated_directory_tables_are_rejected() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for layout in [Layout::Classic, Layout::Big] {
                let mut data = fixture(order, layout);
                let end = layout.header_len() + layout.count_len() + 10 * layout.entry_len();
                assert!(selected_directory(&data[..end], 0).is_none());
                let mut pointer = Vec::new();
                put(
                    &mut pointer,
                    layout.header_len() as u64,
                    layout.pointer_len(),
                    order,
                );
                data[end..end + pointer.len()].copy_from_slice(&pointer);
                assert!(selected_directory(&data, 0).is_none());
            }
        }
    }

    #[test]
    fn tiff_header_overlay_preserves_absolute_seek_positions() {
        let data = fixture(ByteOrder::Little, Layout::Classic);
        let (header, directory, _) = selected_directory(&data, 1).unwrap();
        let mut reader = HeaderOverlay {
            data: Cursor::new(data.as_slice()),
            header: header.select(directory).unwrap(),
            header_len: header.layout.header_len(),
        };
        reader.seek(SeekFrom::Start(3)).unwrap();
        let mut bytes = [0; 8];
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes[1..5], &(directory as u32).to_le_bytes());
        assert_eq!(&bytes[5..], &data[8..11]);
        assert_eq!(reader.stream_position().unwrap(), 11);
    }
}
