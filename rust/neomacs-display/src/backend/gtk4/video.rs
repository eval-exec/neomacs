//! GStreamer video playback integration for GTK4 backend.
//!
//! Uses gtk4paintablesink from gst-plugins-rs for true zero-copy video
//! rendering via DMA-BUF when running on Wayland with VA-API hardware.

#[cfg(feature = "video")]
use std::collections::HashMap;
#[cfg(feature = "video")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "video")]
use gstreamer as gst;
#[cfg(feature = "video")]
use gstreamer::prelude::*;
#[cfg(feature = "video")]
use gtk4::cairo;
#[cfg(feature = "video")]
use gtk4::gdk;
#[cfg(feature = "video")]
use gtk4::glib;
#[cfg(feature = "video")]
use gtk4::prelude::{TextureExt, TextureExtManual, PaintableExt};

use crate::core::error::{DisplayError, DisplayResult};

/// Video playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    /// Video is not playing
    Stopped,
    /// Video is playing
    Playing,
    /// Video is paused
    Paused,
    /// Video loading/buffering
    Buffering,
    /// Error state
    Error,
}

// =============================================================================
// GPU-accelerated Video Player with DMA-BUF zero-copy
// =============================================================================

/// DMA-BUF frame data for zero-copy GPU rendering
#[cfg(feature = "video")]
pub struct DmaBufFrame {
    /// DMA-BUF file descriptor
    pub fd: i32,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// DRM format fourcc
    pub fourcc: u32,
    /// Stride (bytes per row)
    pub stride: u32,
    /// DRM modifier
    pub modifier: u64,
    /// Offset into buffer
    pub offset: u32,
}

/// GPU-accelerated video player using gtk4paintablesink for DMA-BUF zero-copy
///
/// Uses the gst-plugins-rs gtk4paintablesink which handles all DMA-BUF/GL/VideoMeta
/// negotiation internally. This provides true zero-copy video playback when:
/// - Running on Wayland with supported hardware (Intel/AMD VA-API)
/// - GTK4 4.14+ with DMA-BUF import support
#[cfg(feature = "video")]
pub struct GpuVideoPlayer {
    /// GStreamer pipeline
    pipeline: gst::Pipeline,

    /// The gtk4paintablesink element
    gtk4sink: gst::Element,

    /// Video dimensions
    pub width: i32,
    pub height: i32,

    /// Current state
    pub state: VideoState,

    /// Duration in nanoseconds
    pub duration_ns: Option<i64>,

    /// Current position in nanoseconds
    pub position_ns: i64,

    /// Loop playback
    pub looping: bool,

    /// Volume (0.0 - 1.0)
    pub volume: f64,

    /// Whether hardware decoding is active (VA-API)
    pub hw_accel: bool,

    /// Whether DMA-BUF zero-copy is being used
    pub use_dmabuf: bool,
}

#[cfg(feature = "video")]
impl GpuVideoPlayer {
    /// Create a new GPU-accelerated video player using gtk4paintablesink
    ///
    /// This uses gtk4paintablesink from gst-plugins-rs which handles all DMA-BUF/GL
    /// negotiation internally. When running on Wayland with VA-API, this provides
    /// true zero-copy video rendering.
    pub fn new(uri: &str) -> DisplayResult<Self> {
        gst::init()
            .map_err(|e| DisplayError::Backend(format!("Failed to init GStreamer: {}", e)))?;

        // Try to create gtk4paintablesink (from gst-plugins-rs)
        // This sink handles DMA-BUF/GL/VideoMeta negotiation internally
        let gtk4sink = gst::ElementFactory::make("gtk4paintablesink")
            .build()
            .map_err(|e| DisplayError::Backend(format!(
                "Failed to create gtk4paintablesink: {}. Make sure gst-plugins-rs is installed.", e
            )))?;

        // Create playbin - it auto-selects VA-API decoders when available
        let playbin = gst::ElementFactory::make("playbin")
            .name("playbin")
            .property("uri", uri)
            .property("video-sink", &gtk4sink)
            .build()
            .map_err(|e| DisplayError::Backend(format!("Failed to create playbin: {}", e)))?;

        // Get pipeline
        let pipeline: gst::Pipeline = playbin.downcast()
            .map_err(|_| DisplayError::Backend("Failed to downcast to pipeline".into()))?;

        // Check if VA-API decoders are available (indicates hw accel potential)
        let hw_accel = gst::ElementFactory::find("vah264dec").is_some()
            || gst::ElementFactory::find("vaapidecodebin").is_some();

        eprintln!("[GpuVideoPlayer] Created with gtk4paintablesink (DMA-BUF zero-copy enabled)");
        eprintln!("[GpuVideoPlayer] VA-API hardware decoding: {}", if hw_accel { "available" } else { "not available" });

        Ok(Self {
            pipeline,
            gtk4sink,
            width: 0,
            height: 0,
            state: VideoState::Stopped,
            duration_ns: None,
            position_ns: 0,
            looping: false,
            volume: 1.0,
            hw_accel,
            use_dmabuf: true, // gtk4paintablesink handles this automatically
        })
    }

