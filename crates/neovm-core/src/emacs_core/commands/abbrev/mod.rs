//! Abbreviation system -- text abbreviation expansion.
//!
//! Implements GNU Emacs-compatible obarray-based abbrev tables:
//! - An abbrev table is an obarray (vector) with a special empty-string symbol
//!   storing table properties on its plist.
//! - `abbrev-table-p` checks for a numeric `:abbrev-table-modiff` property on
//!   that header symbol.
//! - `define-abbrev` interns symbols into the obarray with expansion as
//!   `symbol-value` and hook as `symbol-function`.
//! - `abbrev-symbol` / `abbrev-expansion` look up symbols in the obarray.
//! - `clear-abbrev-table` resets all buckets and preserves the header symbol's
//!   plist.
//! - `abbrev-table-get` / `abbrev-table-put` access the header symbol's plist.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_min_args};
use std::collections::HashMap;

use super::error::{EvalResult, Flow, signal};
use super::intern::{SymId, intern, intern_uninterned, resolve_sym};
use super::value::{Value, ValueKind, list_to_vec};
use crate::buffer::{EmacsBytePos, TextExtent};
use crate::heap_types::LispString;
use strum::EnumString;

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString)]
#[strum(serialize_all = "kebab-case")]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
enum AbbrevSystemKeyword {
    Force,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
enum AbbrevSystemSetting {
    User,
    System,
    Force,
}

impl AbbrevSystemSetting {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn from_system_flag(flag: Value) -> Self {
        if flag.is_nil() {
            return Self::User;
        }
        match flag.as_symbol_name().and_then(|name| name.parse().ok()) {
            Some(AbbrevSystemKeyword::Force) => Self::Force,
            None => Self::System,
        }
    }

    fn is_system(self) -> bool {
        matches!(self, Self::System | Self::Force)
    }

    fn allows_user_abbrev_overwrite(self) -> bool {
        matches!(self, Self::Force)
    }

    fn stored_property_value(self, flag: Value) -> Value {
        match self {
            Self::Force => Value::T,
            Self::User | Self::System => flag,
        }
    }

    fn marks_abbrevs_changed(self) -> bool {
        matches!(self, Self::User)
    }
}

// ---------------------------------------------------------------------------
// AbbrevManager -- kept for backward compat (eval.rs, pdump)
// ---------------------------------------------------------------------------

/// A single abbreviation entry (kept for pdump compatibility).
#[derive(Clone, Debug)]
pub struct Abbrev {
    pub expansion: LispString,
    pub hook: Option<LispString>,
    pub count: usize,
    pub system: bool,
}

/// A named table of abbreviations (kept for pdump compatibility).
#[derive(Clone, Debug)]
pub struct AbbrevTable {
    pub name: LispString,
    pub abbrevs: HashMap<LispString, Abbrev>,
    pub parent: Option<LispString>,
    pub case_fixed: bool,
    pub enable_quoting: bool,
}

impl AbbrevTable {
    fn new(_name: &str) -> Self {
        Self {
            name: runtime_string_to_abbrev_string(_name),
            abbrevs: HashMap::new(),
            parent: None,
            case_fixed: false,
            enable_quoting: false,
        }
    }
}

/// Central registry -- now only holds the abbrev-mode flag.
/// The tables HashMap is kept for pdump compatibility but no longer used by builtins.
#[derive(Clone, Debug)]
pub struct AbbrevManager {
    tables: HashMap<SymId, AbbrevTable>,
    global_table_sym: SymId,
    abbrev_mode: bool,
}

impl Default for AbbrevManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AbbrevManager {
    pub fn new() -> Self {
        let global_name = runtime_string_to_abbrev_string("global-abbrev-table");
        let global_sym = abbrev_table_sym(&abbrev_string_to_runtime(&global_name));
        let mut tables = HashMap::new();
        tables.insert(
            global_sym,
            AbbrevTable::new(&abbrev_string_to_runtime(&global_name)),
        );
        Self {
            tables,
            global_table_sym: global_sym,
            abbrev_mode: false,
        }
    }

