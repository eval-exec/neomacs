//! Render thread implementation.
//!
//! Owns winit event loop, wgpu, GLib/WebKit. Runs at native VSync.

use std::collections::HashMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopBuilder};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(target_os = "linux")]
use winit::platform::x11::EventLoopBuilderExtX11;
#[cfg(target_os = "linux")]
use winit::platform::wayland::EventLoopBuilderExtWayland;

use crate::backend::wgpu::{
    WgpuGlyphAtlas, WgpuRenderer,
    NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SHIFT_MASK, NEOMACS_SUPER_MASK,
};
use crate::core::face::Face;
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::thread_comm::{InputEvent, RenderCommand, RenderComms};

#[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
use crate::backend::wpe::sys::platform as plat;

#[cfg(feature = "wpe-webkit")]
use crate::backend::wpe::{WpeBackend, WpeWebView};

#[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
use crate::backend::wgpu::WgpuWebKitCache;

#[cfg(feature = "video")]
use crate::backend::wgpu::VideoCache;

/// Render thread state
pub struct RenderThread {
    handle: Option<JoinHandle<()>>,
}

impl RenderThread {
    /// Spawn the render thread
    pub fn spawn(comms: RenderComms, width: u32, height: u32, title: String) -> Self {
        let handle = thread::spawn(move || {
            run_render_loop(comms, width, height, title);
        });

        Self {
            handle: Some(handle),
        }
    }

    /// Wait for render thread to finish
    pub fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Application state for winit event loop
struct RenderApp {
    comms: RenderComms,
    window: Option<Arc<Window>>,
    current_frame: Option<FrameGlyphBuffer>,
    width: u32,
    height: u32,
    title: String,

    // wgpu state
    renderer: Option<WgpuRenderer>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
    glyph_atlas: Option<WgpuGlyphAtlas>,

    // Face cache built from frame data
    faces: HashMap<u32, Face>,

    // Current modifier state (NEOMACS_*_MASK flags)
    modifiers: u32,

    // Last known cursor position
    mouse_pos: (f32, f32),

    // WebKit state
    #[cfg(feature = "wpe-webkit")]
    wpe_backend: Option<WpeBackend>,

    #[cfg(feature = "wpe-webkit")]
    webkit_views: HashMap<u32, WpeWebView>,

    #[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
    webkit_texture_cache: Option<WgpuWebKitCache>,

    // Video state
    #[cfg(feature = "video")]
    video_cache: Option<VideoCache>,
}

impl RenderApp {
    fn new(comms: RenderComms, width: u32, height: u32, title: String) -> Self {
        Self {
            comms,
            window: None,
            current_frame: None,
            width,
            height,
            title,
            renderer: None,
            surface: None,
            surface_config: None,
            device: None,
            queue: None,
            glyph_atlas: None,
            faces: HashMap::new(),
            modifiers: 0,
            mouse_pos: (0.0, 0.0),
            #[cfg(feature = "wpe-webkit")]
            wpe_backend: None,
            #[cfg(feature = "wpe-webkit")]
            webkit_views: HashMap::new(),
            #[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
            webkit_texture_cache: None,
            #[cfg(feature = "video")]
            video_cache: None,
        }
    }

    /// Initialize wgpu with the window
    fn init_wgpu(&mut self, window: Arc<Window>) {
        log::info!("Initializing wgpu for render thread");

        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create surface from window
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to create wgpu surface: {:?}", e);
                return;
            }
        };

