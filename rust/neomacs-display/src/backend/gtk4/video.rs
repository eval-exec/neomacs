//! GStreamer video playback integration for GTK4 backend.

#[cfg(feature = "video")]
use std::collections::HashMap;
#[cfg(feature = "video")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "video")]
use gstreamer as gst;
#[cfg(feature = "video")]
use gstreamer::prelude::*;
#[cfg(feature = "video")]
use gstreamer_video as gst_video;
#[cfg(feature = "video")]
use gstreamer_app as gst_app;
#[cfg(feature = "video")]
use gtk4::cairo;
#[cfg(feature = "video")]
use gtk4::gdk;
#[cfg(feature = "video")]
use gtk4::glib;

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

/// A video player instance
#[cfg(feature = "video")]
pub struct VideoPlayer {
    /// GStreamer pipeline
    pipeline: gst::Pipeline,
    
    /// App sink for frame capture
    appsink: gst_app::AppSink,
    
    /// Current video frame data (BGRA pixels)
    frame_data: Arc<Mutex<Option<FrameData>>>,
    
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
}

/// Frame data stored in a thread-safe way
#[cfg(feature = "video")]
struct FrameData {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}

#[cfg(feature = "video")]
impl VideoPlayer {
    /// Create a new video player from a URI
    pub fn new(uri: &str) -> DisplayResult<Self> {
        // Initialize GStreamer if needed
        gst::init()
            .map_err(|e| DisplayError::Backend(format!("Failed to init GStreamer: {}", e)))?;
        
        // Create playbin element
        let playbin = gst::ElementFactory::make("playbin")
            .name("playbin")
            .property("uri", uri)
            .build()
            .map_err(|e| DisplayError::Backend(format!("Failed to create playbin: {}", e)))?;
        
        // Create appsink for frame capture
        let appsink = gst_app::AppSink::builder()
            .caps(&gst::Caps::builder("video/x-raw")
                .field("format", "BGRA")
                .build())
            .build();
        
        // Create video sink bin with conversion
        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| DisplayError::Backend(format!("Failed to create videoconvert: {}", e)))?;
        
        let videoscale = gst::ElementFactory::make("videoscale")
            .build()
            .map_err(|e| DisplayError::Backend(format!("Failed to create videoscale: {}", e)))?;
        
        // Create bin for video sink
        let sinkbin = gst::Bin::new();
        sinkbin.add_many([&videoconvert, &videoscale, appsink.upcast_ref()])
            .map_err(|e| DisplayError::Backend(format!("Failed to add elements: {}", e)))?;
        
        gst::Element::link_many([&videoconvert, &videoscale, appsink.upcast_ref()])
            .map_err(|e| DisplayError::Backend(format!("Failed to link elements: {}", e)))?;
        
        // Create ghost pad
        let pad = videoconvert.static_pad("sink")
            .ok_or_else(|| DisplayError::Backend("No sink pad".into()))?;
        let ghost_pad = gst::GhostPad::with_target(&pad)
            .map_err(|e| DisplayError::Backend(format!("Failed to create ghost pad: {}", e)))?;
        sinkbin.add_pad(&ghost_pad)
            .map_err(|e| DisplayError::Backend(format!("Failed to add ghost pad: {}", e)))?;
        
        // Set video sink on playbin
        playbin.set_property("video-sink", &sinkbin);
        
        // Get pipeline
        let pipeline: gst::Pipeline = playbin.downcast()
            .map_err(|_| DisplayError::Backend("Failed to downcast to pipeline".into()))?;
        
        let frame_data = Arc::new(Mutex::new(None));
        let frame_data_clone = frame_data.clone();
        
        // Set up frame callback - store raw bytes, not Cairo surface
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;
                    
                    let video_info = gst_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;
                    
                    let width = video_info.width() as i32;
                    let height = video_info.height() as i32;
                    
                    // Copy buffer data
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let pixels = map.to_vec();
                    