    pub fn define_abbrev(&mut self, table: &str, abbrev: &str, expansion: &str) {
        let table_sym = abbrev_table_sym(table);
        let tbl = self
            .tables
            .entry(table_sym)
            .or_insert_with(|| AbbrevTable::new(table));
        let key = runtime_string_to_abbrev_string(&abbrev.to_lowercase());
        tbl.abbrevs.insert(
            key,
            Abbrev {
                expansion: runtime_string_to_abbrev_string(expansion),
                hook: None,
                count: 0,
                system: false,
            },
        );
    }

    pub fn define_abbrev_full(
        &mut self,
        table: &str,
        abbrev: &str,
        expansion: &str,
        hook: Option<LispString>,
        system: bool,
    ) {
        let table_sym = abbrev_table_sym(table);
        let tbl = self
            .tables
            .entry(table_sym)
            .or_insert_with(|| AbbrevTable::new(table));
        let key = runtime_string_to_abbrev_string(&abbrev.to_lowercase());
        tbl.abbrevs.insert(
            key,
            Abbrev {
                expansion: runtime_string_to_abbrev_string(expansion),
                hook,
                count: 0,
                system,
            },
        );
    }

    pub fn expand_abbrev(&mut self, table: &str, word: &str) -> Option<String> {
        self.expand_abbrev_by_sym(abbrev_table_sym(table), word)
    }

    fn expand_abbrev_by_sym(&mut self, table_sym: SymId, word: &str) -> Option<String> {
        let key = runtime_string_to_abbrev_string(&word.to_lowercase());
        if let Some(tbl) = self.tables.get_mut(&table_sym)
            && let Some(ab) = tbl.abbrevs.get_mut(&key)
        {
            ab.count += 1;
            let expansion = apply_case(
                &abbrev_string_to_runtime(&ab.expansion),
                word,
                tbl.case_fixed,
            );
            return Some(expansion);
        }
        let parent = self.tables.get(&table_sym).and_then(|t| t.parent.clone());
        if let Some(parent_name) = parent {
            return self.expand_abbrev_by_sym(abbrev_table_sym_from_lisp(&parent_name), word);
        }
        if table_sym != self.global_table_sym {
            return self.expand_abbrev_by_sym(self.global_table_sym, word);
        }
        None
    }

    pub fn create_table(&mut self, name: &str) -> &mut AbbrevTable {
        let table_sym = abbrev_table_sym(name);
        self.tables
            .entry(table_sym)
            .or_insert_with(|| AbbrevTable::new(name))
    }

    pub fn get_table(&self, name: &str) -> Option<&AbbrevTable> {
        self.tables.get(&abbrev_table_sym(name))
    }

    pub fn list_abbrevs(&self, table: &str) -> Vec<(String, String)> {
        match self.tables.get(&abbrev_table_sym(table)) {
            Some(tbl) => {
                let mut entries: Vec<(String, String)> = tbl
                    .abbrevs
                    .iter()
                    .map(|(k, v)| {
                        (
                            abbrev_string_to_runtime(k),
                            abbrev_string_to_runtime(&v.expansion),
                        )
                    })
                    .collect();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                entries
            }
            None => Vec::new(),
        }
    }

