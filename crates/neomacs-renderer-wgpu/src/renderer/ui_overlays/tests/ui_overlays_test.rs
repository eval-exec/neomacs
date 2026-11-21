use super::super::ui_overlays::{placed_chrome_item_bounds, toolbar_icon_quad, toolbar_texture_id};
use neomacs_display_protocol::ImageRasterExtent;
use neomacs_display_protocol::frame_chrome::{BandRect, FrameRect};
use neomacs_display_protocol::types::{ImageId, Rect};
use neomacs_display_protocol::{Color, DeviceScale, ToolBarIconStyle, ToolBarImageSource};
use std::collections::HashMap;

/// A remembered raster failure is still returned to both chrome painters.
/// Repainting either bar must retain the atlas's first diagnostic without
/// adding a caller warning on every frame.
#[test]
#[tracing_test::traced_test]
fn repeated_menu_and_compact_heading_draws_warn_once_per_failed_label() {
    use crate::glyph_atlas::WgpuGlyphAtlas;
    use crate::renderer::WgpuRenderer;
    use neomacs_display_protocol::font::{ResolvedFontId, ResolvedGlyph, ResolvedGlyphId};
    use neomacs_display_protocol::frame_chrome::{
        MenuHeadingText, PositionedMenuHeading, ResolvedMenuLabel,
    };
    use neomacs_display_protocol::{CompactBarContent, MenuBarItem};
    use std::sync::Arc;

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("menu draw regression requires a real wgpu adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("menu-heading-failure-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .expect("menu draw regression device");
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer =
        WgpuRenderer::with_device(device.clone(), queue.clone(), 128, 32, format, 1.0);
    let mut atlas = WgpuGlyphAtlas::new(&device);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("menu-heading-failure-target"),
        size: wgpu::Extent3d {
            width: 128,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let band = FrameRect::new(0.0, 0.0, 128.0, 32.0).unwrap();
    for compact in [false, true] {
        // Deliberately omit the font publication. This forces the actual
        // resolved-menu raster path to fail without depending on host fonts.
        let label = ResolvedMenuLabel::new(
            vec![ResolvedGlyph {
                resolved_font_id: ResolvedFontId(817),
                glyph_id: ResolvedGlyphId::new(if compact { 2 } else { 1 }),
                x: 0.0,
                y: 0.0,
                x_advance: 12.0,
                cluster_start: 0,
                cluster_end: 1,
            }],
            Vec::new(),
            16.0,
        );
        let heading = PositionedMenuHeading::measure(
            MenuBarItem {
                index: 0,
                label: "X".into(),
                key: "test".into(),
            },
            0.0,
            32.0,
            4.0,
            |_| MenuHeadingText::Pixels(label),
        );
        let content = CompactBarContent::new(
            vec![heading.clone()],
            Vec::new(),
            Color::WHITE,
            Color::BLACK,
            Color::WHITE,
            Color::BLACK,
            16,
            4,
        );
        for _ in 0..8 {
            if compact {
                renderer.render_compact_bar(
                    &view,
                    &content,
                    band,
                    &HashMap::new(),
                    None,
                    None,
                    None,
                    None,
                    &mut atlas,
                    128,
                    32,
                );
            } else {
                renderer.render_menu_bar(
                    &view,
                    std::slice::from_ref(&heading),
                    band,
                    (1.0, 1.0, 1.0),
                    (0.0, 0.0, 0.0),
                    None,
                    None,
                    &mut atlas,
                    128,
                    32,
                );
            }
        }
    }
    logs_assert(|lines| {
        let initial = lines
            .iter()
            .filter(|line| {
                line.contains("menu label glyph atlas lookup failed")
                    && line.contains("RasterizeFailed")
            })
            .count();
        let repeated = lines
            .iter()
            .filter(|line| line.contains("menu heading rasterization failed"))
            .count();
        if initial == 2 && repeated == 0 {
            Ok(())
        } else {
            Err(format!(
                "expected one atlas warning per distinct failed heading and no caller duplicates; initial={initial}, repeated={repeated}: {lines:?}"
            ))
        }
    });
}

#[test]
fn frame_chrome_item_projection_uses_authoritative_band_origin_once() {
    let band = FrameRect::new(0.0, 33.0, 800.0, 34.0).expect("toolbar band");
    let item = BandRect::new(5.0, 0.0, 24.0, 34.0).expect("local toolbar item");

    assert_eq!(
        placed_chrome_item_bounds(band, item),
        Rect::new(5.0, 33.0, 24.0, 34.0)
    );
}

#[test]
fn toolbar_texture_lookup_is_scoped_by_icon_size() {
    let image = ToolBarImageSource::GnuRaster {
        path: "open.xpm".to_string(),
    };
    let textures = HashMap::from([
        (
            ToolBarIconStyle::from_chrome_face(24, Color::WHITE, Color::BLACK)
                .realize(image.clone(), DeviceScale::ONE),
            ImageId::new(7),
        ),
        (
            ToolBarIconStyle::from_chrome_face(48, Color::WHITE, Color::BLACK)
                .realize(image.clone(), DeviceScale::ONE),
            ImageId::new(9),
        ),
    ]);

    assert_eq!(
        toolbar_texture_id(
            &textures,
            &image,
            ToolBarIconStyle::from_chrome_face(24, Color::WHITE, Color::BLACK),
            DeviceScale::ONE
        ),
        Some(ImageId::new(7))
    );
    assert_eq!(
        toolbar_texture_id(
            &textures,
            &image,
            ToolBarIconStyle::from_chrome_face(48, Color::WHITE, Color::BLACK),
            DeviceScale::ONE
        ),
        Some(ImageId::new(9))
    );
    assert_eq!(
        toolbar_texture_id(
            &textures,
            &image,
            ToolBarIconStyle::from_chrome_face(32, Color::WHITE, Color::BLACK),
            DeviceScale::ONE
        ),
        None
    );
    assert_eq!(
        toolbar_texture_id(
            &textures,
            &image,
            ToolBarIconStyle::from_chrome_face(24, Color::BLACK, Color::BLACK),
            DeviceScale::ONE
        ),
        None
    );
    assert_eq!(
        toolbar_texture_id(
            &textures,
            &image,
            ToolBarIconStyle::from_chrome_face(24, Color::WHITE, Color::WHITE),
            DeviceScale::ONE
        ),
        None
    );
    assert_eq!(
        toolbar_texture_id(
            &textures,
            &image,
            ToolBarIconStyle::from_chrome_face(24, Color::WHITE, Color::BLACK),
            DeviceScale::new(1.5).unwrap()
        ),
        None
    );
}

/// The draw quad is the texture's own physical extent, centered in the item and
/// snapped to whole device pixels.  A themed vector has already been realized
/// at exactly that extent, so the sampled icon is 1:1 at fractional scales.
#[test]
fn toolbar_icon_quad_matches_the_texture_extent_at_device_pixel_edges() {
    for scale in [1.0_f32, 1.25, 1.5, 1.7, 2.0] {
        let bounds = Rect::new(10.0, 20.0, 34.0, 34.0);
        let physical = (24.0_f32 * scale).ceil();
        let raster = ImageRasterExtent::new(physical as u32, physical as u32);
        let (x, y, width, height) = toolbar_icon_quad(
            bounds,
            raster,
            DeviceScale::new(scale).expect("valid scale"),
        );

        let physical_width = width * scale;
        let physical_height = height * scale;
        assert!(
            (physical_width - physical).abs() < 1e-3,
            "quad width {physical_width} != texture width {physical} at scale {scale}"
        );
        assert!(
            (physical_height - physical).abs() < 1e-3,
            "quad height {physical_height} != texture height {physical} at scale {scale}"
        );

        // Both edges land on whole device pixels, not half pixels.
        for edge in [
            x * scale,
            (x + width) * scale,
            y * scale,
            (y + height) * scale,
        ] {
            assert!(
                (edge - edge.round()).abs() < 1e-3,
                "edge {edge} is not device-pixel aligned at scale {scale}"
            );
        }
    }
}

/// Overlay chrome text (menu-bar labels, popup items, tooltips) is placed on
/// the device-pixel grid, the same whole-pixel rule the row text pipeline uses
/// (`content::snap_glyph_origin`).  The placed quad origin must be integral in
/// device space and may move at most half a device pixel from the unsnapped
/// origin, so layout/hitboxes are unchanged.
#[test]
fn overlay_glyph_snapping_lands_on_the_device_grid_within_half_a_pixel() {
    use crate::renderer::snap_overlay_glyph_origin;

    for scale in [1.0_f32, 1.25, 1.5, 1.7, 2.0] {
        // Fractional inputs like a menu label centered in a chrome band
        // (`(18 - 13) / 2 = 2.5`) plus a raster bearing.
        let (x, y, bearing_x, bearing_y, ascent) = (3.13_f32, 2.5_f32, 1.0_f32, 3.0_f32, 13.0_f32);
        let (snapped_x, snapped_y) =
            snap_overlay_glyph_origin(x, y, bearing_x, bearing_y, ascent, scale);
        let unsnapped_x = x * scale + bearing_x;
        let unsnapped_y = y * scale + ascent * scale - bearing_y;
        let physical_x = snapped_x * scale;
        let physical_y = snapped_y * scale;

        assert!(
            (physical_x - physical_x.round()).abs() < 1e-3,
            "x origin {physical_x} is not on the device grid at scale {scale}"
        );
        assert!(
            (physical_y - physical_y.round()).abs() < 1e-3,
            "y origin {physical_y} is not on the device grid at scale {scale}"
        );
        assert!(
            (physical_x - unsnapped_x).abs() <= 0.5 + 1e-3,
            "x origin moved more than half a device pixel at scale {scale}: \
             {physical_x} vs {unsnapped_x}"
        );
        assert!(
            (physical_y - unsnapped_y).abs() <= 0.5 + 1e-3,
            "y origin moved more than half a device pixel at scale {scale}: \
             {physical_y} vs {unsnapped_y}"
        );
    }
}
