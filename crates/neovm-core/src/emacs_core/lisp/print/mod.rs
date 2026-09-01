//! Value printing (Lisp representation).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;

use super::chartable::{bool_vector_length, char_table_external_slots};
use super::intern::{
    SymId, intern, lookup_interned_lisp_string, resolve_sym, resolve_sym_lisp_string,
    symbol_name_confusing,
};
use super::string_escape::format_lisp_string_bytes_emacs;
use super::value::{
    HashKey, HashTableTest, LispHashTable, StringTextPropertyRun, Value,
    get_string_text_properties_for_value, list_to_vec,
};
use crate::buffer::EmacsBytePos;
use crate::emacs_core::value::{ValueKind, VecLikeType};

/// Canonical output buffer for the stateful printer.
///
/// GNU's `print_object` always emits Emacs characters to a sink; rendering to
/// a Lisp string is only one possible sink.  Keeping bytes here is essential:
/// Rust `String` cannot represent Emacs byte8/non-Unicode characters, so using
/// it as the graph-aware printer's intermediate representation made
/// `print-circle`, `print-level`, and `print-length` silently lossy.
#[derive(Default)]
struct StatefulPrintOutput {
    bytes: Vec<u8>,
}

impl StatefulPrintOutput {
    fn from_str(value: &str) -> Self {
        Self {
            bytes: value.as_bytes().to_vec(),
        }
    }

    fn push(&mut self, character: char) {
        let mut encoded = [0; 4];
        self.bytes
            .extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    }

    fn push_str(&mut self, value: &str) {
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn append(&mut self, other: Self) {
        self.bytes.extend(other.bytes);
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl std::fmt::Write for StatefulPrintOutput {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.push_str(value);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrintOptions {
    pub print_gensym: bool,
    pub print_circle: bool,
    pub print_quoted: bool,
    pub print_symbols_bare: bool,
    pub print_escape_newlines: bool,
    pub print_escape_nonascii: bool,
    pub print_escape_multibyte: bool,
    pub print_escape_control_characters: bool,
    pub print_integers_as_characters: bool,
    pub print_level: Option<i64>,
    pub print_length: Option<i64>,
    pub print_continuous_numbering: bool,
    pub print_number_table: Option<Value>,
    pub float_output_format: Option<Value>,
    pub print_noescape: bool,
    backquote_output_level: usize,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self::new(false, false, None, None)
    }
}

/// Return the test-name symbol GNU would print for this table, or `None` when
/// the test is the default `eql` (in which case GNU omits the ` test ` field).
///
/// Mirrors GNU's `!BASE_EQ (h->test->name, Qeql)` check (print.c): the
/// comparison is against the test *name* symbol, so a table created with
/// `:test 'eql` (which sets `test_name` to `eql`) is treated as the default.
fn hash_table_printed_test_name(table: &LispHashTable) -> Option<&'static str> {
    match table.test_name {
        Some(test_name) => {
            let name = resolve_sym(test_name);
            if name == "eql" { None } else { Some(name) }
        }
        None => {
            if matches!(table.test, HashTableTest::Eql) {
                None
            } else {
                Some(table.test.name())
            }
        }
    }
}

fn append_hash_table_test_string(table: &LispHashTable, out: &mut StatefulPrintOutput) {
    if let Some(name) = hash_table_printed_test_name(table) {
        out.push_str(" test ");
        out.push_str(name);
    }
}

fn append_hash_table_test_bytes(table: &LispHashTable, out: &mut Vec<u8>) {
    if let Some(name) = hash_table_printed_test_name(table) {
        out.extend_from_slice(b" test ");
        out.extend_from_slice(name.as_bytes());
    }
}

/// Mirrors GNU `named_escape` (print.c): the single-letter escape used to
/// print a character in `?\X` form, or `None` if there is no named escape.
///
/// `\a`, `\v`, `\e` and `\d` are intentionally excluded (matching GNU): they
/// are rare as characters and more likely meant as plain integers.
fn named_escape(i: u32) -> Option<char> {
    match i {
        0x08 => Some('b'), // \b
        0x09 => Some('t'), // \t
        0x0A => Some('n'), // \n
        0x0C => Some('f'), // \f
        0x0D => Some('r'), // \r
        0x20 => Some('s'), // space
        _ => None,
    }
}

/// If `print-integers-as-characters` is active and `i` is a character that
/// GNU prints in `?X` syntax, format it that way and return `true`. Mirrors
/// the `Lisp_Int0/Int1` case of GNU `print_object` (print.c).
///
/// `escapeflag` is true for `prin1`-style output (adds the extra backslash
/// before the self-delimiting characters), false for `princ`-style output.
fn try_format_integer_as_character(
    i: i64,
    escapeflag: bool,
    push_char: &mut dyn FnMut(char),
) -> bool {
    use crate::emacs_core::emacs_char::{MAX_UNICODE_CHAR, char_general_category, graphic_base_p};

    if !(0..=i64::from(MAX_UNICODE_CHAR)).contains(&i) {
        return false;
    }
    let code = i as u32;
    let escaped_name = named_escape(code);
    let is_graphic_base = char_general_category(code).is_some_and(graphic_base_p);
    if escaped_name.is_none() && !is_graphic_base {
        return false;
    }

    push_char('?');
    if let Some(name) = escaped_name {
        push_char('\\');
        push_char(name);
    } else {
        // `code` is graphic_base, so it is a valid scalar value.
        let ch = char::from_u32(code).unwrap();
        if escapeflag
            && matches!(
                ch,
                ';' | '"' | '\'' | '\\' | '(' | ')' | '{' | '}' | '[' | ']'
            )
        {
            push_char('\\');
        }
        push_char(ch);
    }
    true
}

impl PrintOptions {
    pub const fn with_print_gensym(print_gensym: bool) -> Self {
        Self {
            print_gensym,
            print_circle: false,
            print_quoted: true,
            print_symbols_bare: false,
            print_escape_newlines: false,
            print_escape_nonascii: false,
            print_escape_multibyte: false,
            print_escape_control_characters: false,
            print_integers_as_characters: false,
            print_level: None,
            print_length: None,
            print_continuous_numbering: false,
            print_number_table: None,
            float_output_format: None,
            print_noescape: false,
            backquote_output_level: 0,
        }
    }

    /// Full constructor for all print options.
    pub fn new(
        print_gensym: bool,
        print_circle: bool,
        print_level: Option<i64>,
        print_length: Option<i64>,
    ) -> Self {
        Self {
            print_gensym,
            print_circle,
            print_quoted: true,
            print_symbols_bare: false,
            print_escape_newlines: false,
            print_escape_nonascii: false,
            print_escape_multibyte: false,
            print_escape_control_characters: false,
            print_integers_as_characters: false,
            print_level,
            print_length,
            print_continuous_numbering: false,
            print_number_table: None,
            float_output_format: None,
            print_noescape: false,
            backquote_output_level: 0,
        }
    }

    pub(crate) fn enter_backquote(self) -> Self {
        Self {
            backquote_output_level: self.backquote_output_level + 1,
            ..self
        }
    }

    pub(crate) fn exit_backquote(self) -> Self {
        Self {
            backquote_output_level: self.backquote_output_level.saturating_sub(1),
            ..self
        }
    }

    pub(crate) fn allow_unquote_shorthand(self) -> bool {
        self.backquote_output_level > 0
    }
}

// ---------------------------------------------------------------------------
// Print-circle state (two-pass algorithm)
// ---------------------------------------------------------------------------

/// State for the print-circle two-pass algorithm.
/// Keys are object identity values (SymId).
pub struct PrintCircleState {
    /// Maps object identity -> label status:
    /// 0 = seen once (removed after pass 1)
    /// negative = assigned label, not yet printed
    /// positive = already printed with this label
    number_table: HashMap<u64, i64>,
    next_label: i64,
}

impl PrintCircleState {
    fn new() -> Self {
        Self {
            number_table: HashMap::new(),
            next_label: 0,
        }
    }
}

thread_local! {
    static PRINT_NUMBER_INDEX: Cell<i64> = const { Cell::new(0) };
    static PRINT_CALL_DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn reset_print_number_index() {
    PRINT_NUMBER_INDEX.with(|index| index.set(0));
}

fn next_print_number_index() -> i64 {
    PRINT_NUMBER_INDEX.with(|index| {
        let next = index.get() + 1;
        index.set(next);
        next
    })
}

/// Combined print state used by the stateful print path.
pub(crate) struct PrintState<'a> {
    pub options: PrintOptions,
    pub circle: Option<&'a mut PrintCircleState>,
    pub buffers: Option<&'a crate::buffer::BufferManager>,
    pub depth: i64,
    default_print_stack: Vec<Option<u64>>,
    object_stack: Vec<u64>,
}

/// Check if a value is a candidate for circle detection.
/// Matches GNU Emacs's `print_circle_candidate_p`.
fn is_print_circle_candidate(value: &Value, print_gensym: bool) -> bool {
    match value.kind() {
        ValueKind::Cons => true,
        ValueKind::Veclike(VecLikeType::Vector) => {
            // GNU's VECTORP excludes bool-vectors (a distinct pseudovector).
            // neomacs builds bool-vectors as tagged plain vectors, so filter
            // them out explicitly here. Non-empty vectors only.
            !super::chartable::is_bool_vector(value)
                && value.as_vector_data().is_some_and(|v| !v.is_empty())
        }
        ValueKind::Veclike(VecLikeType::Record) => true,
        ValueKind::Veclike(VecLikeType::HashTable) => true,
        ValueKind::Veclike(VecLikeType::Obarray) => true,
        ValueKind::Veclike(VecLikeType::CharTable) => true,
        ValueKind::Veclike(VecLikeType::SubCharTable) => true,
        ValueKind::Veclike(VecLikeType::Lambda) => true,
        ValueKind::Veclike(VecLikeType::Macro) => true,
        ValueKind::Veclike(VecLikeType::ByteCode) => true,
        ValueKind::String => string_print_circle_candidate(value),
        ValueKind::Symbol(id) if print_gensym => {
            // Uninterned symbols only
            let name = resolve_sym_lisp_string(id);
            lookup_interned_lisp_string(name) != Some(id)
        }
        _ => false,
    }
}

fn string_print_circle_candidate(value: &Value) -> bool {
    value.as_lisp_string().is_some_and(|s| !s.is_empty())
}

fn is_uninterned_symbol(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Symbol(id) => {
            let name = resolve_sym_lisp_string(id);
            lookup_interned_lisp_string(name) != Some(id)
        }
        _ => false,
    }
}

/// Return a unique identity key for a circle-candidate value.
fn object_identity_key(value: &Value) -> Option<u64> {
    match value.kind() {
        ValueKind::Cons
        | ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::HashTable)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::Veclike(VecLikeType::SubCharTable)
        | ValueKind::String
        | ValueKind::Veclike(VecLikeType::Lambda)
        | ValueKind::Veclike(VecLikeType::Macro)
        | ValueKind::Veclike(VecLikeType::ByteCode) => Some(value.0 as u64),
        ValueKind::Symbol(id) => {
            // Use a distinct namespace to avoid collisions with heap pointer keys.
            // Set the high bit to separate from pointer keys.
            Some((1u64 << 63) | (id.0 as u64))
        }
        _ => None,
    }
}

