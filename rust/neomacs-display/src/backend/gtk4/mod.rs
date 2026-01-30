//! GTK4/GSK GPU-accelerated display backend.

mod renderer;
mod image;

use std::sync::{Arc, Mutex};

use gtk4::prelude::*;
use gtk4::{cairo, DrawingArea};

use crate::core::error::{DisplayError, DisplayResult};
use crate::core::scene::Scene;
use crate::core::face::Face;
use crate::backend::DisplayBackend;

pub use renderer::Gtk4Renderer;

/// Shared state for the GTK4 backend
struct Gtk4State {
    scene: Scene,
    renderer: Gtk4Renderer,
    needs_redraw: bool,
}

/// GTK4 backend state
pub struct Gtk4Backend {
    initialized: bool,
    width: u32,
    height: u32,
    vsync: bool,
    state: Option<Arc<Mutex<Gtk4State>>>,
    drawing_area: Option<DrawingArea>,
}

impl Default for Gtk4Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl Gtk4Backend {
    pub fn new() -> Self {
        Self {
            initialized: false,
            width: 800,
            height: 600,
            vsync: true,
            state: None,
            drawing_area: None,
        }
    }

    /// Create a drawing area with our render callback
    pub fn create_drawing_area(&mut self) -> DrawingArea {
        let state = Arc::new(Mutex::new(Gtk4State {
            scene: Scene::new(self.width as f32, self.height as f32),
            renderer: Gtk4Renderer::new(),
            needs_redraw: true,
        }));

        self.state = Some(state.clone());

        let drawing_area = DrawingArea::new();
        drawing_area.set_content_width(self.width as i32);
        drawing_area.set_content_height(self.height as i32);

        // Set up the draw function (receives Cairo context)
        let state_clone = state.clone();
        drawing_area.set_draw_func(move |_da, cr, width, height| {
            if let Ok(mut state) = state_clone.lock() {
                // Update scene dimensions if changed
                state.scene.width = width as f32;
                state.scene.height = height as f32;
                
                // Render the scene
                state.renderer.render(cr, &state.scene);
                state.needs_redraw = false;
            }
        });

        // Initialize pango context after drawing area is realized
        let state_for_realize = state.clone();
        drawing_area.connect_realize(move |da| {
            let context = da.pango_context();
            if let Ok(mut state) = state_for_realize.lock() {
                state.renderer.init_with_context(context);
            }
        });

        self.drawing_area = Some(drawing_area.clone());
        drawing_area
    }

    /// Update the scene (call before render)
    pub fn update_scene(&mut self, scene: Scene) {
        if let Some(state) = &self.state {
            if let Ok(mut state) = state.lock() {
                state.scene = scene;
                state.needs_redraw = true;
            }
        }
    }

    /// Register a face with the renderer
    pub fn register_face(&mut self, face: Face) -> u32 {
        if let Some(state) = &self.state {
            if let Ok(mut state) = state.lock() {
                return state.renderer.register_face(face);
            }
        }
        face.id
    }
}

impl DisplayBackend for Gtk4Backend {
    fn init(&mut self) -> DisplayResult<()> {
        // GTK4 should be initialized via gtk::init() before creating backend
        // We just mark ourselves as ready
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) {
        self.state = None;
        self.drawing_area = None;
        self.initialized = false;
    }

    fn render(&mut self, scene: &Scene) -> DisplayResult<()> {
        if !self.initialized {
            return Err(DisplayError::Backend("GTK4 backend not initialized".into()));
        }

        // Update our internal scene copy
        if let Some(state) = &self.state {
            if let Ok(mut state) = state.lock() {
                // Clone the scene data we need
                state.scene.width = scene.width;
                state.scene.height = scene.height;
                state.scene.background = scene.background;
                state.scene.windows = scene.windows.clone();
                state.scene.dirty = scene.dirty;
                state.needs_redraw = true;
            }
        }

        Ok(())
    }

    fn present(&mut self) -> DisplayResult<()> {
        if !self.initialized {
            return Err(DisplayError::Backend("GTK4 backend not initialized".into()));
        }

        // Queue a redraw on the drawing area
        if let Some(da) = &self.drawing_area {
            da.queue_draw();
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "gtk4"
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        
        if let Some(da) = &self.drawing_area {
            da.set_content_width(width as i32);
            da.set_content_height(height as i32);
        }
    }

    fn set_vsync(&mut self, enabled: bool) {
        self.vsync = enabled;
        // GTK4 always uses vsync via frame clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = Gtk4Backend::new();
        assert!(!backend.is_initialized());
        assert_eq!(backend.name(), "gtk4");
    }
}
