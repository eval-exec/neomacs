use super::*;

fn lisp_string(bytes: &[u8]) -> DumpLispString {
    DumpLispString {
        data: bytes.to_vec(),
        size: bytes.len(),
        size_byte: bytes.len() as i64,
    }
}

#[test]
fn runtime_managers_section_round_trips_representative_state() {
    let managers = RuntimeManagersState {
        custom: DumpCustomManager {
            auto_buffer_local_syms: vec![DumpSymId(1)],
            auto_buffer_local: vec!["legacy-local".to_string()],
        },
        modes: DumpModeRegistry {
            major_modes: vec![(
                DumpSymId(2),
                DumpMajorMode {
                    pretty_name: lisp_string(b"Probe"),
                    parent: Some(DumpValue::Symbol(DumpSymId(3))),
                    mode_hook: DumpValue::Nil,
                    keymap_name: Some(DumpValue::Symbol(DumpSymId(4))),
                    syntax_table_name: None,
                    abbrev_table_name: None,
                    font_lock: Some(DumpFontLockDefaults {
                        keywords: vec![DumpFontLockKeyword {
                            pattern_lisp: Some(lisp_string(b"rx")),
                            pattern: None,
                            face_sym: Some(DumpSymId(5)),
                            face: None,
                            group: 1,
                            override_: true,
                            laxmatch: false,
                        }],
                        case_fold: true,
                        syntax_table_lisp: None,
                        syntax_table: Some("syntax".to_string()),
                    }),
                    body: Some(DumpValue::Int(9)),
                },
            )],
            minor_modes: vec![(
                DumpSymId(6),
                DumpMinorMode {
                    lighter: Some(lisp_string(b" L")),
                    keymap_name: None,
                    global: false,
                    body: Some(DumpValue::True),
                },
            )],
            buffer_major_modes: vec![(7, DumpValue::Symbol(DumpSymId(8)))],
            buffer_minor_modes: vec![(7, vec![DumpValue::Symbol(DumpSymId(9))])],
            global_minor_modes: vec![DumpValue::Symbol(DumpSymId(10))],
            auto_mode_alist_lisp: vec![(
                lisp_string(b"\\.rs\\'"),
                DumpValue::Symbol(DumpSymId(11)),
            )],
            auto_mode_alist: vec![("legacy".to_string(), DumpValue::Nil)],
            custom_variables: vec![(
                DumpSymId(12),
                DumpModeCustomVariable {
                    default_value: DumpValue::Int(1),
                    doc: Some(lisp_string(b"doc")),
                    custom_type: DumpModeCustomType::Choice(vec![(
                        "one".to_string(),
                        DumpValue::Int(1),
                    )]),
                    group: None,
                    set_function: None,
                    get_function: None,
                    tag: None,
                },
            )],
            custom_groups: vec![(
                DumpSymId(13),
                DumpModeCustomGroup {
                    doc: None,
                    parent: Some(DumpValue::Symbol(DumpSymId(14))),
                    members: vec![DumpValue::Symbol(DumpSymId(15))],
                },
            )],
            fundamental_mode: DumpValue::Symbol(DumpSymId(16)),
        },
        fontset_registry: DumpFontsetRegistry {
            ordered_names_lisp: vec![lisp_string(b"default")],
            alias_to_name_lisp: vec![(lisp_string(b"alias"), lisp_string(b"default"))],
            fontsets_lisp: vec![(
                lisp_string(b"default"),
                DumpFontsetData {
                    ranges: vec![DumpFontsetRangeEntry {
                        from: 0,
                        to: 127,
                        entries: vec![DumpFontSpecEntry::Font(DumpStoredFontSpec {
                            family_sym: Some(DumpSymId(17)),
                            family: None,
                            registry_sym: None,
                            registry: Some("iso10646-1".to_string()),
                            lang_sym: None,
                            lang: None,
                            weight: Some(400),
                            slant: Some(DumpFontSlant::Normal),
                            width: Some(DumpFontWidth::Normal),
                            repertory: Some(DumpFontRepertory::CharsetSym(DumpSymId(18))),
                        })],
                    }],
                    fallback: Some(vec![DumpFontSpecEntry::ExplicitNone]),
                },
            )],
            ordered_names: vec!["legacy-default".to_string()],
            alias_to_name: vec![("legacy-alias".to_string(), "legacy-default".to_string())],
            fontsets: Vec::new(),
            generation: 4,
        },
        abbrevs: DumpAbbrevManager {
            tables_syms: vec![(
                DumpSymId(19),
                DumpAbbrevTable {
                    name: lisp_string(b"table"),
                    abbrevs: vec![(
                        lisp_string(b"btw"),
                        DumpAbbrev {
                            expansion: lisp_string(b"by the way"),
                            hook: None,
                            count: 3,
                            system: false,
                        },
                    )],
                    parent: None,
                    case_fixed: true,
                    enable_quoting: false,
                },
            )],
            tables: Vec::new(),
            global_table_sym: Some(DumpSymId(19)),
            global_table_name: lisp_string(b"table"),
            abbrev_mode: true,
        },
        interactive: DumpInteractiveRegistry {
            specs: vec![(
                DumpSymId(20),
                DumpInteractiveSpec {
                    spec: DumpValue::Int(1),
                },
            )],
        },
        rectangle: DumpRectangleState {
            killed: vec![lisp_string(b"rect")],
        },
        kmacro: DumpKmacroManager {
            current_macro: vec![DumpValue::Int(1)],
            last_macro: Some(vec![DumpValue::Int(2)]),
            macro_ring: vec![vec![DumpValue::Int(3)]],
            counter: 5,
            counter_format_lisp: Some(lisp_string(b"%d")),
            counter_format: None,
        },
        registers: DumpRegisterManager {
            registers: vec![('a', DumpRegisterContent::File(lisp_string(b"/tmp/a")))],
        },
        bookmarks: DumpBookmarkManager {
            bookmarks_lisp: vec![(
                lisp_string(b"home"),
                DumpBookmark {
                    name: lisp_string(b"home"),
                    filename: Some(lisp_string(b"/tmp/home")),
                    position: 12,
                    front_context: None,
                    rear_context: None,
                    annotation: Some(lisp_string(b"note")),
                    handler: None,
                },
            )],
            bookmarks: Vec::new(),
            recent: vec![lisp_string(b"home")],
        },
        watchers: DumpVariableWatcherList {
            watchers: vec![(DumpSymId(21), vec![DumpValue::Symbol(DumpSymId(22))])],
        },
    };

    let bytes = runtime_managers_section_bytes(&managers).expect("encode runtime managers");
    let loaded = load_runtime_managers_section(&bytes).expect("decode runtime managers");
    assert_eq!(loaded.custom.auto_buffer_local_syms.len(), 1);
    assert_eq!(loaded.modes.major_modes.len(), 1);
    assert_eq!(loaded.fontset_registry.fontsets_lisp.len(), 1);
    assert_eq!(loaded.abbrevs.tables_syms.len(), 1);
    assert_eq!(loaded.interactive.specs.len(), 1);
    assert_eq!(loaded.rectangle.killed.len(), 1);
    assert_eq!(loaded.kmacro.counter, 5);
    assert_eq!(loaded.registers.registers.len(), 1);
    assert_eq!(loaded.bookmarks.bookmarks_lisp.len(), 1);
    assert_eq!(loaded.watchers.watchers.len(), 1);
}
