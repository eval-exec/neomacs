//! AVFoundation playback and zero-copy CoreVideo -> Metal replay.

use std::collections::HashMap;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use neomacs_display_protocol::types::VideoId;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_av_foundation::{
    AVMediaTypeVideo, AVPlayer, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput,
    AVVideoAllowWideColorKey, AVVideoColorPrimaries_ITU_R_709_2, AVVideoColorPrimariesKey,
    AVVideoColorPropertiesKey, AVVideoTransferFunction_IEC_sRGB, AVVideoTransferFunctionKey,
    AVVideoYCbCrMatrix_ITU_R_709_2, AVVideoYCbCrMatrixKey,
};
use objc2_core_foundation::{CFBoolean, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMFormatDescription, CMTime, kCMFormatDescriptionExtension_BitsPerComponent,
    kCMFormatDescriptionExtension_FullRangeVideo, kCMFormatDescriptionExtension_TransferFunction,
    kCMFormatDescriptionTransferFunction_ITU_R_2100_HLG,
    kCMFormatDescriptionTransferFunction_SMPTE_ST_2084_PQ,
};
use objc2_core_video::{
    CVImageBufferGetCleanRect, CVImageBufferGetDisplaySize, CVMetalTexture, CVMetalTextureCache,
    CVMetalTextureGetTexture, CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount, CVPixelBufferGetWidth,
    CVPixelBufferGetWidthOfPlane, kCVImageBufferChromaLocation_Center,
    kCVImageBufferChromaLocation_TopLeft, kCVImageBufferChromaLocationTopFieldKey,
    kCVImageBufferColorPrimaries_EBU_3213, kCVImageBufferColorPrimaries_ITU_R_2020,
    kCVImageBufferColorPrimaries_SMPTE_C, kCVImageBufferColorPrimariesKey,
    kCVImageBufferTransferFunction_ITU_R_2100_HLG, kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,
    kCVImageBufferTransferFunction_sRGB, kCVImageBufferTransferFunctionKey,
    kCVImageBufferYCbCrMatrix_ITU_R_601_4, kCVImageBufferYCbCrMatrix_ITU_R_2020,
    kCVImageBufferYCbCrMatrixKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_metal::{MTLPixelFormat, MTLTextureType};

use crate::backend::{
    BackendEvent, CompletedFrameTransfer, DecodedFrame, DecodedFrameTransfer, DecoderBackend,
    DecoderReconfiguration, FrameImportOutcome, FrameImporter, ImportedFrame, Platform,
    ProductionPlatform, require_fixed_transfer_path,
};
use crate::sampling::{GpuVideoContext, PreparedBiPlanarTexture, PreparedSampledTexture};
use crate::surface_pool::{BoundedSurfacePool, SurfacePoolAcquire};
use crate::{
    BiPlanarVideoFormat, FrameTiming, GpuVideoFrame, InitialPlayback, LoopMode, MediaTime,
    PackedVideoFormat, PlaybackAction, PlaybackEpoch, VideoChromaLocation, VideoColorPrimaries,
    VideoColorRange, VideoColorimetry, VideoCommand, VideoDecodeBackend, VideoFrameFormat,
    VideoGeometry, VideoInitError, VideoMatrixCoefficients, VideoSessionState, VideoSource,
    VideoTransferCharacteristic, VideoTransferPath, VideoWake,
};

pub(crate) struct MacPlatform;

/// Pull AVPlayerItemVideoOutput through the common scheduler often enough for
/// 120-Hz media without installing a second display clock. Once a frame is
/// published, its PTS becomes the more precise compositor deadline.
const MAC_MEDIA_POLL_INTERVAL: Duration = Duration::from_micros(8_000);
const MAX_IN_FLIGHT_MAC_VIDEO_SURFACES: usize = 8;

/// Affine ownership of one CoreVideo decoder surface.
pub(crate) struct MacFrame {
    pixel_buffer: Retained<CVPixelBuffer>,
}

// CVPixelBuffer is explicitly designed for cross-queue video pipelines. This
// private lease states that guarantee at the native boundary; Objective-C
// player objects remain main-thread-confined.
unsafe impl Send for MacFrame {}
unsafe impl Sync for MacFrame {}

struct MacSession {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Option<MacNegotiatedOutput>,
    state: VideoSessionState,
    loop_mode: LoopMode,
    playback_rate: f32,
    announced: bool,
    presented: bool,
    awaiting_frame: bool,
    ended: bool,
    epoch: PlaybackEpoch,
    rotation: Option<crate::VideoRotation>,
}

impl MacSession {
    fn needs_media_poll(&self) -> bool {
        self.presented
            && (matches!(
                self.state,
                VideoSessionState::Opening | VideoSessionState::Playing
            ) || self.awaiting_frame)
    }
}

pub(crate) struct MacDecoder {
    supports_p010: bool,
    sessions: HashMap<VideoId, MacSession>,
    pending: Vec<BackendEvent<MacFrame>>,
}

impl MacDecoder {
    fn new(supports_p010: bool, _wake: VideoWake) -> Result<Self, String> {
        MainThreadMarker::new().ok_or_else(|| {
            "AVFoundation video must initialize on the macOS main thread".to_string()
        })?;
        Ok(Self {
            supports_p010,
            sessions: HashMap::new(),
            pending: Vec::new(),
        })
    }

    fn open(
        &mut self,
        id: VideoId,
        source: VideoSource,
        initial: InitialPlayback,
        loop_mode: LoopMode,
    ) -> Result<(), String> {
        if self.sessions.contains_key(&id) {
            return Err(format!("video {} is already open", id.get()));
        }
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            "AVFoundation video commands must run on the macOS main thread".to_string()
        })?;
        let url = source_url(source)?;
        let item = unsafe { AVPlayerItem::playerItemWithURL(&url, mtm) };
        let player = unsafe { AVPlayer::playerWithPlayerItem(Some(&item), mtm) };
        // Inline Neomacs video has historically been visual-only. Keep that
        // contract consistent with Linux's fakesink until audio is modeled as
        // a separate, focus-aware subsystem.
        unsafe { player.setMuted(true) };
        let state = match initial {
            // Wait until the item is ready and its active track metadata has
            // selected the matching CoreVideo output before starting decode.
            InitialPlayback::Playing => VideoSessionState::Playing,
            InitialPlayback::Paused => {
                unsafe { player.pause() };
                VideoSessionState::Opening
            }
        };
        self.sessions.insert(
            id,
            MacSession {
                player,
                item,
                output: None,
                state,
                loop_mode,
                playback_rate: 1.0,
                announced: false,
                presented: true,
                awaiting_frame: true,
                ended: false,
                epoch: PlaybackEpoch::INITIAL,
                rotation: None,
            },
        );
        Ok(())
    }

    fn playback(&mut self, id: VideoId, action: PlaybackAction) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or_else(|| format!("video {} is not open", id.get()))?;
        match action {
            PlaybackAction::Play => {
                session.state = VideoSessionState::Playing;
                session.ended = false;
                if session.presented && session.output.is_some() {
                    unsafe { session.player.playImmediatelyAtRate(session.playback_rate) };
                }
            }
            PlaybackAction::Pause => {
                session.state = VideoSessionState::Paused;
                unsafe { session.player.pause() };
            }
            PlaybackAction::Stop => {
                session.epoch = session.epoch.next();
                session.state = VideoSessionState::Paused;
                session.awaiting_frame = true;
                session.ended = false;
                unsafe {
                    session.player.pause();
                    session.player.seekToTime(CMTime::new(0, 1_000_000_000));
                }
            }
            PlaybackAction::Seek(time) => {
                session.epoch = session.epoch.next();
                session.awaiting_frame = true;
                session.ended = false;
                unsafe {
                    session
                        .player
                        .seekToTime(CMTime::new(time.as_nanos() as i64, 1_000_000_000));
                }
            }
            PlaybackAction::SetRate(rate) => {
                session.playback_rate = rate.get() as f32;
                if session.presented
                    && session.state == VideoSessionState::Playing
                    && session.output.is_some()
                {
                    unsafe { session.player.setRate(session.playback_rate) };
                }
            }
            PlaybackAction::SetLoop(mode) => session.loop_mode = mode,
        }
        self.pending.push(BackendEvent::StateChanged {
            id,
            state: session.state,
        });
        Ok(())
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejected: VideoFrameFormat,
    ) -> Result<DecoderReconfiguration, String> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or_else(|| format!("video {} is not open", id.get()))?;
        let Some(current) = session.output.as_ref() else {
            return Ok(DecoderReconfiguration::Unsupported);
        };
        let Some(fallback) = current.format.fallback_after_rejection(rejected) else {
            return Ok(DecoderReconfiguration::Unsupported);
        };
        let replacement = create_player_item_output(fallback)?;
        let current = session
            .output
            .take()
            .expect("the current output was validated above");
        unsafe {
            session.item.removeOutput(&current.video_output);
            session.item.addOutput(&replacement.video_output);
        }
        session.output = Some(replacement);
        session.awaiting_frame = true;
        Ok(DecoderReconfiguration::Applied)
    }

    fn poll_sessions(&mut self) {
        autoreleasepool(|_| {
            let mut events = Vec::new();
            let mut failed = Vec::new();
            for (&id, session) in &mut self.sessions {
                if unsafe { session.item.status() } == AVPlayerItemStatus::Failed {
                    let detail = unsafe { session.item.error() }
                        .map(|error| format!("{error:?}"))
                        .unwrap_or_else(|| "unknown AVFoundation error".into());
                    failed.push((id, format!("AVFoundation failed to load video: {detail}")));
                    continue;
                }

                if unsafe { session.item.status() } == AVPlayerItemStatus::ReadyToPlay
                    && session.output.is_none()
                {
                    match configure_player_item_output(&session.item, self.supports_p010) {
                        Ok(output) => {
                            session.output = Some(output);
                            if session.presented && session.state == VideoSessionState::Playing {
                                unsafe {
                                    session.player.playImmediatelyAtRate(session.playback_rate)
                                };
                            }
                        }
                        Err(error) => {
                            failed.push((id, error));
                            continue;
                        }
                    }
                }

                // Presentation visibility suspends both AVPlayer and native
                // pixel-buffer pulls. Another visible session may drive this
                // global service pass; it must not wake hidden decoders.
                if !session.presented {
                    continue;
                }

                let Some(output) = session.output.as_ref() else {
                    continue;
                };

                let item_time = unsafe { session.item.currentTime() };
                if unsafe { output.video_output.hasNewPixelBufferForItemTime(item_time) } {
                    let mut display_time = item_time;
                    if let Some(pixel_buffer) = unsafe {
                        output
                            .video_output
                            .copyPixelBufferForItemTime_itemTimeForDisplay(
                                item_time,
                                &mut display_time,
                            )
                    } {
                        let rotation = *session
                            .rotation
                            .get_or_insert_with(|| player_item_rotation(&session.item));
                        let geometry = geometry_from_pixel_buffer(&pixel_buffer, rotation);
                        let format = match frame_format_from_pixel_buffer(&pixel_buffer) {
                            Ok(format) => format,
                            Err(error) => {
                                failed.push((id, error));
                                continue;
                            }
                        };
                        let colorimetry = colorimetry_from_pixel_buffer(&pixel_buffer, format);
                        if !session.announced {
                            let initial_state = match session.state {
                                VideoSessionState::Opening => VideoSessionState::Paused,
                                state => state,
                            };
                            session.state = initial_state;
                            session.announced = true;
                            events.push(BackendEvent::Opened {
                                id,
                                width: geometry.display_width,
                                height: geometry.display_height,
                                initial_state,
                            });
                        }
                        session.awaiting_frame = false;
                        let pts = media_time(display_time);
                        // AVPlayerItemVideoOutput exposes the current item's
                        // display PTS but not a reliable per-frame duration.
                        // Zero means unknown; the common late-drop policy
                        // must not attach the previous frame's interval to
                        // this variable-rate frame.
                        let duration = MediaTime::ZERO;
                        events.push(BackendEvent::Frame {
                            id,
                            frame: DecodedFrame {
                                lease: MacFrame { pixel_buffer },
                                timing: FrameTiming {
                                    pts,
                                    duration,
                                    epoch: session.epoch,
                                },
                                geometry,
                                format,
                                colorimetry,
                                decoder_transfer: DecodedFrameTransfer::Completed(output.transfer),
                            },
                        });
                    }
                }

                let duration = unsafe { session.item.duration().seconds() };
                let current = unsafe { item_time.seconds() };
                if duration.is_finite()
                    && duration > 0.0
                    && current >= duration - 0.001
                    && !session.ended
                {
                    if session.loop_mode.consume_replay() {
                        session.epoch = session.epoch.next();
                        session.awaiting_frame = true;
                        events.push(BackendEvent::Looped {
                            id,
                            remaining: session.loop_mode,
                        });
                        unsafe {
                            session.player.seekToTime(CMTime::new(0, 1_000_000_000));
                            if session.state == VideoSessionState::Playing && session.presented {
                                session.player.playImmediatelyAtRate(session.playback_rate);
                            }
                        }
                    } else {
                        session.ended = true;
                        session.state = VideoSessionState::Ended;
                        events.push(BackendEvent::Ended { id });
                    }
                }
            }
            for (id, message) in failed {
                self.sessions.remove(&id);
                events.push(BackendEvent::Failed {
                    id,
                    error: message.into(),
                });
            }
            self.pending.extend(events);
        });
    }
}

