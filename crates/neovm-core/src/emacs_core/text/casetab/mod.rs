//! Case-table support for Emacs case conversion.
//!
//! Provides a `CaseTable` struct holding upcase/downcase/canonicalize/equivalences
//! mappings, a `CaseTableManager` with standard ASCII case tables pre-initialized,
//! and pure builtins for case-table predicates and character case conversion.

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args;
use crate::emacs_core::intern::{SymId, intern};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static STANDARD_CASE_TABLE_OBJECT: RefCell<Option<Value>> = const { RefCell::new(None) };
    static STANDARD_CASE_TABLE_HEAP: Cell<usize> = const { Cell::new(0) };
}

/// Clear cached thread-local case table (must be called when heap changes).
pub fn reset_casetab_thread_locals() {
    STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = None);
    STANDARD_CASE_TABLE_HEAP
        .with(|owner| owner.set(crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0)));
}

/// Collect GC roots from the cached case table.
pub fn collect_casetab_gc_roots(roots: &mut Vec<Value>, heap_identity: usize) {
    if STANDARD_CASE_TABLE_HEAP.with(Cell::get) != heap_identity {
        return;
    }
    STANDARD_CASE_TABLE_OBJECT.with(|slot| {
        if let Some(v) = *slot.borrow() {
            roots.push(v);
        }
    });
}

/// Restore the explicit standard-table cache without changing hot case lookups.
pub(crate) fn activate_casetab_thread_locals(table: Option<Value>) {
    STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = table);
    STANDARD_CASE_TABLE_HEAP
        .with(|owner| owner.set(crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0)));
}

// ---------------------------------------------------------------------------
// CaseTable
// ---------------------------------------------------------------------------

/// A case table holding four character mappings.
#[derive(Clone, Debug)]
pub struct CaseTable {
    /// Maps lowercase characters to their uppercase equivalents.
    pub upcase: HashMap<char, char>,
    /// Maps uppercase characters to their lowercase equivalents.
    pub downcase: HashMap<char, char>,
    /// Maps characters to a canonical form (used for case-insensitive comparison).
    pub canonicalize: HashMap<char, char>,
    /// Maps characters to the next character in the equivalence class cycle.
    pub equivalences: HashMap<char, char>,
}

impl CaseTable {
    /// Create an empty case table with no mappings.
    pub fn empty() -> Self {
        Self {
            upcase: HashMap::new(),
            downcase: HashMap::new(),
            canonicalize: HashMap::new(),
            equivalences: HashMap::new(),
        }
    }

    /// Create the standard ASCII case table (a-z <-> A-Z).
    pub fn standard_ascii() -> Self {
        let mut upcase = HashMap::new();
        let mut downcase = HashMap::new();
        let mut canonicalize = HashMap::new();
        let mut equivalences = HashMap::new();

        for lower in b'a'..=b'z' {
            let upper = lower - b'a' + b'A';
            let lc = lower as char;
            let uc = upper as char;

            // Upcase: lowercase -> uppercase
            upcase.insert(lc, uc);
            // Downcase: uppercase -> lowercase
            downcase.insert(uc, lc);

            // Canonicalize: both map to lowercase
            canonicalize.insert(uc, lc);
            canonicalize.insert(lc, lc);

            // Equivalences: cycle upper -> lower -> upper
            equivalences.insert(uc, lc);
            equivalences.insert(lc, uc);
        }

        Self {
            upcase,
            downcase,
            canonicalize,
            equivalences,
        }
    }
}

// ---------------------------------------------------------------------------
// CaseTableManager
// ---------------------------------------------------------------------------

/// Manages case tables, providing standard ASCII case conversion by default.
#[derive(Clone, Debug)]
pub struct CaseTableManager {
    /// The standard (immutable) case table.
    standard: CaseTable,
    /// The current buffer-local case table.
    current: CaseTable,
}

impl CaseTableManager {
    /// Create a new manager with the standard ASCII case table.
    pub fn new() -> Self {
        let table = CaseTable::standard_ascii();
        Self {
            standard: table.clone(),
            current: table,
        }
    }

