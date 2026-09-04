use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU32;
use std::path::Path;

use crate::Frontend;
use serde::{Deserialize, Serialize};

/// Non-zero pixel dimensions of the video presentation exercised by the
/// physical-display benchmark.
///
/// A native-display GUI cannot be sized hermetically by the off-screen GUI
/// adapter.  The benchmark therefore treats the GUI dimensions as its video
/// presentation contract and verifies that the real window can contain it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeVideoPresentationTarget {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl NativeVideoPresentationTarget {
    pub(crate) fn from_frontend(frontend: Frontend) -> Result<Self, InvalidPresentationTarget> {
        let Frontend::Gui { width, height } = frontend else {
            return Err(InvalidPresentationTarget::NotGui);
        };
        Ok(Self {
            width: NonZeroU32::new(width).ok_or(InvalidPresentationTarget::ZeroWidth)?,
            height: NonZeroU32::new(height).ok_or(InvalidPresentationTarget::ZeroHeight)?,
        })
    }

    pub(crate) const fn width(self) -> u32 {
        self.width.get()
    }

    pub(crate) const fn height(self) -> u32 {
        self.height.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InvalidPresentationTarget {
    NotGui,
    ZeroWidth,
    ZeroHeight,
}

impl fmt::Display for InvalidPresentationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotGui => "sustained native-video performance requires the GUI frontend",
            Self::ZeroWidth => "native-video presentation width must be non-zero",
            Self::ZeroHeight => "native-video presentation height must be non-zero",
        })
    }
}

impl std::error::Error for InvalidPresentationTarget {}

/// Optimized build profiles accepted by the physical-GPU acceptance test.
/// Unknown/custom profiles are rejected because their optimization settings
/// cannot be inferred safely from a name.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeVideoBuildProfile {
    Release,
    Profiling,
    ReleasePgo,
    ReleasePgoProfiling,
}

impl NativeVideoBuildProfile {
    pub(crate) fn from_version(version: &str) -> Result<Self, String> {
        let profile = version
            .lines()
            .find_map(|line| line.strip_prefix("Build: "))
            .and_then(|line| line.split_whitespace().next())
            .ok_or_else(|| "Neomacs --version did not report a build profile".to_owned())?;
        match profile {
            "release" => Ok(Self::Release),
            "profiling" => Ok(Self::Profiling),
            "release-pgo" => Ok(Self::ReleasePgo),
            "release-pgo-profiling" => Ok(Self::ReleasePgoProfiling),
            profile => Err(format!(
                "sustained native-video performance requires an optimized release profile, got {profile:?}"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeVideoFrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl NativeVideoFrameRate {
    pub(crate) fn is_60_hz_class(self) -> bool {
        matches!(
            (self.numerator, self.denominator),
            (60, 1) | (60_000, 1_001)
        )
    }
}

/// Metadata discovered from the exact compressed input before launching the
/// editor. GStreamer's typed discoverer API avoids parsing a human CLI format.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeVideoMediaMetadata {
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub frame_rate: NativeVideoFrameRate,
    pub codec_caps: String,
}

/// Content and execution inputs that must be identical across every child of
/// one native-video comparison. Editor identity is intentionally excluded:
/// comparing editors is the purpose of the benchmark.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeVideoComparisonIdentity {
    pub workload_fixture_sha256: String,
    pub video_file_sha256: String,
    pub video_file_size_bytes: u64,
    pub media: NativeVideoMediaMetadata,
    pub presentation_width_pixels: u32,
    pub presentation_height_pixels: u32,
    pub display_environment: BTreeMap<String, String>,
    pub gstreamer_environment: BTreeMap<String, String>,
    pub gpu_frame_timing: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVideoDecoderKind {
    Hardware,
    Software,
    Unknown,
}

impl fmt::Display for NativeVideoDecoderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Hardware => "hardware",
            Self::Software => "software",
            Self::Unknown => "unknown",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVideoGraphicsBackend {
    Vulkan,
    Metal,
    Dx12,
    Gl,
    BrowserWebgpu,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVideoFrameFormat {
    Nv12,
    P010,
    Rgba8,
    Bgra8,
}

impl fmt::Display for NativeVideoFrameFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Nv12 => "nv12",
            Self::P010 => "p010",
            Self::Rgba8 => "rgba8",
            Self::Bgra8 => "bgra8",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVideoGpuTimingStatus {
    Disabled,
    Unsupported,
    Enabled,
}

impl fmt::Display for NativeVideoGraphicsBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::Dx12 => "dx12",
            Self::Gl => "gl",
            Self::BrowserWebgpu => "browser-webgpu",
            Self::Other => "other",
        })
    }
}

/// Runtime path identity that must remain fixed across every sample in a
/// comparison. Content identity alone is insufficient: changing decoder,
/// adapter, driver, render node, or display rate changes the experiment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeVideoExecutionIdentity {
    pub decoder_factory: String,
    pub decoder_plugin: String,
    pub decoder_kind: NativeVideoDecoderKind,
    pub gpu_adapter_name: String,
    pub gpu_vendor: u32,
    pub gpu_device: u32,
    pub gpu_device_type: String,
    pub graphics_backend: NativeVideoGraphicsBackend,
    pub gpu_driver: String,
    pub gpu_driver_info: String,
    pub drm_render_node: Option<String>,
    pub display_refresh_hz: Option<u16>,
    pub frame_format: NativeVideoFrameFormat,
    pub gpu_timing_status: NativeVideoGpuTimingStatus,
}