fn geometry_from_pixel_buffer(
    pixel_buffer: &CVPixelBuffer,
    rotation: crate::VideoRotation,
) -> VideoGeometry {
    let coded_width = CVPixelBufferGetWidth(pixel_buffer) as u32;
    let coded_height = CVPixelBufferGetHeight(pixel_buffer) as u32;
    let clean = CVImageBufferGetCleanRect(pixel_buffer);
    let display = CVImageBufferGetDisplaySize(pixel_buffer);
    let clean_width = finite_rounded_dimension(clean.size.width, coded_width);
    let clean_height = finite_rounded_dimension(clean.size.height, coded_height);
    let clean_x = finite_rounded_coordinate(clean.origin.x, 0).min(coded_width);
    // CoreVideo reports the clean aperture from a lower-left origin; wgpu's
    // video texture convention is top-left.
    let lower_y = finite_rounded_coordinate(clean.origin.y, 0);
    let clean_y = coded_height.saturating_sub(lower_y.saturating_add(clean_height));
    let display_width = finite_rounded_dimension(display.width, clean_width);
    let display_height = finite_rounded_dimension(display.height, clean_height);
    let (display_width, display_height) = match rotation {
        crate::VideoRotation::Clockwise90 | crate::VideoRotation::Clockwise270 => {
            (display_height, display_width)
        }
        crate::VideoRotation::None | crate::VideoRotation::Clockwise180 => {
            (display_width, display_height)
        }
    };
    VideoGeometry::with_visible_rect_and_display_size(
        coded_width,
        coded_height,
        crate::PixelRect {
            x: clean_x,
            y: clean_y,
            width: clean_width.min(coded_width.saturating_sub(clean_x)),
            height: clean_height.min(coded_height.saturating_sub(clean_y)),
        },
        display_width,
        display_height,
        rotation,
    )
}

