//! Interactive command registry, specs and planning data (stage-4 W1/E4).
//!
//! The registry/spec substrate is evaluator data: GNU keeps the interactive
//! forms in function cells and evaluates them at `call-interactively`. Moved
//! out of the core interactive command module so the evaluator can own it;
//! the argument-acquisition entry bodies stay in core (they bind minibuffer
//! and reader behaviour) and reach this module downward.

use super::keymap_lookup::{KeymapMutationEpoch, is_list_keymap, keymap_mutation_epoch};
use super::*;
use neovm_value::emacs_char::to_utf8_lossy;
use neovm_value::tagged::value::SymbolMode;
use std::collections::HashMap;

/// GNU's predeclared `Qinteractive_form` identity.
///
/// Both the `interactive-form` property key and the function designator are
/// the same canonical symbol. Keeping that fact behind a zero-sized type
/// prevents hot command classification from falling back to string interning,
/// while `id` versus `value` makes the required representation explicit at
/// each call site.
pub struct InteractiveFormSymbol;

impl InteractiveFormSymbol {
    #[inline(always)]
    #[doc(hidden)]
    pub fn id() -> SymId {
        static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
        if let Some(id) = SYMBOL.get() {
            *id
        } else {
            *SYMBOL.get_or_init(|| intern("interactive-form"))
        }
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn value() -> Value {
        Value::from_sym_id(Self::id())
    }
}

/// Interactive argument specification for a command.
#[derive(Clone, Debug)]
pub struct InteractiveSpec {
    /// GNU-style SPEC payload from `(interactive SPEC)`.
    pub spec: Value,
}

impl InteractiveSpec {
    /// Create a new interactive spec from a code string.
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            spec: Value::string(code.into()),
        }
    }

    /// Create a spec directly from a Lisp value.
    pub fn from_value(spec: Value) -> Self {
        Self { spec }
    }

    /// Create a spec with no arguments (plain interactive command).
    pub fn no_args() -> Self {
        Self { spec: Value::NIL }
    }

    pub fn string_code_runtime_owned(&self) -> Option<String> {
        match self.spec.kind() {
            ValueKind::Nil => Some(String::new()),
            ValueKind::String => self
                .spec
                .as_lisp_string()
                .map(|ls| to_utf8_lossy(ls.as_bytes())),
            _ => None,
        }
    }
}

/// Static interactive metadata attached to a registered Rust subr.
///
/// GNU stores this beside arity and the function pointer in `Lisp_Subr`.
/// Representing control strings and Lisp forms as distinct states prevents
/// command validation from drifting away from argument preparation.
///
/// There is deliberately no "no arguments" variant: a GNU `DEFUN`'s intspec is
/// a string or a Lisp form and never nil, so `(interactive-form 'recursive-edit)`
/// is `(interactive "")`, not `(interactive nil)`.  The variant that used to be
/// here existed for `ignore` alone, which is `lisp/subr.el:501` and not a subr
/// at all (DIVERGENCES.md 152).
#[derive(Clone, Copy, Debug)]
pub enum BuiltinInteractiveSpec {
    String(&'static str),
    Form(fn() -> Value),
}

impl BuiltinInteractiveSpec {
    #[doc(hidden)]
    pub fn into_spec_value(self) -> Value {
        match self {
            Self::String(code) => Value::string(code),
            Self::Form(build) => build(),
        }
    }

    #[doc(hidden)]
    pub fn into_interactive_form(self) -> Value {
        interactive_form_from_spec_value(self.into_spec_value())
    }
}

/// The keymap state from which a reverse index was derived.
///
/// GNU compares the active keymap list with `equal` and separately flushes its
/// cache at keymap mutation chokepoints. Storing both inputs makes stale reuse
/// unrepresentable through this interface.
pub struct WhereIsKeymapState {
    mutation_epoch: KeymapMutationEpoch,
    keymaps: Vec<Value>,
}

impl WhereIsKeymapState {
    pub fn new(keymaps: &[Value]) -> Self {
        Self {
            mutation_epoch: keymap_mutation_epoch(),
            keymaps: keymaps.to_vec(),
        }
    }

