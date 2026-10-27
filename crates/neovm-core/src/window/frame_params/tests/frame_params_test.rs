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

#[test]
fn fixed_frame_parameter_symbols_match_canonical_names_without_warm_intern() {
    use crate::emacs_core::intern::{intern, intern_calls, reset_intern_calls};
    for key in GNU_FRAME_PARAMS {
        let expected = Value::from_sym_id(intern(key.name()));
        assert_eq!(key.symbol(), expected);
        reset_intern_calls();
        assert_eq!(key.symbol(), expected);
        assert_eq!(intern_calls(), 0);
    }
    for &query in FrameQueryParam::VARIANTS {
        let name: &'static str = query.into();
        let expected = intern(name);
        assert_eq!(FrameParamKey::from(query), FrameParamKey::Unknown(expected));
        reset_intern_calls();
        assert_eq!(FrameParamKey::from(query), FrameParamKey::Unknown(expected));
        assert_eq!(intern_calls(), 0);
        // Native queries must not become GNU handler variants.
        assert_eq!(FrameParam::from_name(name), None);
    }
}

#[test]
fn fixed_frame_queries_preserve_dynamic_parameter_results() {
    use crate::buffer::BufferId;
    use crate::window::FrameManager;
    let mut frames = FrameManager::new();
    let id = frames.create_frame("fixed-query-test", 800, 600, BufferId(1));
    let frame = frames.get_mut(id).expect("created frame");
    for (index, &query) in FrameQueryParam::VARIANTS.iter().enumerate() {
        let name: &'static str = query.into();
        let expected = Value::fixnum(index as i64);
        frame.set_parameter(Value::symbol(name), expected);
        assert_eq!(frame.fixed_parameter(query), frame.parameter(name));
        assert_eq!(frame.fixed_parameter(query), Some(expected));
    }
    frame.set_known_parameter(FrameParam::CursorColor, Value::fixnum(74));
    assert_eq!(
        frame.known_parameter(FrameParam::CursorColor),
        frame.parameter("cursor-color")
    );
}

#[test]
fn fixed_parameter_handles_agree_with_their_printed_spellings() {
    use crate::emacs_core::intern::{intern, is_canonical_id, resolve_sym};
    for &param in &GNU_FRAME_PARAMS {
        let handle = param.well_known();
        let name = param.name();
        assert_eq!(handle.name(), name, "{param:?}");
        assert_eq!(resolve_sym(handle.id()), name, "{param:?}");
        assert!(is_canonical_id(handle.id()), "{param:?}");
        assert_eq!(intern(name), handle.id(), "{param:?}");
        assert_eq!(
            param.symbol().bits(),
            Value::symbol(name).bits(),
            "{param:?}"
        );
        assert_eq!(
            FrameParamKey::from(param).symbol().bits(),
            Value::symbol(name).bits(),
            "{param:?}"
        );
    }
    for &query in FrameQueryParam::VARIANTS {
        let handle = query.well_known();
        let name: &'static str = query.into();
        assert_eq!(handle.name(), name, "{query:?}");
        assert_eq!(resolve_sym(handle.id()), name, "{query:?}");
        assert!(is_canonical_id(handle.id()), "{query:?}");
        assert_eq!(intern(name), handle.id(), "{query:?}");
        assert_eq!(
            FrameParamKey::from(query).symbol().bits(),
            Value::symbol(name).bits(),
            "{query:?}"
        );
    }
}
