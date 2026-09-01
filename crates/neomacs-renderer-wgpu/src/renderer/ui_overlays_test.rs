use super::ui_overlays::{placed_chrome_item_bounds, toolbar_texture_id};
use neomacs_display_protocol::ToolBarImageSource;
use neomacs_display_protocol::frame_chrome::{BandRect, FrameRect};
use neomacs_display_protocol::types::{ImageId, Rect};
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
        ((image.clone(), 24), ImageId::new(7)),
        ((image.clone(), 48), ImageId::new(9)),
    ]);

    assert_eq!(
        toolbar_texture_id(&textures, &image, 24),
        Some(ImageId::new(7))
    );
    assert_eq!(
        toolbar_texture_id(&textures, &image, 48),
        Some(ImageId::new(9))
    );
    assert_eq!(toolbar_texture_id(&textures, &image, 32), None);
}
