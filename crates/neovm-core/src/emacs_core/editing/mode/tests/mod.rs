use super::*;
use crate::emacs_core::intern::{intern, resolve_sym};

fn mode_symbol(name: &str) -> Value {
    Value::symbol(name)
}

fn mode_symbol_opt(name: Option<&str>) -> Option<Value> {
    name.map(mode_symbol)
}

fn mode_display(text: &str) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_utf8(text)
}

fn mode_display_opt(text: Option<&str>) -> Option<crate::heap_types::LispString> {
    text.map(mode_display)
}

fn mode_pattern(text: &str) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_utf8(text)
}

// -------------------------------------------------------------------
// ModeRegistry basics
// -------------------------------------------------------------------

#[test]
fn new_registry_has_fundamental_mode() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    assert!(
        reg.major_modes
            .contains_key(&crate::emacs_core::intern::intern("fundamental-mode",))
    );
}

#[test]
fn default_major_mode_is_fundamental() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    assert_eq!(reg.get_major_mode(1), "fundamental-mode");
}

// -------------------------------------------------------------------
// Major mode registration and switching
// -------------------------------------------------------------------

#[test]
fn register_and_set_major_mode() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "rust-mode",
        MajorMode {
            pretty_name: mode_display("Rust"),
            parent: mode_symbol_opt(Some("prog-mode")),
            mode_hook: mode_symbol("rust-mode-hook"),
            keymap_name: mode_symbol_opt(Some("rust-mode-map")),
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );

    assert!(reg.set_major_mode(1, "rust-mode").is_ok());
    assert_eq!(reg.get_major_mode(1), "rust-mode");
}

#[test]
fn set_unknown_major_mode_fails() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    let result = reg.set_major_mode(1, "nonexistent-mode");
    assert!(result.is_err());
}

