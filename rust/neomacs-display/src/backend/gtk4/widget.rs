//! Custom GTK4 widget for Neomacs GPU-accelerated rendering.
//!
//! This widget uses GtkSnapshot for true GPU rendering via GSK render nodes,
//! bypassing the Cairo software rasterization path.
//!
//! Glyphs accumulate in FrameGlyphBuffer - Emacs incremental redisplay only
//! sends changed content. Window backgrounds cover old content in changed areas.
//!
//! Enable logging with: RUST_LOG=neomacs_display::backend::gtk4::widget=debug

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, graphene, gsk, pango, Snapshot};
use log::{debug, trace, warn, info};

use crate::core::scene::Scene;
use crate::core::scene::FloatingImage;
use crate::core::scene::FloatingWebKit;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use super::gsk_renderer::GskRenderer;
use super::hybrid_renderer::HybridRenderer;
use super::video::VideoCache;
use super::image::ImageCache;
#[cfg(feature = "wpe-webkit")]
use crate::backend::webkit::WebKitCache;

// Thread-local caches for widget rendering
// These are set by the FFI layer before triggering a widget redraw
thread_local! {
    pub static WIDGET_VIDEO_CACHE: RefCell<Option<*const VideoCache>> = const { RefCell::new(None) };
    pub static WIDGET_IMAGE_CACHE: RefCell<Option<*mut ImageCache>> = const { RefCell::new(None) };
    // Store CLONED glyphs, not a pointer - pointer becomes invalid when Emacs clears buffer
    pub static WIDGET_FRAME_GLYPHS: RefCell<Option<FrameGlyphBuffer>> = const { RefCell::new(None) };
    pub static WIDGET_USE_HYBRID: RefCell<bool> = const { RefCell::new(true) };
    // Resize callback - called when widget size changes
    pub static WIDGET_RESIZE_CALLBACK: RefCell<Option<Box<dyn Fn(i32, i32) + Send + 'static>>> = const { RefCell::new(None) };
    // Floating images for overlay rendering in hybrid mode
    pub static WIDGET_FLOATING_IMAGES: RefCell<Vec<FloatingImage>> = const { RefCell::new(Vec::new()) };
    // Floating webkit views for overlay rendering
    pub static WIDGET_FLOATING_WEBKITS: RefCell<Vec<FloatingWebKit>> = const { RefCell::new(Vec::new()) };
    // WebKit cache pointer for rendering webkit textures
    #[cfg(feature = "wpe-webkit")]
    pub static WIDGET_WEBKIT_CACHE: RefCell<Option<*const WebKitCache>> = const { RefCell::new(None) };
    // Mouse button callback - called on button press/release
    // Args: (x, y, button, pressed, modifiers, time)
    pub static WIDGET_MOUSE_BUTTON_CALLBACK: RefCell<Option<Box<dyn Fn(f64, f64, u32, bool, u32, u32) + Send + 'static>>> = const { RefCell::new(None) };
    // Mouse motion callback - called on mouse movement
    // Args: (x, y, modifiers, time)
    pub static WIDGET_MOUSE_MOTION_CALLBACK: RefCell<Option<Box<dyn Fn(f64, f64, u32, u32) + Send + 'static>>> = const { RefCell::new(None) };
    // Mouse scroll callback - called on scroll wheel
    // Args: (x, y, delta_x, delta_y, modifiers, time)
    pub static WIDGET_MOUSE_SCROLL_CALLBACK: RefCell<Option<Box<dyn Fn(f64, f64, f64, f64, u32, u32) + Send + 'static>>> = const { RefCell::new(None) };
    // Flag to request snapshot capture before next redraw (for buffer transitions)
    pub static WIDGET_CAPTURE_SNAPSHOT: RefCell<bool> = const { RefCell::new(false) };
    // Captured snapshot texture for buffer transitions
    pub static WIDGET_SNAPSHOT_TEXTURE: RefCell<Option<gtk4::gdk::Texture>> = const { RefCell::new(None) };
    // Last rendered frame texture (always cached for instant snapshot)
    pub static WIDGET_LAST_FRAME_TEXTURE: RefCell<Option<gtk4::gdk::Texture>> = const { RefCell::new(None) };
    // Flag to enable continuous frame caching (for buffer transitions)
    pub static WIDGET_CACHE_FRAMES: RefCell<bool> = const { RefCell::new(false) };
    // Shared HybridRenderer pointer - set by FFI to share renderer with widget
    pub static WIDGET_HYBRID_RENDERER: RefCell<Option<*mut super::HybridRenderer>> = const { RefCell::new(None) };
    // Tick callback ID for animation frame clock
    pub static WIDGET_TICK_CALLBACK_ID: RefCell<Option<gtk4::TickCallbackId>> = const { RefCell::new(None) };
    // Last frame timestamp for delta time calculation (in microseconds)
    pub static WIDGET_LAST_FRAME_TIME: RefCell<i64> = const { RefCell::new(0) };
    // Widget reference for tick callback (weak ref to avoid preventing drop)
    pub static WIDGET_INSTANCE: RefCell<Option<glib::WeakRef<NeomacsWidget>>> = const { RefCell::new(None) };
}

