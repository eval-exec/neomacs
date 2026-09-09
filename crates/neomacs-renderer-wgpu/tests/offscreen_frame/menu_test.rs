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
                    all_items: std::slice::from_ref(&item),
                    title: None,
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
                    all_items: std::slice::from_ref(&item),
                    title: None,
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
