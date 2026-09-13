use std::collections::HashMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::time::Instant;

use neomacs_display_protocol::types::VideoId;

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;

/// Typed replacement for the legacy `-1`/`0`/positive loop count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    Off,
    Infinite,
    Count(NonZeroU32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VideoModelError {
    #[error("loop count must be -1, 0, or a positive integer")]
    InvalidLoopCount,
    #[error("playback rate must be finite and greater than zero")]
    InvalidPlaybackRate,
}

/// Finite, positive playback-rate multiplier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackRate(f64);

impl PlaybackRate {
    pub const NORMAL: Self = Self(1.0);

    pub fn new(rate: f64) -> Result<Self, VideoModelError> {
        if rate.is_finite() && rate > 0.0 {
            Ok(Self(rate))
        } else {
            Err(VideoModelError::InvalidPlaybackRate)
        }
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl LoopMode {
    pub fn from_legacy(count: i32) -> Result<Self, VideoModelError> {
        match count {
            -1 => Ok(Self::Infinite),
            0 => Ok(Self::Off),
            count if count > 0 => Ok(Self::Count(
                NonZeroU32::new(count as u32).expect("positive i32 is non-zero as u32"),
            )),
            _ => Err(VideoModelError::InvalidLoopCount),
        }
    }

    /// Consume permission for one replay after end-of-stream. `Count(n)` is
    /// the number of additional plays, matching the legacy Lisp surface.
    pub fn consume_replay(&mut self) -> bool {
        match *self {
            Self::Off => false,
            Self::Infinite => true,
            Self::Count(count) if count.get() == 1 => {
                *self = Self::Off;
                true
            }
            Self::Count(count) => {
                *self = Self::Count(
                    NonZeroU32::new(count.get() - 1)
                        .expect("count greater than one remains non-zero"),
                );
                true
            }
        }
    }
}

/// Source accepted by the native playback adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoSource {
    File(PathBuf),
    Uri(String),
}

/// Initial state requested when opening a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialPlayback {
    Playing,
    Paused,
}

/// Immutable parameters required to allocate one stable video session.
///
/// The host supplies the [`VideoId`]; native decoder incarnations use a
/// separate identity internal to `neomacs-video` so device recovery cannot
/// accidentally make stale native events address the replacement session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoOpenRequest {
    pub source: VideoSource,
    pub initial_playback: InitialPlayback,
    pub loop_mode: LoopMode,
}

/// Media timeline position in nanoseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MediaTime(u64);

impl MediaTime {
    pub const ZERO: Self = Self(0);

    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    pub const fn saturating_add(self, duration: Self) -> Self {
        Self(self.0.saturating_add(duration.0))
    }
}

/// Decoder timing attached to one output frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTiming {
    pub pts: MediaTime,
    pub duration: MediaTime,
    /// Timeline identity. Seeking, stopping, or looping advances this epoch so
    /// a delayed pre-discontinuity surface cannot enter the new timeline.
    pub epoch: PlaybackEpoch,
}

/// Monotonic identity for one continuous segment of a playback timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlaybackEpoch(NonZeroU64);