    /// Get the GdkPaintable from the sink for rendering
    ///
    /// This returns a GdkPaintable that can be snapshotted directly into
    /// the GTK4 render tree. The paintable is backed by DMA-BUF when
    /// zero-copy is active.
    pub fn get_paintable(&self) -> Option<gdk::Paintable> {
        self.gtk4sink.property::<Option<gdk::Paintable>>("paintable")
    }

    /// Get current frame as GDK texture
    ///
    /// This snapshots the current paintable to a texture. For most rendering
    /// use cases, prefer using get_paintable() directly for better performance.
    pub fn get_frame_texture(&self) -> Option<gdk::Texture> {
        let paintable = self.get_paintable()?;
        let width = paintable.intrinsic_width();
        let height = paintable.intrinsic_height();

        if width <= 0 || height <= 0 {
            return None;
        }

        // Get the current image as a paintable (may be the same or a snapshot)
        let image = paintable.current_image();

        // Try to downcast to Texture if it's already a texture
        if let Ok(texture) = image.downcast::<gdk::Texture>() {
            return Some(texture);
        }

        // Otherwise snapshot the paintable to create a texture
        // This requires a realized widget/renderer which we may not have
        // For now, return None - callers should use get_paintable() for rendering
        None
    }

    /// Get current frame as Cairo surface (downloads from GPU, fallback path)
    pub fn get_frame(&self) -> Option<cairo::ImageSurface> {
        let texture = self.get_frame_texture()?;
        let width = texture.width();
        let height = texture.height();

        // Download texture to Cairo surface
        let mut surface = cairo::ImageSurface::create(
            cairo::Format::ARgb32,
            width,
            height
        ).ok()?;

        // Use texture.download() to get pixel data
        let stride = surface.stride() as usize;
        {
            let mut data = surface.data().ok()?;
            texture.download(&mut data[..], stride);
        }

        Some(surface)
    }

    /// Play the video
    pub fn play(&mut self) -> DisplayResult<()> {
        self.pipeline.set_state(gst::State::Playing)
            .map_err(|e| DisplayError::Backend(format!("Failed to play: {:?}", e)))?;
        self.state = VideoState::Playing;
        Ok(())
    }

    /// Pause the video
    pub fn pause(&mut self) -> DisplayResult<()> {
        self.pipeline.set_state(gst::State::Paused)
            .map_err(|e| DisplayError::Backend(format!("Failed to pause: {:?}", e)))?;
        self.state = VideoState::Paused;
        Ok(())
    }

    /// Stop the video
    pub fn stop(&mut self) -> DisplayResult<()> {
        self.pipeline.set_state(gst::State::Ready)
            .map_err(|e| DisplayError::Backend(format!("Failed to stop: {:?}", e)))?;
        self.state = VideoState::Stopped;
        Ok(())
    }