#[test]
fn set_major_mode_replaces_previous() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "text-mode",
        MajorMode {
            pretty_name: mode_display("Text"),
            parent: None,
            mode_hook: mode_symbol("text-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.register_major_mode(
        "org-mode",
        MajorMode {
            pretty_name: mode_display("Org"),
            parent: mode_symbol_opt(Some("text-mode")),
            mode_hook: mode_symbol("org-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );

    reg.set_major_mode(1, "text-mode").unwrap();
    assert_eq!(reg.get_major_mode(1), "text-mode");

    reg.set_major_mode(1, "org-mode").unwrap();
    assert_eq!(reg.get_major_mode(1), "org-mode");
}

// -------------------------------------------------------------------
// Minor mode operations
// -------------------------------------------------------------------

#[test]
fn register_and_enable_minor_mode() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "auto-fill-mode",
        MinorMode {
            lighter: mode_display_opt(Some(" Fill")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    assert!(reg.enable_minor_mode(1, "auto-fill-mode").is_ok());
    assert!(reg.is_minor_mode_active(1, "auto-fill-mode"));
}

#[test]
fn enable_unknown_minor_mode_fails() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    let result = reg.enable_minor_mode(1, "nonexistent-mode");
    assert!(result.is_err());
}

#[test]
fn disable_minor_mode() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "flycheck-mode",
        MinorMode {
            lighter: mode_display_opt(Some(" FlyC")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    reg.enable_minor_mode(1, "flycheck-mode").unwrap();
    assert!(reg.is_minor_mode_active(1, "flycheck-mode"));

    reg.disable_minor_mode(1, "flycheck-mode");
    assert!(!reg.is_minor_mode_active(1, "flycheck-mode"));
}

#[test]
fn toggle_minor_mode() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "linum-mode",
        MinorMode {
            lighter: mode_display_opt(Some(" Ln")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    // Toggle on.
    let active = reg.toggle_minor_mode(1, "linum-mode").unwrap();
    assert!(active);
    assert!(reg.is_minor_mode_active(1, "linum-mode"));

    // Toggle off.
    let active = reg.toggle_minor_mode(1, "linum-mode").unwrap();
    assert!(!active);
    assert!(!reg.is_minor_mode_active(1, "linum-mode"));
}

#[test]
fn toggle_unknown_minor_mode_fails() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    let result = reg.toggle_minor_mode(1, "nonexistent");
    assert!(result.is_err());
}

#[test]
fn active_minor_modes_lists_all() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "mode-a",
        MinorMode {
            lighter: mode_display_opt(Some(" A")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );
    reg.register_minor_mode(
        "mode-b",
        MinorMode {
            lighter: mode_display_opt(Some(" B")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    reg.enable_minor_mode(1, "mode-a").unwrap();
    reg.enable_minor_mode(1, "mode-b").unwrap();

    let active = reg.active_minor_modes(1);
    assert_eq!(active.len(), 2);
    assert!(active.contains(&"mode-a"));
    assert!(active.contains(&"mode-b"));
}

#[test]
fn enable_minor_mode_idempotent() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "hl-line-mode",
        MinorMode {
            lighter: None,
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    reg.enable_minor_mode(1, "hl-line-mode").unwrap();
    reg.enable_minor_mode(1, "hl-line-mode").unwrap();

    let active = reg.active_minor_modes(1);
    assert_eq!(active.len(), 1);
}

// -------------------------------------------------------------------
// Global minor modes
// -------------------------------------------------------------------

#[test]
fn global_minor_mode_active_in_all_buffers() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "global-hl-line-mode",
        MinorMode {
            lighter: mode_display_opt(Some(" HL")),
            keymap_name: None,
            global: true,
            body: None,
        },
    );

    reg.enable_global_minor_mode("global-hl-line-mode").unwrap();

    // Active in any buffer, even ones we never explicitly set.
    assert!(reg.is_minor_mode_active(1, "global-hl-line-mode"));
    assert!(reg.is_minor_mode_active(99, "global-hl-line-mode"));
}

#[test]
fn disable_global_minor_mode() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "global-mode",
        MinorMode {
            lighter: None,
            keymap_name: None,
            global: true,
            body: None,
        },
    );

    reg.enable_global_minor_mode("global-mode").unwrap();
    assert!(reg.is_minor_mode_active(1, "global-mode"));

    reg.disable_global_minor_mode("global-mode");
    assert!(!reg.is_minor_mode_active(1, "global-mode"));
}

#[test]
fn global_and_buffer_local_no_duplicates() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "shared-mode",
        MinorMode {
            lighter: mode_display_opt(Some(" S")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    reg.enable_global_minor_mode("shared-mode").unwrap();
    reg.enable_minor_mode(1, "shared-mode").unwrap();

    // Should only appear once.
    let active = reg.active_minor_modes(1);
    assert_eq!(active.iter().filter(|&&m| m == "shared-mode").count(), 1);
}

// -------------------------------------------------------------------
// Auto-mode-alist
// -------------------------------------------------------------------

#[test]
fn auto_mode_alist_suffix_match() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "rust-mode",
        MajorMode {
            pretty_name: mode_display("Rust"),
            parent: None,
            mode_hook: mode_symbol("rust-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.add_auto_mode(mode_pattern(".rs"), mode_symbol("rust-mode"));

    assert_eq!(reg.mode_for_file("main.rs"), Some("rust-mode"));
    assert_eq!(reg.mode_for_file("lib.rs"), Some("rust-mode"));
    assert_eq!(reg.mode_for_file("main.py"), None);
}

#[test]
fn auto_mode_alist_first_match_wins() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "mode-a",
        MajorMode {
            pretty_name: mode_display("A"),
            parent: None,
            mode_hook: mode_symbol("mode-a-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.register_major_mode(
        "mode-b",
        MajorMode {
            pretty_name: mode_display("B"),
            parent: None,
            mode_hook: mode_symbol("mode-b-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.add_auto_mode(mode_pattern(".txt"), mode_symbol("mode-a"));
    reg.add_auto_mode(mode_pattern(".txt"), mode_symbol("mode-b"));

    assert_eq!(reg.mode_for_file("file.txt"), Some("mode-a"));
}

// -------------------------------------------------------------------
// Mode-line rendering
// -------------------------------------------------------------------

#[test]
fn mode_line_string_fundamental() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    let s = reg.mode_line_string(1);
    assert_eq!(s, "(Fundamental)");
}

#[test]
fn mode_line_string_with_minor_modes() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "auto-fill-mode",
        MinorMode {
            lighter: mode_display_opt(Some(" Fill")),
            keymap_name: None,
            global: false,
            body: None,
        },
    );
    reg.enable_minor_mode(1, "auto-fill-mode").unwrap();

    let s = reg.mode_line_string(1);
    assert_eq!(s, "(Fundamental Fill)");
}

#[test]
fn mode_line_format_render() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    let fmt = ModeLineFormat::default_format();
    let rendered = fmt.render(1, &reg, "*scratch*", false, false, 1, 0, 0);
    assert!(rendered.contains("*scratch*"));
    assert!(rendered.contains("Fundamental"));
    assert!(rendered.contains("Top"));
    assert!(rendered.contains("--"));
}

#[test]
fn mode_line_format_modified_and_readonly() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    let fmt = ModeLineFormat::default_format();

    let rendered_mod = fmt.render(1, &reg, "buf", true, false, 10, 5, 50);
    assert!(rendered_mod.contains("**"));
    assert!(rendered_mod.contains("50%"));
    assert!(rendered_mod.contains("10:5"));

    let rendered_ro = fmt.render(1, &reg, "buf", false, true, 1, 0, 100);
    assert!(rendered_ro.contains("%%"));
    assert!(rendered_ro.contains("Bot"));
}

// -------------------------------------------------------------------
// Font-lock keywords
// -------------------------------------------------------------------

#[test]
fn font_lock_keywords_basic() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "lisp-mode",
        MajorMode {
            pretty_name: mode_display("Lisp"),
            parent: None,
            mode_hook: mode_symbol("lisp-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: Some(FontLockDefaults {
                keywords: vec![FontLockKeyword {
                    pattern: mode_pattern(r"\b(defun|defvar)\b"),
                    face: intern("font-lock-keyword-face"),
                    group: 1,
                    override_: false,
                    laxmatch: false,
                }],
                case_fold: false,
                syntax_table: None,
            }),
            body: None,
        },
    );

    let kws = reg.font_lock_keywords("lisp-mode").unwrap();
    assert_eq!(kws.len(), 1);
    assert_eq!(resolve_sym(kws[0].face), "font-lock-keyword-face");
}