impl PlaybackEpoch {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    /// Reconstruct an epoch received across a native backend boundary.
    pub const fn from_raw(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Return the stable non-zero representation used by native backends.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub fn next(self) -> Self {
        Self(
            NonZeroU64::new(
                self.0
                    .get()
                    .checked_add(1)
                    .expect("video playback epoch exhausted"),
            )
            .expect("incrementing a non-zero playback epoch stays non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelAspectRatio {
    pub numerator: NonZeroU32,
    pub denominator: NonZeroU32,
}

impl PixelAspectRatio {
    pub const SQUARE: Self = Self {
        numerator: NonZeroU32::MIN,
        denominator: NonZeroU32::MIN,
    };

    pub fn from_display_geometry(
        visible_width: u32,
        visible_height: u32,
        display_width: u32,
        display_height: u32,
    ) -> Self {
        let numerator = u64::from(display_width).saturating_mul(u64::from(visible_height));
        let denominator = u64::from(display_height).saturating_mul(u64::from(visible_width));
        let divisor = greatest_common_divisor(numerator, denominator);
        let numerator = numerator / divisor;
        let denominator = denominator / divisor;
        match (u32::try_from(numerator), u32::try_from(denominator)) {
            (Ok(numerator), Ok(denominator)) => Self {
                numerator: NonZeroU32::new(numerator).unwrap_or(NonZeroU32::MIN),
                denominator: NonZeroU32::new(denominator).unwrap_or(NonZeroU32::MIN),
            },
            _ => Self::SQUARE,
        }
    }
}

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    if left == 0 || right == 0 {
        return 1;
    }
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoRotation {
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoGeometry {
    pub coded_width: u32,
    pub coded_height: u32,
    pub visible_rect: PixelRect,
    pub display_width: u32,
    pub display_height: u32,
    pub pixel_aspect_ratio: PixelAspectRatio,
    pub rotation: VideoRotation,
}

impl VideoGeometry {
    pub const fn packed(width: u32, height: u32) -> Self {
        Self {
            coded_width: width,
            coded_height: height,
            visible_rect: PixelRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            display_width: width,
            display_height: height,
            pixel_aspect_ratio: PixelAspectRatio::SQUARE,
            rotation: VideoRotation::None,
        }
    }

    /// Resolve coded-pixel crop and orientation into the one transform the
    /// renderer is allowed to use. Pixel aspect ratio affects the published
    /// display dimensions; it does not change which coded texels are sampled.
    pub fn sampling_transform(self) -> VideoSamplingTransform {
        let coded_width = self.coded_width.max(1) as f32;
        let coded_height = self.coded_height.max(1) as f32;
        let left = self.visible_rect.x as f32 / coded_width;
        let top = self.visible_rect.y as f32 / coded_height;
        let right = self
            .visible_rect
            .x
            .saturating_add(self.visible_rect.width)
            .min(self.coded_width) as f32
            / coded_width;
        let bottom = self
            .visible_rect
            .y
            .saturating_add(self.visible_rect.height)
            .min(self.coded_height) as f32
            / coded_height;
        let unrotated = [[left, top], [right, top], [right, bottom], [left, bottom]];
        let corners = match self.rotation {
            VideoRotation::None => unrotated,
            // Destination corners sample the corresponding source corners
            // after applying the declared clockwise presentation rotation.
            VideoRotation::Clockwise90 => [unrotated[3], unrotated[0], unrotated[1], unrotated[2]],
            VideoRotation::Clockwise180 => [unrotated[2], unrotated[3], unrotated[0], unrotated[1]],
            VideoRotation::Clockwise270 => [unrotated[1], unrotated[2], unrotated[3], unrotated[0]],
        };
        VideoSamplingTransform { corners }
    }

    pub fn with_visible_rect_and_display_size(
        coded_width: u32,
        coded_height: u32,
        visible_rect: PixelRect,
        display_width: u32,
        display_height: u32,
        rotation: VideoRotation,
    ) -> Self {
        let display_width = display_width.max(1);
        let display_height = display_height.max(1);
        // Display dimensions are presentation-oriented. Pixel aspect ratio,
        // however, describes the unrotated coded-pixel grid.
        let (unrotated_display_width, unrotated_display_height) = match rotation {
            VideoRotation::None | VideoRotation::Clockwise180 => (display_width, display_height),
            VideoRotation::Clockwise90 | VideoRotation::Clockwise270 => {
                (display_height, display_width)
            }
        };
        Self {
            coded_width,
            coded_height,
            visible_rect,
            display_width,
            display_height,
            pixel_aspect_ratio: PixelAspectRatio::from_display_geometry(
                visible_rect.width,
                visible_rect.height,
                unrotated_display_width,
                unrotated_display_height,
            ),
            rotation,
        }
    }

    pub fn with_pixel_aspect_ratio(
        coded_width: u32,
        coded_height: u32,
        visible_rect: PixelRect,
        pixel_aspect_ratio: PixelAspectRatio,
        rotation: VideoRotation,
    ) -> Self {
        let unrotated_width = u64::from(visible_rect.width)
            .saturating_mul(u64::from(pixel_aspect_ratio.numerator.get()))
            .saturating_add(u64::from(pixel_aspect_ratio.denominator.get()) / 2)
            / u64::from(pixel_aspect_ratio.denominator.get());
        let unrotated_width = u32::try_from(unrotated_width).unwrap_or(u32::MAX).max(1);
        let unrotated_height = visible_rect.height.max(1);
        let (display_width, display_height) = match rotation {
            VideoRotation::None | VideoRotation::Clockwise180 => {
                (unrotated_width, unrotated_height)
            }
            VideoRotation::Clockwise90 | VideoRotation::Clockwise270 => {
                (unrotated_height, unrotated_width)
            }
        };
        Self {
            coded_width,
            coded_height,
            visible_rect,
            display_width,
            display_height,
            pixel_aspect_ratio,
            rotation,
        }
    }
}

/// Affine mapping from normalized destination coordinates to the visible,
/// presentation-oriented region of a decoded surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSamplingTransform {
    corners: [[f32; 2]; 4],
}

impl VideoSamplingTransform {
    pub const fn coordinates(self) -> VideoTextureCoordinates {
        VideoTextureCoordinates(self.corners)
    }

    /// Compose compositor clipping in destination space with native crop and
    /// rotation. Keeping this operation here prevents render paths from each
    /// inventing their own UV convention.
    pub fn coordinates_for_destination_rect(
        self,
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
    ) -> VideoTextureCoordinates {
        VideoTextureCoordinates([
            self.map_destination(left, top),
            self.map_destination(right, top),
            self.map_destination(right, bottom),
            self.map_destination(left, bottom),
        ])
    }

    fn map_destination(self, x: f32, y: f32) -> [f32; 2] {
        let top = lerp_coordinate(self.corners[0], self.corners[1], x);
        let bottom = lerp_coordinate(self.corners[3], self.corners[2], x);
        lerp_coordinate(top, bottom, y)
    }
}

/// Texture coordinates in destination corner order: top-left, top-right,
/// bottom-right, bottom-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoTextureCoordinates([[f32; 2]; 4]);

impl VideoTextureCoordinates {
    pub const fn corners(self) -> [[f32; 2]; 4] {
        self.0
    }

    pub const fn triangle_list(self) -> [[f32; 2]; 6] {
        [
            self.0[0], self.0[1], self.0[2], self.0[0], self.0[2], self.0[3],
        ]
    }
}

fn lerp_coordinate(from: [f32; 2], to: [f32; 2], amount: f32) -> [f32; 2] {
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
    ]
}

/// Byte layout of a single-plane RGB video surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackedVideoFormat {
    Bgra8,
    Rgba8,
}

/// Byte layout of a two-plane 4:2:0 YUV video surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BiPlanarVideoFormat {
    /// 8-bit luma followed by interleaved 8-bit Cb/Cr.
    Nv12,
    /// 10-bit luma and Cb/Cr stored in the most significant bits of 16-bit words.
    P010,
}

/// GPU-visible format of one decoded plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoPlaneFormat {
    Bgra8UnormSrgb,
    Rgba8UnormSrgb,
    R8Unorm,
    Rg8Unorm,
    R16Unorm,
    Rg16Unorm,
}