fn active_print_number_table(options: &PrintOptions) -> Option<Value> {
    if !options.print_continuous_numbering {
        return None;
    }
    options
        .print_number_table
        .filter(|table| table.is_hash_table())
}

pub(crate) struct PrintNumberingGuard;

pub(crate) fn enter_print_call(options: &PrintOptions) -> PrintNumberingGuard {
    PRINT_CALL_DEPTH.with(|depth| {
        let current = depth.get();
        if current == 0
            && (!options.print_continuous_numbering || active_print_number_table(options).is_none())
        {
            reset_print_number_index();
        }
        depth.set(current + 1);
    });
    PrintNumberingGuard
}

impl Drop for PrintNumberingGuard {
    fn drop(&mut self) {
        PRINT_CALL_DEPTH.with(|depth| {
            depth.set(depth.get().saturating_sub(1));
        });
    }
}

fn print_number_table_key(table_value: Value, value: &Value) -> Option<HashKey> {
    let table = table_value.as_hash_table()?;
    Some(value.to_hash_key(&table.test))
}

fn get_print_number_table_entry(table_value: Value, value: &Value) -> Option<(HashKey, Value)> {
    let key = print_number_table_key(table_value, value)?;
    let entry = {
        let table = table_value.as_hash_table()?;
        table.data.get(&key).copied().unwrap_or(Value::NIL)
    };
    Some((key, entry))
}

fn put_print_number_table_entry(
    table_value: Value,
    key: HashKey,
    key_value: Value,
    entry_value: Value,
) {
    let _ = table_value.with_hash_table_mut(|table| {
        table.insert(key, key_value, entry_value);
    });
}

fn remove_print_number_table_t_entries(table_value: Value) {
    let _ = table_value.with_hash_table_mut(|table| {
        // GNU print.c removes all `t` status entries after preprocessing.
        table.data.retain(|_, value| *value != Value::T);
    });
}

fn print_number_entry_is_symbol(entry: Value) -> bool {
    matches!(
        entry.kind(),
        ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_)
    )
}

