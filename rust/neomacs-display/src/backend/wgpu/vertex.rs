//! Vertex types for wgpu rendering.

use bytemuck::{Pod, Zeroable};

/// Vertex for solid color rectangles.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RectVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl RectVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RectVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Vertex for textured quads (images, video, webkit).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
}

impl TextureVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextureVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// Vertex for glyph rendering (textured with color).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GlyphVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
}

impl GlyphVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    // position (2) + tex_coords (2) = 4 floats offset
                    offset: (std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<[f32; 2]>()) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Vertex for SDF rounded rectangle borders.
///
/// Each vertex carries the full rect geometry so the fragment shader can
/// compute the signed distance field per-pixel for anti-aliased rounded corners.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RoundedRectVertex {
    /// Quad corner position (screen pixels, slightly oversized for AA fringe)
    pub position: [f32; 2],
    /// Border color (RGBA, linear)
    pub color: [f32; 4],
    /// Top-left corner of the logical box (screen pixels)
    pub rect_min: [f32; 2],
    /// Bottom-right corner of the logical box (screen pixels)
    pub rect_max: [f32; 2],
    /// [border_width, corner_radius] in pixels
    pub params: [f32; 2],
}

impl RoundedRectVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem::size_of;
        wgpu::VertexBufferLayout {
            array_stride: size_of::<RoundedRectVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // @location(0) position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // @location(1) color
                wgpu::VertexAttribute {
                    offset: size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // @location(2) rect_min
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 2]>() + size_of::<[f32; 4]>()) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // @location(3) rect_max
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 2]>() + size_of::<[f32; 4]>() + size_of::<[f32; 2]>())
                        as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // @location(4) params
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 2]>() + size_of::<[f32; 4]>() + size_of::<[f32; 2]>()
                        + size_of::<[f32; 2]>()) as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// Uniforms passed to shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub screen_size: [f32; 2],
    pub _padding: [f32; 2],
}

