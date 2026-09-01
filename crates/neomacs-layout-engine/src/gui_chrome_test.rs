use super::*;
use neomacs_display_protocol::frame_chrome::ChromeAction;
use neomacs_display_protocol::types::Color;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::load::{
    apply_runtime_startup_state, create_bootstrap_evaluator_cached_with_features,
};
use neovm_core::heap_types::LispString;

#[test]
fn parse_tool_bar_item_preserves_raw_unibyte_label_and_help() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let expected = raw
        .as_runtime_string_owned()
        .expect("runtime string for raw label");
    let def = Value::list(vec![
        Value::symbol("menu-item"),
        raw,
        Value::symbol("ignore"),
        Value::symbol(":help"),
        raw,
    ]);

    let item = parse_tool_bar_item(&mut eval, "raw-item", &def, 0).expect("tool-bar item");
    assert_eq!(item.label, expected);
    assert_eq!(item.help, expected);
}

#[test]
fn parse_tool_bar_item_keeps_wrap_separate_from_button_type() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let def = Value::list(vec![
        Value::symbol("menu-item"),
        Value::string("Wrapped"),
        Value::symbol("ignore"),
        Value::symbol(":button"),
        Value::cons(Value::symbol(":toggle"), Value::T),
        Value::symbol(":wrap"),
        Value::T,
        Value::symbol(":enable"),
        Value::T,
    ]);

    let item = parse_tool_bar_item(&mut eval, "wrapped-toggle", &def, 0).expect("tool-bar item");
    assert_eq!(item.item_type, ToolBarItemType::Toggle);
    assert_eq!(item.item_type.gnu_type_name(), ":toggle");
    assert!(item.selected);
    assert!(item.wrap);
    assert!(!item.enabled);
}

#[test]
fn toolbar_image_extensions_use_typed_gnu_image_domain() {
    assert!(is_supported_toolbar_image_file("open.xpm"));
    assert!(is_supported_toolbar_image_file("photo.JPG"));
    assert!(is_supported_toolbar_image_file("diagram.svgz"));
    assert!(!is_supported_toolbar_image_file("unknown.bmp"));
    assert!(toolbar_image_score("open.xpm") < toolbar_image_score("photo.jpg"));
    assert!(toolbar_image_score("open.pbm") < toolbar_image_score("open.svg"));
}

#[test]
fn toolbar_icon_name_keeps_gnu_image_base_name() {
    assert_eq!(
        tool_bar_icon_name_from_path("search.xpm").as_deref(),
        Some("search")
    );
    assert_eq!(
        tool_bar_icon_name_from_path("low-color/search.xpm").as_deref(),
        Some("search")
    );
    assert_eq!(
        tool_bar_icon_name_from_path("/tmp/neomacs/etc/images/mail/compose.xpm").as_deref(),
        Some("mail/compose")
    );
}

#[test]
fn toolbar_theme_resolves_themed_svg_and_preserves_gnu_fallback() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::symbol(":type"),
        Value::symbol("xpm"),
        Value::symbol(":file"),
        Value::string("search.xpm"),
    ]);

    eval.eval_str("(setq neomacs-toolbar-icon-theme 'material)")
        .expect("set toolbar icon theme");
    let themed = tool_bar_image_source(&eval, &spec).expect("themed image source");
    assert!(
        themed
            .file_path()
            .is_some_and(|path| path.ends_with("etc/toolbar-icons/material/search.svg")),
        "material image path: {themed:#?}"
    );

    eval.eval_str("(setq neomacs-toolbar-icon-theme 'gnu)")
        .expect("set GNU toolbar icon theme");
    let gnu = tool_bar_image_source(&eval, &spec).expect("GNU image source");
    assert!(
        gnu.file_path()
            .is_some_and(|path| path.ends_with("search.xpm") && !path.contains("toolbar-icons")),
        "GNU image path: {gnu:#?}"
    );
}