/// Set the video cache for widget rendering (called from FFI before queue_draw)
pub fn set_widget_video_cache(cache: *const VideoCache) {
    WIDGET_VIDEO_CACHE.with(|c| {
        *c.borrow_mut() = if cache.is_null() { None } else { Some(cache) };
    });
}

/// Set the image cache for widget rendering (called from FFI before queue_draw)
pub fn set_widget_image_cache(cache: *mut ImageCache) {
    WIDGET_IMAGE_CACHE.with(|c| {
        *c.borrow_mut() = if cache.is_null() { None } else { Some(cache) };
    });
}

/// Set the frame glyph buffer for hybrid rendering (called from FFI before queue_draw)
/// Now CLONES the buffer so it survives Emacs clearing it for the next frame.
/// Only updates if buffer has content - preserves last valid frame to prevent flickering.
pub fn set_widget_frame_glyphs(buffer: *const FrameGlyphBuffer) {
    WIDGET_FRAME_GLYPHS.with(|c| {
        if buffer.is_null() {
            return;
        }
        let new_buffer = unsafe { (*buffer).clone() };
        let glyph_count = new_buffer.glyphs.len();
        // Only update if we have content - prevents black flash from empty buffers
        if !new_buffer.glyphs.is_empty() {
            *c.borrow_mut() = Some(new_buffer);
        } else {
        }
    });
}

/// Set whether to use hybrid rendering mode
pub fn set_widget_use_hybrid(use_hybrid: bool) {
    WIDGET_USE_HYBRID.with(|c| {
        *c.borrow_mut() = use_hybrid;
    });
}

/// Set the resize callback - called when widget size changes
pub fn set_widget_resize_callback<F>(callback: F)
where
    F: Fn(i32, i32) + Send + 'static,
{
    WIDGET_RESIZE_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(Box::new(callback));
    });
}

/// Set floating images for overlay rendering (called from FFI before queue_draw)
pub fn set_widget_floating_images(images: Vec<FloatingImage>) {
    WIDGET_FLOATING_IMAGES.with(|c| {
        *c.borrow_mut() = images;
    });
}

/// Set floating webkit views for overlay rendering in hybrid mode
pub fn set_widget_floating_webkits(webkits: Vec<FloatingWebKit>) {
    WIDGET_FLOATING_WEBKITS.with(|c| {
        *c.borrow_mut() = webkits;
    });
}

/// Set the webkit cache for widget rendering (called from FFI before queue_draw)
#[cfg(feature = "wpe-webkit")]
pub fn set_widget_webkit_cache(cache: *const WebKitCache) {
    WIDGET_WEBKIT_CACHE.with(|c| {
        *c.borrow_mut() = if cache.is_null() { None } else { Some(cache) };
    });
}

/// Set the mouse button callback - called on button press/release
/// Args: (x, y, button, pressed, modifiers, time)
pub fn set_widget_mouse_button_callback<F>(callback: F)
where
    F: Fn(f64, f64, u32, bool, u32, u32) + Send + 'static,
{
    info!("Mouse button callback set");
    WIDGET_MOUSE_BUTTON_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(Box::new(callback));
    });
}

/// Set the mouse motion callback - called on mouse movement
/// Args: (x, y, modifiers, time)
pub fn set_widget_mouse_motion_callback<F>(callback: F)
where
    F: Fn(f64, f64, u32, u32) + Send + 'static,
{
    info!("Mouse motion callback set");
    WIDGET_MOUSE_MOTION_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(Box::new(callback));
    });
}

/// Set the mouse scroll callback - called on scroll wheel
/// Args: (x, y, delta_x, delta_y, modifiers, time)
pub fn set_widget_mouse_scroll_callback<F>(callback: F)
where
    F: Fn(f64, f64, f64, f64, u32, u32) + Send + 'static,
{
    info!("Mouse scroll callback set");
    WIDGET_MOUSE_SCROLL_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(Box::new(callback));
    });
}