/// Decoder output format, including the number and meaning of its planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoFrameFormat {
    Packed(PackedVideoFormat),
    BiPlanar420(BiPlanarVideoFormat),
}

/// Renderer pipeline required to sample a decoded frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoSampleKind {
    Packed,
    BiPlanar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VideoFrameLayoutError {
    #[error("video surface coded dimensions must be non-zero")]
    ZeroCodedDimensions,
    #[error("4:2:0 video surface dimensions must be even, got {width}x{height}")]
    OddSubsampledDimensions { width: u32, height: u32 },
    #[error("video surface allocation size overflow")]
    AllocationOverflow,
}

const BGRA8_PLANES: &[VideoPlaneFormat] = &[VideoPlaneFormat::Bgra8UnormSrgb];
const RGBA8_PLANES: &[VideoPlaneFormat] = &[VideoPlaneFormat::Rgba8UnormSrgb];
const NV12_PLANES: &[VideoPlaneFormat] = &[VideoPlaneFormat::R8Unorm, VideoPlaneFormat::Rg8Unorm];
const P010_PLANES: &[VideoPlaneFormat] = &[VideoPlaneFormat::R16Unorm, VideoPlaneFormat::Rg16Unorm];

impl VideoFrameFormat {
    pub const fn sample_kind(self) -> VideoSampleKind {
        match self {
            Self::Packed(_) => VideoSampleKind::Packed,
            Self::BiPlanar420(_) => VideoSampleKind::BiPlanar,
        }
    }

