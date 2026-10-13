use super::*;
use crate::emacs_core::error::FlowKind;

fn positioned(ctx: &mut Context, name: &str) -> Value {
    ctx.tagged_heap
        .alloc_symbol_with_pos(Value::symbol(name), Value::fixnum(11))
}

fn wrong_symbol_operand(result: EvalResult, operand: Value) {
    let error = result.expect_err("disabled positioned symbols must fail CHECK_SYMBOL");
    match error.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data, vec![Value::symbol("symbolp"), operand]);
        }
        other => panic!("expected symbolp condition, got {other:?}"),
    }
}

#[test]
fn positioned_coding_predicate_and_check_preserve_policy_and_operand() {
    let mut ctx = Context::new();
    let coding = positioned(&mut ctx, "utf-8-unix");
    for mode in [SymbolMode::Bare, SymbolMode::Positioned] {
        assert_eq!(
            builtin_coding_system_p(&ctx.coding_systems, vec![coding], mode).unwrap(),
            Value::bool_val(mode == SymbolMode::Positioned)
        );
    }
    assert_eq!(
        builtin_check_coding_system(&ctx.coding_systems, vec![coding], SymbolMode::Positioned)
            .unwrap(),
        coding,
        "GNU coding.c:8668 returns the original argument"
    );
    wrong_symbol_operand(
        builtin_check_coding_system(&ctx.coding_systems, vec![coding], SymbolMode::Bare),
        coding,
    );
    let invalid = positioned(&mut ctx, "issue-554-no-coding-system");
    let error =
        builtin_check_coding_system(&ctx.coding_systems, vec![invalid], SymbolMode::Positioned)
            .unwrap_err();
    match error.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "coding-system-error");
            assert_eq!(signal.data, vec![invalid]);
        }
        other => panic!("expected coding-system-error, got {other:?}"),
    }
}

#[test]
fn positioned_coding_metadata_cache_never_bypasses_disabled_symbol_checks() {
    let mut ctx = Context::new();
    let coding = positioned(&mut ctx, "utf-8-unix");
    let bare = Value::symbol("utf-8-unix");
    for operation in [
        builtin_coding_system_plist,
        builtin_coding_system_base,
        builtin_coding_system_type,
    ] {
        // Populate with a bare key, then take the positioned lookup and error
        // paths. Cache hits must implement the same SYMBOLP policy as misses.
        let expected = operation(&ctx.coding_systems, vec![bare], SymbolMode::Bare).unwrap();
        let observed =
            operation(&ctx.coding_systems, vec![coding], SymbolMode::Positioned).unwrap();
        assert!(crate::emacs_core::value::equal_value(
            &expected, &observed, 0
        ));
        wrong_symbol_operand(
            operation(&ctx.coding_systems, vec![coding], SymbolMode::Bare),
            coding,
        );
    }
    assert_eq!(
        builtin_coding_system_eol_type(&ctx.coding_systems, vec![coding], SymbolMode::Positioned)
            .unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_coding_system_eol_type(&ctx.coding_systems, vec![coding], SymbolMode::Bare)
            .unwrap(),
        Value::NIL
    );
    assert!(
        builtin_coding_system_aliases(&ctx.coding_systems, vec![coding], SymbolMode::Positioned)
            .unwrap()
            .is_cons()
    );
    wrong_symbol_operand(
        builtin_coding_system_aliases(&ctx.coding_systems, vec![coding], SymbolMode::Bare),
        coding,
    );
}