/// Request snapshot capture on next frame (for buffer transitions)
pub fn request_snapshot_capture() {
    WIDGET_CAPTURE_SNAPSHOT.with(|c| {
        *c.borrow_mut() = true;
    });
}

/// Check if snapshot capture was requested
pub fn is_snapshot_requested() -> bool {
    WIDGET_CAPTURE_SNAPSHOT.with(|c| *c.borrow())
}

/// Clear snapshot capture request
pub fn clear_snapshot_request() {
    WIDGET_CAPTURE_SNAPSHOT.with(|c| {
        *c.borrow_mut() = false;
    });
}

/// Store captured snapshot texture
pub fn set_snapshot_texture(texture: gtk4::gdk::Texture) {
    WIDGET_SNAPSHOT_TEXTURE.with(|c| {
        *c.borrow_mut() = Some(texture);
    });
}

/// Get and take the captured snapshot texture
pub fn take_snapshot_texture() -> Option<gtk4::gdk::Texture> {
    WIDGET_SNAPSHOT_TEXTURE.with(|c| c.borrow_mut().take())
}

/// Check if we have a captured snapshot
pub fn has_snapshot_texture() -> bool {
    WIDGET_SNAPSHOT_TEXTURE.with(|c| c.borrow().is_some())
}

/// Enable frame caching for buffer transitions
pub fn enable_frame_caching(enable: bool) {
    WIDGET_CACHE_FRAMES.with(|c| {
        *c.borrow_mut() = enable;
    });
}

/// Check if frame caching is enabled
pub fn is_frame_caching_enabled() -> bool {
    WIDGET_CACHE_FRAMES.with(|c| *c.borrow())
}

/// Store the last rendered frame
pub fn set_last_frame_texture(texture: gtk4::gdk::Texture) {
    WIDGET_LAST_FRAME_TEXTURE.with(|c| {
        *c.borrow_mut() = Some(texture);
    });
}

/// Get the last rendered frame as snapshot (for instant buffer transitions)
/// This copies the texture reference without consuming it
pub fn get_last_frame_as_snapshot() -> Option<gtk4::gdk::Texture> {
    WIDGET_LAST_FRAME_TEXTURE.with(|c| c.borrow().clone())
}

/// Prepare snapshot from last frame (called before buffer switch)
/// This moves the last frame to the snapshot slot
pub fn prepare_snapshot_from_last_frame() -> bool {
    let last_frame = WIDGET_LAST_FRAME_TEXTURE.with(|c| c.borrow().clone());
    if let Some(texture) = last_frame {
        WIDGET_SNAPSHOT_TEXTURE.with(|c| {
            *c.borrow_mut() = Some(texture);
        });
        true
    } else {
        false
    }
}

/// Set the shared HybridRenderer pointer (called from FFI)
/// This allows the widget to use the same renderer as the FFI layer
pub fn set_widget_hybrid_renderer(renderer: *mut super::HybridRenderer) {
    WIDGET_HYBRID_RENDERER.with(|c| {
        *c.borrow_mut() = if renderer.is_null() { None } else { Some(renderer) };
    });
}

/// Get the shared HybridRenderer (unsafe - caller must ensure pointer is valid)
pub fn get_widget_hybrid_renderer() -> Option<*mut super::HybridRenderer> {
    WIDGET_HYBRID_RENDERER.with(|c| *c.borrow())
}

/// Store widget instance for tick callback access
pub fn set_widget_instance(widget: &NeomacsWidget) {
    WIDGET_INSTANCE.with(|c| {
        *c.borrow_mut() = Some(widget.downgrade());
    });
}

