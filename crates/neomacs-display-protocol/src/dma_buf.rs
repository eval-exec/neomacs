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
#[path = "dma_buf/tests/dma_buf_test.rs"]
mod tests;