#[test]
fn toolbar_theme_defaults_to_vscode_like() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::symbol(":type"),
        Value::symbol("xpm"),
        Value::symbol(":file"),
        Value::string("search.xpm"),
    ]);

    let themed = tool_bar_image_source(&eval, &spec).expect("default themed image source");
    assert!(
        themed
            .file_path()
            .is_some_and(|path| path.ends_with("etc/toolbar-icons/vscode-like/search.svg")),
        "default image path: {themed:#?}"
    );
}

#[test]
fn toolbar_theme_resolves_gnu_find_image_expression_to_default_theme() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["neomacs"]).expect("bootstrap evaluator");
    let expression = eval
        .eval_str(
            r#"
            '(find-image
              '((:type xpm :file "search.xpm")
                (:type pbm :file "search.pbm")
                (:type xbm :file "search.xbm")))
            "#,
        )
        .expect("GNU find-image expression");

    let themed = tool_bar_image_source(&eval, &expression).expect("default themed image source");
    assert!(
        themed
            .file_path()
            .is_some_and(|path| path.ends_with("etc/toolbar-icons/vscode-like/search.svg")),
        "default image path from GNU find-image expression: {themed:#?}"
    );
}

#[test]
fn collect_gui_menu_bar_items_runtime_frame_has_help_menu() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["neomacs"]).expect("bootstrap evaluator");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let items = collect_gui_menu_bar_items(&eval);
    assert!(!items.is_empty());
    assert!(items.iter().any(|item| item.key == "help-menu"));
}

#[test]
fn collect_gui_tool_bar_items_after_setup_has_search_item_and_separator() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["neomacs"]).expect("bootstrap evaluator");
    eval.eval_str("(tool-bar-setup)")
        .expect("run GNU tool-bar setup");
    eval.eval_str("(setq neomacs-toolbar-icon-theme 'gnu)")
        .expect("set GNU toolbar icon theme");
    let items = collect_gui_tool_bar_items(&mut eval);
    assert!(
        items.iter().any(|item| item
            .image
            .as_ref()
            .and_then(|image| image.file_path())
            .is_some_and(|path| path.ends_with("/search.xpm") || path == "search.xpm")),
        "tool-bar items: {items:#?}"
    );
    assert!(items.iter().any(|item| item.is_separator()));
}

#[test]
fn collect_gui_tool_bar_items_after_setup_uses_default_theme() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["neomacs"]).expect("bootstrap evaluator");
    eval.eval_str("(tool-bar-setup)")
        .expect("run GNU tool-bar setup");

    let items = collect_gui_tool_bar_items(&mut eval);
    assert!(
        items.iter().any(|item| item
            .image
            .as_ref()
            .and_then(|image| image.file_path())
            .is_some_and(|path| path.ends_with("etc/toolbar-icons/vscode-like/search.svg"))),
        "tool-bar items: {items:#?}"
    );
}

#[test]
fn collect_gui_tool_bar_items_for_frame_uses_that_frames_selected_buffer() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let primary_buffer = eval.buffer_manager_mut().create_buffer("toolbar-primary");
    let secondary_buffer = eval.buffer_manager_mut().create_buffer("toolbar-secondary");
    assert!(
        eval.buffer_manager_mut()
            .switch_current_unrecorded(primary_buffer)
    );
    let primary_frame =
        eval.frame_manager_mut()
            .create_frame("toolbar-primary", 800, 600, primary_buffer);
    let secondary_frame =
        eval.frame_manager_mut()
            .create_frame("toolbar-secondary", 800, 600, secondary_buffer);
    assert!(eval.frame_manager_mut().select_frame(primary_frame));

    let secondary_map = neovm_core::emacs_core::keymap::make_sparse_list_keymap();
    neovm_core::emacs_core::keymap::list_keymap_define(
        secondary_map,
        Value::symbol("secondary-action"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Secondary"),
            Value::symbol("ignore"),
        ]),
    );
    eval.buffer_manager_mut()
        .get_mut(secondary_buffer)
        .expect("secondary buffer")
        .set_buffer_local("tool-bar-map", secondary_map);

    let items = collect_gui_tool_bar_items_for_frame(&mut eval, secondary_frame);
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Secondary"]
    );
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(primary_frame),
        "frame-specific collection must restore the globally selected frame"
    );
    assert_eq!(
        eval.buffer_manager().current_buffer_id(),
        Some(primary_buffer),
        "frame-specific collection must restore the current buffer"
    );
}