    pub const fn plane_formats(self) -> &'static [VideoPlaneFormat] {
        match self {
            Self::Packed(PackedVideoFormat::Bgra8) => BGRA8_PLANES,
            Self::Packed(PackedVideoFormat::Rgba8) => RGBA8_PLANES,
            Self::BiPlanar420(BiPlanarVideoFormat::Nv12) => NV12_PLANES,
            Self::BiPlanar420(BiPlanarVideoFormat::P010) => P010_PLANES,
        }
    }

    pub fn allocation_bytes(self, geometry: VideoGeometry) -> Result<usize, VideoFrameLayoutError> {
        if geometry.coded_width == 0 || geometry.coded_height == 0 {
            return Err(VideoFrameLayoutError::ZeroCodedDimensions);
        }
        let pixels = usize::try_from(geometry.coded_width)
            .ok()
            .and_then(|width| {
                usize::try_from(geometry.coded_height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(VideoFrameLayoutError::AllocationOverflow)?;
        match self {
            Self::Packed(_) => pixels
                .checked_mul(4)
                .ok_or(VideoFrameLayoutError::AllocationOverflow),
            Self::BiPlanar420(format) => {
                if !geometry.coded_width.is_multiple_of(2)
                    || !geometry.coded_height.is_multiple_of(2)
                {
                    return Err(VideoFrameLayoutError::OddSubsampledDimensions {
                        width: geometry.coded_width,
                        height: geometry.coded_height,
                    });
                }
                let bytes_per_two_pixels = match format {
                    BiPlanarVideoFormat::Nv12 => 3,
                    BiPlanarVideoFormat::P010 => 6,
                };
                pixels
                    .checked_mul(bytes_per_two_pixels)
                    .map(|bytes| bytes / 2)
                    .ok_or(VideoFrameLayoutError::AllocationOverflow)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoColorPrimaries {
    Bt601_525,
    Bt601_625,
    Bt709,
    Bt2020,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoTransferCharacteristic {
    Srgb,
    Bt709,
    Pq,
    Hlg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoMatrixCoefficients {
    Identity,
    Bt601,
    Bt709,
    Bt2020NonConstantLuminance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoColorRange {
    Limited,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoChromaLocation {
    Left,
    Center,
    TopLeft,
}

/// Color metadata required to turn decoded sample values into display RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoColorimetry {
    pub primaries: VideoColorPrimaries,
    pub transfer: VideoTransferCharacteristic,
    pub matrix: VideoMatrixCoefficients,
    pub range: VideoColorRange,
    pub chroma_location: VideoChromaLocation,
}

impl VideoColorimetry {
    pub const SRGB: Self = Self {
        primaries: VideoColorPrimaries::Bt709,
        transfer: VideoTransferCharacteristic::Srgb,
        matrix: VideoMatrixCoefficients::Identity,
        range: VideoColorRange::Full,
        chroma_location: VideoChromaLocation::Center,
    };

    pub const BT709_LIMITED: Self = Self {
        primaries: VideoColorPrimaries::Bt709,
        transfer: VideoTransferCharacteristic::Bt709,
        matrix: VideoMatrixCoefficients::Bt709,
        range: VideoColorRange::Limited,
        chroma_location: VideoChromaLocation::Left,
    };
}

/// Commands that act on an opened playback session.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackAction {
    Play,
    Pause,
    Stop,
    Seek(MediaTime),
    SetRate(PlaybackRate),
    SetLoop(LoopMode),
}

/// Whether any eligible native window currently presents a video session.
/// This is compositor policy, not user playback intent: hiding a session may
/// suspend native decode without changing its Lisp-visible Playing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationVisibility {
    Presented,
    Hidden,
}

/// Command accepted by the host video system and its native adapters.
#[derive(Debug, Clone, PartialEq)]
pub enum VideoCommand {
    Open {
        id: VideoId,
        source: VideoSource,
        initial_playback: InitialPlayback,
        loop_mode: LoopMode,
    },
    Playback {
        id: VideoId,
        action: PlaybackAction,
    },
    Presentation {
        id: VideoId,
        visibility: PresentationVisibility,
    },
    Close {
        id: VideoId,
    },
}

/// GPU-independent session intent retained across renderer device loss.
/// The authoritative playback system creates this value; renderer facades do
/// not mirror or reconstruct loop/position/play state.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoRecoveryManifest {
    pub(crate) source: VideoSource,
    pub(crate) loop_mode: LoopMode,
    pub(crate) desired_playing: bool,
    pub(crate) rate: PlaybackRate,
    pub(crate) position: MediaTime,
    pub(crate) presentation: PresentationVisibility,
}

impl VideoRecoveryManifest {
    pub const fn new(
        source: VideoSource,
        loop_mode: LoopMode,
        desired_playing: bool,
        rate: PlaybackRate,
        position: MediaTime,
        presentation: PresentationVisibility,
    ) -> Self {
        Self {
            source,
            loop_mode,
            desired_playing,
            rate,
            position,
            presentation,
        }
    }

    pub const fn source(&self) -> &VideoSource {
        &self.source
    }

    pub const fn loop_mode(&self) -> LoopMode {
        self.loop_mode
    }

    pub const fn desired_playing(&self) -> bool {
        self.desired_playing
    }

    pub const fn rate(&self) -> PlaybackRate {
        self.rate
    }

    pub const fn position(&self) -> MediaTime {
        self.position
    }

    pub const fn presentation(&self) -> PresentationVisibility {
        self.presentation
    }

    pub fn with_desired_playing(mut self, desired_playing: bool) -> Self {
        self.desired_playing = desired_playing;
        self
    }

    pub fn stopped(mut self) -> Self {
        self.desired_playing = false;
        self.position = MediaTime::ZERO;
        self
    }

    pub fn with_loop_mode(mut self, loop_mode: LoopMode) -> Self {
        self.loop_mode = loop_mode;
        self
    }

    pub fn with_presentation(mut self, presentation: PresentationVisibility) -> Self {
        self.presentation = presentation;
        self
    }
}

/// One native video session paired with its GPU-independent recovery state.
///
/// The identity is kept outside [`VideoRecoveryManifest`] so a manifest can
/// be deliberately rebound by a higher layer without temporarily pretending
/// that a stable editor id is a native decoder-session id.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoSessionRecovery {
    pub(crate) id: VideoId,
    pub(crate) manifest: VideoRecoveryManifest,
}

impl VideoSessionRecovery {
    pub const fn new(id: VideoId, manifest: VideoRecoveryManifest) -> Self {
        Self { id, manifest }
    }

    pub const fn id(&self) -> VideoId {
        self.id
    }

    pub const fn manifest(&self) -> &VideoRecoveryManifest {
        &self.manifest
    }

    pub fn into_manifest(self) -> VideoRecoveryManifest {
        self.manifest
    }
}

/// Evidence the playback adapter can provide about where decoding occurred.
///
/// This is intentionally independent from compositor import. A frame can be
/// imported without a pixel copy even when a high-level player does not expose
/// enough information to prove how the decoder produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoDecodeResidency {
    /// The selected hardware decoder reports the renderer's DRM device.
    ///
    /// This says nothing about downstream allocation locality or ownership;
    /// compositor-import evidence records whether the observed frame itself
    /// reached the renderer without a pixel copy.
    HardwareDecoderReportsRendererDevice,
    HardwareUnverified,
    Software,
    Unknown,
}

/// Pixel movement performed at the native-frame-to-compositor seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VideoCompositorImport {
    BorrowedNativeSurface,
    GpuBlit,
    CpuUpload,
}

/// How the selected frame reaches the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPresentationPath {
    WgpuComposited,
    NativeOverlay,
}

/// Independently observable evidence for one compositor-visible frame.
///
/// Keeping the axes separate makes it impossible to use a direct GPU import
/// as proof of hardware decoding, or to confuse native overlay promotion with
/// a sampleable texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoFramePath {
    decode_residency: VideoDecodeResidency,
    compositor_import: VideoCompositorImport,
    presentation: VideoPresentationPath,
}

impl VideoFramePath {
    pub const fn new(
        decode_residency: VideoDecodeResidency,
        compositor_import: VideoCompositorImport,
        presentation: VideoPresentationPath,
    ) -> Self {
        Self {
            decode_residency,
            compositor_import,
            presentation,
        }
    }

    pub const fn decode_residency(self) -> VideoDecodeResidency {
        self.decode_residency
    }

    pub const fn compositor_import(self) -> VideoCompositorImport {
        self.compositor_import
    }

    pub const fn presentation(self) -> VideoPresentationPath {
        self.presentation
    }
}

/// How far the compositor importer may fall back from borrowing a native
/// decoder/player surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameImportPolicy {
    RequireDirectSurface,
    AllowGpuBlit,
    AllowCpuUpload,
}

