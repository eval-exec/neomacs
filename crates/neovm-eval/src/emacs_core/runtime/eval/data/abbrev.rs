//! Abbreviation manager state (stage-4 K2 oracleb batch 7).
//!
//! The manager/table/abbrev records, their pure methods and the GC trace were
//! moved here verbatim from `commands/abbrev` per the design's eval-data
//! disposition for the abbrev WorldSlot.  The Lisp entry bodies stay in the
//! core command module and reach the manager through the Context field.

use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::LispString;
use neovm_value::intern::{SymId, intern, resolve_sym};
use neovm_value::value::Value;
use std::collections::HashMap;

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
    #[doc(hidden)]
    pub fn new(_name: &str) -> Self {
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
    pub(crate) tables: HashMap<SymId, AbbrevTable>,
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

    #[doc(hidden)]
    pub fn expand_abbrev_by_sym(&mut self, table_sym: SymId, word: &str) -> Option<String> {
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
    #[doc(hidden)]
    pub fn dump_tables(&self) -> &HashMap<SymId, AbbrevTable> {
        &self.tables
    }
    #[doc(hidden)]
    pub fn dump_global_table_sym(&self) -> SymId {
        self.global_table_sym
    }
    #[doc(hidden)]
    pub fn dump_abbrev_mode(&self) -> bool {
        self.abbrev_mode
    }
    #[doc(hidden)]
    pub fn from_dump(
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

impl GcTrace for AbbrevManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.trace_roots_with(&mut |value| roots.push(value));
    }

    fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        let Self {
            tables,
            global_table_sym: _,
            abbrev_mode: _,
        } = self;
        for table in tables.values() {
            let AbbrevTable {
                name,
                abbrevs,
                parent,
                case_fixed: _,
                enable_quoting: _,
            } = table;
            // Restoration admits property-bearing names as well as payloads.
            // GNU's intern/make-symbol also retains its supplied name string.
            name.trace_roots_with(visit);
            if let Some(parent) = parent {
                parent.trace_roots_with(visit);
            }
            for (key, abbrev) in abbrevs {
                let Abbrev {
                    expansion,
                    hook,
                    count: _,
                    system: _,
                } = abbrev;
                key.trace_roots_with(visit);
                expansion.trace_roots_with(visit);
                if let Some(hook) = hook {
                    hook.trace_roots_with(visit);
                }
            }
        }
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn runtime_string_to_abbrev_string(text: &str) -> LispString {
    crate::emacs_core::builtins::plain_str_to_lisp_string(text, true)
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn abbrev_table_sym(text: &str) -> SymId {
    intern(text)
}

fn abbrev_table_sym_from_lisp(text: &LispString) -> SymId {
    // Issue #131: intern the symbol-name key from the exact Emacs-byte
    // LispString instead of routing through a lossy/storage Rust string, so the
    // abbrev-table key stays byte-faithful (no eight-bit collision). For ASCII
    // and pure-Unicode names this yields the same SymId as the `&str` path.
    neovm_value::intern::intern_lisp_string(text)
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn abbrev_string_to_runtime(text: &LispString) -> String {
    // Issue #131: remaining consumers are display/expansion/sort text reached
    // through `&str` APIs; a lossy UTF-8 rendering keeps real Unicode (incl.
    // PUA) while dropping the buggy storage-string sentinels.
    neovm_value::emacs_char::to_utf8_lossy(text.as_bytes())
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
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
