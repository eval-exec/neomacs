//! Multi-window management for the render thread.
//!
//! Supports multiple OS windows, each with its own wgpu surface.
//! Shared GPU device/queue/glyph atlas across all windows.
//! Each window holds its own frame data and child frames.

use std::collections::HashMap;
use std::sync::Arc;

use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::core::frame_glyphs::FrameGlyphBuffer;
use super::child_frames::ChildFrameManager;

/// Per-window state. Each Emacs top-level frame gets its own OS window
/// with a separate wgpu surface.
pub(crate) struct WindowState {
    /// The winit window.
    pub window: Arc<Window>,
    /// wgpu surface for this window.
    pub surface: wgpu::Surface<'static>,
    /// Surface configuration.
    pub surface_config: wgpu::SurfaceConfiguration,
    /// Physical width in pixels.
    pub width: u32,
    /// Physical height in pixels.
    pub height: u32,
    /// Display scale factor for this window's monitor.
    pub scale_factor: f64,
    /// The Emacs frame_id that owns this window (used for routing).
    pub emacs_frame_id: u64,
    /// Current root frame glyph buffer for this window.
    pub current_frame: Option<FrameGlyphBuffer>,
    /// Child frames rendered as overlays in this window.
    pub child_frames: ChildFrameManager,
    /// Whether this window needs a redraw.
    pub frame_dirty: bool,
    /// Window title.
    pub title: String,
}

impl WindowState {
    /// Resize this window's surface.
    pub fn handle_resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(device, &self.surface_config);
        self.frame_dirty = true;
    }
}

/// Manages all windows in the render thread.
///
/// Maps between Emacs frame IDs (u64) and winit WindowIds.
/// One shared GPU device services all windows.
pub(crate) struct MultiWindowManager {
    /// Emacs frame_id → WindowState
    pub windows: HashMap<u64, WindowState>,
    /// Winit WindowId → Emacs frame_id (reverse mapping for event dispatch)
    pub winit_to_emacs: HashMap<WindowId, u64>,
    /// Pending window creation requests (processed in resumed/about_to_wait)
    pub pending_creates: Vec<PendingWindow>,
    /// Pending window destruction requests
    pub pending_destroys: Vec<u64>,
}

/// A request to create a new OS window.
pub(crate) struct PendingWindow {
    pub emacs_frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub title: String,
}

