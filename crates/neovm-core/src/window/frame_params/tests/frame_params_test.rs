use super::*;

#[test]
fn gnu_frame_params_preserve_gnu_order() {
    let names: Vec<&str> = GNU_FRAME_PARAMS
        .iter()
        .copied()
        .map(FrameParam::name)
        .collect();
    assert_eq!(
        names,
        vec![
            "auto-raise",
            "auto-lower",
            "background-color",
            "border-color",
            "border-width",
            "cursor-color",
            "cursor-type",
            "font",
            "foreground-color",
            "icon-name",
            "icon-type",
            "child-frame-border-width",
            "internal-border-width",
            "right-divider-width",
            "bottom-divider-width",
            "menu-bar-lines",
            "mouse-color",
            "name",
            "scroll-bar-width",
            "scroll-bar-height",
            "title",
            "unsplittable",
            "vertical-scroll-bars",
            "horizontal-scroll-bars",
            "visibility",
            "tab-bar-lines",
            "tool-bar-lines",
            "scroll-bar-foreground",
            "scroll-bar-background",
            "screen-gamma",
            "line-spacing",
            "left-fringe",
            "right-fringe",
            "wait-for-wm",
            "fullscreen",
            "font-backend",
            "alpha",
            "sticky",
            "tool-bar-position",
            "inhibit-double-buffering",
            "undecorated",
            "parent-frame",
            "skip-taskbar",
            "no-focus-on-map",
            "no-accept-focus",
            "z-group",
            "override-redirect",
            "no-special-glyphs",
            "alpha-background",
            "borders-respect-alpha-background",
            "use-frame-synchronization",
            "shaded",
            "ns-appearance",
            "ns-transparent-titlebar",
        ]
    );
    for (index, param) in GNU_FRAME_PARAMS.iter().copied().enumerate() {
        assert_eq!(param.gnu_index(), index);
        assert_eq!(FrameParam::from_gnu_index(index), Some(param));
        assert_eq!(FrameParam::from_name(param.name()), Some(param));
    }
    assert_eq!(FrameParam::from_gnu_index(GNU_FRAME_PARAM_COUNT), None);
}

#[test]
fn frame_value_domains_match_gnu_symbols() {
    assert_eq!(
        FrameFullscreen::from_symbol_value(&Value::symbol("fullboth")),
        Some(FrameFullscreen::Fullboth)
    );
    assert_eq!(
        FrameFullscreen::from_symbol_value(&Value::symbol("fullscreen")),
        Some(FrameFullscreen::Fullscreen)
    );
    assert_eq!(
        FrameFullscreen::from_symbol_value(&Value::symbol("maximized")),
        Some(FrameFullscreen::Maximized)
    );
    assert_eq!(
        FrameFullscreen::from_symbol_value(&Value::symbol("full")),
        None
    );

    assert_eq!(
        FrameToolBarPosition::from_symbol_value(&Value::symbol("left")),
        Some(FrameToolBarPosition::Left)
    );
    assert_eq!(
        FrameToolBarPosition::from_symbol_value(&Value::symbol("bottom")),
        Some(FrameToolBarPosition::Bottom)
    );
    assert_eq!(
        FrameZGroup::from_symbol_value(&Value::symbol("above-suspended")),
        Some(FrameZGroup::AboveSuspended)
    );
    assert_eq!(
        CursorTypeSymbol::from_symbol_value(&Value::symbol("hbar")),
        Some(CursorTypeSymbol::Hbar)
    );
    assert!(CursorTypeSymbol::Box.accepts_width_tail());
    assert!(!CursorTypeSymbol::Hollow.accepts_width_tail());
}