    /// Convert a character to uppercase using the current case table.
    /// Returns the character unchanged if no upcase mapping exists.
    pub fn upcase_char(&self, c: char) -> char {
        *self.current.upcase.get(&c).unwrap_or(&c)
    }

    /// Convert a character to lowercase using the current case table.
    /// Returns the character unchanged if no downcase mapping exists.
    pub fn downcase_char(&self, c: char) -> char {
        *self.current.downcase.get(&c).unwrap_or(&c)
    }

    /// Convert an entire string to uppercase.
    pub fn upcase_string(&self, s: &str) -> String {
        s.chars().map(|c| self.upcase_char(c)).collect()
    }

    /// Convert an entire string to lowercase.
    pub fn downcase_string(&self, s: &str) -> String {
        s.chars().map(|c| self.downcase_char(c)).collect()
    }

    /// Return a reference to the standard case table.
    pub fn standard_table(&self) -> &CaseTable {
        &self.standard
    }

    /// Return a reference to the current case table.
    pub fn current_table(&self) -> &CaseTable {
        &self.current
    }

    /// Set the current case table.
    pub fn set_current(&mut self, table: CaseTable) {
        self.current = table;
    }

    /// Set the standard case table.
    pub fn set_standard(&mut self, table: CaseTable) {
        self.standard = table;
    }
}

impl Default for CaseTableManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Signal `wrong-type-argument` with a predicate name.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn wrong_type(pred: &str, got: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(pred), *got],
    )
}

/// Extract a character from a Value (Int or Char), signal otherwise.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_char(value: &Value) -> Result<char, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) => super::builtins::character_code_to_rust_char(c).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Invalid character code"), *value],
            )
        }),
        _other => Err(wrong_type("characterp", value)),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// `(case-table-p OBJ)` -- return t if OBJ is a case table.
///
/// A case table is a char-table with `case-table` sub-type and 3 extra slots
/// (upcase, canonicalize, equivalences).
pub(crate) fn builtin_case_table_p(args: Vec<Value>) -> EvalResult {
    expect_args("case-table-p", &args, 1)?;
    Ok(Value::bool_val(is_case_table(&args[0])))
}

/// `(downcase CHAR)` -- convert a character to lowercase.
///
/// If the argument is an integer or character, returns the lowercase version
/// using the standard ASCII case table. Characters outside A-Z are returned
/// unchanged.
#[cfg(test)]
pub(crate) fn builtin_downcase_char(args: Vec<Value>) -> EvalResult {
    expect_args("downcase", &args, 1)?;
    let c = expect_char(&args[0])?;
    let manager = CaseTableManager::new();
    let result = manager.downcase_char(c);
    Ok(Value::fixnum(result as i64))
}

// ---------------------------------------------------------------------------
// Case-table as char-table
// ---------------------------------------------------------------------------

// Phase 10D holdout 5: per-buffer case-table char-table now lives in
// `Buffer::slots[BUFFER_SLOT_CASE_TABLE.index()]`. NeoMacs collapses GNU's four
// separate `downcase_table_` / `upcase_table_` / `case_canon_table_` /
// `case_eqv_table_` BVAR slots (`buffer.h:408-417`) into a single
// downcase char-table whose extras[0..2] hold the upcase / canonicalize /
// equivalence subsidiary tables — the same value shape `Fcurrent_case_table`
// returns. The slot is non-Lisp-visible (`install_as_forwarder: false`),
// always-local (`local_flags_idx == -1`, matching GNU `buffer.c:4731-4734`).
// Reads/writes happen through `(current-case-table)` / `(set-case-table)`.
const STANDARD_CASE_TABLE_SYMBOL: &str = "neovm--standard-case-table-object";

#[inline(always)]
fn standard_case_table_object_symbol_id() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern(STANDARD_CASE_TABLE_SYMBOL))
}

/// Create the standard case table: a char-table with `case-table` subtype,
/// 3 extra slots (upcase, canonicalize, equivalences), and ASCII case mappings.
fn case_table_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| intern("case-table"))
}