impl FrameImportPolicy {
    /// Neomacs's product default: preserve GPU residency while allowing one
    /// observable native GPU blit for platforms and drivers that cannot lend
    /// decoder/player surfaces directly. CPU upload remains explicit opt-in.
    pub const PERFORMANCE_DEFAULT: Self = Self::AllowGpuBlit;

    pub const fn permits(self, import: VideoCompositorImport) -> bool {
        match self {
            Self::RequireDirectSurface => {
                matches!(import, VideoCompositorImport::BorrowedNativeSurface)
            }
            Self::AllowGpuBlit => !matches!(import, VideoCompositorImport::CpuUpload),
            Self::AllowCpuUpload => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoDecodeBackend {
    GStreamer,
    AvFoundation,
    MediaFoundation,
    Unsupported,
}

/// Evidence reported by the native playback backend about the selected codec
/// implementation.  This is separate from decoded-frame residency: a hardware
/// decoder can still require a copy before composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoDecoderKind {
    Hardware,
    Software,
    Unknown,
}

/// Stable, handle-free identity of the codec implementation selected for one
/// playback session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDecoderIdentity {
    pub factory: String,
    pub plugin: String,
    pub kind: VideoDecoderKind,
}

/// Graphics API used by the compositor device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoGraphicsBackend {
    Vulkan,
    Metal,
    Dx12,
    Gl,
    BrowserWebGpu,
    Other,
}

