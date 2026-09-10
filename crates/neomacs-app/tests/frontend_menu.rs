//! Shared menu interaction must not require a native window or GPU surface.

use neomacs_app::frontend::menu::MenuSession;
use neomacs_display_protocol::{
    PopupMenuItem,
    menu::{MenuAvailability, MenuIndicator, MenuItemKind},
};

#[test]
fn pointer_can_open_a_submenu_and_select_its_enabled_command_without_a_window() {
    let item = |label: &str, depth, kind| PopupMenuItem {
        label: label.into(),
        depth,
        kind,
        shortcut: String::new(),
        help: None,
    };
    let command = |availability| MenuItemKind::Command {
        availability,
        indicator: MenuIndicator::None,
    };
    let mut menu = MenuSession::new(
        0.0,
        0.0,
        vec![
            item(
                "File",
                0,
                MenuItemKind::Submenu {
                    availability: MenuAvailability::Enabled,
                },
            ),
            item("Unavailable", 1, command(MenuAvailability::Disabled)),
            item("Open", 1, command(MenuAvailability::Enabled)),
        ],
        None,
        14.0,
        20.0,
        8.0,
    );

    // Panel-local logical pixels; independent of native-window placement.
    menu.hover_panel(0, 10.0, 6.0);
    assert_eq!(menu.panels().len(), 2);
    menu.hover_panel(1, 10.0, 6.0);
    assert_eq!(menu.activate_panel(1), None);
    menu.hover_panel(1, 10.0, 29.0);
    assert_eq!(menu.activate_panel(1), Some(2));
}