#[allow(non_upper_case_globals)]
fn frame_format_from_pixel_buffer(
    pixel_buffer: &CVPixelBuffer,
) -> Result<VideoFrameFormat, String> {
    match CVPixelBufferGetPixelFormatType(pixel_buffer) {
        kCVPixelFormatType_32BGRA => Ok(VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)),
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
        | kCVPixelFormatType_420YpCbCr8BiPlanarFullRange => {
            Ok(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12))
        }
        kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
        | kCVPixelFormatType_420YpCbCr10BiPlanarFullRange => {
            Ok(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010))
        }
        fourcc => Err(format!(
            "AVFoundation produced unsupported CoreVideo pixel format {}",
            display_fourcc(fourcc)
        )),
    }
}

#[allow(non_upper_case_globals)]
fn colorimetry_from_pixel_buffer(
    pixel_buffer: &CVPixelBuffer,
    format: VideoFrameFormat,
) -> VideoColorimetry {
    if matches!(format, VideoFrameFormat::Packed(_)) {
        // Packed BGRA is requested with explicit BT.709 primaries/matrix and
        // an IEC sRGB transfer function, as well as wide-color disabled.
        // The sRGB texture view therefore matches the AVFoundation contract.
        return VideoColorimetry::SRGB;
    }

    let primaries = if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferColorPrimariesKey },
        unsafe { kCVImageBufferColorPrimaries_ITU_R_2020 },
    ) {
        VideoColorPrimaries::Bt2020
    } else if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferColorPrimariesKey },
        unsafe { kCVImageBufferColorPrimaries_SMPTE_C },
    ) {
        VideoColorPrimaries::Bt601_525
    } else if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferColorPrimariesKey },
        unsafe { kCVImageBufferColorPrimaries_EBU_3213 },
    ) {
        VideoColorPrimaries::Bt601_625
    } else {
        VideoColorPrimaries::Bt709
    };
    let transfer = if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferTransferFunctionKey },
        unsafe { kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ },
    ) {
        VideoTransferCharacteristic::Pq
    } else if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferTransferFunctionKey },
        unsafe { kCVImageBufferTransferFunction_ITU_R_2100_HLG },
    ) {
        VideoTransferCharacteristic::Hlg
    } else if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferTransferFunctionKey },
        unsafe { kCVImageBufferTransferFunction_sRGB },
    ) {
        VideoTransferCharacteristic::Srgb
    } else {
        VideoTransferCharacteristic::Bt709
    };
    let matrix = if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferYCbCrMatrixKey },
        unsafe { kCVImageBufferYCbCrMatrix_ITU_R_2020 },
    ) {
        VideoMatrixCoefficients::Bt2020NonConstantLuminance
    } else if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferYCbCrMatrixKey },
        unsafe { kCVImageBufferYCbCrMatrix_ITU_R_601_4 },
    ) {
        VideoMatrixCoefficients::Bt601
    } else {
        VideoMatrixCoefficients::Bt709
    };
    let chroma_location = if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferChromaLocationTopFieldKey },
        unsafe { kCVImageBufferChromaLocation_Center },
    ) {
        VideoChromaLocation::Center
    } else if attachment_equals(
        pixel_buffer,
        unsafe { kCVImageBufferChromaLocationTopFieldKey },
        unsafe { kCVImageBufferChromaLocation_TopLeft },
    ) {
        VideoChromaLocation::TopLeft
    } else {
        VideoChromaLocation::Left
    };
    let range = match CVPixelBufferGetPixelFormatType(pixel_buffer) {
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        | kCVPixelFormatType_420YpCbCr10BiPlanarFullRange => VideoColorRange::Full,
        _ => VideoColorRange::Limited,
    };
    VideoColorimetry {
        primaries,
        transfer,
        matrix,
        range,
        chroma_location,
    }
}

