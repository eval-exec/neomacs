//! Video cache with GStreamer backend and optional VA-API hardware acceleration.
//!
//! Provides async video decoding with DMA-BUF zero-copy when available,
//! falling back to CPU decode + copy otherwise.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
#[cfg(target_os = "linux")]
use std::os::unix::io::RawFd;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use gstreamer_app as gst_app;
#[cfg(target_os = "linux")]
use gstreamer_allocators as gst_allocators;

/// Video playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    /// Video is loading/buffering
    Loading,
    /// Video is playing
    Playing,
    /// Video is paused
    Paused,
    /// Video playback stopped
    Stopped,
    /// Video reached end
    EndOfStream,
    /// Error occurred
    Error,
}

/// DMA-BUF information for zero-copy path
#[cfg(target_os = "linux")]
pub struct DmaBufInfo {
    /// File descriptor
    pub fd: RawFd,
    /// Stride (bytes per row)
    pub stride: u32,
    /// DRM fourcc format code
    pub fourcc: u32,
    /// DRM modifier
    pub modifier: u64,
}

/// Decoded video frame ready for rendering
pub struct DecodedFrame {
    /// Frame ID
    pub id: u32,
    /// Video ID this frame belongs to
    pub video_id: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// RGBA pixel data (CPU path) - empty if using DMA-BUF
    pub data: Vec<u8>,
    /// DMA-BUF info for zero-copy (Linux only)
    #[cfg(target_os = "linux")]
    pub dmabuf: Option<DmaBufInfo>,
    /// Presentation timestamp in nanoseconds
    pub pts: u64,
    /// Duration in nanoseconds
    pub duration: u64,
}

/// Cached video with GStreamer pipeline
pub struct CachedVideo {
    /// Video ID
    pub id: u32,
    /// Video dimensions
    pub width: u32,
    pub height: u32,
    /// Current state
    pub state: VideoState,
    /// Current wgpu texture (updated each frame)
    pub texture: Option<wgpu::Texture>,
    pub texture_view: Option<wgpu::TextureView>,
    pub bind_group: Option<wgpu::BindGroup>,
    /// Frame count
    pub frame_count: u64,
    /// Loop count (-1 = infinite)
    pub loop_count: i32,
}

/// Request to load a video
struct LoadRequest {
    id: u32,
    path: String,
}

/// Video pipeline with frame extraction
struct VideoPipeline {
    pipeline: gst::Pipeline,
    appsink: gst_video::VideoSink,
}

/// Video cache managing multiple videos with async decoding
pub struct VideoCache {
    /// Cached videos by ID
    videos: HashMap<u32, CachedVideo>,
    /// Next video ID
    next_id: u32,
    /// Channel to send load requests
    load_tx: mpsc::Sender<LoadRequest>,
    /// Channel to receive decoded frames
    frame_rx: mpsc::Receiver<DecodedFrame>,
    /// Bind group layout for video textures (created in init_gpu)
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    /// Sampler for video textures (created in init_gpu)
    sampler: Option<wgpu::Sampler>,
}

impl VideoCache {
    /// Create a new video cache
    pub fn new() -> Self {
        // Initialize GStreamer
        if let Err(e) = gst::init() {
            log::error!("Failed to initialize GStreamer: {}", e);
        }

        let (load_tx, load_rx) = mpsc::channel::<LoadRequest>();
        let (frame_tx, frame_rx) = mpsc::channel::<DecodedFrame>();

        // Spawn decoder thread
        thread::spawn(move || {
            Self::decoder_thread(load_rx, frame_tx);
        });

        Self {
            videos: HashMap::new(),
            next_id: 1,
            load_tx,
            frame_rx,
            bind_group_layout: None,
            sampler: None,
        }
    }