    pub fn clear_table(&mut self, table: &str) {
        if let Some(tbl) = self.tables.get_mut(&abbrev_table_sym(table)) {
            tbl.abbrevs.clear();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.abbrev_mode
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.abbrev_mode = enabled;
    }

    pub fn global_table_name(&self) -> LispString {
        runtime_string_to_abbrev_string(resolve_sym(self.global_table_sym))
    }

    pub fn all_table_names(&self) -> Vec<LispString> {
        let mut names: Vec<LispString> = self
            .tables
            .values()
            .map(|table| table.name.clone())
            .collect();
        names.sort_by_key(abbrev_string_to_runtime);
        names
    }

    // pdump accessors
    pub(crate) fn dump_tables(&self) -> &HashMap<SymId, AbbrevTable> {
        &self.tables
    }
    pub(crate) fn dump_global_table_sym(&self) -> SymId {
        self.global_table_sym
    }
    pub(crate) fn dump_abbrev_mode(&self) -> bool {
        self.abbrev_mode
    }
    pub(crate) fn from_dump(
        tables: HashMap<SymId, AbbrevTable>,
        global_table_sym: SymId,
        abbrev_mode: bool,
    ) -> Self {
        Self {
            tables,
            global_table_sym,
            abbrev_mode,
        }
    }
}

fn runtime_string_to_abbrev_string(text: &str) -> LispString {
    super::builtins::plain_str_to_lisp_string(text, true)
}

fn abbrev_table_sym(text: &str) -> SymId {
    intern(text)
}

fn abbrev_table_sym_from_lisp(text: &LispString) -> SymId {
    // Issue #131: intern the symbol-name key from the exact Emacs-byte
    // LispString instead of routing through a lossy/storage Rust string, so the
    // abbrev-table key stays byte-faithful (no eight-bit collision). For ASCII
    // and pure-Unicode names this yields the same SymId as the `&str` path.
    super::intern::intern_lisp_string(text)
}

fn abbrev_string_to_runtime(text: &LispString) -> String {
    // Issue #131: remaining consumers are display/expansion/sort text reached
    // through `&str` APIs; a lossy UTF-8 rendering keeps real Unicode (incl.
    // PUA) while dropping the buggy storage-string sentinels.
    crate::emacs_core::emacs_char::to_utf8_lossy(text.as_bytes())
}

// ---------------------------------------------------------------------------
// Case handling
// ---------------------------------------------------------------------------

fn apply_case(expansion: &str, word: &str, case_fixed: bool) -> String {
    if case_fixed || word.is_empty() || expansion.is_empty() {
        return expansion.to_string();
    }
    let all_upper = word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
    let first_upper = word
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    if all_upper && word.chars().any(|c| c.is_alphabetic()) {
        expansion.to_uppercase()
    } else if first_upper {
        let mut chars = expansion.chars();
        match chars.next() {
            Some(first) => {
                let mut result = first.to_uppercase().to_string();
                result.extend(chars);
                result
            }
            None => expansion.to_string(),
        }
    } else {
        expansion.to_string()
    }
}

// ===========================================================================
// Obarray helpers (shared with builtins/symbols.rs logic)
// ===========================================================================

/// Default obarray size for abbrev tables (same default as `obarray-make`).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const ABBREV_TABLE_DEFAULT_SIZE: usize = 1511;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const ABBREV_TABLE_HEADER_NAME: &str = "";

/// Hash a string for obarray bucket lookup.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_hash(s: &str, len: usize) -> usize {
    let hash = s
        .bytes()
        .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
    hash as usize % len
}

/// Search a bucket chain (cons list) for a symbol with the given name.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_bucket_find(bucket: Value, name: &str) -> Option<Value> {
    let mut current = bucket;
    loop {
        match current.kind() {
            ValueKind::Nil => return None,
            ValueKind::Cons => {
                let car = current.cons_car();
                let cdr = current.cons_cdr();
                if let Some(sym_name) = car.as_symbol_name()
                    && sym_name == name
                {
                    return Some(car);
                }
                current = cdr;
            }
            _ => return None,
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn symbol_id(value: Value) -> Option<SymId> {
    match value.kind() {
        ValueKind::Symbol(id) => Some(id),
        ValueKind::Nil => Some(intern("nil")),
        ValueKind::T => Some(intern("t")),
        _ => None,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_insert_symbol(vec_val: Value, sym: Value) {
    let Some(name) = sym.as_symbol_name() else {
        return;
    };
    let vec_len = super::builtins::symbols::obarray_len(vec_val).unwrap_or(0);
    if vec_len == 0 {
        return;
    }
    let bucket_idx = obarray_hash(name, vec_len);
    let bucket =
        super::builtins::symbols::obarray_bucket(vec_val, bucket_idx).unwrap_or(Value::NIL);
    let new_bucket = Value::cons(sym, bucket);
    let _ = super::builtins::symbols::set_obarray_bucket(vec_val, bucket_idx, new_bucket);
    super::builtins::symbols::note_obarray_symbol_added(vec_val);
}

/// Intern a symbol into a custom obarray (vector). Returns the symbol Value.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_intern(vec_val: Value, name: &str) -> Value {
    let vec_len = super::builtins::symbols::obarray_len(vec_val).unwrap_or(0);
    if vec_len == 0 {
        return Value::symbol(intern_uninterned(name));
    }
    let bucket_idx = obarray_hash(name, vec_len);
    let bucket =
        super::builtins::symbols::obarray_bucket(vec_val, bucket_idx).unwrap_or(Value::NIL);

    // Check if already interned
    if let Some(sym) = obarray_bucket_find(bucket, name) {
        return sym;
    }

    // Not found: create symbol and prepend to bucket chain
    let sym = Value::symbol(intern_uninterned(name));
    obarray_insert_symbol(vec_val, sym);
    sym
}

/// Look up a symbol in a custom obarray (vector) without interning.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_lookup(vec_val: Value, name: &str) -> Option<Value> {
    let vec_len = super::builtins::symbols::obarray_len(vec_val).unwrap_or(0);
    if vec_len == 0 {
        return None;
    }
    let bucket_idx = obarray_hash(name, vec_len);
    let bucket =
        super::builtins::symbols::obarray_bucket(vec_val, bucket_idx).unwrap_or(Value::NIL);
    obarray_bucket_find(bucket, name)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn table_header_symbol(vec_val: Value) -> Option<Value> {
    obarray_lookup(vec_val, ABBREV_TABLE_HEADER_NAME)
}

/// Check if a Value is an abbrev table (obarray with a header symbol carrying
/// a numeric `:abbrev-table-modiff` property).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn is_abbrev_table(eval: &super::eval::Context, value: &Value) -> bool {
    if super::builtins::symbols::check_obarray_value(*value).is_err() {
        return false;
    }
    if super::builtins::symbols::obarray_len(*value).unwrap_or(0) == 0 {
        return false;
    }
    table_header_symbol(*value)
        .and_then(symbol_id)
        .and_then(|id| {
            eval.obarray()
                .get_property_id(id, intern(":abbrev-table-modiff"))
        })
        .is_some_and(|v| v.is_integer())
}

/// Collect all symbols from an obarray into a Vec.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_all_symbols(vec_val: Value) -> Vec<Value> {
    let all_slots = super::builtins::symbols::obarray_buckets(vec_val).unwrap_or_default();
    let mut symbols = Vec::new();
    for slot in &all_slots {
        let mut current = *slot;
        loop {
            match current.kind() {
                ValueKind::Nil => break,
                ValueKind::Cons => {
                    let car = current.cons_car();
                    let cdr = current.cons_cdr();
                    symbols.push(car);
                    current = cdr;
                }
                _ => break,
            }
        }
    }
    symbols
}

// ===========================================================================
// Builtin helpers
// ===========================================================================

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_string(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .expect("ValueKind::String must carry LispString payload")),
        ValueKind::Symbol(id) => Ok(resolve_sym(id).to_owned()),
        ValueKind::Nil => Ok("nil".to_string()),
        ValueKind::T => Ok("t".to_string()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_abbrev_table(eval: &super::eval::Context, value: &Value) -> Result<Value, Flow> {
    if is_abbrev_table(eval, value) {
        Ok(*value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("abbrev-table-p"), *value],
        ))
    }
}

// ===========================================================================
// Obarray-based builtins
// ===========================================================================

/// (make-abbrev-table &optional PROPS) -> obarray
///
/// Create a new empty abbrev table (obarray with a "0" symbol).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_abbrev_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let table = Value::obarray(ABBREV_TABLE_DEFAULT_SIZE);