fn print_number_entry_is_cdr_label(entry: Value) -> bool {
    !entry.is_nil() && entry != Value::T
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrintNumberTableAction {
    PrintedReference,
    PrintedPrefix,
}

fn write_print_number_table_entry(
    value: &Value,
    out: &mut StatefulPrintOutput,
    options: &PrintOptions,
) -> Option<PrintNumberTableAction> {
    if !options.print_circle {
        return None;
    }

    let table_value = active_print_number_table(options)?;
    let (key, entry) = get_print_number_table_entry(table_value, value)?;

    match entry.kind() {
        ValueKind::String => {
            let s = entry.as_lisp_string().unwrap();
            out.extend_from_slice(s.as_bytes());
            Some(PrintNumberTableAction::PrintedReference)
        }
        ValueKind::Fixnum(n) if n < 0 => {
            write!(out, "#{}=", -n).unwrap();
            put_print_number_table_entry(table_value, key, *value, Value::fixnum(-n));
            Some(PrintNumberTableAction::PrintedPrefix)
        }
        ValueKind::Fixnum(n) if n > 0 => {
            write!(out, "#{n}#").unwrap();
            Some(PrintNumberTableAction::PrintedReference)
        }
        _ => None,
    }
}

/// Preprocess pass: walk the value tree to find shared/circular structures.
/// Uses an explicit stack (not recursive) matching GNU Emacs.
fn print_preprocess(value: &Value, state: &mut PrintCircleState, options: PrintOptions) {
    let mut stack: Vec<Value> = vec![*value];
    while let Some(obj) = stack.pop() {
        if !is_print_circle_candidate(&obj, options.print_gensym) {
            continue;
        }
        let key = match object_identity_key(&obj) {
            Some(k) => k,
            None => continue,
        };
        if let Some(status) = state.number_table.get(&key) {
            if *status == 0 {
                // Seen second time -- assign label
                let label = if options.print_continuous_numbering {
                    next_print_number_index()
                } else {
                    state.next_label += 1;
                    state.next_label
                };
                state.number_table.insert(key, -label);
            }
            // Already labeled or already seen multiple times -- skip children
            continue;
        }
        if options.print_continuous_numbering && is_uninterned_symbol(&obj) {
            state.number_table.insert(key, -next_print_number_index());
            continue;
        }
        // First time seen -- mark and process children
        state.number_table.insert(key, 0);
        match obj.kind() {
            ValueKind::Cons => {
                let pair_car = obj.cons_car();
                let pair_cdr = obj.cons_cdr();
                // Push cdr first so car is processed first (stack is LIFO)
                stack.push(pair_cdr);
                stack.push(pair_car);
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = obj.as_vector_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::Record) => {
                let items = obj.as_record_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let table = obj.as_hash_table().unwrap().clone();
                for key_hk in table.live_hash_keys_in_slot_order().into_iter().rev() {
                    if let Some(val) = table.data.get(key_hk) {
                        stack.push(*val);
                        let key_val = super::hashtab::hash_key_to_visible_value(&table, key_hk);
                        stack.push(key_val);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Obarray) => {
                if let Some(obarray) = obj.as_obarray_obj() {
                    for item in obarray.buckets.iter().rev() {
                        stack.push(*item);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
                if let Some(doc) = obj.closure_doc_value() {
                    stack.push(doc);
                }
                if let Some(env) = obj.closure_env().flatten() {
                    stack.push(env);
                }
                if let Some(body) = obj.closure_body_value() {
                    stack.push(body);
                }
                if let Some(params) = obj.closure_params() {
                    stack.push(crate::emacs_core::builtins::lambda_params_to_value(params));
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let _ = with_bytecode_literal_slots(&obj, |slots| {
                    for item in slots.iter().rev() {
                        stack.push(*item);
                    }
                });
            }
            ValueKind::String => push_string_text_property_plists(obj, &mut stack),
            _ => {}
        }
    }
    // Remove entries seen only once
    state.number_table.retain(|_, v| *v != 0);
}

fn print_preprocess_external(value: &Value, table_value: Value, options: PrintOptions) {
    print_preprocess_external_with_t_removal(value, table_value, options, true);
}

/// Core of GNU `print_preprocess`. When `remove_t_entries` is true the
/// neomacs printer path strips the transient `t` status entries afterwards
/// (they are cosmetic — the printer only acts on fixnum/string labels). The
/// `print--preprocess` primitive passes `false` so the user-visible
/// `print-number-table` keeps every traversed candidate, exactly like GNU.
fn print_preprocess_external_with_t_removal(
    value: &Value,
    table_value: Value,
    options: PrintOptions,
    remove_t_entries: bool,
) {
    let mut stack: Vec<Value> = vec![*value];
    while let Some(obj) = stack.pop() {
        if !is_print_circle_candidate(&obj, options.print_gensym) {
            continue;
        }
        let Some((key, entry)) = get_print_number_table_entry(table_value, &obj) else {
            continue;
        };
        if !entry.is_nil() || (options.print_continuous_numbering && is_uninterned_symbol(&obj)) {
            if print_number_entry_is_symbol(entry) {
                let label = next_print_number_index();
                put_print_number_table_entry(table_value, key, obj, Value::fixnum(-label));
            }
            continue;
        }

        put_print_number_table_entry(table_value, key, obj, Value::T);
        match obj.kind() {
            ValueKind::Cons => {
                let pair_car = obj.cons_car();
                let pair_cdr = obj.cons_cdr();
                if !pair_cdr.is_nil() {
                    stack.push(pair_cdr);
                }
                stack.push(pair_car);
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = obj.as_vector_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::Record) => {
                let items = obj.as_record_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let table = obj.as_hash_table().unwrap().clone();
                for key_hk in table.live_hash_keys_in_slot_order().into_iter().rev() {
                    if let Some(val) = table.data.get(key_hk) {
                        stack.push(*val);
                        let key_val = super::hashtab::hash_key_to_visible_value(&table, key_hk);
                        stack.push(key_val);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Obarray) => {
                if let Some(obarray) = obj.as_obarray_obj() {
                    for item in obarray.buckets.iter().rev() {
                        stack.push(*item);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
                if let Some(doc) = obj.closure_doc_value() {
                    stack.push(doc);
                }
                if let Some(env) = obj.closure_env().flatten() {
                    stack.push(env);
                }
                if let Some(body) = obj.closure_body_value() {
                    stack.push(body);
                }
                if let Some(params) = obj.closure_params() {
                    stack.push(crate::emacs_core::builtins::lambda_params_to_value(params));
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let _ = with_bytecode_literal_slots(&obj, |slots| {
                    for item in slots.iter().rev() {
                        stack.push(*item);
                    }
                });
            }
            ValueKind::String => push_string_text_property_plists(obj, &mut stack),
            _ => {}
        }
    }

    if remove_t_entries {
        remove_print_number_table_t_entries(table_value);
    }
}

fn push_string_text_property_plists(value: Value, stack: &mut Vec<Value>) {
    if let Some(runs) = get_string_text_properties_for_value(value) {
        for run in runs.into_iter().rev() {
            stack.push(run.plist);
        }
    }
}

/// Public preprocessing entry point used by the `print--preprocess` builtin.
///
/// Mirrors GNU `print_preprocess` (print.c): resets the shared-structure
/// number index and traverses `value`, filling `table_value` (the Lisp
/// `print-number-table` hash) with sharing info so that circular and shared
/// structures can be printed with `#N=` / `#N#` labels.
///
/// `print_gensym` and `print_continuous_numbering` come from the like-named
/// dynamic variables; they affect which symbols are treated as candidates and
/// the gensym special case, exactly as in GNU.
pub(crate) fn preprocess_print_number_table(
    value: &Value,
    table_value: Value,
    print_gensym: bool,
    print_continuous_numbering: bool,
) {
    reset_print_number_index();
    let options = PrintOptions {
        print_circle: true,
        print_gensym,
        print_continuous_numbering,
        ..PrintOptions::default()
    };
    // GNU's `print--preprocess` leaves the transient `t` status entries in the
    // table (the printer simply ignores non-fixnum entries), so don't strip
    // them here.
    print_preprocess_external_with_t_removal(value, table_value, options, false);
}

/// Entry point for stateful printing (circle/level/length aware).
/// Returns the printed representation as a String.
pub(crate) fn print_value_stateful(value: &Value, options: PrintOptions) -> String {
    print_value_stateful_with_buffers(value, None, options)
}

pub(crate) fn print_value_stateful_with_buffers(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
    options: PrintOptions,
) -> String {
    String::from_utf8_lossy(&print_value_stateful_bytes_with_buffers(
        value, buffers, options,
    ))
    .into_owned()
}

pub(crate) fn print_value_stateful_bytes_with_buffers(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
    options: PrintOptions,
) -> Vec<u8> {
    let _print_guard = enter_print_call(&options);
    let mut out = StatefulPrintOutput::default();
    let number_table = active_print_number_table(&options);
    if options.print_circle {
        let mut circle = PrintCircleState::new();
        if let Some(table_value) = number_table {
            print_preprocess_external(value, table_value, options);
        } else {
            print_preprocess(value, &mut circle, options);
        }
        let mut state = PrintState {
            options,
            circle: if number_table.is_some() {
                None
            } else {
                Some(&mut circle)
            },
            buffers,
            depth: 0,
            default_print_stack: Vec::new(),
            object_stack: Vec::new(),
        };
        write_value_stateful(value, &mut out, &mut state);
    } else {
        let mut state = PrintState {
            options,
            circle: None,
            buffers,
            depth: 0,
            default_print_stack: Vec::new(),
            object_stack: Vec::new(),
        };
        write_value_stateful(value, &mut out, &mut state);
    }
    out.into_bytes()
}

pub(crate) fn default_cycle_candidate_key(value: &Value) -> Option<u64> {
    match value.kind() {
        ValueKind::Cons => object_identity_key(value),
        ValueKind::Veclike(VecLikeType::Vector) => value
            .as_vector_data()
            .is_some_and(|items| !items.is_empty())
            .then(|| object_identity_key(value))
            .flatten(),
        ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::HashTable)
        | ValueKind::Veclike(VecLikeType::Lambda)
        | ValueKind::Veclike(VecLikeType::Macro)
        | ValueKind::Veclike(VecLikeType::ByteCode) => object_identity_key(value),
        ValueKind::String if string_print_circle_candidate(value) => object_identity_key(value),
        _ => None,
    }
}

fn default_cycle_stack_index(value: &Value, state: &PrintState) -> Option<usize> {
    if state.circle.is_some() {
        return None;
    }
    let key = default_cycle_candidate_key(value)?;
    state.object_stack.iter().position(|entry| *entry == key)
}

fn push_default_cycle_object(value: &Value, state: &mut PrintState) -> bool {
    if state.circle.is_some() {
        return false;
    }
    let Some(key) = default_cycle_candidate_key(value) else {
        return false;
    };
    state.object_stack.push(key);
    true
}

fn with_default_cycle_guard(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
    render: impl FnOnce(&mut StatefulPrintOutput, &mut PrintState),
) {
    if !state.options.print_circle {
        render(out, state);
        return;
    }

    if let Some(index) = default_cycle_stack_index(value, state) {
        write!(out, "#{index}").unwrap();
        return;
    }
    let pushed = push_default_cycle_object(value, state);
    render(out, state);
    if pushed {
        state.object_stack.pop();
    }
}

/// Core stateful print routine. Writes the printed representation of `value`
/// into `out`, respecting print-circle, print-level, and print-length.
fn write_value_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    if !state.options.print_circle {
        let key = default_cycle_candidate_key(value);
        if let Some(key) = key
            && let Some(index) = state
                .default_print_stack
                .iter()
                .position(|entry| *entry == Some(key))
        {
            write!(out, "#{index}").unwrap();
            return;
        }

        state.default_print_stack.push(key);
        write_value_stateful_inner(value, out, state);
        state.default_print_stack.pop();
        return;
    }

    write_value_stateful_inner(value, out, state);
}

fn write_value_stateful_inner(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    if let Some(handle) = print_special_handle(value, state.buffers) {
        out.push_str(&handle);
        return;
    }

    // Circle check: if this object is a circle candidate, handle #N= / #N#
    if is_print_circle_candidate(value, state.options.print_gensym) {
        let mut printed_number_table_prefix = false;
        match write_print_number_table_entry(value, out, &state.options) {
            Some(PrintNumberTableAction::PrintedReference) => return,
            Some(PrintNumberTableAction::PrintedPrefix) => {
                printed_number_table_prefix = true;
            }
            None => {}
        }

        if !printed_number_table_prefix
            && let Some(ref mut circle) = state.circle
            && let Some(key) = object_identity_key(value)
            && let Some(label) = circle.number_table.get_mut(&key)
        {
            if *label < 0 {
                // First occurrence: emit #N= prefix
                write!(out, "#{}=", -(*label)).unwrap();
                *label = -(*label); // flip to positive
            } else if *label > 0 {
                // Subsequent: emit #N# and return
                write!(out, "#{}#", *label).unwrap();
                return;
            }
            // label == 0: not shared, fall through to normal print
        }
    }

    match value.kind() {
        ValueKind::Nil => out.push_str("nil"),
        ValueKind::T => out.push('t'),
        ValueKind::Fixnum(v) => {
            if !(state.options.print_integers_as_characters
                && try_format_integer_as_character(v, !state.options.print_noescape, &mut |c| {
                    out.push(c)
                }))
            {
                write!(out, "{}", v).unwrap();
            }
        }
        ValueKind::Float => out.push_str(&format_float_with_options(value.xfloat(), state.options)),
        ValueKind::Symbol(id) => out.extend_from_slice(&symbol_bytes(id, state.options)),
        ValueKind::String => {
            with_default_cycle_guard(value, out, state, |out, state| {
                let ls = value.as_lisp_string().unwrap();
                if state.options.print_noescape {
                    out.extend_from_slice(ls.as_bytes());
                } else {
                    match get_string_text_properties_for_value(*value) {
                        Some(runs) => write_lisp_propertized_string_stateful(ls, &runs, out, state),
                        None => out
                            .extend_from_slice(&format_lisp_string_bytes_emacs(ls, &state.options)),
                    }
                }
            });
        }
        ValueKind::Cons => {
            with_default_cycle_guard(value, out, state, |out, state| {
                // Level check for containers
                if let Some(level) = state.options.print_level
                    && state.depth >= level
                {
                    out.push_str("...");
                    return;
                }
                // Try shorthand (quote, function, backquote, etc.)
                if let Some(shorthand) = write_list_shorthand_stateful(value, state) {
                    out.append(shorthand);
                    return;
                }
                state.depth += 1;
                out.push('(');
                write_cons_stateful(value, out, state);
                out.push(')');
                state.depth -= 1;
            });
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            if let Some(slots) = char_table_external_slots(value) {
                with_default_cycle_guard(value, out, state, |out, state| {
                    state.depth += 1;
                    out.push_str("#^[");
                    for (idx, item) in slots.iter().enumerate() {
                        if idx > 0 {
                            out.push(' ');
                        }
                        write_value_stateful(item, out, state);
                    }
                    out.push(']');
                    state.depth -= 1;
                });
            }
        }
        ValueKind::Veclike(VecLikeType::SubCharTable) => {
            if let Some((depth, min_char, slots)) =
                super::chartable::sub_char_table_external_slots(value)
            {
                with_default_cycle_guard(value, out, state, |out, state| {
                    state.depth += 1;
                    out.push_str("#^^[");
                    out.push_str(&depth.to_string());
                    out.push(' ');
                    out.push_str(&min_char.to_string());
                    for item in &slots {
                        out.push(' ');
                        write_value_stateful(item, out, state);
                    }
                    out.push(']');
                    state.depth -= 1;
                });
            }
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if let Some(nbits) = bool_vector_length(value) {
                out.push_str(&format_bool_vector(value, nbits as usize, state.options));
                return;
            }
            if let Some(slots) = char_table_external_slots(value) {
                with_default_cycle_guard(value, out, state, |out, state| {
                    state.depth += 1;
                    out.push_str("#^[");
                    for (idx, item) in slots.iter().enumerate() {
                        if let Some(length) = state.options.print_length
                            && idx as i64 >= length
                        {
                            if idx > 0 {
                                out.push(' ');
                            }
                            out.push_str("...");
                            break;
                        }
                        if idx > 0 {
                            out.push(' ');
                        }
                        write_value_stateful(item, out, state);
                    }
                    out.push(']');
                    state.depth -= 1;
                });
                return;
            }
            if let Some((depth, min_char, slots)) =
                super::chartable::sub_char_table_external_slots(value)
            {
                with_default_cycle_guard(value, out, state, |out, state| {
                    state.depth += 1;
                    out.push_str("#^^[");
                    out.push_str(&depth.to_string());
                    out.push(' ');
                    out.push_str(&min_char.to_string());
                    for item in &slots {
                        out.push(' ');
                        write_value_stateful(item, out, state);
                    }
                    out.push(']');
                    state.depth -= 1;
                });
                return;
            }
            with_default_cycle_guard(value, out, state, |out, state| {
                state.depth += 1;
                out.push('[');
                let items = value.as_vector_data().unwrap().clone();
                for (idx, item) in items.iter().enumerate() {
                    if let Some(length) = state.options.print_length
                        && idx as i64 >= length
                    {
                        if idx > 0 {
                            out.push(' ');
                        }
                        out.push_str("...");
                        break;
                    }
                    if idx > 0 {
                        out.push(' ');
                    }
                    write_value_stateful(item, out, state);
                }
                out.push(']');
                state.depth -= 1;
            });
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            with_default_cycle_guard(value, out, state, |out, state| {
                state.depth += 1;
                out.push_str("#s(");
                let items = value.as_record_data().unwrap().clone();
                for (idx, item) in items.iter().enumerate() {
                    if let Some(length) = state.options.print_length
                        && idx as i64 >= length
                    {
                        if idx > 0 {
                            out.push(' ');
                        }
                        out.push_str("...");
                        break;
                    }
                    if idx > 0 {
                        out.push(' ');
                    }
                    write_value_stateful(item, out, state);
                }
                out.push(')');
                state.depth -= 1;
            });
        }
        ValueKind::Veclike(VecLikeType::HashTable) => {
            with_default_cycle_guard(value, out, state, |out, state| {
                state.depth += 1;
                write_hash_table_stateful(value, out, state);
                state.depth -= 1;
            });
        }
        ValueKind::Veclike(VecLikeType::Obarray) => {
            let count = value.as_obarray_obj().map_or(0, |obj| obj.count);
            out.push_str(&format!("#<obarray n={count}>"));
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            write_lambda_stateful(value, out, state);
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            write_macro_stateful(value, out, state);
        }
        ValueKind::Subr(id) => write!(out, "#<subr {}>", resolve_sym(id)).unwrap(),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = value.as_subr_id().unwrap();
            write!(out, "#<subr {}>", resolve_sym(id)).unwrap()
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            write_bytecode_literal_stateful(value, out, state);
        }
        ValueKind::Veclike(VecLikeType::Marker) => out.push_str(
            &print_special_handle(value, state.buffers).unwrap_or_else(|| "#<marker>".to_string()),
        ),
        ValueKind::Veclike(VecLikeType::Overlay) => out.push_str(
            &print_special_handle(value, state.buffers).unwrap_or_else(|| "#<overlay>".to_string()),
        ),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let bid = value.as_buffer_id().unwrap();
            if let Some(buffers) = state.buffers {
                if let Some(buf) = buffers.get(bid) {
                    write!(out, "#<buffer {}>", buf.name_runtime_string_owned()).unwrap();
                } else if buffers.dead_buffer_last_name_value(bid).is_some() {
                    out.push_str("#<killed buffer>");
                } else {
                    write!(out, "#<buffer {}>", bid.0).unwrap();
                }
            } else {
                write!(out, "#<buffer {}>", bid.0).unwrap();
            }
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            let wid = value.as_window_id().unwrap();
            write!(out, "#<window {}>", wid).unwrap();
        }
        ValueKind::Veclike(VecLikeType::WindowConfiguration) => {
            out.push_str("#<window-configuration>");
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            let fid = value.as_frame_id().unwrap();
            out.push_str(&format_frame_handle(fid));
        }
        ValueKind::Veclike(VecLikeType::Timer) => {
            let tid = value.as_timer_id().unwrap();
            write!(out, "#<timer {}>", tid).unwrap();
        }
        ValueKind::Veclike(VecLikeType::Process) => {
            out.push_str(
                &super::process::print_process_handle(value)
                    .unwrap_or_else(|| "#<process>".to_string()),
            );
        }
        ValueKind::Veclike(VecLikeType::Terminal) => {
            out.push_str(
                &print_special_handle(value, state.buffers)
                    .unwrap_or_else(|| "#<terminal>".to_string()),
            );
        }
        ValueKind::Veclike(VecLikeType::Xwidget) => {
            let xw = value.as_xwidget().unwrap();
            write!(out, "#<xwidget {}>", xw.xwidget_id).unwrap();
        }
        ValueKind::Veclike(VecLikeType::XwidgetView) => {
            out.push_str("#<xwidget-view>");
        }
        ValueKind::Veclike(VecLikeType::SurfaceHandle) => {
            let id = value.as_surface_handle().unwrap();
            write!(out, "#<neomacs-surface {id}>").unwrap();
        }
        ValueKind::Veclike(VecLikeType::Bignum) => {
            // GNU `print_object` formats bignums via `mpz_get_str`
            // (`src/print.c` PRINT_INTEGER branch). `malachite::Integer`'s
            // Display implements the same formatting.
            write!(out, "{}", value.as_bignum().unwrap()).unwrap();
        }
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
            write_symbol_with_pos_stateful(value, out, state);
        }
        ValueKind::Veclike(VecLikeType::Finalizer) => {
            // GNU `print_vectorlike_unreadable` prints finalizers opaquely.
            out.push_str("#<finalizer>");
        }
        ValueKind::Veclike(VecLikeType::Sqlite) => {
            let obj = value.as_sqlite().unwrap();
            if obj.is_statement {
                write!(out, "#<sqlite statement {}>", obj.id).unwrap();
            } else {
                write!(out, "#<sqlite db {}>", obj.id).unwrap();
            }
        }
        ValueKind::Veclike(VecLikeType::UserPtr) => {
            out.push_str("#<user-ptr>");
        }
        ValueKind::Veclike(VecLikeType::ModuleFunction) => {
            out.push_str("#<module-function>");
        }
        ValueKind::Veclike(VecLikeType::Font) => out.push_str("#<font-object>"),
        ValueKind::Unbound => out.push_str("#<unbound>"),
        ValueKind::Unknown => write!(out, "#<unknown {:#x}>", value.0).unwrap(),
    }
}

fn with_bytecode_literal_slots<R>(value: &Value, f: impl FnOnce(&[Value]) -> R) -> Option<R> {
    let bc = value.get_bytecode_data()?.clone();
    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();

    let arglist = bc.arglist;
    crate::emacs_core::eval::push_scratch_gc_root(arglist);

    let code = if let Some(bytes) = &bc.gnu_bytecode_bytes {
        Value::heap_string(crate::heap_types::LispString::from_unibyte(
            bytes.as_slice().to_vec(),
        ))
    } else {
        Value::NIL
    };
    crate::emacs_core::eval::push_scratch_gc_root(code);

    let constants = if let Some(env) = bc.env {
        env
    } else {
        Value::vector(bc.constants.as_slice().to_vec())
    };
    crate::emacs_core::eval::push_scratch_gc_root(constants);

    let depth = Value::fixnum(bc.max_stack as i64);
    let doc = bc
        .doc_form
        .or_else(|| bc.docstring.as_ref().map(|d| Value::heap_string(d.clone())))
        .unwrap_or(Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(doc);

    let interactive = bc.interactive.unwrap_or(Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(interactive);

    let slot_count = bc.observable_closure_slot_count();
    let mut slots = vec![arglist, code, constants, depth];
    if slot_count > 4 {
        slots.push(doc);
    }
    if slot_count > 5 {
        slots.push(interactive);
    }
    if slot_count > 6 {
        let extra_count = slot_count - 6;
        for idx in 0..extra_count {
            slots.push(bc.extra_slots.get(idx).copied().unwrap_or(Value::NIL));
        }
    }
    let result = f(&slots);
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
    Some(result)
}

/// Public wrapper over [`with_bytecode_literal_slots`] so other modules (the
/// princ byte printer in `misc_eval`) can iterate a byte-code object's literal
/// slots — `[arglist, code, constants, depth, doc?, interactive?, …]` — in the
/// same order the `#[…]` printer uses. Returns `None` for non-byte-code values.
pub(crate) fn with_bytecode_literal_slots_public<R>(
    value: &Value,
    f: impl FnOnce(&[Value]) -> R,
) -> Option<R> {
    with_bytecode_literal_slots(value, f)
}

fn write_bytecode_literal_stateful(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    with_default_cycle_guard(value, out, state, |out, state| {
        let _ = with_bytecode_literal_slots(value, |slots| {
            state.depth += 1;
            out.push_str("#[");
            for (idx, item) in slots.iter().enumerate() {
                if let Some(length) = state.options.print_length
                    && idx as i64 >= length
                {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str("...");
                    break;
                }
                if idx > 0 {
                    out.push(' ');
                }
                write_value_stateful(item, out, state);
            }
            out.push(']');
            state.depth -= 1;
        });
    });
}

fn write_closure_body_forms_stateful(
    body: Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let Some(forms) = list_to_vec(&body) else {
        write_value_stateful(&body, out, state);
        return;
    };
    if forms.is_empty() {
        out.push_str("nil");
    } else {
        for (idx, form) in forms.iter().enumerate() {
            if idx > 0 {
                out.push(' ');
            }
            write_value_stateful(form, out, state);
        }
    }
}

fn write_interpreted_closure_stateful(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    out.push_str("#[");
    if let Some(slots) = value.closure_slots() {
        for (idx, item) in slots.iter().enumerate() {
            if idx > 0 {
                out.push(' ');
            }
            write_value_stateful(item, out, state);
        }
    }
    out.push(']');
}

fn write_lambda_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    with_lambda_print_guard(value, out, state, |out, state| {
        with_default_cycle_guard(value, out, state, |out, state| {
            write_interpreted_closure_stateful(value, out, state);
        });
    });
}

fn write_macro_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    with_default_cycle_guard(value, out, state, |out, state| {
        out.push_str("(macro ");
        write_params_stateful(value.closure_params(), out, state);
        out.push(' ');
        if let Some(body) = value.closure_body_value() {
            write_closure_body_forms_stateful(body, out, state);
        } else {
            out.push_str("nil");
        }
        out.push(')');
    });
}

fn append_bytecode_literal_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    if with_bytecode_literal_slots(value, |slots| {
        out.extend_from_slice(b"#[");
        for (idx, item) in slots.iter().enumerate() {
            if idx > 0 {
                out.push(b' ');
            }
            append_print_value_bytes(item, out, options);
        }
        out.push(b']');
    })
    .is_none()
    {
        out.extend_from_slice(b"#<bytecode>");
    }
}

#[inline]
fn symbol_id_is(id: SymId, name: &str) -> bool {
    id == intern(name)
}

#[inline]
fn value_is_symbol_named(value: &Value, name: &str) -> bool {
    matches!(value.kind(), ValueKind::Symbol(id) if symbol_id_is(id, name))
}

#[derive(Clone, Copy)]
enum SymbolShorthand {
    Quote,
    Function,
    Backquote,
    Comma,
    CommaAt,
}

fn symbol_shorthand(id: SymId) -> Option<SymbolShorthand> {
    if symbol_id_is(id, "quote") {
        Some(SymbolShorthand::Quote)
    } else if symbol_id_is(id, "function") {
        Some(SymbolShorthand::Function)
    } else if symbol_id_is(id, "`") {
        Some(SymbolShorthand::Backquote)
    } else if symbol_id_is(id, ",") {
        Some(SymbolShorthand::Comma)
    } else if symbol_id_is(id, ",@") {
        Some(SymbolShorthand::CommaAt)
    } else {
        None
    }
}

/// Try to produce a shorthand form (quote, function, backquote, etc.) using
/// stateful printing. Returns `Some(string)` on success.
fn write_list_shorthand_stateful(
    value: &Value,
    state: &mut PrintState,
) -> Option<StatefulPrintOutput> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    let head = match items[0].kind() {
        ValueKind::Symbol(id) => id,
        _ => return None,
    };

    if symbol_id_is(head, "make-hash-table-from-literal") {
        if let Some(payload) = quote_payload_stateful(&items[1]) {
            let mut out = StatefulPrintOutput::from_str("#s");
            write_value_stateful(&payload, &mut out, state);
            return Some(out);
        }
        return None;
    }

    let (prefix, nested_options) = match symbol_shorthand(head)? {
        SymbolShorthand::Quote => ("'", state.options),
        SymbolShorthand::Function => ("#'", state.options),
        SymbolShorthand::Backquote => ("`", state.options.enter_backquote()),
        SymbolShorthand::Comma => {
            if !state.options.allow_unquote_shorthand() {
                return None;
            }
            (",", state.options.exit_backquote())
        }
        SymbolShorthand::CommaAt => {
            if !state.options.allow_unquote_shorthand() {
                return None;
            }
            (",@", state.options.exit_backquote())
        }
    };

    let saved_options = state.options;
    state.options = nested_options;
    let mut out = StatefulPrintOutput::from_str(prefix);
    write_value_stateful(&items[1], &mut out, state);
    state.options = saved_options;
    Some(out)
}

fn quote_payload_stateful(value: &Value) -> Option<Value> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }
    value_is_symbol_named(&items[0], "quote").then_some(items[1])
}

/// Print a cons cell (list elements) with stateful print support.
fn write_cons_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = state.options.print_length.unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut n: i64 = 2;
    let mut m: i64 = 2;
    let mut tortoise_idx: i64 = 0;
    let stack_len = state.object_stack.len();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if first {
                    if maxlen == 0 {
                        out.push_str("...");
                        state.object_stack.truncate(stack_len);
                        return;
                    }
                } else {
                    out.push(' ');

                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.push_str("...");
                        state.object_stack.truncate(stack_len);
                        return;
                    }

                    if state.circle.is_none() {
                        n -= 1;
                        if n == 0 {
                            tortoise_idx = tortoise_idx.saturating_add(m);
                            m = m.saturating_mul(2);
                            n = m;
                            tortoise = cursor;
                        } else if cursor == tortoise {
                            write!(out, ". #{tortoise_idx}").unwrap();
                            state.object_stack.truncate(stack_len);
                            return;
                        }
                    }
                }
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();

                // Circle check on the cdr (for detecting shared tails)
                // But first, print the car
                write_value_stateful(&pair_car, out, state);
                cursor = pair_cdr;
                first = false;

                // Check if cdr is a cons that has a circle label
                if cursor.is_cons() {
                    if let Some(table_value) = active_print_number_table(&state.options) {
                        if let Some((_key, entry)) =
                            get_print_number_table_entry(table_value, &cursor)
                            && print_number_entry_is_cdr_label(entry)
                        {
                            out.push_str(" . ");
                            write_value_stateful(&cursor, out, state);
                            return;
                        }
                    } else if let Some(ref circle) = state.circle
                        && let Some(key) = object_identity_key(&cursor)
                        && let Some(label) = circle.number_table.get(&key)
                        && *label != 0
                    {
                        // This cons is shared/circular -- print as dotted pair
                        out.push_str(" . ");
                        write_value_stateful(&cursor, out, state);
                        return;
                    }
                }
            }
            ValueKind::Nil => {
                state.object_stack.truncate(stack_len);
                return;
            }
            _ => {
                if !first {
                    out.push_str(" . ");
                }
                write_value_stateful(&cursor, out, state);
                state.object_stack.truncate(stack_len);
                return;
            }
        }
    }
}

