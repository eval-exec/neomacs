//! `describe-buffer-bindings` and `help--describe-vector` (GNU keymap.c
//! `Fdescribe_buffer_bindings` / `describe_vector`).
//!
//! Both are Lisp entries that insert text into the current buffer and run an
//! arbitrary Lisp DESCRIBER, so they bind display behaviour and stay in the
//! core command layer (STAGE4 design §5, R0). They read evaluator state
//! through `&mut Context`; the keymap Value substrate they walk stays below.

use crate::emacs_core::builtins::keymaps::builtin_key_description;
use crate::emacs_core::error::{
    EvalResult, Flow, LispCondition, expect_args, expect_args_range, signal,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::value::{Value, equal_value};

use super::collect_minor_mode_map_entries_in_state;
use neovm_edit::emacs_core::edit_host::EditHost;

#[allow(clippy::too_many_arguments)] // mirrors the Lisp helper's positional argument contract
fn call_help_describe_map_tree(
    eval: &mut Context,
    startmap: Value,
    partial: Value,
    shadow: Value,
    prefix: Value,
    title: Value,
    nomenu: Value,
    transl: Value,
    always_title: Value,
    mention_shadow: Value,
    buffer: Value,
) -> Result<Value, Flow> {
    eval.apply(
        Value::symbol("help--describe-map-tree"),
        vec![
            startmap,
            partial,
            shadow,
            prefix,
            title,
            nomenu,
            transl,
            always_title,
            mention_shadow,
            buffer,
        ],
    )
}

pub(crate) fn builtin_describe_buffer_bindings(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_args_range("describe-buffer-bindings", &args, 1, 3)?;
    if !args[0].is_buffer() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), args[0]],
        ));
    }
    if let Some(prefixes) = args.get(1)
        && !prefixes.is_nil()
        && !(prefixes.is_cons()
            || prefixes.is_vector()
            || prefixes.is_string()
            || prefixes.is_nil())
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *prefixes],
        ));
    }

    let buffer = args[0];
    let prefix = args.get(1).copied().unwrap_or(Value::NIL);
    let nomenu = if args.get(2).is_some_and(|v| !v.is_nil()) {
        Value::NIL
    } else {
        Value::T
    };

    let Some(buffer_id) = buffer.as_buffer_id() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), buffer],
        ));
    };
    let Some(buf) = eval.edit.buffers.get(buffer_id) else {
        return Err(signal(
            "error",
            vec![Value::string("Selecting deleted buffer")],
        ));
    };

    let buffer_keymap = buf.local_map();
    let buffer_point = buf.point_lisp_char_pos().as_i64();
    let major_mode_name = buf
        .get_buffer_local("major-mode")
        .and_then(|value| value.as_symbol_name(symbol_mode))
        .unwrap_or("fundamental-mode")
        .to_string();

    let sections = describe_buffer_binding_sections(
        eval,
        buffer,
        buffer_id,
        buffer_keymap,
        buffer_point,
        &major_mode_name,
    )?;

    // Every keymap and shadow cons below is held in a Rust local across
    // help--describe-map-tree (arbitrary Lisp, can GC); root them, or a
    // collection mid-describe frees them and the later passes walk freed
    // keymaps. GNU's C locals survive via conservative stack scanning. The
    // scope unwinds with the specpdl on nonlocal exit.
    let root_scope = eval.save_specpdl_roots();
    for section in &sections {
        eval.push_specpdl_root(section.map);
    }

    let mut shadow = Value::NIL;
    for section in sections {
        let (partial, transl, always_title, section_shadow) = match section.kind {
            BindingSectionKind::Bindings { always_title } => (
                Value::T,
                Value::NIL,
                if always_title { Value::T } else { Value::NIL },
                shadow,
            ),
            // GNU passes nil shadow to every translation map and never adds one
            // to the accumulator: a translation is not a binding, so it neither
            // hides nor is hidden by one.
            BindingSectionKind::Translations => (Value::NIL, Value::T, Value::NIL, Value::NIL),
        };
        call_help_describe_map_tree(
            eval,
            section.map,
            partial,
            section_shadow,
            prefix,
            Value::string(section.title),
            nomenu,
            transl,
            always_title,
            Value::NIL,
            buffer,
        )?;
        if matches!(section.kind, BindingSectionKind::Bindings { .. }) {
            shadow = Value::cons(section.map, shadow);
            eval.push_specpdl_root(shadow);
        }
    }

    eval.restore_specpdl_roots(root_scope);
    Ok(Value::NIL)
}