    /// Seek to position in nanoseconds
    pub fn seek(&mut self, position_ns: i64) -> DisplayResult<()> {
        self.pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_nseconds(position_ns as u64),
        ).map_err(|e| DisplayError::Backend(format!("Failed to seek: {:?}", e)))?;
        Ok(())
    }

    /// Update video state
    pub fn update(&mut self) {
        if let Some(position) = self.pipeline.query_position::<gst::ClockTime>() {
            self.position_ns = position.nseconds() as i64;
        }

        if self.duration_ns.is_none() {
            if let Some(duration) = self.pipeline.query_duration::<gst::ClockTime>() {
                self.duration_ns = Some(duration.nseconds() as i64);
            }
        }

        // Check for end of stream
        if let Some(bus) = self.pipeline.bus() {
            while let Some(msg) = bus.pop() {
                match msg.view() {
                    gst::MessageView::Eos(_) => {
                        if self.looping {
                            let _ = self.seek(0);
                        } else {
                            self.state = VideoState::Stopped;
                        }
                    }
                    gst::MessageView::Error(err) => {
                        eprintln!("[GpuVideoPlayer] GStreamer error: {:?}", err);
                        self.state = VideoState::Error;
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(feature = "video")]
impl Drop for GpuVideoPlayer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Create Cairo surface from raw BGRA pixel data (called on main thread)
#[cfg(feature = "video")]
fn create_surface_from_raw(
    data: &[u8],
    width: i32,
    height: i32,
) -> DisplayResult<cairo::ImageSurface> {
    let stride = width * 4; // BGRA = 4 bytes per pixel

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| DisplayError::Backend(format!("Failed to create surface: {}", e)))?;

    // Get surface stride before borrowing data
    let surface_stride = surface.stride() as usize;

    {
        let mut surface_data = surface.data()
            .map_err(|e| DisplayError::Backend(format!("Failed to get surface data: {}", e)))?;

        // Copy data row by row
        for y in 0..height as usize {
            let src_offset = y * stride as usize;
            let dst_offset = y * surface_stride;

            if src_offset + (width as usize * 4) <= data.len() {
                let src_row = &data[src_offset..src_offset + width as usize * 4];
                let dst_row = &mut surface_data[dst_offset..dst_offset + width as usize * 4];
                dst_row.copy_from_slice(src_row);
            }
        }
    }

    surface.mark_dirty();
    Ok(surface)
}

/// Video cache for managing multiple video players (uses GPU acceleration when available)
#[cfg(feature = "video")]
#[derive(Default)]
pub struct VideoCache {
    players: HashMap<u32, GpuVideoPlayer>,
    next_id: u32,
}

#[cfg(feature = "video")]
impl VideoCache {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            next_id: 1,
        }
    }

    /// Load a video from URI
    pub fn load(&mut self, uri: &str) -> DisplayResult<u32> {
        let player = GpuVideoPlayer::new(uri)?;
        let id = self.next_id;
        self.next_id += 1;
        self.players.insert(id, player);
        Ok(id)
    }

    /// Get a video player
    pub fn get(&self, id: u32) -> Option<&GpuVideoPlayer> {
        self.players.get(&id)
    }

    /// Get a video player mutably
    pub fn get_mut(&mut self, id: u32) -> Option<&mut GpuVideoPlayer> {
        self.players.get_mut(&id)
    }

    /// Remove a video player
    pub fn remove(&mut self, id: u32) -> bool {
        self.players.remove(&id).is_some()
    }

    /// Update all video players
    pub fn update_all(&mut self) {
        for player in self.players.values_mut() {
            player.update();
        }
    }

    /// Get number of loaded videos
    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

/// GPU-accelerated video cache for managing multiple GPU video players
#[cfg(feature = "video")]
#[derive(Default)]
pub struct GpuVideoCache {
    players: HashMap<u32, GpuVideoPlayer>,
    next_id: u32,
}

#[cfg(feature = "video")]
impl GpuVideoCache {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            next_id: 1,
        }
    }

    /// Load a video with GPU acceleration
    pub fn load(&mut self, uri: &str) -> DisplayResult<u32> {
        let player = GpuVideoPlayer::new(uri)?;
        let id = self.next_id;
        self.next_id += 1;
        self.players.insert(id, player);
        Ok(id)
    }

    /// Get a video player by ID
    pub fn get(&self, id: u32) -> Option<&GpuVideoPlayer> {
        self.players.get(&id)
    }

    /// Get a mutable video player by ID
    pub fn get_mut(&mut self, id: u32) -> Option<&mut GpuVideoPlayer> {
        self.players.get_mut(&id)
    }

    /// Remove a video player
    pub fn remove(&mut self, id: u32) -> bool {
        self.players.remove(&id).is_some()
    }

    /// Update all video players
    pub fn update_all(&mut self) {
        for player in self.players.values_mut() {
            player.update();
        }
    }

    /// Get number of loaded videos
    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

// Stub implementation when video feature is disabled
#[cfg(not(feature = "video"))]
pub struct GpuVideoCache;

#[cfg(not(feature = "video"))]
impl GpuVideoCache {
    pub fn new() -> Self { Self }
    pub fn load(&mut self, _uri: &str) -> DisplayResult<u32> {
        Err(DisplayError::Backend("Video support not compiled".into()))
    }
    pub fn get(&self, _id: u32) -> Option<&()> { None }
    pub fn get_mut(&mut self, _id: u32) -> Option<&mut ()> { None }
    pub fn remove(&mut self, _id: u32) -> bool { false }
    pub fn update_all(&mut self) {}
    pub fn len(&self) -> usize { 0 }
    pub fn is_empty(&self) -> bool { true }
}

#[cfg(not(feature = "video"))]
impl Default for GpuVideoCache {
    fn default() -> Self { Self::new() }
}

// Stub implementation when video feature is disabled
#[cfg(not(feature = "video"))]
pub struct VideoCache;

#[cfg(not(feature = "video"))]
impl VideoCache {
    pub fn new() -> Self { Self }
    pub fn load(&mut self, _uri: &str) -> DisplayResult<u32> {
        Err(DisplayError::Backend("Video support not compiled".into()))
    }
    pub fn get(&self, _id: u32) -> Option<&()> { None }
    pub fn get_mut(&mut self, _id: u32) -> Option<&mut ()> { None }
    pub fn remove(&mut self, _id: u32) -> bool { false }
    pub fn update_all(&mut self) {}
    pub fn len(&self) -> usize { 0 }
    pub fn is_empty(&self) -> bool { true }
}

#[cfg(not(feature = "video"))]
impl Default for VideoCache {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_cache_creation() {
        let cache = VideoCache::new();
        assert!(cache.is_empty());
    }
}