    /// Initialize GPU resources
    pub fn init_gpu(&mut self, device: &wgpu::Device) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Video Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Video Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.bind_group_layout = Some(bind_group_layout);
        self.sampler = Some(sampler);
        log::info!("VideoCache: GPU resources initialized");
    }

    /// Load a video file
    pub fn load_file(&mut self, path: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        // Create placeholder entry
        self.videos.insert(id, CachedVideo {
            id,
            width: 0,
            height: 0,
            state: VideoState::Loading,
            texture: None,
            texture_view: None,
            bind_group: None,
            frame_count: 0,
            loop_count: 0,
        });

        // Send load request
        let _ = self.load_tx.send(LoadRequest {
            id,
            path: path.to_string(),
        });

        log::info!("VideoCache: queued video {} for loading: {}", id, path);
        id
    }

    /// Get video state
    pub fn get_state(&self, id: u32) -> Option<VideoState> {
        self.videos.get(&id).map(|v| v.state)
    }

    /// Get video dimensions
    pub fn get_dimensions(&self, id: u32) -> Option<(u32, u32)> {
        self.videos.get(&id).map(|v| (v.width, v.height))
    }

    /// Get video for rendering
    pub fn get(&self, id: u32) -> Option<&CachedVideo> {
        self.videos.get(&id)
    }

    /// Play video
    pub fn play(&mut self, id: u32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Playing;
            log::debug!("VideoCache: play video {}", id);
        }
    }

    /// Pause video
    pub fn pause(&mut self, id: u32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Paused;
            log::debug!("VideoCache: pause video {}", id);
        }
    }

    /// Stop video
    pub fn stop(&mut self, id: u32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Stopped;
            log::debug!("VideoCache: stop video {}", id);
        }
    }

    /// Set loop count (-1 for infinite)
    pub fn set_loop(&mut self, id: u32, count: i32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.loop_count = count;
        }
    }

    /// Remove video from cache
    pub fn remove(&mut self, id: u32) {
        self.videos.remove(&id);
        log::debug!("VideoCache: removed video {}", id);
    }

    /// Check if any video is currently in Playing state
    pub fn has_playing_videos(&self) -> bool {
        self.videos.values().any(|v| v.state == VideoState::Playing)
    }

    /// Process pending decoded frames using stored GPU resources (call each frame)
    pub fn process_pending_frames(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // Take resources temporarily to avoid borrow conflict
        let layout = match self.bind_group_layout.take() {
            Some(l) => l,
            None => {
                log::warn!("VideoCache: GPU resources not initialized, skipping frame processing");
                return;
            }
        };
        let sampler = match self.sampler.take() {
            Some(s) => s,
            None => {
                self.bind_group_layout = Some(layout);
                log::warn!("VideoCache: GPU resources not initialized, skipping frame processing");
                return;
            }
        };

        // Process frames
        self.process_pending(device, queue, &layout, &sampler);

        // Put resources back
        self.bind_group_layout = Some(layout);
        self.sampler = Some(sampler);
    }

    /// Process pending decoded frames (call each frame)
    /// Uses the provided bind_group_layout and sampler from image_cache
    /// to ensure compatibility with the shared image/video rendering pipeline.
    pub fn process_pending(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        // Process all available frames
        let mut frame_count = 0;
        while let Ok(frame) = self.frame_rx.try_recv() {
            frame_count += 1;
            let total = self.videos.get(&frame.video_id).map(|v| v.frame_count).unwrap_or(0) + 1;

            log::info!("VideoCache::process_pending received frame #{} for video {}, pts={}ms, size={}x{}",
                total, frame.video_id, frame.pts / 1_000_000, frame.width, frame.height);
            if let Some(video) = self.videos.get_mut(&frame.video_id) {
                // Check if we need to create new texture (first frame or size changed)
                let need_new_texture = video.texture.is_none()
                    || video.width != frame.width
                    || video.height != frame.height;

                if need_new_texture {
                    // Update dimensions
                    video.width = frame.width;
                    video.height = frame.height;
                    if video.state == VideoState::Loading {
                        video.state = VideoState::Playing;
                    }

                    // Create new texture (only when dimensions change)
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Video Frame Texture"),
                        size: wgpu::Extent3d {
                            width: frame.width,
                            height: frame.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

                    // Create bind group
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Video Bind Group"),
                        layout: bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(sampler),
                            },
                        ],
                    });

                    video.texture = Some(texture);
                    video.texture_view = Some(texture_view);
                    video.bind_group = Some(bind_group);
                }

                // Update texture data
                // Try DMA-BUF zero-copy path first, fall back to CPU copy
                #[cfg(target_os = "linux")]
                let dmabuf_imported = if let Some(ref dmabuf) = frame.dmabuf {
                    // Try to import DMA-BUF directly into wgpu texture
                    use super::external_buffer::DmaBufBuffer;

                    let dmabuf_buffer = DmaBufBuffer::single_plane(
                        dmabuf.fd,
                        frame.width,
                        frame.height,
                        dmabuf.stride,
                        dmabuf.fourcc,
                        dmabuf.modifier,
                    );

                    if let Some(imported_texture) = dmabuf_buffer.to_wgpu_texture(device, queue) {
                        log::debug!("DMA-BUF zero-copy import successful for video {}", frame.video_id);

                        // Replace texture with imported one
                        let texture_view = imported_texture.create_view(&wgpu::TextureViewDescriptor::default());
                        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("Video DMA-BUF Bind Group"),
                            layout: bind_group_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(&texture_view),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(sampler),
                                },
                            ],
                        });

                        video.texture = Some(imported_texture);
                        video.texture_view = Some(texture_view);
                        video.bind_group = Some(bind_group);
                        true
                    } else {
                        log::debug!("DMA-BUF import failed, falling back to CPU copy");
                        false
                    }
                } else {
                    false
                };

                #[cfg(not(target_os = "linux"))]
                let dmabuf_imported = false;

                // Fall back to CPU copy if DMA-BUF import failed or not available
                if !dmabuf_imported && !frame.data.is_empty() {
                    if let Some(ref texture) = video.texture {
                        queue.write_texture(
                            wgpu::ImageCopyTexture {
                                texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            &frame.data,
                            wgpu::ImageDataLayout {
                                offset: 0,
                                bytes_per_row: Some(frame.width * 4),
                                rows_per_image: Some(frame.height),
                            },
                            wgpu::Extent3d {
                                width: frame.width,
                                height: frame.height,
                                depth_or_array_layers: 1,
                            },
                        );
                    }
                }

                video.frame_count += 1;
                log::trace!("VideoCache: updated video {} frame {}", frame.video_id, video.frame_count);
            }
        }
    }

    /// Try to extract DMA-BUF info from a GStreamer buffer
    ///
    /// Supports multiple memory types:
    /// - DmaBufMemory: Direct DMA-BUF allocator
    /// - FdMemory: Generic fd-backed memory (includes VA-API memory)
    #[cfg(target_os = "linux")]
    fn try_extract_dmabuf(buffer: &gst::BufferRef, info: &gst_video::VideoInfo) -> Option<DmaBufInfo> {
        use gst_allocators::prelude::*;

        // Get the first memory block from the buffer
        let n_memory = buffer.n_memory();
        if n_memory == 0 {
            return None;
        }

        let memory = buffer.memory(0)?;

        // Debug: log allocator info
        let allocator_type = if let Some(allocator) = memory.allocator() {
            use gst::prelude::*;
            let type_name = allocator.type_().name();
            log::debug!("Memory allocator type: {}", type_name);
            type_name.to_string()
        } else {
            log::debug!("Memory has no allocator");
            String::new()
        };

        // Try DmaBufMemory first (explicit DMA-BUF allocator)
        let fd = if let Some(dmabuf_mem) = memory.downcast_memory_ref::<gst_allocators::DmaBufMemory>() {
            log::debug!("Found DmaBufMemory");
            dmabuf_mem.fd()
        } else if let Some(fd_mem) = memory.downcast_memory_ref::<gst_allocators::FdMemory>() {
            // FdMemory: generic fd-backed memory (VA-API uses this)
            log::debug!("Found FdMemory (VA-API or other fd-backed)");
            fd_mem.fd()
        } else if allocator_type == "GstVaAllocator" {
            // VA-API memory - try to export via vaExportSurfaceHandle
            log::debug!("Attempting VA-API DMA-BUF export...");
            return Self::try_export_va_surface(buffer, &memory, info);
        } else {
            log::debug!("Buffer memory is not fd-backed (DmaBuf or Fd)");
            return None;
        };

        // Validate fd
        if fd < 0 {
            log::warn!("Invalid fd from memory: {}", fd);
            return None;
        }

        // Get stride from video info
        let stride = info.stride()[0] as u32;

        // Determine fourcc from format
        let format = info.format();
        let fourcc = match format {
            gst_video::VideoFormat::Rgba => 0x34324142, // AB24 - actually RGBA maps to ABGR in DRM
            gst_video::VideoFormat::Bgra => 0x34324241, // AR24 - BGRA maps to ARGB in DRM
            gst_video::VideoFormat::Argb => 0x34325241, // RA24
            gst_video::VideoFormat::Abgr => 0x34324152, // RA24
            gst_video::VideoFormat::Rgbx => 0x34325842, // XB24
            gst_video::VideoFormat::Bgrx => 0x34325258, // XR24
            gst_video::VideoFormat::Nv12 => 0x3231564e, // NV12
            _ => {
                log::debug!("Unsupported video format for DMA-BUF: {:?}", format);
                return None;
            }
        };

        log::info!("Extracted DMA-BUF: fd={}, stride={}, format={:?}, fourcc={:#x}", fd, stride, format, fourcc);

        Some(DmaBufInfo {
            fd,
            stride,
            fourcc,
            modifier: 0, // Linear modifier - VA-API typically uses linear
        })
    }

    /// Try to export VA-API surface as DMA-BUF
    #[cfg(target_os = "linux")]
    fn try_export_va_surface(
        buffer: &gst::BufferRef,
        memory: &gst::Memory,
        info: &gst_video::VideoInfo,
    ) -> Option<DmaBufInfo> {
        use super::va_dmabuf_export::{get_va_display_from_memory, try_export_va_dmabuf};

        // Get VA display from allocator
        let va_display = get_va_display_from_memory(memory)?;

        // Export surface as DMA-BUF
        let export = try_export_va_dmabuf(
            buffer,
            va_display,
            info.width(),
            info.height(),
        )?;

        // Use the first fd and plane info
        if export.num_planes == 0 || export.fds[0] < 0 {
            log::warn!("VA export returned no valid planes");
            return None;
        }

        log::info!(
            "VA-API DMA-BUF export: fd={}, pitch={}, fourcc={:#x}, modifier={:#x}",
            export.fds[0], export.pitches[0], export.fourcc, export.modifier
        );

        Some(DmaBufInfo {
            fd: export.fds[0],
            stride: export.pitches[0],
            fourcc: export.fourcc,
            modifier: export.modifier,
        })
    }

    /// Background decoder thread — dispatches each video to its own thread
    fn decoder_thread(
        rx: mpsc::Receiver<LoadRequest>,
        tx: mpsc::Sender<DecodedFrame>,
    ) {
        log::debug!("Video decoder thread started");

        while let Ok(request) = rx.recv() {
            log::info!("Decoder thread: dispatching video {}: {}", request.id, request.path);
            let tx_clone = tx.clone();
            // Spawn a dedicated thread per video so multiple videos load/play concurrently
            thread::spawn(move || {
                Self::decode_single_video(request.id, &request.path, tx_clone);
            });
        }

        log::debug!("Video decoder thread exiting");
    }

    /// Decode a single video: create pipeline, pull frames, wait for EOS, cleanup.
    fn decode_single_video(
        video_id: u32,
        raw_path: &str,
        tx: mpsc::Sender<DecodedFrame>,
    ) {
        log::info!("Video thread: loading video {}: {}", video_id, raw_path);

        // Strip file:// prefix if present (filesrc needs raw paths)
        let path = if raw_path.starts_with("file://") {
            &raw_path[7..]
        } else {
            raw_path
        };

        // Check if VA-API hardware acceleration is available
        let has_vapostproc = gst::ElementFactory::find("vapostproc").is_some();

        // Create GStreamer pipeline
        // NOTE: vapostproc does YUV→RGB conversion but doesn't respect downstream
        // colorimetry caps (GitLab issue #80). For BT.2020 content (10-bit VP9/AV1),
        // colors may be slightly off.
        let pipeline_str = if has_vapostproc {
            log::info!("Using VA-API hardware acceleration pipeline with zero-copy DMA-BUF");
            format!(
                "filesrc location=\"{}\" ! decodebin ! \
                 queue max-size-buffers=3 ! vapostproc ! \
                 video/x-raw(memory:VAMemory),format=RGBA ! appsink name=sink",
                path.replace("\"", "\\\"")
            )
        } else {
            log::info!("VA-API not available, using software decoding");
            format!(
                "filesrc location=\"{}\" ! decodebin ! \
                 queue ! videoconvert ! video/x-raw,format=RGBA ! appsink name=sink",
                path.replace("\"", "\\\"")
            )
        };

        log::debug!("Creating GStreamer pipeline: {}", pipeline_str);

        let pipeline = match gst::parse::launch(&pipeline_str) {
            Ok(elem) => match elem.dynamic_cast::<gst::Pipeline>() {
                Ok(p) => p,
                Err(_) => {
                    log::error!("Failed to cast pipeline element for video {}", video_id);
                    return;
                }
            },
            Err(e) => {
                log::error!("Failed to create pipeline for video {}: {}", video_id, e);
                return;
            }
        };

        // Get appsink
        let sink_element = match pipeline.by_name("sink") {
            Some(e) => e,
            None => {
                log::error!("Could not get appsink element for video {}", video_id);
                let _ = pipeline.set_state(gst::State::Null);
                return;
            }
        };
        let appsink = match sink_element.dynamic_cast::<gst_app::AppSink>() {
            Ok(s) => s,
            Err(_) => {
                log::error!("Could not cast sink to AppSink for video {}", video_id);
                let _ = pipeline.set_state(gst::State::Null);
                return;
            }
        };

        // Configure appsink for pull mode
        appsink.set_max_buffers(2);
        appsink.set_drop(true);

        // Start playing
        log::debug!("Setting pipeline to Playing state for video {}", video_id);
        if let Err(e) = pipeline.set_state(gst::State::Playing) {
            log::error!("Failed to start pipeline for video {}: {:?}", video_id, e);
            let _ = pipeline.set_state(gst::State::Null);
            return;
        }
        log::info!("Pipeline started successfully for video {}", video_id);

        // Spawn frame pulling thread
        let appsink_clone = appsink.clone();
        let pipeline_weak = pipeline.downgrade();
        let tx_puller = tx.clone();
        thread::spawn(move || {
            log::info!("Frame puller thread started for video {}", video_id);

            // Wait for pipeline to reach PLAYING state
            if let Some(pipeline) = pipeline_weak.upgrade() {
                let (res, state, _) = pipeline.state(gst::ClockTime::from_seconds(5));
                log::info!("Video {} pipeline state: {:?}, result: {:?}", video_id, state, res);
            }
            let mut frame_count = 0u64;
            let mut timeout_count = 0u64;

            loop {
                match appsink_clone.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
                    Some(sample) => {
                        timeout_count = 0;
                        frame_count += 1;
                        if let Some(buffer) = sample.buffer() {
                            if let Some(caps) = sample.caps() {
                                if let Ok(info) = gst_video::VideoInfo::from_caps(caps) {
                                    let width = info.width();
                                    let height = info.height();

                                    #[cfg(target_os = "linux")]
                                    let dmabuf_info = Self::try_extract_dmabuf(buffer, &info);
                                    #[cfg(not(target_os = "linux"))]
                                    let dmabuf_info: Option<()> = None;

                                    let has_dmabuf = dmabuf_info.is_some();
                                    if frame_count <= 5 || frame_count % 60 == 0 {
                                        log::info!("Frame #{} for video {}, {}x{}, format={:?}, DMA-BUF: {}",
                                            frame_count, video_id, width, height, info.format(), has_dmabuf);
                                    }

                                    let data = if let Ok(map) = buffer.map_readable() {
                                        map.as_slice().to_vec()
                                    } else if has_dmabuf {
                                        log::debug!("DMA-BUF memory not mappable (expected for zero-copy)");
                                        Vec::new()
                                    } else {
                                        log::warn!("Failed to map buffer and no DMA-BUF available");
                                        Vec::new()
                                    };

                                    if tx_puller.send(DecodedFrame {
                                        id: frame_count as u32,
                                        video_id,
                                        width,
                                        height,
                                        data,
                                        #[cfg(target_os = "linux")]
                                        dmabuf: dmabuf_info,
                                        pts: buffer.pts().map(|p| p.nseconds()).unwrap_or(0),
                                        duration: buffer.duration().map(|d| d.nseconds()).unwrap_or(0),
                                    }).is_err() {
                                        log::debug!("Frame receiver dropped, stopping puller");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        timeout_count += 1;
                        if appsink_clone.is_eos() {
                            log::info!("Video {} reached EOS after {} frames", video_id, frame_count);
                            break;
                        }
                        if timeout_count == 1 || timeout_count % 50 == 0 {
                            log::debug!("Video {} pull timeout #{}, frames so far: {}", video_id, timeout_count, frame_count);
                        }
                    }
                }
            }
            log::debug!("Frame puller thread exiting for video {}", video_id);
        });

        // Wait for EOS or error on bus (this thread is dedicated to this video,
        // so blocking here doesn't prevent other videos from loading)
        let bus = match pipeline.bus() {
            Some(b) => b,
            None => {
                log::error!("Could not get bus from pipeline for video {}", video_id);
                let _ = pipeline.set_state(gst::State::Null);
                return;
            }
        };
        for msg in bus.iter_timed(gst::ClockTime::NONE) {
            match msg.view() {
                gst::MessageView::Eos(..) => {
                    log::debug!("Video {} bus: end of stream", video_id);
                    break;
                }
                gst::MessageView::Error(err) => {
                    log::error!(
                        "Video {} error: {} ({:?})",
                        video_id,
                        err.error(),
                        err.debug()
                    );
                    break;
                }
                _ => {}
            }
        }

        // Cleanup
        let _ = pipeline.set_state(gst::State::Null);
        log::debug!("Video {} pipeline cleaned up", video_id);
    }
}

impl Default for VideoCache {
    fn default() -> Self {
        Self::new()
    }
}