/// GNU `MAX_5_BYTE_CHAR + 1` (character.h): the boundary `describe_vector` walks
/// a char-table in two passes around, so that ordinary characters are described
/// before the raw 8-bit ones.
const DESCRIBE_VECTOR_CHAR_TABLE_STOP: i64 = 0x3F_FF80;
/// GNU `MAX_CHAR + 1`.
const DESCRIBE_VECTOR_CHAR_TABLE_END: i64 = 0x40_0000;

/// One row of `describe_vector` output: a key or key RANGE that shares a single
/// definition, together with whether an outer keymap shadows it.
///
/// Deciding "is this row shadowed, and does that mean skip it or annotate it?"
/// in the middle of the emit loop is how GNU's own bug#9293 arose (a range was
/// printed whose members were not all shadowed alike). Naming the three outcomes
/// keeps the emit loop unable to forget one.
enum RowShadowing {
    /// Nothing shadows this row; print it plainly.
    None,
    /// Shadowed, and MENTION-SHADOW is off: GNU drops the row entirely.
    Suppressed,
    /// Shadowed, and MENTION-SHADOW is on: print the row and say what shadows it.
    Mentioned { shadowed_by: Value },
}

/// GNU `Fhelp__describe_vector` / `describe_vector` (keymap.c).
///
/// Describes one VECTOR or char-table element of a keymap into the current
/// buffer. `describe-map` (help.el) reaches every dense keymap element through
/// here, so while this was a no-op stub every character bound in a char-table --
/// `self-insert-command` across the printable range, every `C-x 8` composition,
/// the whole of `key-translation-map` -- was silently missing from
/// `describe-bindings` and `describe-mode`.
pub(crate) fn builtin_help_describe_vector(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_args("help--describe-vector", &args, 7)?;
    let vector = args[0];
    let prefix = args[1];
    let describer = args[2];
    let partial = args[3].is_truthy();
    let shadow = args[4];
    let entire_map = args[5];
    let mention_shadow = args[6].is_truthy();

    let is_char_table = crate::emacs_core::chartable::is_char_table(&vector);
    if !is_char_table && !vector.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vector-or-char-table-p"), vector],
        ));
    }

    // GNU's `elt_prefix` stays nil here: `help--describe-vector` always describes
    // a keymap element, so the prefix is rendered by `key-description` together
    // with the key rather than pasted in front of it.
    let (mut stop, to) = if is_char_table {
        (
            DESCRIBE_VECTOR_CHAR_TABLE_STOP,
            DESCRIBE_VECTOR_CHAR_TABLE_END,
        )
    } else {
        let len = vector.as_vector_data().map_or(0, |data| data.len() as i64);
        (len, len)
    };

    let mut first = true;
    let mut i: i64 = 0;
    loop {
        if i == stop {
            if i == to {
                break;
            }
            stop = to;
        }
        let starting_i = i;

        let raw = if is_char_table {
            let (val, _from, run_end) =
                crate::emacs_core::chartable::char_table_ref_and_range_with_symbols(
                    &vector,
                    starting_i,
                    symbol_mode,
                )?;
            i = run_end.min(stop - 1).max(starting_i);
            val
        } else {
            vector.as_vector_data().unwrap()[starting_i as usize]
        };
        let definition = super::get_keyelt(raw, symbol_mode);
        if definition.is_nil() {
            i += 1;
            continue;
        }

        // Don't mention suppressed commands.
        if partial
            && definition.is_symbol(symbol_mode)
            && let Some(name) = definition.as_symbol_name(symbol_mode)
            && eval
                .obarray()
                .get_property(name, "suppress-keymap")
                .is_some_and(|value| value.is_truthy())
        {
            i += 1;
            continue;
        }

        let key_vector = |index: i64| Value::vector(vec![Value::fixnum(index)]);
        let shadowing = if shadow.is_nil() {
            RowShadowing::None
        } else {
            let shadowed_by = shadow_lookup_for_describe(eval, shadow, key_vector(starting_i))?;
            if shadowed_by.is_nil() || shadowed_by == definition {
                RowShadowing::None
            } else if mention_shadow {
                RowShadowing::Mentioned { shadowed_by }
            } else {
                RowShadowing::Suppressed
            }
        };
        if matches!(shadowing, RowShadowing::Suppressed) {
            i += 1;
            continue;
        }

        // Ignore a definition shadowed by an earlier one in the same keymap.
        if !entire_map.is_nil() {
            let in_whole_map = eval.apply(
                Value::symbol("lookup-key"),
                vec![entire_map, key_vector(starting_i), Value::T],
            )?;
            if in_whole_map != definition {
                i += 1;
                continue;
            }
        }

        if first {
            insert_describe_text(eval, "\n")?;
            first = false;
        }
        insert_fontified_key(eval, key_vector(starting_i), prefix)?;

        // Find all consecutive keys that share this definition. A char-table
        // already reported its run above.
        if !is_char_table {
            let data = vector.as_vector_data().unwrap();
            while i + 1 < stop {
                let next = super::get_keyelt(data[(i + 1) as usize], symbol_mode);
                if next.is_nil() || !equal_value(&next, &definition, 0) {
                    break;
                }
                i += 1;
            }
        }

        // A range is only honest if every key in it is shadowed the same way, so
        // GNU truncates it at the first member that is not (bug#9293).
        let check_ranges = eval
            .builtin_var_value_by_name("describe-bindings-check-shadowing-in-ranges")
            .unwrap_or(Value::NIL);
        let skip_self_insert = check_ranges.as_symbol_name(symbol_mode)
            == Some("ignore-self-insert")
            && definition.as_symbol_name(symbol_mode) == Some("self-insert-command");
        if check_ranges.is_truthy() && is_char_table && i != starting_i && !skip_self_insert {
            let shadowed_by = match shadowing {
                RowShadowing::Mentioned { shadowed_by } => shadowed_by,
                RowShadowing::None | RowShadowing::Suppressed => Value::NIL,
            };
            for j in (starting_i + 1)..=i {
                let at_j = shadow_lookup_for_describe(eval, shadow, key_vector(j))?;
                if !equal_value(&at_j, &shadowed_by, 0) {
                    i = j - 1;
                    break;
                }
            }
        }

        if i != starting_i {
            insert_describe_text(eval, " .. ")?;
            insert_fontified_key(eval, key_vector(i), prefix)?;
        }

        // DESCRIBER inserts the definition, including its own alignment.
        eval.apply(describer, vec![definition])?;

        if let RowShadowing::Mentioned { shadowed_by } = shadowing {
            // GNU steps back over the newline DESCRIBER just wrote so the note
            // lands on the same line, then steps forward again.
            let point = eval.apply(Value::symbol("point"), vec![])?;
            let before_newline = Value::fixnum(point.as_fixnum().unwrap_or(1) - 1);
            eval.apply(Value::symbol("goto-char"), vec![before_newline])?;
            let note = match shadowed_by.as_symbol_name(symbol_mode) {
                Some(name) => format!("  (currently shadowed by `{name}')"),
                // Could be a keymap, a lambda, or a keyboard macro.
                None => "  (currently shadowed)".to_string(),
            };
            insert_describe_text(eval, &note)?;
            eval.apply(
                Value::symbol("goto-char"),
                vec![Value::fixnum(
                    before_newline.as_fixnum().unwrap_or(1) + note.chars().count() as i64 + 1,
                )],
            )?;
        }

        i += 1;
    }

    if is_char_table {
        let default = crate::emacs_core::chartable::char_table_default(&vector);
        if !default.is_nil() {
            insert_describe_text(eval, "default")?;
            eval.apply(describer, vec![default])?;
        }
    }

    Ok(Value::NIL)
}

