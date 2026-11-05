use super::*;

use neovm_value::intern::{intern, resolve_sym};

#[test]
fn new_registry_has_fundamental_mode() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    assert!(
        reg.major_modes
            .contains_key(&neovm_value::intern::intern("fundamental-mode",))
    );
}

#[test]
fn default_major_mode_is_fundamental() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    assert_eq!(reg.get_major_mode(1), "fundamental-mode");
}

#[test]
fn set_unknown_major_mode_fails() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    let result = reg.set_major_mode(1, "nonexistent-mode");
    assert!(result.is_err());
}

#[test]
fn enable_unknown_minor_mode_fails() {
    crate::test_utils::init_test_tracing();
    let mut reg = ModeRegistry::new();
    let result = reg.enable_minor_mode(1, "nonexistent-mode");
    assert!(result.is_err());
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
fn mode_line_string_fundamental() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    let s = reg.mode_line_string(1);
    assert_eq!(s, "(Fundamental)");
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

#[test]
fn font_lock_keywords_none() {
    crate::test_utils::init_test_tracing();
    let reg = ModeRegistry::new();
    assert!(reg.font_lock_keywords("fundamental-mode").is_none());
}

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

#[test]
fn font_lock_level_default_is_level3() {
    crate::test_utils::init_test_tracing();
    let level = FontLockLevel::default();
    assert_eq!(level, FontLockLevel::Level3);
}

#[test]
fn mode_line_format_default_has_elements() {
    crate::test_utils::init_test_tracing();
    let fmt = ModeLineFormat::default_format();
    assert!(!fmt.elements.is_empty());
}

fn mode_symbol(name: &str) -> Value {
    Value::symbol(name)
}
