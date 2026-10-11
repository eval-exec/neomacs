use super::*;
#[test]
fn pending_heading_cancels_with_control_g_and_escape() {
    for key in [
        winit::keyboard::Key::Character("g".into()),
        winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape),
    ] {
        let mut menus = MenuPresentation::default();
        menus.select_heading(
            crate::menus::MenuHeading {
                frame: 1,
                parent: WindowId::from_raw(1),
                key: "help-menu".into(),
                index: 5,
                compact: false,
            },
            false,
        );
        menus.modifiers = winit::keyboard::ModifiersState::CONTROL;
        menus.navigate(&key);
        assert!(menus.heading().is_none());
    }
}

#[test]
fn heading_dismissal_retains_release_ownership() {
    let mut menus = MenuPresentation::default();
    let heading = crate::menus::MenuHeading {
        frame: 1,
        parent: WindowId::from_raw(1),
        key: "help-menu".into(),
        index: 5,
        compact: false,
    };
    menus.select_heading(heading.clone(), true);
    assert_eq!(
        menus.select_heading(heading.clone(), true),
        crate::menus::HeadingAction::Close
    );
    assert!(menus.heading().is_none());
    assert_eq!(
        menus.release_owner,
        Some((heading.parent, winit::event::MouseButton::Left))
    );
}