/// GNU `shadow_lookup (SHADOW, KEY, Qt, 0)` (keymap.c): the binding KEY has in
/// the shadowing maps, with a "key too long" answer reported as no binding.
fn shadow_lookup_for_describe(eval: &mut Context, shadow: Value, key: Value) -> EvalResult {
    let value = eval.apply(Value::symbol("lookup-key"), vec![shadow, key, Value::T])?;
    if value.as_fixnum().is_some_and(|n| n >= 0) {
        return Ok(Value::NIL);
    }
    Ok(value)
}

fn insert_describe_text(eval: &mut Context, text: &str) -> EvalResult {
    crate::emacs_core::buffer::builtin_insert(eval, vec![Value::string(text)])
}

/// GNU `describe_key_maybe_fontify` with `keymap_p` true: the key description,
/// carrying the `help-key-binding` face that `*Help*` renders keys in.
fn insert_fontified_key(eval: &mut Context, key: Value, prefix: Value) -> EvalResult {
    let description = builtin_key_description(vec![key, prefix], eval.symbol_mode())?;
    let length = eval.apply(Value::symbol("length"), vec![description])?;
    eval.apply(
        Value::symbol("add-text-properties"),
        vec![
            Value::fixnum(0),
            length,
            Value::list(vec![
                Value::symbol("font-lock-face"),
                Value::symbol("help-key-binding"),
            ]),
            description,
        ],
    )?;
    crate::emacs_core::buffer::builtin_insert(eval, vec![description])
}

