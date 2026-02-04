//! Glyph texture atlas for wgpu GPU rendering
//!
//! Caches rasterized glyphs as individual wgpu textures with bind groups.

use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, ShapeBuffer, SwashCache, Style, Weight,
};

use crate::core::face::Face;

/// Key for glyph cache lookup
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GlyphKey {
    /// Character code
    pub charcode: u32,
    /// Face ID (determines font, style)
    pub face_id: u32,
    /// Font size in pixels (for text-scale-increase support)
    /// Using u32 bits of f32 for hashing
    pub font_size_bits: u32,
}

/// A cached glyph with its wgpu texture and bind group
pub struct CachedGlyph {
    /// Texture containing this glyph (R8Unorm format - alpha mask)
    pub texture: wgpu::Texture,
    /// Texture view for sampling
    pub view: wgpu::TextureView,
    /// Bind group for this glyph's texture
    pub bind_group: wgpu::BindGroup,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bearing X (offset from origin)
    pub bearing_x: f32,
    /// Bearing Y (offset from baseline)
    pub bearing_y: f32,
}

/// Wgpu-based glyph atlas for text rendering
pub struct WgpuGlyphAtlas {
    /// Cached glyphs: (charcode, face_id) -> CachedGlyph
    cache: HashMap<GlyphKey, CachedGlyph>,
    /// Font system for text rendering
    font_system: FontSystem,
    /// Swash cache for glyph rasterization
    swash_cache: SwashCache,
    /// Shape buffer for text shaping
    #[allow(dead_code)]
    shape_buffer: ShapeBuffer,
    /// Bind group layout for glyph textures
    bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler for glyph textures
    sampler: wgpu::Sampler,
    /// Default font size in pixels
    default_font_size: f32,
    /// Default line height in pixels
    default_line_height: f32,
    /// Maximum cache size
    max_size: usize,
}