/// Print a hash table with stateful support.
fn write_hash_table_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    let table = value.as_hash_table().unwrap().clone();
    out.push_str("#s(hash-table");

    append_hash_table_test_string(&table, out);

    if let Some(ref weakness) = table.weakness {
        out.push_str(" weakness ");
        out.push_str(weakness.name());
    }

    if !table.data.is_empty() {
        out.push_str(" data (");
        let mut first = true;
        let mut count: i64 = 0;
        for key in table.live_hash_keys_in_slot_order() {
            if let Some(val) = table.data.get(key) {
                if let Some(length) = state.options.print_length
                    && count >= length
                {
                    if !first {
                        out.push(' ');
                    }
                    out.push_str("...");
                    break;
                }
                if !first {
                    out.push(' ');
                }
                let key_val = super::hashtab::hash_key_to_visible_value(&table, key);
                write_value_stateful(&key_val, out, state);
                out.push(' ');
                write_value_stateful(val, out, state);
                first = false;
                count += 1;
            }
        }
        out.push(')');
    }

    out.push(')');
}

thread_local! {
    static PRINT_OBJECT_STACK: RefCell<Vec<PrintObjectRef>> = const { RefCell::new(Vec::new()) };
    static PRINT_BYTES_OBJECT_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrintObjectRef {
    Lambda(usize),
}

fn with_print_object_guard<R>(
    object: PrintObjectRef,
    on_cycle: impl FnOnce(usize) -> R,
    render: impl FnOnce() -> R,
) -> R {
    PRINT_OBJECT_STACK.with(|stack| {
        if let Some(index) = stack.borrow().iter().position(|entry| *entry == object) {
            return on_cycle(index);
        }

        stack.borrow_mut().push(object);
        let rendered = render();
        stack.borrow_mut().pop();
        rendered
    })
}

fn with_lambda_print_guard(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
    render: impl FnOnce(&mut StatefulPrintOutput, &mut PrintState),
) {
    let object = PrintObjectRef::Lambda(value.0);
    PRINT_OBJECT_STACK.with(|stack| {
        if let Some(index) = stack.borrow().iter().position(|entry| *entry == object) {
            write!(out, "#{index}").unwrap();
            return;
        }

        stack.borrow_mut().push(object);
        render(out, state);
        stack.borrow_mut().pop();
    });
}

fn append_bytes_cycle_ref_if_any(value: &Value, out: &mut Vec<u8>) -> bool {
    let Some(key) = default_cycle_candidate_key(value) else {
        return false;
    };
    PRINT_BYTES_OBJECT_STACK.with(|stack| {
        if let Some(index) = stack.borrow().iter().position(|entry| *entry == key) {
            out.extend_from_slice(format!("#{index}").as_bytes());
            true
        } else {
            false
        }
    })
}

fn push_bytes_cycle_object(value: &Value) -> bool {
    let Some(key) = default_cycle_candidate_key(value) else {
        return false;
    };
    PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow_mut().push(key));
    true
}

