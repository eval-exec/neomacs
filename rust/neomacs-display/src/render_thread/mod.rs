//! Render thread implementation.
//!
//! Owns winit event loop, wgpu, GLib/WebKit. Runs at native VSync.

pub(crate) mod child_frames;
mod cursor;
mod input;
pub(crate) mod multi_window;
mod popup_menu;
mod transitions;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
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
use crate::core::types::{
    AnimatedCursor, Color, CursorAnimStyle, Rect,
    ease_out_quad, ease_out_cubic, ease_out_expo, ease_in_out_cubic, ease_linear,
};
use crate::thread_comm::{InputEvent, PopupMenuItem, RenderCommand, RenderComms};
use cursor::{CursorTarget, CornerSpring, CursorState};
pub(crate) use popup_menu::{MenuPanel, PopupMenuState, TooltipState};
use transitions::{CrossfadeTransition, ScrollTransition, TransitionState};

#[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
use crate::backend::wpe::sys::platform as plat;

#[cfg(feature = "wpe-webkit")]
use crate::backend::wpe::{WpeBackend, WpeWebView};

// All GPU caches (image, video, webkit) are managed by WgpuRenderer

/// Shared storage for image dimensions accessible from both threads
pub type SharedImageDimensions = Arc<Mutex<HashMap<u32, (u32, u32)>>>;

/// Monitor information collected from winit
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub width_mm: i32,
    pub height_mm: i32,
    pub name: Option<String>,
}

/// Shared storage for monitor info accessible from both threads.
/// The Condvar is notified once monitors have been populated.
pub type SharedMonitorInfo = Arc<(Mutex<Vec<MonitorInfo>>, std::sync::Condvar)>;

/// Render thread state
pub struct RenderThread {
    handle: Option<JoinHandle<()>>,
}

impl RenderThread {
    /// Spawn the render thread
    pub fn spawn(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_dimensions: SharedImageDimensions,
        shared_monitors: SharedMonitorInfo,
        #[cfg(feature = "neo-term")]
        shared_terminals: crate::terminal::SharedTerminals,
    ) -> Self {
        let handle = thread::spawn(move || {
            run_render_loop(
                comms, width, height, title, image_dimensions,
                shared_monitors,
                #[cfg(feature = "neo-term")]
                shared_terminals,
            );
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


#[cfg(feature = "wpe-webkit")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebKitImportPolicy {
    /// Prefer raw pixel upload first, fallback to DMA-BUF.
    PixelsFirst,
    /// Prefer DMA-BUF import first, fallback to raw pixels.
    DmaBufFirst,
    /// Default compatibility mode (currently PixelsFirst).
    Auto,
}

#[cfg(feature = "wpe-webkit")]
impl WebKitImportPolicy {
    fn from_env() -> Self {
        match std::env::var("NEOMACS_WEBKIT_IMPORT").ok().as_deref() {
            Some("dmabuf-first") | Some("dmabuf") | Some("dma-buf-first") => {
                log::info!("NEOMACS_WEBKIT_IMPORT=dmabuf-first");
                Self::DmaBufFirst
            }
            Some("pixels-first") | Some("pixels") => {
                log::info!("NEOMACS_WEBKIT_IMPORT=pixels-first");
                Self::PixelsFirst
            }
            Some("auto") => {
                log::info!("NEOMACS_WEBKIT_IMPORT=auto (effective: pixels-first)");
                Self::Auto
            }
            Some(val) => {
                log::warn!(
                    "NEOMACS_WEBKIT_IMPORT={}: unrecognized value, defaulting to auto (effective: pixels-first)",
                    val
                );
                Self::Auto
            }
            None => {
                log::info!("NEOMACS_WEBKIT_IMPORT not set (effective: pixels-first)");
                Self::Auto
            }
        }
    }

    fn effective(self) -> Self {
        match self {
            Self::Auto => Self::PixelsFirst,
            other => other,
        }
    }
}



/// FPS counter and frame time tracking state.
struct FpsCounter {
    enabled: bool,
    last_instant: std::time::Instant,
    frame_count: u32,
    display_value: f32,
    frame_time_ms: f32,
    render_start: std::time::Instant,
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            enabled: false,
            last_instant: std::time::Instant::now(),
            frame_count: 0,
            display_value: 0.0,
            frame_time_ms: 0.0,
            render_start: std::time::Instant::now(),
        }
    }
}

/// Borderless window chrome state (title bar, resize edges, decorations).
struct WindowChrome {
    decorations_enabled: bool,
    resize_edge: Option<winit::window::ResizeDirection>,
    title: String,
    titlebar_height: f32,
    titlebar_hover: u32,
    last_titlebar_click: std::time::Instant,
    is_fullscreen: bool,
    corner_radius: f32,
}

impl Default for WindowChrome {
    fn default() -> Self {
        Self {
            decorations_enabled: true,
            resize_edge: None,
            title: String::from("neomacs"),
            titlebar_height: 30.0,
            titlebar_hover: 0,
            last_titlebar_click: std::time::Instant::now(),
            is_fullscreen: false,
            corner_radius: 0.0,
        }
    }
}

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

    // Display scale factor (physical pixels / logical pixels)
    scale_factor: f64,

    // Current modifier state (NEOMACS_*_MASK flags)
    modifiers: u32,

    // Last known cursor position
    mouse_pos: (f32, f32),
    /// Whether the mouse cursor is hidden during keyboard input
    mouse_hidden_for_typing: bool,

    // Shared image dimensions (written here, read from main thread)
    image_dimensions: SharedImageDimensions,

    // Frame dirty flag: set when new frame data arrives, cleared after render
    frame_dirty: bool,

    // Cursor state (blink, animation, size transition)
    cursor: CursorState,

    // All visual effect configurations
    effects: crate::effect_config::EffectsConfig,

    // Window transition state (crossfade, scroll)
    transitions: TransitionState,

    // WebKit state (video cache is managed by renderer)
    #[cfg(feature = "wpe-webkit")]
    wpe_backend: Option<WpeBackend>,

    #[cfg(feature = "wpe-webkit")]
    webkit_views: HashMap<u32, WpeWebView>,

    #[cfg(feature = "wpe-webkit")]
    webkit_import_policy: WebKitImportPolicy,

    // Floating WebKit overlays (position/size from C side, rendered on render thread)
    #[cfg(feature = "wpe-webkit")]
    floating_webkits: Vec<crate::core::scene::FloatingWebKit>,

    // Terminal manager (neo-term)
    #[cfg(feature = "neo-term")]
    terminal_manager: crate::terminal::TerminalManager,
    #[cfg(feature = "neo-term")]
    shared_terminals: crate::terminal::SharedTerminals,

    // Multi-window manager (secondary OS windows for top-level frames)
    multi_windows: multi_window::MultiWindowManager,
    // wgpu adapter (needed for creating surfaces on new windows)
    adapter: Option<wgpu::Adapter>,

    // Child frames (posframe, which-key-posframe, etc.)
    child_frames: child_frames::ChildFrameManager,
    // Child frame visual style
    child_frame_corner_radius: f32,
    child_frame_shadow_enabled: bool,
    child_frame_shadow_layers: u32,
    child_frame_shadow_offset: f32,
    child_frame_shadow_opacity: f32,

    // Active popup menu (shown by x-popup-menu)
    popup_menu: Option<PopupMenuState>,

    // Active tooltip overlay
    tooltip: Option<TooltipState>,

    // Visual bell state (flash overlay)
    visual_bell_start: Option<std::time::Instant>,

    // IME state
    ime_enabled: bool,
    ime_preedit_active: bool,
    ime_preedit_text: String,

    // UI overlay state
    scroll_indicators_enabled: bool,

    // Window chrome (borderless title bar, resize, decorations)
    chrome: WindowChrome,
    // FPS counter state
    fps: FpsCounter,
    /// Extra line spacing in pixels (added between rows)
    extra_line_spacing: f32,
    /// Extra letter spacing in pixels (added between characters)
    extra_letter_spacing: f32,
    prev_selected_window_id: i64,
    prev_background: Option<(f32, f32, f32, f32)>,
    last_activity_time: std::time::Instant,
    idle_dim_current_alpha: f32, // current dimming alpha 0.0 (none) to opacity (full)
    idle_dim_active: bool,       // true when dimmed or fading
    /// Key press timestamps for WPM calculation
    key_press_times: Vec<std::time::Instant>,
    /// Smoothed WPM value for display
    displayed_wpm: f32,

    /// Shared monitor info (populated in resumed(), read from FFI thread)
    shared_monitors: Option<SharedMonitorInfo>,
    monitors_populated: bool,
}

impl RenderApp {
    fn new(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_dimensions: SharedImageDimensions,
        shared_monitors: SharedMonitorInfo,
        #[cfg(feature = "neo-term")]
        shared_terminals: crate::terminal::SharedTerminals,
    ) -> Self {
        #[cfg(feature = "wpe-webkit")]
        let webkit_import_policy = WebKitImportPolicy::from_env();

        Self {
            comms,
            window: None,
            current_frame: None,
            width,
            height,
            title,
            scale_factor: 1.0,
            renderer: None,
            surface: None,
            surface_config: None,
            device: None,
            queue: None,
            glyph_atlas: None,
            faces: HashMap::new(),
            modifiers: 0,
            mouse_pos: (0.0, 0.0),
            mouse_hidden_for_typing: false,
            image_dimensions,
            frame_dirty: false,
            cursor: CursorState::default(),
            effects: crate::effect_config::EffectsConfig::default(),
            transitions: TransitionState::default(),
            #[cfg(feature = "wpe-webkit")]
            wpe_backend: None,
            #[cfg(feature = "wpe-webkit")]
            webkit_views: HashMap::new(),
            #[cfg(feature = "wpe-webkit")]
            webkit_import_policy,
            #[cfg(feature = "wpe-webkit")]
            floating_webkits: Vec::new(),
            #[cfg(feature = "neo-term")]
            terminal_manager: crate::terminal::TerminalManager::new(),
            #[cfg(feature = "neo-term")]
            shared_terminals,
            multi_windows: multi_window::MultiWindowManager::new(),
            adapter: None,
            child_frames: child_frames::ChildFrameManager::new(),
            child_frame_corner_radius: 8.0,
            child_frame_shadow_enabled: true,
            child_frame_shadow_layers: 4,
            child_frame_shadow_offset: 2.0,
            child_frame_shadow_opacity: 0.3,
            popup_menu: None,
            tooltip: None,
            visual_bell_start: None,
            ime_enabled: false,
            ime_preedit_active: false,
            ime_preedit_text: String::new(),
            scroll_indicators_enabled: true,
            chrome: WindowChrome::default(),
            fps: FpsCounter::default(),
            extra_line_spacing: 0.0,
            extra_letter_spacing: 0.0,
            prev_selected_window_id: 0,
            key_press_times: Vec::new(),
            displayed_wpm: 0.0,
            prev_background: None,
            last_activity_time: std::time::Instant::now(),
            idle_dim_current_alpha: 0.0,
            idle_dim_active: false,

            shared_monitors: Some(shared_monitors),
            monitors_populated: false,
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
            power_preference: crate::gpu_power_preference(),
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

        // Prefer PreMultiplied alpha for window transparency support
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            caps.alpha_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: self.width,
            height: self.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create renderer with existing device and surface format
        let renderer = WgpuRenderer::with_device(
            device.clone(), queue.clone(),
            self.width, self.height,
            format,
            self.scale_factor as f32,
        );

        // Create glyph atlas with scale factor for crisp HiDPI text
        let glyph_atlas = WgpuGlyphAtlas::new_with_scale(&device, self.scale_factor as f32);

        log::info!(
            "wgpu initialized: {}x{}, format: {:?}",
            self.width,
            self.height,
            format
        );

        self.adapter = Some(adapter);
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
        }

        // All GPU caches (image, video, webkit) are managed by the renderer
        #[cfg(feature = "video")]
        log::info!("Video cache initialized");
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

        // Invalidate offscreen textures (they reference old size)
        self.transitions.offscreen_a = None;
        self.transitions.offscreen_b = None;
        // Cancel active transitions (they reference old-sized textures)
        self.transitions.crossfades.clear();
        self.transitions.scroll_slides.clear();

        // Trigger resize padding transition
        if self.effects.resize_padding.enabled {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.trigger_resize_padding(std::time::Instant::now());
            }
        }