fn make_standard_case_table_value() -> Value {
    // GNU `init_casetab_once' (casetab.c): four `case-table' char-tables,
    // each carrying the purpose's three extra slots, filled for ASCII.
    // `set_case_table' later points canon's third slot at eqv.  These are
    // real char-tables, so a lookup indexes instead of scanning the pairs
    // `characters.el' adds for every cased character.
    let down = make_empty_case_table();
    let up = make_empty_case_table();
    let canon = make_empty_case_table();
    let eqv = make_empty_case_table();
    for i in 0i64..128 {
        let lower = if (b'A' as i64..=b'Z' as i64).contains(&i) {
            i + (b'a' as i64 - b'A' as i64)
        } else {
            i
        };
        let upper = if (b'a' as i64..=b'z' as i64).contains(&i) {
            i + (b'A' as i64 - b'a' as i64)
        } else {
            i
        };
        let other = if lower != i { lower } else { upper };
        super::chartable::ct_set_single(&down, i, Value::fixnum(lower));
        super::chartable::ct_set_single(&up, i, Value::fixnum(upper));
        super::chartable::ct_set_single(&canon, i, Value::fixnum(lower));
        super::chartable::ct_set_single(&eqv, i, Value::fixnum(other));
    }
    set_case_table_extra(down, 0, up);
    set_case_table_extra(down, 1, canon);
    set_case_table_extra(down, 2, eqv);
    set_case_table_extra(canon, 2, eqv);
    down
}

/// Build a custom case table equal to the standard ASCII table but with one
/// extra uppercase/lowercase pair installed, exactly as Lisp's
/// `(set-case-syntax-pair UC LC (copy-case-table (standard-case-table)))`
/// does (`lisp/case-table.el`): downcase[UC]=LC, downcase[LC]=LC, upcase[UC]=UC,
/// upcase[LC]=UC. The canon/eqv extras are left nil so they are recomputed from
/// the down/up tables by `ensure_case_table_derived_slots` (GNU `set_case_table`).
#[cfg(test)]
pub(crate) fn make_case_table_with_pair(uc: i64, lc: i64) -> Value {
    let mut downcase_pairs = Vec::with_capacity(128);
    let mut upcase_pairs = Vec::with_capacity(128);

    for i in 0i64..128 {
        let down = if (b'A' as i64..=b'Z' as i64).contains(&i) {
            i + (b'a' as i64 - b'A' as i64)
        } else {
            i
        };
        downcase_pairs.push((i, Value::fixnum(down)));

        let up = if (b'a' as i64..=b'z' as i64).contains(&i) {
            i + (b'A' as i64 - b'a' as i64)
        } else {
            i
        };
        upcase_pairs.push((i, Value::fixnum(up)));
    }

    // Apply the pair (set-case-syntax-pair UC LC).
    for (key, val) in [(uc, lc), (lc, lc)] {
        if let Some(slot) = downcase_pairs.get_mut(key as usize) {
            *slot = (key, Value::fixnum(val));
        }
    }
    for (key, val) in [(uc, uc), (lc, uc)] {
        if let Some(slot) = upcase_pairs.get_mut(key as usize) {
            *slot = (key, Value::fixnum(val));
        }
    }

    let upcase_ct = make_empty_case_table();
    for (ch, value) in upcase_pairs {
        super::chartable::ct_set_single(&upcase_ct, ch, value);
    }
    // canon (extras[1]) and eqv (extras[2]) are nil: recomputed on install.
    let downcase_ct = make_empty_case_table();
    for (ch, value) in downcase_pairs {
        super::chartable::ct_set_single(&downcase_ct, ch, value);
    }
    set_case_table_extra(downcase_ct, 0, upcase_ct);
    downcase_ct
}

/// `(current-case-table)` -- evaluator-backed current buffer case table object.
pub(crate) fn builtin_current_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-case-table", &args, 0)?;
    current_case_table_for_buffer_in_state(&mut ctx.obarray, &mut ctx.buffers)
}