fn pop_bytes_cycle_object(pushed: bool) {
    if pushed {
        PRINT_BYTES_OBJECT_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn bytes_object_stack_len() -> usize {
    PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow().len())
}

fn truncate_bytes_object_stack(len: usize) {
    PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow_mut().truncate(len));
}

fn bytes_cycle_stack_index(value: &Value) -> Option<usize> {
    let key = default_cycle_candidate_key(value)?;
    PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow().iter().position(|entry| *entry == key))
}

fn format_marker_handle(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
) -> Option<String> {
    if !super::marker::is_marker(value) {
        return None;
    }

    if !value.is_marker() {
        return None;
    };
    let marker = value.as_marker_data().unwrap().clone();
    let buffer_name = marker
        .buffer
        .and_then(|buffer_id| buffers.and_then(|manager| manager.get(buffer_id)))
        .map(|buffer| buffer.name_runtime_string_owned());

    let mut out = String::from("#<marker ");
    if marker.insertion_type {
        out.push_str("(moves after insertion) ");
    }
    // T7: read authoritative charpos (1-based Lisp shape). A marker with
    // no buffer prints as "in no buffer"; otherwise include its current
    // chain-tracked position.
    if let Some(name) = buffer_name.as_deref() {
        out.push_str(&format!("at {} in {}", marker.charpos + 1, name));
    } else {
        out.push_str("in no buffer");
    }
    out.push('>');
    Some(out)
}

fn format_overlay_handle(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
) -> Option<String> {
    if !value.is_overlay() {
        return None;
    };

    let overlay = value.as_overlay_data().unwrap();
    let Some(buffer_id) = overlay.buffer else {
        return Some("#<overlay in no buffer>".to_string());
    };
    let (start, end) = overlay.current_range();

    let Some(buffers) = buffers else {
        return Some(format!("#<overlay from {} to {}>", start, end));
    };

    let Some(buffer) = buffers.get(buffer_id) else {
        return Some("#<overlay in no buffer>".to_string());
    };

    Some(format!(
        "#<overlay from {} to {} in {}>",
        buffer
            .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(start))
            .as_i64(),
        buffer
            .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(end))
            .as_i64(),
        buffer.name_runtime_string_owned()
    ))
}

fn print_special_handle(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
) -> Option<String> {
    super::terminal::pure::print_terminal_handle(value)
        .or_else(|| format_marker_handle(value, buffers))
        .or_else(|| format_overlay_handle(value, buffers))
}

fn format_frame_handle(id: u64) -> String {
    if id >= crate::window::FRAME_ID_BASE {
        let ordinal = id - crate::window::FRAME_ID_BASE + 1;
        format!("#<frame F{} 0x{:x}>", ordinal, id)
    } else {
        format!("#<frame {}>", id)
    }
}