    let header = obarray_intern(table, ABBREV_TABLE_HEADER_NAME);
    let header_id = symbol_id(header).expect("abbrev-table header should be a symbol");
    eval.obarray_mut()
        .set_symbol_value_id(header_id, Value::NIL);
    eval.obarray_mut().put_property_id(
        header_id,
        intern(":abbrev-table-modiff"),
        Value::fixnum(0),
    )?;

    // Process optional property list
    if let Some(props_val) = args.first()
        && !props_val.is_nil()
        && let Some(props) = list_to_vec(props_val)
    {
        let mut i = 0;
        while i + 1 < props.len() {
            let prop = &props[i];
            let val = props[i + 1];
            if let Some(prop_name) = prop.as_symbol_name() {
                eval.obarray_mut()
                    .put_property_id(header_id, intern(prop_name), val)?;
            }
            i += 2;
        }
    }

    Ok(table)
}

/// (abbrev-table-p OBJ) -> t or nil
///
/// Return t if OBJ is an abbrev table (obarray with "0" symbol having
/// `abbrev-table` property).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_abbrev_table_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("abbrev-table-p", &args, 1)?;
    Ok(Value::bool_val(is_abbrev_table(eval, &args[0])))
}

/// (define-abbrev TABLE NAME EXPANSION &optional HOOK &rest PROPS) -> name-symbol
///
/// TABLE is an abbrev table (obarray).
/// NAME is a string. EXPANSION is a string or nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_define_abbrev(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("define-abbrev", &args, 3)?;
    let vec_val = expect_abbrev_table(eval, &args[0])?;
    let name = expect_string(&args[1])?;
    let mut props = if args.len() > 4 {
        args[4..].to_vec()
    } else {
        Vec::new()
    };
    if props
        .first()
        .is_some_and(|v| v.is_nil() || v.is_fixnum() || v.is_char())
    {
        let count = props.first().copied().unwrap_or(Value::NIL);
        let system = props.get(1).copied().unwrap_or(Value::NIL);
        props = vec![Value::keyword(":count"), count];
        if !system.is_nil() {
            props.push(Value::keyword(":system"));
            props.push(system);
        }
    }
    if !props
        .chunks_exact(2)
        .any(|chunk| chunk[0].as_symbol_name() == Some(":count"))
    {
        props.push(Value::keyword(":count"));
        props.push(Value::fixnum(0));
    }
    props.push(Value::keyword(":abbrev-table-modiff"));
    props.push(
        get_table_property(eval, vec_val, ":abbrev-table-modiff").unwrap_or(Value::fixnum(0)),
    );
    let system_flag = props
        .chunks_exact(2)
        .find(|chunk| chunk[0].as_symbol_name() == Some(":system"))
        .map(|chunk| chunk[1])
        .unwrap_or(Value::NIL);

    // Intern the abbreviation symbol into the obarray
    let sym = obarray_intern(vec_val, &name);
    let sym_id = symbol_id(sym).expect("abbrev symbol should be a symbol");

    let existing_expansion = eval.obarray().symbol_value_id(sym_id).cloned();
    let existing_hook = eval.obarray().symbol_function_id(sym_id);
    let existing_system = eval
        .obarray()
        .get_property_id(sym_id, intern(":system"))
        .is_some_and(|value| value.is_truthy());

    let system_setting = AbbrevSystemSetting::from_system_flag(system_flag);
    if system_setting.is_system()
        && !system_setting.allows_user_abbrev_overwrite()
        && existing_expansion.is_some_and(|value| !value.is_nil())
        && !existing_system
    {
        return Ok(Value::string(name));
    }

    // Set symbol-value to expansion (the expansion string or nil)
    let expansion = args[2];
    eval.obarray_mut().set_symbol_value_id(sym_id, expansion);

    // Set symbol-function to hook (4th arg), or nil
    let hook = if args.len() > 3 { args[3] } else { Value::NIL };
    eval.obarray_mut().set_symbol_function_id(sym_id, hook);

    let mut plist_entries = Vec::new();
    for chunk in props.chunks_exact(2) {
        if let Some(prop_name) = chunk[0].as_symbol_name() {
            let value = if prop_name == ":system" {
                system_setting.stored_property_value(chunk[1])
            } else {
                chunk[1]
            };
            plist_entries.push((intern(prop_name), value));
        }
    }
    eval.obarray_mut()
        .replace_symbol_plist_id(sym_id, plist_entries);

    let changed = existing_expansion != Some(expansion) || existing_hook != Some(hook);
    if changed && system_setting.marks_abbrevs_changed() {
        eval.obarray_mut()
            .set_symbol_value("abbrevs-changed", Value::T);
    }
    increment_table_modiff(eval, vec_val);

    Ok(Value::string(name))
}

