//! Menu pixel regressions using the actual popup painter, not a mock widget.
use super::*;
use neomacs_display_protocol::menu::{
    MenuCheckState, MenuIndicator, MenuPanel, MenuPanelPaint, PopupMenuItem,
};

#[test]
fn popup_menu_disabled_submenu_dims_its_arrow() {
    use neomacs_display_protocol::menu::{MenuAvailability, MenuItemKind};
    let mut h = try_harness().expect("menu pixel test requires an offscreen GPU adapter");
    let panel = MenuPanel {
        x: 0.0,
        y: 0.0,
        item_indices: vec![0],
        hover_index: -1,
        bounds: (0.0, 0.0, W as f32, H as f32),
        item_offsets: vec![4.0],
        item_height: 24.0,
    };
    let mut frames = Vec::new();
    for availability in [MenuAvailability::Enabled, MenuAvailability::Disabled] {
        let item = PopupMenuItem {
            kind: MenuItemKind::Submenu { availability },
            help: None,
            label: String::new(),
            shortcut: String::new(),
            depth: 0,
        };
        let SurfaceState::Drawable(surface) =
            SurfaceState::from_device_size(W, H, DeviceScale::new(1.0).unwrap()).unwrap()
        else {
            unreachable!()
        };
        h.renderer
            .begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
                &h.view, surface,
            ))
            .expect("live device draw parameters")
            .paint_menu(
                &MenuPanelPaint {
                    panel: &panel,
                    menu: &neomacs_display_protocol::menu::MeasuredMenu::measure(
                        vec![item.clone()],
                        None,
                        8.0,
                        |_| 8.0,
                    ),
                    role: neomacs_display_protocol::menu::MenuPanelRole::Root,
                    face_fg: None,
                    face_bg: None,
                    font_face: None,
                },
                &mut h.atlas,
            );
        frames.push(read_back(&h));
    }
    assert_ne!(
        frames[0], frames[1],
        "disabled submenu arrow must use its disabled appearance"
    );
}

#[test]
fn popup_menu_toggle_and_radio_states_have_distinct_indicator_pixels() {
    let mut h = try_harness().expect("menu pixel test requires an offscreen GPU adapter");
    let panel = MenuPanel {
        x: 0.0,
        y: 0.0,
        item_indices: vec![0],
        hover_index: -1,
        bounds: (0.0, 0.0, W as f32, H as f32),
        item_offsets: vec![4.0],
        item_height: 24.0,
    };
    let mut item = PopupMenuItem {
        kind: neomacs_display_protocol::menu::MenuItemKind::Command {
            availability: neomacs_display_protocol::menu::MenuAvailability::Enabled,
            indicator: MenuIndicator::Toggle(MenuCheckState::Off),
        },
        help: None,
        label: String::new(),
        shortcut: String::new(),
        depth: 0,
    };
    let mut frames = Vec::new();
    for indicator in [
        MenuIndicator::Toggle(MenuCheckState::Off),
        MenuIndicator::Toggle(MenuCheckState::On),
        MenuIndicator::Radio(MenuCheckState::Off),
        MenuIndicator::Radio(MenuCheckState::On),
    ] {
        item.kind = neomacs_display_protocol::menu::MenuItemKind::Command {
            availability: neomacs_display_protocol::menu::MenuAvailability::Enabled,
            indicator,
        };
        let SurfaceState::Drawable(surface) =
            SurfaceState::from_device_size(W, H, DeviceScale::new(1.0).unwrap()).unwrap()
        else {
            unreachable!()
        };
        h.renderer
            .begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
                &h.view, surface,
            ))
            .expect("live device draw parameters")
            .paint_menu(
                &MenuPanelPaint {
                    panel: &panel,
                    menu: &neomacs_display_protocol::menu::MeasuredMenu::measure(
                        vec![item.clone()],
                        None,
                        8.0,
                        |_| 8.0,
                    ),
                    role: neomacs_display_protocol::menu::MenuPanelRole::Root,
                    face_fg: None,
                    face_bg: None,
                    font_face: None,
                },
                &mut h.atlas,
            );
        frames.push(read_back(&h));
    }
    for (index, frame) in frames.iter().enumerate() {
        for other in &frames[index + 1..] {
            assert_ne!(
                frame, other,
                "each toggle/radio state must be visually distinct"
            );
        }
    }
}