fn write_lisp_propertized_string_stateful(
    ls: &crate::heap_types::LispString,
    runs: &[StringTextPropertyRun],
    out: &mut StatefulPrintOutput,
    state: &mut PrintState<'_>,
) {
    out.push_str("#(");
    out.extend_from_slice(&format_lisp_string_bytes_emacs(ls, &state.options));
    for run in runs {
        out.push(' ');
        out.push_str(&run.start.to_string());
        out.push(' ');
        out.push_str(&run.end.to_string());
        out.push(' ');
        write_value_stateful(&run.plist, out, state);
    }
    out.push(')');
}

/// Print a `Value` as a Lisp string, with buffer-manager awareness for
/// proper buffer name / killed-buffer rendering.
pub fn print_value_with_buffers(value: &Value, buffers: &crate::buffer::BufferManager) -> String {
    // String result -> escape unibyte high bytes, consistent with `print_value`
    // (GNU `prin1-to-string` into a multibyte buffer); see `print_value`.
    let options = PrintOptions {
        print_escape_nonascii: true,
        ..PrintOptions::default()
    };
    print_value_with_buffers_and_options(value, buffers, options)
}

pub fn print_value_with_buffers_and_options(
    value: &Value,
    buffers: &crate::buffer::BufferManager,
    options: PrintOptions,
) -> String {
    print_value_stateful_with_buffers(value, Some(buffers), options)
}

/// Print a `Value` as a Lisp string.
pub fn print_value(value: &Value) -> String {
    // GNU renders a value to a String via `prin1-to-string`, which prints into a
    // multibyte buffer; `print_prepare' (print.c:170-177) then binds
    // `print-escape-nonascii' to t, so a unibyte string's raw 0x80..0xFF bytes come
    // out octal-escaped (`\NNN`) instead of raw. A String result must escape them
    // regardless: a raw eight-bit byte does not round-trip through UTF-8 (it
    // lossily becomes U+FFFD). The byte sink `print_value_bytes` deliberately stays
    // raw -- it mirrors `prin1' to stdout / a buffer, where the destination governs
    // the encoding.
    let options = PrintOptions {
        print_escape_nonascii: true,
        ..PrintOptions::default()
    };
    print_value_with_options(value, options)
}

pub fn print_value_with_options(value: &Value, options: PrintOptions) -> String {
    print_value_stateful(value, options)
}

/// Print a `Value` as a Lisp byte sequence.
///
/// This preserves non-UTF-8 byte payloads encoded via NeoVM string sentinels.
pub fn print_value_bytes(value: &Value) -> Vec<u8> {
    print_value_bytes_with_options(value, PrintOptions::default())
}

pub fn print_value_bytes_with_options(value: &Value, options: PrintOptions) -> Vec<u8> {
    let _print_guard = enter_print_call(&options);
    // Delegate to the stateful printer when circle/level/length are active.
    if options.print_circle || options.print_level.is_some() || options.print_length.is_some() {
        return print_value_stateful_bytes_with_buffers(value, None, options);
    }
    let mut out = Vec::new();
    append_print_value_bytes(value, &mut out, options);
    out
}