/// `(standard-case-table)` -- evaluator-backed standard case table object.
pub(crate) fn builtin_standard_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("standard-case-table", &args, 0)?;
    ensure_standard_case_table_object_in_state(&mut ctx.obarray)
}

/// `(set-case-table TABLE)` -- evaluator-backed current buffer case table set.
pub(crate) fn builtin_set_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-case-table", &args, 1)?;
    if !is_case_table(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), args[0]],
        ));
    }
    let table = args[0];
    let _ = ensure_standard_case_table_object_in_state(&mut ctx.obarray)?;
    ensure_case_table_derived_slots(table)?;
    set_current_case_table_for_buffer_in_state(&mut ctx.buffers, table)?;
    Ok(table)
}

/// `(set-standard-case-table TABLE)` -- evaluator-backed standard table set.
pub(crate) fn builtin_set_standard_case_table(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-standard-case-table", &args, 1)?;
    if !is_case_table(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), args[0]],
        ));
    }
    STANDARD_CASE_TABLE_OBJECT.with(|slot| {
        *slot.borrow_mut() = Some(args[0]);
    });
    ctx.cached_standard_case_table = Some(args[0]);
    STANDARD_CASE_TABLE_HEAP.with(|owner| owner.set(ctx.tagged_heap.identity()));
    let table = args[0];
    ensure_case_table_derived_slots(table)?;
    ctx.obarray
        .set_symbol_value_id(standard_case_table_object_symbol_id(), table);
    Ok(table)
}

fn ensure_standard_case_table_object_in_state(obarray: &mut super::symbol::Obarray) -> EvalResult {
    if let Some(value) = obarray.symbol_value_id_copied(standard_case_table_object_symbol_id())
        && is_case_table(&value)
    {
        return Ok(value);
    }
    let table = make_standard_case_table_value();
    obarray.set_symbol_value_id(standard_case_table_object_symbol_id(), table);
    Ok(table)
}

fn current_case_table_for_buffer_in_state(
    obarray: &mut super::symbol::Obarray,
    buffers: &mut crate::buffer::BufferManager,
) -> Result<Value, Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
    let fallback = ensure_standard_case_table_object_in_state(obarray)?;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Mirrors GNU `Fcurrent_case_table` (`casetab.c:65-72`):
    //     return BVAR (current_buffer, downcase_table);
    let value = buf.slots[BUFFER_SLOT_CASE_TABLE.index()];
    if is_case_table(&value) {
        return Ok(value);
    }

    // Slot unset or invalid: seed from the standard table —
    // matches GNU `reset_buffer` cloning the standard tables
    // into a fresh buffer (`buffer.c:1149-1157`).
    buf.slots[BUFFER_SLOT_CASE_TABLE.index()] = fallback;
    Ok(fallback)
}