        // Force immediate re-render with old frame at new surface size.
        // Ensures the window always shows content during resize
        // (background fills new area, old glyphs stay at their positions).
        self.frame_dirty = true;

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
                RenderCommand::ScrollBlit { .. } => {
                    // No-op: scroll blitting is no longer needed with full-frame rendering.
                    // The entire frame is rebuilt from current_matrix each time.
                    log::debug!("ScrollBlit ignored (full-frame rendering mode)");
                }
                RenderCommand::ImageLoadFile { id, path, max_width, max_height } => {
                    log::info!("Loading image {}: {} (max {}x{})", id, path, max_width, max_height);
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.load_image_file_with_id(id, &path, max_width, max_height);
                        // Get dimensions and notify Emacs
                        if let Some((w, h)) = renderer.get_image_size(id) {
                            // Store in shared map for main thread to read
                            if let Ok(mut dims) = self.image_dimensions.lock() {
                                dims.insert(id, (w, h));
                            }
                            // Send event to Emacs so it can trigger redisplay
                            self.comms.send_input(InputEvent::ImageDimensionsReady {
                                id,
                                width: w,
                                height: h,
                            });
                            log::debug!("Sent ImageDimensionsReady for image {}: {}x{}", id, w, h);
                        }
                    } else {
                        log::warn!("Renderer not initialized, cannot load image {}", id);
                    }
                }
                RenderCommand::ImageLoadArgb32 { id, data, width, height, stride } => {
                    log::debug!("Loading ARGB32 image {}: {}x{} stride={}", id, width, height, stride);
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.load_image_argb32_with_id(id, &data, width, height, stride);
                        if let Some((w, h)) = renderer.get_image_size(id) {
                            if let Ok(mut dims) = self.image_dimensions.lock() {
                                dims.insert(id, (w, h));
                            }
                        }
                    }
                }
                RenderCommand::ImageLoadRgb24 { id, data, width, height, stride } => {
                    log::debug!("Loading RGB24 image {}: {}x{} stride={}", id, width, height, stride);
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.load_image_rgb24_with_id(id, &data, width, height, stride);
                        if let Some((w, h)) = renderer.get_image_size(id) {
                            if let Ok(mut dims) = self.image_dimensions.lock() {
                                dims.insert(id, (w, h));
                            }
                        }
                    }
                }
                RenderCommand::ImageFree { id } => {
                    log::debug!("Freeing image {}", id);
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.free_image(id);
                    }
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
                        // Clean up the renderer's webkit cache
                        if let Some(ref mut renderer) = self.renderer {
                            renderer.remove_webkit_view(id);
                        }
                    }
                }
                RenderCommand::WebKitClick { id, x, y, button } => {
                    log::debug!("WebKit click view {} at ({}, {}), button {}", id, x, y, button);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.click(x, y, button);
                    }
                }
                RenderCommand::WebKitPointerEvent { id, event_type, x, y, button, state, modifiers } => {
                    log::trace!("WebKit pointer event view {} type {} at ({}, {})", id, event_type, x, y);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.send_pointer_event(event_type, x, y, button, state, modifiers);
                    }
                }
                RenderCommand::WebKitScroll { id, x, y, delta_x, delta_y } => {
                    log::debug!("WebKit scroll view {} at ({}, {}), delta ({}, {})", id, x, y, delta_x, delta_y);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.scroll(x, y, delta_x, delta_y);
                    }
                }
                RenderCommand::WebKitKeyEvent { id, keyval, keycode, pressed, modifiers } => {
                    log::debug!("WebKit key event view {} keyval {} pressed {}", id, keyval, pressed);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.send_keyboard_event(keyval, keycode, pressed, modifiers);
                    }
                }
                RenderCommand::WebKitGoBack { id } => {
                    log::info!("WebKit go back view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        let _ = view.go_back();
                    }
                }
                RenderCommand::WebKitGoForward { id } => {
                    log::info!("WebKit go forward view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        let _ = view.go_forward();
                    }
                }
                RenderCommand::WebKitReload { id } => {
                    log::info!("WebKit reload view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        let _ = view.reload();
                    }
                }
                RenderCommand::WebKitExecuteJavaScript { id, script } => {
                    log::debug!("WebKit execute JS view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        let _ = view.execute_javascript(&script);
                    }
                }
                RenderCommand::WebKitSetFloating { id, x, y, width, height } => {
                    log::info!("WebKit set floating: id={} at ({},{}) {}x{}", id, x, y, width, height);
                    #[cfg(feature = "wpe-webkit")]
                    {
                        self.floating_webkits.retain(|w| w.webkit_id != id);
                        self.floating_webkits.push(crate::core::scene::FloatingWebKit {
                            webkit_id: id, x, y, width, height,
                        });
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::WebKitRemoveFloating { id } => {
                    log::info!("WebKit remove floating: id={}", id);
                    #[cfg(feature = "wpe-webkit")]
                    {
                        self.floating_webkits.retain(|w| w.webkit_id != id);
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::VideoCreate { id, path } => {
                    log::info!("Loading video {}: {}", id, path);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        let video_id = renderer.load_video_file(&path);
                        log::info!("Video loaded with id {} (requested id was {})", video_id, id);
                    }
                }
                RenderCommand::VideoPlay { id } => {
                    log::debug!("Playing video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.video_play(id);
                    }
                }
                RenderCommand::VideoPause { id } => {
                    log::debug!("Pausing video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.video_pause(id);
                    }
                }
                RenderCommand::VideoDestroy { id } => {
                    log::info!("Destroying video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.video_stop(id);
                    }
                }
                RenderCommand::SetMouseCursor { cursor_type } => {
                    if let Some(ref window) = self.window {
                        if cursor_type == 0 {
                            // Hidden/invisible cursor
                            window.set_cursor_visible(false);
                        } else {
                            use winit::window::CursorIcon;
                            window.set_cursor_visible(true);
                            let icon = match cursor_type {
                                2 => CursorIcon::Text,       // I-beam
                                3 => CursorIcon::Pointer,    // Hand/pointer
                                4 => CursorIcon::Crosshair,
                                5 => CursorIcon::EwResize,   // Horizontal resize
                                6 => CursorIcon::NsResize,   // Vertical resize
                                7 => CursorIcon::Wait,       // Hourglass
                                8 => CursorIcon::NwseResize, // NW-SE (top-left/bottom-right)
                                9 => CursorIcon::NeswResize, // NE-SW (top-right/bottom-left)
                                10 => CursorIcon::NeswResize,
                                11 => CursorIcon::NwseResize,
                                _ => CursorIcon::Default,    // Arrow
                            };
                            window.set_cursor(icon);
                        }
                    }
                }
                RenderCommand::WarpMouse { x, y } => {
                    if let Some(ref window) = self.window {
                        use winit::dpi::PhysicalPosition;
                        let pos = PhysicalPosition::new(x as f64, y as f64);
                        let _ = window.set_cursor_position(pos);
                    }
                }
                RenderCommand::SetWindowTitle { title } => {
                    self.chrome.title = title.clone();
                    if let Some(ref window) = self.window {
                        window.set_title(&title);
                    }
                    if !self.chrome.decorations_enabled {
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::SetWindowFullscreen { mode } => {
                    if let Some(ref window) = self.window {
                        use winit::window::Fullscreen;
                        match mode {
                            3 => {
                                // FULLSCREEN_BOTH: borderless fullscreen
                                window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                                self.chrome.is_fullscreen = true;
                            }
                            4 => {
                                // FULLSCREEN_MAXIMIZED
                                window.set_maximized(true);
                                self.chrome.is_fullscreen = false;
                            }
                            _ => {
                                // FULLSCREEN_NONE or partial: exit fullscreen
                                window.set_fullscreen(None);
                                window.set_maximized(false);
                                self.chrome.is_fullscreen = false;
                            }
                        }
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::SetWindowMinimized { minimized } => {
                    if let Some(ref window) = self.window {
                        window.set_minimized(minimized);
                    }
                }
                RenderCommand::SetWindowPosition { x, y } => {
                    if let Some(ref window) = self.window {
                        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    }
                }
                RenderCommand::SetWindowSize { width, height } => {
                    if let Some(ref window) = self.window {
                        // Emacs sends logical pixel dimensions
                        let size = winit::dpi::LogicalSize::new(width, height);
                        let _ = window.request_inner_size(size);
                    }
                }
                RenderCommand::SetWindowDecorated { decorated } => {
                    self.chrome.decorations_enabled = decorated;
                    if let Some(ref window) = self.window {
                        window.set_decorations(decorated);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorBlink { enabled, interval_ms } => {
                    log::debug!("Cursor blink: enabled={}, interval={}ms", enabled, interval_ms);
                    self.cursor.blink_enabled = enabled;
                    self.cursor.blink_interval = std::time::Duration::from_millis(interval_ms as u64);
                    if !enabled {
                        self.cursor.blink_on = true;
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::SetCursorAnimation { enabled, speed } => {
                    log::debug!("Cursor animation: enabled={}, speed={}", enabled, speed);
                    self.cursor.anim_enabled = enabled;
                    self.cursor.anim_speed = speed;
                    if !enabled {
                        self.cursor.animating = false;
                    }
                }
                RenderCommand::SetAnimationConfig {
                    cursor_enabled, cursor_speed,
                    cursor_style, cursor_duration_ms,
                    crossfade_enabled, crossfade_duration_ms,
                    scroll_enabled, scroll_duration_ms,
                    scroll_effect, scroll_easing,
                    trail_size,
                    crossfade_effect, crossfade_easing,
                } => {
                    use crate::core::scroll_animation::{ScrollEffect, ScrollEasing};
                    let effect = ScrollEffect::ALL.get(scroll_effect as usize)
                        .copied().unwrap_or(ScrollEffect::Slide);
                    let easing = match scroll_easing {
                        0 => ScrollEasing::EaseOutQuad,
                        1 => ScrollEasing::EaseOutCubic,
                        2 => ScrollEasing::Spring,
                        3 => ScrollEasing::Linear,
                        4 => ScrollEasing::EaseInOutCubic,
                        _ => ScrollEasing::EaseOutQuad,
                    };
                    let cf_effect = ScrollEffect::ALL.get(crossfade_effect as usize)
                        .copied().unwrap_or(ScrollEffect::Crossfade);
                    let cf_easing = match crossfade_easing {
                        0 => ScrollEasing::EaseOutQuad,
                        1 => ScrollEasing::EaseOutCubic,
                        2 => ScrollEasing::Spring,
                        3 => ScrollEasing::Linear,
                        4 => ScrollEasing::EaseInOutCubic,
                        _ => ScrollEasing::EaseOutQuad,
                    };
                    log::debug!("Animation config: cursor={}/{}/style={:?}/{}ms/trail={}, crossfade={}/{}ms/effect={:?}/easing={:?}, scroll={}/{}ms/effect={:?}/easing={:?}",
                        cursor_enabled, cursor_speed, cursor_style, cursor_duration_ms, trail_size,
                        crossfade_enabled, crossfade_duration_ms, cf_effect, cf_easing,
                        scroll_enabled, scroll_duration_ms, effect, easing);
                    self.cursor.anim_enabled = cursor_enabled;
                    self.cursor.anim_speed = cursor_speed;
                    self.cursor.anim_style = cursor_style;
                    self.cursor.anim_duration = cursor_duration_ms as f32 / 1000.0;
                    self.cursor.trail_size = trail_size.clamp(0.0, 1.0);
                    self.transitions.crossfade_enabled = crossfade_enabled;
                    self.transitions.crossfade_duration = std::time::Duration::from_millis(crossfade_duration_ms as u64);
                    self.transitions.crossfade_effect = cf_effect;
                    self.transitions.crossfade_easing = cf_easing;
                    self.transitions.scroll_enabled = scroll_enabled;
                    self.transitions.scroll_duration = std::time::Duration::from_millis(scroll_duration_ms as u64);
                    self.transitions.scroll_effect = effect;
                    self.transitions.scroll_easing = easing;
                    if !cursor_enabled {
                        self.cursor.animating = false;
                    }
                    if !crossfade_enabled {
                        self.transitions.crossfades.clear();
                    }
                    if !scroll_enabled {
                        self.transitions.scroll_slides.clear();
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalCreate { id, cols, rows, mode, shell } => {
                    let term_mode = match mode {
                        1 => crate::terminal::TerminalMode::Inline,
                        2 => crate::terminal::TerminalMode::Floating,
                        _ => crate::terminal::TerminalMode::Window,
                    };
                    match crate::terminal::TerminalView::new(
                        id, cols, rows, term_mode, shell.as_deref(),
                    ) {
                        Ok(view) => {
                            // Register term Arc in shared map for cross-thread access
                            if let Ok(mut shared) = self.shared_terminals.lock() {
                                shared.insert(id, view.term.clone());
                            }
                            self.terminal_manager.terminals.insert(id, view);
                            log::info!("Terminal {} created ({}x{}, {:?})", id, cols, rows, term_mode);
                        }
                        Err(e) => {
                            log::error!("Failed to create terminal {}: {}", id, e);
                        }
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalWrite { id, data } => {
                    if let Some(view) = self.terminal_manager.get_mut(id) {
                        if let Err(e) = view.write(&data) {
                            log::warn!("Terminal {} write error: {}", id, e);
                        }
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalResize { id, cols, rows } => {
                    if let Some(view) = self.terminal_manager.get_mut(id) {
                        view.resize(cols, rows);
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalDestroy { id } => {
                    if let Ok(mut shared) = self.shared_terminals.lock() {
                        shared.remove(&id);
                    }
                    self.terminal_manager.destroy(id);
                    log::info!("Terminal {} destroyed", id);
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalSetFloat { id, x, y, opacity } => {
                    if let Some(view) = self.terminal_manager.get_mut(id) {
                        view.float_x = x;
                        view.float_y = y;
                        view.float_opacity = opacity;
                    }
                }
                RenderCommand::ShowPopupMenu { x, y, items, title, fg, bg } => {
                    log::info!("ShowPopupMenu at ({}, {}) with {} items", x, y, items.len());
                    let (fs, lh) = self.glyph_atlas.as_ref()
                        .map(|a| (a.default_font_size(), a.default_line_height()))
                        .unwrap_or((13.0, 17.0));
                    let mut menu = PopupMenuState::new(x, y, items, title, fs, lh);
                    menu.face_fg = fg;
                    menu.face_bg = bg;
                    self.popup_menu = Some(menu);
                    self.frame_dirty = true;
                }
                RenderCommand::HidePopupMenu => {
                    log::info!("HidePopupMenu");
                    self.popup_menu = None;
                    self.frame_dirty = true;
                }
                RenderCommand::ShowTooltip { x, y, text, fg_r, fg_g, fg_b, bg_r, bg_g, bg_b } => {
                    log::debug!("ShowTooltip at ({}, {})", x, y);
                    let (fs, lh) = self.glyph_atlas.as_ref()
                        .map(|a| (a.default_font_size(), a.default_line_height()))
                        .unwrap_or((13.0, 17.0));
                    self.tooltip = Some(TooltipState::new(
                        x, y, &text,
                        (fg_r, fg_g, fg_b),
                        (bg_r, bg_g, bg_b),
                        self.width as f32 / self.scale_factor as f32,
                        self.height as f32 / self.scale_factor as f32,
                        fs, lh,
                    ));
                    self.frame_dirty = true;
                }
                RenderCommand::HideTooltip => {
                    log::debug!("HideTooltip");
                    self.tooltip = None;
                    self.frame_dirty = true;
                }
                RenderCommand::VisualBell => {
                    self.visual_bell_start = Some(std::time::Instant::now());
                    // Trigger cursor error pulse if enabled
                    if self.effects.cursor_error_pulse.enabled {
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.trigger_cursor_error_pulse(std::time::Instant::now());
                        }
                    }
                    // Trigger edge snap indicator if enabled
                    if self.effects.edge_snap.enabled {
                        if let Some(ref frame) = self.current_frame {
                            // Find selected window and check boundary
                            for info in &frame.window_infos {
                                if info.selected && !info.is_minibuffer {
                                    let at_top = info.window_start <= 1;
                                    let at_bottom = info.window_end >= info.buffer_size;
                                    if at_top || at_bottom {
                                        if let Some(renderer) = self.renderer.as_mut() {
                                            renderer.trigger_edge_snap(
                                                info.bounds,
                                                info.mode_line_height,
                                                at_top,
                                                at_bottom,
                                                std::time::Instant::now(),
                                            );
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::RequestAttention { urgent } => {
                    if let Some(ref window) = self.window {
                        let attention = if urgent {
                            Some(winit::window::UserAttentionType::Critical)
                        } else {
                            Some(winit::window::UserAttentionType::Informational)
                        };
                        window.request_user_attention(attention);
                    }
                }
                RenderCommand::UpdateEffect(updater) => {
                    (updater.0)(&mut self.effects);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.effects = self.effects.clone();
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetScrollIndicators { enabled } => {
                    self.scroll_indicators_enabled = enabled;
                    self.frame_dirty = true;
                }
                RenderCommand::SetTitlebarHeight { height } => {
                    self.chrome.titlebar_height = height;
                    self.frame_dirty = true;
                }
                RenderCommand::SetShowFps { enabled } => {
                    self.fps.enabled = enabled;
                    self.frame_dirty = true;
                }
                RenderCommand::SetCornerRadius { radius } => {
                    self.chrome.corner_radius = radius;
                    self.frame_dirty = true;
                }
                RenderCommand::SetExtraSpacing { line_spacing, letter_spacing } => {
                    self.extra_line_spacing = line_spacing;
                    self.extra_letter_spacing = letter_spacing;
                    self.frame_dirty = true;
                }
                RenderCommand::SetIndentGuideRainbow {
                    enabled, colors,
                } => {
                    // Convert sRGB colors to linear for GPU rendering
                    let linear_colors: Vec<(f32, f32, f32, f32)> = colors.iter().map(|(r, g, b, a)| {
                        let c = crate::core::types::Color::new(*r, *g, *b, *a).srgb_to_linear();
                        (c.r, c.g, c.b, c.a)
                    }).collect();
                    self.effects.indent_guides.rainbow_enabled = enabled;
                    self.effects.indent_guides.rainbow_colors = linear_colors.clone();
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_indent_guide_rainbow(enabled, linear_colors);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorSizeTransition { enabled, duration_ms } => {
                    self.cursor.size_transition_enabled = enabled;
                    self.cursor.size_transition_duration = duration_ms as f32 / 1000.0;
                    if !enabled {
                        self.cursor.size_animating = false;
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetLigaturesEnabled { enabled } => {
                    log::info!("Ligatures enabled: {}", enabled);
                    // Ligatures are handled by the layout engine (Emacs thread),
                    // not the render thread. The flag is stored on
                    // NeomacsDisplay/LayoutEngine via a separate static.
                    // This command is a no-op on the render thread but we log it.
                }
                RenderCommand::RemoveChildFrame { frame_id } => {
                    log::info!("Removing child frame 0x{:x}", frame_id);
                    self.child_frames.remove_frame(frame_id);
                    self.frame_dirty = true;
                }
                RenderCommand::SetChildFrameStyle {
                    corner_radius, shadow_enabled, shadow_layers,
                    shadow_offset, shadow_opacity,
                } => {
                    self.child_frame_corner_radius = corner_radius;
                    self.child_frame_shadow_enabled = shadow_enabled;
                    self.child_frame_shadow_layers = shadow_layers;
                    self.child_frame_shadow_offset = shadow_offset;
                    self.child_frame_shadow_opacity = shadow_opacity;
                    self.frame_dirty = true;
                }
                RenderCommand::CreateWindow { emacs_frame_id, width, height, title } => {
                    log::info!("CreateWindow request: frame_id=0x{:x} {}x{} \"{}\"",
                        emacs_frame_id, width, height, title);
                    self.multi_windows.request_create(emacs_frame_id, width, height, title);
                    // Actual creation happens in about_to_wait() with ActiveEventLoop
                }
                RenderCommand::DestroyWindow { emacs_frame_id } => {
                    log::info!("DestroyWindow request: frame_id=0x{:x}", emacs_frame_id);
                    self.multi_windows.request_destroy(emacs_frame_id);
                }
            }
        }

        should_exit
    }

    /// Get latest frame from Emacs (non-blocking)
    fn poll_frame(&mut self) {
        // Get the newest frame, discarding older ones
        // Route child frames to the child frame manager, root frames to current_frame
        // Secondary windows route to multi_windows manager
        self.child_frames.tick();
        while let Ok(frame) = self.comms.frame_rx.try_recv() {
            // Check if this frame belongs to a secondary window
            let frame_id = frame.frame_id;
            let parent_id = frame.parent_id;

            // Try routing to secondary windows first (by frame_id)
            if frame_id != 0 && parent_id == 0 && self.multi_windows.windows.contains_key(&frame_id) {
                self.multi_windows.route_frame(frame);
                continue;
            }
            // Try routing child frames to secondary windows (by parent_id)
            if parent_id != 0 && self.multi_windows.windows.contains_key(&parent_id) {
                self.multi_windows.route_frame(frame);
                continue;
            }

            if parent_id != 0 {
                // Child frame: store in primary window's manager
                self.child_frames.update_frame(frame);
            } else {
                // Root frame: update primary window's current_frame
                self.current_frame = Some(frame);
                // Reset blink to visible when new frame arrives (cursor just moved/redrawn)
                self.cursor.reset_blink();
            }
            self.frame_dirty = true;
        }
        // Remove child frames not updated for 60 cycles (~1 second at 60fps)
        self.child_frames.prune_stale(60);

        // Extract active cursor target for animation
        // Scan root frame first, then child frames (only one active cursor exists)
        {
            let mut active_cursor: Option<CursorTarget> = None;

            if let Some(ref frame) = self.current_frame {
                active_cursor = frame.glyphs.iter().find_map(|g| match g {
                    FrameGlyph::Cursor { window_id, x, y, width, height, style, color }
                        if *style != 3 => Some(CursorTarget {
                            window_id: *window_id,
                            x: *x, y: *y,
                            width: *width, height: *height,
                            style: *style,
                            color: *color,
                            frame_id: 0,
                        }),
                    _ => None,
                });
            }

            // If no active cursor in root frame, check child frames
            if active_cursor.is_none() {
                for (_, entry) in &self.child_frames.frames {
                    if let Some(ct) = entry.frame.glyphs.iter().find_map(|g| match g {
                        FrameGlyph::Cursor { window_id, x, y, width, height, style, color }
                            if *style != 3 => Some(CursorTarget {
                                window_id: *window_id,
                                x: *x, y: *y,
                                width: *width, height: *height,
                                style: *style,
                                color: *color,
                                frame_id: entry.frame_id,
                            }),
                        _ => None,
                    }) {
                        active_cursor = Some(ct);
                        break;
                    }
                }
            }

            if let Some(new_target) = active_cursor {
                let had_target = self.cursor.target.is_some();
                let target_moved = self.cursor.target.as_ref().map_or(true, |old| {
                    (old.x - new_target.x).abs() > 0.5
                    || (old.y - new_target.y).abs() > 0.5
                    || (old.width - new_target.width).abs() > 0.5
                    || (old.height - new_target.height).abs() > 0.5
                });

                if !had_target || !self.cursor.anim_enabled {
                    // First appearance or animation disabled: snap
                    self.cursor.current_x = new_target.x;
                    self.cursor.current_y = new_target.y;
                    self.cursor.current_w = new_target.width;
                    self.cursor.current_h = new_target.height;
                    self.cursor.animating = false;
                    // Snap corner springs to target corners
                    let corners = CursorState::target_corners(&new_target);
                    for i in 0..4 {
                        self.cursor.corner_springs[i].x = corners[i].0;
                        self.cursor.corner_springs[i].y = corners[i].1;
                        self.cursor.corner_springs[i].vx = 0.0;
                        self.cursor.corner_springs[i].vy = 0.0;
                        self.cursor.corner_springs[i].target_x = corners[i].0;
                        self.cursor.corner_springs[i].target_y = corners[i].1;
                    }
                    self.cursor.prev_target_cx = new_target.x + new_target.width / 2.0;
                    self.cursor.prev_target_cy = new_target.y + new_target.height / 2.0;
                } else if target_moved {
                    let now = std::time::Instant::now();
                    self.cursor.animating = true;
                    self.cursor.last_anim_time = now;
                    // Capture start position for easing/linear/spring styles
                    self.cursor.start_x = self.cursor.current_x;
                    self.cursor.start_y = self.cursor.current_y;
                    self.cursor.start_w = self.cursor.current_w;
                    self.cursor.start_h = self.cursor.current_h;
                    self.cursor.anim_start_time = now;
                    // For spring: reset velocities
                    self.cursor.velocity_x = 0.0;
                    self.cursor.velocity_y = 0.0;
                    self.cursor.velocity_w = 0.0;
                    self.cursor.velocity_h = 0.0;

                    // Set up 4-corner springs for trail effect (spring style only)
                    if self.cursor.anim_style == CursorAnimStyle::CriticallyDampedSpring {
                        let new_corners = CursorState::target_corners(&new_target);
                        let new_cx = new_target.x + new_target.width / 2.0;
                        let new_cy = new_target.y + new_target.height / 2.0;
                        let old_cx = self.cursor.prev_target_cx;
                        let old_cy = self.cursor.prev_target_cy;

                        // Travel direction (normalized)
                        let dx = new_cx - old_cx;
                        let dy = new_cy - old_cy;
                        let len = (dx * dx + dy * dy).sqrt();
                        let (dir_x, dir_y) = if len > 0.001 {
                            (dx / len, dy / len)
                        } else {
                            (1.0, 0.0)
                        };

                        // Corner direction vectors from center: TL(-1,-1), TR(1,-1), BR(1,1), BL(-1,1)
                        let corner_dirs: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

                        // Compute dot products and rank corners
                        let mut dots: [(f32, usize); 4] = corner_dirs.iter().enumerate()
                            .map(|(i, (cx, cy))| (cx * dir_x + cy * dir_y, i))
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap();
                        dots.sort_by(|a, b| a.0.total_cmp(&b.0));
                        // dots[0] = most trailing (lowest dot), dots[3] = most leading (highest dot)

                        let base_dur = self.cursor.anim_duration; // seconds
                        for (rank, &(_dot, corner_idx)) in dots.iter().enumerate() {
                            let factor = 1.0 - self.cursor.trail_size * (rank as f32 / 3.0);
                            let duration_i = (base_dur * factor).max(0.01);
                            let omega_i = 4.0 / duration_i;

                            self.cursor.corner_springs[corner_idx].target_x = new_corners[corner_idx].0;
                            self.cursor.corner_springs[corner_idx].target_y = new_corners[corner_idx].1;
                            self.cursor.corner_springs[corner_idx].omega = omega_i;
                            // Don't reset velocity — preserve momentum from in-flight animation
                        }

                        self.cursor.prev_target_cx = new_cx;
                        self.cursor.prev_target_cy = new_cy;
                    }
                }

                // Spawn typing ripple when cursor moves (if enabled)
                if target_moved && had_target && self.effects.typing_ripple.enabled {
                    if let Some(renderer) = self.renderer.as_mut() {
                        let cx = new_target.x + new_target.width / 2.0;
                        let cy = new_target.y + new_target.height / 2.0;
                        renderer.spawn_ripple(cx, cy);
                    }
                }

                // Record cursor trail fade position when cursor moves
                if target_moved && had_target && self.effects.cursor_trail_fade.enabled {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.record_cursor_trail(
                            self.cursor.current_x,
                            self.cursor.current_y,
                            self.cursor.current_w,
                            self.cursor.current_h,
                        );
                    }
                }

                // Update IME cursor area so candidate window follows text cursor
                if let Some(ref window) = self.window {
                    // If cursor is in a child frame, offset by the child's abs position
                    let (ime_off_x, ime_off_y) = if new_target.frame_id != 0 {
                        self.child_frames.frames.get(&new_target.frame_id)
                            .map(|e| (e.abs_x as f64, e.abs_y as f64))
                            .unwrap_or((0.0, 0.0))
                    } else {
                        (0.0, 0.0)
                    };
                    let x = (new_target.x as f64 + ime_off_x) * self.scale_factor;
                    let y = (new_target.y as f64 + new_target.height as f64 + ime_off_y) * self.scale_factor;
                    let w = new_target.width as f64 * self.scale_factor;
                    let h = new_target.height as f64 * self.scale_factor;
                    window.set_ime_cursor_area(
                        winit::dpi::PhysicalPosition::new(x, y),
                        winit::dpi::PhysicalSize::new(w, h),
                    );
                }

                // Detect cursor size change for smooth size transition
                if self.cursor.size_transition_enabled {
                    let dw = (new_target.width - self.cursor.size_target_w).abs();
                    let dh = (new_target.height - self.cursor.size_target_h).abs();
                    if dw > 2.0 || dh > 2.0 {
                        self.cursor.size_animating = true;
                        self.cursor.size_start_w = self.cursor.current_w;
                        self.cursor.size_start_h = self.cursor.current_h;
                        self.cursor.size_anim_start = std::time::Instant::now();
                    }
                    self.cursor.size_target_w = new_target.width;
                    self.cursor.size_target_h = new_target.height;
                }

                self.cursor.target = Some(new_target);
            }
        }
    }







    /// Update cursor blink state, returns true if blink toggled
    fn tick_cursor_blink(&mut self) -> bool {
        if !self.cursor.blink_enabled || self.current_frame.is_none() {
            return false;
        }
        // Check if any cursor exists in the current frame
        let has_cursor = self.current_frame.as_ref()
            .map(|f| f.glyphs.iter().any(|g| matches!(g, crate::core::frame_glyphs::FrameGlyph::Cursor { .. })))
            .unwrap_or(false);
        if !has_cursor {
            return false;
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.cursor.last_blink_toggle) >= self.cursor.blink_interval {
            let was_off = !self.cursor.blink_on;
            self.cursor.blink_on = !self.cursor.blink_on;
            self.cursor.last_blink_toggle = now;
            // Trigger wake animation when cursor becomes visible after blink-off
            if was_off && self.cursor.blink_on && self.effects.cursor_wake.enabled {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.trigger_cursor_wake(now);
                }
            }
            true
        } else {
            false
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
        use crate::backend::wpe::DmaBufData;
        use crate::backend::wgpu::external_buffer::DmaBufBuffer;

        // Get mutable reference to renderer - we need to update its internal webkit cache
        let renderer = match &mut self.renderer {
            Some(r) => r,
            None => {
                log::trace!("process_webkit_frames: no renderer available");
                return;
            }
        };

        if self.webkit_views.is_empty() {
            log::trace!("process_webkit_frames: no webkit views");
            return;
        }

        let policy = self.webkit_import_policy.effective();

        let try_upload_dmabuf = |renderer: &mut WgpuRenderer, view_id: u32, dmabuf: DmaBufData| -> bool {
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

            renderer.update_webkit_view_dmabuf(view_id, buffer)
        };

        for (view_id, view) in &self.webkit_views {
            match policy {
                WebKitImportPolicy::DmaBufFirst => {
                    if let Some(dmabuf) = view.take_latest_dmabuf() {
                        if try_upload_dmabuf(renderer, *view_id, dmabuf) {
                            // Discard pending pixel fallback when DMA-BUF succeeds.
                            let _ = view.take_latest_pixels();
                            log::debug!("Imported DMA-BUF for webkit view {} (dmabuf-first)", view_id);
                        } else if let Some(raw_pixels) = view.take_latest_pixels() {
                            if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                                log::debug!("Uploaded pixels for webkit view {} (dmabuf-first fallback)", view_id);
                            } else {
                                log::warn!("Both DMA-BUF and pixel upload failed for webkit view {}", view_id);
                            }
                        } else {
                            log::warn!("Both DMA-BUF import and pixel fallback unavailable for webkit view {}", view_id);
                        }
                    } else if let Some(raw_pixels) = view.take_latest_pixels() {
                        if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                            log::debug!("Uploaded pixels for webkit view {} (dmabuf-first: no dmabuf frame)", view_id);
                        }
                    }
                }
                WebKitImportPolicy::PixelsFirst | WebKitImportPolicy::Auto => {
                    // Prefer pixel upload over DMA-BUF zero-copy.
                    //
                    // wgpu's create_texture_from_hal() always inserts textures with
                    // UNINITIALIZED tracking state, causing a second UNDEFINED layout
                    // transition that discards DMA-BUF content on AMD RADV (and
                    // potentially other drivers with compressed modifiers like DCC/CCS).
                    // Until wgpu supports pre-initialized HAL textures, pixel upload
                    // via wpe_buffer_import_to_pixels() is the reliable path.
                    if let Some(raw_pixels) = view.take_latest_pixels() {
                        // Drain any pending DMA-BUF so it doesn't accumulate
                        let _ = view.take_latest_dmabuf();
                        if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                            log::debug!("Uploaded pixels for webkit view {}", view_id);
                        }
                    }
                    // DMA-BUF zero-copy fallback (only if no pixel data available)
                    else if let Some(dmabuf) = view.take_latest_dmabuf() {
                        if try_upload_dmabuf(renderer, *view_id, dmabuf) {
                            log::debug!("Imported DMA-BUF for webkit view {} (pixels-first fallback)", view_id);
                        } else if let Some(raw_pixels) = view.take_latest_pixels() {
                            if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                                log::debug!("Uploaded pixels for webkit view {} (pixels-first second fallback)", view_id);
                            } else {
                                log::warn!("Both pixel and DMA-BUF import failed for webkit view {}", view_id);
                            }
                        } else {
                            log::warn!("Both pixel and DMA-BUF import failed for webkit view {}", view_id);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", target_os = "linux")))]
    fn process_webkit_frames(&mut self) {}

    /// Process pending video frames
    #[cfg(feature = "video")]
    fn process_video_frames(&mut self) {
        log::trace!("process_video_frames called");
        if let Some(ref mut renderer) = self.renderer {
            renderer.process_pending_videos();
        }
    }

    #[cfg(not(feature = "video"))]
    fn process_video_frames(&mut self) {}

    /// Check if any video is currently playing (needs continuous rendering)
    #[cfg(feature = "video")]
    fn has_playing_videos(&self) -> bool {
        self.renderer.as_ref().map_or(false, |r| r.has_playing_videos())
    }

    #[cfg(not(feature = "video"))]
    fn has_playing_videos(&self) -> bool { false }

    /// Check if any WebKit view needs redraw
    #[cfg(feature = "wpe-webkit")]
    fn has_webkit_needing_redraw(&self) -> bool {
        self.webkit_views.values().any(|v| v.needs_redraw())
    }

    #[cfg(not(feature = "wpe-webkit"))]
    fn has_webkit_needing_redraw(&self) -> bool { false }

    /// Check if any terminal has pending content from PTY reader threads.
    #[cfg(feature = "neo-term")]
    fn has_terminal_activity(&self) -> bool {
        for view in self.terminal_manager.terminals.values() {
            if view.event_proxy.peek_wakeup() || view.dirty {
                return true;
            }
        }
        false
    }

    #[cfg(not(feature = "neo-term"))]
    fn has_terminal_activity(&self) -> bool { false }

    /// Process pending image uploads (decode → GPU texture)
    fn process_pending_images(&mut self) {
        if let Some(ref mut renderer) = self.renderer {
            renderer.process_pending_images();
        }
    }

    /// Update terminal content and expand Terminal glyphs into renderable cells.
    #[cfg(feature = "neo-term")]
    fn update_terminals(&mut self) {
        use crate::terminal::TerminalMode;

        // Get frame font metrics for terminal cell sizing.
        // These come from FRAME_COLUMN_WIDTH / FRAME_LINE_HEIGHT / FRAME_FONT->pixel_size.
        let (cell_w, cell_h, font_size, frame_w, frame_h) = if let Some(ref frame) = self.current_frame {
            (frame.char_width, frame.char_height, frame.font_pixel_size,
             frame.width, frame.height)
        } else {
            (8.0, 16.0, 14.0, self.width as f32, self.height as f32)
        };
        let ascent = cell_h * 0.8;

        // Auto-resize Window-mode terminals to fit the frame area.
        // Reserve space for mode-line (~1 row) and echo area (~1 row).
        let term_area_height = (frame_h - cell_h * 2.0).max(cell_h);
        let target_cols = (frame_w / cell_w).floor() as u16;
        let target_rows = (term_area_height / cell_h).floor() as u16;

        if target_cols > 0 && target_rows > 0 {
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get_mut(id) {
                    if view.mode != TerminalMode::Window {
                        continue;
                    }
                    // Resize if grid dimensions changed
                    if let Some(content) = view.content() {
                        if content.cols as u16 != target_cols || content.rows as u16 != target_rows {
                            view.resize(target_cols, target_rows);
                        }
                    }
                }
            }
        }

        // Update all terminal content (check for PTY data)
        self.terminal_manager.update_all();

        // Check for exited terminals and notify Emacs
        for id in self.terminal_manager.ids() {
            if let Some(view) = self.terminal_manager.get_mut(id) {
                if view.event_proxy.is_exited() && !view.exit_notified {
                    view.exit_notified = true;
                    self.comms.send_input(InputEvent::TerminalExited { id });
                }
            }
        }

        // Expand FrameGlyph::Terminal entries (placed by C redisplay) into cells
        if let Some(ref mut frame) = self.current_frame {
            let mut extra_glyphs = Vec::new();

            for glyph in &frame.glyphs {
                if let FrameGlyph::Terminal { terminal_id, x, y, width, height } = glyph {
                    if let Some(view) = self.terminal_manager.get(*terminal_id) {
                        if let Some(content) = view.content() {
                            extra_glyphs.push(FrameGlyph::Stretch {
                                x: *x, y: *y, width: *width, height: *height,
                                bg: content.default_bg, face_id: 0, is_overlay: false,
                                stipple_id: 0, stipple_fg: None,
                            });

                            Self::expand_terminal_cells(
                                content, *x, *y, cell_w, cell_h, ascent, font_size,
                                false, 1.0, &mut extra_glyphs,
                            );
                        }
                    }
                }
            }

            if !extra_glyphs.is_empty() {
                frame.glyphs.extend(extra_glyphs);
                self.frame_dirty = true;
            }
        }

        // Render Window-mode terminals as overlays covering the frame body.
        if let Some(ref mut frame) = self.current_frame {
            let mut win_glyphs = Vec::new();
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get(id) {
                    if view.mode != TerminalMode::Window {
                        continue;
                    }
                    if let Some(content) = view.content() {
                        let x = 0.0_f32;
                        let y = 0.0_f32;
                        let width = content.cols as f32 * cell_w;
                        let height = content.rows as f32 * cell_h;

                        // Terminal background
                        win_glyphs.push(FrameGlyph::Stretch {
                            x, y, width, height,
                            bg: content.default_bg, face_id: 0, is_overlay: true,
                            stipple_id: 0, stipple_fg: None,
                        });

                        Self::expand_terminal_cells(
                            content, x, y, cell_w, cell_h, ascent, font_size,
                            true, 1.0, &mut win_glyphs,
                        );
                    }
                }
            }

            if !win_glyphs.is_empty() {
                frame.glyphs.extend(win_glyphs);
                self.frame_dirty = true;
            }
        }

        // Render floating terminals
        if let Some(ref mut frame) = self.current_frame {
            let mut float_glyphs = Vec::new();
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get(id) {
                    if view.mode != TerminalMode::Floating {
                        continue;
                    }
                    if let Some(content) = view.content() {
                        let x = view.float_x;
                        let y = view.float_y;
                        let width = content.cols as f32 * cell_w;
                        let height = content.rows as f32 * cell_h;

                        let mut bg = content.default_bg;
                        bg.a = view.float_opacity;
                        float_glyphs.push(FrameGlyph::Stretch {
                            x, y, width, height, bg, face_id: 0, is_overlay: true,
                            stipple_id: 0, stipple_fg: None,
                        });

                        Self::expand_terminal_cells(
                            content, x, y, cell_w, cell_h, ascent, font_size,
                            true, view.float_opacity, &mut float_glyphs,
                        );
                    }
                }
            }

            if !float_glyphs.is_empty() {
                frame.glyphs.extend(float_glyphs);
                self.frame_dirty = true;
            }
        }
    }

    /// Expand terminal content cells into FrameGlyph entries.
    #[cfg(feature = "neo-term")]
    fn expand_terminal_cells(
        content: &crate::terminal::content::TerminalContent,
        origin_x: f32,
        origin_y: f32,
        cell_w: f32,
        cell_h: f32,
        ascent: f32,
        font_size: f32,
        is_overlay: bool,
        opacity: f32,
        out: &mut Vec<FrameGlyph>,
    ) {
        use alacritty_terminal::term::cell::Flags as CellFlags;

        for cell in &content.cells {
            let cx = origin_x + cell.col as f32 * cell_w;
            let cy = origin_y + cell.row as f32 * cell_h;

            if cell.bg != content.default_bg {
                let mut bg = cell.bg;
                bg.a *= opacity;
                out.push(FrameGlyph::Stretch {
                    x: cx, y: cy, width: cell_w, height: cell_h,
                    bg, face_id: 0, is_overlay,
                    stipple_id: 0, stipple_fg: None,
                });
            }

            if cell.c != ' ' && cell.c != '\0' {
                let mut fg = cell.fg;
                fg.a *= opacity;
                out.push(FrameGlyph::Char {
                    char: cell.c,
                    composed: None,
                    x: cx, y: cy,
                    width: cell_w, height: cell_h,
                    ascent, fg,
                    bg: None, face_id: 0,
                    font_weight: if cell.flags.contains(CellFlags::BOLD) { 700 } else { 400 },
                    italic: cell.flags.contains(CellFlags::ITALIC),
                    font_size,
                    underline: if cell.flags.contains(CellFlags::UNDERLINE) { 1 } else { 0 },
                    underline_color: None,
                    strike_through: if cell.flags.contains(CellFlags::STRIKEOUT) { 1 } else { 0 },
                    strike_through_color: None,
                    overline: 0, overline_color: None,
                    overstrike: false,
                    is_overlay,
                });
            }
        }

        // Terminal cursor
        if content.cursor.visible {
            let cx = origin_x + content.cursor.col as f32 * cell_w;
            let cy = origin_y + content.cursor.row as f32 * cell_h;
            let mut fg = content.default_fg;
            fg.a *= opacity;
            out.push(FrameGlyph::Border {
                x: cx, y: cy, width: cell_w, height: cell_h,
                color: fg,
            });
        }
    }

    /// Apply extra line spacing and letter spacing to glyph positions.
    /// Groups glyphs by Y position (rows) and applies cumulative offsets.
    fn apply_extra_spacing(
        glyphs: &mut [FrameGlyph],
        line_spacing: f32,
        letter_spacing: f32,
    ) {
        use crate::core::frame_glyphs::FrameGlyph;

        let mut last_y: f32 = f32::NEG_INFINITY;
        let mut row_index: i32 = -1;
        let mut char_in_row: i32 = 0;
        let mut last_window_y: f32 = f32::NEG_INFINITY;

        for glyph in glyphs.iter_mut() {
            match glyph {
                FrameGlyph::Char { x, y, is_overlay, .. } => {
                    if *is_overlay { continue; }
                    // Detect window boundary: Y jumps backwards
                    if *y < last_window_y - 1.0 {
                        row_index = -1;
                        last_y = f32::NEG_INFINITY;
                    }
                    last_window_y = *y;

                    if (*y - last_y).abs() > 0.5 {
                        row_index += 1;
                        char_in_row = 0;
                        last_y = *y;
                    } else {
                        char_in_row += 1;
                    }
                    *y += row_index as f32 * line_spacing;
                    *x += char_in_row as f32 * letter_spacing;
                }
                FrameGlyph::Stretch { x, y, is_overlay, .. } => {
                    if *is_overlay { continue; }
                    if *y < last_window_y - 1.0 {
                        row_index = -1;
                        last_y = f32::NEG_INFINITY;
                    }
                    last_window_y = *y;

                    if (*y - last_y).abs() > 0.5 {
                        row_index += 1;
                        char_in_row = 0;
                        last_y = *y;
                    } else {
                        char_in_row += 1;
                    }
                    *y += row_index as f32 * line_spacing;
                    *x += char_in_row as f32 * letter_spacing;
                }
                FrameGlyph::Cursor { y, x, .. } => {
                    // Apply same row-based Y offset to cursor
                    if (*y - last_y).abs() < 0.5 {
                        *y += row_index.max(0) as f32 * line_spacing;
                        *x += char_in_row as f32 * letter_spacing;
                    }
                }
                _ => {}
            }
        }
    }

    fn render(&mut self) {
        // Early return checks
        if self.current_frame.is_none()
            || self.surface.is_none()
            || self.renderer.is_none()
            || self.glyph_atlas.is_none()
        {
            return;
        }

        // FPS tracking
        if self.fps.enabled {
            self.fps.render_start = std::time::Instant::now();
            self.fps.frame_count += 1;
            let elapsed = self.fps.last_instant.elapsed();
            if elapsed.as_secs_f32() >= 1.0 {
                self.fps.display_value =
                    self.fps.frame_count as f32 / elapsed.as_secs_f32();
                self.fps.frame_count = 0;
                self.fps.last_instant = std::time::Instant::now();
            }
        }

        // Update terminals (expand terminal glyphs into renderable cells)
        #[cfg(feature = "neo-term")]
        self.update_terminals();

        // Process webkit frames (import DMA-BUF to textures)
        self.process_webkit_frames();

        // Process video frames
        self.process_video_frames();

        // Process pending image uploads (decoded images → GPU textures)
        self.process_pending_images();

        // Update faces from frame data (the frame carries the full face map
        // set by the FFI side, including box/underline/overline attributes).
        if let Some(ref frame) = self.current_frame {
            // Use full face data from frame (set by neomacs_display_set_face FFI)
            for (face_id, face) in &frame.faces {
                self.faces.insert(*face_id, face.clone());
            }
            // Also update font families from the per-glyph font cache
            for (face_id, font_family) in &frame.face_fonts {
                if let Some(face) = self.faces.get_mut(face_id) {
                    face.font_family = font_family.clone();
                }
            }
            // Build/update Face entries from per-glyph data. This handles the
            // Rust layout engine path where frame.faces is empty but per-glyph
            // font_size/bold/italic are set. Always update because face_ids can
            // be reused by Emacs for different realized faces across frames.
            for glyph in &frame.glyphs {
                if let crate::core::frame_glyphs::FrameGlyph::Char {
                    face_id, font_weight, italic, font_size, ..
                } = glyph {
                    let face = self.faces.entry(*face_id).or_insert_with(|| {
                        crate::core::face::Face::new(*face_id)
                    });
                    face.font_size = *font_size;
                    face.font_weight = *font_weight;
                    if *font_weight >= 700 {
                        face.attributes |= crate::core::face::FaceAttributes::BOLD;
                    } else {
                        face.attributes.remove(crate::core::face::FaceAttributes::BOLD);
                    }
                    if *italic {
                        face.attributes |= crate::core::face::FaceAttributes::ITALIC;
                    } else {
                        face.attributes.remove(crate::core::face::FaceAttributes::ITALIC);
                    }
                    if let Some(family) = frame.face_fonts.get(face_id) {
                        face.font_family = family.clone();
                    }
                }
            }

            // Merge box attributes from the Rust layout engine's side-channel.
            // The layout engine populates face_box_attrs instead of inserting
            // full Face objects (which causes heap corruption on the Emacs thread).
            for (face_id, box_attrs) in &frame.face_box_attrs {
                let face = self.faces.entry(*face_id).or_insert_with(|| {
                    crate::core::face::Face::new(*face_id)
                });
                face.box_type = if box_attrs.box_type == 1 {
                    crate::core::face::BoxType::Line
                } else {
                    crate::core::face::BoxType::None
                };
                face.box_color = Some(box_attrs.box_color);
                face.box_line_width = box_attrs.box_line_width;
                face.box_corner_radius = box_attrs.box_corner_radius;
                face.attributes |= crate::core::face::FaceAttributes::BOX;
            }
        }

        // Build face cache from child frame glyphs too
        for entry in self.child_frames.frames.values() {
            let child = &entry.frame;
            for (face_id, face) in &child.faces {
                self.faces.insert(*face_id, face.clone());
            }
            for (face_id, font_family) in &child.face_fonts {
                if let Some(face) = self.faces.get_mut(face_id) {
                    face.font_family = font_family.clone();
                }
            }
            for glyph in &child.glyphs {
                if let crate::core::frame_glyphs::FrameGlyph::Char {
                    face_id, font_weight, italic, font_size, ..
                } = glyph {
                    let face = self.faces.entry(*face_id).or_insert_with(|| {
                        crate::core::face::Face::new(*face_id)
                    });
                    face.font_size = *font_size;
                    face.font_weight = *font_weight;
                    if *font_weight >= 700 {
                        face.attributes |= crate::core::face::FaceAttributes::BOLD;
                    } else {
                        face.attributes.remove(crate::core::face::FaceAttributes::BOLD);
                    }
                    if *italic {
                        face.attributes |= crate::core::face::FaceAttributes::ITALIC;
                    } else {
                        face.attributes.remove(crate::core::face::FaceAttributes::ITALIC);
                    }
                    if let Some(family) = child.face_fonts.get(face_id) {
                        face.font_family = family.clone();
                    }
                }
            }
        }

        // Apply extra spacing adjustments to glyph positions
        if self.extra_line_spacing != 0.0 || self.extra_letter_spacing != 0.0 {
            if let Some(ref mut frame) = self.current_frame {
                Self::apply_extra_spacing(
                    &mut frame.glyphs,
                    self.extra_line_spacing,
                    self.extra_letter_spacing,
                );
            }
        }

        // Get surface texture
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
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

        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Build animated cursor override if applicable
        let animated_cursor = if let (true, Some(target)) =
            (self.cursor.anim_enabled, self.cursor.target.as_ref())
        {
            let corners = if self.cursor.anim_style == CursorAnimStyle::CriticallyDampedSpring
                && self.cursor.animating
            {
                Some([
                    (self.cursor.corner_springs[0].x, self.cursor.corner_springs[0].y),
                    (self.cursor.corner_springs[1].x, self.cursor.corner_springs[1].y),
                    (self.cursor.corner_springs[2].x, self.cursor.corner_springs[2].y),
                    (self.cursor.corner_springs[3].x, self.cursor.corner_springs[3].y),
                ])
            } else {
                None
            };
            Some(AnimatedCursor {
                window_id: target.window_id,
                x: self.cursor.current_x,
                y: self.cursor.current_y,
                width: self.cursor.current_w,
                height: self.cursor.current_h,
                corners,
                frame_id: target.frame_id,
            })
        } else {
            None
        };

        // Build background gradient option
        let bg_gradient = if self.effects.bg_gradient.enabled {
            Some((self.effects.bg_gradient.top, self.effects.bg_gradient.bottom))
        } else {
            None
        };

        // Check if we need offscreen rendering (for transitions)
        let need_offscreen = self.transitions.crossfade_enabled || self.transitions.scroll_enabled;

        if need_offscreen {
            // Swap: previous ← current
            self.transitions.current_is_a = !self.transitions.current_is_a;

            // Ensure offscreen textures exist
            self.ensure_offscreen_textures();

            // Render frame to current offscreen texture
            if let Some((current_view, _)) = self.current_offscreen_view_and_bg()
                .map(|(v, bg)| (v as *const wgpu::TextureView, bg))
            {
                let frame = self.current_frame.as_ref().expect("checked in render");
                let renderer = self.renderer.as_mut().expect("checked in render");
                let glyph_atlas = self.glyph_atlas.as_mut().expect("checked in render");
                renderer.set_idle_dim_alpha(self.idle_dim_current_alpha);

                // SAFETY: current_view is valid for the duration of this block
                renderer.render_frame_glyphs(
                    unsafe { &*current_view },
                    frame,
                    glyph_atlas,
                    &self.faces,
                    self.width,
                    self.height,
                    self.cursor.blink_on,
                    animated_cursor,
                    self.mouse_pos,
                    bg_gradient,
                );
            }

            // Detect transitions (compare window_infos)
            self.detect_transitions();

            // Blit current offscreen to surface
            if let Some((_, current_bg)) = self.current_offscreen_view_and_bg()
                .map(|(v, bg)| (v, bg as *const wgpu::BindGroup))
            {
                let renderer = self.renderer.as_ref().expect("checked in render");
                renderer.blit_texture_to_view(
                    unsafe { &*current_bg },
                    &surface_view,
                    self.width,
                    self.height,
                );
            }

            // Composite active transitions on top
            self.render_transitions(&surface_view);
        } else if self.effects.blur.enabled {
            // Blur path: render to blur texture, then apply blur to surface
            let renderer = self.renderer.as_mut().expect("checked in render");
            renderer.ensure_blur_textures();
            let blur_view_ptr = renderer.blur_render_target_view()
                .expect("blur textures initialized") as *const wgpu::TextureView;

            let frame = self.current_frame.as_ref().expect("checked in render");
            let glyph_atlas = self.glyph_atlas.as_mut().expect("checked in render");
            let renderer = self.renderer.as_mut().expect("checked in render");
            renderer.set_idle_dim_alpha(self.idle_dim_current_alpha);

            // Render frame content to blur texture A
            renderer.render_frame_glyphs(
                unsafe { &*blur_view_ptr },
                frame,
                glyph_atlas,
                &self.faces,
                self.width,
                self.height,
                self.cursor.blink_on,
                animated_cursor,
                self.mouse_pos,
                bg_gradient,
            );

            // Apply blur from texture A to surface
            let passes = self.effects.blur.passes;
            let radius = self.effects.blur.radius;
            let renderer = self.renderer.as_ref().expect("checked in render");
            renderer.apply_blur_from_a(&surface_view, passes, radius);
        } else {
            // Simple path: render directly to surface
            let frame = self.current_frame.as_ref().expect("checked in render");
            let renderer = self.renderer.as_mut().expect("checked in render");
            let glyph_atlas = self.glyph_atlas.as_mut().expect("checked in render");
            renderer.set_idle_dim_alpha(self.idle_dim_current_alpha);

            // Free blur textures if blur was just disabled
            renderer.free_blur_textures();

            renderer.render_frame_glyphs(
                &surface_view,
                frame,
                glyph_atlas,
                &self.faces,
                self.width,
                self.height,
                self.cursor.blink_on,
                animated_cursor,
                self.mouse_pos,
                bg_gradient,
            );
        }

        // Render child frames as floating overlays on top of the parent frame
        if !self.child_frames.is_empty() {
            for &child_id in self.child_frames.sorted_for_rendering() {
                if let Some(child_entry) = self.child_frames.frames.get(&child_id) {
                    if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                        (&self.renderer, &mut self.glyph_atlas)
                    {
                        // Pass animated cursor only if it belongs to this child frame
                        let child_anim = animated_cursor.filter(|ac| ac.frame_id == child_id);
                        renderer.render_child_frame(
                            &surface_view,
                            &child_entry.frame,
                            child_entry.abs_x,
                            child_entry.abs_y,
                            glyph_atlas,
                            &self.faces,
                            self.width,
                            self.height,
                            self.cursor.blink_on,
                            child_anim,
                            self.child_frame_corner_radius,
                            self.child_frame_shadow_enabled,
                            self.child_frame_shadow_layers,
                            self.child_frame_shadow_offset,
                            self.child_frame_shadow_opacity,
                        );
                    }
                }
            }
        }

        // Render breadcrumb/path bar overlay
        if self.effects.breadcrumb.enabled {
            if let (Some(ref mut renderer), Some(ref mut glyph_atlas), Some(ref frame)) =
                (&mut self.renderer, &mut self.glyph_atlas, &self.current_frame)
            {
                renderer.render_breadcrumbs(&surface_view, frame, glyph_atlas);
            }
        }

        // Render scroll position indicators and focus ring
        if self.scroll_indicators_enabled {
            if let (Some(ref renderer), Some(ref frame)) =
                (&self.renderer, &self.current_frame)
            {
                renderer.render_scroll_indicators(
                    &surface_view, &frame.window_infos,
                    self.width, self.height,
                );
            }
        }

        // Render window watermarks for empty/small buffers
        if self.effects.window_watermark.enabled {
            if let (Some(ref renderer), Some(ref mut glyph_atlas), Some(ref frame)) =
                (&self.renderer, &mut self.glyph_atlas, &self.current_frame)
            {
                renderer.render_window_watermarks(&surface_view, frame, glyph_atlas);
            }
        }

        // Render custom title bar when decorations are disabled (not in fullscreen)
        log::debug!("CSD state: decorations_enabled={} is_fullscreen={} titlebar_height={}",
            self.chrome.decorations_enabled, self.chrome.is_fullscreen, self.chrome.titlebar_height);
        if !self.chrome.decorations_enabled && !self.chrome.is_fullscreen && self.chrome.titlebar_height > 0.0 {
            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                let frame_bg = self.current_frame.as_ref()
                    .map(|f| (f.background.r, f.background.g, f.background.b));
                renderer.render_custom_titlebar(
                    &surface_view,
                    &self.chrome.title,
                    self.chrome.titlebar_height,
                    self.chrome.titlebar_hover,
                    frame_bg,
                    glyph_atlas,
                    self.width,
                    self.height,
                );
            }
        }

        // Render floating WebKit overlays on top of everything
        #[cfg(feature = "wpe-webkit")]
        if !self.floating_webkits.is_empty() {
            if let Some(ref renderer) = self.renderer {
                renderer.render_floating_webkits(&surface_view, &self.floating_webkits);
            }
        }

        // Render popup menu overlay (topmost layer)
        if let Some(ref menu) = self.popup_menu {
            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                renderer.render_popup_menu(&surface_view, menu, glyph_atlas, self.width, self.height);
            }
        }

        // Render tooltip overlay (above everything including popup menu)
        if let Some(ref tip) = self.tooltip {
            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                renderer.render_tooltip(&surface_view, tip, glyph_atlas, self.width, self.height);
            }
        }

        // Render IME preedit text overlay at cursor position
        if self.ime_preedit_active && !self.ime_preedit_text.is_empty() {
            if let (Some(ref renderer), Some(ref mut glyph_atlas), Some(ref target)) =
                (&self.renderer, &mut self.glyph_atlas, &self.cursor.target)
            {
                renderer.render_ime_preedit(
                    &surface_view,
                    &self.ime_preedit_text,
                    target.x,
                    target.y,
                    target.height,
                    glyph_atlas,
                    self.width,
                    self.height,
                );
            }
        }

        // Render visual bell flash overlay (above everything)
        if let Some(start) = self.visual_bell_start {
            let elapsed = start.elapsed().as_secs_f32();
            let duration = 0.15; // 150ms flash
            if elapsed < duration {
                let alpha = (1.0 - elapsed / duration) * 0.3; // max 30% opacity, fading out
                if let Some(ref renderer) = self.renderer {
                    renderer.render_visual_bell(
                        &surface_view,
                        self.width, self.height,
                        alpha,
                    );
                }
                self.frame_dirty = true; // Keep redrawing during animation
            } else {
                self.visual_bell_start = None;
            }
        }

        // Render FPS counter overlay (topmost) with profiling stats
        if self.fps.enabled {
            // Measure frame time
            let frame_time = self.fps.render_start.elapsed().as_secs_f32() * 1000.0;
            // Exponential moving average (smooth over ~10 frames)
            self.fps.frame_time_ms = self.fps.frame_time_ms * 0.9 + frame_time * 0.1;

            // Gather stats
            let glyph_count = self.current_frame.as_ref()
                .map(|f| f.glyphs.len())
                .unwrap_or(0);
            let window_count = self.current_frame.as_ref()
                .map(|f| f.window_infos.len())
                .unwrap_or(0);
            let transition_count = self.transitions.crossfades.len() + self.transitions.scroll_slides.len();

            // Build multi-line stats text
            let stats_lines = vec![
                format!("{:.0} FPS | {:.1}ms", self.fps.display_value, self.fps.frame_time_ms),
                format!("{}g {}w {}t  {}x{}", glyph_count, window_count,
                    transition_count, self.width, self.height),
            ];

            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                renderer.render_fps_overlay(
                    &surface_view,
                    &stats_lines,
                    glyph_atlas,
                    self.width,
                    self.height,
                );
            }
        }

        // Render typing speed indicator
        if self.effects.typing_speed.enabled {
            let now = std::time::Instant::now();
            let window_secs = 5.0_f64;
            // Remove key presses older than the window
            self.key_press_times.retain(|t| now.duration_since(*t).as_secs_f64() < window_secs);
            // Calculate chars/second, then WPM (5 chars per word, * 60 for minutes)
            let count = self.key_press_times.len() as f64;
            let target_wpm = if count > 1.0 {
                let span = now.duration_since(self.key_press_times[0]).as_secs_f64();
                if span > 0.1 {
                    (count / span) * 60.0 / 5.0
                } else {
                    0.0
                }
            } else {
                0.0
            };
            // Exponential smoothing
            let alpha = 0.15_f32;
            self.displayed_wpm += (target_wpm as f32 - self.displayed_wpm) * alpha;
            if self.displayed_wpm < 0.5 { self.displayed_wpm = 0.0; }

            if let (Some(ref renderer), Some(ref mut glyph_atlas), Some(ref frame)) =
                (&self.renderer, &mut self.glyph_atlas, &self.current_frame)
            {
                renderer.render_typing_speed(&surface_view, frame, glyph_atlas, self.displayed_wpm);
            }
            // Keep redrawing while WPM is decaying
            if self.displayed_wpm > 0.5 || !self.key_press_times.is_empty() {
                self.frame_dirty = true;
            }
        }

        // Render corner mask for rounded window corners (borderless only, not fullscreen)
        if !self.chrome.decorations_enabled && !self.chrome.is_fullscreen && self.chrome.corner_radius > 0.0 {
            if let Some(ref renderer) = self.renderer {
                renderer.render_corner_mask(
                    &surface_view,
                    self.chrome.corner_radius,
                    self.width,
                    self.height,
                );
            }
        }

        // Present the frame
        output.present();
    }

    /// Set the window icon from the embedded Neomacs logo PNG.
    fn set_window_icon(window: &Window) {
        let icon_bytes = include_bytes!("../../assets/logo-128.png");
        if let Ok(img) = image::load_from_memory(icon_bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            if let Ok(icon) = winit::window::Icon::from_rgba(rgba.into_raw(), w, h) {
                window.set_window_icon(Some(icon));
            }
        }
    }

}

impl ApplicationHandler for RenderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            // Use LogicalSize so winit applies the display scale
            let attrs = Window::default_attributes()
                .with_title(&self.title)
                .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
                .with_transparent(true);

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    let window = Arc::new(window);

                    // Read scale factor once at launch
                    self.scale_factor = window.scale_factor();
                    log::info!("Display scale factor: {}", self.scale_factor);

                    // Update width/height to physical pixels for surface config
                    let phys = window.inner_size();
                    self.width = phys.width;
                    self.height = phys.height;
                    log::info!("Render thread: window created (physical {}x{})", self.width, self.height);

                    // Initialize wgpu with the window
                    self.init_wgpu(window.clone());

                    // Enable IME input for CJK and compose support
                    window.set_ime_allowed(true);

                    // Set window icon from embedded Emacs icon
                    Self::set_window_icon(&window);

                    self.window = Some(window);
                }
                Err(e) => {
                    log::error!("Failed to create window: {:?}", e);
                }
            }
        }

        // Populate monitor info on first resume (requires ActiveEventLoop)
        if !self.monitors_populated {
            self.monitors_populated = true;
            if let Some(ref shared) = self.shared_monitors {
                let mut monitors = Vec::new();
                for monitor in event_loop.available_monitors() {
                    let pos = monitor.position();
                    let size = monitor.size();
                    let scale = monitor.scale_factor();
                    let name = monitor.name();
                    let width_mm = if scale > 0.0 {
                        (size.width as f64 * 25.4 / (96.0 * scale)) as i32
                    } else {
                        0
                    };
                    let height_mm = if scale > 0.0 {
                        (size.height as f64 * 25.4 / (96.0 * scale)) as i32
                    } else {
                        0
                    };
                    log::info!(
                        "Monitor: {:?} pos=({},{}) size={}x{} scale={} mm={}x{}",
                        name, pos.x, pos.y, size.width, size.height, scale, width_mm, height_mm
                    );
                    monitors.push(MonitorInfo {
                        x: pos.x,
                        y: pos.y,
                        width: size.width as i32,
                        height: size.height as i32,
                        scale,
                        width_mm,
                        height_mm,
                        name,
                    });
                }
                let (ref lock, ref cvar) = **shared;
                if let Ok(mut shared) = lock.lock() {
                    *shared = monitors;
                    cvar.notify_all();
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
                let emacs_fid = self.multi_windows.emacs_frame_for_winit(_window_id).unwrap_or(0);
                self.comms.send_input(InputEvent::WindowClose { emacs_frame_id: emacs_fid });
                if emacs_fid == 0 {
                    // Primary window closing — exit
                    event_loop.exit();
                } else {
                    // Secondary window closing — just remove it
                    self.multi_windows.request_destroy(emacs_fid);
                }
            }

            WindowEvent::Resized(size) => {
                log::info!("WindowEvent::Resized: {}x{}", size.width, size.height);

                let emacs_fid = self.multi_windows.emacs_frame_for_winit(_window_id).unwrap_or(0);
                if emacs_fid == 0 {
                    // Primary window resize
                    self.handle_resize(size.width, size.height);
                    let logical_w = (size.width as f64 / self.scale_factor) as u32;
                    let logical_h = (size.height as f64 / self.scale_factor) as u32;
                    log::info!("Sending WindowResize event to Emacs: {}x{} (logical)", logical_w, logical_h);
                    self.comms.send_input(InputEvent::WindowResize {
                        width: logical_w,
                        height: logical_h,
                        emacs_frame_id: 0,
                    });
                } else if let Some(device) = self.device.clone() {
                    // Secondary window resize
                    if let Some(ws) = self.multi_windows.get_mut(emacs_fid) {
                        ws.handle_resize(&device, size.width, size.height);
                        let scale = ws.scale_factor;
                        let logical_w = (size.width as f64 / scale) as u32;
                        let logical_h = (size.height as f64 / scale) as u32;
                        self.comms.send_input(InputEvent::WindowResize {
                            width: logical_w,
                            height: logical_h,
                            emacs_frame_id: emacs_fid,
                        });
                    }
                }
            }

            WindowEvent::Focused(focused) => {
                let emacs_fid = self.multi_windows.emacs_frame_for_winit(_window_id).unwrap_or(0);
                self.comms.send_input(InputEvent::WindowFocus { focused, emacs_frame_id: emacs_fid });
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key, state, text, ..
                    },
                ..
            } => {
                if state == ElementState::Pressed {
                    log::debug!("KeyboardInput: logical_key={:?} text={:?} ime_preedit_active={}",
                               logical_key, text, self.ime_preedit_active);
                }
                // If popup menu is active, handle keyboard navigation
                if self.popup_menu.is_some() && state == ElementState::Pressed {
                    match logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                            self.popup_menu = None;
                            self.frame_dirty = true;
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.move_hover(1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.move_hover(-1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                let panel = menu.active_panel();
                                let hi = panel.hover_index;
                                if hi >= 0 && (hi as usize) < panel.item_indices.len() {
                                    let global_idx = panel.item_indices[hi as usize];
                                    if menu.all_items[global_idx].submenu {
                                        // Open submenu instead of selecting
                                        if menu.open_submenu() {
                                            self.frame_dirty = true;
                                        }
                                    } else {
                                        self.comms.send_input(InputEvent::MenuSelection { index: global_idx as i32 });
                                        self.popup_menu = None;
                                        self.frame_dirty = true;
                                    }
                                } else {
                                    self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                                    self.popup_menu = None;
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.open_submenu() {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.close_submenu() {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::Home) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                menu.active_panel_mut().hover_index = -1;
                                if menu.move_hover(1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::End) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                let len = menu.active_panel().item_indices.len() as i32;
                                menu.active_panel_mut().hover_index = len;
                                if menu.move_hover(-1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        _ => {} // Swallow other keys
                    }
                } else if self.ime_preedit_active {
                    // When IME preedit is active, suppress character
                    // keys to avoid double input.  The committed text
                    // will arrive via Ime::Commit instead.
                    log::debug!("IME preedit active, suppressing KeyboardInput: {:?}", logical_key);
                } else {
                    // On X11, some IME backends (e.g. fcitx5 with certain XIM
                    // styles) deliver committed text via KeyboardInput's `text`
                    // field instead of Ime::Commit.  Check `text` first for
                    // multi-char or non-ASCII content that translate_key would
                    // miss (e.g. CJK characters from shuangpin input).
                    let mut handled_via_text = false;
                    if state == ElementState::Pressed {
                        if let Some(ref txt) = text {
                            let s = txt.as_str();
                            // If text contains non-ASCII or multiple chars,
                            // it's likely IME-committed text
                            if !s.is_empty() && (s.len() > 1 || s.as_bytes()[0] > 0x7f) {
                                log::info!("KeyboardInput text field (IME fallback): '{}'", s);
                                for ch in s.chars() {
                                    let keysym = ch as u32;
                                    if keysym != 0 {
                                        self.comms.send_input(InputEvent::Key {
                                            keysym,
                                            modifiers: 0,
                                            pressed: true,
                                        });
                                    }
                                }
                                handled_via_text = true;
                            }
                        }
                    }
                    if !handled_via_text {
                        let keysym = Self::translate_key(&logical_key);
                        if keysym != 0 {
                            // Hide mouse cursor on keyboard input
                            if state == ElementState::Pressed && !self.mouse_hidden_for_typing {
                                if let Some(ref window) = self.window {
                                    window.set_cursor_visible(false);
                                    self.mouse_hidden_for_typing = true;
                                }
                            }
                            // Track key presses for typing speed indicator
                            if self.effects.typing_speed.enabled && state == ElementState::Pressed {
                                self.key_press_times.push(std::time::Instant::now());
                            }
                            // Track activity for idle dimming
                            if self.effects.idle_dim.enabled {
                                self.last_activity_time = std::time::Instant::now();
                            }
                            self.comms.send_input(InputEvent::Key {
                                keysym,
                                modifiers: self.modifiers,
                                pressed: state == ElementState::Pressed,
                            });
                        }
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                // If popup menu is active, handle clicks for it
                if let Some(ref mut menu) = self.popup_menu {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let idx = menu.hit_test(self.mouse_pos.0, self.mouse_pos.1);
                        if idx >= 0 {
                            // Regular item selected
                            self.comms.send_input(InputEvent::MenuSelection { index: idx });
                            self.popup_menu = None;
                            self.frame_dirty = true;
                        } else {
                            // Check if click is on a submenu item (which hit_test returns -1 for)
                            let (depth, local_idx) = menu.hit_test_all(self.mouse_pos.0, self.mouse_pos.1);
                            if depth >= 0 && local_idx >= 0 {
                                let panel = if depth == 0 {
                                    &menu.root_panel
                                } else {
                                    &menu.submenu_panels[(depth - 1) as usize]
                                };
                                let global_idx = panel.item_indices[local_idx as usize];
                                if menu.all_items[global_idx].submenu {
                                    // Clicked a submenu item — keep menu open, submenu auto-opened on hover
                                    self.frame_dirty = true;
                                } else {
                                    // Clicked outside or on a disabled item — cancel
                                    self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                                    self.popup_menu = None;
                                    self.frame_dirty = true;
                                }
                            } else {
                                // Clicked outside all panels — cancel
                                self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                                self.popup_menu = None;
                                self.frame_dirty = true;
                            }
                        }
                    } else if state == ElementState::Pressed {
                        // Any other button cancels the menu
                        self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                        self.popup_menu = None;
                        self.frame_dirty = true;
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && self.chrome.resize_edge.is_some()
                {
                    // Borderless: initiate window resize drag
                    if let (Some(dir), Some(ref window)) =
                        (self.chrome.resize_edge, self.window.as_ref())
                    {
                        let _ = window.drag_resize_window(dir);
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && self.titlebar_hit_test(self.mouse_pos.0, self.mouse_pos.1) > 0
                {
                    // Custom title bar click
                    match self.titlebar_hit_test(self.mouse_pos.0, self.mouse_pos.1) {
                        1 => {
                            // Drag area: double-click toggles maximize
                            let now = std::time::Instant::now();
                            if now.duration_since(self.chrome.last_titlebar_click).as_millis() < 400 {
                                if let Some(ref window) = self.window {
                                    window.set_maximized(!window.is_maximized());
                                }
                            } else if let Some(ref window) = self.window {
                                let _ = window.drag_window();
                            }
                            self.chrome.last_titlebar_click = now;
                        }
                        2 => {
                            // Close button (titlebar is only on primary window)
                            self.comms.send_input(InputEvent::WindowClose { emacs_frame_id: 0 });
                        }
                        3 => {
                            // Maximize/restore toggle
                            if let Some(ref window) = self.window {
                                if window.is_maximized() {
                                    window.set_maximized(false);
                                } else {
                                    window.set_maximized(true);
                                }
                            }
                        }
                        4 => {
                            // Minimize
                            if let Some(ref window) = self.window {
                                window.set_minimized(true);
                            }
                        }
                        _ => {}
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && !self.chrome.decorations_enabled
                    && (self.modifiers & NEOMACS_SUPER_MASK) != 0
                {
                    // Borderless: Super+click to drag-move window
                    if let Some(ref window) = self.window {
                        let _ = window.drag_window();
                    }
                } else {
                    let btn = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                        MouseButton::Back => 4,
                        MouseButton::Forward => 5,
                        MouseButton::Other(n) => n as u32,
                    };
                    // Hit test child frames
                    let (ev_x, ev_y, target_fid) =
                        if let Some((fid, lx, ly)) = self.child_frames.hit_test(self.mouse_pos.0, self.mouse_pos.1) {
                            (lx, ly, fid)
                        } else {
                            (self.mouse_pos.0, self.mouse_pos.1, 0)
                        };
                    self.comms.send_input(InputEvent::MouseButton {
                        button: btn,
                        x: ev_x,
                        y: ev_y,
                        pressed: state == ElementState::Pressed,
                        modifiers: self.modifiers,
                        target_frame_id: target_fid,
                    });
                    // Click halo effect on press
                    if state == ElementState::Pressed && self.effects.click_halo.enabled {
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.trigger_click_halo(self.mouse_pos.0, self.mouse_pos.1, std::time::Instant::now());
                        }
                        self.frame_dirty = true;
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Convert to logical pixels for Emacs
                let lx = (position.x / self.scale_factor) as f32;
                let ly = (position.y / self.scale_factor) as f32;
                self.mouse_pos = (lx, ly);
                // Track activity for idle dimming
                if self.effects.idle_dim.enabled {
                    self.last_activity_time = std::time::Instant::now();
                }

                // Restore mouse cursor visibility when mouse moves
                if self.mouse_hidden_for_typing {
                    if let Some(ref window) = self.window {
                        window.set_cursor_visible(true);
                    }
                    self.mouse_hidden_for_typing = false;
                }

                // Borderless resize edge detection
                let edge = self.detect_resize_edge(lx, ly);
                if edge != self.chrome.resize_edge {
                    self.chrome.resize_edge = edge;
                    if let Some(ref window) = self.window {
                        use winit::window::CursorIcon;
                        let icon = match edge {
                            Some(dir) => CursorIcon::from(dir),
                            None => CursorIcon::Default,
                        };
                        window.set_cursor(icon);
                    }
                }

                // Update title bar hover state and cursor
                if !self.chrome.decorations_enabled {
                    let new_hover = self.titlebar_hit_test(lx, ly);
                    if new_hover != self.chrome.titlebar_hover {
                        self.chrome.titlebar_hover = new_hover;
                        self.frame_dirty = true;
                        // Set cursor icon based on title bar region
                        if self.chrome.resize_edge.is_none() {
                            if let Some(ref window) = self.window {
                                use winit::window::CursorIcon;
                                let icon = match new_hover {
                                    2 | 3 | 4 => CursorIcon::Pointer, // buttons
                                    _ => CursorIcon::Default,
                                };
                                window.set_cursor(icon);
                            }
                        }
                    }
                }

                // Update popup menu hover state (multi-panel)
                if let Some(ref mut menu) = self.popup_menu {
                    let (hit_depth, hit_local) = menu.hit_test_all(lx, ly);
                    if hit_depth >= 0 {
                        // Close deeper submenus if hovering on a shallower panel
                        let target_depth = hit_depth as usize;
                        while menu.submenu_panels.len() > target_depth {
                            menu.submenu_panels.pop();
                            self.frame_dirty = true;
                        }
                        // Update hover in the target panel
                        let panel = if target_depth == 0 {
                            &mut menu.root_panel
                        } else {
                            &mut menu.submenu_panels[target_depth - 1]
                        };
                        if hit_local != panel.hover_index {
                            panel.hover_index = hit_local;
                            self.frame_dirty = true;
                            // Auto-open submenu on hover
                            if hit_local >= 0 && (hit_local as usize) < panel.item_indices.len() {
                                let global_idx = panel.item_indices[hit_local as usize];
                                if menu.all_items[global_idx].submenu {
                                    menu.open_submenu();
                                }
                            }
                        }
                    }
                } else {
                    // Hit test child frames for mouse move
                    let (ev_x, ev_y, target_fid) =
                        if let Some((fid, local_x, local_y)) = self.child_frames.hit_test(lx, ly) {
                            (local_x, local_y, fid)
                        } else {
                            (lx, ly, 0)
                        };
                    self.comms.send_input(InputEvent::MouseMove {
                        x: ev_x,
                        y: ev_y,
                        modifiers: self.modifiers,
                        target_frame_id: target_fid,
                    });
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy, pixel_precise) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (x, y, false)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        // Pass raw logical pixel deltas for touchpad
                        ((pos.x / self.scale_factor) as f32,
                         (pos.y / self.scale_factor) as f32,
                         true)
                    }
                };
                // Hit test child frames for scroll
                let (ev_x, ev_y, target_fid) =
                    if let Some((fid, local_x, local_y)) = self.child_frames.hit_test(self.mouse_pos.0, self.mouse_pos.1) {
                        (local_x, local_y, fid)
                    } else {
                        (self.mouse_pos.0, self.mouse_pos.1, 0)
                    };
                self.comms.send_input(InputEvent::MouseScroll {
                    delta_x: dx,
                    delta_y: dy,
                    x: ev_x,
                    y: ev_y,
                    modifiers: self.modifiers,
                    pixel_precise,
                    target_frame_id: target_fid,
                });
            }

            WindowEvent::RedrawRequested => {
                self.render();
                self.frame_dirty = false;
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

            WindowEvent::Ime(ime_event) => {
                match ime_event {
                    winit::event::Ime::Enabled => {
                        self.ime_enabled = true;
                        log::info!("IME enabled");
                    }
                    winit::event::Ime::Disabled => {
                        self.ime_enabled = false;
                        self.ime_preedit_active = false;
                        self.ime_preedit_text.clear();
                        log::info!("IME disabled");
                    }
                    winit::event::Ime::Commit(text) => {
                        log::debug!("IME Commit: '{}'", text);
                        self.ime_preedit_active = false;
                        self.ime_preedit_text.clear();
                        self.frame_dirty = true;
                        // Send each committed character as an individual
                        // key event to Emacs (no modifiers — IME already
                        // composed the final characters)
                        for ch in text.chars() {
                            let keysym = ch as u32;
                            if keysym != 0 {
                                self.comms.send_input(InputEvent::Key {
                                    keysym,
                                    modifiers: 0,
                                    pressed: true,
                                });
                            }
                        }
                    }
                    winit::event::Ime::Preedit(text, cursor_range) => {
                        log::debug!("IME Preedit: '{}' cursor: {:?}", text, cursor_range);
                        // Track whether preedit is active to suppress
                        // raw KeyboardInput during IME composition
                        self.ime_preedit_active = !text.is_empty();
                        self.ime_preedit_text = text.clone();

                        // Update IME cursor area so the OS positions the
                        // candidate window near the text cursor
                        if let Some(ref window) = self.window {
                            if let Some(ref target) = self.cursor.target {
                                let x = (target.x as f64) * self.scale_factor;
                                let y = (target.y as f64 + target.height as f64) * self.scale_factor;
                                let w = target.width as f64 * self.scale_factor;
                                let h = target.height as f64 * self.scale_factor;
                                window.set_ime_cursor_area(
                                    winit::dpi::PhysicalPosition::new(x, y),
                                    winit::dpi::PhysicalSize::new(w, h),
                                );
                            }
                        }
                        self.frame_dirty = true;
                    }
                }
            }

            WindowEvent::DroppedFile(path) => {
                if let Some(path_str) = path.to_str() {
                    log::info!("File dropped: {}", path_str);
                    self.comms.send_input(InputEvent::FileDrop {
                        paths: vec![path_str.to_string()],
                        x: self.mouse_pos.0,
                        y: self.mouse_pos.1,
                    });
                }
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                log::info!("Scale factor changed: {} -> {}", self.scale_factor, scale_factor);
                self.scale_factor = scale_factor;
                // Update renderer's scale factor
                if let Some(ref mut renderer) = self.renderer {
                    renderer.set_scale_factor(scale_factor as f32);
                }
                // Clear glyph atlas so text re-rasterizes at new DPI
                if let Some(ref mut atlas) = self.glyph_atlas {
                    atlas.set_scale_factor(scale_factor as f32);
                }
                self.frame_dirty = true;
                // The Resized event will follow, which handles surface reconfiguration
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

        // Process multi-window creates/destroys
        if let (Some(device), Some(adapter)) = (&self.device, &self.adapter) {
            self.multi_windows.process_creates(event_loop, device, adapter);
        }
        self.multi_windows.process_destroys();

        // Get latest frame from Emacs
        self.poll_frame();

        // Pump GLib for WebKit
        self.pump_glib();

        // Update cursor blink state
        if self.tick_cursor_blink() {
            self.frame_dirty = true;
        }

        // Tick cursor animation
        if self.cursor.tick_animation() {
            self.frame_dirty = true;
        }

        // Tick cursor size transition (runs after position animation, overrides w/h)
        if self.cursor.tick_size_animation() {
            self.frame_dirty = true;
        }

        // Tick idle dimming
        if self.effects.idle_dim.enabled {
            let idle_time = self.last_activity_time.elapsed();
            let target_alpha = if idle_time >= self.effects.idle_dim.delay {
                self.effects.idle_dim.opacity
            } else {
                0.0
            };
            let diff = target_alpha - self.idle_dim_current_alpha;
            if diff.abs() > 0.001 {
                let fade_speed = if self.effects.idle_dim.fade_duration.as_secs_f32() > 0.0 {
                    1.0 / self.effects.idle_dim.fade_duration.as_secs_f32() * 0.016
                } else {
                    1.0
                };
                if diff > 0.0 {
                    self.idle_dim_current_alpha = (self.idle_dim_current_alpha + fade_speed * self.effects.idle_dim.opacity).min(target_alpha);
                } else {
                    self.idle_dim_current_alpha = (self.idle_dim_current_alpha - fade_speed * self.effects.idle_dim.opacity).max(0.0);
                }
                self.idle_dim_active = true;
                self.frame_dirty = true;
            } else if self.idle_dim_current_alpha > 0.001 {
                self.idle_dim_active = true;
                self.frame_dirty = true;
            } else {
                self.idle_dim_active = false;
            }
        }

        // Keep dirty if cursor pulse is active (needs continuous redraw)
        if self.effects.cursor_pulse.enabled && self.effects.cursor_glow.enabled {
            self.frame_dirty = true;
        }

        // Keep dirty if renderer signals need for continuous redraws (dim fade)
        if let Some(ref renderer) = self.renderer {
            if renderer.needs_continuous_redraw {
                self.frame_dirty = true;
            }
        }

        // Keep dirty if transitions are active
        if self.transitions.has_active() {
            self.frame_dirty = true;
        }

        // Check for terminal PTY activity
        if self.has_terminal_activity() {
            self.frame_dirty = true;
        }

        // Determine if continuous rendering is needed
        let has_active_content = self.has_webkit_needing_redraw() || self.has_playing_videos();

        // Request redraw when we have new frame data, cursor blink toggled,
        // or webkit/video content changed
        if self.frame_dirty || has_active_content {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }

        // Use WaitUntil with smart timeouts instead of Poll to save CPU.
        // Window events (key, mouse, resize) still wake immediately.
        let now = std::time::Instant::now();
        let next_wake = if self.frame_dirty || has_active_content
            || self.cursor.animating || self.cursor.size_animating
            || self.idle_dim_active || self.transitions.has_active()
        {
            // Active rendering: cap at ~240fps to avoid spinning
            now + std::time::Duration::from_millis(4)
        } else if self.cursor.blink_enabled {
            // Idle with cursor blink: wake at next toggle time
            self.cursor.last_blink_toggle + self.cursor.blink_interval
        } else {
            // Fully idle: poll for new Emacs frames at 60fps
            now + std::time::Duration::from_millis(16)
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_wake));
    }
}

/// Run the render loop (called on render thread)
fn run_render_loop(
    comms: RenderComms,
    width: u32,
    height: u32,
    title: String,
    image_dimensions: SharedImageDimensions,
    shared_monitors: SharedMonitorInfo,
    #[cfg(feature = "neo-term")]
    shared_terminals: crate::terminal::SharedTerminals,
) {
    log::info!("Render thread starting");

    // CRITICAL: Set up a dedicated GMainContext for WebKit before any WebKit initialization.
    // This ensures WebKit attaches its GLib sources (IPC sockets, etc.) to this context,
    // not the default context. Only the render thread will dispatch events from this context,
    // preventing the Emacs main thread's xg_select from dispatching WebKit callbacks.
    #[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
    let webkit_main_context = unsafe {
        let ctx = plat::g_main_context_new();
        if !ctx.is_null() {
            // Acquire the context so we can dispatch on it
            plat::g_main_context_acquire(ctx);
            // Push as thread-default - WebKit will attach sources here
            plat::g_main_context_push_thread_default(ctx);
            log::info!("Created dedicated GMainContext for WebKit: {:?}", ctx);
        } else {
            log::warn!("Failed to create dedicated GMainContext for WebKit");
        }
        ctx
    };

    // Use any_thread() since we're running on a non-main thread
    #[cfg(target_os = "linux")]
    let event_loop = {
        let mut builder = EventLoop::builder();
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

    // Start with WaitUntil to avoid busy-polling; about_to_wait() adjusts dynamically
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(16),
    ));

    let mut app = RenderApp::new(
        comms, width, height, title, image_dimensions,
        shared_monitors,
        #[cfg(feature = "neo-term")]
        shared_terminals,
    );

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