/// Stable, serializable renderer facts needed to compare native-video runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoRendererIdentity {
    pub adapter_name: String,
    pub vendor: u32,
    pub device: u32,
    pub device_type: String,
    pub graphics_backend: VideoGraphicsBackend,
    pub driver: String,
    pub driver_info: String,
    pub drm_render_node: Option<String>,
    pub display_refresh_hz: Option<u16>,
}

/// Per-path frame counts and byte volumes that the platform could actually
/// observe. Unknown upstream conversion volume is deliberately not estimated.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VideoImportCounts {
    pub borrowed_native_frames: u64,
    pub gpu_blit_frames: u64,
    pub cpu_upload_frames: u64,
    pub reported_gpu_blit_bytes: u64,
    pub cpu_upload_bytes: u64,
}

/// Renderer-owned evidence that imported frames reached the GPU and surface.
///
/// Import success alone does not prove a draw was submitted or that its target
/// swapchain image was handed to the window system. Keeping those observations
/// separate prevents diagnostics from upgrading an intended path into an
/// end-to-end claim.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VideoPresentationCounts {
    /// GPU draw submissions containing this video.
    pub submitted_frames: u64,
    /// Legacy diagnostic field: completed surface handoffs via queue.present.
    /// This does not count compositor-confirmed presentations or scanout.
    pub presented_frames: u64,
}

/// CPU-observed spacing between completed surface submissions.
///
/// Percentiles cover the renderer's bounded recent observation window.  They
/// measure the complete composited surface cadence for frames containing this
/// video; they are not decoder timestamps and do not claim scan-out timing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VideoPresentationTiming {
    pub interval_samples: u64,
    pub interval_total_us: u64,
    pub interval_min_us: Option<u64>,
    pub interval_max_us: Option<u64>,
    pub interval_p50_us: Option<u64>,
    pub interval_p95_us: Option<u64>,
    pub interval_p99_us: Option<u64>,
}

/// GPU timestamp-query observations for render passes containing a video.
///
/// This is the complete frame-content pass, including text and other inline
/// media in that pass.  It deliberately does not call this decoder time or
/// video-only shader time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum VideoGpuTimingStatus {
    /// Instrumentation was not requested, so ordinary playback pays no query cost.
    #[default]
    Disabled,
    /// Instrumentation was requested but the selected adapter cannot provide it.
    Unsupported,
    /// Timestamp queries are active and samples may be reported.
    Enabled,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VideoGpuTiming {
    pub status: VideoGpuTimingStatus,
    pub pass_samples: u64,
    pub pass_total_us: u64,
    pub pass_min_us: Option<u64>,
    pub pass_max_us: Option<u64>,
}

/// Truthful counters and effective frame-path evidence for one playback session.
/// Platform handles stay private; absent native details are never fabricated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoSessionDiagnostics {
    pub id: VideoId,
    pub backend: VideoDecodeBackend,
    pub decoder: Option<VideoDecoderIdentity>,
    pub state: VideoSessionState,
    pub frame_path: Option<VideoFramePath>,
    pub frame_format: Option<VideoFrameFormat>,
    pub colorimetry: Option<VideoColorimetry>,
    pub decoded_frames: u64,
    pub replaced_frames: u64,
    pub late_dropped_frames: u64,
    pub imported_frames: u64,
    pub backpressured_frames: u64,
    pub output_reconfigurations: u64,
    pub import_counts: VideoImportCounts,
    pub presentation_counts: VideoPresentationCounts,
    pub presentation_timing: VideoPresentationTiming,
    pub gpu_timing: VideoGpuTiming,
    pub terminal_error: Option<VideoCommandError>,
}

/// Which native-video stage owns a bounded surface pool.
///
/// Keeping this typed distinguishes decoder-owned output surfaces from
/// compositor import/copy targets without exposing platform handle types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoSurfacePoolRole {
    DecoderOutput,
    CompositorImport,
}

/// Point-in-time occupancy plus cumulative activity for one native surface
/// pool. Counts describe real pool operations rather than inferred frame
/// sizes or assumed decoder behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSurfacePoolDiagnostics {
    pub role: VideoSurfacePoolRole,
    /// Maximum number of live or reserved entries.
    pub capacity: usize,
    /// Fulfilled native surfaces, including idle and checked-out entries.
    pub allocated: usize,
    /// Fulfilled surfaces currently available for keyed reuse.
    pub idle: usize,
    /// Fulfilled surfaces retained by frames awaiting GPU retirement.
    pub in_flight: usize,
    /// Cumulative successfully fulfilled native allocations.
    pub allocations: u64,
    /// Cumulative keyed acquisitions served without allocating.
    pub reuses: u64,
    /// Cumulative acquisitions rejected because every slot was checked out.
    pub backpressured_acquires: u64,
    /// Largest observed number of fulfilled checked-out surfaces.
    pub in_flight_high_water: usize,
}

