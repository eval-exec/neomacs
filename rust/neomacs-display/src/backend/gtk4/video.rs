//! GStreamer video playback integration for GTK4 backend.
//!
//! Uses gtk4paintablesink from gst-plugins-rs for true zero-copy video
//! rendering via DMA-BUF when running on Wayland with VA-API hardware.

#[cfg(feature = "video")]
use std::cell::RefCell;
#[cfg(feature = "video")]
use std::collections::HashMap;
#[cfg(feature = "video")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "video")]
use std::sync::atomic::{AtomicBool, Ordering};

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
use gtk4::prelude::{TextureExt, TextureExtManual, PaintableExt, WidgetExt, WidgetExtManual};

// Thread-local widget reference for video frame invalidation callbacks
#[cfg(feature = "video")]
thread_local! {
    static VIDEO_WIDGET: RefCell<Option<gtk4::Widget>> = const { RefCell::new(None) };
}

// Flag to indicate new frame is available (set by GStreamer thread via invalidate-contents signal,
// read by tick callback on main thread)
#[cfg(feature = "video")]
static FRAME_PENDING: AtomicBool = AtomicBool::new(false);

/// Set the widget for video frame invalidation callbacks
#[cfg(feature = "video")]
pub fn set_video_widget(widget: Option<gtk4::Widget>) {
    VIDEO_WIDGET.with(|w| {
        // Set new widget
        *w.borrow_mut() = widget.clone();
        
        // Add tick callback that only redraws when a new frame is pending
        // This is more efficient than a fixed 16ms timer - it only redraws
        // when the GStreamer paintable signals a new frame via invalidate-contents
        if let Some(ref w) = widget {
            // Use add_tick_callback for vsync-synchronized updates
            // This runs once per display refresh, checking if a new frame is ready
            w.add_tick_callback(|widget, _frame_clock| {
                // Only redraw if a new frame is pending (set by invalidate-contents signal)
                if FRAME_PENDING.swap(false, Ordering::Relaxed) {
                    widget.queue_draw();
                }
                glib::ControlFlow::Continue
            });
        }
    });
}

/// Get the widget for video frame invalidation callbacks
#[cfg(feature = "video")]
pub fn get_video_widget() -> Option<gtk4::Widget> {
    VIDEO_WIDGET.with(|w| w.borrow().clone())
}

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

    /// Loop playback (-1 = infinite, 0 = no loop, n > 0 = loop n times)
    pub loop_count: i32,
    
    /// Remaining loops (decremented on each loop completion)
    pub loops_remaining: i32,

    /// Volume (0.0 - 1.0)
    pub volume: f64,

    /// Whether hardware decoding is active (VA-API)
    pub hw_accel: bool,

    /// Whether DMA-BUF zero-copy is being used
    pub use_dmabuf: bool,
    
    /// Frame counter for FPS tracking
    frame_count: u64,
    
    /// Last FPS report time
    last_fps_time: std::time::Instant,
    
    /// Last reported FPS
    pub fps: f32,
    
    /// Video ID for logging
    pub video_id: u32,
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

        let player = Self {
            pipeline,
            gtk4sink,
            width: 0,
            height: 0,
            state: VideoState::Stopped,
            duration_ns: None,
            position_ns: 0,
            loop_count: 0,
            loops_remaining: 0,
            volume: 1.0,
            hw_accel,
            use_dmabuf: true, // gtk4paintablesink handles this automatically
            frame_count: 0,
            last_fps_time: std::time::Instant::now(),
            fps: 0.0,
            video_id: 0,  // Set later by cache
        };

        // Connect paintable's invalidate-contents signal to trigger widget redraw
        player.connect_invalidate_signal();

        Ok(player)
    }

    /// Connect paintable's invalidate-contents signal to trigger widget redraw
    ///
    /// This is essential for video playback: when gtk4paintablesink produces a new
    /// frame, it emits invalidate-contents on the paintable. We need to listen for
    /// this and queue a redraw on the Emacs widget.
    ///
    /// The signal fires from GStreamer's streaming thread, so we just set an atomic
    /// flag. The tick callback (running on the main thread via frame clock) checks
    /// this flag and queues the redraw.
    fn connect_invalidate_signal(&self) {
        if let Some(paintable) = self.get_paintable() {
            paintable.connect_invalidate_contents(move |_paintable| {
                // Set flag - the tick callback will read this and queue_draw
                FRAME_PENDING.store(true, Ordering::Relaxed);
            });
        } else {
        }
    }

    /// Get the GdkPaintable from the sink for rendering
    ///
    /// This returns a GdkPaintable that can be snapshotted directly into
    /// the GTK4 render tree. The paintable is backed by DMA-BUF when
    /// zero-copy is active.
    pub fn get_paintable(&self) -> Option<gdk::Paintable> {
        self.gtk4sink.property::<Option<gdk::Paintable>>("paintable")
    }
    
    /// Count a rendered frame and calculate FPS
    pub fn count_frame(&mut self) {
        self.frame_count += 1;
        let elapsed = self.last_fps_time.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            eprintln!("[Video {}] FPS: {:.1}", self.video_id, self.fps);
            self.frame_count = 0;
            self.last_fps_time = std::time::Instant::now();
        }
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
        let ret = self.pipeline.set_state(gst::State::Playing)
            .map_err(|e| DisplayError::Backend(format!("Failed to play: {:?}", e)))?;
        
        // If state change is async, wait for it to complete (up to 5 seconds)
        if ret == gst::StateChangeSuccess::Async {
            let (_ret2, _current, _pending) = self.pipeline.state(gst::ClockTime::from_seconds(5));
        }
        
        // Re-connect signal after state change in case paintable changed
        self.connect_invalidate_signal();
        
        // Query actual state
        let (_, _current, _) = self.pipeline.state(gst::ClockTime::NONE);
        
        // Check paintable dimensions
        if let Some(paintable) = self.get_paintable() {
            let _w = paintable.intrinsic_width();
            let _h = paintable.intrinsic_height();
        }
        
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

    /// Update video state - called once per frame for all videos
    pub fn update(&mut self) {
        // Only query position occasionally to reduce IPC overhead
        // The paintable handles frame updates automatically
        
        // Check for end of stream - limit to a few messages per update
        if let Some(bus) = self.pipeline.bus() {
            for _ in 0..5 {  // Process at most 5 messages per update
                if let Some(msg) = bus.pop() {
                    match msg.view() {
                        gst::MessageView::Eos(_) => {
                            // Check if we should loop
                            let should_loop = self.loop_count < 0 ||  // infinite
                                self.loops_remaining > 0;  // still have loops
                            
                            if should_loop {
                                if self.loop_count > 0 {
                                    self.loops_remaining -= 1;
                                }
                                let _ = self.seek(0);
                                // Also need to restart playback after seek
                                let _ = self.pipeline.set_state(gst::State::Playing);
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
                } else {
                    break;  // No more messages
                }
            }
        }
    }
    
    /// Set loop mode
    /// count: -1 = infinite loop, 0 = no loop, n > 0 = loop n times
    pub fn set_looping(&mut self, count: i32) {
        self.loop_count = count;
        self.loops_remaining = if count > 0 { count } else { 0 };
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
        let mut player = GpuVideoPlayer::new(uri)?;
        let id = self.next_id;
        player.video_id = id;  // Set video ID for FPS logging
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