impl WgpuGlyphAtlas {
    /// Create a new wgpu glyph atlas
    pub fn new(device: &wgpu::Device) -> Self {
        // Create bind group layout for glyph texture + sampler
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glyph Bind Group Layout"),
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

        // Create sampler for glyph textures
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            cache: HashMap::new(),
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            shape_buffer: ShapeBuffer::default(),
            bind_group_layout,
            sampler,
            default_font_size: 13.0,
            default_line_height: 17.0,
            max_size: 4096,
        }
    }

    /// Get the bind group layout for glyph textures
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Get or create a cached glyph
    ///
    /// If the glyph is already cached, returns a reference to it.
    /// Otherwise, rasterizes the glyph, uploads to GPU, and caches it.
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &GlyphKey,
        face: Option<&Face>,
    ) -> Option<&CachedGlyph> {
        // Check cache first
        if self.cache.contains_key(key) {
            return self.cache.get(key);
        }

        // Rasterize the glyph
        let c = char::from_u32(key.charcode)?;
        let (width, height, alpha_data, bearing_x, bearing_y) =
            self.rasterize_glyph(c, face)?;

        if width == 0 || height == 0 {
            log::debug!("glyph_atlas: skipping empty glyph '{}' ({}x{})", c, width, height);
            return None;
        }

        // Check alpha data has valid content
        let max_alpha = alpha_data.iter().copied().max().unwrap_or(0);
        log::debug!("glyph_atlas: rasterized '{}' {}x{} bearing ({:.1},{:.1}) max_alpha={}",
            c, width, height, bearing_x, bearing_y, max_alpha);

        // Create texture (R8Unorm for alpha mask)
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload pixel data
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &alpha_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // Create texture view
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glyph Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // Evict old entries if cache is full
        if self.cache.len() >= self.max_size {
            let keys_to_remove: Vec<_> = self
                .cache
                .keys()
                .take(self.max_size / 2)
                .cloned()
                .collect();
            for k in keys_to_remove {
                self.cache.remove(&k);
            }
        }

        // Insert into cache
        let cached_glyph = CachedGlyph {
            texture,
            view,
            bind_group,
            width,
            height,
            bearing_x,
            bearing_y,
        };
        self.cache.insert(key.clone(), cached_glyph);
        self.cache.get(key)
    }

    /// Rasterize a single glyph and return alpha mask data
    ///
    /// Returns (width, height, alpha_data, bearing_x, bearing_y)
    fn rasterize_glyph(
        &mut self,
        c: char,
        face: Option<&Face>,
    ) -> Option<(u32, u32, Vec<u8>, f32, f32)> {
        // Create attributes from face
        let attrs = self.face_to_attrs(face);

        // Use font_size from face if available, otherwise default
        let font_size = face.map(|f| f.font_size).unwrap_or(self.default_font_size);

        // Create metrics with the face's font size
        let metrics = Metrics::new(font_size, self.default_line_height);

        // Create a small buffer for single character
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(100.0), Some(50.0));
        buffer.set_text(
            &mut self.font_system,
            &c.to_string(),
            attrs,
            cosmic_text::Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Get the glyph info
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                // Rasterize at 1x scale
                let physical_glyph = glyph.physical((0.0, 0.0), 1.0);

                if let Some(image) = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical_glyph.cache_key)
                {
                    let width = image.placement.width as u32;
                    let height = image.placement.height as u32;

                    if width == 0 || height == 0 {
                        continue;
                    }

                    let bearing_x = image.placement.left as f32;
                    let bearing_y = image.placement.top as f32;

                    // Extract alpha channel based on content type
                    let alpha_data = match image.content {
                        cosmic_text::SwashContent::Mask => {
                            // Already an alpha mask
                            image.data.clone()
                        }
                        cosmic_text::SwashContent::Color => {
                            // Extract alpha from RGBA
                            image.data.chunks(4).map(|chunk| chunk[3]).collect()
                        }
                        cosmic_text::SwashContent::SubpixelMask => {
                            // Average RGB for alpha
                            image
                                .data
                                .chunks(3)
                                .map(|chunk| {
                                    ((chunk[0] as u16 + chunk[1] as u16 + chunk[2] as u16) / 3)
                                        as u8
                                })
                                .collect()
                        }
                    };

                    return Some((width, height, alpha_data, bearing_x, bearing_y));
                }
            }
        }

        None
    }

    /// Convert Face to cosmic-text Attrs
    fn face_to_attrs(&self, face: Option<&Face>) -> Attrs<'static> {
        let mut attrs = Attrs::new();

        if let Some(f) = face {
            // Font family
            attrs = match f.font_family.to_lowercase().as_str() {
                "monospace" | "mono" => attrs.family(Family::Monospace),
                "serif" => attrs.family(Family::Serif),
                "sans-serif" | "sans" => attrs.family(Family::SansSerif),
                _ => attrs.family(Family::Monospace),
            };

            // Font weight
            attrs = attrs.weight(Weight(f.font_weight));

            // Font style (italic)
            if f.attributes.contains(crate::core::face::FaceAttributes::ITALIC) {
                attrs = attrs.style(Style::Italic);
            }
        } else {
            attrs = attrs.family(Family::Monospace);
        }

        attrs
    }

    /// Get a cached glyph without creating it
    ///
    /// Returns a reference to the cached glyph if it exists.
    /// This is useful for immutable access after glyphs have been cached.
    pub fn get(&self, key: &GlyphKey) -> Option<&CachedGlyph> {
        self.cache.get(key)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached glyphs
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Set font metrics
    pub fn set_metrics(&mut self, font_size: f32, line_height: f32) {
        if (self.default_font_size - font_size).abs() > 0.1
            || (self.default_line_height - line_height).abs() > 0.1
        {
            self.default_font_size = font_size;
            self.default_line_height = line_height;
            // Clear cache when metrics change
            self.clear();
        }
    }
}
