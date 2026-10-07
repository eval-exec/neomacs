use super::*;

fn dark_pixel_count(
    icon: &RasterizedWindowIcon,
    x_range: std::ops::Range<u32>,
    y_range: std::ops::Range<u32>,
) -> usize {
    y_range
        .flat_map(|y| x_range.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let offset = ((y * icon.width + x) * 4) as usize;
            let [red, green, blue, alpha] = icon.premultiplied_rgba[offset..offset + 4] else {
                unreachable!("each icon pixel has four channels")
            };
            alpha > 200 && red < 50 && green < 50 && blue < 50
        })
        .count()
}

#[test]
fn canonical_icon_raster_keeps_both_lambda_marks_without_fonts() {
    let icon = load_window_icon().expect("decode canonical SVG without a font database");

    assert!(
        dark_pixel_count(&icon, 125..200, 65..140) > 350,
        "the purple lobe must retain its vector lambda mark"
    );
    assert!(
        dark_pixel_count(&icon, 45..130, 130..200) > 350,
        "the orange lobe must retain its vector lambda mark"
    );
}

#[test]
fn wayland_argb8888_preserves_premultiplied_color_in_native_byte_order() {
    let icon = RasterizedWindowIcon {
        width: 1,
        height: 1,
        premultiplied_rgba: vec![0x11, 0x22, 0x33, 0x44],
    };

    assert_eq!(icon.to_wayland_argb8888(), 0x4411_2233_u32.to_ne_bytes());
}
