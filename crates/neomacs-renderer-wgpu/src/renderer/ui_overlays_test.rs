use super::ui_overlays::{placed_chrome_item_bounds, toolbar_texture_id};
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
    let image = ToolBarImageSource::File {
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