pub(crate) fn sync_current_buffer_case_table_state(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<(), Flow> {
    let _ = current_case_table_for_buffer_in_state(&mut ctx.obarray, &mut ctx.buffers)?;
    Ok(())
}

pub(crate) fn current_case_canon_table(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<Value, Flow> {
    let table = current_case_table_for_buffer_in_state(&mut ctx.obarray, &mut ctx.buffers)?;
    ensure_case_table_derived_slots(table)?;
    Ok(case_table_extra(table, 1))
}

/// Resolve GNU Fupcase's actual upcase table, including explicit changes to
/// the standard table. The caller keeps a call-local, read-only projection;
/// this adds no cache or Lisp state shared between independently owned Contexts.
pub(crate) fn current_case_upcase_table(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<Value, Flow> {
    let table = current_case_table_for_buffer_in_state(&mut ctx.obarray, &mut ctx.buffers)?;
    // GNU prepare_casing_context only rebuilds a modified table when the
    // canonicalize slot is nil, so ordinary comparisons do not write tables.
    if case_table_extra(table, 1).is_nil() {
        ensure_case_table_derived_slots(table)?;
    }
    Ok(case_table_extra(table, 0))
}

/// The case-fold canon char-table to use as the search translate table for a
/// search in `buf` -- GNU's `BVAR (current_buffer, case_canon_table)` used as
/// the search `trt`. Returns `None` when `buf` uses the standard case table
/// (the hot default path), so the search engine keeps its fast hardwired
/// Unicode folding; returns `Some(canon)` only when a custom
/// `set-case-syntax-pair` table is installed, so a custom pair (e.g. `[`/`]`)
/// folds during search just as `char-equal` already does.
pub(crate) fn buffer_case_canon_table(buf: &crate::buffer::Buffer) -> Option<Value> {
    use crate::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
    let table = buf.slots[BUFFER_SLOT_CASE_TABLE.index()];
    if !is_case_table(&table) {
        return None;
    }
    // Standard table (by object identity): fold via the hardwired path.
    let standard = STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow());
    if standard.is_some_and(|s| s.bits() == table.bits()) {
        return None;
    }
    // Custom table: the canon subsidiary lives in extras[1] (derived by
    // `set-case-table`). Only use it if it is a usable char-table.
    let canon = case_table_extra(table, 1);
    super::chartable::is_char_table(&canon).then_some(canon)
}

/// Which subsidiary case char-table to consult for a per-buffer override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaseMap {
    /// The downcase (main) table — `BVAR (current_buffer, downcase_table)`.
    Down,
    /// The upcase table — extras[0] / `BVAR (current_buffer, upcase_table)`.
    Up,
    /// The canonicalize table — extras[1] / `BVAR (current_buffer, case_canon_table)`.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Canon,
}

/// Call-local case-table projection. Standard Unicode and installed tables
/// are distinct modes: a missing entry in an installed table means identity,
/// never fallback to Unicode (GNU buffer.h:1649-1663). Installed heap handles
/// remain on the mutator that resolved them; no shared cache is added.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CaseTableOverride {
    Standard,
    Installed(InstalledCaseTables),
}

/// Mutator-local heap handles borrowed from the current buffer's case table.
/// The buffer owns their roots throughout a synchronous casing operation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InstalledCaseTables {
    down: Value,
    up: Value,
    canon: Value,
    mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(CaseTableOverride: Send, Sync);
static_assertions::assert_not_impl_any!(InstalledCaseTables: Send, Sync);

impl CaseTableOverride {
    /// Resolve the override for the current buffer, mirroring GNU's use of the
    /// per-buffer downcase/upcase/canon char-tables (`buffer.h:1648-1663`,
    /// `editfns.c:4440`).
    pub(crate) fn for_current_buffer(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> Result<Self, Flow> {
        // Without a current buffer there is no buffer-local case table; behave
        // like GNU's default (the standard table) and use the hardwired path.
        if ctx.buffers.current_buffer_id().is_none() {
            return Ok(Self::none());
        }
        let standard = ensure_standard_case_table_object_in_state(&mut ctx.obarray)?;
        let table = current_case_table_for_buffer_in_state(&mut ctx.obarray, &mut ctx.buffers)?;
        // The standard table's explicit entries already match the hardwired
        // ASCII path; skip the override entirely so we stay byte-identical.
        if table.bits() == standard.bits() {
            return Ok(Self::Standard);
        }
        // A custom table is installed: make sure the up/canon subsidiary tables
        // exist (GNU `set_case_table` recomputes them lazily via extras[1] nil).
        ensure_case_table_derived_slots(table)?;
        Ok(Self::Installed(InstalledCaseTables {
            down: table,
            up: case_table_extra(table, 0),
            canon: case_table_extra(table, 1),
            mutator: std::marker::PhantomData,
        }))
    }

    /// An override that never overrides — used by pure/test callers and the
    /// default path so the hardwired Unicode mapping is used verbatim.
    pub(crate) fn none() -> Self {
        Self::Standard
    }

    /// True when a non-standard case table is installed.
    pub(crate) fn is_custom(&self) -> bool {
        matches!(self, Self::Installed(_))
    }

    /// Read-only counterpart of [`Self::for_current_buffer`]: resolve the
    /// override straight from a buffer's installed case table (subsidiary tables
    /// already exist once `set-case-table` ran). Used where only an immutable
    /// buffer is in scope (replace-match case analysis).
    pub(crate) fn for_buffer_readonly(buf: &crate::buffer::Buffer) -> Self {
        use crate::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
        let table = buf.slots[BUFFER_SLOT_CASE_TABLE.index()];
        if !is_case_table(&table) {
            return Self::none();
        }
        let standard = STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow());
        if standard.is_some_and(|s| s.bits() == table.bits()) {
            return Self::none();
        }
        Self::Installed(InstalledCaseTables {
            down: table,
            up: case_table_extra(table, 0),
            canon: case_table_extra(table, 1),
            mutator: std::marker::PhantomData,
        })
    }

