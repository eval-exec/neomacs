//! Case-table support for Emacs case conversion.
//!
//! Provides a `CaseTable` struct holding upcase/downcase/canonicalize/equivalences
//! mappings, a `CaseTableManager` with standard ASCII case tables pre-initialized,
//! and pure builtins for case-table predicates and character case conversion.

use neovm_value::error::LispCondition;
use neovm_value::error::expect_args;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::SymbolMode;
use neovm_value::value::*;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static STANDARD_CASE_TABLE_OBJECT: RefCell<Option<Value>> = const { RefCell::new(None) };
    static STANDARD_CASE_TABLE_HEAP: Cell<usize> = const { Cell::new(0) };
}

/// Clear cached thread-local case table (must be called when heap changes).
pub fn reset_casetab_thread_locals() {
    STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = None);
    STANDARD_CASE_TABLE_HEAP.with(|owner| {
        owner.set(neovm_value::tagged::gc::current_tagged_heap_identity().unwrap_or(0))
    });
}

/// Install the cached standard-table object (the core-side
/// `set-standard-case-table` entry); the cache itself stays private to text.
#[doc(hidden)]
pub fn cache_standard_case_table_object(table: Value) {
    STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = Some(table));
}

/// Record the tagged-heap identity of the cached standard table.
#[doc(hidden)]
pub fn cache_standard_case_table_heap(heap_identity: usize) {
    STANDARD_CASE_TABLE_HEAP.with(|owner| owner.set(heap_identity));
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
#[doc(hidden)]
pub fn activate_casetab_thread_locals(table: Option<Value>) {
    STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = table);
    STANDARD_CASE_TABLE_HEAP.with(|owner| {
        owner.set(neovm_value::tagged::gc::current_tagged_heap_identity().unwrap_or(0))
    });
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
        ValueKind::Fixnum(c) => {
            neovm_value::emacs_char::character_code_to_rust_char(c).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Invalid character code"), *value],
                )
            })
        }
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
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_case_table_p(args: Vec<Value>) -> EvalResult {
    builtin_case_table_p_with_symbols(args, SymbolMode::Bare)
}

pub(crate) fn builtin_case_table_p_with_symbols(args: Vec<Value>, mode: SymbolMode) -> EvalResult {
    expect_args("case-table-p", &args, 1)?;
    Ok(Value::bool_val(is_case_table_with_symbols(&args[0], mode)))
}

/// `(downcase CHAR)` -- convert a character to lowercase.
///
/// If the argument is an integer or character, returns the lowercase version
/// using the standard ASCII case table. Characters outside A-Z are returned
/// unchanged.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
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

/// The variable holding the standard case table: storage for what GNU
/// keeps in C globals (`Vascii_downcase_table`, casetab.c), so never a
/// thread projection (admission leaves it as it is).
#[inline(always)]
#[doc(hidden)]
pub fn standard_case_table_object_symbol_id() -> SymId {
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
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
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

#[doc(hidden)]
pub fn ensure_standard_case_table_object_in_state_with_symbols(
    obarray: &mut neovm_value::symbol::Obarray,
    mode: SymbolMode,
) -> EvalResult {
    if let Some(value) = obarray.canonical_value(
        neovm_value::symbol::CanonicalAccess::STORAGE,
        standard_case_table_object_symbol_id(),
    ) && is_case_table_with_symbols(&value, mode)
    {
        return Ok(value);
    }
    let table = make_standard_case_table_value();
    obarray.set_canonical_value(
        neovm_value::symbol::CanonicalAccess::STORAGE,
        standard_case_table_object_symbol_id(),
        table,
    );
    Ok(table)
}

#[doc(hidden)]
pub fn current_case_table_for_buffer_in_state_with_symbols(
    obarray: &mut neovm_value::symbol::Obarray,
    buffers: &mut neovm_edit::buffer::BufferManager,
    mode: SymbolMode,
) -> Result<Value, Flow> {
    use neovm_edit::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
    let fallback = ensure_standard_case_table_object_in_state_with_symbols(obarray, mode)?;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Mirrors GNU `Fcurrent_case_table` (`casetab.c:65-72`):
    //     return BVAR (current_buffer, downcase_table);
    let value = buf.slots()[BUFFER_SLOT_CASE_TABLE.index()];
    if is_case_table_with_symbols(&value, mode) {
        return Ok(value);
    }

    // Slot unset or invalid: seed from the standard table —
    // matches GNU `reset_buffer` cloning the standard tables
    // into a fresh buffer (`buffer.c:1149-1157`).
    buf.slots_mut()[BUFFER_SLOT_CASE_TABLE.index()] = fallback;
    Ok(fallback)
}

/// The case-fold canon char-table to use as the search translate table for a
/// search in `buf` -- GNU's `BVAR (current_buffer, case_canon_table)` used as
/// the search `trt`. Returns `None` when `buf` uses the standard case table
/// (the hot default path), so the search engine keeps its fast hardwired
/// Unicode folding; returns `Some(canon)` only when a custom
/// `set-case-syntax-pair` table is installed, so a custom pair (e.g. `[`/`]`)
/// folds during search just as `char-equal` already does.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn buffer_case_canon_table(buf: &neovm_edit::buffer::Buffer) -> Option<Value> {
    buffer_case_canon_table_with_symbols(buf, SymbolMode::Bare)
}