#[test]
fn positioned_coding_property_keys_and_hook_values_honor_mode() {
    let mut ctx = Context::new();
    let coding = positioned(&mut ctx, "utf-8-unix");
    let property = positioned(&mut ctx, ":post-read-conversion");
    let hook = positioned(&mut ctx, "issue-554-hook");
    assert_eq!(
        builtin_coding_system_put(
            &mut ctx.coding_systems,
            vec![coding, property, hook],
            SymbolMode::Positioned,
        )
        .unwrap(),
        hook
    );
    assert_eq!(
        builtin_coding_system_get(
            &ctx.coding_systems,
            vec![coding, property],
            SymbolMode::Positioned,
        )
        .unwrap(),
        hook
    );
    wrong_symbol_operand(
        builtin_coding_system_put(
            &mut ctx.coding_systems,
            vec![
                Value::symbol("utf-8-unix"),
                Value::symbol(":post-read-conversion"),
                hook,
            ],
            SymbolMode::Bare,
        ),
        hook,
    );
    for name in [":decode-translation-table", ":encode-translation-table"] {
        let property = positioned(&mut ctx, name);
        assert_eq!(
            builtin_coding_system_put(
                &mut ctx.coding_systems,
                vec![coding, property, hook],
                SymbolMode::Positioned,
            )
            .unwrap(),
            hook
        );
        wrong_symbol_operand(
            builtin_coding_system_put(
                &mut ctx.coding_systems,
                vec![Value::symbol("utf-8-unix"), Value::symbol(name), hook],
                SymbolMode::Bare,
            ),
            hook,
        );
    }
}

fn definition(
    name: Value,
    coding_type: Value,
    charset: Value,
    hook: Value,
    eol: Value,
) -> Vec<Value> {
    vec![
        name,
        Value::fixnum(i64::from(b'U')),
        coding_type,
        Value::list(vec![charset]),
        Value::T,
        Value::NIL,
        Value::NIL,
        hook,
        hook,
        Value::fixnum(i64::from(b'?')),
        Value::NIL,
        Value::NIL,
        eol,
    ]
}

#[test]
fn positioned_coding_definition_resolves_nested_symbol_inputs() {
    let mut ctx = Context::new();
    let name = positioned(&mut ctx, "issue-554-defined");
    let coding_type = positioned(&mut ctx, "utf-8");
    let charset = positioned(&mut ctx, "ascii");
    let hook = positioned(&mut ctx, "issue-554-hook");
    let eol = positioned(&mut ctx, "unix");
    let key = positioned(&mut ctx, ":issue-554-property");
    let mut args = definition(name, coding_type, charset, hook, eol);
    args[11] = Value::list(vec![key, Value::fixnum(7)]);
    assert_eq!(
        builtin_define_coding_system_internal(
            &mut ctx.coding_systems,
            args.clone(),
            SymbolMode::Positioned
        )
        .unwrap(),
        Value::NIL
    );
    let info = ctx.coding_systems.get("issue-554-defined").unwrap();
    assert_eq!(info.coding_type, intern("utf-8"));
    assert_eq!(info.charset_list, vec![intern("ascii")]);
    assert_eq!(info.post_read_conversion, Some(intern("issue-554-hook")));
    assert_eq!(info.pre_write_conversion, Some(intern("issue-554-hook")));
    assert_eq!(info.eol_type, EolType::Unix);
    assert_eq!(
        info.properties.get(&intern(":issue-554-property")),
        Some(&Value::fixnum(7))
    );
    // SAFETY: registration is complete; this noncalling recorder only writes
    // coding metadata values/properties, preserving canonical dispatch state.
    unsafe {
        ctx.obarray.with_legacy_value_write(|obarray| {
            record_lisp_define_coding_system_internal(obarray, &args, SymbolMode::Positioned);
        });
    }
    let names = ctx
        .obarray
        .symbol_value_copied("coding-system-list")
        .unwrap();
    assert!(
        list_to_vec(&names)
            .unwrap()
            .contains(&Value::symbol("issue-554-defined"))
    );
    wrong_symbol_operand(
        builtin_define_coding_system_internal(
            &mut ctx.coding_systems,
            args.clone(),
            SymbolMode::Bare,
        ),
        name,
    );
    args[0] = Value::symbol("issue-554-bare-name");
    wrong_symbol_operand(
        builtin_define_coding_system_internal(&mut ctx.coding_systems, args, SymbolMode::Bare),
        coding_type,
    );
}