/// Start animation tick callback (frame clock driven updates)
/// This enables continuous animation updates at display refresh rate
pub fn start_animation_tick() {
    // Check if already running
    let already_running = WIDGET_TICK_CALLBACK_ID.with(|c| c.borrow().is_some());
    if already_running {
        return;
    }

    // Get widget reference
    let widget = WIDGET_INSTANCE.with(|c| {
        c.borrow().as_ref().and_then(|w| w.upgrade())
    });

    if let Some(widget) = widget {
        info!("Starting animation tick callback");

        // Initialize last frame time
        WIDGET_LAST_FRAME_TIME.with(|c| *c.borrow_mut() = 0);

        // Add tick callback - called every frame by GTK
        let tick_id = widget.add_tick_callback(|widget, frame_clock| {
            // Get current frame time in microseconds
            let frame_time = frame_clock.frame_time();

            // Calculate delta time
            let last_time = WIDGET_LAST_FRAME_TIME.with(|c| *c.borrow());
            let dt = if last_time == 0 {
                1.0 / 60.0  // Assume 60fps for first frame
            } else {
                (frame_time - last_time) as f32 / 1_000_000.0  // Convert microseconds to seconds
            };
            WIDGET_LAST_FRAME_TIME.with(|c| *c.borrow_mut() = frame_time);

            // Update animations via shared renderer
            let mut animations_active = false;
            if let Some(renderer_ptr) = get_widget_hybrid_renderer() {
                let renderer = unsafe { &mut *renderer_ptr };
                animations_active = renderer.update_animation(dt);
            }

            // If animations are active, request redraw
            if animations_active {
                widget.queue_draw();
                glib::ControlFlow::Continue
            } else {
                // No more animations - stop tick callback
                info!("Animation complete, stopping tick callback");
                WIDGET_TICK_CALLBACK_ID.with(|c| *c.borrow_mut() = None);
                WIDGET_LAST_FRAME_TIME.with(|c| *c.borrow_mut() = 0);
                glib::ControlFlow::Break
            }
        });

        WIDGET_TICK_CALLBACK_ID.with(|c| *c.borrow_mut() = Some(tick_id));
    }
}

/// Stop animation tick callback
pub fn stop_animation_tick() {
    WIDGET_TICK_CALLBACK_ID.with(|c| {
        if let Some(tick_id) = c.borrow_mut().take() {
            tick_id.remove();
            info!("Animation tick callback stopped");
        }
    });
    WIDGET_LAST_FRAME_TIME.with(|c| *c.borrow_mut() = 0);
}

/// Check if animation tick is running
pub fn is_animation_tick_running() -> bool {
    WIDGET_TICK_CALLBACK_ID.with(|c| c.borrow().is_some())
}

/// Inner state for NeomacsWidget
#[derive(Default)]
pub struct NeomacsWidgetInner {
    /// The scene to render (legacy mode)
    scene: RefCell<Option<Scene>>,
    /// GSK renderer (legacy mode)
    renderer: RefCell<GskRenderer>,
    /// Hybrid renderer (hybrid mode)
    hybrid_renderer: RefCell<HybridRenderer>,
    /// Whether Pango context has been initialized
    pango_initialized: RefCell<bool>,
    /// Last known size (to detect changes)
    last_size: RefCell<(i32, i32)>,
}

/// GObject subclass implementation
#[glib::object_subclass]
impl ObjectSubclass for NeomacsWidgetInner {
    const NAME: &'static str = "NeomacsWidget";
    type Type = NeomacsWidget;
    type ParentType = gtk4::Widget;
}

impl ObjectImpl for NeomacsWidgetInner {
    fn constructed(&self) {
        self.parent_constructed();

        let widget = self.obj();

        // Request keyboard focus and enable input targeting
        widget.set_focusable(true);
        widget.set_can_focus(true);
        widget.set_can_target(true);  // Enable receiving pointer events
        widget.set_sensitive(true);   // Enable input events

        // Store widget instance for tick callback access
        set_widget_instance(&widget);

        // Note: Mouse event handlers are added by C code in neomacsfns.c
        // (neomacs_click_pressed_cb, neomacs_motion_cb, neomacs_scroll_cb)
        // The Rust-side handlers via FFI callbacks are for cases where
        // C code wants to delegate to Rust, but currently all mouse handling
        // is done in C for proper Emacs integration.

        info!("NeomacsWidget constructed");
    }
}