/// Point-in-time diagnostics owned by the authoritative video system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDiagnostics {
    pub renderer: Option<VideoRendererIdentity>,
    pub sessions: Vec<VideoSessionDiagnostics>,
    pub surface_pools: Vec<VideoSurfacePoolDiagnostics>,
    pub gpu_memory_bytes: usize,
}

impl VideoDiagnostics {
    /// Rebind native decoder identities to the stable identity domain owned by
    /// a caller, dropping sessions whose native incarnation is no longer
    /// current. Pool and GPU totals remain unchanged because they describe the
    /// shared video subsystem rather than an individual session.
    pub fn filter_map_session_ids(
        mut self,
        mut map: impl FnMut(VideoId) -> Option<VideoId>,
    ) -> Self {
        self.sessions.retain_mut(|session| {
            let Some(id) = map(session.id) else {
                return false;
            };
            session.id = id;
            true
        });
        self
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum VideoInitError {
    #[error("video playback is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("{backend:?} requires compositor import {path:?}, forbidden by {policy:?}")]
    ImportForbidden {
        backend: VideoDecodeBackend,
        policy: FrameImportPolicy,
        path: VideoCompositorImport,
    },
    #[error("failed to initialize {backend:?}: {message}")]
    Backend {
        backend: VideoDecodeBackend,
        message: String,
    },
}

/// Backend-owned action that can install a missing media component.
///
/// The closed enum prevents a platform-independent caller from treating an
/// opaque installer token as portable across native backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoInstallerHint {
    GStreamer { detail: String },
}

impl VideoInstallerHint {
    pub fn gstreamer(detail: impl Into<String>) -> Self {
        Self::GStreamer {
            detail: detail.into(),
        }
    }
}

/// One unavailable media component described by a native backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingVideoPlugin {
    description: String,
    installer_hint: Option<VideoInstallerHint>,
}

impl MissingVideoPlugin {
    pub fn new(description: impl Into<String>, installer_hint: Option<VideoInstallerHint>) -> Self {
        Self {
            description: description.into(),
            installer_hint,
        }
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// Backend-tagged machine-readable installation action, when supplied.
    pub fn installer_hint(&self) -> Option<&VideoInstallerHint> {
        self.installer_hint.as_ref()
    }
}

/// Non-empty set of codec/demuxer plugins required by one media source.
/// Storing the first element separately makes an empty `MissingPlugins`
/// failure unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingVideoPlugins {
    first: MissingVideoPlugin,
    additional: Vec<MissingVideoPlugin>,
}

impl MissingVideoPlugins {
    pub fn new(first: MissingVideoPlugin) -> Self {
        Self {
            first,
            additional: Vec::new(),
        }
    }

    pub fn push(&mut self, plugin: MissingVideoPlugin) {
        if self.iter().all(|existing| existing != &plugin) {
            self.additional.push(plugin);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &MissingVideoPlugin> {
        std::iter::once(&self.first).chain(&self.additional)
    }
}

impl std::fmt::Display for MissingVideoPlugins {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, plugin) in self.iter().enumerate() {
            if index != 0 {
                formatter.write_str(", ")?;
            }
            formatter.write_str(plugin.description())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoCommandError {
    #[error("video {id} is already open")]
    SessionAlreadyOpen { id: u32 },
    #[error("video {id} is not open")]
    SessionNotOpen { id: u32 },
    #[error("video {id} is in a terminal failed state")]
    SessionFailed { id: u32 },
    #[error("video decoder and compositor adapters are incompatible: {details}")]
    AdapterMismatch { details: String },
    #[error("video compositor import {path:?} is forbidden by {policy:?}")]
    ImportForbidden {
        policy: FrameImportPolicy,
        path: VideoCompositorImport,
    },
    #[error("video importer classified {planned:?} but produced {actual:?}")]
    ImportContract {
        planned: VideoCompositorImport,
        actual: VideoCompositorImport,
    },
    #[error("video source requires unavailable media components: {plugins}")]
    MissingPlugins { plugins: MissingVideoPlugins },
    #[error("native video backend failed: {message}")]
    Backend { message: String },
    #[error("video frame import failed: {message}")]
    Import { message: String },
}

impl From<String> for VideoCommandError {
    fn from(message: String) -> Self {
        Self::Backend { message }
    }
}

impl From<&str> for VideoCommandError {
    fn from(message: &str) -> Self {
        Self::Backend {
            message: message.to_owned(),
        }
    }
}

/// One coherent playback lifecycle; impossible combinations are not separate flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoSessionState {
    Opening,
    Paused,
    Playing,
    Ended,
    Failed,
    Closed,
}

/// Observable events produced while servicing native playback adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoEvent {
    Ready {
        id: VideoId,
        width: u32,
        height: u32,
    },
    StateChanged {
        id: VideoId,
        state: VideoSessionState,
    },
    /// The successfully imported frame selected a different observable path.
    /// Emitted once per transition rather than once per frame.
    FramePathChanged {
        id: VideoId,
        previous: Option<VideoFramePath>,
        current: VideoFramePath,
    },
    Ended {
        id: VideoId,
    },
    Failed {
        id: VideoId,
        error: VideoCommandError,
    },
}

impl VideoEvent {
    /// Return the session identity carried by this event.
    pub const fn id(&self) -> VideoId {
        match self {
            Self::Ready { id, .. }
            | Self::StateChanged { id, .. }
            | Self::FramePathChanged { id, .. }
            | Self::Ended { id }
            | Self::Failed { id, .. } => *id,
        }
    }