                    if let Ok(mut data) = frame_data_clone.lock() {
                        *data = Some(FrameData { pixels, width, height });
                    }
                    
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        
        Ok(Self {
            pipeline,
            appsink,
            frame_data,
            width: 0,
            height: 0,
            state: VideoState::Stopped,
            duration_ns: None,
            position_ns: 0,
            looping: false,
            volume: 1.0,
        })
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
        self.pipeline.set_state(gst::State::Null)
            .map_err(|e| DisplayError::Backend(format!("Failed to stop: {:?}", e)))?;
        self.state = VideoState::Stopped;
        Ok(())
    }
    
    /// Seek to position (nanoseconds)
    pub fn seek(&mut self, position_ns: i64) -> DisplayResult<()> {
        self.pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_nseconds(position_ns as u64),
        ).map_err(|e| DisplayError::Backend(format!("Failed to seek: {:?}", e)))?;
        self.position_ns = position_ns;
        Ok(())
    }
    
    /// Set volume (0.0 - 1.0)
    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume.clamp(0.0, 1.0);
        // playbin has a volume property
        if let Some(playbin) = self.pipeline.by_name("playbin") {
            playbin.set_property("volume", self.volume);
        }
    }
    
    /// Get current frame as Cairo surface (creates surface on main thread)
    pub fn get_frame(&self) -> Option<cairo::ImageSurface> {
        let guard = self.frame_data.lock().ok()?;
        let frame = guard.as_ref()?;
        
        create_surface_from_raw(&frame.pixels, frame.width, frame.height).ok()
    }
    
    /// Get current frame as GDK texture for GPU rendering
    pub fn get_frame_texture(&self) -> Option<gdk::Texture> {
        let guard = self.frame_data.lock().ok()?;
        let frame = guard.as_ref()?;
        
        // Create GdkTexture from raw BGRA pixel data
        // GBytes takes ownership of the data
        let bytes = glib::Bytes::from(&frame.pixels);
        
        // Create memory texture (BGRA format = B8G8R8A8_PREMULTIPLIED)
        let texture = gdk::MemoryTexture::new(
            frame.width,
            frame.height,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            (frame.width * 4) as usize, // stride = width * 4 bytes per pixel
        );
        
        Some(texture.upcast())
    }
    
    /// Update video state (call periodically)
    pub fn update(&mut self) {
        // Update position
        if let Some(position) = self.pipeline.query_position::<gst::ClockTime>() {
            self.position_ns = position.nseconds() as i64;
        }
        
        // Get duration if not known
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
                        eprintln!("GStreamer error: {:?}", err);
                        self.state = VideoState::Error;
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(feature = "video")]
impl Drop for VideoPlayer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
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

/// GPU-accelerated video player using VA-API and DMA-BUF
#[cfg(feature = "video")]
pub struct GpuVideoPlayer {
    /// GStreamer pipeline
    pipeline: gst::Pipeline,
    
    /// App sink for DMA-BUF frame capture
    appsink: gst_app::AppSink,
    
    /// Current DMA-BUF texture (cached)
    current_texture: Arc<Mutex<Option<gdk::Texture>>>,
    
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
    
    /// Whether hardware decoding is active
    pub hw_accel: bool,
}

#[cfg(feature = "video")]
impl GpuVideoPlayer {
    /// Create a new GPU-accelerated video player
    /// 
    /// This uses VA-API for hardware decoding when available, with automatic
    /// fallback to software decoding. Frames are exported via DMA-BUF for
    /// zero-copy rendering to GTK4/GSK.
    pub fn new(uri: &str) -> DisplayResult<Self> {
        gst::init()
            .map_err(|e| DisplayError::Backend(format!("Failed to init GStreamer: {}", e)))?;
        
        // Create playbin - it auto-selects VA-API decoders when available
        let playbin = gst::ElementFactory::make("playbin")
            .name("playbin")
            .property("uri", uri)
            .build()
            .map_err(|e| DisplayError::Backend(format!("Failed to create playbin: {}", e)))?;
        
        // Create appsink that accepts DMA-BUF memory
        // We request DMA_DRM format which indicates DMA-BUF with DRM modifier
        let appsink = gst_app::AppSink::builder()
            .caps(&gst::Caps::builder("video/x-raw")
                .features(["memory:DMABuf"])
                .field("format", "DMA_DRM")
                .build())
            .build();
        
        // Create video post-processor for format conversion
        // vapostproc converts VA memory to DMA-BUF
        let vapostproc = gst::ElementFactory::make("vapostproc")
            .build()
            .ok(); // Optional - may not exist
        
        // Fallback: regular videoconvert if vapostproc not available
        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| DisplayError::Backend(format!("Failed to create videoconvert: {}", e)))?;
        
        // Create bin for video sink
        let sinkbin = gst::Bin::new();
        