    #[doc(hidden)]
    pub fn matches(&self, keymaps: &[Value]) -> bool {
        self.mutation_epoch == keymap_mutation_epoch() && self.keymaps == keymaps
    }
}

/// Definition identity used by GNU's reverse-keymap cache.
///
/// A symbol and its canonical subr denote the same command even though their
/// Lisp representations differ. Other definitions retain Lisp `equal`
/// semantics through `Value`'s `Eq`/`Hash` implementation.
#[derive(Clone, Hash, Eq, PartialEq)]
pub enum WhereIsDefinitionKey {
    Command(SymId),
    Equal(Value),
}

impl WhereIsDefinitionKey {
    pub fn from_value(value: Value) -> Option<Self> {
        if value.is_nil() || is_list_keymap(&value, SymbolMode::Bare) {
            return None;
        }
        if let Some(symbol) = value.as_bare_symbol_id().or_else(|| value.as_subr_id()) {
            return Some(Self::Command(symbol));
        }
        Some(Self::Equal(value))
    }

    #[doc(hidden)]
    pub fn trace_roots_with(&self, visit: &mut (impl FnMut(Value) + ?Sized)) {
        if let Self::Equal(value) = self
            && value.is_heap_object()
        {
            visit(*value);
        }
    }
}

/// Reverse lookup for every definition reachable from one active keymap set.
///
/// This is deliberately a complete index rather than a per-command memo: an
/// empty M-x affixation asks about thousands of distinct commands exactly
/// once, so per-command caching would leave its first invocation quadratic.
pub struct WhereIsReverseIndex {
    state: WhereIsKeymapState,
    sequences_by_definition: HashMap<WhereIsDefinitionKey, Vec<Vec<Value>>>,
    remapping_by_command: HashMap<SymId, Value>,
    /// The heap objects this index keeps alive, collected once at build time.
    ///
    /// The index is a complete reverse map of every accessible binding, so
    /// enumerating its roots means walking every key sequence of every
    /// definition -- tens of thousands of events, nearly all of which are
    /// integers and so not roots at all.  Root seeding runs twice per
    /// collection cycle, and this index is replaced wholesale rather than
    /// mutated, so the answer is the same every time it is asked.  Computing
    /// it with the index turns that repeated scan into a walk of the few
    /// values that are actually roots.
    traced_roots: Vec<Value>,
}

impl WhereIsReverseIndex {
    /// Rebuild an index from its scanned parts (core's where-is builder).
    pub fn from_parts(
        state: WhereIsKeymapState,
        sequences_by_definition: HashMap<WhereIsDefinitionKey, Vec<Vec<Value>>>,
        remapping_by_command: HashMap<SymId, Value>,
    ) -> Self {
        let traced_roots =
            Self::collect_traced_roots(&state, &sequences_by_definition, &remapping_by_command);
        Self {
            state,
            sequences_by_definition,
            remapping_by_command,
            traced_roots,
        }
    }

    #[doc(hidden)]
    pub fn sequences_for(&self, definition: Value) -> Vec<Vec<Value>> {
        WhereIsDefinitionKey::from_value(definition)
            .and_then(|key| self.sequences_by_definition.get(&key))
            .cloned()
            .unwrap_or_default()
    }

    pub fn remapping_for(&self, command: SymId) -> Option<Value> {
        self.remapping_by_command.get(&command).copied()
    }

    /// Every heap object reachable from the index, in one pass over it.
    ///
    /// Kept beside `trace_roots_with` so the two cannot drift: this is the
    /// definition of the index's root set, and the collector reads the vector
    /// this produces.
    pub fn collect_traced_roots(
        state: &WhereIsKeymapState,
        sequences_by_definition: &HashMap<WhereIsDefinitionKey, Vec<Vec<Value>>>,
        remapping_by_command: &HashMap<SymId, Value>,
    ) -> Vec<Value> {
        let mut roots = Vec::new();
        roots.extend(state.keymaps.iter().copied());
        for (definition, sequences) in sequences_by_definition {
            definition.trace_roots_with(&mut |root| roots.push(root));
            for event in sequences.iter().flatten() {
                if event.is_heap_object() {
                    roots.push(*event);
                }
            }
        }
        for remapping in remapping_by_command.values() {
            if remapping.is_heap_object() {
                roots.push(*remapping);
            }
        }
        roots.shrink_to_fit();
        roots
    }