#[test]
fn popup_initial_cjk_spacing_matches_real_font_and_leaves_shortcut_gap() {
    let mut h = try_harness().expect("menu pixel test requires an offscreen GPU adapter");
    let label = "中中";
    // Independent reference: shape the whole CJK run, rather than counting
    // scalars or reusing the menu's measurement result as its own oracle.
    let reference = h
        .atlas
        .get_or_create_composed_atlas(
            h.renderer.device(),
            h.renderer.queue(),
            label,
            FaceId::new(0),
            0.0_f32.to_bits(),
            None,
            cosmic_text::SubpixelBin::Zero,
            cosmic_text::SubpixelBin::Zero,
            SubpixelRequest::Disabled,
        )
        .expect("GUI tests require an installed CJK fallback font");
    let expected_width = reference.first().expect("shaped CJK glyphs").advance_width;
    let space = 8.0;
    assert!(
        expected_width > 2.0 * space + 2.0,
        "fixture needs genuinely wide CJK glyphs, got {expected_width}"
    );
    let items = vec![PopupMenuItem {
        kind: neomacs_display_protocol::menu::MenuItemKind::Command {
            availability: neomacs_display_protocol::menu::MenuAvailability::Enabled,
            indicator: MenuIndicator::None,
        },
        help: None,
        label: label.into(),
        shortcut: "C-y".into(),
        depth: 0,
    }];
    let text = h.atlas.measure_menu(
        items,
        None,
        space,
        None,
        h.renderer.device(),
        h.renderer.queue(),
    );
    assert!(
        (text.text().item(0).label.width() - expected_width).abs() < 1.0,
        "initial menu must use real CJK advances: menu={} shaped={expected_width}",
        text.text().item(0).label.width()
    );
    let panel = MenuPanel {
        x: 0.0,
        y: 0.0,
        item_indices: vec![0],
        hover_index: -1,
        bounds: (0.0, 0.0, W as f32, H as f32),
        item_offsets: vec![4.0],
        item_height: 24.0,
    };
    let SurfaceState::Drawable(surface) =
        SurfaceState::from_device_size(W, H, DeviceScale::new(1.0).unwrap()).unwrap()
    else {
        unreachable!()
    };
    h.renderer
        .begin_draw(neomacs_renderer_wgpu::renderer::RenderTarget::new(
            &h.view, surface,
        ))
        .expect("live device draw parameters")
        .paint_menu(
            &MenuPanelPaint {
                panel: &panel,
                menu: &text,
                role: neomacs_display_protocol::menu::MenuPanelRole::Root,
                face_fg: Some((1.0, 1.0, 1.0)),
                face_bg: Some((0.0, 0.0, 0.0)),
                font_face: None,
            },
            &mut h.atlas,
        );
    let pixels = read_back(&h);
    let bright_in = |left: u32, right: u32| {
        (left..right)
            .flat_map(|x| (6..28).map(move |y| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b, _] = px(&pixels, x, y);
                r > 120 && g > 120 && b > 120
            })
            .count()
    };
    let label_end = (8.0 + expected_width).ceil() as u32;
    assert!(
        bright_in(8, (8.0 + expected_width / 2.0) as u32) > 0,
        "first CJK glyph must be visible"
    );
    assert!(
        bright_in((8.0 + expected_width / 2.0) as u32, label_end) > 0,
        "second CJK glyph must occupy its own advance"
    );
    // The repeated glyph must have the same ink mask one independently shaped
    // advance later. Merely finding some ink in each half misses overlapping
    // glyphs whose tails extend into the second half.
    let advance = (expected_width / 2.0).round() as u32;
    assert!((expected_width / 2.0 - advance as f32).abs() < 0.01);
    let mask = |origin: u32| {
        (0..advance)
            .flat_map(|x| (6..28).map(move |y| (origin + x, y)))
            .map(|(x, y)| {
                let [r, g, b, _] = px(&pixels, x, y);
                r > 120 && g > 120 && b > 120
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        mask(8),
        mask(8 + advance),
        "CJK glyphs must be spaced by the real font advance"
    );
    // A fixed right-side shortcut column is separate from the two CJK glyphs.
    assert!(bright_in(64, 88) > 0, "shortcut must be visible");
    assert_eq!(
        bright_in(label_end + 1, 62),
        0,
        "label ink must not collide with shortcut column"
    );
}

#[test]
fn menu_bar_initial_cjk_glyphs_do_not_overlap() {
    assert_menu_heading_pixels(false);
}

#[test]
fn compact_bar_initial_cjk_glyphs_do_not_overlap() {
    assert_menu_heading_pixels(true);
}

fn assert_menu_heading_pixels(compact: bool) {
    use neomacs_display_protocol::MenuBarItem;
    let mut h = try_harness().expect("menu bar pixel test requires an offscreen GPU adapter");
    let reference = h
        .atlas
        .get_or_create_composed_atlas(
            h.renderer.device(),
            h.renderer.queue(),
            "中中",
            FaceId::new(0),
            0.0_f32.to_bits(),
            None,
            cosmic_text::SubpixelBin::Zero,
            cosmic_text::SubpixelBin::Zero,
            SubpixelRequest::Disabled,
        )
        .expect("requires a CJK fallback font");
    let advance = reference[0].advance_width / 2.0;
    assert!(advance > h.atlas.default_char_width() + 2.0);
    let mut fonts = neomacs_layout_engine::font::metrics::FontMetricsService::new();
    let items = vec![
        neomacs_display_protocol::frame_chrome::PositionedMenuHeading::measure(
            MenuBarItem {
                index: 0,
                key: "file".into(),
                label: "中中".into(),
            },
            0.0,
            32.0,
            8.0,
            |text| {
                let size = h.atlas.default_font_size();
                let (glyphs, fonts) = fonts
                    .resolved_glyphs_for_cluster(text, "monospace", 400, false, size)
                    .unwrap();
                neomacs_display_protocol::frame_chrome::MenuHeadingText::Pixels(
                    neomacs_display_protocol::frame_chrome::ResolvedMenuLabel::new(
                        glyphs, fonts, size,
                    ),
                )
            },
        ),
    ];
    if compact {
        let content = neomacs_display_protocol::CompactBarContent::new(
            items,
            vec![],
            Color::WHITE,
            Color::BLACK,
            Color::WHITE,
            Color::BLACK,
            16,
            4,
        );
        h.renderer.render_compact_bar(
            &h.view,
            &content,
            FrameRect::new(0.0, 0.0, W as f32, 32.0).unwrap(),
            &std::collections::HashMap::new(),
            None,
            None,
            None,
            None,
            &mut h.atlas,
            W,
            H,
        );
    } else {
        h.renderer.render_menu_bar(
            &h.view,
            &items,
            FrameRect::new(0.0, 0.0, W as f32, 32.0).unwrap(),
            (1.0, 1.0, 1.0),
            (0.0, 0.0, 0.0),
            None,
            None,
            &mut h.atlas,
            W,
            H,
        );
    }
    let pixels = read_back(&h);
    let step = advance.round() as u32;
    assert!((advance - step as f32).abs() < 0.01);
    let ink = |origin| {
        (0..step)
            .flat_map(|x| (0..30).map(move |y| (origin + x, y)))
            .map(|(x, y)| {
                let [r, g, b, _] = px(&pixels, x, y);
                r > 120 && g > 120 && b > 120
            })
            .collect::<Vec<_>>()
    };
    let first = ink(8);
    let second = ink(8 + step);
    assert!(first.iter().any(|v| *v), "heading must be visible");
    assert!(
        first == second,
        "repeated CJK heading glyphs must be one real-font advance apart"
    );
}

fn menu_heading_at(
    h: &mut Harness,
    fonts: &mut neomacs_layout_engine::font::metrics::FontMetricsService,
    x: f32,
    height: f32,
    padding: f32,
) -> neomacs_display_protocol::frame_chrome::PositionedMenuHeading {
    use neomacs_display_protocol::frame_chrome::{
        MenuHeadingText, PositionedMenuHeading, ResolvedMenuLabel,
    };
    PositionedMenuHeading::measure(
        neomacs_display_protocol::MenuBarItem {
            index: 0,
            key: "file".into(),
            label: "HHH".into(),
        },
        x,
        height,
        padding,
        |text| {
            let size = h.atlas.default_font_size();
            let (glyphs, resolved) = fonts
                .resolved_glyphs_for_cluster(text, "monospace", 400, false, size)
                .expect("monospace font for chrome menu text");
            MenuHeadingText::Pixels(ResolvedMenuLabel::new(glyphs, resolved, size))
        },
    )
}

fn paint_heading_band(
    h: &mut Harness,
    items: &[neomacs_display_protocol::frame_chrome::PositionedMenuHeading],
    band_y: f32,
    band_height: f32,
) {
    h.renderer.render_menu_bar(
        &h.view,
        items,
        FrameRect::new(0.0, band_y, 40.0, band_height).expect("menu band"),
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        None,
        None,
        &mut h.atlas,
        W,
        H,
    );
}

/// Menu-bar labels are pre-rasterized textures placed at a fractional logical
/// origin (centered band + accumulated advances).  They must be placed on the
/// device-pixel grid like row text, so the label render is pixel-identical to
/// the same label whose origin was already integral (the reference render,
/// one whole device pixel down and right); pre-fix the linear sampler
/// resampled the texture and the two differed.
#[test]
fn menu_bar_text_origin_is_device_pixel_aligned_at_fractional_scales() {
    use neomacs_display_protocol::frame_chrome::MenuHeadingText;
    use neomacs_renderer_wgpu::renderer::snap_overlay_glyph_origin;

    for scale in [1.0_f32, 1.25, 1.5, 1.7, 2.0] {
        let mut fixed =
            try_harness().expect("menu text pixel test requires an offscreen GPU adapter");
        fixed.renderer.set_scale_factor(scale);
        fixed.atlas.set_scale_factor(scale);
        let font_size = fixed.atlas.default_font_size();
        let band_height = font_size + 5.0;
        let padding = 4.0;
        let mut fonts = neomacs_layout_engine::font::metrics::FontMetricsService::new();

        let heading = menu_heading_at(&mut fixed, &mut fonts, 0.0, band_height, padding);
        let text_x = padding;
        let text_y = (band_height - font_size) / 2.0;
        let label = match heading.text() {
            MenuHeadingText::Pixels(label) => label,
            _ => unreachable!("pixel menu heading"),
        };
        let handles = fixed
            .atlas
            .menu_label_atlas(label, fixed.renderer.device(), fixed.renderer.queue())
            .expect("composed menu label atlas");
        assert!(!handles.is_empty(), "label must rasterize at scale {scale}");
        let metrics = handles[0].entry.metrics();
        let ascent = fixed.atlas.default_font_ascent();

        // Integral device-pixel origin the fix must land on.
        let (snapped_x, snapped_y) = snap_overlay_glyph_origin(
            text_x,
            text_y,
            metrics.bearing_x,
            metrics.bearing_y,
            ascent,
            scale,
        );
        let physical_x = snapped_x * scale;
        let physical_y = snapped_y * scale;
        assert!(
            (physical_x - physical_x.round()).abs() < 1e-3
                && (physical_y - physical_y.round()).abs() < 1e-3,
            "menu text origin ({physical_x},{physical_y}) is off the device grid at scale {scale}"
        );

        // Reference: the same label shifted so its unsnapped origin is an
        // integral device position.  The correction is sub-pixel and can be
        // negative, but chrome rects require non-negative origins, so add one
        // whole device pixel; the reference is then the fixed render shifted
        // by exactly one physical pixel.
        let unsnapped_x = text_x + metrics.bearing_x / scale;
        let unsnapped_y = text_y + ascent - metrics.bearing_y / scale;
        let device_pixel = 1.0 / scale;
        let reference_shift_x = snapped_x - unsnapped_x + device_pixel;
        let reference_shift_y = snapped_y - unsnapped_y + device_pixel;
        assert!(
            reference_shift_x >= 0.0 && reference_shift_y >= 0.0,
            "reference shift must keep chrome rect origins non-negative at scale {scale}: \
             ({reference_shift_x},{reference_shift_y})"
        );

        paint_heading_band(&mut fixed, std::slice::from_ref(&heading), 0.0, band_height);
        let rendered = read_back(&fixed);

        let mut reference = try_harness().expect("reference menu text harness");
        reference.renderer.set_scale_factor(scale);
        reference.atlas.set_scale_factor(scale);
        let mut reference_fonts = neomacs_layout_engine::font::metrics::FontMetricsService::new();
        let reference_heading = menu_heading_at(
            &mut reference,
            &mut reference_fonts,
            reference_shift_x,
            band_height,
            padding,
        );
        paint_heading_band(
            &mut reference,
            std::slice::from_ref(&reference_heading),
            reference_shift_y,
            band_height,
        );
        let reference_pixels = read_back(&reference);

        // Compare the painted band in physical pixels; the sub-pixel band
        // shift can leave the outermost rows unpainted in one of the renders.
        let width = (heading.local_bounds().raw().width * scale).ceil() as u32;
        let rows = (band_height * scale).floor() as u32;
        let mut compared = 0;
        let mut ink = false;
        for y in 1..rows.saturating_sub(1).min(H - 2) {
            for x in 0..width.min(W - 2) {
                let got = px(&rendered, x, y);
                // The reference is one whole device pixel down and right.
                let want = px(&reference_pixels, x + 1, y + 1);
                assert_eq!(
                    got, want,
                    "menu text is resampled at ({x},{y}) scale {scale}: got {got:?}, reference {want:?}"
                );
                ink |= got[0] > 32;
                compared += 1;
            }
        }
        assert!(compared > 0, "the label band must be inspected");
        assert!(ink, "menu label ink must render at scale {scale}");
    }
}