impl NativeVideoMediaMetadata {
    pub(crate) fn validate_4k60(&self) -> Result<(), String> {
        if (self.width_pixels, self.height_pixels) != (3840, 2160) {
            return Err(format!(
                "sustained native-video performance requires 3840x2160 input, got {}x{}",
                self.width_pixels, self.height_pixels
            ));
        }
        if !self.frame_rate.is_60_hz_class() {
            return Err(format!(
                "sustained native-video performance requires 60 fps-class input, got {}/{} fps",
                self.frame_rate.numerator, self.frame_rate.denominator
            ));
        }
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "native-video"))]
pub(crate) fn discover_media_metadata(path: &Path) -> Result<NativeVideoMediaMetadata, String> {
    use gstreamer_pbutils::prelude::*;

    gstreamer::init().map_err(|error| format!("failed to initialize GStreamer: {error}"))?;
    let uri = gstreamer::glib::filename_to_uri(path, None)
        .map_err(|error| format!("failed to form native-video input URI: {error}"))?;
    let discoverer = gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(10))
        .map_err(|error| format!("failed to create GStreamer discoverer: {error}"))?;
    let discovered = discoverer
        .discover_uri(uri.as_str())
        .map_err(|error| format!("failed to inspect native-video input: {error}"))?;
    let mut videos: Vec<_> = discovered
        .video_streams()
        .into_iter()
        .filter(|video| !video.is_image())
        .collect();
    if videos.len() != 1 {
        return Err(format!(
            "sustained native-video input must contain exactly one video stream, found {}",
            videos.len()
        ));
    }
    let video = videos.pop().expect("the single video stream exists");
    let frame_rate = video.framerate();
    let numerator = u32::try_from(frame_rate.numer())
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| "native-video input has no positive frame-rate numerator".to_owned())?;
    let denominator = u32::try_from(frame_rate.denom())
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| "native-video input has no positive frame-rate denominator".to_owned())?;
    let codec_caps = video
        .caps()
        .map(|caps| caps.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    Ok(NativeVideoMediaMetadata {
        width_pixels: video.width(),
        height_pixels: video.height(),
        frame_rate: NativeVideoFrameRate {
            numerator,
            denominator,
        },
        codec_caps,
    })
}

#[cfg(all(target_os = "linux", not(feature = "native-video")))]
pub(crate) fn discover_media_metadata(_path: &Path) -> Result<NativeVideoMediaMetadata, String> {
    Err(
        "sustained native-video media discovery requires the native-video \
         feature; rebuild the harness with \
         `cargo xtask --features perf-native-video perf ...`"
            .to_owned(),
    )
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn discover_media_metadata(_path: &Path) -> Result<NativeVideoMediaMetadata, String> {
    Err("sustained native-video media discovery is currently Linux-only".to_owned())
}

#[cfg(test)]
#[path = "native_video_test.rs"]
mod tests;