fn attachment_equals(pixel_buffer: &CVPixelBuffer, key: &CFString, value: &CFString) -> bool {
    unsafe { pixel_buffer.attachment(key, std::ptr::null_mut()) }
        .and_then(|attachment| attachment.downcast::<CFString>().ok())
        .is_some_and(|attachment| &*attachment == value)
}

fn display_fourcc(fourcc: u32) -> String {
    let bytes = fourcc.to_be_bytes();
    if bytes.iter().all(u8::is_ascii_graphic) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        format!("0x{fourcc:08x}")
    }
}

#[allow(deprecated)]
fn player_item_rotation(item: &AVPlayerItem) -> crate::VideoRotation {
    let Some(media_type) = (unsafe { AVMediaTypeVideo }) else {
        return crate::VideoRotation::None;
    };
    let tracks = unsafe { item.asset().tracksWithMediaType(media_type) };
    let Some(track) = tracks.firstObject() else {
        return crate::VideoRotation::None;
    };
    let transform = unsafe { track.preferredTransform() };
    let near = |value: f64, target: f64| (value - target).abs() <= 0.001;
    if near(transform.b, 1.0) && near(transform.c, -1.0) {
        crate::VideoRotation::Clockwise90
    } else if near(transform.a, -1.0) && near(transform.d, -1.0) {
        crate::VideoRotation::Clockwise180
    } else if near(transform.b, -1.0) && near(transform.c, 1.0) {
        crate::VideoRotation::Clockwise270
    } else {
        crate::VideoRotation::None
    }
}

fn finite_rounded_dimension(value: f64, fallback: u32) -> u32 {
    if value.is_finite() && value > 0.0 && value <= f64::from(u32::MAX) {
        value.round() as u32
    } else {
        fallback.max(1)
    }
}

fn finite_rounded_coordinate(value: f64, fallback: u32) -> u32 {
    if value.is_finite() && value >= 0.0 && value <= f64::from(u32::MAX) {
        value.round() as u32
    } else {
        fallback
    }
}

impl DecoderBackend for MacDecoder {
    type Frame = MacFrame;

