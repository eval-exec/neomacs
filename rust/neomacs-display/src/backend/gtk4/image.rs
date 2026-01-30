//! Image loading and caching for GTK4 backend.

use std::collections::HashMap;
use std::path::Path;

use gtk4::cairo;
use gtk4::gdk_pixbuf::{Pixbuf, PixbufLoader};
use gtk4::gdk_pixbuf::prelude::*;
use gtk4::glib;

use crate::core::error::{DisplayError, DisplayResult};

/// Cached image data
pub struct CachedImage {
    /// Cairo surface for rendering
    pub surface: cairo::ImageSurface,
    
    /// Original width
    pub width: i32,
    
    /// Original height
    pub height: i32,
    
    /// Animation frames (for GIFs)
    pub frames: Option<Vec<AnimationFrame>>,
    
    /// Current frame index
    pub current_frame: usize,
    
    /// Is this an animated image?
    pub is_animated: bool,
}

/// A single animation frame
pub struct AnimationFrame {
    /// Frame surface
    pub surface: cairo::ImageSurface,
    
    /// Delay in milliseconds before next frame
    pub delay_ms: u32,
}

/// Image cache for efficient rendering
#[derive(Default)]
pub struct ImageCache {
    images: HashMap<u32, CachedImage>,
    next_id: u32,
}