    #[doc(hidden)]
    pub fn trace_roots_with(&self, visit: &mut (impl FnMut(Value) + ?Sized)) {
        for root in &self.traced_roots {
            visit(*root);
        }
    }
}

/// Registry for interactive command specifications.
///
/// Tracks which function symbols are interactive (i.e., can be called via
/// `M-x` or key bindings) and their argument specs.
pub struct InteractiveRegistry {
    specs: HashMap<SymId, InteractiveSpec>,
    heap_specs: HashMap<SymId, Value>,
    /// Stack tracking whether the current function was called interactively.
    interactive_call_stack: Vec<bool>,
    /// GNU-shaped lazy reverse-keymap index used by menu-free
    /// `where-is-internal` lookups.
    where_is_reverse_index: Option<WhereIsReverseIndex>,
    /// Number of complete reverse-keymap scans performed by
    /// `where-is-internal` in this evaluator.
    ///
    /// Test-only instrumentation makes the performance contract observable
    /// without depending on wall-clock timing.
    #[cfg(any(test, feature = "test-support"))]
    where_is_reverse_index_build_count: usize,
    /// Number of command-remapping queries answered by the reverse index.
    #[cfg(any(test, feature = "test-support"))]
    where_is_reverse_index_remapping_lookup_count: usize,
}

impl InteractiveRegistry {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            heap_specs: HashMap::new(),
            interactive_call_stack: Vec::new(),
            where_is_reverse_index: None,
            #[cfg(any(test, feature = "test-support"))]
            where_is_reverse_index_build_count: 0,
            #[cfg(any(test, feature = "test-support"))]
            where_is_reverse_index_remapping_lookup_count: 0,
        }
    }

    /// Register a function symbol as interactive with the given spec.
    pub fn register_interactive(&mut self, symbol: SymId, spec: InteractiveSpec) {
        if spec.spec.is_heap_object() {
            self.heap_specs.insert(symbol, spec.spec);
        } else {
            // A re-registration can replace a heap spec with a non-heap one.
            self.heap_specs.remove(&symbol);
        }
        self.specs.insert(symbol, spec);
    }

    pub fn unregister_interactive(&mut self, symbol: SymId) {
        self.heap_specs.remove(&symbol);
        self.specs.remove(&symbol);
    }

    /// Check if a function symbol is registered as interactive.
    pub fn is_interactive(&self, symbol: SymId) -> bool {
        self.specs.contains_key(&symbol)
    }

    /// Get the interactive spec for a function symbol, if registered.
    pub fn get_spec(&self, symbol: SymId) -> Option<&InteractiveSpec> {
        self.specs.get(&symbol)
    }

    /// Push an interactive call frame.
    pub fn push_interactive_call(&mut self, is_interactive: bool) {
        self.interactive_call_stack.push(is_interactive);
    }

    /// Pop an interactive call frame.
    pub fn pop_interactive_call(&mut self) {
        self.interactive_call_stack.pop();
    }

    /// Check if the current function was called interactively.
    pub fn is_called_interactively(&self) -> bool {
        self.interactive_call_stack.last().copied().unwrap_or(false)
    }

    // pdump accessors
    #[doc(hidden)]
    pub fn dump_specs(&self) -> &HashMap<SymId, InteractiveSpec> {
        &self.specs
    }

    /// Borrowed heap-root subset of the spec map (peer #1005 field
    /// remediation): the field is `pub(crate)` in every arm and this is the
    /// read surface for the moved interactive-registry oracle.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    pub(crate) fn heap_specs_for_test(&self) -> &HashMap<SymId, Value> {
        &self.heap_specs
    }
    #[doc(hidden)]
    pub fn from_dump(specs: HashMap<SymId, InteractiveSpec>) -> Self {
        let heap_specs = specs
            .iter()
            .filter(|(_, spec)| spec.spec.is_heap_object())
            .map(|(symbol, spec)| (*symbol, spec.spec))
            .collect();
        Self {
            specs,
            heap_specs,
            interactive_call_stack: Vec::new(),
            where_is_reverse_index: None,
            #[cfg(any(test, feature = "test-support"))]
            where_is_reverse_index_build_count: 0,
            #[cfg(any(test, feature = "test-support"))]
            where_is_reverse_index_remapping_lookup_count: 0,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub(crate) fn note_where_is_reverse_index_build(&mut self) {
        self.where_is_reverse_index_build_count += 1;
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn where_is_reverse_index_build_count(&self) -> usize {
        self.where_is_reverse_index_build_count
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn where_is_reverse_index_remapping_lookup_count(&self) -> usize {
        self.where_is_reverse_index_remapping_lookup_count
    }

    pub fn cached_where_is_sequences(
        &self,
        keymaps: &[Value],
        definition: Value,
    ) -> Option<Vec<Vec<Value>>> {
        self.cached_where_is_reverse_index(keymaps)
            .map(|index| index.sequences_for(definition))
    }

    pub fn cached_where_is_reverse_index(&self, keymaps: &[Value]) -> Option<&WhereIsReverseIndex> {
        self.where_is_reverse_index
            .as_ref()
            .filter(|index| index.state.matches(keymaps))
    }

    pub fn install_where_is_reverse_index(&mut self, index: WhereIsReverseIndex) {
        self.where_is_reverse_index = Some(index);
        #[cfg(any(test, feature = "test-support"))]
        self.note_where_is_reverse_index_build();
    }

    pub fn clear_where_is_reverse_index(&mut self) {
        self.where_is_reverse_index = None;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn note_where_is_reverse_index_remapping_lookup(&mut self) {
        self.where_is_reverse_index_remapping_lookup_count += 1;
    }

    #[doc(hidden)]
    pub fn trace_roots_with(&self, visit: &mut (impl FnMut(Value) + ?Sized)) {
        for spec in self.heap_specs.values() {
            visit(*spec);
        }
        if let Some(index) = &self.where_is_reverse_index {
            index.trace_roots_with(visit);
        }
    }
}

impl Default for InteractiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn interactive_form_from_spec_value(spec: Value) -> Value {
    Value::list(vec![
        Value::symbol(neovm_value::intern::WellKnown::Interactive),
        spec,
    ])
}

pub fn registry_interactive_form(registry: &InteractiveRegistry, symbol: SymId) -> Option<Value> {
    registry
        .get_spec(symbol)
        .map(|spec| interactive_form_from_spec_value(spec.spec))
}

pub fn registered_builtin_interactive_spec(symbol: SymId) -> Option<BuiltinInteractiveSpec> {
    super::lookup_global_subr_entry(symbol).and_then(|entry| entry.interactive_spec)
}

pub fn registered_builtin_interactive_form(symbol: SymId) -> Option<Value> {
    registered_builtin_interactive_spec(symbol).map(BuiltinInteractiveSpec::into_interactive_form)
}

pub fn sync_interactive_registry_for_symbol_definition(
    interactive: &mut InteractiveRegistry,
    symbol: SymId,
    definition: Value,
    symbol_mode: SymbolMode,
) {
    // The mutable registry owns Lisp definitions whose command identity is not
    // part of their function object (notably interactive autoloads). Rust
    // subrs keep their immutable command contract in `SubrEntry`; copying it
    // here would create a second source of truth and unnecessarily serialize
    // heap-backed interactive forms into pdump images.
    let replacement = if value_is_interactive_autoload(&definition, symbol_mode) {
        Some(InteractiveSpec::no_args())
    } else {
        None
    };

    if let Some(spec) = replacement {
        interactive.register_interactive(symbol, spec);
    } else {
        interactive.unregister_interactive(symbol);
    }
}

pub fn expect_optional_command_keys_vector(keys: Option<&Value>) -> Result<(), Flow> {
    if let Some(keys_value) = keys
        && !keys_value.is_nil()
        && !keys_value.is_vector()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), *keys_value],
        ));
    }
    Ok(())
}

pub fn validate_call_interactively_args(args: &[Value]) -> Result<(), Flow> {
    expect_min_args("call-interactively", args, 1)?;
    expect_max_args("call-interactively", args, 3)?;
    expect_optional_command_keys_vector(args.get(2))
}

pub fn value_list_to_vec(list: &Value) -> Option<Vec<Value>> {
    let mut values = Vec::new();
    let mut cursor = *list;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Some(values),
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                values.push(pair_car);
                cursor = pair_cdr;
            }
            _ => return None,
        }
    }
}

pub fn value_is_interactive_autoload(value: &Value, symbol_mode: SymbolMode) -> bool {
    if !crate::emacs_core::autoload::is_autoload_value(value, symbol_mode) {
        return false;
    }
    let Some(items) = value_list_to_vec(value) else {
        return false;
    };
    items.get(3).is_some_and(|v| !v.is_nil())
}
