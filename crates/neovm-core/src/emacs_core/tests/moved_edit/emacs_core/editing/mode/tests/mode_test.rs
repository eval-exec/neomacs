use super::*;

use neovm_value::gc_trace::GcTrace;
use neovm_value::intern::SymId;
use std::collections::HashMap;

use neovm_edit::emacs_core::mode::*;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

use neovm_value::intern::{intern, resolve_sym};

#[test]
fn mode_copied_properties_have_only_manager_roots() {
    use crate::emacs_core::Context;
    use neovm_edit::buffer::{CharPos0, CharRange};

    struct Probe(usize);
    fn payload(probes: &mut Vec<Probe>) -> Value {
        let value = Value::vector(vec![Value::fixnum(probes.len() as i64)]);
        probes.push(Probe(value.bits()));
        value
    }
    fn text(probes: &mut Vec<Probe>, key: Value, mut text: LispString) -> LispString {
        text.intervals_mut().put_property_in_char_range(
            CharRange::from_usize(0, 1),
            key,
            payload(probes),
        );
        text
    }
    fn choices(probes: &mut Vec<Probe>) -> CustomType {
        CustomType::Choice(vec![
            ("first".to_owned(), payload(probes)),
            ("second".to_owned(), payload(probes)),
        ])
    }

    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
    eval.gc_collect_exact();
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    unsafe { eval.tagged_heap_mut() }.set_gc_threshold(usize::MAX);
    let key = Value::symbol("mode-owned-property");
    let probes = eval.with_gc_inhibited(|eval| {
        let mut probes = Vec::new();
        let mut registry = ModeRegistry::new();
        registry.register_major_mode(
            "owned-mode",
            MajorMode {
                pretty_name: text(&mut probes, key, LispString::from_utf8("Mode-é")),
                parent: Some(mode_symbol("fundamental-mode")),
                mode_hook: mode_symbol("owned-mode-hook"),
                keymap_name: Some(mode_symbol("owned-mode-map")),
                syntax_table_name: Some(mode_symbol("owned-mode-syntax-table")),
                abbrev_table_name: Some(mode_symbol("owned-mode-abbrev-table")),
                font_lock: Some(FontLockDefaults {
                    keywords: vec![
                        FontLockKeyword {
                            pattern: text(&mut probes, key, LispString::from_utf8("é")),
                            face: intern("font-lock-keyword-face"),
                            group: 0,
                            override_: false,
                            laxmatch: false,
                        },
                        FontLockKeyword {
                            pattern: text(&mut probes, key, LispString::from_unibyte(vec![0x80])),
                            face: intern("font-lock-string-face"),
                            group: 0,
                            override_: false,
                            laxmatch: false,
                        },
                    ],
                    case_fold: false,
                    syntax_table: Some(text(
                        &mut probes,
                        key,
                        LispString::from_unibyte(b"syntax".to_vec()),
                    )),
                }),
                body: Some(payload(&mut probes)),
            },
        );
        registry.register_minor_mode(
            "owned-minor-mode",
            MinorMode {
                lighter: Some(text(&mut probes, key, LispString::from_utf8(" Light-é"))),
                keymap_name: Some(mode_symbol("owned-minor-mode-map")),
                global: false,
                body: Some(payload(&mut probes)),
            },
        );
        registry.register_custom_group(
            "owned-group",
            CustomGroup {
                doc: Some(text(&mut probes, key, LispString::from_utf8("Group-é"))),
                parent: Some(mode_symbol("emacs")),
                members: vec![mode_symbol("owned-option")],
            },
        );
        registry.register_custom_variable(
            "owned-option",
            CustomVariable {
                default_value: payload(&mut probes),
                doc: Some(text(&mut probes, key, LispString::from_utf8("Option-é"))),
                type_: CustomType::List(Box::new(CustomType::Alist(
                    Box::new(choices(&mut probes)),
                    Box::new(CustomType::Plist(
                        Box::new(choices(&mut probes)),
                        Box::new(CustomType::List(Box::new(choices(&mut probes)))),
                    )),
                ))),
                group: Some(mode_symbol("owned-group")),
                set_function: Some(mode_symbol("set-default")),
                get_function: Some(mode_symbol("default-value")),
                tag: Some(text(&mut probes, key, LispString::from_utf8("Tag-é"))),
            },
        );
        registry.add_auto_mode(
            text(
                &mut probes,
                key,
                LispString::from_unibyte(b"owned-pattern".to_vec()),
            ),
            mode_symbol("owned-mode"),
        );
        // Public registrations retain the copies; no buffer, function cell,
        // Lisp variable or explicit root aliases their property graphs.
        *eval.modes_mut() = registry;
        probes
    });
    let check_live = |eval: &Context| {
        for Probe(bits) in &probes {
            assert!(
                eval.tagged_heap()
                    .retains_veclike(unsafe { Value::from_bits(*bits) }),
                "mode-only property vector was reclaimed"
            );
        }
        for (index, Probe(bits)) in probes.iter().enumerate() {
            assert_eq!(
                unsafe { Value::from_bits(*bits) }
                    .as_vector_data()
                    .expect("live mode property vector")
                    .as_slice(),
                &[Value::fixnum(index as i64)]
            );
        }
        assert_eq!(
            eval.modes()
                .get_major_mode_def("owned-mode")
                .expect("retained major mode")
                .pretty_name
                .intervals()
                .get_property_at_char_pos(CharPos0::ZERO, key)
                .expect("retained name property")
                .bits(),
            probes[0].0
        );
    };

    let completed = eval.gc_count_for_test();
    if eval.tagged_heap().tagged_heap_generational_enabled() {
        assert!(eval.tagged_heap().should_run_minor(false, false));
        neovm_eval::test_support::authority::set_gc_pending_for_test(&mut eval, true);
        eval.gc_safe_point_exact();
        assert!(
            eval.tagged_heap().sweep_in_progress(),
            "expected minor sweep"
        );
        assert!(!eval.tagged_heap().concurrent_mark_running());
        for _ in 0..10_000 {
            if !eval.tagged_heap().sweep_in_progress() {
                break;
            }
            eval.gc_safe_point_exact();
        }
        assert!(!eval.tagged_heap().sweep_in_progress());
        assert!(!eval.tagged_heap().mark_in_progress());
    } else {
        eval.gc_collect_exact();
    }
    assert_eq!(eval.gc_count_for_test(), completed + 1);
    check_live(&eval);
    let completed = eval.gc_count_for_test();
    eval.gc_collect_exact();
    assert_eq!(eval.gc_count_for_test(), completed + 1);
    check_live(&eval);

    *eval.modes_mut() = ModeRegistry::new();
    eval.gc_collect_exact();
    for Probe(bits) in probes {
        assert!(
            !eval
                .tagged_heap()
                .retains_veclike(unsafe { Value::from_bits(bits) }),
            "retired mode graph remained permanently rooted"
        );
    }
}

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

fn mode_symbol(name: &str) -> Value {
    Value::symbol(name)
}

fn mode_symbol_opt(name: Option<&str>) -> Option<Value> {
    name.map(mode_symbol)
}

fn mode_display(text: &str) -> neovm_value::heap_types::LispString {
    neovm_value::heap_types::LispString::from_utf8(text)
}

fn mode_display_opt(text: Option<&str>) -> Option<neovm_value::heap_types::LispString> {
    text.map(mode_display)
}

fn mode_pattern(text: &str) -> neovm_value::heap_types::LispString {
    neovm_value::heap_types::LispString::from_utf8(text)
}