        // Request adapter
        let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })) {
            Some(a) => a,
            None => {
                log::error!("Failed to find suitable GPU adapter");
                return;
            }
        };

        let adapter_info = adapter.get_info();
        log::info!(
            "wgpu adapter: {} (vendor={:04x}, device={:04x}, backend={:?})",
            adapter_info.name,
            adapter_info.vendor,
            adapter_info.device,
            adapter_info.backend
        );

        // Request device and queue
        let (device, queue) = match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Neomacs Render Thread Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        )) {
            Ok((d, q)) => (d, q),
            Err(e) => {
                log::error!("Failed to create wgpu device: {:?}", e);
                return;
            }
        };

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: self.width,
            height: self.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create renderer with existing device
        let renderer = WgpuRenderer::with_device(device.clone(), queue.clone(), self.width, self.height);

        // Create glyph atlas
        let glyph_atlas = WgpuGlyphAtlas::new(&device);

        log::info!(
            "wgpu initialized: {}x{}, format: {:?}",
            self.width,
            self.height,
            format
        );

        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.device = Some(device.clone());
        self.queue = Some(queue);
        self.renderer = Some(renderer);
        self.glyph_atlas = Some(glyph_atlas);

        // Initialize WPE backend for WebKit
        #[cfg(feature = "wpe-webkit")]
        {
            use crate::backend::wgpu::get_render_node_from_adapter_info;

            // Get DRM render node from adapter to ensure WebKit uses the same GPU
            let render_node = get_render_node_from_adapter_info(&adapter_info)
                .map(|p| p.to_string_lossy().into_owned());

            log::info!("Initializing WPE backend (render_node: {:?})", render_node);

            // SAFETY: We pass null for egl_display_hint as WPE Platform API doesn't use it
            match unsafe { WpeBackend::new_with_device(std::ptr::null_mut(), render_node.as_deref()) } {
                Ok(backend) => {
                    log::info!("WPE backend initialized successfully");
                    self.wpe_backend = Some(backend);
                }
                Err(e) => {
                    log::warn!("Failed to initialize WPE backend: {:?}", e);
                }
            }

            // Initialize WebKit texture cache
            self.webkit_texture_cache = Some(WgpuWebKitCache::new(&device));
        }

        // Initialize video cache
        #[cfg(feature = "video")]
        {
            let mut video_cache = VideoCache::new();
            video_cache.init_gpu(&device);
            self.video_cache = Some(video_cache);
            log::info!("Video cache initialized");
        }
    }

    /// Handle surface resize
    fn handle_resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.width = width;
        self.height = height;

        // Reconfigure surface
        if let (Some(surface), Some(config), Some(device)) =
            (&self.surface, &mut self.surface_config, &self.device)
        {
            config.width = width;
            config.height = height;
            surface.configure(device, config);
        }

        // Resize renderer
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height);
        }

        log::debug!("Surface resized to {}x{}", width, height);
    }


    /// Process pending commands from Emacs
    fn process_commands(&mut self) -> bool {
        let mut should_exit = false;

        while let Ok(cmd) = self.comms.cmd_rx.try_recv() {
            match cmd {
                RenderCommand::Shutdown => {
                    log::info!("Render thread received shutdown command");
                    should_exit = true;
                }
                RenderCommand::WebKitCreate { id, width, height } => {
                    log::info!("Creating WebKit view: id={}, {}x{}", id, width, height);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(ref backend) = self.wpe_backend {
                        if let Some(platform_display) = backend.platform_display() {
                            match WpeWebView::new(id, platform_display, width, height) {
                                Ok(view) => {
                                    self.webkit_views.insert(id, view);
                                    log::info!("WebKit view {} created successfully", id);
                                }
                                Err(e) => log::error!("Failed to create WebKit view {}: {:?}", id, e),
                            }
                        } else {
                            log::error!("WPE platform display not available");
                        }
                    } else {
                        log::warn!("WPE backend not initialized, cannot create WebKit view");
                    }
                }
                RenderCommand::WebKitLoadUri { id, url } => {
                    log::info!("Loading URL in WebKit view {}: {}", id, url);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        if let Err(e) = view.load_uri(&url) {
                            log::error!("Failed to load URL in view {}: {:?}", id, e);
                        }
                    } else {
                        log::warn!("WebKit view {} not found", id);
                    }
                }
                RenderCommand::WebKitResize { id, width, height } => {
                    log::debug!("Resizing WebKit view {}: {}x{}", id, width, height);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        view.resize(width, height);
                    }
                }
                RenderCommand::WebKitDestroy { id } => {
                    log::info!("Destroying WebKit view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    {
                        self.webkit_views.remove(&id);
                        if let Some(ref mut cache) = self.webkit_texture_cache {
                            cache.remove(id);
                        }
                    }
                }
                RenderCommand::VideoCreate { id, path } => {
                    log::info!("Loading video {}: {}", id, path);
                    #[cfg(feature = "video")]
                    if let Some(ref mut cache) = self.video_cache {
                        let video_id = cache.load_file(&path);
                        log::info!("Video loaded with id {} (requested id was {})", video_id, id);
                    }
                }
                RenderCommand::VideoPlay { id } => {
                    log::debug!("Playing video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut cache) = self.video_cache {
                        cache.play(id);
                    }
                }
                RenderCommand::VideoPause { id } => {
                    log::debug!("Pausing video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut cache) = self.video_cache {
                        cache.pause(id);
                    }
                }
                RenderCommand::VideoDestroy { id } => {
                    log::info!("Destroying video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut cache) = self.video_cache {
                        cache.remove(id);
                    }
                }
            }
        }

        should_exit
    }

    /// Get latest frame from Emacs (non-blocking)
    fn poll_frame(&mut self) {
        // Get the newest frame, discarding older ones
        while let Ok(frame) = self.comms.frame_rx.try_recv() {
            self.current_frame = Some(frame);
        }
    }

    /// Pump GLib events (non-blocking) and update webkit views
    #[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
    fn pump_glib(&mut self) {
        unsafe {
            // WPEViewHeadless attaches to thread-default context
            let thread_ctx = plat::g_main_context_get_thread_default();
            let ctx = if thread_ctx.is_null() {
                plat::g_main_context_default()
            } else {
                thread_ctx
            };

            // Non-blocking iteration - process all pending events
            while plat::g_main_context_iteration(ctx, 0) != 0 {}

            // Also check default context if different
            let default_ctx = plat::g_main_context_default();
            if default_ctx != ctx {
                while plat::g_main_context_iteration(default_ctx, 0) != 0 {}
            }
        }

        // Update all webkit views and send state change events
        for (id, view) in self.webkit_views.iter_mut() {
            let old_title = view.title.clone();
            let old_url = view.url.clone();
            let old_progress = view.progress;

            view.update();

            // Send state change events
            if view.title != old_title {
                if let Some(ref title) = view.title {
                    self.comms.send_input(InputEvent::WebKitTitleChanged {
                        id: *id,
                        title: title.clone(),
                    });
                }
            }
            if view.url != old_url {
                self.comms.send_input(InputEvent::WebKitUrlChanged {
                    id: *id,
                    url: view.url.clone(),
                });
            }
            if (view.progress - old_progress).abs() > 0.01 {
                self.comms.send_input(InputEvent::WebKitProgressChanged {
                    id: *id,
                    progress: view.progress,
                });
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", wpe_platform_available)))]
    fn pump_glib(&mut self) {}

    /// Process webkit frames and import to wgpu textures
    #[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
    fn process_webkit_frames(&mut self) {
        use crate::backend::wgpu::external_buffer::DmaBufBuffer;

        let device = match &self.device {
            Some(d) => d,
            None => return,
        };
        let queue = match &self.queue {
            Some(q) => q,
            None => return,
        };
        let cache = match &mut self.webkit_texture_cache {
            Some(c) => c,
            None => return,
        };

        for (view_id, view) in &self.webkit_views {
            // Try DMA-BUF first (zero-copy)
            if let Some(dmabuf) = view.take_latest_dmabuf() {
                let num_planes = dmabuf.fds.len().min(4) as u32;
                let mut fds = [-1i32; 4];
                let mut strides = [0u32; 4];
                let mut offsets = [0u32; 4];

                for i in 0..num_planes as usize {
                    fds[i] = dmabuf.fds[i];
                    strides[i] = dmabuf.strides[i];
                    offsets[i] = dmabuf.offsets[i];
                }

                let buffer = DmaBufBuffer::new(
                    fds,
                    strides,
                    offsets,
                    num_planes,
                    dmabuf.width,
                    dmabuf.height,
                    dmabuf.fourcc,
                    dmabuf.modifier,
                );

                if cache.update_view(*view_id, buffer, device, queue) {
                    log::debug!("Imported DMA-BUF for webkit view {}", view_id);
                } else {
                    log::warn!("Failed to import DMA-BUF for webkit view {}", view_id);
                }
            }
            // Fallback to pixel upload
            else if let Some(raw_pixels) = view.take_latest_pixels() {
                if cache.update_view_from_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels, device, queue) {
                    log::debug!("Uploaded pixels for webkit view {}", view_id);
                }
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", target_os = "linux")))]
    fn process_webkit_frames(&mut self) {}

    /// Process pending video frames
    #[cfg(feature = "video")]
    fn process_video_frames(&mut self) {
        let device = match &self.device {
            Some(d) => d,
            None => return,
        };
        let queue = match &self.queue {
            Some(q) => q,
            None => return,
        };

        if let Some(ref mut cache) = self.video_cache {
            cache.process_pending_frames(device, queue);
        }
    }

    #[cfg(not(feature = "video"))]
    fn process_video_frames(&mut self) {}

    /// Render the current frame
    fn render(&mut self) {
        // Early return checks
        if self.current_frame.is_none() || self.surface.is_none() || self.renderer.is_none() {
            return;
        }

        // Process webkit frames (import DMA-BUF to textures)
        self.process_webkit_frames();

        // Process video frames
        self.process_video_frames();

        // Build faces from frame data first (while we can mutably borrow self)
        if let Some(ref frame) = self.current_frame {
            // Build faces from frame glyphs
            for glyph in &frame.glyphs {
                if let FrameGlyph::Char {
                    face_id,
                    fg,
                    bold,
                    italic,
                    font_size,
                    ..
                } = glyph
                {
                    if !self.faces.contains_key(face_id) {
                        let mut face = Face::new(*face_id);
                        face.foreground = *fg;
                        face.font_size = *font_size;
                        if *bold {
                            face.font_weight = 700;
                        }
                        if *italic {
                            face.attributes |= crate::core::face::FaceAttributes::ITALIC;
                        }
                        if let Some(font_family) = frame.face_fonts.get(face_id) {
                            face.font_family = font_family.clone();
                        }
                        self.faces.insert(*face_id, face);
                    }
                }
            }
        }

        // Get surface texture
        let surface = self.surface.as_ref().unwrap();
        let output = match surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost) => {
                // Reconfigure surface
                let (w, h) = (self.width, self.height);
                self.handle_resize(w, h);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("Out of GPU memory");
                return;
            }
            Err(e) => {
                log::warn!("Surface error: {:?}", e);
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Now render with immutable borrows
        let frame = self.current_frame.as_ref().unwrap();
        let renderer = self.renderer.as_ref().unwrap();
        let glyph_atlas = self.glyph_atlas.as_mut().unwrap();

        // Render frame glyphs
        log::trace!(
            "Rendering frame: {}x{}, {} glyphs",
            frame.width,
            frame.height,
            frame.glyphs.len()
        );

        renderer.render_frame_glyphs(
            &view,
            frame,
            glyph_atlas,
            &self.faces,
            self.width,
            self.height,
        );

        // Present the frame
        output.present();
    }

    /// Translate winit key to X11 keysym
    fn translate_key(key: &Key) -> u32 {
        match key {
            Key::Named(named) => match named {
                // Function keys
                NamedKey::F1 => 0xffbe,
                NamedKey::F2 => 0xffbf,
                NamedKey::F3 => 0xffc0,
                NamedKey::F4 => 0xffc1,
                NamedKey::F5 => 0xffc2,
                NamedKey::F6 => 0xffc3,
                NamedKey::F7 => 0xffc4,
                NamedKey::F8 => 0xffc5,
                NamedKey::F9 => 0xffc6,
                NamedKey::F10 => 0xffc7,
                NamedKey::F11 => 0xffc8,
                NamedKey::F12 => 0xffc9,
                // Navigation
                NamedKey::Escape => 0xff1b,
                NamedKey::Enter => 0xff0d,
                NamedKey::Tab => 0xff09,
                NamedKey::Backspace => 0xff08,
                NamedKey::Delete => 0xffff,
                NamedKey::Insert => 0xff63,
                NamedKey::Home => 0xff50,
                NamedKey::End => 0xff57,
                NamedKey::PageUp => 0xff55,
                NamedKey::PageDown => 0xff56,
                NamedKey::ArrowLeft => 0xff51,
                NamedKey::ArrowUp => 0xff52,
                NamedKey::ArrowRight => 0xff53,
                NamedKey::ArrowDown => 0xff54,
                // Whitespace
                NamedKey::Space => 0x20,
                // Modifier keys (as keys themselves)
                NamedKey::Shift => 0xffe1,
                NamedKey::Control => 0xffe3,
                NamedKey::Alt => 0xffe9,
                NamedKey::Super => 0xffeb,
                NamedKey::CapsLock => 0xffe5,
                // Other
                NamedKey::PrintScreen => 0xff61,
                NamedKey::ScrollLock => 0xff14,
                NamedKey::Pause => 0xff13,
                NamedKey::NumLock => 0xff7f,
                _ => 0,
            },
            Key::Character(c) => {
                c.chars().next().map(|ch| ch as u32).unwrap_or(0)
            }
            _ => 0,
        }
    }
}

impl ApplicationHandler for RenderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title(&self.title)
                .with_inner_size(winit::dpi::PhysicalSize::new(self.width, self.height));

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    log::info!("Render thread: window created");
                    let window = Arc::new(window);

                    // Initialize wgpu with the window
                    self.init_wgpu(window.clone());

                    self.window = Some(window);
                }
                Err(e) => {
                    log::error!("Failed to create window: {:?}", e);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Window close requested");
                self.comms.send_input(InputEvent::WindowClose);
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                log::info!("WindowEvent::Resized: {}x{}", size.width, size.height);

                // Handle wgpu surface resize
                self.handle_resize(size.width, size.height);

                // Notify Emacs of the resize
                log::info!("Sending WindowResize event to Emacs: {}x{}", size.width, size.height);
                self.comms.send_input(InputEvent::WindowResize {
                    width: size.width,
                    height: size.height,
                });
            }

            WindowEvent::Focused(focused) => {
                self.comms.send_input(InputEvent::WindowFocus { focused });
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key, state, ..
                    },
                ..
            } => {
                let keysym = Self::translate_key(&logical_key);
                if keysym != 0 {
                    self.comms.send_input(InputEvent::Key {
                        keysym,
                        modifiers: self.modifiers,
                        pressed: state == ElementState::Pressed,
                    });
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    MouseButton::Left => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Right => 3,
                    MouseButton::Back => 4,
                    MouseButton::Forward => 5,
                    MouseButton::Other(n) => n as u32,
                };
                self.comms.send_input(InputEvent::MouseButton {
                    button: btn,
                    x: self.mouse_pos.0,
                    y: self.mouse_pos.1,
                    pressed: state == ElementState::Pressed,
                    modifiers: self.modifiers,
                });
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x as f32, position.y as f32);
                self.comms.send_input(InputEvent::MouseMove {
                    x: position.x as f32,
                    y: position.y as f32,
                    modifiers: self.modifiers,
                });
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x, y),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32 / 10.0, pos.y as f32 / 10.0)
                    }
                };
                self.comms.send_input(InputEvent::MouseScroll {
                    delta_x: dx,
                    delta_y: dy,
                    x: self.mouse_pos.0,
                    y: self.mouse_pos.1,
                    modifiers: self.modifiers,
                });
            }

            WindowEvent::RedrawRequested => {
                self.render();
            }

            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                self.modifiers = 0;
                if state.shift_key() {
                    self.modifiers |= NEOMACS_SHIFT_MASK;
                }
                if state.control_key() {
                    self.modifiers |= NEOMACS_CTRL_MASK;
                }
                if state.alt_key() {
                    self.modifiers |= NEOMACS_META_MASK;
                }
                if state.super_key() {
                    self.modifiers |= NEOMACS_SUPER_MASK;
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Check for shutdown
        if self.process_commands() {
            event_loop.exit();
            return;
        }

        // Get latest frame from Emacs
        self.poll_frame();

        // Pump GLib for WebKit
        self.pump_glib();

        // Request redraw for VSync
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}