#[test]
fn layout_gui_menu_bar_content_assigns_local_bounds_and_actions() {
    let content = layout_gui_menu_bar_content(
        vec![
            MenuBarItem {
                index: 0,
                label: "File".to_string(),
                key: "file".to_string(),
            },
            MenuBarItem {
                index: 1,
                label: "Edit".to_string(),
                key: "edit".to_string(),
            },
        ],
        200.0,
        18.0,
        8.0,
        8.0,
        Color::WHITE,
        Color::BLACK,
    );

    assert_eq!(
        content.items()[0].local_bounds().raw(),
        neomacs_display_protocol::types::Rect::new(8.0, 0.0, 48.0, 18.0)
    );
    assert_eq!(
        content.items()[0].action(),
        Some(&ChromeAction::OpenMenu {
            index: 0,
            key: "file".to_string(),
        })
    );
    assert_eq!(content.items()[1].local_bounds().raw().x, 56.0);
}

/// Regression: on a terminal frame (char_width == 1 cell) the menu bar must use
/// GNU's `SCHARS + 1` (one-cell) item separation, not the window-system pixel
/// gutter. With the pixel gutter the full `dired` menu overflowed a 160-column
/// frame and the trailing `Immediate`/`Subdir`/`Help` items were dropped; with
/// GNU's one-cell gutter all items fit (total ~74 cells).
#[test]
fn tty_menu_bar_keeps_all_items_with_gnu_one_cell_gutter() {
    // Global + dired-mode menu bar: File Edit Options Buffers Tools Operate Mark
    // Regexp Immediate Subdir Help.
    let labels = [
        "File",
        "Edit",
        "Options",
        "Buffers",
        "Tools",
        "Operate",
        "Mark",
        "Regexp",
        "Immediate",
        "Subdir",
        "Help",
    ];
    let items: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| MenuBarItem {
            index: index as u32,
            label: (*label).to_string(),
            key: label.to_lowercase(),
        })
        .collect();

    // Terminal metrics: 1x1 cells, 160-column frame, half-cell padding per side
    // (== one cell of separation, GNU's SCHARS + 1).
    let content =
        layout_gui_menu_bar_content(items, 160.0, 1.0, 1.0, 0.5, Color::WHITE, Color::BLACK);

    let kept: Vec<&str> = content
        .items()
        .iter()
        .map(|it| it.item().label.as_str())
        .collect();
    assert_eq!(kept, labels, "all menu-bar items must survive TTY layout");
}

#[test]
fn layout_gui_tool_bar_content_uses_one_height_policy() {
    let items = vec![
        ToolBarItem {
            index: 0,
            key: "save".to_string(),
            image: None,
            label: "Save".to_string(),
            help: String::new(),
            enabled: true,
            selected: false,
            item_type: ToolBarItemType::Button,
            wrap: false,
        },
        ToolBarItem {
            index: 1,
            key: "separator".to_string(),
            image: None,
            label: String::new(),
            help: String::new(),
            enabled: false,
            selected: false,
            item_type: ToolBarItemType::Separator,
            wrap: false,
        },
        ToolBarItem {
            index: 2,
            key: "disabled".to_string(),
            image: None,
            label: "Disabled".to_string(),
            help: String::new(),
            enabled: false,
            selected: false,
            item_type: ToolBarItemType::Button,
            wrap: false,
        },
    ];
    let content = layout_gui_tool_bar_content(items, 200.0, 34.0, Color::WHITE, Color::BLACK);

    assert_eq!(content.icon_size(), 24);
    assert_eq!(content.padding(), 5);
    assert_eq!(
        content.items()[0].local_bounds().raw(),
        neomacs_display_protocol::types::Rect::new(5.0, 0.0, 34.0, 34.0)
    );
    assert_eq!(
        content.items()[0].action(),
        Some(&ChromeAction::InvokeToolBarItem { index: 0 })
    );
    assert_eq!(content.items()[1].action(), None);
    assert_eq!(content.items()[2].action(), None);
}

