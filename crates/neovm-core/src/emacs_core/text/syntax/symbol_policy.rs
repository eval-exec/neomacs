//! GNU SYMBOLP/EQ policy for positioned syntax-table purposes.
use super::*;
use crate::tagged::SymbolMode;

pub(crate) fn builtin_syntax_table_p_with_symbols(
    args: Vec<Value>,
    mode: SymbolMode,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("syntax-table-p"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    Ok(
        if crate::emacs_core::chartable::char_table_has_subtype_named_with_symbols(
            &args[0],
            "syntax-table",
            mode,
        ) {
            Value::T
        } else {
            Value::NIL
        },
    )
}

pub(crate) fn builtin_copy_syntax_table_with_symbols(
    args: Vec<Value>,
    mode: SymbolMode,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("copy-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let source = if args.is_empty() || args[0].is_nil() {
        builtin_standard_syntax_table(vec![])?
    } else {
        let table = args[0];
        if builtin_syntax_table_p_with_symbols(vec![table], mode)?.is_nil() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("syntax-table-p"), table],
            ));
        }
        table
    };

    let copy = crate::emacs_core::chartable::copy_char_table_with_symbols(source, mode)
        .expect("validated syntax table");
    crate::emacs_core::chartable::builtin_set_char_table_range_with_symbols(
        vec![copy, Value::NIL, Value::NIL],
        None,
        mode,
    )?;
    if crate::emacs_core::chartable::builtin_char_table_parent(vec![copy])?.is_nil() {
        crate::emacs_core::chartable::builtin_set_char_table_parent(vec![
            copy,
            ensure_standard_syntax_table_object()?,
        ])?;
    }
    Ok(copy)
}

pub(crate) fn current_buffer_syntax_table_object_in_buffers_with_symbols(
    buffers: &mut BufferManager,
    mode: SymbolMode,
) -> Result<Value, Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Mirrors GNU `Fsyntax_table` (`syntax.c:987-993`):
    //     return BVAR (current_buffer, syntax_table);
    let value = buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()];
    if !value.is_nil()
        && crate::emacs_core::chartable::char_table_has_subtype_named_with_symbols(
            &value,
            "syntax-table",
            mode,
        )
    {
        return Ok(value);
    }
    // Slot invalid: only now is the standard-table fallback needed (its
    // eager computation sat on every per-scan table fetch).
    let fallback = ensure_standard_syntax_table_object()?;

    // Slot is unset (fresh buffer or never assigned). Seed it
    // from the standard syntax table — matches GNU's
    // `reset_buffer` (`buffer.c:1149-1157`) which copies the
    // standard tables into a fresh buffer.
    buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()] = fallback;
    Ok(fallback)
}

pub(crate) fn builtin_syntax_table_in_buffers_with_symbols(
    buffers: &mut BufferManager,
    args: &[Value],
    mode: SymbolMode,
) -> EvalResult {
    if !args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![syntax_table_prop_symbol(), Value::fixnum(args.len() as i64)],
        ));
    }
    current_buffer_syntax_table_object_in_buffers_with_symbols(buffers, mode)
}

pub(crate) fn builtin_set_syntax_table_in_buffers_with_symbols(
    buffers: &mut BufferManager,
    args: &[Value],
    mode: SymbolMode,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if builtin_syntax_table_p_with_symbols(vec![args[0]], mode)?.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("syntax-table-p"), args[0]],
        ));
    }
    let table = args[0];
    // Matches GNU `Fset_syntax_table` — just bset_syntax_table on the
    // slot. Motion code reads it live via `SyntaxTable::for_buffer`.
    set_current_buffer_syntax_table_object_in_buffers(buffers, table)?;
    Ok(table)
}

pub(crate) fn modify_syntax_entry_in_buffers_with_symbols(
    buffers: &mut BufferManager,
    args: &[Value],
    mode: SymbolMode,
) -> EvalResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("modify-syntax-entry"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let descriptor = syntax_runtime_string(&args[1])?;
    let entry =
        string_to_syntax(&descriptor).map_err(|msg| signal("error", vec![Value::string(&msg)]))?;
    let target_table = if let Some(table) = args.get(2) {
        if builtin_syntax_table_p_with_symbols(vec![*table], mode)?.is_nil() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("syntax-table-p"), *table],
            ));
        }
        *table
    } else {
        current_buffer_syntax_table_object_in_buffers_with_symbols(buffers, mode)?
    };
    let current_table = current_buffer_syntax_table_object_in_buffers_with_symbols(buffers, mode)?;
    let update_current_buffer_table = target_table == current_table;

    // Update the exposed syntax-table object.
    let chartable_entry = if matches!(entry.class, SyntaxClass::InheritStd) {
        Value::NIL
    } else {
        syntax_entry_to_value(&entry)
    };
    crate::emacs_core::chartable::builtin_set_char_table_range_with_symbols(
        vec![target_table, args[0], chartable_entry],
        None,
        mode,
    )?;
    // GNU `Fmodify_syntax_entry` ends with `clear_regexp_cache ()` —
    // compiled regexps whose fastmap/bitmap baked syntax-table content
    // must not survive a table mutation.  Our caches key those entries
    // by (table identity, epoch); bump the epoch.
    note_syntax_table_mutation();

    if !update_current_buffer_table {
        return Ok(Value::NIL);
    }
    // Current buffer's slot already points at `target_table` (it's the
    // same chartable we just mutated above via set-char-table-range).
    // No compiled form to refresh — motion reads the chartable live.
    let _ = target_table;
    Ok(Value::NIL)
}