fn append_print_value_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    if let Some(handle) = print_special_handle(value, None) {
        out.extend_from_slice(handle.as_bytes());
        return;
    }
    match value.kind() {
        ValueKind::Nil => out.extend_from_slice(b"nil"),
        ValueKind::T => out.extend_from_slice(b"t"),
        ValueKind::Fixnum(v) => {
            let mut buf = String::new();
            if options.print_integers_as_characters
                && try_format_integer_as_character(v, !options.print_noescape, &mut |c| buf.push(c))
            {
                out.extend_from_slice(buf.as_bytes());
            } else {
                out.extend_from_slice(v.to_string().as_bytes());
            }
        }
        ValueKind::Float => {
            out.extend_from_slice(format_float_with_options(value.xfloat(), options).as_bytes())
        }
        ValueKind::Symbol(id) => append_symbol_bytes(id, out, options),
        ValueKind::String => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            let ls = value.as_lisp_string().unwrap();
            if options.print_noescape {
                out.extend_from_slice(ls.as_bytes());
            } else {
                let str_bytes = format_lisp_string_bytes_emacs(ls, &options);
                if let Some(runs) = get_string_text_properties_for_value(*value) {
                    out.extend_from_slice(b"#(");
                    out.extend_from_slice(&str_bytes);
                    for run in runs {
                        out.push(b' ');
                        out.extend_from_slice(run.start.to_string().as_bytes());
                        out.push(b' ');
                        out.extend_from_slice(run.end.to_string().as_bytes());
                        out.push(b' ');
                        append_print_value_bytes(&run.plist, out, options);
                    }
                    out.push(b')');
                } else {
                    out.extend_from_slice(&str_bytes);
                }
            }
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Cons => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            if let Some(shorthand) = print_list_shorthand_bytes(value, options) {
                out.extend_from_slice(&shorthand);
                pop_bytes_cycle_object(pushed);
                return;
            }
            out.push(b'(');
            print_cons_bytes(value, out, options);
            out.push(b')');
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            if let Some(slots) = char_table_external_slots(value) {
                out.extend_from_slice(b"#^[");
                for (idx, item) in slots.iter().enumerate() {
                    if idx > 0 {
                        out.push(b' ');
                    }
                    append_print_value_bytes(item, out, options);
                }
                out.push(b']');
            }
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::SubCharTable) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            if let Some((depth, min_char, slots)) =
                super::chartable::sub_char_table_external_slots(value)
            {
                out.extend_from_slice(b"#^^[");
                out.extend_from_slice(depth.to_string().as_bytes());
                out.push(b' ');
                out.extend_from_slice(min_char.to_string().as_bytes());
                for item in &slots {
                    out.push(b' ');
                    append_print_value_bytes(item, out, options);
                }
                out.push(b']');
            }
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if let Some(nbits) = bool_vector_length(value) {
                append_bool_vector_bytes(value, nbits as usize, out, options);
                return;
            }
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            if let Some(slots) = char_table_external_slots(value) {
                out.extend_from_slice(b"#^[");
                for (idx, item) in slots.iter().enumerate() {
                    if idx > 0 {
                        out.push(b' ');
                    }
                    append_print_value_bytes(item, out, options);
                }
                out.push(b']');
                pop_bytes_cycle_object(pushed);
                return;
            }
            if let Some((depth, min_char, slots)) =
                super::chartable::sub_char_table_external_slots(value)
            {
                out.extend_from_slice(b"#^^[");
                out.extend_from_slice(depth.to_string().as_bytes());
                out.push(b' ');
                out.extend_from_slice(min_char.to_string().as_bytes());
                for item in &slots {
                    out.push(b' ');
                    append_print_value_bytes(item, out, options);
                }
                out.push(b']');
                pop_bytes_cycle_object(pushed);
                return;
            }
            out.push(b'[');
            let items = value.as_vector_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(b' ');
                }
                append_print_value_bytes(item, out, options);
            }
            out.push(b']');
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            out.extend_from_slice(b"#s(");
            let items = value.as_record_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(b' ');
                }
                append_print_value_bytes(item, out, options);
            }
            out.push(b')');
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::HashTable) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            append_hash_table_bytes(value, out, options);
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::Obarray) => {
            let count = value.as_obarray_obj().map_or(0, |obj| obj.count);
            out.extend_from_slice(format!("#<obarray n={count}>").as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            let text = with_print_object_guard(
                PrintObjectRef::Lambda(value.0),
                |index| format!("#{index}"),
                || format_interpreted_closure(value, options),
            );
            out.extend_from_slice(text.as_bytes());
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            let params = value.closure_params().map_or_else(
                || "nil".to_string(),
                |params| format_params(params, options),
            );
            let body = value
                .closure_body_value()
                .map(|body| format_closure_body_forms(body, options))
                .unwrap_or_else(|| "nil".to_string());
            out.extend_from_slice(format!("(macro {} {})", params, body).as_bytes());
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Subr(id) => {
            out.extend_from_slice(format!("#<subr {}>", resolve_sym(id)).as_bytes())
        }
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = value.as_subr_id().unwrap();
            out.extend_from_slice(format!("#<subr {}>", resolve_sym(id)).as_bytes())
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let pushed = push_bytes_cycle_object(value);
            append_bytecode_literal_bytes(value, out, options);
            pop_bytes_cycle_object(pushed);
        }
        ValueKind::Veclike(VecLikeType::Marker) => out.extend_from_slice(
            print_special_handle(value, None)
                .unwrap_or_else(|| "#<marker>".to_string())
                .as_bytes(),
        ),
        ValueKind::Veclike(VecLikeType::Overlay) => out.extend_from_slice(
            print_special_handle(value, None)
                .unwrap_or_else(|| "#<overlay>".to_string())
                .as_bytes(),
        ),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            out.extend_from_slice(
                format!("#<buffer {}>", value.as_buffer_id().unwrap().0).as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            out.extend_from_slice(
                format!("#<window {}>", value.as_window_id().unwrap()).as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::WindowConfiguration) => {
            out.extend_from_slice(b"#<window-configuration>");
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            out.extend_from_slice(format_frame_handle(value.as_frame_id().unwrap()).as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Timer) => {
            out.extend_from_slice(format!("#<timer {}>", value.as_timer_id().unwrap()).as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Process) => {
            out.extend_from_slice(
                super::process::print_process_handle(value)
                    .unwrap_or_else(|| "#<process>".to_string())
                    .as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Terminal) => {
            out.extend_from_slice(
                print_special_handle(value, None)
                    .unwrap_or_else(|| "#<terminal>".to_string())
                    .as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Xwidget) => {
            let xw = value.as_xwidget().unwrap();
            out.extend_from_slice(format!("#<xwidget {}>", xw.xwidget_id).as_bytes());
        }
        ValueKind::Veclike(VecLikeType::XwidgetView) => {
            out.extend_from_slice(b"#<xwidget-view>");
        }
        ValueKind::Veclike(VecLikeType::SurfaceHandle) => {
            let id = value.as_surface_handle().unwrap();
            out.extend_from_slice(format!("#<neomacs-surface {id}>").as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Bignum) => {
            out.extend_from_slice(value.as_bignum().unwrap().to_string().as_bytes());
        }
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
            append_symbol_with_pos_bytes(value, out, options);
        }
        ValueKind::Veclike(VecLikeType::Finalizer) => {
            out.extend_from_slice(b"#<finalizer>");
        }
        ValueKind::Veclike(VecLikeType::Sqlite) => {
            let obj = value.as_sqlite().unwrap();
            if obj.is_statement {
                out.extend_from_slice(format!("#<sqlite statement {}>", obj.id).as_bytes());
            } else {
                out.extend_from_slice(format!("#<sqlite db {}>", obj.id).as_bytes());
            }
        }
        ValueKind::Veclike(VecLikeType::UserPtr) => {
            out.extend_from_slice(b"#<user-ptr>");
        }
        ValueKind::Veclike(VecLikeType::ModuleFunction) => {
            out.extend_from_slice(b"#<module-function>");
        }
        ValueKind::Veclike(VecLikeType::Font) => {
            out.extend_from_slice(b"#<font-object>");
        }
        ValueKind::Unbound => {
            out.extend_from_slice(b"#<unbound>");
        }
        ValueKind::Unknown => {
            out.extend_from_slice(format!("#<unknown {:#x}>", value.0).as_bytes());
        }
    }
}

fn append_symbol_bytes(id: super::intern::SymId, out: &mut Vec<u8>, options: PrintOptions) {
    out.extend_from_slice(&symbol_bytes(id, options));
}

fn write_symbol_with_pos_stateful(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let Some(swp) = value.as_symbol_with_pos() else {
        out.push_str("#<symbol-with-pos>");
        return;
    };

    let sym = swp.sym;
    if state.options.print_symbols_bare {
        write_value_stateful(&sym, out, state);
        return;
    }

    out.push_str("#<symbol ");
    if sym.is_symbol() {
        write_value_stateful(&sym, out, state);
    } else {
        out.push_str("NOT A SYMBOL!!");
    }
    if let Some(pos) = swp.pos.as_fixnum() {
        write!(out, " at {pos}").unwrap();
    } else {
        out.push_str(" NOT A POSITION!!");
    }
    out.push('>');
}

fn append_symbol_with_pos_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    let Some(swp) = value.as_symbol_with_pos() else {
        out.extend_from_slice(b"#<symbol-with-pos>");
        return;
    };

    let sym = swp.sym;
    if options.print_symbols_bare {
        append_print_value_bytes(&sym, out, options);
        return;
    }

    out.extend_from_slice(b"#<symbol ");
    if sym.is_symbol() {
        append_print_value_bytes(&sym, out, options);
    } else {
        out.extend_from_slice(b"NOT A SYMBOL!!");
    }
    if let Some(pos) = swp.pos.as_fixnum() {
        out.extend_from_slice(format!(" at {pos}").as_bytes());
    } else {
        out.extend_from_slice(b" NOT A POSITION!!");
    }
    out.push(b'>');
}

fn symbol_bytes(id: super::intern::SymId, options: PrintOptions) -> Vec<u8> {
    // Identity is decided on the name ATOM, the printed spelling on the name
    // OBJECT: GNU prints the string the symbol was created from, which Lisp can
    // have mutated in place since.
    let canonical = lookup_interned_lisp_string(resolve_sym_lisp_string(id));
    let visible_name = super::intern::resolve_lisp_visible_symbol_name(id);
    let name = visible_name.text();
    let mut out = Vec::new();
    if canonical == Some(id) {
        append_symbol_name_bytes_with_escape(name, &mut out, !options.print_noescape);
    } else if options.print_gensym {
        out.extend_from_slice(b"#:");
        if !name.is_empty() {
            append_symbol_name_bytes_with_escape(name, &mut out, !options.print_noescape);
        }
    } else {
        append_symbol_name_bytes_with_escape(name, &mut out, !options.print_noescape);
    }
    out
}

fn append_symbol_name_bytes_with_escape(
    name: &crate::heap_types::LispString,
    out: &mut Vec<u8>,
    escape: bool,
) {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        out.extend_from_slice(b"##");
        return;
    }

    let mut confusing = symbol_name_confusing(bytes);
    for byte in bytes.iter().copied() {
        let needs_escape = matches!(
            byte,
            b'(' | b')' | b'[' | b']' | b'"' | b'\\' | b';' | b'#' | b'\'' | b'`' | b','
        ) || byte <= b' '
            || confusing;
        if escape && needs_escape {
            out.push(b'\\');
            confusing = false;
        }
        if !name.is_multibyte() && byte >= 0x80 {
            let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            let len = crate::emacs_core::emacs_char::char_string(
                crate::emacs_core::emacs_char::unibyte_to_char(byte),
                &mut buf,
            );
            out.extend_from_slice(&buf[..len]);
        } else {
            out.push(byte);
        }
    }
}

pub(crate) fn format_float(f: f64) -> String {
    const NAN_QUIET_BIT: u64 = 1u64 << 51;
    const NAN_PAYLOAD_MASK: u64 = (1u64 << 51) - 1;

    if f.is_nan() {
        let bits = f.to_bits();
        let frac = bits & ((1u64 << 52) - 1);
        if (frac & NAN_QUIET_BIT) != 0 {
            let payload = frac & NAN_PAYLOAD_MASK;
            return if f.is_sign_negative() {
                format!("-{}.0e+NaN", payload)
            } else {
                format!("{}.0e+NaN", payload)
            };
        }
        return if f.is_sign_negative() {
            "-0.0e+NaN".to_string()
        } else {
            "0.0e+NaN".to_string()
        };
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "1.0e+INF".to_string()
        } else {
            "-1.0e+INF".to_string()
        };
    }
    format_float_dtoastr(f)
}

pub(crate) fn format_float_with_options(f: f64, options: PrintOptions) -> String {
    format_float_with_output_format(f, options.float_output_format)
}

pub(crate) fn format_float_with_output_format(f: f64, output_format: Option<Value>) -> String {
    if f.is_nan() || f.is_infinite() {
        return format_float(f);
    }

    let Some((precision, spec, width)) = parse_float_output_format(output_format) else {
        return format_float(f);
    };

    let Some(mut text) = format_float_printf(precision, spec, f) else {
        return format_float(f);
    };
    finish_float_output_format(&mut text, width);
    text
}

fn parse_float_output_format(value: Option<Value>) -> Option<(usize, u8, i32)> {
    let value = value?;
    let bytes = value.as_lisp_string()?.as_bytes();
    let nul = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let format = &bytes[..nul];
    if format.len() < 3 || format[0] != b'%' || format[1] != b'.' {
        return None;
    }

    let mut index = 2;
    let mut width = -1;
    if format.get(index).is_some_and(u8::is_ascii_digit) {
        width = 0;
        while let Some(byte) = format.get(index).filter(|byte| byte.is_ascii_digit()) {
            width = width * 10 + i32::from(byte - b'0');
            if width > f64::DIGITS as i32 {
                return None;
            }
            index += 1;
        }
        if width == 0 && format.get(index) != Some(&b'f') {
            return None;
        }
    }

    let spec = *format.get(index)?;
    if !matches!(spec, b'e' | b'f' | b'g') {
        return None;
    }
    index += 1;
    if index != format.len() {
        return None;
    }

    let precision = if width >= 0 { width as usize } else { 0 };
    Some((precision, spec, width))
}

fn format_float_printf(precision: usize, spec: u8, f: f64) -> Option<String> {
    match spec {
        b'e' => Some(normalize_float_exponent(&format!(
            "{:.prec$e}",
            f,
            prec = precision
        ))),
        b'f' => Some(format!("{:.prec$}", f, prec = precision)),
        b'g' => {
            let exp_form = normalize_float_exponent(&format!("{:.prec$e}", f, prec = precision));
            let fix_form = format!("{:.prec$}", f, prec = precision);
            let chosen = if exp_form.len() <= fix_form.len() {
                &exp_form
            } else {
                &fix_form
            };
            let trimmed = chosen.trim_end_matches('0');
            let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
            Some(trimmed.to_string())
        }
        _ => None,
    }
}

fn normalize_float_exponent(s: &str) -> String {
    let Some((mantissa, exponent)) = s.split_once('e') else {
        return s.to_string();
    };

    let exp = exponent.parse::<i32>().unwrap_or(0);
    if exp >= 0 {
        format!("{mantissa}e+{exp:02}")
    } else {
        format!("{mantissa}e-{abs:02}", abs = -exp)
    }
}

fn finish_float_output_format(text: &mut String, width: i32) {
    if width == 0 {
        return;
    }

    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'-') {
        index += 1;
    }

    if bytes.get(index) == Some(&b'.') && index + 1 == bytes.len() {
        text.push('0');
    } else if index == bytes.len() {
        text.push_str(".0");
    }
}

/// Format a finite float matching GNU Emacs's `dtoastr` / `float_to_string`:
/// use `%g`-style formatting with the minimum precision (starting from DBL_DIG=15)
/// that round-trips through strtod, then ensure a decimal point or exponent is present.
fn format_float_dtoastr(f: f64) -> String {
    let abs_f = f.abs();
    let start_prec = if abs_f != 0.0 && abs_f < f64::MIN_POSITIVE {
        1
    } else {
        15 // DBL_DIG
    };
    for prec in start_prec..=20 {
        // %g: uses %e if exponent < -4 or >= precision, otherwise %f.
        // %g also trims trailing zeros.
        let s = format!("{:.prec$e}", f, prec = prec - 1);
        // Parse back and check round-trip
        if let Ok(parsed) = s.parse::<f64>()
            && parsed.to_bits() == f.to_bits()
        {
            // Convert from Rust's scientific notation to %g-style output
            return rust_sci_to_emacs_g(f, &s, prec);
        }
    }
    // Fallback: maximum precision
    let s = format!("{:.20e}", f);
    rust_sci_to_emacs_g(f, &s, 21)
}