pub(crate) fn buffer_case_canon_table_with_symbols(
    buf: &neovm_edit::buffer::Buffer,
    mode: SymbolMode,
) -> Option<Value> {
    use neovm_edit::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
    let table = buf.slots()[BUFFER_SLOT_CASE_TABLE.index()];
    // Standard table (by object identity): fold via the hardwired path.
    // Both invalid and valid standard tables return None, so their purpose
    // and extras do not need validation or collection dependencies here.
    let standard = STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow());
    if standard.is_some_and(|s| s.bits() == table.bits()) {
        return None;
    }
    if !is_case_table_with_symbols(&table, mode) {
        return None;
    }
    // Custom table: the canon subsidiary lives in extras[1] (derived by
    // `set-case-table`). Only use it if it is a usable char-table.
    let canon = case_table_extra(table, 1);
    super::chartable::is_char_table(&canon).then_some(canon)
}

/// Which subsidiary case char-table to consult for a per-buffer override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum CaseMap {
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
#[doc(hidden)]
pub enum CaseTableOverride {
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
    symbol_mode: SymbolMode,
    mutator: std::marker::PhantomData<*const ()>,
}

impl CaseTableOverride {
    /// Build an installed override from already-resolved subsidiary tables.
    ///
    /// Used by the core-side current-buffer override resolver: the evaluator
    /// phases select the tables, and this constructor keeps the private field
    /// layout in the text layer.
    #[doc(hidden)]
    pub fn from_installed_tables(
        down: Value,
        up: Value,
        canon: Value,
        symbol_mode: SymbolMode,
    ) -> Self {
        Self::Installed(InstalledCaseTables {
            down,
            up,
            canon,
            symbol_mode,
            mutator: std::marker::PhantomData,
        })
    }

    /// An override that never overrides — used by pure/test callers and the
    /// default path so the hardwired Unicode mapping is used verbatim.
    #[doc(hidden)]
    pub fn none() -> Self {
        Self::Standard
    }

    /// True when a non-standard case table is installed.
    #[doc(hidden)]
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Installed(_))
    }

    /// Read-only counterpart of [`Self::for_current_buffer`]: resolve the
    /// override straight from a buffer's installed case table (subsidiary tables
    /// already exist once `set-case-table` ran). Used where only an immutable
    /// buffer is in scope (replace-match case analysis).
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn for_buffer_readonly_with_symbols(
        buf: &neovm_edit::buffer::Buffer,
        mode: SymbolMode,
    ) -> Self {
        use neovm_edit::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
        let table = buf.slots()[BUFFER_SLOT_CASE_TABLE.index()];
        if !is_case_table_with_symbols(&table, mode) {
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
            symbol_mode: mode,
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
    #[doc(hidden)]
    pub fn map(&self, which: CaseMap, code: i64) -> Option<i64> {
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
            super::chartable::ct_lookup_with_symbols(&table, code, tables.symbol_mode)
                .ok()
                .and_then(fixnum_char)
                .unwrap_or(code),
        )
    }
}

fn is_case_table_subsidiary(table: &Value) -> bool {
    super::chartable::is_char_table(table)
}

#[doc(hidden)]
pub fn set_current_case_table_for_buffer_in_state(
    buffers: &mut neovm_edit::buffer::BufferManager,
    table: Value,
) -> Result<(), Flow> {
    use neovm_edit::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
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
    buf.slots_mut()[BUFFER_SLOT_CASE_TABLE.index()] = table;
    Ok(())
}

