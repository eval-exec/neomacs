//! Glyph texture atlas for wgpu GPU rendering
//!
//! Caches rasterized glyphs as individual wgpu textures with bind groups.

use std::collections::{HashMap, HashSet};

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

/// Key for composed (multi-codepoint) glyph cache lookup.
/// Used for grapheme clusters like emoji ZWJ sequences, combining diacritics, etc.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ComposedGlyphKey {
    /// The full text of the composed grapheme cluster
    pub text: Box<str>,
    /// Face ID (determines font, style)
    pub face_id: u32,
    /// Font size in pixels (using u32 bits of f32 for hashing)
    pub font_size_bits: u32,
}

/// A cached glyph with its wgpu texture and bind group
pub struct CachedGlyph {
    /// Texture containing this glyph
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
    /// True if this is a color glyph (RGBA texture, e.g. color emoji).
    /// Color glyphs should be rendered with the image pipeline (direct RGBA),
    /// not the glyph pipeline (alpha-mask tinted with foreground color).
    pub is_color: bool,
    /// Frame generation when this glyph was last accessed
    last_accessed: u64,
}

/// Wgpu-based glyph atlas for text rendering
pub struct WgpuGlyphAtlas {
    /// Cached glyphs: (charcode, face_id) -> CachedGlyph
    cache: HashMap<GlyphKey, CachedGlyph>,
    /// Cached composed glyphs (multi-codepoint grapheme clusters)
    composed_cache: HashMap<ComposedGlyphKey, CachedGlyph>,
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
    /// Display scale factor for HiDPI rasterization
    scale_factor: f32,
    /// Maximum cache size
    max_size: usize,
    /// Interned font family names (avoids Box::leak memory growth)
    interned_families: HashSet<&'static str>,
    /// Frame generation counter (incremented each frame)
    generation: u64,
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
        // Use Linear filtering for smooth antialiased text
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
            composed_cache: HashMap::new(),
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            shape_buffer: ShapeBuffer::default(),
            bind_group_layout,
            sampler,
            default_font_size: 13.0,
            default_line_height: 17.0,
            scale_factor: 1.0,
            max_size: 4096,
            interned_families: HashSet::new(),
            generation: 0,
        }
    }

    /// Create a new wgpu glyph atlas with a specific scale factor for HiDPI
    pub fn new_with_scale(device: &wgpu::Device, scale_factor: f32) -> Self {
        let mut atlas = Self::new(device);
        atlas.scale_factor = scale_factor;
        atlas
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
        // Check cache first — update access generation on hit
        if let Some(cached) = self.cache.get_mut(key) {
            cached.last_accessed = self.generation;
            return self.cache.get(key);
        }

        // Rasterize the glyph
        let c = char::from_u32(key.charcode)?;

        // Whitespace characters (space, tab, newline, carriage return) don't need
        // visible glyphs - their backgrounds are handled separately by the renderer.
        // Return None silently without warning.
        if c.is_whitespace() {
            return None;
        }

        let rasterize_result = self.rasterize_glyph(c, face);
        if rasterize_result.is_none() {
            log::warn!("glyph_atlas: failed to rasterize '{}' (U+{:04X}) face_id={} has_face={}",
                c, key.charcode, key.face_id, face.is_some());
            return None;
        }
        let (width, height, pixel_data, bearing_x, bearing_y, is_color) = rasterize_result?;

        if width == 0 || height == 0 {
            log::debug!("glyph_atlas: skipping empty glyph '{}' ({}x{})", c, width, height);
            return None;
        }

        log::debug!("glyph_atlas: rasterized '{}' {}x{} bearing ({:.1},{:.1}) color={}",
            c, width, height, bearing_x, bearing_y, is_color);

        // Color glyphs use Rgba8UnormSrgb (4 bytes/pixel), mask glyphs use R8Unorm (1 byte/pixel)
        let (format, bytes_per_pixel) = if is_color {
            (wgpu::TextureFormat::Rgba8UnormSrgb, 4u32)
        } else {
            (wgpu::TextureFormat::R8Unorm, 1u32)
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(if is_color { "Color Glyph Texture" } else { "Glyph Texture" }),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
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
            &pixel_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * bytes_per_pixel),
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

        // Evict least-recently-used entries if cache is full
        if self.cache.len() >= self.max_size {
            let mut entries: Vec<_> = self.cache.iter()
                .map(|(k, v)| (k.clone(), v.last_accessed))
                .collect();
            entries.sort_by_key(|(_, gen)| *gen);
            let evict_count = self.max_size / 4;
            for (k, _) in entries.into_iter().take(evict_count) {
                self.cache.remove(&k);
            }
        }

        // Insert into cache
        let gen = self.generation;
        let cached_glyph = CachedGlyph {
            texture,
            view,
            bind_group,
            width,
            height,
            bearing_x,
            bearing_y,
            is_color,
            last_accessed: gen,
        };
        self.cache.insert(key.clone(), cached_glyph);
        self.cache.get(key)
    }

    /// Get or create a cached glyph for a composed (multi-codepoint) grapheme cluster.
    ///
    /// Used for emoji ZWJ sequences, combining diacritics, etc.
    pub fn get_or_create_composed(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        face_id: u32,
        font_size_bits: u32,
        face: Option<&Face>,
    ) -> Option<&CachedGlyph> {
        let key = ComposedGlyphKey {
            text: text.into(),
            face_id,
            font_size_bits,
        };

        // Check cache first
        if let Some(cached) = self.composed_cache.get_mut(&key) {
            cached.last_accessed = self.generation;
            let key2 = key.clone();
            return self.composed_cache.get(&key2);
        }

        // Rasterize the composed text
        let rasterize_result = self.rasterize_text(text, face);
        if rasterize_result.is_none() {
            log::warn!("glyph_atlas: failed to rasterize composed text '{}'", text);
            return None;
        }
        let (width, height, pixel_data, bearing_x, bearing_y, is_color) = rasterize_result?;

        if width == 0 || height == 0 {
            return None;
        }

        // Create GPU texture (same logic as single-char path)
        let (format, bytes_per_pixel) = if is_color {
            (wgpu::TextureFormat::Rgba8UnormSrgb, 4u32)
        } else {
            (wgpu::TextureFormat::R8Unorm, 1u32)
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Composed Glyph Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
            },
            &pixel_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * bytes_per_pixel),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composed Glyph Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });

        let gen = self.generation;
        self.composed_cache.insert(key.clone(), CachedGlyph {
            texture, view, bind_group, width, height,
            bearing_x, bearing_y, is_color, last_accessed: gen,
        });
        self.composed_cache.get(&key)
    }

    /// Get a cached composed glyph without creating it
    pub fn get_composed(&self, key: &ComposedGlyphKey) -> Option<&CachedGlyph> {
        self.composed_cache.get(key)
    }

    /// Rasterize text (single char or multi-codepoint sequence) and return pixel data.
    ///
    /// Returns (width, height, pixel_data, bearing_x, bearing_y, is_color)
    /// - For mask glyphs: pixel_data is R8 alpha, is_color=false
    /// - For color glyphs: pixel_data is RGBA, is_color=true
    fn rasterize_text(
        &mut self,
        text: &str,
        face: Option<&Face>,
    ) -> Option<(u32, u32, Vec<u8>, f32, f32, bool)> {
        // Create attributes from face
        let attrs = self.face_to_attrs(face);

        // Use font_size from face if available, otherwise default
        let font_size = face.map(|f| f.font_size).unwrap_or(self.default_font_size);

        // Create metrics with the face's font size
        let line_height = font_size * 1.3;
        let metrics = Metrics::new(font_size, line_height);

        // Create a small buffer for the text
        // Make buffer large enough for large fonts and multi-char sequences
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(font_size * 8.0), Some(font_size * 3.0));
        buffer.set_text(
            &mut self.font_system,
            text,
            attrs,
            cosmic_text::Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // For multi-glyph sequences (e.g. emoji ZWJ), we need to composite
        // all sub-glyphs into a single texture. Collect them first.
        let mut sub_glyphs: Vec<(f32, f32, u32, u32, Vec<u8>, bool)> = Vec::new();

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical_glyph = glyph.physical((0.0, 0.0), self.scale_factor);

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

                    let font_family_str = face.map(|f| f.font_family.as_str()).unwrap_or("(none)");
                    log::debug!(
                        "rasterize_text: text='{}' glyph U+{:04X} font='{}' content={:?} size={}x{}",
                        text, glyph.start, font_family_str, image.content, width, height
                    );

                    let (pixel_data, is_color) = match image.content {
                        cosmic_text::SwashContent::Mask => {
                            (image.data.clone(), false)
                        }
                        cosmic_text::SwashContent::Color => {
                            (image.data.clone(), true)
                        }
                        cosmic_text::SwashContent::SubpixelMask => {
                            let alpha: Vec<u8> = image
                                .data
                                .chunks(3)
                                .map(|chunk| {
                                    ((chunk[0] as u16 + chunk[1] as u16 + chunk[2] as u16) / 3)
                                        as u8
                                })
                                .collect();
                            (alpha, false)
                        }
                    };

                    sub_glyphs.push((bearing_x, bearing_y, width, height, pixel_data, is_color));
                }
            }
        }

        if sub_glyphs.is_empty() {
            return None;
        }

        // Single glyph: return directly (common case for single chars and
        // composed emoji that the font renders as a single glyph)
        if sub_glyphs.len() == 1 {
            let (bx, by, w, h, data, is_color) = sub_glyphs.into_iter().next().unwrap();
            return Some((w, h, data, bx, by, is_color));
        }

        // Multiple sub-glyphs: composite into a single RGBA texture.
        // Find bounding box of all sub-glyphs.
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut any_color = false;

        for (bx, by, w, h, _, is_color) in &sub_glyphs {
            min_x = min_x.min(*bx);
            max_x = max_x.max(*bx + *w as f32);
            min_y = min_y.min(-*by);  // bearing_y is distance from baseline (positive = up)
            max_y = max_y.max(-*by + *h as f32);
            if *is_color { any_color = true; }
        }

        let total_w = (max_x - min_x).ceil() as u32;
        let total_h = (max_y - min_y).ceil() as u32;

        if total_w == 0 || total_h == 0 {
            return None;
        }

        // Composite all sub-glyphs into a single RGBA buffer
        let bpp = 4u32; // always RGBA for composited result
        let mut composite = vec![0u8; (total_w * total_h * bpp) as usize];

        for (bx, by, w, h, data, is_color) in &sub_glyphs {
            let ox = (*bx - min_x).round() as i32;
            let oy = (-*by - min_y).round() as i32;

            for py in 0..*h {
                for px in 0..*w {
                    let dx = ox + px as i32;
                    let dy = oy + py as i32;
                    if dx < 0 || dy < 0 || dx >= total_w as i32 || dy >= total_h as i32 {
                        continue;
                    }
                    let dst_idx = ((dy as u32 * total_w + dx as u32) * bpp) as usize;
                    if *is_color {
                        // RGBA source
                        let src_idx = ((py * *w + px) * 4) as usize;
                        if src_idx + 3 < data.len() {
                            let sa = data[src_idx + 3] as u32;
                            if sa > 0 {
                                // Alpha composite (premultiplied)
                                let da = composite[dst_idx + 3] as u32;
                                let inv_sa = 255 - sa;
                                composite[dst_idx] = ((data[src_idx] as u32 * sa + composite[dst_idx] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 1] = ((data[src_idx + 1] as u32 * sa + composite[dst_idx + 1] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 2] = ((data[src_idx + 2] as u32 * sa + composite[dst_idx + 2] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 3] = (sa + da * inv_sa / 255) as u8;
                            }
                        }
                    } else {
                        // Alpha mask source — treat as white text with alpha
                        let src_idx = (py * *w + px) as usize;
                        if src_idx < data.len() {
                            let sa = data[src_idx] as u32;
                            if sa > 0 {
                                let da = composite[dst_idx + 3] as u32;
                                let inv_sa = 255 - sa;
                                composite[dst_idx] = ((255 * sa + composite[dst_idx] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 1] = ((255 * sa + composite[dst_idx + 1] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 2] = ((255 * sa + composite[dst_idx + 2] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 3] = (sa + da * inv_sa / 255) as u8;
                            }
                        }
                    }
                }
            }
        }

        // For composited result with mixed content, always use color (RGBA)
        Some((total_w, total_h, composite, min_x, -min_y, any_color || sub_glyphs.len() > 1))
    }

    /// Rasterize a single glyph and return pixel data (convenience wrapper)
    fn rasterize_glyph(
        &mut self,
        c: char,
        face: Option<&Face>,
    ) -> Option<(u32, u32, Vec<u8>, f32, f32, bool)> {
        self.rasterize_text(&c.to_string(), face)
    }

    /// Convert Face to cosmic-text Attrs
    fn face_to_attrs(&mut self, face: Option<&Face>) -> Attrs<'static> {
        let mut attrs = Attrs::new();

        if let Some(f) = face {
            // Font family - support specific font names
            let family_lower = f.font_family.to_lowercase();
            attrs = match family_lower.as_str() {
                "monospace" | "mono" | "" => attrs.family(Family::Monospace),
                "serif" => attrs.family(Family::Serif),
                "sans-serif" | "sans" | "sansserif" => attrs.family(Family::SansSerif),
                // For specific font names, intern the string to get 'static lifetime
                // without unbounded memory growth (each unique name leaked only once)
                _ => {
                    let interned = if let Some(&existing) = self.interned_families.get(f.font_family.as_str()) {
                        existing
                    } else {
                        let leaked: &'static str = Box::leak(f.font_family.clone().into_boxed_str());
                        self.interned_families.insert(leaked);
                        leaked
                    };
                    attrs.family(Family::Name(interned))
                }
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
        self.composed_cache.clear();
    }

    /// Update the scale factor and clear the cache so glyphs are
    /// re-rasterized at the new DPI.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        if (self.scale_factor - scale_factor).abs() > 0.001 {
            self.scale_factor = scale_factor;
            self.cache.clear();
            self.composed_cache.clear();
            log::info!("Glyph atlas: scale factor -> {}, cache cleared", scale_factor);
        }
    }

    /// Get the number of cached glyphs
    pub fn len(&self) -> usize {
        self.cache.len() + self.composed_cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty() && self.composed_cache.is_empty()
    }

    /// Get the default font size
    pub fn default_font_size(&self) -> f32 {
        self.default_font_size
    }

    /// Get the default line height
    pub fn default_line_height(&self) -> f32 {
        self.default_line_height
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

    /// Advance the frame generation counter.
    /// Call once per frame before rendering.
    /// Also evicts stale composed glyphs (not accessed for 60+ frames).
    pub fn advance_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        // Evict stale composed glyphs (they're less likely to be reused).
        // Threshold raised from 256 to 1024 to accommodate ligature runs
        // which generate more composed cache entries per frame.
        if self.composed_cache.len() > 1024 {
            let cutoff = self.generation.saturating_sub(60);
            self.composed_cache.retain(|_, v| v.last_accessed >= cutoff);
        }
    }
}
