//! Renderer-advertised DMA-BUF import formats, independent of native APIs.

use std::sync::Arc;

/// DRM fourcc and modifier are one inseparable buffer-layout identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DmaBufFormat {
    pub fourcc: u32,
    pub modifier: u64,
}

/// Immutable capabilities of the receiving device, not the exporting device.
/// An empty set is the safe default for an unknown or non-DMA-BUF renderer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmaBufImportFormats(Arc<[DmaBufFormat]>);

impl DmaBufImportFormats {
    pub fn new(formats: impl IntoIterator<Item = DmaBufFormat>) -> Self {
        let mut formats: Vec<_> = formats.into_iter().collect();
        formats.sort_unstable();
        formats.dedup();
        Self(formats.into())
    }

    pub fn contains(&self, format: DmaBufFormat) -> bool {
        self.0.binary_search(&format).is_ok()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_support_requires_the_exact_format_and_modifier() {
        let linear = DmaBufFormat {
            fourcc: 0x34325258,
            modifier: 0,
        };
        let tiled = DmaBufFormat {
            modifier: 0x0200000028a6bf04,
            ..linear
        };
        let formats = DmaBufImportFormats::new([linear, linear]);
        assert_eq!(formats, DmaBufImportFormats::new([linear]));
        assert!(formats.contains(linear));
        assert!(!formats.contains(tiled));
        assert!(!formats.contains(DmaBufFormat {
            fourcc: 0x34325241,
            ..linear
        }));
        assert!(!DmaBufImportFormats::default().contains(linear));
    }
}