impl NeomacsWidgetInner {
    fn snapshot_impl(&self, snapshot: &Snapshot) {
        let widget = self.obj();
        let width = widget.width() as f32;
        let height = widget.height() as f32;
        debug!("snapshot: size={}x{}", width, height);

        // Initialize Pango context for legacy renderer (GSK renderer still uses Pango)
        if !*self.pango_initialized.borrow() {
            let context = widget.pango_context();
            self.renderer.borrow_mut().init_with_context(context.clone());
            // Note: hybrid_renderer now uses cosmic-text, no Pango init needed
            *self.pango_initialized.borrow_mut() = true;
        }

        // Check if using hybrid mode
        let use_hybrid = WIDGET_USE_HYBRID.with(|c| *c.borrow());

        if use_hybrid {
            // Hybrid path: render from CLONED FrameGlyphBuffer (uses cosmic-text)
            // We clone the buffer when setting it, so it survives Emacs clearing
            // the original for the next frame.
            let frame_glyphs: Option<FrameGlyphBuffer> = WIDGET_FRAME_GLYPHS.with(|c| {
                c.borrow().clone()  // Clone the Option<FrameGlyphBuffer>
            });

            // Get floating images from thread-local
            let floating_images: Vec<FloatingImage> = WIDGET_FLOATING_IMAGES.with(|c| {
                c.borrow().clone()
            });

            // Get floating webkits from thread-local
            let floating_webkits: Vec<FloatingWebKit> = WIDGET_FLOATING_WEBKITS.with(|c| {
                c.borrow().clone()
            });
            
            log::trace!("snapshot_impl: floating_webkits.len() = {}", floating_webkits.len());

            if let Some(ref buffer) = frame_glyphs {

                // Get video cache from thread-local (mutable for update())
                let mut video_cache = WIDGET_VIDEO_CACHE.with(|c| {
                    c.borrow().map(|ptr| unsafe { &mut *(ptr as *const VideoCache as *mut VideoCache) })
                });

                // Get image cache from thread-local
                let mut image_cache_ptr = WIDGET_IMAGE_CACHE.with(|c| *c.borrow());
                let image_cache = image_cache_ptr.as_mut().map(|ptr| unsafe { &mut **ptr });

                // Get webkit cache from thread-local
                #[cfg(feature = "wpe-webkit")]
                let webkit_cache = WIDGET_WEBKIT_CACHE.with(|c| {
                    c.borrow().map(|ptr| unsafe { &*ptr })
                });
                #[cfg(not(feature = "wpe-webkit"))]
                let webkit_cache: Option<()> = None;

                // Try to use shared renderer from FFI layer (for animation state sharing)
                // Fall back to local renderer if not available
                let shared_renderer = get_widget_hybrid_renderer();
                
                let node = if let Some(renderer_ptr) = shared_renderer {
                    // Use the shared renderer from FFI
                    let renderer = unsafe { &mut *renderer_ptr };
                    
                    // Set scale factor for HiDPI rendering
                    let scale_factor = widget.scale_factor() as f32;
                    trace!("snapshot_impl: Using SHARED renderer, scale_factor={}", scale_factor);
                    renderer.set_scale_factor(scale_factor);
                    
                    // Check if we have a snapshot to use for transition
                    if let Some(snapshot_tex) = take_snapshot_texture() {
                        debug!("Widget: Passing snapshot to shared renderer");
                        renderer.set_snapshot_texture(snapshot_tex);
                    }
                    
                    renderer.build_render_node(buffer, video_cache, image_cache, &floating_images, &floating_webkits, webkit_cache)
                } else {
                    trace!("snapshot_impl: Using LOCAL renderer");
                    // Use the widget's local renderer (no animation support)
                    let mut renderer = self.hybrid_renderer.borrow_mut();
                    
                    // Set scale factor for HiDPI rendering
                    let scale_factor = widget.scale_factor() as f32;
                    renderer.set_scale_factor(scale_factor);
                    
                    // Check if we have a snapshot to use for transition
                    if let Some(snapshot_tex) = take_snapshot_texture() {
                        renderer.set_snapshot_texture(snapshot_tex);
                    }
                    
                    renderer.build_render_node(buffer, video_cache, image_cache, &floating_images, &floating_webkits, webkit_cache)
                };

                if let Some(node) = node {
                    // Cache this frame if frame caching is enabled (for instant buffer transitions)
                    if is_frame_caching_enabled() {
                        let native = widget.native();
                        if let Some(native) = native {
                            if let Some(gsk_renderer) = native.renderer() {
                                let rect = graphene::Rect::new(0.0, 0.0, width, height);
                                let texture = gsk_renderer.render_texture(&node, Some(&rect));
                                trace!("Cached frame for transitions: {}x{}", width, height);
                                set_last_frame_texture(texture);
                            }
                        }
                    }
                    
                    // If snapshot capture is requested (legacy path), capture explicitly
                    if is_snapshot_requested() {
                        clear_snapshot_request();
                        let native = widget.native();
                        if let Some(native) = native {
                            if let Some(gsk_renderer) = native.renderer() {
                                let rect = graphene::Rect::new(0.0, 0.0, width, height);
                                let texture = gsk_renderer.render_texture(&node, Some(&rect));
                                debug!("Captured transition snapshot: {}x{}", width, height);
                                set_snapshot_texture(texture);
                            }
                        }
                    }
                    
                    log::debug!("snapshot_impl: appending render node, bounds={:?}", node.bounds());
                    snapshot.append_node(&node);
                    
                    // Start animation tick if animations need continuous updates
                    if let Some(renderer_ptr) = shared_renderer {
                        let renderer = unsafe { &*renderer_ptr };
                        if renderer.needs_animation_frame() && !is_animation_tick_running() {
                            start_animation_tick();
                        }
                    }
                } else {
                    let rect = graphene::Rect::new(0.0, 0.0, width, height);
                    let color = gtk4::gdk::RGBA::new(0.5, 0.1, 0.1, 1.0);
                    snapshot.append_color(&color, &rect);
                }
            } else {
                // No frame glyphs - draw background
                let rect = graphene::Rect::new(0.0, 0.0, width, height);
                let color = gtk4::gdk::RGBA::new(0.1, 0.1, 0.12, 1.0);
                snapshot.append_color(&color, &rect);
            }
        } else {
            // Legacy path: render from Scene
            let scene_opt = self.scene.borrow();

            if let Some(scene) = scene_opt.as_ref() {
                // Get video cache from thread-local (set by FFI before queue_draw)
                let video_cache = WIDGET_VIDEO_CACHE.with(|c| {
                    c.borrow().map(|ptr| unsafe { &*ptr })
                });

                // Get image cache from thread-local
                let mut image_cache_ptr = WIDGET_IMAGE_CACHE.with(|c| *c.borrow());
                let image_cache = image_cache_ptr.as_mut().map(|ptr| unsafe { &mut **ptr });

                // Build render node with caches
                let mut renderer = self.renderer.borrow_mut();
                if let Some(node) = renderer.build_render_node(scene, video_cache, image_cache, None) {
                    snapshot.append_node(&node);
                }
            } else {
                // No scene - draw background
                let rect = graphene::Rect::new(0.0, 0.0, width, height);
                let color = gtk4::gdk::RGBA::new(0.1, 0.1, 0.12, 1.0);
                snapshot.append_color(&color, &rect);
            }
        }
    }
}

