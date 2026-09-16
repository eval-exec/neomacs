//! CPU-only decoding for hosts whose evaluator and GPU live in separate workers.

use crate::ImageSequenceCache;
use crate::decoder::{DecodeRequest, ImageDecoder, ImageSource, WorkerDecodeOutcome};
use neomacs_display_protocol::{
    DecodedImage, ImageColorContext, ImageFrameIndex, ImageLoadToken, ImageMaskPolicy,
    ImageRealization, ImageRotation, ImageSequenceId, ImageSizeSpec,
};

/// One encoded image realization; matches the native decoder's inputs.
pub struct EncodedImage {
    pub load: ImageLoadToken,
    pub bytes: Vec<u8>,
    pub size: ImageSizeSpec,
    pub rotation: ImageRotation,
    pub realization: ImageRealization,
    pub colors: ImageColorContext,
    pub mask: ImageMaskPolicy,
    pub frame: ImageFrameIndex,
    pub sequence: ImageSequenceId,
}

/// Reuses native decoding, sizing, mask semantics, and animation composition.
/// Call outside redisplay; this interface intentionally has no GPU dependency.
pub struct PortableImageDecoder {
    sequences: ImageSequenceCache,
}

impl Default for PortableImageDecoder {
    fn default() -> Self {
        Self {
            sequences: ImageSequenceCache::new(),
        }
    }
}

impl PortableImageDecoder {
    pub fn decode(&self, image: EncodedImage) -> Result<DecodedImage, &'static str> {
        let request = DecodeRequest {
            load: image.load,
            source: ImageSource::Data {
                data: image.bytes,
                resources: crate::SvgResourceContext::Isolated,
                sequence: image.sequence,
            },
            size: image.size,
            rotation: image.rotation,
            realization: image.realization,
            colors: image.colors,
            mask: image.mask,
            frame: image.frame,
        };
        match ImageDecoder::decode_request(request, &self.sequences) {
            WorkerDecodeOutcome::Ready(image) => Ok(image),
            WorkerDecodeOutcome::Failed(_) => Err("image decode failed"),
        }
    }
}