    /// Replace only the session identity while preserving the event payload.
    ///
    /// Renderer integrations use this at the boundary between ephemeral
    /// native session identities and editor-stable video identities. Keeping
    /// the exhaustive match here makes a new event variant a compile-time
    /// obligation in the model instead of duplicated consumer bookkeeping.
    pub fn with_id(self, id: VideoId) -> Self {
        match self {
            Self::Ready { width, height, .. } => Self::Ready { id, width, height },
            Self::StateChanged { state, .. } => Self::StateChanged { id, state },
            Self::FramePathChanged {
                previous, current, ..
            } => Self::FramePathChanged {
                id,
                previous,
                current,
            },
            Self::Ended { .. } => Self::Ended { id },
            Self::Failed { error, .. } => Self::Failed { id, error },
        }
    }
}

/// Work discovered by one non-blocking service pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoFrameReady {
    pub id: VideoId,
    pub pts: MediaTime,
    pub frame_path: VideoFramePath,
}

/// The two monotonic instants needed by a native video service pass.
///
/// Pull-based adapters sample for `target_presentation_time`; push-based
/// adapters can use `service_time` for clock and deadline bookkeeping. The
/// private fields prevent a caller from constructing a target in the past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoServiceTiming {
    service_time: Instant,
    target_presentation_time: Instant,
}

impl VideoServiceTiming {
    pub fn new(service_time: Instant, target_presentation_time: Instant) -> Self {
        Self {
            service_time,
            target_presentation_time: target_presentation_time.max(service_time),
        }
    }

    pub const fn immediate(service_time: Instant) -> Self {
        Self {
            service_time,
            target_presentation_time: service_time,
        }
    }

    pub const fn service_time(self) -> Instant {
        self.service_time
    }

    pub const fn target_presentation_time(self) -> Instant {
        self.target_presentation_time
    }

    pub fn time_until_presentation(self) -> std::time::Duration {
        self.target_presentation_time
            .saturating_duration_since(self.service_time)
    }
}

/// One non-blocking service pass with an independent presentation target for
/// every visible video session.
///
/// A video can appear in windows with different refresh rates. Storing targets
/// by [`VideoId`] prevents one window's cadence from being applied to an
/// unrelated player, while repeated presentations of the same video converge
/// on the earliest compositor opportunity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoServiceRequest {
    service_time: Instant,
    presentation_targets: HashMap<VideoId, Instant>,
}

impl VideoServiceRequest {
    pub fn new(service_time: Instant) -> Self {
        Self {
            service_time,
            presentation_targets: HashMap::new(),
        }
    }

    pub const fn service_time(&self) -> Instant {
        self.service_time
    }

    pub fn set_presentation_target(&mut self, id: VideoId, target: Instant) {
        let target = target.max(self.service_time);
        self.presentation_targets
            .entry(id)
            .and_modify(|existing| *existing = std::cmp::min(*existing, target))
            .or_insert(target);
    }

    pub fn is_presented(&self, id: VideoId) -> bool {
        self.presentation_targets.contains_key(&id)
    }

    pub fn timing_for(&self, id: VideoId) -> VideoServiceTiming {
        VideoServiceTiming::new(
            self.service_time,
            self.presentation_targets
                .get(&id)
                .copied()
                .unwrap_or(self.service_time),
        )
    }

    pub fn presentation_targets(&self) -> impl Iterator<Item = (VideoId, Instant)> + '_ {
        self.presentation_targets
            .iter()
            .map(|(&id, &target)| (id, target))
    }
}

/// Work discovered by one non-blocking service pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VideoServiceResult {
    pub events: Vec<VideoEvent>,
    pub ready_frames: Vec<VideoFrameReady>,
    /// Earliest wall-clock instant at which a queued frame becomes due.
    pub next_deadline: Option<Instant>,
}