    fn command(&mut self, command: VideoCommand) -> Result<(), crate::VideoCommandError> {
        match command {
            VideoCommand::Open {
                id,
                source,
                initial_playback,
                loop_mode,
            } => self
                .open(id, source, initial_playback, loop_mode)
                .map_err(Into::into),
            VideoCommand::Playback { id, action } => self.playback(id, action).map_err(Into::into),
            VideoCommand::Presentation { id, visibility } => {
                let session = self
                    .sessions
                    .get_mut(&id)
                    .ok_or_else(|| format!("video {} is not open", id.get()))?;
                let presented = matches!(visibility, crate::PresentationVisibility::Presented);
                if session.presented == presented {
                    return Ok(());
                }
                session.presented = presented;
                session.awaiting_frame = presented;
                unsafe {
                    if presented
                        && session.state == VideoSessionState::Playing
                        && session.output.is_some()
                    {
                        session.player.playImmediatelyAtRate(session.playback_rate);
                    } else {
                        session.player.pause();
                    }
                }
                Ok(())
            }
            VideoCommand::Close { id } => {
                if self.sessions.remove(&id).is_none() {
                    return Err(crate::VideoCommandError::SessionNotOpen { id: id.get() });
                }
                self.pending.push(BackendEvent::StateChanged {
                    id,
                    state: VideoSessionState::Closed,
                });
                Ok(())
            }
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
        self.poll_sessions();
        std::mem::take(&mut self.pending)
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejected: VideoFrameFormat,
    ) -> Result<DecoderReconfiguration, String> {
        MacDecoder::reconfigure_after_import_failure(self, id, rejected)
    }

    fn next_service_deadline(&self, now: Instant) -> Option<Instant> {
        self.sessions
            .values()
            .any(MacSession::needs_media_poll)
            .then_some(now + MAC_MEDIA_POLL_INTERVAL)
    }
}

fn source_url(source: VideoSource) -> Result<Retained<NSURL>, String> {
    match source {
        VideoSource::File(path) => Ok(NSURL::fileURLWithPath(&NSString::from_str(
            &path.to_string_lossy(),
        ))),
        VideoSource::Uri(uri) => NSURL::URLWithString(&NSString::from_str(&uri))
            .ok_or_else(|| format!("invalid video URI {uri:?}")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MacSourceMetadata {
    bits_per_component: Option<u32>,
    full_range: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MacOutputFormat {
    surface: MacOutputSurface,
    range: VideoColorRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacOutputSurface {
    BiPlanar(BiPlanarVideoFormat),
    Bgra8,
}

impl MacOutputFormat {
    const fn frame_format(self) -> VideoFrameFormat {
        match self.surface {
            MacOutputSurface::BiPlanar(format) => VideoFrameFormat::BiPlanar420(format),
            MacOutputSurface::Bgra8 => VideoFrameFormat::Packed(PackedVideoFormat::Bgra8),
        }
    }

    #[allow(non_upper_case_globals)]
    const fn core_video_pixel_format(self) -> u32 {
        match (self.surface, self.range) {
            (MacOutputSurface::BiPlanar(BiPlanarVideoFormat::Nv12), VideoColorRange::Limited) => {
                kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
            }
            (MacOutputSurface::BiPlanar(BiPlanarVideoFormat::Nv12), VideoColorRange::Full) => {
                kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
            }
            (MacOutputSurface::BiPlanar(BiPlanarVideoFormat::P010), VideoColorRange::Limited) => {
                kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
            }
            (MacOutputSurface::BiPlanar(BiPlanarVideoFormat::P010), VideoColorRange::Full) => {
                kCVPixelFormatType_420YpCbCr10BiPlanarFullRange
            }
            (MacOutputSurface::Bgra8, _) => kCVPixelFormatType_32BGRA,
        }
    }

    const fn completed_transfer(self) -> CompletedFrameTransfer {
        // AVPlayerItemVideoOutput does not expose current source/output
        // identity for adaptive media, nor whether it materialized a new
        // CVPixelBuffer. Plane wrapping is zero-copy after this boundary, but
        // the decoder-side transfer cannot be proven direct.
        CompletedFrameTransfer::GpuInteropCopy {
            reported_bytes: None,
        }
    }

    const fn allows_wide_color(self) -> bool {
        // Bi-planar frames retain their tagged source colorimetry for the
        // shared shader.  The terminal BGRA fallback is an explicit SDR RGB
        // conversion because packed sampling uses an sRGB texture view.
        !self.requires_explicit_sdr_color_properties()
    }

    const fn requires_explicit_sdr_color_properties(self) -> bool {
        matches!(self.surface, MacOutputSurface::Bgra8)
    }

    const fn next_lower_tier(self) -> Option<Self> {
        match self.surface {
            MacOutputSurface::BiPlanar(BiPlanarVideoFormat::P010) => Some(Self {
                surface: MacOutputSurface::BiPlanar(BiPlanarVideoFormat::Nv12),
                range: self.range,
            }),
            MacOutputSurface::BiPlanar(BiPlanarVideoFormat::Nv12) => Some(Self {
                surface: MacOutputSurface::Bgra8,
                range: VideoColorRange::Full,
            }),
            MacOutputSurface::Bgra8 => None,
        }
    }

    fn fallback_after_rejection(self, rejected: VideoFrameFormat) -> Option<Self> {
        let candidate = self.next_lower_tier()?;
        // AVFoundation may return a lower-tier format than requested.  Never
        // "fallback" to the same representation that the importer rejected.
        if candidate.frame_format() == rejected {
            candidate.next_lower_tier()
        } else {
            Some(candidate)
        }
    }
}

struct MacNegotiatedOutput {
    video_output: Retained<AVPlayerItemVideoOutput>,
    format: MacOutputFormat,
    transfer: CompletedFrameTransfer,
}

const fn select_mac_output_format(
    supports_p010: bool,
    source: MacSourceMetadata,
) -> MacOutputFormat {
    let source_is_high_depth = matches!(source.bits_per_component, Some(bits) if bits > 8);
    MacOutputFormat {
        surface: if source_is_high_depth && supports_p010 {
            MacOutputSurface::BiPlanar(BiPlanarVideoFormat::P010)
        } else {
            MacOutputSurface::BiPlanar(BiPlanarVideoFormat::Nv12)
        },
        range: if source.full_range {
            VideoColorRange::Full
        } else {
            VideoColorRange::Limited
        },
    }
}

fn configure_player_item_output(
    item: &AVPlayerItem,
    supports_p010: bool,
) -> Result<MacNegotiatedOutput, String> {
    let source = source_metadata_from_player_item(item)?;
    let format = select_mac_output_format(supports_p010, source);
    let output = create_player_item_output(format)?;
    unsafe { item.addOutput(&output.video_output) };
    Ok(output)
}

fn create_player_item_output(format: MacOutputFormat) -> Result<MacNegotiatedOutput, String> {
    let settings = native_output_settings(format)?;
    let video_output = unsafe {
        AVPlayerItemVideoOutput::initWithOutputSettings(
            AVPlayerItemVideoOutput::alloc(),
            Some(&settings),
        )
    };
    Ok(MacNegotiatedOutput {
        video_output,
        format,
        transfer: format.completed_transfer(),
    })
}

fn source_metadata_from_player_item(item: &AVPlayerItem) -> Result<MacSourceMetadata, String> {
    let video_media_type = unsafe { AVMediaTypeVideo }
        .ok_or_else(|| "this macOS runtime does not expose AVMediaTypeVideo".to_owned())?;
    let tracks = unsafe { item.tracks() };
    let asset_track = tracks
        .iter()
        .filter(|track| unsafe { track.isEnabled() })
        .filter_map(|track| unsafe { track.assetTrack() })
        .find(|track| {
            let media_type = unsafe { track.mediaType() };
            <Retained<NSString> as AsRef<NSString>>::as_ref(&media_type) == video_media_type
        })
        .ok_or_else(|| "AVFoundation item has no active video track".to_owned())?;

    let mut source = MacSourceMetadata {
        bits_per_component: None,
        full_range: false,
    };
    for object in unsafe { asset_track.formatDescriptions() }.iter() {
        let Some(description) = format_description_from_object(&object) else {
            continue;
        };
        let Some(extensions) = (unsafe { description.extensions() }) else {
            continue;
        };
        // CMFormatDescription guarantees a CFString/property-list dictionary.
        let extensions = unsafe { extensions.cast_unchecked::<CFString, CFType>() };
        if let Some(bits) = extensions
            .get(unsafe { kCMFormatDescriptionExtension_BitsPerComponent })
            .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i32))
            .and_then(|bits| u32::try_from(bits).ok())
        {
            source.bits_per_component =
                Some(source.bits_per_component.map_or(bits, |old| old.max(bits)));
        }
        if let Some(value) = extensions.get(unsafe { kCMFormatDescriptionExtension_FullRangeVideo })
        {
            source.full_range |= value
                .downcast_ref::<CFBoolean>()
                .is_some_and(CFBoolean::as_bool)
                || value
                    .downcast_ref::<CFNumber>()
                    .and_then(CFNumber::as_i32)
                    .is_some_and(|value| value != 0);
        }
        let hdr_transfer = extensions
            .get(unsafe { kCMFormatDescriptionExtension_TransferFunction })
            .and_then(|value| value.downcast::<CFString>().ok())
            .is_some_and(|value| {
                &*value == unsafe { kCMFormatDescriptionTransferFunction_SMPTE_ST_2084_PQ }
                    || &*value == unsafe { kCMFormatDescriptionTransferFunction_ITU_R_2100_HLG }
            });
        if hdr_transfer {
            // Apple's HDR playback contract uses a 10-bit bi-planar output;
            // this also covers streams that omit BitsPerComponent.
            source.bits_per_component = Some(source.bits_per_component.unwrap_or(10).max(10));
        }
    }
    Ok(source)
}

fn format_description_from_object(object: &AnyObject) -> Option<&CMFormatDescription> {
    // AVAssetTrack's Objective-C declaration uses an untyped NSArray even
    // though its documented elements are CMFormatDescription CF objects.
    let value = unsafe { &*(object as *const AnyObject).cast::<CFType>() };
    value.downcast_ref::<CMFormatDescription>()
}

fn native_output_settings(
    output: MacOutputFormat,
) -> Result<Retained<NSDictionary<NSString, AnyObject>>, String> {
    // CoreFoundation/NSString and CFNumber/NSNumber are toll-free bridged.
    let format_key =
        unsafe { &*(kCVPixelBufferPixelFormatTypeKey as *const CFString as *const NSString) };
    let metal_key =
        unsafe { &*(kCVPixelBufferMetalCompatibilityKey as *const CFString as *const NSString) };
    let format = NSNumber::new_u32(output.core_video_pixel_format());
    let compatible = NSNumber::new_bool(true);
    let wide_color_key = unsafe { AVVideoAllowWideColorKey }
        .ok_or_else(|| "this macOS runtime cannot request wide-color video output".to_owned())?;
    let wide_color = NSNumber::new_bool(output.allows_wide_color());
    if output.requires_explicit_sdr_color_properties() {
        let color_properties_key = unsafe { AVVideoColorPropertiesKey }.ok_or_else(|| {
            "this macOS runtime cannot request explicit video color properties".to_owned()
        })?;
        let primaries_key = unsafe { AVVideoColorPrimariesKey }
            .ok_or_else(|| "this macOS runtime lacks the color primaries key".to_owned())?;
        let primaries = unsafe { AVVideoColorPrimaries_ITU_R_709_2 }
            .ok_or_else(|| "this macOS runtime lacks BT.709 color primaries".to_owned())?;
        let transfer_key = unsafe { AVVideoTransferFunctionKey }
            .ok_or_else(|| "this macOS runtime lacks the color transfer key".to_owned())?;
        let transfer = unsafe { AVVideoTransferFunction_IEC_sRGB }
            .ok_or_else(|| "this macOS runtime lacks the sRGB transfer function".to_owned())?;
        let matrix_key = unsafe { AVVideoYCbCrMatrixKey }
            .ok_or_else(|| "this macOS runtime lacks the color matrix key".to_owned())?;
        let matrix = unsafe { AVVideoYCbCrMatrix_ITU_R_709_2 }
            .ok_or_else(|| "this macOS runtime lacks the BT.709 color matrix".to_owned())?;
        let color_values: [&AnyObject; 3] = [primaries, transfer, matrix];
        let color_properties =
            NSDictionary::from_slices(&[primaries_key, transfer_key, matrix_key], &color_values);
        let values: [&AnyObject; 4] = [
            format.as_ref(),
            compatible.as_ref(),
            wide_color.as_ref(),
            color_properties.as_ref(),
        ];
        Ok(NSDictionary::from_slices(
            &[format_key, metal_key, wide_color_key, color_properties_key],
            &values,
        ))
    } else {
        let values: [&AnyObject; 3] = [format.as_ref(), compatible.as_ref(), wide_color.as_ref()];
        Ok(NSDictionary::from_slices(
            &[format_key, metal_key, wide_color_key],
            &values,
        ))
    }
}

fn media_time(time: CMTime) -> MediaTime {
    let seconds = unsafe { time.seconds() };
    if seconds.is_finite() && seconds > 0.0 {
        MediaTime::from_nanos((seconds * 1_000_000_000.0).round() as u64)
    } else {
        MediaTime::ZERO
    }
}

pub(crate) struct MacImporter {
    gpu: GpuVideoContext,
    texture_cache: CFRetained<CVMetalTextureCache>,
    surfaces: BoundedSurfacePool<MacSurfaceKey, MacSurface>,
}

struct MacPlaneDescriptor {
    index: usize,
    width: u32,
    height: u32,
    metal_format: MTLPixelFormat,
    wgpu_format: wgpu::TextureFormat,
    label: &'static str,
}

impl MacImporter {
    fn new(gpu: GpuVideoContext) -> Result<Self, String> {
        use wgpu::hal::api::Metal;
        let texture_cache = unsafe {
            let hal = gpu
                .device()
                .as_hal::<Metal>()
                .ok_or_else(|| "CoreVideo import requires wgpu's Metal backend".to_string())?;
            let mut raw = std::ptr::null_mut();
            let out = NonNull::new(&mut raw as *mut *mut CVMetalTextureCache)
                .expect("address of output pointer is non-null");
            let status = CVMetalTextureCache::create(None, None, hal.raw_device(), None, out);
            if status != 0 {
                return Err(format!("CVMetalTextureCacheCreate failed with {status}"));
            }
            CFRetained::from_raw(
                NonNull::new(raw)
                    .ok_or_else(|| "CVMetalTextureCacheCreate returned a null cache".to_string())?,
            )
        };
        Ok(Self {
            gpu,
            texture_cache,
            surfaces: BoundedSurfacePool::new(MAX_IN_FLIGHT_MAC_VIDEO_SURFACES),
        })
    }

    fn allocate_surface(&self, frame: &DecodedFrame<MacFrame>) -> Result<MacSurface, String> {
        match frame.format {
            VideoFrameFormat::Packed(PackedVideoFormat::Bgra8) => {
                self.allocate_packed_surface(frame)
            }
            VideoFrameFormat::Packed(PackedVideoFormat::Rgba8) => {
                Err("CoreVideo does not expose RGBA decoder surfaces".to_owned())
            }
            VideoFrameFormat::BiPlanar420(format) => self.allocate_bi_planar_surface(frame, format),
        }
    }

    fn allocate_packed_surface(
        &self,
        frame: &DecodedFrame<MacFrame>,
    ) -> Result<MacSurface, String> {
        let width = frame.geometry.coded_width;
        let height = frame.geometry.coded_height;
        let (texture, cv_texture) = self.wrap_pixel_buffer_plane(
            &frame.lease.pixel_buffer,
            MacPlaneDescriptor {
                index: 0,
                width,
                height,
                metal_format: MTLPixelFormat::BGRA8Unorm_sRGB,
                wgpu_format: wgpu::TextureFormat::Bgra8UnormSrgb,
                label: "Neomacs packed CoreVideo surface",
            },
        )?;
        Ok(MacSurface::Packed {
            sampled: self.gpu.prepare_texture(
                texture,
                frame
                    .format
                    .allocation_bytes(frame.geometry)
                    .map_err(|error| error.to_string())?,
            ),
            _cv_texture: cv_texture,
        })
    }

    fn allocate_bi_planar_surface(
        &self,
        frame: &DecodedFrame<MacFrame>,
        format: BiPlanarVideoFormat,
    ) -> Result<MacSurface, String> {
        let plane_count = CVPixelBufferGetPlaneCount(&frame.lease.pixel_buffer);
        if plane_count != 2 {
            return Err(format!(
                "CoreVideo {:?} surface has {plane_count} planes instead of 2",
                format
            ));
        }
        if format == BiPlanarVideoFormat::P010
            && !self
                .gpu
                .device()
                .features()
                .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
        {
            return Err("P010 CoreVideo import requires wgpu TEXTURE_FORMAT_16BIT_NORM".to_owned());
        }
        let luma_width = CVPixelBufferGetWidthOfPlane(&frame.lease.pixel_buffer, 0) as u32;
        let luma_height = CVPixelBufferGetHeightOfPlane(&frame.lease.pixel_buffer, 0) as u32;
        let chroma_width = CVPixelBufferGetWidthOfPlane(&frame.lease.pixel_buffer, 1) as u32;
        let chroma_height = CVPixelBufferGetHeightOfPlane(&frame.lease.pixel_buffer, 1) as u32;
        let (luma_metal_format, chroma_metal_format, luma_wgpu_format, chroma_wgpu_format) =
            match format {
                BiPlanarVideoFormat::Nv12 => (
                    MTLPixelFormat::R8Unorm,
                    MTLPixelFormat::RG8Unorm,
                    wgpu::TextureFormat::R8Unorm,
                    wgpu::TextureFormat::Rg8Unorm,
                ),
                BiPlanarVideoFormat::P010 => (
                    MTLPixelFormat::R16Unorm,
                    MTLPixelFormat::RG16Unorm,
                    wgpu::TextureFormat::R16Unorm,
                    wgpu::TextureFormat::Rg16Unorm,
                ),
            };
        let (luma_texture, luma_cv_texture) = self.wrap_pixel_buffer_plane(
            &frame.lease.pixel_buffer,
            MacPlaneDescriptor {
                index: 0,
                width: luma_width,
                height: luma_height,
                metal_format: luma_metal_format,
                wgpu_format: luma_wgpu_format,
                label: "Neomacs CoreVideo luma plane",
            },
        )?;
        let (chroma_texture, chroma_cv_texture) = self.wrap_pixel_buffer_plane(
            &frame.lease.pixel_buffer,
            MacPlaneDescriptor {
                index: 1,
                width: chroma_width,
                height: chroma_height,
                metal_format: chroma_metal_format,
                wgpu_format: chroma_wgpu_format,
                label: "Neomacs CoreVideo chroma plane",
            },
        )?;
        Ok(MacSurface::BiPlanar {
            sampled: Box::new(self.gpu.prepare_bi_planar_textures(
                luma_texture,
                chroma_texture,
                format,
                frame.colorimetry,
                frame.geometry,
            )?),
            _luma_cv_texture: luma_cv_texture,
            _chroma_cv_texture: chroma_cv_texture,
        })
    }

    fn wrap_pixel_buffer_plane(
        &self,
        pixel_buffer: &CVPixelBuffer,
        descriptor: MacPlaneDescriptor,
    ) -> Result<(wgpu::Texture, CFRetained<CVMetalTexture>), String> {
        use wgpu::hal::api::Metal;
        let cv_texture = unsafe {
            let mut raw = std::ptr::null_mut();
            let out = NonNull::new(&mut raw as *mut *mut CVMetalTexture)
                .expect("address of output pointer is non-null");
            let status = CVMetalTextureCache::create_texture_from_image(
                None,
                &self.texture_cache,
                pixel_buffer,
                None,
                descriptor.metal_format,
                descriptor.width as usize,
                descriptor.height as usize,
                descriptor.index,
                out,
            );
            if status != 0 {
                return Err(format!(
                    "CVMetalTextureCacheCreateTextureFromImage failed for plane {} with {status}",
                    descriptor.index
                ));
            }
            CFRetained::from_raw(
                NonNull::new(raw)
                    .ok_or_else(|| "CoreVideo returned a null Metal texture".to_string())?,
            )
        };
        let metal_texture = CVMetalTextureGetTexture(&cv_texture)
            .ok_or_else(|| "CVMetalTexture has no MTLTexture".to_string())?;
        let hal_texture = unsafe {
            wgpu_hal::metal::Device::texture_from_raw(
                metal_texture,
                descriptor.wgpu_format,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::Extent3d {
                    width: descriptor.width,
                    height: descriptor.height,
                    depth_or_array_layers: 1,
                }
                .into(),
                None,
            )
        };
        let texture = unsafe {
            self.gpu.device().create_texture_from_hal::<Metal>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some(descriptor.label),
                    size: wgpu::Extent3d {
                        width: descriptor.width,
                        height: descriptor.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: descriptor.wgpu_format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::TextureUses::RESOURCE,
            )
        };
        Ok((texture, cv_texture))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MacSurfaceKey {
    pixel_buffer: usize,
    width: u32,
    height: u32,
    format: VideoFrameFormat,
    colorimetry: VideoColorimetry,
}

enum MacSurface {
    Packed {
        sampled: PreparedSampledTexture,
        _cv_texture: CFRetained<CVMetalTexture>,
    },
    BiPlanar {
        sampled: Box<PreparedBiPlanarTexture>,
        _luma_cv_texture: CFRetained<CVMetalTexture>,
        _chroma_cv_texture: CFRetained<CVMetalTexture>,
    },
}

#[derive(Clone)]
enum PreparedMacSample {
    Packed(PreparedSampledTexture),
    BiPlanar(PreparedBiPlanarTexture),
}

impl MacSurface {
    fn prepared(&self) -> PreparedMacSample {
        match self {
            Self::Packed { sampled, .. } => PreparedMacSample::Packed(sampled.clone()),
            Self::BiPlanar { sampled, .. } => PreparedMacSample::BiPlanar((**sampled).clone()),
        }
    }
}

struct MetalFrameLease {
    _frame: MacFrame,
    _surface: crate::surface_pool::SurfaceLease<MacSurfaceKey, MacSurface>,
}

unsafe impl Send for MetalFrameLease {}
unsafe impl Sync for MetalFrameLease {}

impl FrameImporter<MacFrame> for MacImporter {
    type Sampled = GpuVideoFrame;

    fn transfer_path(&self, frame: &DecodedFrame<MacFrame>) -> VideoTransferPath {
        frame
            .decoder_transfer
            .path()
            .expect("AVFoundation completes its transfer before frame publication")
    }

    fn import(
        &mut self,
        frame: DecodedFrame<MacFrame>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        let width = frame.geometry.coded_width;
        let height = frame.geometry.coded_height;
        let path = self.transfer_path(&frame);
        let key = MacSurfaceKey {
            pixel_buffer: (&*frame.lease.pixel_buffer as *const CVPixelBuffer) as usize,
            width,
            height,
            format: frame.format,
            colorimetry: frame.colorimetry,
        };
        let surface = match self.surfaces.acquire(key) {
            SurfacePoolAcquire::Reused(surface) => surface,
            SurfacePoolAcquire::Allocate(reservation) => {
                let surface = match self.allocate_surface(&frame) {
                    Ok(surface) => surface,
                    Err(reason) if matches!(frame.format, VideoFrameFormat::BiPlanar420(_)) => {
                        return Ok(FrameImportOutcome::ReconfigureDecoder {
                            rejected: frame.format,
                            reason,
                        });
                    }
                    Err(reason) => return Err(reason),
                };
                reservation.fulfill(surface)
            }
            SurfacePoolAcquire::Backpressured => {
                return Ok(FrameImportOutcome::Backpressured);
            }
        };
        let geometry = frame.geometry;
        let prepared = surface.value().prepared();
        let transfer = frame
            .decoder_transfer
            .completed()
            .expect("AVFoundation completes its transfer before frame publication");
        let lease = MetalFrameLease {
            _frame: frame.lease,
            _surface: surface,
        };
        let sampled = match prepared {
            PreparedMacSample::Packed(prepared) => {
                self.gpu.wrap_prepared_texture(geometry, prepared, lease)
            }
            PreparedMacSample::BiPlanar(prepared) => self
                .gpu
                .wrap_prepared_bi_planar_texture(geometry, prepared, lease),
        };
        debug_assert_eq!(transfer.path(), path);
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled,
            transfer,
        }))
    }
}

impl Platform for MacPlatform {
    const BACKEND: VideoDecodeBackend = VideoDecodeBackend::AvFoundation;
    type Frame = MacFrame;
    type Sampled = GpuVideoFrame;
    type Decoder = MacDecoder;
    type Importer = MacImporter;
}

impl ProductionPlatform for MacPlatform {
    fn create(
        gpu: GpuVideoContext,
        policy: crate::FrameTransferPolicy,
        wake: VideoWake,
    ) -> Result<(Self::Decoder, Self::Importer), VideoInitError> {
        require_fixed_transfer_path(
            VideoDecodeBackend::AvFoundation,
            policy,
            VideoTransferPath::GpuInteropCopy,
        )?;
        let supports_p010 = gpu
            .device()
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM);
        let importer = MacImporter::new(gpu).map_err(|message| VideoInitError::Backend {
            backend: VideoDecodeBackend::AvFoundation,
            message,
        })?;
        let decoder =
            MacDecoder::new(supports_p010, wake).map_err(|message| VideoInitError::Backend {
                backend: VideoDecodeBackend::AvFoundation,
                message,
            })?;
        Ok((decoder, importer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_depth_and_range_select_the_matching_core_video_surface() {
        let output = select_mac_output_format(
            true,
            MacSourceMetadata {
                bits_per_component: Some(10),
                full_range: true,
            },
        );

        assert_eq!(
            output.frame_format(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)
        );
        assert_eq!(output.range, VideoColorRange::Full);
        assert_eq!(
            output.core_video_pixel_format(),
            kCVPixelFormatType_420YpCbCr10BiPlanarFullRange
        );
    }

    #[test]
    fn unsupported_ten_bit_sampling_selects_an_explicit_conversion() {
        let output = select_mac_output_format(
            false,
            MacSourceMetadata {
                bits_per_component: Some(10),
                full_range: false,
            },
        );

        assert_eq!(
            output.frame_format(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
        );
        assert_eq!(output.range, VideoColorRange::Limited);
        assert_eq!(
            output.completed_transfer(),
            CompletedFrameTransfer::GpuInteropCopy {
                reported_bytes: None
            }
        );
    }

    #[test]
    fn ordinary_eight_bit_video_keeps_the_native_nv12_surface() {
        let output = select_mac_output_format(
            true,
            MacSourceMetadata {
                bits_per_component: Some(8),
                full_range: false,
            },
        );

        assert_eq!(
            output.frame_format(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
        );
        assert_eq!(
            output.completed_transfer(),
            CompletedFrameTransfer::GpuInteropCopy {
                reported_bytes: None
            }
        );
    }

    #[test]
    fn unknown_source_depth_is_not_reported_as_proven_direct_import() {
        let output = select_mac_output_format(
            true,
            MacSourceMetadata {
                bits_per_component: None,
                full_range: false,
            },
        );

        assert_eq!(
            output.frame_format(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
        );
        assert_eq!(
            output.completed_transfer(),
            CompletedFrameTransfer::GpuInteropCopy {
                reported_bytes: None
            }
        );
    }

    #[test]
    fn twelve_bit_source_to_p010_is_reported_as_conversion() {
        let output = select_mac_output_format(
            true,
            MacSourceMetadata {
                bits_per_component: Some(12),
                full_range: false,
            },
        );

        assert_eq!(
            output.frame_format(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)
        );
        assert_eq!(
            output.completed_transfer(),
            CompletedFrameTransfer::GpuInteropCopy {
                reported_bytes: None
            }
        );
    }

    #[test]
    fn macos_output_fallback_is_p010_then_nv12_then_bgra() {
        let p010 = select_mac_output_format(
            true,
            MacSourceMetadata {
                bits_per_component: Some(10),
                full_range: true,
            },
        );
        let nv12 = p010
            .fallback_after_rejection(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010))
            .unwrap();
        let bgra = nv12
            .fallback_after_rejection(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12))
            .unwrap();

        assert_eq!(
            nv12.frame_format(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
        );
        assert_eq!(nv12.range, VideoColorRange::Full);
        assert_eq!(
            bgra.frame_format(),
            VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
        );
        assert_eq!(
            bgra.fallback_after_rejection(VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)),
            None
        );
        assert_eq!(
            p010.fallback_after_rejection(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12))
                .unwrap()
                .frame_format(),
            VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
        );
        assert!(p010.allows_wide_color());
        assert!(nv12.allows_wide_color());
        assert!(!bgra.allows_wide_color());
        assert!(!p010.requires_explicit_sdr_color_properties());
        assert!(!nv12.requires_explicit_sdr_color_properties());
        assert!(bgra.requires_explicit_sdr_color_properties());
    }
}