impl WidgetImpl for NeomacsWidgetInner {
    fn snapshot(&self, snapshot: &Snapshot) {
        // Wrap everything in catch_unwind to prevent panics from crashing Emacs
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.snapshot_impl(snapshot);
        }));
        
        if let Err(e) = result {
            eprintln!("PANIC in snapshot: {:?}", e);
            warn!("PANIC in snapshot: {:?}", e);
            // Draw error background
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;
            let rect = graphene::Rect::new(0.0, 0.0, width, height);
            let color = gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
            snapshot.append_color(&color, &rect);
        }
    }

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        // Call parent implementation
        self.parent_size_allocate(width, height, baseline);
        
        // Check if size changed
        let last_size = *self.last_size.borrow();
        if last_size != (width, height) {
            *self.last_size.borrow_mut() = (width, height);
            debug!("NeomacsWidget size_allocate: {}x{}", width, height);
            
            // Call resize callback if set
            WIDGET_RESIZE_CALLBACK.with(|c| {
                if let Some(ref callback) = *c.borrow() {
                    callback(width, height);
                }
            });
        }
    }

    fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
        // Return reasonable defaults
        match orientation {
            gtk4::Orientation::Horizontal => (400, 800, -1, -1),
            gtk4::Orientation::Vertical => (300, 600, -1, -1),
            _ => (100, 400, -1, -1),
        }
    }
}

glib::wrapper! {
    /// Custom widget for GPU-accelerated Emacs rendering.
    pub struct NeomacsWidget(ObjectSubclass<NeomacsWidgetInner>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible;
}

impl Default for NeomacsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl NeomacsWidget {
    /// Create a new NeomacsWidget
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Update the scene to render
    pub fn set_scene(&self, scene: Scene) {
        let inner = self.imp();
        *inner.scene.borrow_mut() = Some(scene);
        self.queue_draw();
    }

    /// Clear the scene
    pub fn clear_scene(&self) {
        let inner = self.imp();
        *inner.scene.borrow_mut() = None;
        self.queue_draw();
    }

    /// Get access to the renderer (for face registration, etc.)
    pub fn with_renderer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut GskRenderer) -> R,
    {
        let inner = self.imp();
        f(&mut inner.renderer.borrow_mut())
    }
}