#[test]
fn font_lock_keywords_inherit_from_parent() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();

    // Parent with font-lock.
    reg.register_major_mode(
        "prog-mode",
        MajorMode {
            pretty_name: mode_display("Prog"),
            parent: None,
            mode_hook: mode_symbol("prog-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: Some(FontLockDefaults {
                keywords: vec![FontLockKeyword {
                    pattern: mode_pattern(r"TODO"),
                    face: intern("font-lock-warning-face"),
                    group: 0,
                    override_: true,
                    laxmatch: false,
                }],
                case_fold: false,
                syntax_table: None,
            }),
            body: None,
        },
    );

    // Child without font-lock — should inherit.
    reg.register_major_mode(
        "rust-mode",
        MajorMode {
            pretty_name: mode_display("Rust"),
            parent: mode_symbol_opt(Some("prog-mode")),
            mode_hook: mode_symbol("rust-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );

    let kws = reg.font_lock_keywords("rust-mode").unwrap();
    assert_eq!(kws.len(), 1);
    assert_eq!(kws[0].pattern, mode_pattern("TODO"));
}

#[test]
fn font_lock_keywords_none() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    assert!(reg.font_lock_keywords("fundamental-mode").is_none());
}

// -------------------------------------------------------------------
// Custom variables and groups
// -------------------------------------------------------------------

#[test]
fn register_custom_variable() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_custom_variable(
        "indent-tabs-mode",
        CustomVariable {
            default_value: Value::T,
            doc: Some(mode_display("Use tabs for indentation.")),
            type_: CustomType::Boolean,
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        },
    );

    let var = reg.get_custom_variable("indent-tabs-mode").unwrap();
    assert!(var.default_value.is_truthy());
}

#[test]
fn custom_variable_in_group() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_custom_group(
        "editing",
        CustomGroup {
            doc: Some(mode_display("Editing options.")),
            parent: None,
            members: vec![],
        },
    );

    reg.register_custom_variable(
        "fill-column",
        CustomVariable {
            default_value: Value::fixnum(70),
            doc: None,
            type_: CustomType::Integer,
            group: mode_symbol_opt(Some("editing")),
            set_function: None,
            get_function: None,
            tag: None,
        },
    );

    let group = reg.get_custom_group("editing").unwrap();
    assert!(group.members.contains(&mode_symbol("fill-column")));
}