#[doc(hidden)]
pub fn case_table_extra(table: Value, idx: usize) -> Value {
    table
        .as_char_table_obj()
        .and_then(|obj| obj.extras.as_slice().get(idx).copied())
        .unwrap_or(Value::NIL)
}

fn set_case_table_extra(table: Value, idx: usize, value: Value) {
    let _ = table.with_char_table_mut(|obj| {
        obj.set_extra(idx, value);
    });
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn make_empty_case_table() -> Value {
    Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3)
}

fn fixnum_char(value: Value) -> Option<i64> {
    match value.kind() {
        ValueKind::Fixnum(n) if (0..=neovm_value::emacs_char::MAX_CHAR as i64).contains(&n) => {
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

fn set_canon(case_table: Value, key: Value, elt: Value, mode: SymbolMode) -> Result<(), Flow> {
    let Some(elt) = fixnum_char(elt) else {
        return Ok(());
    };
    let up = case_table_extra(case_table, 0);
    let canon = case_table_extra(case_table, 1);
    let up_elt = super::chartable::ct_lookup_with_symbols(&up, elt, mode)?;
    let Some(up_elt) = fixnum_char(up_elt) else {
        return Ok(());
    };
    let canonical = super::chartable::ct_lookup_with_symbols(&case_table, up_elt, mode)?;
    set_char_table_range_value(canon, key, canonical)
}

#[doc(hidden)]
pub fn ensure_case_table_derived_slots_with_symbols(
    table: Value,
    mode: SymbolMode,
) -> Result<(), Flow> {
    if !is_case_table_with_symbols(&table, mode) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("case-table-p"), table],
        ));
    }

    let mut up = case_table_extra(table, 0);
    if up.is_nil() {
        up = make_empty_case_table();
        map_case_table_with_symbols(table, |key, elt| set_identity(up, key, elt), mode)?;
        map_case_table_with_symbols(table, |key, elt| shuffle(up, key, elt), mode)?;
        set_case_table_extra(table, 0, up);
    }

    let mut canon = case_table_extra(table, 1);
    if canon.is_nil() {
        canon = make_empty_case_table();
        set_case_table_extra(table, 1, canon);
        map_case_table_with_symbols(table, |key, elt| set_canon(table, key, elt, mode), mode)?;
    }

    let mut eqv = case_table_extra(table, 2);
    if eqv.is_nil() {
        eqv = make_empty_case_table();
        map_case_table_with_symbols(canon, |key, elt| set_identity(eqv, key, elt), mode)?;
        map_case_table_with_symbols(canon, |key, elt| shuffle(eqv, key, elt), mode)?;
        set_case_table_extra(table, 2, eqv);
    }
    set_case_table_extra(canon, 2, eqv);

    Ok(())
}

/// Return `true` if `v` is a case table (GNU `Fcase_table_p`): a char-table
/// with the `case-table` purpose whose up/canon/eqv extras are nil or
/// char-tables, with eqv present only alongside canon.
pub fn is_case_table(v: &Value) -> bool {
    is_case_table_with_symbols(v, SymbolMode::Bare)
}

pub fn is_case_table_with_symbols(v: &Value, mode: SymbolMode) -> bool {
    use super::chartable::is_char_table;
    let Some(obj) = v.as_char_table_obj() else {
        return false;
    };
    let purpose = case_table_sym_id();
    // GNU EQ accepts identical bits before consulting positioned-symbol policy.
    let purpose_matches = obj.purpose.get().bits() == Value::from_sym_id(purpose).bits()
        || obj.purpose.get().as_symbol_id(mode) == Some(purpose);
    if !purpose_matches || obj.extras.len() < 3 {
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

#[doc(hidden)]
pub fn builtin_case_table_p_in_state<H: super::text_host::TextHost>(
    eval: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    builtin_case_table_p_with_symbols(args, eval.symbol_mode())
}

fn map_case_table_with_symbols(
    table: Value,
    f: impl FnMut(Value, Value) -> Result<(), Flow>,
    mode: SymbolMode,
) -> Result<(), Flow> {
    super::chartable::for_each_char_table_mapping_with_symbols(&table, f, mode)
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix(h, STANDARD_CASE_TABLE_HEAP.with(|slot| slot.get() as u64));
    neovm_value::tls_fingerprint::mix(
        h,
        STANDARD_CASE_TABLE_OBJECT.with(|slot| slot.borrow().is_some() as u64),
    );
}

#[cfg(test)]
#[path = "tests/casetab_test.rs"]
mod tests;