/// (abbrev-symbol ABBREV &optional TABLE) -> symbol or nil
///
/// Look up ABBREV in TABLE (or the local/global abbrev tables).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_abbrev_symbol(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("abbrev-symbol", &args, 1)?;
    let name = expect_string(&args[0])?;

    // If TABLE is provided
    if let Some(table_val) = args.get(1)
        && !table_val.is_nil()
    {
        let vec_val = expect_abbrev_table(eval, table_val)?;
        if let Some(sym) = find_abbrev_symbol_in_table(eval, &name, vec_val) {
            return Ok(sym);
        }
        return Ok(Value::NIL);
    }

    // Fall back to global-abbrev-table
    let global_table = eval
        .obarray()
        .symbol_value("global-abbrev-table")
        .cloned()
        .unwrap_or(Value::NIL);
    if expect_abbrev_table(eval, &global_table).is_ok()
        && let Some(sym) = find_abbrev_symbol_in_table(eval, &name, global_table)
    {
        return Ok(sym);
    }
    Ok(Value::NIL)
}

/// (abbrev-expansion ABBREV &optional TABLE) -> string or nil
///
/// Look up the expansion of ABBREV without expanding it.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_abbrev_expansion(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("abbrev-expansion", &args, 1)?;
    let name = expect_string(&args[0])?;

    // If TABLE is provided
    if let Some(table_val) = args.get(1)
        && !table_val.is_nil()
    {
        let vec_val = expect_abbrev_table(eval, table_val)?;
        if let Some(sym) = find_abbrev_symbol_in_table(eval, &name, vec_val)
            && let Some(sym_id) = symbol_id(sym)
        {
            return Ok(eval
                .obarray()
                .symbol_value_id(sym_id)
                .cloned()
                .unwrap_or(Value::NIL));
        }
        return Ok(Value::NIL);
    }

    // Fall back to global-abbrev-table
    let global_table = eval
        .obarray()
        .symbol_value("global-abbrev-table")
        .cloned()
        .unwrap_or(Value::NIL);
    if expect_abbrev_table(eval, &global_table).is_ok()
        && let Some(sym) = find_abbrev_symbol_in_table(eval, &name, global_table)
        && let Some(sym_id) = symbol_id(sym)
    {
        return Ok(eval
            .obarray()
            .symbol_value_id(sym_id)
            .cloned()
            .unwrap_or(Value::NIL));
    }
    Ok(Value::NIL)
}