    /// GNU `UPPERCASEP(c)`: downcasing through the case table changes the char.
    /// Standard tables use Unicode; missing installed entries mean identity.
    pub(crate) fn is_upper(&self, ch: char) -> bool {
        match self.map(CaseMap::Down, ch as i64) {
            Some(down) => down != ch as i64,
            None => ch.is_uppercase(),
        }
    }

    /// GNU `LOWERCASEP(c)`: not uppercase, and upcasing through the case table
    /// changes the char. Missing installed entries mean identity.
    pub(crate) fn is_lower(&self, ch: char) -> bool {
        if self.is_upper(ch) {
            return false;
        }
        match self.map(CaseMap::Up, ch as i64) {
            Some(up) => up != ch as i64,
            None => ch.is_lowercase(),
        }
    }

    /// Standard tables defer to the built-in Unicode mapping; installed tables
    /// return their natnum entry or the original character for every lookup.
    pub(crate) fn map(&self, which: CaseMap, code: i64) -> Option<i64> {
        let Self::Installed(tables) = *self else {
            return None;
        };
        let table = match which {
            CaseMap::Down => tables.down,
            CaseMap::Up => tables.up,
            CaseMap::Canon => tables.canon,
        };
        if !is_case_table_subsidiary(&table) {
            return Some(code);
        }
        Some(
            super::chartable::ct_lookup(&table, code)
                .ok()
                .and_then(fixnum_char)
                .unwrap_or(code),
        )
    }
}

fn is_case_table_subsidiary(table: &Value) -> bool {
    super::chartable::is_char_table(table)
}

fn set_current_case_table_for_buffer_in_state(
    buffers: &mut crate::buffer::BufferManager,
    table: Value,
) -> Result<(), Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Mirrors GNU `Fset_case_table` (`casetab.c:82-86`) → `set_case_table`
    // (`casetab.c:135-202`) which decomposes the table into 4 BVAR slots
    // and bset_*'s each one. NeoMacs collapses those into a single slot,
    // so the write here is the equivalent of GNU's bset_downcase_table
    // plus the implicit consistency between extras[0..2] and the other
    // 3 case tables. The case-table slot is always-local
    // (`local_flags_idx == -1`), so no flag bit needs setting.
    buf.slots[BUFFER_SLOT_CASE_TABLE.index()] = table;
    Ok(())
}

fn case_table_extra(table: Value, idx: usize) -> Value {
    table
        .as_char_table_obj()
        .and_then(|obj| obj.extras.as_slice().get(idx).copied())
        .unwrap_or(Value::NIL)
}

fn set_case_table_extra(table: Value, idx: usize, value: Value) {
    let _ = table.with_char_table_mut(|obj| {
        if let Some(slot) = obj.extras.ensure_owned().get_mut(idx) {
            *slot = value;
        }
    });
}

fn make_empty_case_table() -> Value {
    Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3)
}

fn fixnum_char(value: Value) -> Option<i64> {
    match value.kind() {
        ValueKind::Fixnum(n)
            if (0..=crate::emacs_core::emacs_char::MAX_CHAR as i64).contains(&n) =>
        {
            Some(n)
        }
        _ => None,
    }
}

fn range_bounds(key: Value) -> Option<(i64, i64)> {
    match key.kind() {
        ValueKind::Fixnum(n) => Some((n, n)),
        ValueKind::Cons => Some((key.cons_car().as_fixnum()?, key.cons_cdr().as_fixnum()?)),
        _ => None,
    }
}