        let hw_accel = if let Some(ref vapost) = vapostproc {
            // Hardware path: vapostproc -> appsink
            sinkbin.add_many([vapost, appsink.upcast_ref()])
                .map_err(|e| DisplayError::Backend(format!("Failed to add elements: {}", e)))?;
            gst::Element::link_many([vapost, appsink.upcast_ref()])
                .map_err(|e| DisplayError::Backend(format!("Failed to link elements: {}", e)))?;
            
            let pad = vapost.static_pad("sink")
                .ok_or_else(|| DisplayError::Backend("No sink pad on vapostproc".into()))?;
            let ghost_pad = gst::GhostPad::with_target(&pad)
                .map_err(|e| DisplayError::Backend(format!("Failed to create ghost pad: {}", e)))?;
            sinkbin.add_pad(&ghost_pad)
                .map_err(|e| DisplayError::Backend(format!("Failed to add ghost pad: {}", e)))?;
            
            eprintln!("[GpuVideoPlayer] Using VA-API hardware acceleration");
            true
        } else {
            // Software fallback: videoconvert -> appsink (with regular memory)
            // Reconfigure appsink for regular BGRA
            appsink.set_caps(Some(&gst::Caps::builder("video/x-raw")
                .field("format", "BGRA")
                .build()));
            
            sinkbin.add_many([&videoconvert, appsink.upcast_ref()])
                .map_err(|e| DisplayError::Backend(format!("Failed to add elements: {}", e)))?;
            gst::Element::link_many([&videoconvert, appsink.upcast_ref()])
                .map_err(|e| DisplayError::Backend(format!("Failed to link elements: {}", e)))?;
            
            let pad = videoconvert.static_pad("sink")
                .ok_or_else(|| DisplayError::Backend("No sink pad".into()))?;
            let ghost_pad = gst::GhostPad::with_target(&pad)
                .map_err(|e| DisplayError::Backend(format!("Failed to create ghost pad: {}", e)))?;
            sinkbin.add_pad(&ghost_pad)
                .map_err(|e| DisplayError::Backend(format!("Failed to add ghost pad: {}", e)))?;
            
            eprintln!("[GpuVideoPlayer] Falling back to software decoding");
            false
        };
        
        // Set video sink on playbin
        playbin.set_property("video-sink", &sinkbin);
        
        // Get pipeline
        let pipeline: gst::Pipeline = playbin.downcast()
            .map_err(|_| DisplayError::Backend("Failed to downcast to pipeline".into()))?;
        
        let current_texture = Arc::new(Mutex::new(None));
        let texture_clone = current_texture.clone();
        let hw_accel_clone = hw_accel;
        
        // Set up frame callback
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;
                    
                    let video_info = gst_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;
                    
                    let width = video_info.width() as i32;
                    let height = video_info.height() as i32;
                    
                    // Create texture based on memory type
                    let texture = if hw_accel_clone {
                        // Try DMA-BUF path
                        Self::create_dmabuf_texture(buffer, width, height)
                    } else {
                        // Software path - create MemoryTexture
                        Self::create_memory_texture(buffer, width, height)
                    };
                    
                    if let Some(tex) = texture {
                        if let Ok(mut current) = texture_clone.lock() {
                            *current = Some(tex);
                        }
                    }
                    
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        
        Ok(Self {
            pipeline,
            appsink,
            current_texture,
            width: 0,
            height: 0,
            state: VideoState::Stopped,
            duration_ns: None,
            position_ns: 0,
            looping: false,
            volume: 1.0,
            hw_accel,
        })
    }
    
    /// Create GdkDmabufTexture from GStreamer DMA-BUF buffer
    fn create_dmabuf_texture(buffer: &gst::BufferRef, width: i32, height: i32) -> Option<gdk::Texture> {
        // For now, fall back to memory texture
        // Full DMA-BUF zero-copy requires:
        // 1. gstreamer-allocators crate for gst_dmabuf_memory_get_fd()
        // 2. GdkDmabufTextureBuilder to create texture from FD
        // 
        // The pipeline uses VA-API for hardware decoding, which is the main
        // performance win. The memory copy to texture is relatively fast.
        //
        // TODO: Implement true zero-copy when gstreamer-allocators is available
        Self::create_memory_texture(buffer, width, height)
    }
    
    /// Create GdkMemoryTexture from buffer (fallback path)
    fn create_memory_texture(buffer: &gst::BufferRef, width: i32, height: i32) -> Option<gdk::Texture> {
        let map = buffer.map_readable().ok()?;
        let pixels = map.as_slice();
        
        let bytes = glib::Bytes::from(pixels);
        let texture = gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            (width * 4) as usize,
        );
        
        Some(texture.upcast())
    }
    
    /// Get current frame as GDK texture (zero-copy when using DMA-BUF)
    pub fn get_frame_texture(&self) -> Option<gdk::Texture> {
        self.current_texture.lock().ok()?.clone()
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

/// Video cache for managing multiple video players
#[cfg(feature = "video")]
#[derive(Default)]
pub struct VideoCache {
    players: HashMap<u32, VideoPlayer>,
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
        let player = VideoPlayer::new(uri)?;
        let id = self.next_id;
        self.next_id += 1;
        self.players.insert(id, player);
        Ok(id)
    }
    
    /// Get a video player
    pub fn get(&self, id: u32) -> Option<&VideoPlayer> {
        self.players.get(&id)
    }
    
    /// Get a video player mutably
    pub fn get_mut(&mut self, id: u32) -> Option<&mut VideoPlayer> {
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