/// (clear-abbrev-table TABLE) -> nil
///
/// Reset all symbols in TABLE except the "0" property symbol.
/// In GNU Emacs, system abbrevs are kept but with empty expansion.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_clear_abbrev_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("clear-abbrev-table", &args, 1)?;
    let vec_val = expect_abbrev_table(eval, &args[0])?;
    let header = table_header_symbol(vec_val)
        .unwrap_or_else(|| Value::symbol(intern_uninterned(ABBREV_TABLE_HEADER_NAME)));
    let vec_len = super::builtins::symbols::obarray_len(vec_val).unwrap_or(0);
    let _ = super::builtins::symbols::replace_obarray_buckets(vec_val, vec![Value::NIL; vec_len]);
    let _ = vec_val.with_obarray_mut(|obj| obj.count = 0);
    obarray_insert_symbol(vec_val, header);
    if let Some(header_id) = symbol_id(header) {
        eval.obarray_mut()
            .set_symbol_value_id(header_id, Value::NIL);
    }
    increment_table_modiff(eval, vec_val);

    Ok(Value::NIL)
}

/// (abbrev-table-get TABLE PROP) -> value
///
/// Get property PROP from the header symbol of TABLE.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_abbrev_table_get(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("abbrev-table-get", &args, 2)?;
    let vec_val = expect_abbrev_table(eval, &args[0])?;
    let prop = args[1].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        )
    })?;
    Ok(get_table_property(eval, vec_val, prop).unwrap_or(Value::NIL))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn get_table_property(eval: &super::eval::Context, vec_val: Value, prop: &str) -> Option<Value> {
    table_header_symbol(vec_val)
        .and_then(symbol_id)
        .and_then(|id| eval.obarray().get_property_id(id, intern(prop)))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn increment_table_modiff(eval: &mut super::eval::Context, vec_val: Value) {
    let next = match get_table_property(eval, vec_val, ":abbrev-table-modiff") {
        Some(v) if v.is_fixnum() => v.as_fixnum().unwrap() + 1,
        _ => 1,
    };
    if let Some(header_id) = table_header_symbol(vec_val).and_then(symbol_id) {
        let _ = eval.obarray_mut().put_property_id(
            header_id,
            intern(":abbrev-table-modiff"),
            Value::fixnum(next),
        );
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn find_abbrev_symbol_in_table(
    eval: &super::eval::Context,
    abbrev: &str,
    vec_val: Value,
) -> Option<Value> {
    let case_fold =
        !get_table_property(eval, vec_val, ":case-fixed").is_some_and(|value| value.is_truthy());
    let direct = obarray_lookup(vec_val, abbrev);
    let folded = case_fold
        .then(|| abbrev.to_lowercase())
        .and_then(|lowered| {
            obarray_lookup(vec_val, &lowered).filter(|sym| {
                symbol_id(*sym)
                    .and_then(|id| eval.obarray().get_property_id(id, intern(":case-fixed")))
                    .is_none_or(|value| !value.is_truthy())
            })
        });

    if let Some(sym) = direct.or(folded)
        && let Some(sym_id) = symbol_id(sym)
        && eval
            .obarray()
            .symbol_value_id(sym_id)
            .is_some_and(|value| !value.is_nil())
    {
        return Some(sym);
    }

    if let Some(parents) = get_table_property(eval, vec_val, ":parents")
        && let Some(parent_list) = list_to_vec(&parents)
    {
        for parent in &parent_list {
            if expect_abbrev_table(eval, parent).is_ok()
                && let Some(sym) = find_abbrev_symbol_in_table(eval, abbrev, *parent)
            {
                return Some(sym);
            }
        }
    }

    None
}

/// (define-abbrev-table NAME DEFS &optional DOCSTRING &rest PROPS) -> nil
///
/// NAME is a symbol. Creates the table as an obarray, sets it as NAME's value,
/// and adds NAME to `abbrev-table-name-list`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_define_abbrev_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("define-abbrev-table", &args, 2)?;

    // NAME must be a symbol (quoted)
    let name = args[0].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;

    // Check if table already exists
    let table = if let Some(existing) = eval.obarray().symbol_value(name).cloned() {
        if is_abbrev_table(eval, &existing) {
            existing
        } else {
            // Create new table
            builtin_make_abbrev_table(eval, vec![])?
        }
    } else {
        builtin_make_abbrev_table(eval, vec![])?
    };

    let header_id = table_header_symbol(table)
        .and_then(symbol_id)
        .expect("abbrev table header should exist");

    // Set as symbol value
    eval.obarray_mut().set_symbol_value(name, table);

    // Add to abbrev-table-name-list if not already present
    let name_sym = Value::symbol(name);
    let current_list = eval
        .obarray()
        .symbol_value("abbrev-table-name-list")
        .cloned()
        .unwrap_or(Value::NIL);

    // Check if already in list
    let mut already_in_list = false;
    if let Some(items) = list_to_vec(&current_list) {
        for item in &items {
            if let (Some(a), Some(b)) = (item.as_symbol_name(), name_sym.as_symbol_name())
                && a == b
            {
                already_in_list = true;
                break;
            }
        }
    }

    if !already_in_list {
        let new_list = Value::cons(name_sym, current_list);
        eval.obarray_mut()
            .set_symbol_value("abbrev-table-name-list", new_list);
    }

    // Process DOCSTRING (3rd arg) -- store as table property if it's a string
    if let Some(docstring) = args.get(2)
        && docstring.is_string()
    {
        eval.obarray_mut()
            .put_property_id(header_id, intern(":docstring"), *docstring)?;
    }

    // Process properties (PROPS after docstring)
    // In GNU Emacs: (define-abbrev-table 'name defs "doc" :prop1 val1 :prop2 val2 ...)
    if args.len() > 3 {
        let mut i = 3;
        while i + 1 < args.len() {
            let prop = &args[i];
            let val = args[i + 1];
            if let Some(prop_name) = prop.as_symbol_name() {
                eval.obarray_mut()
                    .put_property_id(header_id, intern(prop_name), val)?;
            }
            i += 2;
        }
    }

    // Process DEFS (2nd arg) -- list of (name expansion hook &rest props)
    let defs = &args[1];
    if !defs.is_nil()
        && let Some(def_list) = list_to_vec(defs)
    {
        for def_val in &def_list {
            if let Some(def_items) = list_to_vec(def_val)
                && def_items.len() >= 2
            {
                let abbrev_name = expect_string(&def_items[0])?;
                let expansion = def_items[1];
                let hook = if def_items.len() > 2 {
                    def_items[2]
                } else {
                    Value::NIL
                };

                // Build args for define-abbrev
                let mut da_args = vec![table, Value::string(&abbrev_name), expansion, hook];

                // Append remaining items as keyword properties
                if def_items.len() > 3 {
                    da_args.extend_from_slice(&def_items[3..]);
                }

                builtin_define_abbrev(eval, da_args)?;
            }
        }
    }

    Ok(Value::NIL)
}

/// (expand-abbrev) -> string or nil
///
/// NeoVM stub: returns nil in batch/non-interactive use.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_expand_abbrev(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("expand-abbrev", &args, 0)?;
    Ok(Value::NIL)
}

/// (insert-abbrev-table-description NAME &optional READABLE) -> nil
///
/// Insert a description of the abbrev table named NAME into the current buffer.
/// This is a simplified version that inserts into the current buffer.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_insert_abbrev_table_description(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("insert-abbrev-table-description", &args, 1)?;

    let name = args[0].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;

    // Get the table value
    let table_val = eval
        .obarray()
        .symbol_value(name)
        .cloned()
        .unwrap_or(Value::NIL);

    if expect_abbrev_table(eval, &table_val).is_err() {
        // Insert empty table description
        let text = format!("(define-abbrev-table '{})\n", name);
        if let Some(current_id) = eval.buffers.current_buffer_id() {
            let (insert_pos, target_multibyte) = eval
                .buffers
                .get(current_id)
                .map(|b| (b.point_emacs_byte_pos(), b.get_multibyte()))
                .unwrap_or((EmacsBytePos::ZERO, true));
            let change = super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
                &eval.buffers,
                current_id,
                insert_pos,
                TextExtent::from_emacs_bytes(text.as_bytes(), target_multibyte),
            )?;
            super::editfns::signal_before_text_change(eval, change)?;
            let _ = eval.buffers.insert_into_buffer(current_id, &text);
            super::editfns::signal_after_text_change(eval, change)?;
        }
        return Ok(Value::NIL);
    }

    // Collect all abbrev symbols (not the header symbol).
    let symbols = obarray_all_symbols(table_val);
    let mut entries: Vec<(String, String, i64, Option<String>)> = Vec::new();

    for sym in &symbols {
        if let Some(sym_name) = sym.as_symbol_name() {
            if sym_name.is_empty() {
                continue;
            }
            let Some(sym_id) = symbol_id(*sym) else {
                continue;
            };
            let expansion = eval
                .obarray()
                .symbol_value_id(sym_id)
                .cloned()
                .unwrap_or(Value::NIL);
            if expansion.is_nil() {
                continue;
            }
            let exp_str = match expansion.kind() {
                ValueKind::String => expansion
                    .as_lisp_string()
                    .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                    .expect("ValueKind::String must carry LispString payload"),
                _ => continue,
            };
            let count = eval
                .obarray()
                .get_property_id(sym_id, intern(":count"))
                .and_then(|v| v.as_fixnum())
                .unwrap_or(0);
            let hook_fn = eval
                .obarray()
                .symbol_function_id(sym_id)
                .and_then(|v: Value| v.as_symbol_name().map(|s| s.to_string()));
            entries.push((sym_name.to_string(), exp_str, count, hook_fn));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut text = format!("(define-abbrev-table '{}\n  '(\n", name);
    for (abbrev, expansion, count, hook) in &entries {
        let hook_str = match hook {
            Some(h) => format!(" '{}", h),
            None => String::new(),
        };
        text.push_str(&format!(
            "    (\"{}\"{} \"{}\" {})\n",
            abbrev, hook_str, expansion, count
        ));
    }
    text.push_str("   ))\n\n");

    // Insert into current buffer
    if let Some(current_id) = eval.buffers.current_buffer_id() {
        let (insert_pos, target_multibyte) = eval
            .buffers
            .get(current_id)
            .map(|b| (b.point_emacs_byte_pos(), b.get_multibyte()))
            .unwrap_or((EmacsBytePos::ZERO, true));
        let change = super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
            &eval.buffers,
            current_id,
            insert_pos,
            TextExtent::from_emacs_bytes(text.as_bytes(), target_multibyte),
        )?;
        super::editfns::signal_before_text_change(eval, change)?;
        let _ = eval.buffers.insert_into_buffer(current_id, &text);
        super::editfns::signal_after_text_change(eval, change)?;
    }
    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