impl ImageCache {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            next_id: 1,
        }
    }
    
    /// Load an image from file path
    pub fn load_from_file(&mut self, path: &Path) -> DisplayResult<u32> {
        let pixbuf = Pixbuf::from_file(path)
            .map_err(|e| DisplayError::Backend(format!("Failed to load image: {}", e)))?;
        
        let surface = pixbuf_to_surface(&pixbuf)?;
        
        let id = self.next_id;
        self.next_id += 1;
        
        self.images.insert(id, CachedImage {
            width: pixbuf.width(),
            height: pixbuf.height(),
            surface,
            frames: None,
            current_frame: 0,
            is_animated: false,
        });
        
        Ok(id)
    }
    
    /// Load an image from raw bytes
    pub fn load_from_bytes(&mut self, data: &[u8]) -> DisplayResult<u32> {
        let loader = PixbufLoader::new();
        loader.write(data)
            .map_err(|e| DisplayError::Backend(format!("Failed to parse image: {}", e)))?;
        loader.close()
            .map_err(|e| DisplayError::Backend(format!("Failed to close loader: {}", e)))?;
        
        let pixbuf = loader.pixbuf()
            .ok_or_else(|| DisplayError::Backend("No pixbuf from loader".into()))?;
        
        let surface = pixbuf_to_surface(&pixbuf)?;
        
        let id = self.next_id;
        self.next_id += 1;
        
        self.images.insert(id, CachedImage {
            width: pixbuf.width(),
            height: pixbuf.height(),
            surface,
            frames: None,
            current_frame: 0,
            is_animated: false,
        });
        
        Ok(id)
    }
    
    /// Load an animated GIF from file
    pub fn load_animation_from_file(&mut self, path: &Path) -> DisplayResult<u32> {
        use std::time::SystemTime;
        
        let animation = gtk4::gdk_pixbuf::PixbufAnimation::from_file(path)
            .map_err(|e| DisplayError::Backend(format!("Failed to load animation: {}", e)))?;
        
        if animation.is_static_image() {
            // Not actually animated, load as regular image
            return self.load_from_file(path);
        }
        
        // Get all frames
        let mut frames = Vec::new();
        let mut iter = animation.iter(Some(SystemTime::now()));
        
        // Get first frame's dimensions
        let first_pixbuf = iter.pixbuf();
        let width = first_pixbuf.width();
        let height = first_pixbuf.height();
        
        // Track start time for frame extraction
        let start_time = SystemTime::now();
        
        loop {
            let pixbuf = iter.pixbuf();
            let surface = pixbuf_to_surface(&pixbuf)?;
            
            // Get delay time - convert Option<Duration> to milliseconds
            let delay_ms = iter.delay_time()
                .map(|d| d.as_millis() as u32)
                .unwrap_or(100)
                .max(16); // Minimum 16ms (60fps)
            
            frames.push(AnimationFrame {
                surface,
                delay_ms,
            });
            
            // Advance to next frame using current system time
            let current_time = SystemTime::now();
            if !iter.advance(current_time) {
                break;
            }
            
            // Safety limit
            if frames.len() >= 1000 {
                break;
            }
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        // Use first frame as main surface
        let main_surface = if !frames.is_empty() {
            // Clone first frame's surface
            let first = &frames[0];
            clone_surface(&first.surface)?
        } else {
            pixbuf_to_surface(&first_pixbuf)?
        };
        
        self.images.insert(id, CachedImage {
            width,
            height,
            surface: main_surface,
            frames: Some(frames),
            current_frame: 0,
            is_animated: true,
        });
        
        Ok(id)
    }
    
    /// Get a cached image
    pub fn get(&self, id: u32) -> Option<&CachedImage> {
        self.images.get(&id)
    }
    
    /// Get a cached image mutably (for animation updates)
    pub fn get_mut(&mut self, id: u32) -> Option<&mut CachedImage> {
        self.images.get_mut(&id)
    }
    
    /// Advance animation frame and return new delay
    pub fn advance_animation(&mut self, id: u32) -> Option<u32> {
        let image = self.images.get_mut(&id)?;
        
        if !image.is_animated {
            return None;
        }
        
        let frames = image.frames.as_ref()?;
        
        if frames.is_empty() {
            return None;
        }
        
        // Move to next frame
        image.current_frame = (image.current_frame + 1) % frames.len();
        
        // Update main surface to current frame
        if let Ok(new_surface) = clone_surface(&frames[image.current_frame].surface) {
            image.surface = new_surface;
        }
        
        Some(frames[image.current_frame].delay_ms)
    }
    
    /// Remove an image from cache
    pub fn remove(&mut self, id: u32) -> bool {
        self.images.remove(&id).is_some()
    }
    
    /// Clear all cached images
    pub fn clear(&mut self) {
        self.images.clear();
    }
    
    /// Get number of cached images
    pub fn len(&self) -> usize {
        self.images.len()
    }
    
    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// Convert a GdkPixbuf to a Cairo ImageSurface
fn pixbuf_to_surface(pixbuf: &Pixbuf) -> DisplayResult<cairo::ImageSurface> {
    let width = pixbuf.width();
    let height = pixbuf.height();
    let has_alpha = pixbuf.has_alpha();
    let n_channels = pixbuf.n_channels();
    let rowstride = pixbuf.rowstride() as usize;
    
    let format = if has_alpha {
        cairo::Format::ARgb32
    } else {
        cairo::Format::Rgb24
    };
    
    let mut surface = cairo::ImageSurface::create(format, width, height)
        .map_err(|e| DisplayError::Backend(format!("Failed to create surface: {}", e)))?;
    
    // Get surface stride before borrowing data
    let stride = surface.stride() as usize;
    
    // Get pixbuf data
    let pixels = unsafe { pixbuf.pixels() };
    
    // Copy pixel data manually (convert RGBA/RGB to BGRA)
    {
        let mut surface_data = surface.data()
            .map_err(|e| DisplayError::Backend(format!("Failed to get surface data: {}", e)))?;
        
        for y in 0..height as usize {
            let src_row = &pixels[y * rowstride..];
            let dst_row = &mut surface_data[y * stride..];
            
            for x in 0..width as usize {
                let src_offset = x * n_channels as usize;
                let dst_offset = x * 4;
                
                let r = src_row[src_offset];
                let g = src_row[src_offset + 1];
                let b = src_row[src_offset + 2];
                let a = if has_alpha { src_row[src_offset + 3] } else { 255 };
                
                // Cairo uses BGRA format (or premultiplied ARGB on some systems)
                // Actually it's native-endian ARGB32, which on little-endian is BGRA in memory
                dst_row[dst_offset] = b;
                dst_row[dst_offset + 1] = g;
                dst_row[dst_offset + 2] = r;
                dst_row[dst_offset + 3] = a;
            }
        }
    }
    
    surface.mark_dirty();
    Ok(surface)
}

/// Clone a Cairo surface
fn clone_surface(source: &cairo::ImageSurface) -> DisplayResult<cairo::ImageSurface> {
    let width = source.width();
    let height = source.height();
    let format = source.format();
    
    let new_surface = cairo::ImageSurface::create(format, width, height)
        .map_err(|e| DisplayError::Backend(format!("Failed to create surface: {}", e)))?;
    
    let cr = cairo::Context::new(&new_surface)
        .map_err(|e| DisplayError::Backend(format!("Failed to create context: {}", e)))?;
    
    cr.set_source_surface(source, 0.0, 0.0)
        .map_err(|e| DisplayError::Backend(format!("Failed to set source: {}", e)))?;
    cr.paint()
        .map_err(|e| DisplayError::Backend(format!("Failed to paint: {}", e)))?;
    
    Ok(new_surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_creation() {
        let cache = ImageCache::new();
        assert!(cache.is_empty());
    }
}
