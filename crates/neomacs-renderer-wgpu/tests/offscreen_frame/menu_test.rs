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