impl MultiWindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            winit_to_emacs: HashMap::new(),
            pending_creates: Vec::new(),
            pending_destroys: Vec::new(),
        }
    }

    /// Schedule a new window to be created on the next event loop iteration.
    pub fn request_create(&mut self, emacs_frame_id: u64, width: u32, height: u32, title: String) {
        self.pending_creates.push(PendingWindow {
            emacs_frame_id,
            width,
            height,
            title,
        });
    }

    /// Schedule a window for destruction.
    pub fn request_destroy(&mut self, emacs_frame_id: u64) {
        self.pending_destroys.push(emacs_frame_id);
    }

    /// Process pending window creations. Must be called from the event loop
    /// (requires ActiveEventLoop for window creation).
    pub fn process_creates(
        &mut self,
        event_loop: &ActiveEventLoop,
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
    ) {
        let pending = std::mem::take(&mut self.pending_creates);
        for req in pending {
            if self.windows.contains_key(&req.emacs_frame_id) {
                log::warn!("Window for frame {} already exists", req.emacs_frame_id);
                continue;
            }

            let attrs = Window::default_attributes()
                .with_title(&req.title)
                .with_inner_size(winit::dpi::LogicalSize::new(req.width, req.height))
                .with_transparent(true);

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    let window = Arc::new(window);
                    let scale_factor = window.scale_factor();
                    let phys = window.inner_size();

                    // Create surface for this window
                    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                        backends: wgpu::Backends::all(),
                        ..Default::default()
                    });
                    let surface = match instance.create_surface(window.clone()) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("Failed to create surface for frame {}: {:?}", req.emacs_frame_id, e);
                            continue;
                        }
                    };

                    // Configure surface
                    let caps = surface.get_capabilities(adapter);
                    let format = caps.formats.iter().copied()
                        .find(|f| f.is_srgb())
                        .unwrap_or(caps.formats[0]);
                    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
                        wgpu::CompositeAlphaMode::PreMultiplied
                    } else {
                        caps.alpha_modes[0]
                    };
                    let config = wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format,
                        width: phys.width,
                        height: phys.height,
                        present_mode: wgpu::PresentMode::Fifo,
                        alpha_mode,
                        view_formats: vec![],
                        desired_maximum_frame_latency: 2,
                    };
                    surface.configure(device, &config);

                    // Enable IME
                    window.set_ime_allowed(true);

                    let winit_id = window.id();
                    log::info!(
                        "Created window for frame {} (winit {:?}, {}x{}, scale={})",
                        req.emacs_frame_id, winit_id, phys.width, phys.height, scale_factor
                    );

                    self.winit_to_emacs.insert(winit_id, req.emacs_frame_id);
                    self.windows.insert(req.emacs_frame_id, WindowState {
                        window,
                        surface,
                        surface_config: config,
                        width: phys.width,
                        height: phys.height,
                        scale_factor,
                        emacs_frame_id: req.emacs_frame_id,
                        current_frame: None,
                        child_frames: ChildFrameManager::new(),
                        frame_dirty: false,
                        title: req.title,
                    });
                }
                Err(e) => {
                    log::error!("Failed to create window for frame {}: {:?}", req.emacs_frame_id, e);
                }
            }
        }
    }

    /// Process pending window destructions.
    pub fn process_destroys(&mut self) {
        let pending = std::mem::take(&mut self.pending_destroys);
        for frame_id in pending {
            if let Some(state) = self.windows.remove(&frame_id) {
                self.winit_to_emacs.remove(&state.window.id());
                log::info!("Destroyed window for frame {}", frame_id);
                // Window and surface are dropped here
            }
        }
    }

    /// Look up the Emacs frame_id for a winit WindowId.
    pub fn emacs_frame_for_winit(&self, winit_id: WindowId) -> Option<u64> {
        self.winit_to_emacs.get(&winit_id).copied()
    }

    /// Get a window state by Emacs frame_id.
    pub fn get(&self, emacs_frame_id: u64) -> Option<&WindowState> {
        self.windows.get(&emacs_frame_id)
    }

    /// Get a mutable window state by Emacs frame_id.
    pub fn get_mut(&mut self, emacs_frame_id: u64) -> Option<&mut WindowState> {
        self.windows.get_mut(&emacs_frame_id)
    }

    /// Get a window state by winit WindowId.
    pub fn get_by_winit(&self, winit_id: WindowId) -> Option<&WindowState> {
        self.winit_to_emacs.get(&winit_id)
            .and_then(|id| self.windows.get(id))
    }

    /// Get a mutable window state by winit WindowId.
    pub fn get_by_winit_mut(&mut self, winit_id: WindowId) -> Option<&mut WindowState> {
        self.winit_to_emacs.get(&winit_id).copied()
            .and_then(move |id| self.windows.get_mut(&id))
    }

    /// Route a FrameGlyphBuffer to the appropriate window.
    /// Returns true if the frame was routed to a secondary window.
    pub fn route_frame(&mut self, frame: FrameGlyphBuffer) -> bool {
        let frame_id = frame.frame_id;
        if frame_id != 0 {
            if frame.parent_id != 0 {
                // Child frame: route to the window that owns the parent
                // Find which window has the parent as its root frame
                for (_, ws) in self.windows.iter_mut() {
                    if ws.emacs_frame_id == frame.parent_id {
                        ws.child_frames.update_frame(frame);
                        ws.frame_dirty = true;
                        return true;
                    }
                }
            } else if let Some(ws) = self.windows.get_mut(&frame_id) {
                // Root frame for a secondary window
                ws.current_frame = Some(frame);
                ws.frame_dirty = true;
                return true;
            }
        }
        false // Not handled — belongs to primary window
    }

    /// Check if any secondary window needs redrawing.
    pub fn any_dirty(&self) -> bool {
        self.windows.values().any(|ws| ws.frame_dirty)
    }

    /// Return number of secondary windows.
    pub fn count(&self) -> usize {
        self.windows.len()
    }

    /// Iterate over all windows that need rendering.
    pub fn dirty_windows(&mut self) -> Vec<u64> {
        self.windows.iter()
            .filter(|(_, ws)| ws.frame_dirty)
            .map(|(&id, _)| id)
            .collect()
    }
}