/// Convert Rust scientific notation string to GNU Emacs %g-style output.
/// %g rules: use fixed notation unless exponent >= precision or exponent < -4.
/// %g trims trailing zeros (but keeps at least one digit after decimal point
/// for Emacs's post-processing).
fn rust_sci_to_emacs_g(f: f64, sci: &str, prec: usize) -> String {
    // Parse the exponent from Rust's scientific notation (e.g., "3.14e2")
    let (mantissa_str, exp_str) = sci.split_once('e').unwrap_or((sci, "0"));
    let exp: i32 = exp_str.parse().unwrap_or(0);

    // %g uses fixed notation when -4 <= exp < prec
    let result = if exp >= -4 && exp < prec as i32 {
        // Fixed notation
        format_g_fixed(f, mantissa_str, exp, prec)
    } else {
        // Scientific notation with Emacs-style exponent formatting
        format_g_scientific(mantissa_str, exp, prec)
    };

    // Emacs post-processing: ensure decimal point or exponent is present
    ensure_decimal_point(result)
}

/// Format as fixed-point notation for %g, trimming trailing zeros.
fn format_g_fixed(f: f64, _mantissa: &str, exp: i32, prec: usize) -> String {
    // %g precision = total significant digits.
    // digits_after_dot = prec - exp - 1 (works for both positive and negative exp)
    let digits_after_dot = (prec as i32 - exp - 1).max(0) as usize;
    let s = format!("{:.digits$}", f, digits = digits_after_dot);
    trim_trailing_zeros_g(&s)
}

/// Format as scientific notation for %g, trimming trailing zeros.
fn format_g_scientific(mantissa: &str, exp: i32, _prec: usize) -> String {
    // Trim trailing zeros from mantissa
    let trimmed = trim_trailing_zeros_g(mantissa);
    // Emacs uses e+XX / e-XX with at least 2-digit exponent for |exp| < 100,
    // but %g in glibc actually uses minimal digits. Let's match C's %g.
    if exp >= 0 {
        format!("{}e+{:02}", trimmed, exp)
    } else {
        format!("{}e-{:02}", trimmed, -exp)
    }
}

/// Trim trailing zeros after decimal point (%g style).
/// "3.1400" -> "3.14", "3.0000" -> "3", "100" -> "100"
fn trim_trailing_zeros_g(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    trimmed.to_string()
}

/// Ensure the output has a decimal point with trailing digit (Emacs requirement).
/// If no decimal point or exponent, append ".0".
fn ensure_decimal_point(mut s: String) -> String {
    // Check if there's already a decimal point or exponent
    let has_dot_or_exp = s.bytes().any(|b| b == b'.' || b == b'e' || b == b'E');
    if !has_dot_or_exp {
        s.push_str(".0");
    } else if s.ends_with('.') {
        s.push('0');
    }
    s
}

fn params_value(params: Option<&super::value::LambdaParams>) -> Value {
    params
        .map(crate::emacs_core::builtins::lambda_params_to_value)
        .unwrap_or(Value::NIL)
}

fn write_params_stateful(
    params: Option<&super::value::LambdaParams>,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let value = params_value(params);
    write_value_stateful(&value, out, state);
}

fn format_params(params: &super::value::LambdaParams, options: PrintOptions) -> String {
    let value = crate::emacs_core::builtins::lambda_params_to_value(params);
    print_value_with_options(&value, options)
}

fn format_closure_body_forms(body: Value, options: PrintOptions) -> String {
    let Some(forms) = list_to_vec(&body) else {
        return print_value_with_options(&body, options);
    };
    if forms.is_empty() {
        "nil".to_string()
    } else {
        forms
            .iter()
            .map(|form| print_value_with_options(form, options))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn format_interpreted_closure(value: &Value, options: PrintOptions) -> String {
    value
        .closure_slots()
        .map(|slots| {
            let parts = slots
                .iter()
                .map(|slot| print_value_with_options(slot, options))
                .collect::<Vec<_>>();
            format!("#[{}]", parts.join(" "))
        })
        .unwrap_or_else(|| "#[]".to_string())
}

fn print_list_shorthand_bytes(value: &Value, options: PrintOptions) -> Option<Vec<u8>> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    let head = match items[0].kind() {
        ValueKind::Symbol(id) => id,
        _ => return None,
    };

    if symbol_id_is(head, "make-hash-table-from-literal") {
        let payload = quote_payload(&items[1])?;
        let mut out = Vec::new();
        out.extend_from_slice(b"#s");
        append_print_value_bytes(&payload, &mut out, options);
        return Some(out);
    }

    let shorthand = symbol_shorthand(head)?;
    if !options.print_quoted {
        return None;
    }

    let (prefix, nested_options): (&[u8], PrintOptions) = match shorthand {
        SymbolShorthand::Quote => (b"'", options),
        SymbolShorthand::Function => (b"#'", options),
        SymbolShorthand::Backquote => (b"`", options.enter_backquote()),
        SymbolShorthand::Comma => {
            if !options.allow_unquote_shorthand() {
                return None;
            }
            (b",", options.exit_backquote())
        }
        SymbolShorthand::CommaAt => {
            if !options.allow_unquote_shorthand() {
                return None;
            }
            (b",@", options.exit_backquote())
        }
    };

    let mut out = Vec::new();
    out.extend_from_slice(prefix);
    append_print_value_bytes(&items[1], &mut out, nested_options);
    Some(out)
}

fn quote_payload(value: &Value) -> Option<Value> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }
    value_is_symbol_named(&items[0], "quote").then_some(items[1])
}

fn print_cons_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    let mut cursor = *value;
    let mut first = true;
    let stack_len = bytes_object_stack_len();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if !first {
                    if let Some(index) = bytes_cycle_stack_index(&cursor) {
                        out.extend_from_slice(b" . ");
                        out.extend_from_slice(format!("#{index}").as_bytes());
                        truncate_bytes_object_stack(stack_len);
                        return;
                    }
                    push_bytes_cycle_object(&cursor);
                }
                if !first {
                    out.push(b' ');
                }
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                append_print_value_bytes(&pair_car, out, options);
                cursor = pair_cdr;
                first = false;
            }
            ValueKind::Nil => {
                truncate_bytes_object_stack(stack_len);
                return;
            }
            _ => {
                if !first {
                    out.extend_from_slice(b" . ");
                }
                append_print_value_bytes(&cursor, out, options);
                truncate_bytes_object_stack(stack_len);
                return;
            }
        }
    }
}
// -- Bool-vector printing ---------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoolVectorByteSyntax {
    Literal,
    Quoted,
    NamedEscape(u8),
    Octal,
}

fn bool_vector_byte_syntax(byte: u8, options: PrintOptions) -> BoolVectorByteSyntax {
    match byte {
        b'\n' if options.print_escape_newlines => BoolVectorByteSyntax::NamedEscape(b'n'),
        b'\x0c' if options.print_escape_newlines => BoolVectorByteSyntax::NamedEscape(b'f'),
        b if b > 0x7f || (options.print_escape_control_characters && (b < 0x20 || b == 0x7f)) => {
            BoolVectorByteSyntax::Octal
        }
        b'"' | b'\\' => BoolVectorByteSyntax::Quoted,
        _ => BoolVectorByteSyntax::Literal,
    }
}

fn bool_vector_packed_bytes(value: &Value, nbits: usize) -> Vec<u8> {
    let items = match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => value.as_vector_data().unwrap().clone(),
        _ => return Vec::new(),
    };

    (0..nbits.div_ceil(8))
        .map(|byte_idx| {
            let mut byte = 0_u8;
            for bit_idx in 0..8 {
                let overall_bit = byte_idx * 8 + bit_idx;
                if overall_bit >= nbits {
                    break;
                }
                let is_set = match items.get(2 + overall_bit) {
                    Some(value) => match value.kind() {
                        ValueKind::Fixnum(n) => n != 0,
                        _ => value.is_truthy(),
                    },
                    None => false,
                };
                if is_set {
                    byte |= 1 << bit_idx;
                }
            }
            byte
        })
        .collect()
}

/// Format a bool-vector as `#&N"..."`.
fn format_bool_vector(value: &Value, nbits: usize, options: PrintOptions) -> String {
    let mut out = Vec::new();
    append_bool_vector_bytes(value, nbits, &mut out, options);
    String::from_utf8_lossy(&out).into_owned()
}

/// Append bool-vector bytes as `#&N"..."`.
fn append_bool_vector_bytes(value: &Value, nbits: usize, out: &mut Vec<u8>, options: PrintOptions) {
    let packed = bool_vector_packed_bytes(value, nbits);
    out.extend_from_slice(b"#&");
    out.extend_from_slice(nbits.to_string().as_bytes());
    out.push(b'"');

    let printed_len = options
        .print_length
        .and_then(|length| usize::try_from(length).ok())
        .map_or(packed.len(), |length| length.min(packed.len()));
    let printed = &packed[..printed_len];

    for (index, &byte) in printed.iter().enumerate() {
        match bool_vector_byte_syntax(byte, options) {
            BoolVectorByteSyntax::Literal => out.push(byte),
            BoolVectorByteSyntax::Quoted => {
                out.push(b'\\');
                out.push(byte);
            }
            BoolVectorByteSyntax::NamedEscape(name) => {
                out.push(b'\\');
                out.push(name);
            }
            BoolVectorByteSyntax::Octal => {
                super::string_escape::push_octal_escape_contextual_u32(
                    out,
                    byte,
                    printed.get(index + 1).map(|next| u32::from(*next)),
                );
            }
        }
    }

    if printed_len < packed.len() {
        out.extend_from_slice(b" ...");
    }
    out.push(b'"');
}

// -- Hash-table printing ----------------------------------------------------

fn append_hash_table_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    let table = value.as_hash_table().unwrap().clone();
    out.extend_from_slice(b"#s(hash-table");

    append_hash_table_test_bytes(&table, out);

    if let Some(ref weakness) = table.weakness {
        out.extend_from_slice(b" weakness ");
        out.extend_from_slice(weakness.name().as_bytes());
    }

    if !table.data.is_empty() {
        out.extend_from_slice(b" data (");
        let mut first = true;
        for key in table.live_hash_keys_in_slot_order() {
            if let Some(val) = table.data.get(key) {
                if !first {
                    out.push(b' ');
                }
                let key_val = super::hashtab::hash_key_to_visible_value(&table, key);
                append_print_value_bytes(&key_val, out, options);
                out.push(b' ');
                append_print_value_bytes(val, out, options);
                first = false;
            }
        }
        out.push(b')');
    }

    out.push(b')');
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