// -------------------------------------------------------------------
// Mode inheritance (derived-mode-p)
// -------------------------------------------------------------------

#[test]
fn derived_mode_p_self() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "text-mode",
        MajorMode {
            pretty_name: mode_display("Text"),
            parent: None,
            mode_hook: mode_symbol("text-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );

    assert!(reg.derived_mode_p("text-mode", "text-mode"));
}

#[test]
fn derived_mode_p_parent_chain() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "text-mode",
        MajorMode {
            pretty_name: mode_display("Text"),
            parent: None,
            mode_hook: mode_symbol("text-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.register_major_mode(
        "org-mode",
        MajorMode {
            pretty_name: mode_display("Org"),
            parent: mode_symbol_opt(Some("text-mode")),
            mode_hook: mode_symbol("org-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.register_major_mode(
        "org-journal-mode",
        MajorMode {
            pretty_name: mode_display("Org-Journal"),
            parent: mode_symbol_opt(Some("org-mode")),
            mode_hook: mode_symbol("org-journal-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );

    assert!(reg.derived_mode_p("org-journal-mode", "text-mode"));
    assert!(reg.derived_mode_p("org-journal-mode", "org-mode"));
    assert!(reg.derived_mode_p("org-mode", "text-mode"));
    assert!(!reg.derived_mode_p("text-mode", "org-mode"));
}

#[test]
fn derived_mode_p_unrelated() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_major_mode(
        "text-mode",
        MajorMode {
            pretty_name: mode_display("Text"),
            parent: None,
            mode_hook: mode_symbol("text-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );
    reg.register_major_mode(
        "prog-mode",
        MajorMode {
            pretty_name: mode_display("Prog"),
            parent: None,
            mode_hook: mode_symbol("prog-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        },
    );

    assert!(!reg.derived_mode_p("text-mode", "prog-mode"));
    assert!(!reg.derived_mode_p("prog-mode", "text-mode"));
}

// -------------------------------------------------------------------
// Buffer removal
// -------------------------------------------------------------------

#[test]
fn remove_buffer_cleans_up() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_minor_mode(
        "test-mode",
        MinorMode {
            lighter: None,
            keymap_name: None,
            global: false,
            body: None,
        },
    );

    reg.set_major_mode(1, "fundamental-mode").unwrap();
    reg.enable_minor_mode(1, "test-mode").unwrap();

    reg.remove_buffer(1);

    // Falls back to fundamental-mode (no entry).
    assert_eq!(reg.get_major_mode(1), "fundamental-mode");
    assert!(
        reg.active_minor_modes(1).is_empty()
            || reg
                .active_minor_modes(1)
                .iter()
                .all(|m| reg.global_minor_modes.contains(&mode_symbol(m)))
    );
}

// -------------------------------------------------------------------
// FontLockLevel default
// -------------------------------------------------------------------

#[test]
fn font_lock_level_default_is_level3() {
    crate::test_utils::init_test_tracing();
    let level = FontLockLevel::default();
    assert_eq!(level, FontLockLevel::Level3);
}

// -------------------------------------------------------------------
// ModeLineFormat default
// -------------------------------------------------------------------

#[test]
fn mode_line_format_default_has_elements() {
    crate::test_utils::init_test_tracing();
    let fmt = ModeLineFormat::default_format();
    assert!(!fmt.elements.is_empty());
}

// -------------------------------------------------------------------
// Custom types
// -------------------------------------------------------------------

#[test]
fn custom_type_choice() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_custom_variable(
        "my-choice",
        CustomVariable {
            default_value: Value::symbol("fast"),
            doc: None,
            type_: CustomType::Choice(vec![
                ("fast".to_string(), Value::symbol("fast")),
                ("slow".to_string(), Value::symbol("slow")),
            ]),
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        },
    );

    let var = reg.get_custom_variable("my-choice").unwrap();
    assert!(matches!(var.type_, CustomType::Choice(_)));
}

#[test]
fn custom_type_nested_list() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    reg.register_custom_variable(
        "my-list",
        CustomVariable {
            default_value: Value::NIL,
            doc: None,
            type_: CustomType::List(Box::new(CustomType::String)),
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        },
    );

    let var = reg.get_custom_variable("my-list").unwrap();
    assert!(matches!(var.type_, CustomType::List(_)));
}