#[test]
fn positioned_coding_alias_priority_and_terminal_setters_honor_mode() {
    let mut ctx = Context::new();
    let alias = positioned(&mut ctx, "issue-554-alias");
    let coding = positioned(&mut ctx, "utf-8-unix");
    builtin_define_coding_system_alias(
        &mut ctx.coding_systems,
        vec![alias, coding],
        SymbolMode::Positioned,
    )
    .unwrap();
    assert_eq!(
        ctx.coding_systems.resolve("issue-554-alias"),
        ctx.coding_systems.resolve("utf-8-unix")
    );
    wrong_symbol_operand(
        builtin_define_coding_system_alias(
            &mut ctx.coding_systems,
            vec![alias, coding],
            SymbolMode::Bare,
        ),
        alias,
    );
    builtin_set_coding_system_priority(
        &mut ctx.coding_systems,
        vec![coding],
        SymbolMode::Positioned,
    )
    .unwrap();
    wrong_symbol_operand(
        builtin_set_coding_system_priority(&mut ctx.coding_systems, vec![coding], SymbolMode::Bare),
        coding,
    );
    builtin_set_keyboard_coding_system(
        &mut ctx.coding_systems,
        vec![coding],
        SymbolMode::Positioned,
    )
    .unwrap();
    builtin_set_terminal_coding_system(
        &mut ctx.coding_systems,
        vec![coding],
        SymbolMode::Positioned,
    )
    .unwrap();
    assert_eq!(
        ctx.coding_systems.keyboard_coding_sym(),
        intern("utf-8-unix")
    );
    assert_eq!(
        ctx.coding_systems.terminal_coding_sym(),
        intern("utf-8-unix")
    );
    wrong_symbol_operand(
        builtin_set_keyboard_coding_system(&mut ctx.coding_systems, vec![coding], SymbolMode::Bare),
        coding,
    );
    wrong_symbol_operand(
        builtin_set_terminal_coding_system(&mut ctx.coding_systems, vec![coding], SymbolMode::Bare),
        coding,
    );
}

#[test]
fn positioned_coding_region_list_and_eol_helper_honor_mode() {
    let mut ctx = Context::new();
    let coding = positioned(&mut ctx, "us-ascii");
    let source = Value::string("é");
    let args = vec![source, Value::NIL, Value::list(vec![coding])];
    ctx.symbols_with_pos_enabled = true;
    let result = builtin_check_coding_systems_region(&mut ctx, args.clone()).unwrap();
    let entry = result.cons_car();
    assert_eq!(entry.cons_car(), coding);
    assert_eq!(entry.cons_cdr().cons_car(), Value::fixnum(0));
    ctx.symbols_with_pos_enabled = false;
    assert!(builtin_check_coding_systems_region(&mut ctx, args).is_err());
    let undecided = positioned(&mut ctx, "utf-8");
    assert_eq!(
        coding_inherit_eol_type_unix(&ctx.coding_systems, undecided, SymbolMode::Positioned),
        Value::symbol("utf-8-unix")
    );
    assert_eq!(
        coding_inherit_eol_type_unix(&ctx.coding_systems, undecided, SymbolMode::Bare),
        undecided
    );
}

#[test]
fn positioned_coding_iso_descriptors_and_quoting_style_honor_mode() {
    let mut ctx = Context::new();
    let charset = positioned(&mut ctx, "ascii");
    let mut info = CodingSystemInfo::new("issue-554-iso", "iso-2022", 'I', EolType::Unix);
    info.int_properties.insert(
        ISO2022_KEY_INITIAL,
        Value::vector(vec![charset, Value::NIL, Value::NIL, Value::NIL]),
    );
    info.int_properties.insert(
        ISO2022_KEY_REQUEST,
        Value::list(vec![Value::cons(charset, Value::fixnum(0))]),
    );
    info.int_properties
        .insert(ISO2022_KEY_FLAGS, Value::fixnum(0));
    let enabled = iso2022_spec(&info, SymbolMode::Positioned).unwrap();
    assert_eq!(enabled.initial[0], Some(intern("ascii")));
    assert_eq!(enabled.request, vec![(intern("ascii"), 0)]);
    let disabled = iso2022_spec(&info, SymbolMode::Bare).unwrap();
    assert_eq!(disabled.initial[0], None);
    assert!(disabled.request.is_empty());
    let style = positioned(&mut ctx, "grave");
    assert_eq!(
        TextQuotingStyle::from_symbol_value(style, SymbolMode::Positioned),
        Some(TextQuotingStyle::Grave)
    );
    assert_eq!(
        TextQuotingStyle::from_symbol_value(style, SymbolMode::Bare),
        None
    );
}