#[test]
fn layout_gui_compact_bar_content_places_tools_after_menu_items() {
    let menu_items = vec![MenuBarItem {
        index: 0,
        label: "File".to_string(),
        key: "file".to_string(),
    }];
    let tool_items = vec![ToolBarItem {
        index: 0,
        key: "save".to_string(),
        image: None,
        label: "Save".to_string(),
        help: String::new(),
        enabled: true,
        selected: false,
        item_type: ToolBarItemType::Button,
        wrap: false,
    }];
    let content = layout_gui_compact_bar_content(
        menu_items,
        tool_items,
        240.0,
        34.0,
        8.0,
        Color::WHITE,
        Color::BLACK,
        Color::WHITE,
        Color::BLACK,
    );

    let menu_right = {
        let bounds = content.menu_items()[0].local_bounds().raw();
        bounds.x + bounds.width
    };
    assert!(content.tool_items()[0].local_bounds().raw().x > menu_right);
}

/// **The stale-bytecode refusal covers this crate's in-process tests.**
///
/// It did not.  Ledger 202 gated the refusal on `cfg!(test)`, which Rust sets
/// only for the crate being compiled as a test -- so it was live for
/// `neovm-core`'s own 482 in-process tests and DARK for the 13 here and the 62
/// in `neomacs-bin`, which link `neovm-core` as an ordinary
/// dependency.  202 recorded that as residual 1; ledger 206 reproduced it.
///
/// The reproduction, on one deliberately staled tree carrying a single stale
/// `lisp/international/emoji-zwj.elc`:
///
/// ```text
/// neovm-core  the_gui_terminal_layer_adds_documentation_and_never_rewrites_it
///             REFUSED in 2.0s, naming the file and both mtimes
/// neomacs     startup::tests::bootstrap_gui_frame_uses_gnu_cursor_and_pointer_color_defaults
///             1 passed in 9.4s, silently
/// ```
///
/// RED before ledger 206: `for_this_process` did not exist, and the policy this
/// process got was `Warn`.  It is now `Refuse` by default in every process that
/// has not announced itself a shipped editor -- and the only one that does is
/// `neomacs`'s own `main`, which is a different program from this test
/// binary and does not link this crate's tests.
///
/// One honest caveat: with `NEOVM_ALLOW_STALE_BYTECODE` set, both arms are
/// `Warn` and this check cannot tell them apart -- which is what that variable
/// is FOR, and why the red above was produced with it unset.  A gate run that
/// exported it globally would make this guard vacuous.
#[test]
fn the_stale_bytecode_refusal_covers_this_crates_tests() {
    use neovm_core::emacs_core::load::{ALLOW_STALE_BYTECODE_ENV, StaleBytecodePolicy};

    let expected = match std::env::var_os(ALLOW_STALE_BYTECODE_ENV) {
        Some(value) if !value.is_empty() => StaleBytecodePolicy::Warn,
        _ => StaleBytecodePolicy::Refuse,
    };
    assert_eq!(
        StaleBytecodePolicy::for_this_process(),
        expected,
        "this crate's tests boot an image in-process, so they must not be \
         allowed to read bytecode that does not implement the checked-out \
         source; `main' announcing itself a shipped editor is a different \
         process from this one"
    );
}
