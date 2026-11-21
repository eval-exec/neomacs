use super::super::ui_overlays::{placed_chrome_item_bounds, toolbar_icon_quad, toolbar_texture_id};
use neomacs_display_protocol::ImageRasterExtent;
use neomacs_display_protocol::frame_chrome::{BandRect, FrameRect};
use neomacs_display_protocol::types::{ImageId, Rect};
use neomacs_display_protocol::{Color, DeviceScale, ToolBarIconStyle, ToolBarImageSource};
use std::collections::HashMap;

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