/// How one `describe-bindings` section participates in shadowing, which is the
/// only axis on which GNU's ten-argument `help--describe-map-tree` calls actually
/// differ.
///
/// Spelling it as an enum keeps the rule in one place: every real keymap is
/// described against the maps already described (and shadows the ones after it),
/// while `key-translation-map`, `local-function-key-map` and `input-decode-map`
/// are described against nothing and shadow nothing. Passing the accumulated
/// shadow list to a translation map -- which is what this function used to do
/// with `key-translation-map` -- silently drops translations that happen to
/// collide with a binding.
#[derive(Clone, Copy)]
enum BindingSectionKind {
    /// A real keymap. GNU: PARTIAL=t, TRANSL=nil, SHADOW=the accumulator.
    Bindings {
        /// GNU's ALWAYS-TITLE: print the heading even when nothing is under it.
        /// Only the global section sets it.
        always_title: bool,
    },
    /// A translation map. GNU: PARTIAL=nil, TRANSL=t, SHADOW=nil.
    Translations,
}

struct BindingSection {
    title: String,
    map: Value,
    kind: BindingSectionKind,
}

/// The sections GNU `Fdescribe_buffer_bindings` (keymap.c) emits, in its order.
///
/// Collecting them before describing any of them keeps the order and the
/// overriding-map exclusion readable: GNU describes an overriding map INSTEAD OF
/// the `keymap` property, minor-mode and local maps, never alongside them.
fn describe_buffer_binding_sections(
    eval: &mut Context,
    buffer: Value,
    buffer_id: crate::buffer::BufferId,
    buffer_keymap: Value,
    buffer_point: i64,
    major_mode_name: &str,
) -> Result<Vec<BindingSection>, Flow> {
    let symbol_mode = eval.symbol_mode();
    let mut sections = Vec::new();
    let named_map = |name: &str| -> Option<Value> {
        eval.builtin_var_value_by_name(name)
            .filter(|map| !map.is_nil())
    };

    if let Some(key_translation_map) = named_map("key-translation-map") {
        sections.push(BindingSection {
            title: "Key translations".to_string(),
            map: key_translation_map,
            kind: BindingSectionKind::Translations,
        });
    }

    // GNU prefers `overriding-terminal-local-map`, falls back to
    // `overriding-local-map`, and when either is in force describes ONLY it.
    let overriding =
        named_map("overriding-terminal-local-map").or_else(|| named_map("overriding-local-map"));
    if let Some(overriding) = overriding {
        sections.push(BindingSection {
            title: "\u{c}\nOverriding Bindings".to_string(),
            map: overriding,
            kind: BindingSectionKind::Bindings {
                always_title: false,
            },
        });
    } else {
        let keymap_property = super::local_map_property_at_buffer_point(
            eval.variables(),
            &eval.edit.buffers,
            buffer,
            buffer_point,
            super::LocalMapProperty::Keymap,
            buffer_keymap,
            symbol_mode,
        )?;
        if !keymap_property.is_nil() {
            sections.push(BindingSection {
                title: "\u{c}\n`keymap' Property Bindings".to_string(),
                map: keymap_property,
                kind: BindingSectionKind::Bindings {
                    always_title: false,
                },
            });
        }

        for (mode, keymap) in collect_minor_mode_map_entries_in_state(
            eval.variables(),
            &[],
            &eval.edit.buffers,
            Some(buffer_id),
            symbol_mode,
        ) {
            sections.push(BindingSection {
                title: format!("\u{c}\n`{}' Minor Mode Bindings", resolve_sym(mode)),
                map: keymap,
                kind: BindingSectionKind::Bindings {
                    always_title: false,
                },
            });
        }

        let local_map = super::local_map_property_at_buffer_point(
            eval.variables(),
            &eval.edit.buffers,
            buffer,
            buffer_point,
            super::LocalMapProperty::LocalMap,
            buffer_keymap,
            symbol_mode,
        )?;
        if !local_map.is_nil() {
            // A `local-map' property REPLACES the buffer's own keymap, so the
            // heading names the property rather than the major mode.
            let title = if local_map == buffer_keymap {
                format!("\u{c}\n`{major_mode_name}' Major Mode Bindings")
            } else {
                "\u{c}\n`local-map' Property Bindings".to_string()
            };
            sections.push(BindingSection {
                title,
                map: local_map,
                kind: BindingSectionKind::Bindings {
                    always_title: false,
                },
            });
        }
    }

    sections.push(BindingSection {
        title: "\u{c}\nGlobal Bindings".to_string(),
        map: eval.current_global_map(),
        kind: BindingSectionKind::Bindings { always_title: true },
    });

    if let Some(function_key_map) = named_map("local-function-key-map") {
        sections.push(BindingSection {
            title: "\u{c}\nFunction key map translations".to_string(),
            map: function_key_map,
            kind: BindingSectionKind::Translations,
        });
    }

    if let Some(input_decode_map) = named_map("input-decode-map") {
        sections.push(BindingSection {
            title: "\u{c}\nInput decoding map translations".to_string(),
            map: input_decode_map,
            kind: BindingSectionKind::Translations,
        });
    }

    Ok(sections)
}