/// Run the render loop (called on render thread)
fn run_render_loop(comms: RenderComms, width: u32, height: u32, title: String) {
    log::info!("Render thread starting");

    // Use any_thread() since we're running on a non-main thread
    #[cfg(target_os = "linux")]
    let event_loop = {
        let mut builder = EventLoopBuilder::new();
        // Try Wayland first, fall back to X11
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
        } else {
            EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
        }
        builder.build().expect("Failed to create event loop")
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = RenderApp::new(comms, width, height, title);

    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Event loop error: {:?}", e);
    }

    log::info!("Render thread exiting");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_comm::ThreadComms;

    #[test]
    fn test_translate_key_named() {
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Escape)), 0xff1b);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Enter)), 0xff0d);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Tab)), 0xff09);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Backspace)), 0xff08);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Delete)), 0xffff);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Home)), 0xff50);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::End)), 0xff57);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::PageUp)), 0xff55);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::PageDown)), 0xff56);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowLeft)), 0xff51);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowUp)), 0xff52);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowRight)), 0xff53);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowDown)), 0xff54);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Space)), 0x20);
    }

    #[test]
    fn test_translate_key_character() {
        assert_eq!(
            RenderApp::translate_key(&Key::Character("a".into())),
            'a' as u32
        );
        assert_eq!(
            RenderApp::translate_key(&Key::Character("A".into())),
            'A' as u32
        );
        assert_eq!(
            RenderApp::translate_key(&Key::Character("1".into())),
            '1' as u32
        );
    }

    #[test]
    fn test_translate_key_function_keys() {
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::F1)), 0xffbe);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::F12)), 0xffc9);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Insert)), 0xff63);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::PrintScreen)), 0xff61);
    }

    #[test]
    fn test_translate_key_unknown() {
        // Unknown named keys should return 0
        assert_eq!(RenderApp::translate_key(&Key::Dead(None)), 0);
    }

    #[test]
    fn test_render_thread_creation() {
        // Just test that ThreadComms can be created and split
        let comms = ThreadComms::new().expect("Failed to create ThreadComms");
        let (emacs, render) = comms.split();

        // Verify we can access the channels
        assert!(emacs.input_rx.is_empty());
        assert!(render.cmd_rx.is_empty());
    }
}