fn set_char_table_range_value(table: Value, range: Value, value: Value) -> Result<(), Flow> {
    super::chartable::builtin_set_char_table_range(vec![table, range, value], None)?;
    Ok(())
}

fn set_identity(table: Value, key: Value, elt: Value) -> Result<(), Flow> {
    let Some(_elt) = fixnum_char(elt) else {
        return Ok(());
    };
    let Some((from, to)) = range_bounds(key) else {
        return Ok(());
    };
    for ch in from..=to {
        set_char_table_range_value(table, Value::fixnum(ch), Value::fixnum(ch))?;
    }
    Ok(())
}

fn shuffle(table: Value, key: Value, elt: Value) -> Result<(), Flow> {
    let Some(elt) = fixnum_char(elt) else {
        return Ok(());
    };
    let Some((from, to)) = range_bounds(key) else {
        return Ok(());
    };
    for ch in from..=to {
        let tem = super::chartable::ct_lookup(&table, elt)?;
        set_char_table_range_value(table, Value::fixnum(elt), Value::fixnum(ch))?;
        set_char_table_range_value(table, Value::fixnum(ch), tem)?;
    }
    Ok(())
}

fn set_canon(case_table: Value, key: Value, elt: Value) -> Result<(), Flow> {
    let Some(elt) = fixnum_char(elt) else {
        return Ok(());
    };
    let up = case_table_extra(case_table, 0);
    let canon = case_table_extra(case_table, 1);
    let up_elt = super::chartable::ct_lookup(&up, elt)?;
    let Some(up_elt) = fixnum_char(up_elt) else {
        return Ok(());
    };
    let canonical = super::chartable::ct_lookup(&case_table, up_elt)?;
    set_char_table_range_value(canon, key, canonical)
}

fn map_case_table(
    table: Value,
    f: impl FnMut(Value, Value) -> Result<(), Flow>,
) -> Result<(), Flow> {
    super::chartable::for_each_char_table_mapping(&table, f)
}

fn ensure_case_table_derived_slots(table: Value) -> Result<(), Flow> {
    if !is_case_table(&table) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), table],
        ));
    }

    let mut up = case_table_extra(table, 0);
    if up.is_nil() {
        up = make_empty_case_table();
        map_case_table(table, |key, elt| set_identity(up, key, elt))?;
        map_case_table(table, |key, elt| shuffle(up, key, elt))?;
        set_case_table_extra(table, 0, up);
    }

    let mut canon = case_table_extra(table, 1);
    if canon.is_nil() {
        canon = make_empty_case_table();
        set_case_table_extra(table, 1, canon);
        map_case_table(table, |key, elt| set_canon(table, key, elt))?;
    }

    let mut eqv = case_table_extra(table, 2);
    if eqv.is_nil() {
        eqv = make_empty_case_table();
        map_case_table(canon, |key, elt| set_identity(eqv, key, elt))?;
        map_case_table(canon, |key, elt| shuffle(eqv, key, elt))?;
        set_case_table_extra(table, 2, eqv);
    }
    set_case_table_extra(canon, 2, eqv);

    Ok(())
}

/// Return `true` if `v` is a case table (GNU `Fcase_table_p`): a char-table
/// with the `case-table` purpose whose up/canon/eqv extras are nil or
/// char-tables, with eqv present only alongside canon.
pub fn is_case_table(v: &Value) -> bool {
    use super::chartable::is_char_table;
    let Some(obj) = v.as_char_table_obj() else {
        return false;
    };
    if obj.purpose.as_symbol_id() != Some(case_table_sym_id()) || obj.extras.len() < 3 {
        return false;
    }
    let up = obj.extras[0];
    let canon = obj.extras[1];
    let eqv = obj.extras[2];
    (up.is_nil() || is_char_table(&up))
        && ((canon.is_nil() && eqv.is_nil())
            || (is_char_table(&canon) && (eqv.is_nil() || is_char_table(&eqv))))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/casetab_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;