/// Uniforms for the Gaussian blur shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BlurUniforms {
    /// Texel size: (1.0/width, 1.0/height)
    pub texel_size: [f32; 2],
    /// Blur direction: (1,0) for horizontal, (0,1) for vertical
    pub direction: [f32; 2],
    /// Blur radius multiplier
    pub radius: f32,
    pub _pad: [f32; 3],
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{size_of, align_of};

    // ---- Struct size tests ----

    #[test]
    fn rect_vertex_size() {
        // position: [f32; 2] = 8 bytes, color: [f32; 4] = 16 bytes => 24 bytes
        assert_eq!(size_of::<RectVertex>(), 24);
    }

    #[test]
    fn texture_vertex_size() {
        // position: [f32; 2] = 8 bytes, tex_coords: [f32; 2] = 8 bytes => 16 bytes
        assert_eq!(size_of::<TextureVertex>(), 16);
    }

    #[test]
    fn glyph_vertex_size() {
        // position: [f32; 2] = 8, tex_coords: [f32; 2] = 8, color: [f32; 4] = 16 => 32 bytes
        assert_eq!(size_of::<GlyphVertex>(), 32);
    }

    #[test]
    fn rounded_rect_vertex_size() {
        // position: [f32; 2] = 8, color: [f32; 4] = 16, rect_min: [f32; 2] = 8,
        // rect_max: [f32; 2] = 8, params: [f32; 2] = 8 => 48 bytes
        assert_eq!(size_of::<RoundedRectVertex>(), 48);
    }

    #[test]
    fn uniforms_size() {
        // screen_size: [f32; 2] = 8, _padding: [f32; 2] = 8 => 16 bytes
        assert_eq!(size_of::<Uniforms>(), 16);
    }

    // ---- Alignment tests (all repr(C) with f32 fields, should be 4-byte aligned) ----

    #[test]
    fn rect_vertex_alignment() {
        assert_eq!(align_of::<RectVertex>(), 4);
    }

    #[test]
    fn texture_vertex_alignment() {
        assert_eq!(align_of::<TextureVertex>(), 4);
    }

    #[test]
    fn glyph_vertex_alignment() {
        assert_eq!(align_of::<GlyphVertex>(), 4);
    }

    #[test]
    fn rounded_rect_vertex_alignment() {
        assert_eq!(align_of::<RoundedRectVertex>(), 4);
    }

    #[test]
    fn uniforms_alignment() {
        assert_eq!(align_of::<Uniforms>(), 4);
    }

    // ---- RectVertex descriptor tests ----

    #[test]
    fn rect_vertex_desc_array_stride() {
        let desc = RectVertex::desc();
        assert_eq!(desc.array_stride, size_of::<RectVertex>() as u64);
    }

    #[test]
    fn rect_vertex_desc_step_mode() {
        let desc = RectVertex::desc();
        assert_eq!(desc.step_mode, wgpu::VertexStepMode::Vertex);
    }

    #[test]
    fn rect_vertex_desc_attribute_count() {
        let desc = RectVertex::desc();
        assert_eq!(desc.attributes.len(), 2);
    }

    #[test]
    fn rect_vertex_desc_position_attribute() {
        let desc = RectVertex::desc();
        let attr = &desc.attributes[0];
        assert_eq!(attr.offset, 0);
        assert_eq!(attr.shader_location, 0);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn rect_vertex_desc_color_attribute() {
        let desc = RectVertex::desc();
        let attr = &desc.attributes[1];
        assert_eq!(attr.offset, 8); // after [f32; 2]
        assert_eq!(attr.shader_location, 1);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x4);
    }

    // ---- TextureVertex descriptor tests ----

    #[test]
    fn texture_vertex_desc_array_stride() {
        let desc = TextureVertex::desc();
        assert_eq!(desc.array_stride, size_of::<TextureVertex>() as u64);
    }

    #[test]
    fn texture_vertex_desc_step_mode() {
        let desc = TextureVertex::desc();
        assert_eq!(desc.step_mode, wgpu::VertexStepMode::Vertex);
    }

    #[test]
    fn texture_vertex_desc_attribute_count() {
        let desc = TextureVertex::desc();
        assert_eq!(desc.attributes.len(), 2);
    }

    #[test]
    fn texture_vertex_desc_position_attribute() {
        let desc = TextureVertex::desc();
        let attr = &desc.attributes[0];
        assert_eq!(attr.offset, 0);
        assert_eq!(attr.shader_location, 0);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn texture_vertex_desc_tex_coords_attribute() {
        let desc = TextureVertex::desc();
        let attr = &desc.attributes[1];
        assert_eq!(attr.offset, 8); // after [f32; 2]
        assert_eq!(attr.shader_location, 1);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    // ---- GlyphVertex descriptor tests ----

    #[test]
    fn glyph_vertex_desc_array_stride() {
        let desc = GlyphVertex::desc();
        assert_eq!(desc.array_stride, size_of::<GlyphVertex>() as u64);
    }

    #[test]
    fn glyph_vertex_desc_step_mode() {
        let desc = GlyphVertex::desc();
        assert_eq!(desc.step_mode, wgpu::VertexStepMode::Vertex);
    }

    #[test]
    fn glyph_vertex_desc_attribute_count() {
        let desc = GlyphVertex::desc();
        assert_eq!(desc.attributes.len(), 3);
    }

    #[test]
    fn glyph_vertex_desc_position_attribute() {
        let desc = GlyphVertex::desc();
        let attr = &desc.attributes[0];
        assert_eq!(attr.offset, 0);
        assert_eq!(attr.shader_location, 0);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn glyph_vertex_desc_tex_coords_attribute() {
        let desc = GlyphVertex::desc();
        let attr = &desc.attributes[1];
        assert_eq!(attr.offset, 8); // after position [f32; 2]
        assert_eq!(attr.shader_location, 1);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn glyph_vertex_desc_color_attribute() {
        let desc = GlyphVertex::desc();
        let attr = &desc.attributes[2];
        assert_eq!(attr.offset, 16); // after position [f32; 2] + tex_coords [f32; 2]
        assert_eq!(attr.shader_location, 2);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x4);
    }

    // ---- RoundedRectVertex descriptor tests ----

    #[test]
    fn rounded_rect_vertex_desc_array_stride() {
        let desc = RoundedRectVertex::desc();
        assert_eq!(desc.array_stride, size_of::<RoundedRectVertex>() as u64);
    }

    #[test]
    fn rounded_rect_vertex_desc_step_mode() {
        let desc = RoundedRectVertex::desc();
        assert_eq!(desc.step_mode, wgpu::VertexStepMode::Vertex);
    }

    #[test]
    fn rounded_rect_vertex_desc_attribute_count() {
        let desc = RoundedRectVertex::desc();
        assert_eq!(desc.attributes.len(), 5);
    }

    #[test]
    fn rounded_rect_vertex_desc_position_attribute() {
        let desc = RoundedRectVertex::desc();
        let attr = &desc.attributes[0];
        assert_eq!(attr.offset, 0);
        assert_eq!(attr.shader_location, 0);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn rounded_rect_vertex_desc_color_attribute() {
        let desc = RoundedRectVertex::desc();
        let attr = &desc.attributes[1];
        assert_eq!(attr.offset, 8); // after position [f32; 2]
        assert_eq!(attr.shader_location, 1);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x4);
    }

    #[test]
    fn rounded_rect_vertex_desc_rect_min_attribute() {
        let desc = RoundedRectVertex::desc();
        let attr = &desc.attributes[2];
        assert_eq!(attr.offset, 24); // 8 (position) + 16 (color)
        assert_eq!(attr.shader_location, 2);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn rounded_rect_vertex_desc_rect_max_attribute() {
        let desc = RoundedRectVertex::desc();
        let attr = &desc.attributes[3];
        assert_eq!(attr.offset, 32); // 8 + 16 + 8
        assert_eq!(attr.shader_location, 3);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn rounded_rect_vertex_desc_params_attribute() {
        let desc = RoundedRectVertex::desc();
        let attr = &desc.attributes[4];
        assert_eq!(attr.offset, 40); // 8 + 16 + 8 + 8
        assert_eq!(attr.shader_location, 4);
        assert_eq!(attr.format, wgpu::VertexFormat::Float32x2);
    }

    // ---- Offset consistency tests ----
    // Verify that each attribute's offset + size equals the next attribute's offset,
    // and the last attribute's offset + size equals the array stride.

    fn format_size(format: wgpu::VertexFormat) -> u64 {
        match format {
            wgpu::VertexFormat::Float32x2 => 8,
            wgpu::VertexFormat::Float32x4 => 16,
            _ => panic!("Unexpected vertex format in test: {:?}", format),
        }
    }

    #[test]
    fn rect_vertex_offsets_are_contiguous() {
        let desc = RectVertex::desc();
        for i in 1..desc.attributes.len() {
            let prev = &desc.attributes[i - 1];
            let curr = &desc.attributes[i];
            assert_eq!(
                prev.offset + format_size(prev.format),
                curr.offset,
                "Gap between attributes {} and {} in RectVertex",
                i - 1,
                i
            );
        }
        // Last attribute end should equal stride
        let last = desc.attributes.last().unwrap();
        assert_eq!(last.offset + format_size(last.format), desc.array_stride);
    }

    #[test]
    fn texture_vertex_offsets_are_contiguous() {
        let desc = TextureVertex::desc();
        for i in 1..desc.attributes.len() {
            let prev = &desc.attributes[i - 1];
            let curr = &desc.attributes[i];
            assert_eq!(
                prev.offset + format_size(prev.format),
                curr.offset,
                "Gap between attributes {} and {} in TextureVertex",
                i - 1,
                i
            );
        }
        let last = desc.attributes.last().unwrap();
        assert_eq!(last.offset + format_size(last.format), desc.array_stride);
    }

    #[test]
    fn glyph_vertex_offsets_are_contiguous() {
        let desc = GlyphVertex::desc();
        for i in 1..desc.attributes.len() {
            let prev = &desc.attributes[i - 1];
            let curr = &desc.attributes[i];
            assert_eq!(
                prev.offset + format_size(prev.format),
                curr.offset,
                "Gap between attributes {} and {} in GlyphVertex",
                i - 1,
                i
            );
        }
        let last = desc.attributes.last().unwrap();
        assert_eq!(last.offset + format_size(last.format), desc.array_stride);
    }

    #[test]
    fn rounded_rect_vertex_offsets_are_contiguous() {
        let desc = RoundedRectVertex::desc();
        for i in 1..desc.attributes.len() {
            let prev = &desc.attributes[i - 1];
            let curr = &desc.attributes[i];
            assert_eq!(
                prev.offset + format_size(prev.format),
                curr.offset,
                "Gap between attributes {} and {} in RoundedRectVertex",
                i - 1,
                i
            );
        }
        let last = desc.attributes.last().unwrap();
        assert_eq!(last.offset + format_size(last.format), desc.array_stride);
    }

    // ---- Shader location uniqueness tests ----

    #[test]
    fn rect_vertex_shader_locations_unique() {
        let desc = RectVertex::desc();
        let locs: Vec<u32> = desc.attributes.iter().map(|a| a.shader_location).collect();
        for i in 0..locs.len() {
            for j in (i + 1)..locs.len() {
                assert_ne!(locs[i], locs[j], "Duplicate shader location in RectVertex");
            }
        }
    }

    #[test]
    fn texture_vertex_shader_locations_unique() {
        let desc = TextureVertex::desc();
        let locs: Vec<u32> = desc.attributes.iter().map(|a| a.shader_location).collect();
        for i in 0..locs.len() {
            for j in (i + 1)..locs.len() {
                assert_ne!(locs[i], locs[j], "Duplicate shader location in TextureVertex");
            }
        }
    }

    #[test]
    fn glyph_vertex_shader_locations_unique() {
        let desc = GlyphVertex::desc();
        let locs: Vec<u32> = desc.attributes.iter().map(|a| a.shader_location).collect();
        for i in 0..locs.len() {
            for j in (i + 1)..locs.len() {
                assert_ne!(locs[i], locs[j], "Duplicate shader location in GlyphVertex");
            }
        }
    }

    #[test]
    fn rounded_rect_vertex_shader_locations_unique() {
        let desc = RoundedRectVertex::desc();
        let locs: Vec<u32> = desc.attributes.iter().map(|a| a.shader_location).collect();
        for i in 0..locs.len() {
            for j in (i + 1)..locs.len() {
                assert_ne!(locs[i], locs[j], "Duplicate shader location in RoundedRectVertex");
            }
        }
    }

    // ---- Shader locations start at 0 and are sequential ----

    #[test]
    fn rect_vertex_shader_locations_sequential() {
        let desc = RectVertex::desc();
        for (i, attr) in desc.attributes.iter().enumerate() {
            assert_eq!(attr.shader_location, i as u32);
        }
    }

    #[test]
    fn texture_vertex_shader_locations_sequential() {
        let desc = TextureVertex::desc();
        for (i, attr) in desc.attributes.iter().enumerate() {
            assert_eq!(attr.shader_location, i as u32);
        }
    }

    #[test]
    fn glyph_vertex_shader_locations_sequential() {
        let desc = GlyphVertex::desc();
        for (i, attr) in desc.attributes.iter().enumerate() {
            assert_eq!(attr.shader_location, i as u32);
        }
    }

    #[test]
    fn rounded_rect_vertex_shader_locations_sequential() {
        let desc = RoundedRectVertex::desc();
        for (i, attr) in desc.attributes.iter().enumerate() {
            assert_eq!(attr.shader_location, i as u32);
        }
    }

    // ---- First attribute always starts at offset 0 ----

    #[test]
    fn all_descriptors_start_at_offset_zero() {
        assert_eq!(RectVertex::desc().attributes[0].offset, 0);
        assert_eq!(TextureVertex::desc().attributes[0].offset, 0);
        assert_eq!(GlyphVertex::desc().attributes[0].offset, 0);
        assert_eq!(RoundedRectVertex::desc().attributes[0].offset, 0);
    }

    // ---- Pod/Zeroable safety: verify bytemuck traits are sound ----

    #[test]
    fn rect_vertex_zeroed_is_valid() {
        let v: RectVertex = bytemuck::Zeroable::zeroed();
        assert_eq!(v.position, [0.0, 0.0]);
        assert_eq!(v.color, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn texture_vertex_zeroed_is_valid() {
        let v: TextureVertex = bytemuck::Zeroable::zeroed();
        assert_eq!(v.position, [0.0, 0.0]);
        assert_eq!(v.tex_coords, [0.0, 0.0]);
    }

    #[test]
    fn glyph_vertex_zeroed_is_valid() {
        let v: GlyphVertex = bytemuck::Zeroable::zeroed();
        assert_eq!(v.position, [0.0, 0.0]);
        assert_eq!(v.tex_coords, [0.0, 0.0]);
        assert_eq!(v.color, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn rounded_rect_vertex_zeroed_is_valid() {
        let v: RoundedRectVertex = bytemuck::Zeroable::zeroed();
        assert_eq!(v.position, [0.0, 0.0]);
        assert_eq!(v.color, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(v.rect_min, [0.0, 0.0]);
        assert_eq!(v.rect_max, [0.0, 0.0]);
        assert_eq!(v.params, [0.0, 0.0]);
    }

    #[test]
    fn uniforms_zeroed_is_valid() {
        let u: Uniforms = bytemuck::Zeroable::zeroed();
        assert_eq!(u.screen_size, [0.0, 0.0]);
        assert_eq!(u._padding, [0.0, 0.0]);
    }

    // ---- Bytemuck cast round-trip: struct <-> byte slice ----

    #[test]
    fn rect_vertex_bytemuck_cast_roundtrip() {
        let v = RectVertex {
            position: [1.0, 2.0],
            color: [0.1, 0.2, 0.3, 0.4],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), size_of::<RectVertex>());
        let v2: &RectVertex = bytemuck::from_bytes(bytes);
        assert_eq!(v2.position, v.position);
        assert_eq!(v2.color, v.color);
    }

    #[test]
    fn glyph_vertex_bytemuck_cast_roundtrip() {
        let v = GlyphVertex {
            position: [10.0, 20.0],
            tex_coords: [0.5, 0.75],
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), size_of::<GlyphVertex>());
        let v2: &GlyphVertex = bytemuck::from_bytes(bytes);
        assert_eq!(v2.position, v.position);
        assert_eq!(v2.tex_coords, v.tex_coords);
        assert_eq!(v2.color, v.color);
    }

    #[test]
    fn rounded_rect_vertex_bytemuck_cast_roundtrip() {
        let v = RoundedRectVertex {
            position: [5.0, 10.0],
            color: [1.0, 1.0, 1.0, 1.0],
            rect_min: [0.0, 0.0],
            rect_max: [100.0, 50.0],
            params: [2.0, 8.0],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), size_of::<RoundedRectVertex>());
        let v2: &RoundedRectVertex = bytemuck::from_bytes(bytes);
        assert_eq!(v2.position, v.position);
        assert_eq!(v2.color, v.color);
        assert_eq!(v2.rect_min, v.rect_min);
        assert_eq!(v2.rect_max, v.rect_max);
        assert_eq!(v2.params, v.params);
    }
}