pub(crate) fn syntax_entry_at_char_code_with_symbols(
    table: &Value,
    code: u32,
    mode: SymbolMode,
) -> Option<SyntaxEntry> {
    if mode == SymbolMode::Bare {
        return syntax_entry_at_char_code(table, code);
    }
    let effective = if table.is_nil() {
        ensure_standard_syntax_table_object().unwrap_or(Value::NIL)
    } else {
        *table
    };
    if effective.is_nil() {
        return None;
    }
    let entry =
        match crate::emacs_core::chartable::ct_lookup_with_symbols(&effective, code as i64, mode) {
            Ok(entry) => entry,
            result @ Err(_) => {
                discard_syntax_lookup_error(result);
                return None;
            }
        };
    syntax_entry_from_chartable_entry(&entry)
}

pub(crate) fn syntax_entry_from_syntax_property_with_symbols(
    prop: Value,
    ch: char,
    mode: SymbolMode,
) -> Option<SyntaxEntry> {
    if mode == SymbolMode::Bare {
        return syntax_entry_from_syntax_property(prop, ch);
    }
    if builtin_syntax_table_p_with_symbols(vec![prop], mode)
        .ok()?
        .is_truthy()
    {
        let raw = crate::emacs_core::chartable::builtin_char_table_range_with_symbols(
            vec![prop, Value::fixnum(ch as i64)],
            None,
            mode,
        )
        .ok()?;
        syntax_entry_from_chartable_entry(&raw)
    } else {
        syntax_entry_from_chartable_entry(&prop)
    }
}

pub(crate) fn regexp_syntax_class_at_emacs_byte_with_symbols(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    byte_pos: EmacsBytePos,
    prop_cache: &SyntaxPropByteRun<'_>,
    mode: SymbolMode,
) -> SyntaxClass {
    if mode == SymbolMode::Bare {
        return regexp_syntax_class_at_emacs_byte(buf, table, ch, byte_pos, prop_cache);
    }
    if let Some(prop) = prop_cache.syntax_table_prop_at_emacs_byte(buf, byte_pos)
        && let Some(entry) = syntax_entry_from_syntax_property_with_symbols(prop, ch, mode)
    {
        return entry.class;
    }
    syntax_entry_at_char_code_with_symbols(&table.chartable(), ch as u32, mode)
        .map_or_else(|| table.char_syntax(ch), |entry| entry.class)
}

pub(crate) fn regexp_syntax_class_at_string_byte_with_symbols(
    table: &SyntaxTable,
    ch: char,
    byte_pos: usize,
    prop_cache: &StringSyntaxPropByteRun<'_>,
    mode: SymbolMode,
) -> SyntaxClass {
    if mode == SymbolMode::Bare {
        return regexp_syntax_class_at_string_byte(table, ch, byte_pos, prop_cache);
    }
    if let Some(prop) = prop_cache.syntax_table_prop_at_string_byte(byte_pos)
        && let Some(entry) = syntax_entry_from_syntax_property_with_symbols(prop, ch, mode)
    {
        return entry.class;
    }
    syntax_entry_at_char_code_with_symbols(&table.chartable(), ch as u32, mode)
        .map_or_else(|| table.char_syntax(ch), |entry| entry.class)
}

pub(crate) fn builtin_syntax_table_p_in_state(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_syntax_table_p_with_symbols(args, eval.symbol_mode())
}

pub(crate) fn builtin_copy_syntax_table_in_state(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_copy_syntax_table_with_symbols(args, eval.symbol_mode())
}

impl<'a> SyntaxProperties<'a> {
    pub(crate) fn for_scan_with_symbols(
        honor: bool,
        obarray: &'a Obarray,
        buffers: &BufferManager,
        mode: SymbolMode,
    ) -> Self {
        let properties = Self::for_scan(honor, obarray, buffers);
        if mode == SymbolMode::Bare {
            return properties;
        }
        match properties {
            Self::Ignore => Self::PositionedIgnore,
            Self::Honor(resolver) => Self::PositionedHonor(resolver),
            positioned => positioned,
        }
    }

    pub(crate) fn symbol_mode(self) -> SymbolMode {
        match self {
            Self::PositionedIgnore | Self::PositionedHonor(_) => SymbolMode::Positioned,
            Self::Ignore | Self::Honor(_) => SymbolMode::Bare,
        }
    }
}

pub(crate) fn syntax_entry_from_table_with_symbols(
    table: &SyntaxTable,
    ch: char,
    mode: SymbolMode,
) -> SyntaxEntry {
    if mode == SymbolMode::Bare {
        return syntax_entry_from_table(table, ch);
    }
    record_syntax_table_decode();
    syntax_entry_at_char_code_with_symbols(&table.chartable(), ch as u32, mode)
        .unwrap_or_else(|| SyntaxEntry::simple(table.char_syntax(ch)))
}
