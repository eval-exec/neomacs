//! Interactive command system.
//!
//! Implements:
//! - `InteractiveSpec` and `InteractiveRegistry` for tracking which functions
//!   are interactive commands and their argument specifications.
//! - Built-in functions: `call-interactively`, `interactive-p`,
//!   `called-interactively-p`, `commandp`,
//!   `key-binding`, `local-key-binding`,
//!   `minor-mode-key-binding`, `where-is-internal`,
//!   `describe-key-briefly`, `this-command-keys`,
//!   `this-command-keys-vector`, `thing-at-point`, `bounds-of-thing-at-point`,
//!   `symbol-at-point`.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use std::collections::HashMap;

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{intern, resolve_sym};
use super::keyboard::pure::{
    KEY_CHAR_ALT, KEY_CHAR_CTRL, KEY_CHAR_HYPER, KEY_CHAR_META, KEY_CHAR_MOD_MASK, KEY_CHAR_SHIFT,
    KEY_CHAR_SUPER, make_event_array_value,
};
use super::keymap::{
    DefaultBindingMode, KeymapMarker, KeymapMutationEpoch,
    command_remapping_command_name as keymap_command_remapping_command_name,
    command_remapping_lookup_in_keymaps as keymap_command_remapping_lookup_in_keymaps,
    command_remapping_lookup_in_lisp_keymap as keymap_command_remapping_lookup_in_lisp_keymap,
    command_remapping_normalize_target as keymap_command_remapping_normalize_target,
    current_active_maps_for_position, current_active_maps_for_position_read_only, get_keyelt,
    get_keymap_in_obarray, is_list_keymap, key_binding_apply_remap_in_active_maps,
    key_event_to_emacs_event, keymap_mutation_epoch, list_keymap_for_each_binding,
    lookup_key_in_keymaps_in_obarray_runtime, lookup_keymap_with_partial,
    minor_mode_key_binding_in_context, resolve_active_key_binding, where_is_keymaps_in_context,
};
use super::symbol::Obarray;
use super::value::*;
use crate::buffer::EmacsBytePos;
use crate::emacs_core::SymId;

/// GNU's predeclared `Qinteractive_form` identity.
///
/// Both the `interactive-form` property key and the function designator are
/// the same canonical symbol. Keeping that fact behind a zero-sized type
/// prevents hot command classification from falling back to string interning,
/// while `id` versus `value` makes the required representation explicit at
/// each call site.
pub(crate) struct InteractiveFormSymbol;

impl InteractiveFormSymbol {
    #[inline(always)]
    pub(crate) fn id() -> SymId {
        static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
        if let Some(id) = SYMBOL.get() {
            *id
        } else {
            *SYMBOL.get_or_init(|| intern("interactive-form"))
        }
    }

    #[inline(always)]
    pub(crate) fn value() -> Value {
        Value::from_sym_id(Self::id())
    }
}

/// GNU's predeclared `Qcommandp` identity.
///
/// Completion compares its predicate with this exact symbol before consulting
/// the symbol's function cell.  Exposing the identity through a zero-sized
/// type keeps that primitive dispatch distinct from an arbitrary callable
/// whose printed name happens to be `commandp`.
pub(crate) struct CommandpSymbol;

impl CommandpSymbol {
    #[inline(always)]
    pub(crate) fn id() -> SymId {
        static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
        if let Some(id) = SYMBOL.get() {
            *id
        } else {
            *SYMBOL.get_or_init(|| intern("commandp"))
        }
    }
}

// ---------------------------------------------------------------------------
// InteractiveSpec — describes how a command reads its arguments
// ---------------------------------------------------------------------------

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
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())),
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
pub(crate) enum BuiltinInteractiveSpec {
    String(&'static str),
    Form(fn() -> Value),
}

impl BuiltinInteractiveSpec {
    fn into_spec_value(self) -> Value {
        match self {
            Self::String(code) => Value::string(code),
            Self::Form(build) => build(),
        }
    }

    fn into_interactive_form(self) -> Value {
        interactive_form_from_spec_value(self.into_spec_value())
    }
}

// ---------------------------------------------------------------------------
// InteractiveRegistry — tracks which functions are interactive commands
// ---------------------------------------------------------------------------

/// The keymap state from which a reverse index was derived.
///
/// GNU compares the active keymap list with `equal` and separately flushes its
/// cache at keymap mutation chokepoints. Storing both inputs makes stale reuse
/// unrepresentable through this interface.
struct WhereIsKeymapState {
    mutation_epoch: KeymapMutationEpoch,
    keymaps: Vec<Value>,
}

impl WhereIsKeymapState {
    fn new(keymaps: &[Value]) -> Self {
        Self {
            mutation_epoch: keymap_mutation_epoch(),
            keymaps: keymaps.to_vec(),
        }
    }

    fn matches(&self, keymaps: &[Value]) -> bool {
        self.mutation_epoch == keymap_mutation_epoch() && self.keymaps == keymaps
    }
}

/// Definition identity used by GNU's reverse-keymap cache.
///
/// A symbol and its canonical subr denote the same command even though their
/// Lisp representations differ. Other definitions retain Lisp `equal`
/// semantics through `Value`'s `Eq`/`Hash` implementation.
#[derive(Clone, Hash, Eq, PartialEq)]
enum WhereIsDefinitionKey {
    Command(SymId),
    Equal(Value),
}

impl WhereIsDefinitionKey {
    fn from_value(value: Value) -> Option<Self> {
        if value.is_nil() || is_list_keymap(&value) {
            return None;
        }
        if let Some(symbol) = value.as_symbol_id().or_else(|| value.as_subr_id()) {
            return Some(Self::Command(symbol));
        }
        Some(Self::Equal(value))
    }

    fn trace_roots_with(&self, visit: &mut (impl FnMut(Value) + ?Sized)) {
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
struct WhereIsReverseIndex {
    state: WhereIsKeymapState,
    sequences_by_definition: HashMap<WhereIsDefinitionKey, Vec<Vec<Value>>>,
    remapping_by_command: HashMap<SymId, Value>,
}

impl WhereIsReverseIndex {
    fn sequences_for(&self, definition: Value) -> Vec<Vec<Value>> {
        WhereIsDefinitionKey::from_value(definition)
            .and_then(|key| self.sequences_by_definition.get(&key))
            .cloned()
            .unwrap_or_default()
    }

    fn remapping_for(&self, command: SymId) -> Option<Value> {
        self.remapping_by_command.get(&command).copied()
    }

    fn trace_roots_with(&self, visit: &mut (impl FnMut(Value) + ?Sized)) {
        for keymap in &self.state.keymaps {
            visit(*keymap);
        }
        for (definition, sequences) in &self.sequences_by_definition {
            definition.trace_roots_with(visit);
            for event in sequences.iter().flatten() {
                if event.is_heap_object() {
                    visit(*event);
                }
            }
        }
        for remapping in self.remapping_by_command.values() {
            if remapping.is_heap_object() {
                visit(*remapping);
            }
        }
    }
}

/// Registry for interactive command specifications.
///
/// Tracks which function symbols are interactive (i.e., can be called via
/// `M-x` or key bindings) and their argument specs.
pub struct InteractiveRegistry {
    /// Map from function symbol to its interactive spec.
    specs: HashMap<SymId, InteractiveSpec>,
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
    #[cfg(test)]
    where_is_reverse_index_build_count: usize,
    /// Number of command-remapping queries answered by the reverse index.
    #[cfg(test)]
    where_is_reverse_index_remapping_lookup_count: usize,
}

impl InteractiveRegistry {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            interactive_call_stack: Vec::new(),
            where_is_reverse_index: None,
            #[cfg(test)]
            where_is_reverse_index_build_count: 0,
            #[cfg(test)]
            where_is_reverse_index_remapping_lookup_count: 0,
        }
    }

    /// Register a function symbol as interactive with the given spec.
    pub fn register_interactive(&mut self, symbol: SymId, spec: InteractiveSpec) {
        self.specs.insert(symbol, spec);
    }

    pub fn unregister_interactive(&mut self, symbol: SymId) {
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
    pub(crate) fn dump_specs(&self) -> &HashMap<SymId, InteractiveSpec> {
        &self.specs
    }
    pub(crate) fn from_dump(specs: HashMap<SymId, InteractiveSpec>) -> Self {
        Self {
            specs,
            interactive_call_stack: Vec::new(),
            where_is_reverse_index: None,
            #[cfg(test)]
            where_is_reverse_index_build_count: 0,
            #[cfg(test)]
            where_is_reverse_index_remapping_lookup_count: 0,
        }
    }

    #[cfg(test)]
    fn note_where_is_reverse_index_build(&mut self) {
        self.where_is_reverse_index_build_count += 1;
    }

    #[cfg(test)]
    fn where_is_reverse_index_build_count(&self) -> usize {
        self.where_is_reverse_index_build_count
    }

    #[cfg(test)]
    fn where_is_reverse_index_remapping_lookup_count(&self) -> usize {
        self.where_is_reverse_index_remapping_lookup_count
    }

    fn cached_where_is_sequences(
        &self,
        keymaps: &[Value],
        definition: Value,
    ) -> Option<Vec<Vec<Value>>> {
        self.cached_where_is_reverse_index(keymaps)
            .map(|index| index.sequences_for(definition))
    }

    fn cached_where_is_reverse_index(&self, keymaps: &[Value]) -> Option<&WhereIsReverseIndex> {
        self.where_is_reverse_index
            .as_ref()
            .filter(|index| index.state.matches(keymaps))
    }

    fn install_where_is_reverse_index(&mut self, index: WhereIsReverseIndex) {
        self.where_is_reverse_index = Some(index);
        #[cfg(test)]
        self.note_where_is_reverse_index_build();
    }

    fn clear_where_is_reverse_index(&mut self) {
        self.where_is_reverse_index = None;
    }

    #[cfg(test)]
    fn note_where_is_reverse_index_remapping_lookup(&mut self) {
        self.where_is_reverse_index_remapping_lookup_count += 1;
    }

    pub(crate) fn trace_roots_with(&self, visit: &mut (impl FnMut(Value) + ?Sized)) {
        for spec in self.specs.values() {
            if spec.spec.is_heap_object() {
                visit(spec.spec);
            }
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

fn interactive_form_from_spec_value(spec: Value) -> Value {
    Value::list(vec![Value::symbol("interactive"), spec])
}

pub(crate) fn registry_interactive_form(
    registry: &InteractiveRegistry,
    symbol: SymId,
) -> Option<Value> {
    registry
        .get_spec(symbol)
        .map(|spec| interactive_form_from_spec_value(spec.spec))
}

fn registered_builtin_interactive_spec(symbol: SymId) -> Option<BuiltinInteractiveSpec> {
    super::eval::lookup_global_subr_entry(symbol).and_then(|entry| entry.interactive_spec)
}

pub(crate) fn registered_builtin_interactive_form(symbol: SymId) -> Option<Value> {
    registered_builtin_interactive_spec(symbol).map(BuiltinInteractiveSpec::into_interactive_form)
}

pub(crate) fn sync_interactive_registry_for_symbol_definition(
    interactive: &mut InteractiveRegistry,
    symbol: SymId,
    definition: Value,
) {
    // The mutable registry owns Lisp definitions whose command identity is not
    // part of their function object (notably interactive autoloads). Rust
    // subrs keep their immutable command contract in `SubrEntry`; copying it
    // here would create a second source of truth and unnecessarily serialize
    // heap-backed interactive forms into pdump images.
    let replacement = if value_is_interactive_autoload(&definition) {
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

// ---------------------------------------------------------------------------
// Expect helpers (local to this module)
// ---------------------------------------------------------------------------

fn expect_optional_command_keys_vector(keys: Option<&Value>) -> Result<(), Flow> {
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

pub(crate) fn validate_call_interactively_args(args: &[Value]) -> Result<(), Flow> {
    expect_min_args("call-interactively", args, 1)?;
    expect_max_args("call-interactively", args, 3)?;
    expect_optional_command_keys_vector(args.get(2))
}

// ---------------------------------------------------------------------------
// Built-in functions (evaluator-dependent)
// ---------------------------------------------------------------------------

/// `(call-interactively FUNCTION &optional RECORD-FLAG KEYS)`
/// Call FUNCTION interactively, reading arguments according to its
/// interactive spec.
pub(crate) fn builtin_call_interactively(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    validate_call_interactively_args(&args)?;
    let func_val = args[0];
    // GNU Fcall_interactively snapshots command identity before even asking
    // for the interactive form.  Argument acquisition may recursively read
    // input or run arbitrary Lisp; none of those temporary command values
    // belong to the command that will ultimately be invoked.
    let command_identity = CallInteractivelyCommandIdentity::capture(eval);
    let root_scope = eval.save_specpdl_roots();
    command_identity.push_gc_roots(eval);
    // GNU callint.c obtains the interactive form before deciding whether the
    // function is a command.  This is observable for autoloads: loading and
    // any load error must happen before a possible `commandp` signal.
    let result = (|| {
        let interactive_form = eval.apply(InteractiveFormSymbol::value(), vec![func_val])?;
        let plan = plan_call_interactively_after_interactive_form_in_state(
            &eval.obarray,
            eval.read_command_keys(),
            &args,
            interactive_form,
            command_identity,
        )?;
        finish_call_interactively_in_eval(eval, plan)
    })();
    eval.restore_specpdl_roots(root_scope);
    result
}

/// `(interactive-p)` -> t if the calling function was called interactively.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_interactive_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("interactive-p", &args, 0)?;
    let _ = eval;
    // Emacs 30 keeps `interactive-p` obsolete; it effectively returns nil.
    Ok(Value::NIL)
}

/// `(called-interactively-p &optional KIND)`
/// Return t if the calling function was called interactively.
/// KIND can be 'interactive or 'any.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_called_interactively_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    // Accept 0 or 1 args
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("called-interactively-p"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if !eval.interactive.is_called_interactively() {
        return Ok(Value::NIL);
    }

    // GNU Emacs semantics:
    // - KIND = 'interactive => nil
    // - KIND = nil / 'any / unknown => t (when called interactively)
    if args
        .first()
        .is_some_and(|v| v.is_symbol_named("interactive"))
    {
        Ok(Value::NIL)
    } else {
        Ok(Value::T)
    }
}

/// `(commandp FUNCTION &optional FOR-CALL-INTERACTIVELY)`
/// Return non-nil if FUNCTION is a command (i.e., can be called interactively).
///
/// Matches GNU Emacs eval.c:Fcommandp. Resolves a symbol designator once, then
/// classifies whether GNU returns immediately, checks the original symbol
/// chain's property, or performs generic interactive-form dispatch.
pub(crate) fn builtin_commandp_interactive(eval: &mut Context, args: &[Value]) -> EvalResult {
    expect_min_args("commandp", args, 1)?;
    expect_max_args("commandp", args, 2)?;
    let for_call_interactively = args.get(1).is_some_and(|value| !value.is_nil());

    let classification =
        classify_command_designator_in_state(&eval.obarray, &args[0], for_call_interactively);
    let fallback = match classification {
        CommandpClassification::Interactive => return Ok(Value::T),
        CommandpClassification::Reject => return Ok(Value::NIL),
        CommandpClassification::CheckInteractiveFormProperty(fallback) => fallback,
    };

    // GNU Emacs eval.c:Fcommandp checks `interactive-form' properties after
    // ordinary checks fail for a callable object. The property is not accepted
    // as making a function interactive; it is a hard error. Invalid scalars,
    // rejected keyboard macros, and non-lambda lists return before this walk.
    let mut fun = args[0];
    while let Some(symbol) = fun.as_symbol_id() {
        if eval
            .obarray
            .get_property_id(symbol, InteractiveFormSymbol::id())
            .is_some_and(|value| !value.is_nil())
        {
            return Err(signal(
                "error",
                vec![Value::string("Found an 'interactive-form' property!")],
            ));
        }
        let Some(next) = crate::emacs_core::builtins::symbols::symbol_function_cell_in_obarray(
            &eval.obarray,
            symbol,
        ) else {
            return Ok(Value::NIL);
        };
        fun = next;
    }

    // GNU only calls `interactive-form' here for the generic-function path
    // selected while classifying an invalid closure doc slot.
    if let InteractiveFormFallback::GenericDispatch(resolved_function) = fallback {
        let iform = eval.apply(InteractiveFormSymbol::value(), vec![resolved_function])?;
        if !iform.is_nil() {
            return Ok(Value::T);
        }
    }

    Ok(Value::NIL)
}

fn unquote_command_modes_value(value: Value) -> Value {
    let Some(items) = value_list_to_vec(&value) else {
        return value;
    };
    if items.len() == 2 && items[0].as_symbol_name() == Some("quote") {
        items[1]
    } else {
        value
    }
}

fn command_modes_from_stored_interactive_spec(spec: Value) -> Value {
    let Some(items) = spec.as_vector_data() else {
        return Value::NIL;
    };
    items
        .get(1)
        .copied()
        .map(unquote_command_modes_value)
        .unwrap_or(Value::NIL)
}

fn command_modes_from_quoted_interactive_form(form: &Value) -> Result<Option<Value>, Flow> {
    if !form.is_cons() {
        return Ok(None);
    };
    let pair_car = form.cons_car();
    let pair_cdr = form.cons_cdr();
    if pair_car.as_symbol_name() != Some("interactive") {
        return Ok(None);
    }

    match pair_cdr.kind() {
        ValueKind::Nil => Ok(Some(Value::NIL)),
        ValueKind::Cons => {
            let _arg_pair_car = pair_cdr.cons_car();
            let arg_pair_cdr = pair_cdr.cons_cdr();
            Ok(Some(arg_pair_cdr))
        }
        _tail => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), pair_cdr],
        )),
    }
}

fn command_modes_from_quoted_lambda(value: &Value) -> Result<Option<Value>, Flow> {
    let Some(items) = value_list_to_vec(value) else {
        return Ok(None);
    };
    if items.first().and_then(|v| v.as_symbol_name()) != Some("lambda") {
        return Ok(None);
    }

    let mut body_index = 2;
    if items.get(body_index).is_some_and(|v| v.is_string()) {
        body_index += 1;
    }
    while items.get(body_index).is_some_and(value_is_declare_form) {
        body_index += 1;
    }

    for form in &items[body_index..] {
        if let Some(modes) = command_modes_from_quoted_interactive_form(form)? {
            return Ok(Some(modes));
        }
    }

    Ok(None)
}

pub(crate) fn builtin_command_modes_impl(obarray: &Obarray, args: &[Value]) -> EvalResult {
    expect_args("command-modes", args, 1)?;
    let command = args[0];
    let mut function = command;

    if let Some(mut current) = crate::emacs_core::builtins::symbols::symbol_id(&command) {
        let Some((_, indirect_function)) =
            crate::emacs_core::builtins::symbols::resolve_indirect_symbol_by_id_in_obarray(
                obarray, current,
            )
        else {
            return Ok(Value::NIL);
        };
        if indirect_function.is_nil() {
            return Ok(Value::NIL);
        }

        loop {
            if let Some(modes) = obarray
                .get_property_id(current, intern("command-modes"))
                .filter(|value| !value.is_nil())
            {
                return Ok(modes);
            }
            let Some(next_function) =
                crate::emacs_core::builtins::symbols::symbol_function_cell_in_obarray(
                    obarray, current,
                )
            else {
                return Ok(Value::NIL);
            };
            function = next_function;
            let Some(next_symbol) = crate::emacs_core::builtins::symbols::symbol_id(&function)
            else {
                break;
            };
            current = next_symbol;
        }
    }

    match function.kind() {
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => Ok(Value::NIL),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
            Ok(function
                .closure_interactive()
                .map(command_modes_from_stored_interactive_spec)
                .unwrap_or(Value::NIL))
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            let Some(probe) = function.bytecode_interactive_probe() else {
                return Ok(Value::NIL);
            };
            if probe.slot_count <= 5 {
                return Ok(Value::NIL);
            };
            Ok(probe
                .interactive
                .map(command_modes_from_stored_interactive_spec)
                .unwrap_or(Value::NIL))
        }
        ValueKind::Cons if super::autoload::is_autoload_value(&function) => {
            let Some(items) = value_list_to_vec(&function) else {
                return Ok(Value::NIL);
            };
            Ok(match items.get(3).copied() {
                Some(v) if v.is_cons() => v,
                _ => Value::NIL,
            })
        }
        ValueKind::Cons => Ok(command_modes_from_quoted_lambda(&function)?.unwrap_or(Value::NIL)),
        _ => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_command_modes(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_command_modes_impl(&eval.obarray, &args)
}

/// `(command-remapping COMMAND &optional POSITION KEYMAP)` -- return remapped
/// command for COMMAND.
///
/// Respects local/global keymaps when KEYMAP is omitted or nil.
pub(crate) fn builtin_command_remapping(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_command_remapping_impl(eval, args)
}

pub(crate) fn builtin_command_remapping_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("command-remapping", &args, 1)?;
    expect_max_args("command-remapping", &args, 3)?;
    if let Some(keymap) = args.get(2)
        && !keymap.is_nil()
        && !command_remapping_keymap_arg_valid(keymap)
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("keymapp"), *keymap],
        ));
    }
    let Some(command_name) = command_remapping_command_name(&args[0]) else {
        return Ok(Value::NIL);
    };
    if let Some(keymap_arg) = args.get(2) {
        match keymap_arg.kind() {
            ValueKind::Cons => {
                for keymap in command_remapping_explicit_keymaps(ctx, keymap_arg) {
                    if let Some(target) =
                        command_remapping_lookup_in_lisp_keymap(&keymap, command_name)
                    {
                        return Ok(command_remapping_normalize_target(target));
                    }
                }
                return Ok(Value::NIL);
            }
            ValueKind::Nil => {
                let active_maps =
                    current_active_maps_for_position_read_only(ctx, true, args.get(1))?;
                return Ok(
                    command_remapping_lookup_in_keymaps(&active_maps, command_name)
                        .unwrap_or(Value::NIL),
                );
            }
            _ => {
                // Not a valid keymap
                return Ok(Value::NIL);
            }
        }
    }
    let active_maps = current_active_maps_for_position_read_only(ctx, true, args.get(1))?;
    Ok(command_remapping_lookup_in_keymaps(&active_maps, command_name).unwrap_or(Value::NIL))
}

fn value_list_to_vec(list: &Value) -> Option<Vec<Value>> {
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

fn value_is_interactive_form(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let _pair_cdr = value.cons_cdr();
            pair_car.as_symbol_name() == Some("interactive")
        }
        _ => false,
    }
}

fn value_is_interactive_autoload(value: &Value) -> bool {
    if !super::autoload::is_autoload_value(value) {
        return false;
    }
    let Some(items) = value_list_to_vec(value) else {
        return false;
    };
    items.get(3).is_some_and(|v| !v.is_nil())
}

fn is_valid_docstring_reference(value: Value) -> bool {
    value.is_fixnum()
        || value.is_string()
        || (value.is_cons() && value.cons_car().is_string() && value.cons_cdr().is_fixnum())
}

fn closure_interactive_form_fallback(value: Value) -> InteractiveFormFallback {
    let invalid_doc_slot = match value.kind() {
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => value
            .closure_doc_value()
            .is_some_and(|doc| !doc.is_nil() && !is_valid_docstring_reference(doc)),
        ValueKind::Veclike(VecLikeType::ByteCode) => value
            .bytecode_interactive_probe()
            .and_then(|probe| probe.doc_form)
            .is_some_and(|doc| !is_valid_docstring_reference(doc)),
        _ => false,
    };
    if invalid_doc_slot {
        InteractiveFormFallback::GenericDispatch(value)
    } else {
        InteractiveFormFallback::PropertyOnly
    }
}

fn quoted_lambda_body_values(value: &Value) -> Option<Vec<Value>> {
    let mut items = value_list_to_vec(value)?;
    if items.first().and_then(|v| v.as_symbol_name()) != Some("lambda") || items.len() < 2 {
        return None;
    }
    Some(items.split_off(2))
}

fn quoted_lambda_has_interactive_form(value: &Value) -> bool {
    let Some(items) = quoted_lambda_body_values(value) else {
        return false;
    };
    let mut body_index = 0;
    if items.get(body_index).is_some_and(|v| v.is_string()) {
        body_index += 1;
    }
    while items.get(body_index).is_some_and(value_is_declare_form) {
        body_index += 1;
    }

    items.get(body_index).is_some_and(value_is_interactive_form)
}

fn value_is_declare_form(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let _pair_cdr = value.cons_cdr();
            pair_car.as_symbol_name() == Some("declare")
        }
        _ => false,
    }
}

fn resolve_function_designator_symbol_in_state(
    obarray: &Obarray,
    symbol: SymId,
) -> Option<(SymId, Value)> {
    crate::emacs_core::builtins::symbols::resolve_indirect_symbol_by_id_in_obarray(obarray, symbol)
}

fn registered_builtin_command(subr_id: SymId) -> bool {
    registered_builtin_interactive_spec(subr_id).is_some()
}

/// GNU `Fcommandp` has three semantically distinct outcomes after resolving a
/// function designator. Keeping them as variants prevents an immediate `nil`
/// result from accidentally entering the property/generic-function fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandpClassification {
    Interactive,
    Reject,
    CheckInteractiveFormProperty(InteractiveFormFallback),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InteractiveFormFallback {
    PropertyOnly,
    GenericDispatch(Value),
}

fn classify_command_object_in_state(
    value: &Value,
    for_call_interactively: bool,
) -> CommandpClassification {
    if value_is_interactive_autoload(value) {
        return CommandpClassification::Interactive;
    }

    match value.kind() {
        // GNU eval.c:Fcommandp classifies closures from their observable
        // vector shape.  Slot 5 may contain nil; its presence is sufficient.
        // A short closure whose body happens to contain `(interactive)` is
        // not a command and must not trigger an O(n) body materialization.
        ValueKind::Veclike(VecLikeType::Lambda) => {
            if value.closure_interactive().is_some() {
                CommandpClassification::Interactive
            } else {
                CommandpClassification::CheckInteractiveFormProperty(
                    closure_interactive_form_fallback(*value),
                )
            }
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            CommandpClassification::CheckInteractiveFormProperty(closure_interactive_form_fallback(
                *value,
            ))
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            if value
                .bytecode_interactive_probe()
                .is_some_and(|probe| probe.slot_count > 5)
            {
                CommandpClassification::Interactive
            } else {
                CommandpClassification::CheckInteractiveFormProperty(
                    closure_interactive_form_fallback(*value),
                )
            }
        }
        ValueKind::Cons if super::autoload::is_autoload_value(value) => {
            CommandpClassification::CheckInteractiveFormProperty(
                InteractiveFormFallback::PropertyOnly,
            )
        }
        ValueKind::Cons => {
            if quoted_lambda_has_interactive_form(value) {
                CommandpClassification::Interactive
            } else {
                CommandpClassification::Reject
            }
        }
        ValueKind::Subr(id) => {
            if registered_builtin_command(id) {
                CommandpClassification::Interactive
            } else {
                CommandpClassification::CheckInteractiveFormProperty(
                    InteractiveFormFallback::PropertyOnly,
                )
            }
        }
        ValueKind::Veclike(VecLikeType::Subr) => {
            if value
                .subr_interactivity()
                .is_some_and(crate::tagged::header::SubrInteractivity::is_interactive)
            {
                CommandpClassification::Interactive
            } else {
                CommandpClassification::CheckInteractiveFormProperty(
                    InteractiveFormFallback::PropertyOnly,
                )
            }
        }
        ValueKind::String | ValueKind::Veclike(VecLikeType::Vector) => {
            if for_call_interactively {
                CommandpClassification::Reject
            } else {
                CommandpClassification::Interactive
            }
        }
        _ => CommandpClassification::Reject,
    }
}

fn classify_command_designator_in_state(
    obarray: &Obarray,
    designator: &Value,
    for_call_interactively: bool,
) -> CommandpClassification {
    if let Some(symbol) = designator.as_symbol_id() {
        if let Some((_resolved_symbol, resolved_value)) =
            resolve_function_designator_symbol_in_state(obarray, symbol)
        {
            if resolved_value.is_nil() {
                return CommandpClassification::Reject;
            }
            return classify_command_object_in_state(&resolved_value, for_call_interactively);
        }
        return CommandpClassification::Reject;
    }
    classify_command_object_in_state(designator, for_call_interactively)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandInvocationKind {
    CallInteractively,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    CommandExecute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumIter)]
enum InteractiveControlLetter {
    FunctionName,
    ExistingBuffer,
    Buffer,
    Character,
    Command,
    Point,
    DirectoryName,
    InvokingEvent,
    ExistingFile,
    File,
    FileWithDirectoryDefault,
    Ignore,
    KeySequence,
    KeySequenceVector,
    UpEvent,
    Mark,
    StringWithInputMethod,
    NumberOrPrefix,
    Number,
    RawPrefix,
    NumericPrefix,
    ActiveRegion,
    Region,
    String,
    Symbol,
    Variable,
    Expression,
    EvalExpression,
    CodingSystemWithPrefix,
    CodingSystem,
}

impl InteractiveControlLetter {
    fn from_char(letter: char) -> Option<Self> {
        Some(match letter {
            'a' => Self::FunctionName,
            'b' => Self::ExistingBuffer,
            'B' => Self::Buffer,
            'c' => Self::Character,
            'C' => Self::Command,
            'd' => Self::Point,
            'D' => Self::DirectoryName,
            'e' => Self::InvokingEvent,
            'f' => Self::ExistingFile,
            'F' => Self::File,
            'G' => Self::FileWithDirectoryDefault,
            'i' => Self::Ignore,
            'k' => Self::KeySequence,
            'K' => Self::KeySequenceVector,
            'U' => Self::UpEvent,
            'm' => Self::Mark,
            'M' => Self::StringWithInputMethod,
            'N' => Self::NumberOrPrefix,
            'n' => Self::Number,
            'P' => Self::RawPrefix,
            'p' => Self::NumericPrefix,
            'R' => Self::ActiveRegion,
            'r' => Self::Region,
            's' => Self::String,
            'S' => Self::Symbol,
            'v' => Self::Variable,
            'x' => Self::Expression,
            'X' => Self::EvalExpression,
            'Z' => Self::CodingSystemWithPrefix,
            'z' => Self::CodingSystem,
            _ => return None,
        })
    }

    #[cfg(test)]
    fn letter(self) -> char {
        match self {
            Self::FunctionName => 'a',
            Self::ExistingBuffer => 'b',
            Self::Buffer => 'B',
            Self::Character => 'c',
            Self::Command => 'C',
            Self::Point => 'd',
            Self::DirectoryName => 'D',
            Self::InvokingEvent => 'e',
            Self::ExistingFile => 'f',
            Self::File => 'F',
            Self::FileWithDirectoryDefault => 'G',
            Self::Ignore => 'i',
            Self::KeySequence => 'k',
            Self::KeySequenceVector => 'K',
            Self::UpEvent => 'U',
            Self::Mark => 'm',
            Self::StringWithInputMethod => 'M',
            Self::NumberOrPrefix => 'N',
            Self::Number => 'n',
            Self::RawPrefix => 'P',
            Self::NumericPrefix => 'p',
            Self::ActiveRegion => 'R',
            Self::Region => 'r',
            Self::String => 's',
            Self::Symbol => 'S',
            Self::Variable => 'v',
            Self::Expression => 'x',
            Self::EvalExpression => 'X',
            Self::CodingSystemWithPrefix => 'Z',
            Self::CodingSystem => 'z',
        }
    }

    /// How this letter's arguments are written into `command-history`, one
    /// entry per argument the letter contributes.
    ///
    /// The slice length is the letter's argument count, which is what keeps
    /// this table in step with the arguments the spec walk actually pushes;
    /// `every_letter_reports_one_history_form_per_argument_it_pushes` checks
    /// the two against each other.
    ///
    /// The position-reading letters are GNU's `varies' codes 1-6
    /// (callint.c:151-153, assigned at :510, :631 and :679-681).
    fn history_forms(self) -> &'static [ArgHistoryForm] {
        match self {
            Self::Point => &[ArgHistoryForm::Call("point")],
            Self::Mark => &[ArgHistoryForm::Call("mark")],
            Self::Region => &[
                ArgHistoryForm::Call("region-beginning"),
                ArgHistoryForm::Call("region-end"),
            ],
            Self::ActiveRegion => &[
                ArgHistoryForm::Call("use-region-beginning"),
                ArgHistoryForm::Call("use-region-end"),
            ],
            _ => &[ArgHistoryForm::ByValue],
        }
    }
}

/// How one interactive argument is written into `command-history`.
///
/// GNU keeps this in the `varies' array of `Fcall_interactively'
/// (callint.c:447): an argument that came from point, the mark or the region
/// is recorded as a call to the function that produced it, so replaying the
/// entry through `repeat-complex-command' re-reads the *current* position
/// rather than the one that happened to be current when the command ran.
/// Every other argument is recorded by value.
///
/// Which form an argument takes is fixed by the spec letter that produced it,
/// never by the value it produced -- `R' records the `use-region-*' calls even
/// when the region was inactive and both arguments came out nil.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArgHistoryForm {
    ByValue,
    Call(&'static str),
}

impl ArgHistoryForm {
    /// GNU's `visargs[i]` assignment (callint.c:780-782).
    fn record(self, value: Value) -> Value {
        match self {
            Self::ByValue => quotify_history_arg(value),
            Self::Call(function) => Value::list(vec![Value::symbol(function)]),
        }
    }
}

/// GNU's `quotify_arg` (callint.c:127): wrap the argument in `quote` unless it
/// already evaluates to itself.  Conses and symbols other than nil and t need
/// the quote; numbers, strings, vectors and the two self-evaluating symbols do
/// not.
fn quotify_history_arg(value: Value) -> Value {
    let needs_quote =
        value.is_cons() || (value.as_symbol_id().is_some() && !value.is_nil() && value != Value::T);
    if needs_quote {
        Value::list(vec![Value::symbol("quote"), value])
    } else {
        value
    }
}

/// Environment in which GNU `call-interactively` evaluates a Lisp-form
/// interactive specification.
///
/// `callint.c:Fcall_interactively` does not use the caller's ambient lexical
/// environment.  It passes an interpreted closure's `CLOSURE_CONSTANTS` to
/// `Feval`, and passes nil for every other callable representation.  Keeping
/// that choice in the type prevents a form spec from being detached from the
/// environment required to evaluate it.
#[derive(Clone, Copy, Debug)]
enum InteractiveFormEnvironment {
    Dynamic,
    InterpretedClosure(Value),
}

impl InteractiveFormEnvironment {
    fn for_callable(callable: Value) -> Self {
        if callable
            .closure_body_value()
            .is_some_and(|body| body.is_cons())
        {
            Self::InterpretedClosure(callable.closure_env().flatten().unwrap_or(Value::NIL))
        } else {
            Self::Dynamic
        }
    }

    fn lexical_arg(self) -> Value {
        match self {
            Self::Dynamic => Value::NIL,
            Self::InterpretedClosure(environment) => environment,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct InteractiveFormSpec {
    form: Value,
    environment: InteractiveFormEnvironment,
}

#[derive(Clone, Debug)]
enum ParsedInteractiveSpec {
    NoArgs,
    StringCode(crate::heap_types::LispString),
    Form(InteractiveFormSpec),
}

#[derive(Clone, Debug, Default)]
struct ParsedInteractiveStringCode {
    prefix_flags: Vec<char>,
    entries: Vec<(char, crate::heap_types::LispString)>,
}

#[derive(Clone, Debug, Default)]
struct InteractiveInvocationContext {
    command_keys: Vec<Value>,
    next_event_with_parameters_index: usize,
    has_command_keys_context: bool,
    pending_up_event: Option<Value>,
}

impl InteractiveInvocationContext {
    fn from_keys_arg_in_state(read_command_keys: &[Value], keys: Option<&Value>) -> Self {
        let mut context = Self::default();
        if let Some(keys_val) = keys
            && keys_val.is_vector()
            && let Some(vec_data) = keys_val.as_vector_data()
            && !vec_data.is_empty()
        {
            context.command_keys = vec_data.clone();
            context.has_command_keys_context = true;
            return context;
        }
        if !read_command_keys.is_empty() {
            context.command_keys = read_command_keys.to_vec();
            context.has_command_keys_context = true;
        }
        context
    }
}

fn interactive_event_symbol_name(event: &Value) -> Option<&'static str> {
    match event.kind() {
        ValueKind::Symbol(id) => Some(resolve_sym(id)),
        ValueKind::Cons => {
            let car = event.cons_car();
            match car.kind() {
                ValueKind::Symbol(id) => Some(resolve_sym(id)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn interactive_strip_event_modifier_prefixes(mut name: &str) -> &str {
    loop {
        if let Some(rest) = name.strip_prefix("C-") {
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("M-") {
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("S-") {
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("s-") {
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("H-") {
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("A-") {
            name = rest;
            continue;
        }
        break;
    }
    name
}

fn interactive_event_is_down_event(event: &Value) -> bool {
    let Some(name) = interactive_event_symbol_name(event) else {
        return false;
    };
    interactive_strip_event_modifier_prefixes(name).starts_with("down-")
}

fn interactive_last_key_sequence_event(sequence: &Value) -> Option<Value> {
    match sequence.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            sequence.as_vector_data().and_then(|v| v.last().copied())
        }
        _ => None,
    }
}

fn interactive_capture_up_event_in_eval(
    eval: &mut Context,
    sequence: &Value,
    context: &mut InteractiveInvocationContext,
) -> Result<(), Flow> {
    context.pending_up_event = None;
    if interactive_last_key_sequence_event(sequence)
        .is_some_and(|event| interactive_event_is_down_event(&event))
    {
        let up_event = super::lread::builtin_read_event(eval, vec![])?;
        if !up_event.is_nil() {
            context.pending_up_event = Some(up_event);
        }
    }
    Ok(())
}

fn interactive_capture_up_event_in_vm_batch_runtime(
    shared: &mut super::eval::Context,
    sequence: &Value,
    context: &mut InteractiveInvocationContext,
) -> Result<(), Flow> {
    context.pending_up_event = None;
    if interactive_last_key_sequence_event(sequence)
        .is_some_and(|event| interactive_event_is_down_event(&event))
        && let Some(up_event) = super::lread::builtin_read_event_in_runtime(shared, &[])?
        && !up_event.is_nil()
    {
        context.pending_up_event = Some(up_event);
    }
    Ok(())
}

fn interactive_u_arg(context: &mut InteractiveInvocationContext) -> Value {
    context
        .pending_up_event
        .take()
        .map(|event| Value::vector(vec![event]))
        .unwrap_or(Value::NIL)
}

/// Route symbol-value reads through the full GNU lookup path so
/// LOCALIZED BLV / FORWARDED slot / specpdl let-binding state is
/// observed. See the extended comment on the identical helper in
/// `builtins/misc_eval.rs` (audit finding #3 in
/// `drafts/regex-search-audit.md`).
fn dynamic_or_global_symbol_value(eval: &Context, name: &str) -> Option<Value> {
    let id = crate::emacs_core::intern::intern(name);
    eval.eval_symbol_by_id(id).ok()
}

fn dynamic_or_global_symbol_value_in_state(
    obarray: &Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    name: &str,
) -> Option<Value> {
    obarray.symbol_value(name).cloned()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn dynamic_buffer_or_global_symbol_value(
    eval: &Context,
    buf: &crate::buffer::Buffer,
    name: &str,
) -> Option<Value> {
    dynamic_buffer_or_global_symbol_value_in_state(&eval.obarray, &[], buf, name)
}

fn dynamic_buffer_or_global_symbol_value_in_state(
    obarray: &Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    buf: &crate::buffer::Buffer,
    name: &str,
) -> Option<Value> {
    let sym = crate::emacs_core::intern::intern(name);
    if let Some(v) = buf.get_buffer_local_by_sym_id_gated(sym, obarray.is_localized(sym)) {
        return Some(v);
    }
    obarray.symbol_value(name).cloned()
}

fn prefix_numeric_value(value: &Value) -> i64 {
    crate::emacs_core::prefix::prefix_numeric_value(value)
}

fn interactive_prefix_raw_arg(eval: &Context, kind: CommandInvocationKind) -> Value {
    interactive_prefix_raw_arg_in_state(&eval.obarray, &[], kind)
}

fn interactive_prefix_raw_arg_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    kind: CommandInvocationKind,
) -> Value {
    let symbol = match kind {
        CommandInvocationKind::CallInteractively => "current-prefix-arg",
        CommandInvocationKind::CommandExecute => "prefix-arg",
    };
    dynamic_or_global_symbol_value_in_state(obarray, dynamic, symbol).unwrap_or(Value::NIL)
}

fn interactive_prefix_numeric_arg(eval: &Context, kind: CommandInvocationKind) -> Value {
    let raw = interactive_prefix_raw_arg(eval, kind);
    Value::fixnum(prefix_numeric_value(&raw))
}

fn interactive_prefix_numeric_arg_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    kind: CommandInvocationKind,
) -> Value {
    let raw = interactive_prefix_raw_arg_in_state(obarray, dynamic, kind);
    Value::fixnum(prefix_numeric_value(&raw))
}

fn interactive_region_args(eval: &Context, missing_mark_signal: &str) -> Result<Vec<Value>, Flow> {
    interactive_region_args_in_buffers(&eval.buffers, missing_mark_signal)
}

fn interactive_region_args_in_buffers(
    buffers: &crate::buffer::BufferManager,
    missing_mark_signal: &str,
) -> Result<Vec<Value>, Flow> {
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let mark = buf.mark_emacs_byte_pos().ok_or_else(|| {
        signal(
            missing_mark_signal,
            vec![Value::string(
                "The mark is not set now, so there is no region",
            )],
        )
    })?;
    let pt = buf.point_emacs_byte_pos();
    let beg = pt.min(mark);
    let end = pt.max(mark);
    // Region-taking builtins use Emacs-style 1-based character positions.
    let beg_char = buf.emacs_byte_pos_to_lisp_char_pos(beg).as_i64();
    let end_char = buf.emacs_byte_pos_to_lisp_char_pos(end).as_i64();
    Ok(vec![Value::fixnum(beg_char), Value::fixnum(end_char)])
}

fn interactive_point_arg(eval: &Context) -> Result<Value, Flow> {
    interactive_point_arg_in_buffers(&eval.buffers)
}

fn interactive_point_arg_in_buffers(buffers: &crate::buffer::BufferManager) -> Result<Value, Flow> {
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let point_char = buf.point_lisp_char_pos().as_i64();
    Ok(Value::fixnum(point_char))
}

fn interactive_mark_arg(eval: &Context) -> Result<Value, Flow> {
    interactive_mark_arg_in_buffers(&eval.buffers)
}

fn interactive_mark_arg_in_buffers(buffers: &crate::buffer::BufferManager) -> Result<Value, Flow> {
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.mark_emacs_byte_pos()
        .ok_or_else(|| signal("error", vec![Value::string("The mark is not set now")]))?;
    let mark_char = buf
        .mark_char_pos()
        .expect("mark byte/char stay in sync")
        .get() as i64
        + 1;
    Ok(Value::fixnum(mark_char))
}

fn interactive_current_buffer_default(buffers: &crate::buffer::BufferManager) -> Value {
    buffers
        .current_buffer_id()
        .map(Value::make_buffer)
        .unwrap_or(Value::NIL)
}

fn interactive_other_buffer_default(buffers: &mut crate::buffer::BufferManager) -> Value {
    let avoid = interactive_current_buffer_default(buffers);
    super::builtins::other_buffer_impl(buffers, vec![avoid]).unwrap_or(Value::NIL)
}

fn interactive_last_input_event_with_parameters_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
) -> Option<Value> {
    let event = dynamic_or_global_symbol_value_in_state(obarray, dynamic, "last-input-event")?;
    interactive_event_with_parameters_p(&event).then_some(event)
}

fn interactive_next_event_with_parameters_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    context: &mut InteractiveInvocationContext,
) -> Option<Value> {
    if context.has_command_keys_context {
        return interactive_next_event_with_parameters_from_keys(context);
    }
    interactive_last_input_event_with_parameters_in_state(obarray, dynamic)
}

#[allow(dead_code, clippy::too_many_arguments)] // explicit split-state compatibility seam
fn interactive_args_from_string_code_in_state(
    obarray: &mut Obarray,
    dynamic: &mut Vec<OrderedRuntimeBindingMap>,
    buffers: &mut crate::buffer::BufferManager,
    custom: &crate::emacs_core::custom::CustomManager,
    specpdl: &[crate::emacs_core::eval::SpecBinding],
    code: &crate::heap_types::LispString,
    kind: CommandInvocationKind,
    context: &mut InteractiveInvocationContext,
) -> Result<Option<Vec<Value>>, Flow> {
    let parsed = parse_interactive_code_entries(code);
    interactive_apply_prefix_flags_in_state(
        obarray,
        dynamic.as_mut_slice(),
        buffers,
        custom,
        specpdl,
        &parsed.prefix_flags,
    )?;
    if parsed.entries.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut args = Vec::new();
    for (letter, _prompt) in parsed.entries {
        let Some(control) = InteractiveControlLetter::from_char(letter) else {
            return Ok(None);
        };
        match control {
            InteractiveControlLetter::Point => {
                args.push(interactive_point_arg_in_buffers(buffers)?)
            }
            InteractiveControlLetter::InvokingEvent => {
                if let Some(event) =
                    interactive_next_event_with_parameters_in_state(obarray, dynamic, context)
                {
                    args.push(event);
                } else {
                    return Err(signal(
                        "error",
                        vec![Value::string(
                            "command must be bound to an event with parameters",
                        )],
                    ));
                }
            }
            InteractiveControlLetter::Ignore => args.push(Value::NIL),
            InteractiveControlLetter::Mark => args.push(interactive_mark_arg_in_buffers(buffers)?),
            InteractiveControlLetter::NumberOrPrefix => {
                let raw = interactive_prefix_raw_arg_in_state(obarray, dynamic.as_slice(), kind);
                if raw.is_nil() {
                    return Ok(None);
                }
                args.push(Value::fixnum(prefix_numeric_value(&raw)));
            }
            InteractiveControlLetter::NumericPrefix => args.push(
                interactive_prefix_numeric_arg_in_state(obarray, dynamic.as_slice(), kind),
            ),
            InteractiveControlLetter::RawPrefix => args.push(interactive_prefix_raw_arg_in_state(
                obarray,
                dynamic.as_slice(),
                kind,
            )),
            InteractiveControlLetter::Region => {
                args.extend(interactive_region_args_in_buffers(buffers, "error")?)
            }
            InteractiveControlLetter::UpEvent => args.push(Value::NIL),
            InteractiveControlLetter::CodingSystemWithPrefix => {
                let raw = interactive_prefix_raw_arg_in_state(obarray, dynamic.as_slice(), kind);
                if raw.is_nil() {
                    args.push(Value::NIL);
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        }
    }

    Ok(Some(args))
}

fn interactive_args_from_string_code_in_vm_runtime(
    shared: &mut super::eval::Context,
    code: &crate::heap_types::LispString,
    kind: CommandInvocationKind,
    context: &mut InteractiveInvocationContext,
) -> Result<Option<Vec<Value>>, Flow> {
    let roots = shared.save_specpdl_roots();
    let result = (|| -> Result<Option<Vec<Value>>, Flow> {
        let parsed = parse_interactive_code_entries(code);
        interactive_apply_prefix_flags(shared, &parsed.prefix_flags, context)?;
        if parsed.entries.is_empty() {
            return Ok(Some(Vec::new()));
        }

        let mut args = Vec::new();
        let mut visible_args = Vec::new();
        for (letter, prompt) in parsed.entries {
            let prompt = interactive_prompt_with_visible_args(shared, &prompt, &visible_args)?;
            let args_before = args.len();
            let Some(control) = InteractiveControlLetter::from_char(letter) else {
                return Ok(None);
            };
            match control {
                InteractiveControlLetter::FunctionName | InteractiveControlLetter::Command => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    super::minibuffer::builtin_read_command_in_runtime(shared, &letter_args)?;
                    args.push(super::minibuffer::finish_read_command_with_minibuffer(
                        &letter_args,
                        |minibuffer_args| {
                            super::reader::finish_read_from_minibuffer_in_vm_runtime(
                                shared,
                                minibuffer_args,
                            )
                        },
                    )?);
                }
                InteractiveControlLetter::ExistingBuffer => {
                    let default = interactive_current_buffer_default(&shared.buffers);
                    let letter_args = [Value::heap_string(prompt.clone()), default, Value::T];
                    args.push(super::minibuffer::finish_read_buffer_in_vm_runtime(
                        shared,
                        &letter_args,
                    )?);
                }
                InteractiveControlLetter::Buffer => {
                    let default = interactive_other_buffer_default(&mut shared.buffers);
                    let letter_args = [Value::heap_string(prompt.clone()), default, Value::NIL];
                    args.push(super::minibuffer::finish_read_buffer_in_vm_runtime(
                        shared,
                        &letter_args,
                    )?);
                }
                InteractiveControlLetter::Character => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    let arg = if let Some(arg) =
                        super::reader::builtin_read_char_in_runtime(shared, &letter_args)?
                    {
                        arg
                    } else {
                        super::reader::finish_read_char_interactive_in_runtime(
                            shared,
                            &letter_args,
                        )?
                    };
                    args.push(arg);
                }
                InteractiveControlLetter::Point => {
                    args.push(interactive_point_arg_in_buffers(&shared.buffers)?)
                }
                InteractiveControlLetter::DirectoryName => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    args.push(super::minibuffer::finish_read_directory_name_in_vm_runtime(
                        shared,
                        &letter_args,
                    )?);
                }
                InteractiveControlLetter::InvokingEvent => {
                    if let Some(event) = interactive_next_event_with_parameters_in_state(
                        &shared.obarray,
                        &[],
                        context,
                    ) {
                        args.push(event);
                    } else {
                        return Err(signal(
                            "error",
                            vec![Value::string(
                                "command must be bound to an event with parameters",
                            )],
                        ));
                    }
                }
                InteractiveControlLetter::ExistingFile => {
                    let letter_args = [
                        Value::heap_string(prompt.clone()),
                        Value::NIL,
                        Value::NIL,
                        Value::T,
                    ];
                    args.push(super::minibuffer::finish_read_file_name_in_vm_runtime(
                        shared,
                        &letter_args,
                    )?);
                }
                InteractiveControlLetter::File
                | InteractiveControlLetter::FileWithDirectoryDefault => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    args.push(super::minibuffer::finish_read_file_name_in_vm_runtime(
                        shared,
                        &letter_args,
                    )?);
                }
                InteractiveControlLetter::Ignore => args.push(Value::NIL),
                InteractiveControlLetter::KeySequence => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    let arg = if let Some(arg) =
                        super::reader::builtin_read_key_sequence_in_runtime(shared, &letter_args)?
                    {
                        arg
                    } else {
                        super::reader::finish_read_key_sequence_interactive_in_runtime(
                            shared,
                            super::reader::read_key_sequence_options_from_args(&letter_args),
                        )?
                    };
                    interactive_capture_up_event_in_vm_batch_runtime(shared, &arg, context)?;
                    args.push(arg);
                }
                InteractiveControlLetter::KeySequenceVector => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    let arg = if let Some(arg) =
                        super::reader::builtin_read_key_sequence_vector_in_runtime(
                            shared,
                            &letter_args,
                        )? {
                        arg
                    } else {
                        super::reader::finish_read_key_sequence_vector_interactive_in_runtime(
                            shared,
                            super::reader::read_key_sequence_options_from_args(&letter_args),
                        )?
                    };
                    interactive_capture_up_event_in_vm_batch_runtime(shared, &arg, context)?;
                    args.push(arg);
                }
                InteractiveControlLetter::StringWithInputMethod => {
                    let letter_args = [
                        Value::heap_string(prompt.clone()),
                        Value::NIL,
                        Value::NIL,
                        Value::NIL,
                        Value::T,
                    ];
                    super::reader::builtin_read_string_in_runtime(shared, &letter_args)?;
                    args.push(super::reader::finish_read_string_with_minibuffer(
                        &letter_args,
                        |minibuffer_args| {
                            super::reader::finish_read_from_minibuffer_in_vm_runtime(
                                shared,
                                minibuffer_args,
                            )
                        },
                    )?);
                }
                InteractiveControlLetter::Mark => {
                    args.push(interactive_mark_arg_in_buffers(&shared.buffers)?)
                }
                InteractiveControlLetter::NumberOrPrefix => {
                    let raw = interactive_prefix_raw_arg_in_state(&shared.obarray, &[], kind);
                    if raw.is_nil() {
                        args.push(read_number_through_the_function_cell(
                            shared,
                            Value::heap_string(prompt.clone()),
                        )?);
                    } else {
                        args.push(Value::fixnum(prefix_numeric_value(&raw)));
                    }
                }
                InteractiveControlLetter::NumericPrefix => args.push(
                    interactive_prefix_numeric_arg_in_state(&shared.obarray, &[], kind),
                ),
                InteractiveControlLetter::RawPrefix => args.push(
                    interactive_prefix_raw_arg_in_state(&shared.obarray, &[], kind),
                ),
                InteractiveControlLetter::Region => args.extend(
                    interactive_region_args_in_buffers(&shared.buffers, "error")?,
                ),
                InteractiveControlLetter::ActiveRegion => {
                    if interactive_use_region_p_in_vm_runtime(shared)? {
                        args.extend(interactive_region_args_in_buffers(
                            &shared.buffers,
                            "error",
                        )?);
                    } else {
                        args.push(Value::NIL);
                        args.push(Value::NIL);
                    }
                }
                InteractiveControlLetter::Number => {
                    args.push(read_number_through_the_function_cell(
                        shared,
                        Value::heap_string(prompt.clone()),
                    )?);
                }
                InteractiveControlLetter::String => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    super::reader::builtin_read_string_in_runtime(shared, &letter_args)?;
                    args.push(super::reader::finish_read_string_with_minibuffer(
                        &letter_args,
                        |minibuffer_args| {
                            super::reader::finish_read_from_minibuffer_in_vm_runtime(
                                shared,
                                minibuffer_args,
                            )
                        },
                    )?);
                }
                InteractiveControlLetter::Symbol => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    super::reader::builtin_read_string_in_runtime(shared, &letter_args)?;
                    let sym_name = super::reader::finish_read_string_with_minibuffer(
                        &letter_args,
                        |minibuffer_args| {
                            super::reader::finish_read_from_minibuffer_in_vm_runtime(
                                shared,
                                minibuffer_args,
                            )
                        },
                    )?;
                    if let Some(name) = sym_name.as_utf8_str() {
                        args.push(Value::symbol(name));
                    } else {
                        return Ok(None);
                    }
                }
                InteractiveControlLetter::Expression => args.push(
                    interactive_read_expression_arg_in_vm_runtime(shared, prompt)?,
                ),
                InteractiveControlLetter::EvalExpression => args.push(
                    interactive_eval_expression_arg_in_vm_runtime(shared, prompt)?,
                ),
                InteractiveControlLetter::UpEvent => args.push(interactive_u_arg(context)),
                InteractiveControlLetter::Variable => {
                    let letter_args = [Value::heap_string(prompt.clone())];
                    super::minibuffer::builtin_read_variable_in_runtime(shared, &letter_args)?;
                    args.push(super::minibuffer::finish_read_variable_with_minibuffer(
                        &letter_args,
                        |minibuffer_args| {
                            super::reader::finish_read_from_minibuffer_in_vm_runtime(
                                shared,
                                minibuffer_args,
                            )
                        },
                    )?);
                }
                InteractiveControlLetter::CodingSystem => {
                    args.push(super::lread::builtin_read_coding_system(
                        shared,
                        vec![Value::heap_string(prompt.clone())],
                    )?)
                }
                InteractiveControlLetter::CodingSystemWithPrefix => {
                    let raw = interactive_prefix_raw_arg_in_state(&shared.obarray, &[], kind);
                    if raw.is_nil() {
                        args.push(Value::NIL);
                    } else {
                        args.push(interactive_read_coding_system_optional_arg(shared, prompt)?);
                    }
                }
            }
            root_new_interactive_args(shared, &args, &mut visible_args, args_before);
        }

        Ok(Some(args))
    })();
    shared.restore_specpdl_roots(roots);
    result
}

fn interactive_read_expression_arg(
    eval: &mut Context,
    prompt: crate::heap_types::LispString,
) -> Result<Value, Flow> {
    let input =
        super::reader::builtin_read_from_minibuffer(eval, vec![Value::heap_string(prompt)])?;
    super::reader::builtin_read(eval, vec![input])
}

fn interactive_read_expression_arg_in_vm_runtime(
    shared: &mut super::eval::Context,
    prompt: crate::heap_types::LispString,
) -> Result<Value, Flow> {
    let input = super::reader::finish_read_from_minibuffer_in_vm_runtime(
        shared,
        &[Value::heap_string(prompt)],
    )?;
    super::reader::builtin_read(shared, vec![input])
}

fn interactive_eval_expression_arg_in_vm_runtime(
    shared: &mut super::eval::Context,
    prompt: crate::heap_types::LispString,
) -> Result<Value, Flow> {
    let expr_value = interactive_read_expression_arg(shared, prompt)?;
    shared.eval_value(&expr_value)
}

fn interactive_read_coding_system_optional_arg(
    eval: &mut super::eval::Context,
    prompt: crate::heap_types::LispString,
) -> Result<Value, Flow> {
    match super::lread::builtin_read_coding_system(eval, vec![Value::heap_string(prompt)]) {
        Ok(value) => Ok(value),
        Err(Flow::Signal(sig)) if sig.symbol == intern("end-of-file") => Ok(Value::NIL),
        Err(flow) => Err(flow),
    }
}

/// The `n` and `N` code letters, GNU's way: `calln (Qread_number,
/// callint_message)` (src/callint.c:645).
///
/// This is the ONE control letter GNU dispatches through a Lisp function cell
/// -- `s` is `Fread_string`, `S` is `Fintern` over `Fcompleting_read`, and so
/// on -- and going through the cell is observable: rebinding `read-number`
/// changes what `(interactive "n")` produces.  Measured on GNU 31.0.90,
/// `(cl-letf (((symbol-function 'read-number) (lambda (&rest _) 42))) ...)`
/// answers 42.  Calling a Rust `read-number` here instead answered the Rust
/// one, and `read-number` is `lisp/subr.el:3725` -- DIVERGENCES.md 152.
fn read_number_through_the_function_cell(
    eval: &mut super::eval::Context,
    prompt: Value,
) -> EvalResult {
    eval.apply(Value::symbol("read-number"), vec![prompt])
}

fn interactive_use_region_p_in_vm_runtime(shared: &mut super::eval::Context) -> Result<bool, Flow> {
    shared
        .apply(Value::symbol("use-region-p"), vec![])
        .map(|value| value.is_truthy())
}

fn interactive_buffer_read_only_active_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buf: &crate::buffer::Buffer,
) -> bool {
    if buf.get_read_only() {
        return true;
    }
    dynamic_buffer_or_global_symbol_value_in_state(obarray, dynamic, buf, "buffer-read-only")
        .is_some_and(|v| v.is_truthy())
}

fn interactive_require_writable_current_buffer_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
) -> Result<(), Flow> {
    let Some(buf) = buffers.current_buffer() else {
        return Ok(());
    };
    if dynamic_buffer_or_global_symbol_value_in_state(obarray, dynamic, buf, "inhibit-read-only")
        .is_some_and(|v| v.is_truthy())
    {
        return Ok(());
    }
    if interactive_buffer_read_only_active_in_state(obarray, dynamic, buf) {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![buf.name_value()],
        ));
    }
    Ok(())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn interactive_apply_shift_selection_prefix(eval: &mut Context) {
    interactive_apply_shift_selection_prefix_in_state(
        &mut eval.obarray,
        &mut [],
        &mut eval.buffers,
        &eval.custom,
        eval.specpdl.as_slice(),
    );
}

fn interactive_apply_shift_selection_prefix_in_state(
    obarray: &mut Obarray,
    dynamic: &mut [OrderedRuntimeBindingMap],
    buffers: &mut crate::buffer::BufferManager,
    custom: &crate::emacs_core::custom::CustomManager,
    specpdl: &[crate::emacs_core::eval::SpecBinding],
) {
    let shifted = dynamic_or_global_symbol_value_in_state(
        obarray,
        dynamic,
        "this-command-keys-shift-translated",
    )
    .is_some_and(|v| v.is_truthy());
    let shift_select_mode =
        dynamic_or_global_symbol_value_in_state(obarray, dynamic, "shift-select-mode")
            .is_some_and(|v| v.is_truthy());
    if !shifted || !shift_select_mode {
        return;
    }

    let mut mark_activated = false;
    if let Some(current_id) = buffers.current_buffer_id() {
        let point = buffers
            .get(current_id)
            .map(|buf| buf.point_emacs_byte_pos())
            .unwrap_or(EmacsBytePos::ZERO);
        let _ = buffers.set_buffer_mark_emacs_byte_pos(current_id, point);
        let _ = buffers.set_buffer_local_property(current_id, "mark-active", Value::T);
        mark_activated = true;
    }
    if mark_activated {
        let _ = super::eval::set_runtime_binding(
            obarray,
            buffers,
            custom,
            specpdl,
            intern("mark-active"),
            Value::T,
        );
    }
}

fn interactive_first_event_with_parameters_from_keys(
    context: &InteractiveInvocationContext,
) -> Option<Value> {
    context
        .command_keys
        .iter()
        .copied()
        .find(interactive_event_with_parameters_p)
}

fn interactive_first_event_with_parameters(
    eval: &Context,
    context: &InteractiveInvocationContext,
) -> Option<Value> {
    if context.has_command_keys_context {
        return interactive_first_event_with_parameters_from_keys(context);
    }
    interactive_last_input_event_with_parameters(eval)
}

fn interactive_event_target_window(event: &Value) -> Option<Value> {
    let event_slots = crate::emacs_core::value::list_to_vec(event)?;
    let mut position = *event_slots.get(1)?;
    if let Some(positions) = crate::emacs_core::value::list_to_vec(&position)
        && let Some(first_position) = positions.first()
    {
        position = *first_position;
    }
    let position_slots = crate::emacs_core::value::list_to_vec(&position)?;
    let first = *position_slots.first()?;
    if first.is_window() { Some(first) } else { None }
}

fn interactive_inactive_minibuffer_target_p(
    eval: &Context,
    window_id: crate::window::WindowId,
) -> bool {
    eval.frames.frame_list().into_iter().any(|frame_id| {
        eval.frames
            .get(frame_id)
            .is_some_and(|frame| frame.minibuffer_window == Some(window_id))
    }) && eval.active_minibuffer_window != Some(window_id)
}

fn interactive_select_window_from_prefix_context(
    eval: &mut Context,
    context: &InteractiveInvocationContext,
) -> Result<(), Flow> {
    let Some(event) = interactive_first_event_with_parameters(eval, context) else {
        return Ok(());
    };
    let Some(window_value) = interactive_event_target_window(&event) else {
        return Ok(());
    };
    if !window_value.is_window() {
        return Ok(());
    };
    let Some(wid) = window_value.as_window_id() else {
        return Ok(());
    };
    let window_id = crate::window::WindowId(wid);
    if interactive_inactive_minibuffer_target_p(eval, window_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to select inactive minibuffer window",
            )],
        ));
    }

    eval.run_hook_if_bound("mouse-leave-buffer-hook")?;
    crate::emacs_core::window_cmds::builtin_select_window(eval, vec![window_value, Value::NIL])?;
    Ok(())
}

fn interactive_apply_prefix_flags(
    eval: &mut Context,
    prefix_flags: &[char],
    context: &InteractiveInvocationContext,
) -> Result<(), Flow> {
    for prefix_flag in prefix_flags {
        match prefix_flag {
            '*' => interactive_require_writable_current_buffer_in_state(
                &eval.obarray,
                &[],
                &eval.buffers,
            )?,
            '@' => interactive_select_window_from_prefix_context(eval, context)?,
            '^' => interactive_apply_shift_selection_prefix_in_state(
                &mut eval.obarray,
                &mut [],
                &mut eval.buffers,
                &eval.custom,
                eval.specpdl.as_slice(),
            ),
            _ => {}
        }
    }
    Ok(())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn interactive_apply_prefix_flags_in_state(
    obarray: &mut Obarray,
    dynamic: &mut [OrderedRuntimeBindingMap],
    buffers: &mut crate::buffer::BufferManager,
    custom: &crate::emacs_core::custom::CustomManager,
    specpdl: &[crate::emacs_core::eval::SpecBinding],
    prefix_flags: &[char],
) -> Result<(), Flow> {
    for prefix_flag in prefix_flags {
        match prefix_flag {
            '*' => interactive_require_writable_current_buffer_in_state(obarray, dynamic, buffers)?,
            '@' => {
                // Selecting the window from the first mouse event requires command-loop
                // event context; current batch paths have no such events yet.
            }
            '^' => interactive_apply_shift_selection_prefix_in_state(
                obarray, dynamic, buffers, custom, specpdl,
            ),
            _ => {}
        }
    }
    Ok(())
}

fn interactive_event_with_parameters_p(event: &Value) -> bool {
    event.is_cons()
}

fn interactive_next_event_with_parameters_from_keys(
    context: &mut InteractiveInvocationContext,
) -> Option<Value> {
    while context.next_event_with_parameters_index < context.command_keys.len() {
        let event = context.command_keys[context.next_event_with_parameters_index];
        context.next_event_with_parameters_index += 1;
        if interactive_event_with_parameters_p(&event) {
            return Some(event);
        }
    }
    None
}

fn interactive_last_input_event_with_parameters(eval: &Context) -> Option<Value> {
    let event = dynamic_or_global_symbol_value(eval, "last-input-event")?;
    interactive_event_with_parameters_p(&event).then_some(event)
}

fn interactive_next_event_with_parameters(
    eval: &Context,
    context: &mut InteractiveInvocationContext,
) -> Option<Value> {
    if context.has_command_keys_context {
        return interactive_next_event_with_parameters_from_keys(context);
    }
    interactive_last_input_event_with_parameters(eval)
}

/// Parse a prepared interactive spec value.
///
/// The input may be an `(interactive SPEC)` form returned by
/// `interactive-form`, or the already-extracted SPEC stored by a closure.
fn parse_interactive_spec_from_value(
    spec: &Value,
    environment: InteractiveFormEnvironment,
) -> Option<ParsedInteractiveSpec> {
    if value_is_interactive_form(spec) {
        let items = value_list_to_vec(spec)?;
        return match items.get(1) {
            Some(nested_spec) => parse_interactive_spec_from_value(nested_spec, environment),
            None => Some(ParsedInteractiveSpec::NoArgs),
        };
    }
    match spec.kind() {
        ValueKind::Nil => Some(ParsedInteractiveSpec::NoArgs),
        ValueKind::String => {
            let s = spec
                .as_lisp_string()
                .cloned()
                .expect("ValueKind::String must carry LispString payload");
            if s.is_empty() {
                Some(ParsedInteractiveSpec::NoArgs)
            } else {
                Some(ParsedInteractiveSpec::StringCode(s))
            }
        }
        _ => {
            // Could be a form to evaluate.  Preserve its evaluation
            // environment as part of the parsed state so no caller can
            // accidentally evaluate it in the ambient lexical scope.
            Some(ParsedInteractiveSpec::Form(InteractiveFormSpec {
                form: *spec,
                environment,
            }))
        }
    }
}

fn prepare_call_interactively_spec(
    function: Value,
    interactive_form: Value,
    environment: InteractiveFormEnvironment,
) -> Result<ParsedInteractiveSpec, Flow> {
    if !interactive_form.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("commandp"), function],
        ));
    }

    let tail = interactive_form.cons_cdr();
    let spec = if tail.is_nil() {
        Value::NIL
    } else if tail.is_cons() {
        tail.cons_car()
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        ));
    };

    parse_interactive_spec_from_value(&spec, environment).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), spec],
        )
    })
}

fn interactive_form_value_to_args(value: Value) -> Result<Vec<Value>, Flow> {
    if value.is_nil() {
        return Ok(Vec::new());
    }
    if let Some(values) = value_list_to_vec(&value) {
        return Ok(values);
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("listp"), value],
    ))
}

fn parse_interactive_prefix_flags(line: &[u8]) -> (Vec<char>, usize) {
    let mut flags = Vec::new();
    let mut offset = 0usize;
    while let Some(&byte) = line.get(offset) {
        if matches!(byte, b'*' | b'@' | b'^') {
            flags.push(byte as char);
            offset += 1;
        } else {
            break;
        }
    }
    (flags, offset)
}

fn empty_lisp_string_like(
    template: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    if template.is_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(Vec::new())
    } else {
        crate::heap_types::LispString::from_unibyte(Vec::new())
    }
}

fn parse_interactive_code_entries(
    code: &crate::heap_types::LispString,
) -> ParsedInteractiveStringCode {
    let mut parsed = ParsedInteractiveStringCode::default();
    if code.is_empty() {
        return parsed;
    }

    let bytes = code.as_bytes();
    let mut line_start = 0usize;
    let mut index = 0usize;
    while line_start <= bytes.len() {
        let rel_end = bytes[line_start..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|offset| line_start + offset)
            .unwrap_or(bytes.len());
        let line = code
            .slice(line_start, rel_end)
            .unwrap_or_else(|| empty_lisp_string_like(code));
        let line_bytes = line.as_bytes();
        let mut entry_offset = 0usize;
        if index == 0 {
            let (flags, stripped) = parse_interactive_prefix_flags(line_bytes);
            parsed.prefix_flags = flags;
            entry_offset = stripped;
        }
        if entry_offset < line_bytes.len() {
            let letter = line_bytes[entry_offset] as char;
            let prompt = line
                .slice(entry_offset + 1, line_bytes.len())
                .unwrap_or_else(|| empty_lisp_string_like(&line));
            parsed.entries.push((letter, prompt));
        }
        if rel_end == bytes.len() {
            break;
        }
        line_start = rel_end + 1;
        index += 1;
    }
    parsed
}

fn interactive_prompt_with_visible_args(
    eval: &mut Context,
    prompt: &crate::heap_types::LispString,
    visible_args: &[Value],
) -> Result<crate::heap_types::LispString, Flow> {
    let mut format_args = Vec::with_capacity(visible_args.len() + 1);
    format_args.push(Value::heap_string(prompt.clone()));
    format_args.extend_from_slice(visible_args);
    let roots = eval.save_specpdl_roots();
    for value in &format_args {
        eval.push_specpdl_root(*value);
    }
    let formatted = super::builtins::dispatch_builtin(eval, "format-message", format_args)
        .expect("format-message builtin should be registered");
    eval.restore_specpdl_roots(roots);
    let formatted = formatted?;
    formatted.as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), formatted],
        )
    })
}

fn interactive_visible_arg(value: Value) -> Value {
    if let Some(string) = value.as_lisp_string() {
        Value::heap_string(string.clone())
    } else if let Some(name) = value.as_symbol_name() {
        Value::string(name)
    } else if let Some(number) = value.as_fixnum() {
        Value::string(number.to_string())
    } else {
        Value::string(format!("{value}"))
    }
}

fn root_new_interactive_args(
    eval: &mut Context,
    args: &[Value],
    visible_args: &mut Vec<Value>,
    args_before: usize,
) {
    for value in &args[args_before..] {
        eval.push_specpdl_root(*value);
    }
    for value in &args[args_before..] {
        let visible = interactive_visible_arg(*value);
        eval.push_specpdl_root(visible);
        visible_args.push(visible);
    }
}

fn invalid_interactive_control_letter_error(letter: char) -> Flow {
    let codepoint = letter as u32;
    signal(
        "error",
        vec![Value::string(format!(
            "Invalid control letter \u{2018}{letter}\u{2019} (#o{codepoint:o}, #x{codepoint:04x}) in interactive calling string"
        ))],
    )
}

fn interactive_args_from_string_code(
    eval: &mut Context,
    code: &crate::heap_types::LispString,
    kind: CommandInvocationKind,
    context: &mut InteractiveInvocationContext,
) -> Result<Option<Vec<Value>>, Flow> {
    let roots = eval.save_specpdl_roots();
    let result = (|| -> Result<Option<Vec<Value>>, Flow> {
        let parsed = parse_interactive_code_entries(code);
        interactive_apply_prefix_flags(eval, &parsed.prefix_flags, context)?;
        if parsed.entries.is_empty() {
            return Ok(Some(Vec::new()));
        }

        let mut args = Vec::new();
        let mut visible_args = Vec::new();
        // Args collected by EARLIER spec letters (fresh minibuffer strings,
        // key-sequence vectors, up-event conses) live only in these Rust
        // Vecs while LATER letters run arbitrary Lisp in the minibuffer.
        // Re-thread everything collected so far onto a single rooted holder
        // slot at each iteration (spec entries are few, so the rebuild is
        // trivially cheap).
        let args_root_slot = eval.push_specpdl_root_slot(Value::NIL);
        for (letter, prompt) in parsed.entries {
            let mut holder = Value::NIL;
            for value in args.iter().chain(context.pending_up_event.iter()) {
                if value.is_heap_object() {
                    holder = Value::cons(*value, holder);
                }
            }
            eval.set_specpdl_root_slot(&args_root_slot, holder);
            let prompt = interactive_prompt_with_visible_args(eval, &prompt, &visible_args)?;
            let args_before = args.len();
            let control = InteractiveControlLetter::from_char(letter)
                .ok_or_else(|| invalid_interactive_control_letter_error(letter))?;
            match control {
                InteractiveControlLetter::FunctionName => {
                    args.push(super::minibuffer::builtin_read_command(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::ExistingBuffer => {
                    args.push(super::minibuffer::builtin_read_buffer(
                        eval,
                        vec![
                            Value::heap_string(prompt),
                            interactive_current_buffer_default(&eval.buffers),
                            Value::T,
                        ],
                    )?)
                }
                InteractiveControlLetter::Buffer => {
                    let default = interactive_other_buffer_default(&mut eval.buffers);
                    args.push(super::minibuffer::builtin_read_buffer(
                        eval,
                        vec![Value::heap_string(prompt), default, Value::NIL],
                    )?)
                }
                InteractiveControlLetter::Character => args.push(super::reader::builtin_read_char(
                    eval,
                    vec![Value::heap_string(prompt)],
                )?),
                InteractiveControlLetter::Command => {
                    args.push(super::minibuffer::builtin_read_command(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::Point => args.push(interactive_point_arg(eval)?),
                InteractiveControlLetter::DirectoryName => {
                    args.push(super::minibuffer::builtin_read_directory_name(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::InvokingEvent => {
                    if let Some(event) = interactive_next_event_with_parameters(eval, context) {
                        args.push(event);
                    } else {
                        return Err(signal(
                            "error",
                            vec![Value::string(
                                "command must be bound to an event with parameters",
                            )],
                        ));
                    }
                }
                InteractiveControlLetter::ExistingFile => {
                    args.push(super::minibuffer::builtin_read_file_name(
                        eval,
                        vec![Value::heap_string(prompt), Value::NIL, Value::NIL, Value::T],
                    )?)
                }
                InteractiveControlLetter::File => {
                    args.push(super::minibuffer::builtin_read_file_name(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::FileWithDirectoryDefault => {
                    args.push(super::minibuffer::builtin_read_file_name(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::Ignore => args.push(Value::NIL),
                InteractiveControlLetter::KeySequence => {
                    let arg = super::reader::builtin_read_key_sequence(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?;
                    interactive_capture_up_event_in_eval(eval, &arg, context)?;
                    args.push(arg);
                }
                InteractiveControlLetter::KeySequenceVector => {
                    let arg = super::reader::builtin_read_key_sequence_vector(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?;
                    interactive_capture_up_event_in_eval(eval, &arg, context)?;
                    args.push(arg);
                }
                InteractiveControlLetter::StringWithInputMethod => args.push(
                    super::reader::builtin_read_string(eval, vec![Value::heap_string(prompt)])?,
                ),
                InteractiveControlLetter::Mark => args.push(interactive_mark_arg(eval)?),
                InteractiveControlLetter::NumberOrPrefix => {
                    let raw = interactive_prefix_raw_arg(eval, kind);
                    if raw.is_nil() {
                        args.push(read_number_through_the_function_cell(
                            eval,
                            Value::heap_string(prompt),
                        )?);
                    } else {
                        args.push(Value::fixnum(prefix_numeric_value(&raw)));
                    }
                }
                InteractiveControlLetter::NumericPrefix => {
                    args.push(interactive_prefix_numeric_arg(eval, kind))
                }
                InteractiveControlLetter::RawPrefix => {
                    args.push(interactive_prefix_raw_arg(eval, kind))
                }
                InteractiveControlLetter::Region => {
                    args.extend(interactive_region_args(eval, "error")?)
                }
                InteractiveControlLetter::ActiveRegion => {
                    let use_region = eval
                        .apply(Value::symbol("use-region-p"), vec![])?
                        .is_truthy();
                    if use_region {
                        args.extend(interactive_region_args(eval, "error")?);
                    } else {
                        args.push(Value::NIL);
                        args.push(Value::NIL);
                    }
                }
                InteractiveControlLetter::Symbol => {
                    let sym_name =
                        super::reader::builtin_read_string(eval, vec![Value::heap_string(prompt)])?;
                    if let Some(name) = sym_name.as_utf8_str() {
                        args.push(Value::symbol(name));
                    } else {
                        return Ok(None);
                    }
                }
                InteractiveControlLetter::String => args.push(super::reader::builtin_read_string(
                    eval,
                    vec![Value::heap_string(prompt)],
                )?),
                InteractiveControlLetter::Number => args.push(
                    read_number_through_the_function_cell(eval, Value::heap_string(prompt))?,
                ),
                InteractiveControlLetter::Expression => {
                    args.push(interactive_read_expression_arg(eval, prompt)?)
                }
                InteractiveControlLetter::EvalExpression => {
                    let expr_value = interactive_read_expression_arg(eval, prompt)?;
                    args.push(eval.eval_value(&expr_value)?);
                }
                InteractiveControlLetter::UpEvent => args.push(interactive_u_arg(context)),
                InteractiveControlLetter::Variable => {
                    args.push(super::minibuffer::builtin_read_variable(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::CodingSystem => {
                    args.push(super::lread::builtin_read_coding_system(
                        eval,
                        vec![Value::heap_string(prompt)],
                    )?)
                }
                InteractiveControlLetter::CodingSystemWithPrefix => {
                    let raw = interactive_prefix_raw_arg(eval, kind);
                    if raw.is_nil() {
                        args.push(Value::NIL);
                    } else {
                        args.push(interactive_read_coding_system_optional_arg(eval, prompt)?);
                    }
                }
            }
            root_new_interactive_args(eval, &args, &mut visible_args, args_before);
        }

        Ok(Some(args))
    })();
    eval.restore_specpdl_roots(roots);
    result
}

fn eval_interactive_form_value(
    eval: &mut super::eval::Context,
    spec: InteractiveFormSpec,
) -> Result<Vec<Value>, Flow> {
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(spec.form);
    eval.push_specpdl_root(spec.environment.lexical_arg());
    let result = (|| -> Result<Vec<Value>, Flow> {
        let value =
            eval.eval_value_with_lexical_arg(spec.form, Some(spec.environment.lexical_arg()))?;
        interactive_form_value_to_args(value)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

fn resolve_command_target_in_state(
    obarray: &Obarray,
    designator: &Value,
) -> Option<(Option<SymId>, Value)> {
    if let Some(symbol) = designator.as_symbol_id() {
        if let Some((resolved_symbol, value)) =
            resolve_function_designator_symbol_in_state(obarray, symbol)
        {
            return Some((Some(resolved_symbol), value));
        }
        return None;
    }
    match designator.kind() {
        ValueKind::Subr(id) => Some((Some(id), *designator)),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = designator.as_subr_id().unwrap();
            Some((Some(id), *designator))
        }
        _ => Some((designator.as_symbol_id(), *designator)),
    }
}

/// Command identity that belongs to the caller of `call-interactively`.
///
/// GNU saves these four variables together in `Fcall_interactively` and
/// restores them together after interactive argument acquisition. Keeping
/// them in one Rust value prevents an interpreter or bytecode call path from
/// restoring only a subset and exposing a mixed command identity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CallInteractivelyCommandIdentity {
    this_command: Value,
    this_original_command: Value,
    real_this_command: Value,
    last_command: Value,
}

impl CallInteractivelyCommandIdentity {
    pub(crate) fn capture(eval: &Context) -> Self {
        Self {
            this_command: eval.eval_symbol("this-command").unwrap_or(Value::NIL),
            this_original_command: eval
                .eval_symbol("this-original-command")
                .unwrap_or(Value::NIL),
            real_this_command: eval.eval_symbol("real-this-command").unwrap_or(Value::NIL),
            last_command: eval.eval_symbol("last-command").unwrap_or(Value::NIL),
        }
    }

    pub(crate) fn push_gc_roots(self, eval: &mut Context) {
        for value in self.values() {
            eval.push_specpdl_root(value);
        }
    }

    pub(crate) fn values(self) -> [Value; 4] {
        [
            self.this_command,
            self.this_original_command,
            self.real_this_command,
            self.last_command,
        ]
    }

    fn restore(self, eval: &mut Context) {
        eval.assign("this-command", self.this_command);
        eval.assign("this-original-command", self.this_original_command);
        eval.assign("real-this-command", self.real_this_command);
        eval.assign("last-command", self.last_command);
    }
}

pub(crate) struct CallInteractivelyPlan {
    invocation_function: Value,
    func: Value,
    interactive_spec: ParsedInteractiveSpec,
    context: InteractiveInvocationContext,
    record_flag: bool,
    command_identity: CallInteractivelyCommandIdentity,
}

impl CallInteractivelyPlan {
    pub(crate) fn gc_roots(&self) -> Vec<Value> {
        let mut roots = vec![self.invocation_function, self.func];
        roots.extend(self.command_identity.values());
        if let ParsedInteractiveSpec::Form(spec) = self.interactive_spec {
            roots.push(spec.form);
            roots.push(spec.environment.lexical_arg());
        }
        roots
    }

    /// Consume argument-acquisition state and make the command invocable.
    ///
    /// The invocation function remains private until this transition restores
    /// GNU's saved command identity, so call paths outside this module cannot
    /// accidentally invoke a command from an unrestored plan.
    pub(crate) fn restore_for_invocation(
        self,
        eval: &mut Context,
    ) -> RestoredCallInteractivelyInvocation {
        self.command_identity.restore(eval);
        RestoredCallInteractivelyInvocation {
            function: self.invocation_function,
        }
    }
}

/// A `call-interactively` target whose caller command identity is restored.
pub(crate) struct RestoredCallInteractivelyInvocation {
    function: Value,
}

impl RestoredCallInteractivelyInvocation {
    pub(crate) fn into_funcall_args(self, call_args: Vec<Value>) -> Vec<Value> {
        let mut funcall_args = Vec::with_capacity(call_args.len() + 1);
        funcall_args.push(self.function);
        funcall_args.extend(call_args);
        funcall_args
    }
}

pub(crate) fn plan_call_interactively_after_interactive_form_in_state(
    obarray: &Obarray,
    read_command_keys: &[Value],
    args: &[Value],
    interactive_form: Value,
    command_identity: CallInteractivelyCommandIdentity,
) -> Result<CallInteractivelyPlan, Flow> {
    validate_call_interactively_args(args)?;

    let func_val = args[0];
    if !interactive_form.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("commandp"), func_val],
        ));
    }
    let Some((_, func)) = resolve_command_target_in_state(obarray, &func_val) else {
        return Err(signal(LispCondition::VoidFunction, vec![func_val]));
    };
    let interactive_spec = prepare_call_interactively_spec(
        func_val,
        interactive_form,
        InteractiveFormEnvironment::for_callable(func),
    )?;
    let context =
        InteractiveInvocationContext::from_keys_arg_in_state(read_command_keys, args.get(2));
    let record_flag = args.get(1).is_some_and(|value| value.is_truthy());
    Ok(CallInteractivelyPlan {
        invocation_function: func_val,
        func,
        interactive_spec,
        context,
        record_flag,
        command_identity,
    })
}

/// The `command-history` form of each argument a string spec produces.
///
/// Returns `None` when the flattened table does not cover the arguments that
/// were actually resolved, which can only happen if a spec letter's argument
/// count drifts from `history_forms`; recording every argument by value is
/// then still correct for all but the position letters.
fn string_code_history_forms(
    code: &crate::heap_types::LispString,
    arg_count: usize,
) -> Option<Vec<ArgHistoryForm>> {
    let mut forms = Vec::with_capacity(arg_count);
    for (letter, _) in parse_interactive_code_entries(code).entries {
        forms.extend_from_slice(InteractiveControlLetter::from_char(letter)?.history_forms());
    }
    (forms.len() == arg_count).then_some(forms)
}

/// GNU's `fix_command` (callint.c:175), which runs only for a Lisp-form
/// interactive spec.  It substitutes the replacements a command declares in its
/// `interactive-args` property, then drops trailing nil optional arguments so
/// the entry reads the way the user would type it.
fn fix_recorded_command_args(eval: &mut Context, function: Value, args: &mut Vec<Value>) {
    if args.is_empty() || function.as_symbol_id().is_none() {
        return;
    }

    let reps = function
        .as_symbol_id()
        .and_then(|symbol| {
            eval.obarray
                .get_property_id(symbol, intern("interactive-args"))
        })
        .unwrap_or(Value::NIL);
    if reps.is_cons() {
        for (index, arg) in args.iter_mut().enumerate() {
            let key = Value::fixnum(index as i64);
            let mut tail = reps;
            while tail.is_cons() {
                let entry = tail.cons_car();
                if entry.is_cons() && entry.cons_car() == key {
                    *arg = entry.cons_cdr();
                    break;
                }
                tail = tail.cons_cdr();
            }
        }
    }

    // A `&rest' function has no fixed maximum arity, and GNU deliberately
    // leaves its trailing nils alone: they may be meaningful positionals.
    let Some((min_args, _max_args)) = fixed_arity_of_function(eval, function) else {
        return;
    };
    let Some(last_non_nil) = args.iter().rposition(|arg| !arg.is_nil()) else {
        return;
    };
    if last_non_nil > 0 && last_non_nil + 1 >= min_args {
        args.truncate(last_non_nil + 1);
    }
}

/// The `(MIN . MAX)` arity of FUNCTION when both ends are fixnums, matching
/// GNU's `FIXNUMP (XCAR (arity)) && FIXNUMP (XCDR (arity))` guard.
fn fixed_arity_of_function(eval: &mut Context, function: Value) -> Option<(usize, usize)> {
    let arity = eval
        .apply(Value::symbol("func-arity"), vec![function])
        .ok()?;
    if !arity.is_cons() {
        return None;
    }
    let min = arity.cons_car().as_fixnum()?;
    let max = arity.cons_cdr().as_fixnum()?;
    Some((min.max(0) as usize, max.max(0) as usize))
}

fn record_call_interactively_command_history(
    eval: &mut Context,
    invocation_function: Value,
    interactive_spec: &ParsedInteractiveSpec,
    call_args: &[Value],
) -> Result<(), Flow> {
    let mut recorded: Vec<Value> = match interactive_spec {
        ParsedInteractiveSpec::StringCode(code) => {
            let forms = string_code_history_forms(code, call_args.len());
            call_args
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    forms
                        .as_ref()
                        .map_or(ArgHistoryForm::ByValue, |forms| forms[index])
                        .record(*arg)
                })
                .collect()
        }
        _ => {
            let mut args: Vec<Value> = call_args.iter().copied().map(quotify_history_arg).collect();
            fix_recorded_command_args(eval, invocation_function, &mut args);
            args
        }
    };

    let mut command = Vec::with_capacity(recorded.len() + 1);
    command.push(invocation_function);
    command.append(&mut recorded);
    let command = Value::list(command);

    if eval.obarray.fboundp("add-to-history") {
        let _ = eval.apply(
            Value::symbol("add-to-history"),
            vec![
                Value::symbol("command-history"),
                command,
                Value::NIL,
                Value::T,
            ],
        )?;
    } else {
        let existing = eval.eval_symbol("command-history").unwrap_or(Value::NIL);
        eval.assign("command-history", Value::cons(command, existing));
    }
    Ok(())
}

pub(crate) fn finish_call_interactively_in_eval(
    eval: &mut Context,
    mut plan: CallInteractivelyPlan,
) -> EvalResult {
    let roots = eval.save_specpdl_roots();
    for value in plan.gc_roots() {
        eval.push_specpdl_root(value);
    }
    let minibuffer_reads_before = eval.interactive_minibuffer_read_count();
    let resolved = resolve_call_interactively_target_and_args_in_eval(eval, &mut plan);
    let (_, call_args) = match resolved {
        Ok(resolved) => resolved,
        Err(error) => {
            eval.restore_specpdl_roots(roots);
            return Err(error);
        }
    };
    let should_record =
        plan.record_flag || eval.interactive_minibuffer_read_count() != minibuffer_reads_before;
    for value in &call_args {
        eval.push_specpdl_root(*value);
    }
    let result = (|| -> EvalResult {
        if should_record {
            record_call_interactively_command_history(
                eval,
                plan.invocation_function,
                &plan.interactive_spec,
                &call_args,
            )?;
        }
        // GNU callint.c restores all four saved command variables only after
        // argument acquisition/history recording, immediately before the
        // target function runs (src/callint.c:340-343,796-799).
        let invocation = plan.restore_for_invocation(eval);
        let funcall_args = invocation.into_funcall_args(call_args);
        eval.apply(Value::symbol("funcall-interactively"), funcall_args)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

pub(crate) fn resolve_call_interactively_target_and_args_in_eval(
    eval: &mut Context,
    plan: &mut CallInteractivelyPlan,
) -> Result<(Value, Vec<Value>), Flow> {
    let func = plan.func;
    let call_args = match &plan.interactive_spec {
        ParsedInteractiveSpec::NoArgs => Vec::new(),
        ParsedInteractiveSpec::StringCode(code) => interactive_args_from_string_code(
            eval,
            code,
            CommandInvocationKind::CallInteractively,
            &mut plan.context,
        )?
        .unwrap_or_default(),
        ParsedInteractiveSpec::Form(form) => eval_interactive_form_value(eval, *form)?,
    };
    Ok((func, call_args))
}

#[allow(dead_code, clippy::too_many_arguments)] // explicit split-state compatibility seam
pub(crate) fn resolve_call_interactively_target_and_args_in_state(
    obarray: &mut Obarray,
    dynamic: &mut Vec<OrderedRuntimeBindingMap>,
    buffers: &mut crate::buffer::BufferManager,
    custom: &crate::emacs_core::custom::CustomManager,
    specpdl: &[crate::emacs_core::eval::SpecBinding],
    plan: &mut CallInteractivelyPlan,
) -> Result<Option<(Value, Vec<Value>)>, Flow> {
    let func = plan.func;
    match &plan.interactive_spec {
        ParsedInteractiveSpec::NoArgs => Ok(Some((func, Vec::new()))),
        ParsedInteractiveSpec::StringCode(code) => interactive_args_from_string_code_in_state(
            obarray,
            dynamic,
            buffers,
            custom,
            specpdl,
            code,
            CommandInvocationKind::CallInteractively,
            &mut plan.context,
        )
        .map(|maybe_args| maybe_args.map(|args| (func, args))),
        ParsedInteractiveSpec::Form(_) => Ok(None),
    }
}

pub(crate) fn resolve_call_interactively_target_and_args_in_vm_runtime(
    shared: &mut super::eval::Context,
    plan: &mut CallInteractivelyPlan,
) -> Result<Option<(Value, Vec<Value>)>, Flow> {
    let func = plan.func;
    match plan.interactive_spec.clone() {
        ParsedInteractiveSpec::NoArgs => Ok(Some((func, Vec::new()))),
        ParsedInteractiveSpec::StringCode(code) => interactive_args_from_string_code_in_vm_runtime(
            shared,
            &code,
            CommandInvocationKind::CallInteractively,
            &mut plan.context,
        )
        .map(|maybe_args| maybe_args.map(|args| (func, args))),
        ParsedInteractiveSpec::Form(form) => {
            eval_interactive_form_value(shared, form).map(|args| Some((func, args)))
        }
    }
}

pub(crate) fn resolve_call_interactively_target_and_args_with_vm_fallback(
    shared: &mut super::eval::Context,
    plan: &mut CallInteractivelyPlan,
) -> Result<(Value, Vec<Value>), Flow> {
    if let Some((function, call_args)) =
        resolve_call_interactively_target_and_args_in_vm_runtime(shared, plan)?
    {
        return Ok((function, call_args));
    }

    let roots = shared.save_specpdl_roots();
    let result = resolve_call_interactively_target_and_args_in_eval(shared, plan);
    shared.restore_specpdl_roots(roots);
    result
}

/// `(self-insert-command N &optional C)` -- insert character C (or the last
/// typed character) N times.
///
/// Matches GNU Emacs cmds.c `Fself_insert_command`:
///   - arg 1 (N): repeat count (required, fixnum)
///   - arg 2 (C): character to insert (optional; nil → use `last-command-event`)
///     When C is provided and non-nil, `last-command-event` is also set to C.
pub(crate) fn builtin_self_insert_command(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("self-insert-command"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // CHECK_FIXNUM (n)
    let repeats = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), args[0]],
            ));
        }
    };
    if repeats < 0 {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Negative repetition argument {}",
                repeats
            ))],
        ));
    }

    // GNU: if (NILP (c)) c = last_command_event; else last_command_event = c;
    let c_arg = args.get(1).copied().unwrap_or(Value::NIL);
    let c = if c_arg.is_nil() {
        dynamic_or_global_symbol_value(eval, "last-command-event").unwrap_or(Value::NIL)
    } else {
        eval.assign("last-command-event", c_arg);
        c_arg
    };

    if repeats == 0 {
        return Ok(Value::NIL);
    }

    // GNU cmds.c:Fself_insert_command calls `undo-auto-amalgamate`
    // for single-char interactive insertions so repeated typing groups
    // correctly while still leaving command boundaries for `undo`.
    if repeats < 2 {
        eval.apply(Value::symbol("undo-auto-amalgamate"), vec![])?;
    }

    // Barf if the key that invoked this was not a character.
    let ch = match c.kind() {
        ValueKind::Fixnum(code) => {
            if let Some(ch) = char::from_u32(code as u32) {
                ch
            } else {
                // bitch_at_user — beep/ding
                tracing::warn!("self-insert-command: not a valid character: {}", code);
                return Ok(Value::NIL);
            }
        }
        _ => {
            tracing::warn!(
                "self-insert-command: last-command-event is not a character: {}",
                c
            );
            return Ok(Value::NIL);
        }
    };

    eval.apply(Value::symbol("barf-if-buffer-read-only"), vec![])?;

    // GNU `internal_self_insert` (cmds.c) implements overwrite-mode by deleting
    // (and, where a wider char would shift text, padding) the characters that C
    // overwrites, then inserting C.  Compute how many following characters to
    // delete and how many trailing spaces to insert, mirroring GNU exactly,
    // before building the inserted text.
    let (chars_to_delete, spaces_to_insert) = self_insert_overwrite_plan(eval, ch, repeats)?;

    let abbrev = self_insert_expand_abbrev_at_word_boundary(eval, ch)?;
    if abbrev.suppress_self_insert {
        return Ok(Value::NIL);
    }

    let repeat_count = repeats as usize;
    let mut text = String::with_capacity(repeat_count * ch.len_utf8() + spaces_to_insert);
    for _ in 0..repeat_count {
        text.push(ch);
    }
    for _ in 0..spaces_to_insert {
        text.push(' ');
    }
    if let Some(current_id) = eval.buffers.current_buffer_id() {
        if chars_to_delete > 0 {
            // GNU `replace_range (PT, PT + chars_to_delete, string)` followed by
            // `Fforward_char (n)`: replace the overwritten run with the inserted
            // characters (plus any padding spaces), then move point past the
            // inserted characters so the cursor lands after C.
            let (point_pos, accessible_end) = eval
                .buffers
                .get(current_id)
                .map(|b| {
                    (
                        b.point_emacs_byte_pos(),
                        b.accessible_emacs_byte_region().end(),
                    )
                })
                .unwrap_or((EmacsBytePos::ZERO, EmacsBytePos::ZERO));
            let mut del_end = point_pos;
            for _ in 0..chars_to_delete {
                if del_end >= accessible_end {
                    break;
                }
                match eval
                    .buffers
                    .get(current_id)
                    .and_then(|b| b.char_after_emacs_byte_len(del_end))
                {
                    Some(len) => del_end = del_end.add_len(len),
                    None => break,
                }
            }
            let start_char = eval
                .buffers
                .get(current_id)
                .map(|b| b.emacs_byte_pos_to_lisp_char_pos(point_pos).as_i64())
                .unwrap_or(1);
            let end_char = eval
                .buffers
                .get(current_id)
                .map(|b| b.emacs_byte_pos_to_lisp_char_pos(del_end).as_i64())
                .unwrap_or(start_char);
            // delete-region followed by insert reproduces replace_range's net
            // effect; both route through signal_before/after_change like GNU.
            // GNU's `replace_range' leaves point at the replacement start and
            // then `Fforward_char (n)' advances it past the n inserted copies of
            // C (but not the trailing padding spaces).  neomacs `insert' instead
            // advances point past the whole inserted string, so set point
            // explicitly to start + n to match GNU.
            super::editfns::builtin_delete_region(
                eval,
                vec![Value::fixnum(start_char), Value::fixnum(end_char)],
            )?;
            // GNU `internal_self_insert' uses `replace_range (..., inherit=t)'
            // in overwrite mode (cmds.c), so the replacement inherits text
            // properties from the surrounding text.
            eval.apply(
                Value::symbol("insert-and-inherit"),
                vec![Value::string(text.clone())],
            )?;
            eval.apply(
                Value::symbol("goto-char"),
                vec![Value::fixnum(start_char + repeats)],
            )?;
        } else {
            // GNU `internal_self_insert' inserts with inheritance
            // (`insert_and_inherit', cmds.c), so a self-inserted character picks
            // up the rear-sticky text properties of the preceding character.
            // `insert-and-inherit' runs the before/after-change signals itself
            // and advances point past the inserted text.
            eval.apply(
                Value::symbol("insert-and-inherit"),
                vec![Value::string(text.clone())],
            )?;
        }
    } else {
        tracing::warn!("self-insert-command: no current buffer");
    }
    if self_insert_should_auto_fill(eval, ch)
        && !dynamic_or_global_symbol_value(eval, "auto-fill-function")
            .unwrap_or(Value::NIL)
            .is_nil()
    {
        // GNU `internal_self_insert' (src/cmds.c:484-492) straddles the filler
        // call with a one-character point step when the self-inserted character
        // was a newline: the filler has to see the line the newline just
        // terminated, not the empty line it opened.
        let straddle = NewlineFillStraddle::for_self_inserted_char(ch);
        straddle.step(eval, NewlineFillStep::BeforeFill)?;
        eval.apply(Value::symbol("internal-auto-fill"), vec![])?;
        straddle.step(eval, NewlineFillStep::AfterFill)?;
    }
    eval.apply(
        Value::symbol("run-hooks"),
        vec![Value::symbol("post-self-insert-hook")],
    )?;
    if abbrev.expanded {
        // GNU returns the "needs undo boundary" result from
        // `internal_self_insert` when `expand-abbrev` changed the buffer.
        eval.assign("undo-auto--this-command-amalgamating", Value::NIL);
    }
    Ok(Value::NIL)
}

#[derive(Clone, Copy, Debug, Default)]
struct SelfInsertAbbrevOutcome {
    expanded: bool,
    suppress_self_insert: bool,
}

/// Apply GNU `internal_self_insert`'s abbrev boundary policy before inserting
/// the triggering character.  Keeping the syntax check here makes direct
/// calls, keyboard macros, and the interactive command loop share one path.
fn self_insert_expand_abbrev_at_word_boundary(
    eval: &mut Context,
    inserted: char,
) -> Result<SelfInsertAbbrevOutcome, Flow> {
    if dynamic_or_global_symbol_value(eval, "abbrev-mode").is_none_or(|value| value.is_nil()) {
        return Ok(SelfInsertAbbrevOutcome::default());
    }

    let should_expand = eval
        .buffers
        .current_buffer_id()
        .and_then(|buffer_id| eval.buffers.get(buffer_id))
        .is_some_and(|buffer| {
            let syntax = super::syntax::SyntaxTable::for_buffer(buffer);
            let point = buffer.point_emacs_byte_pos();
            point > buffer.accessible_emacs_byte_region().start()
                && syntax.char_syntax(inserted) != super::syntax::SyntaxClass::Word
                && buffer
                    .char_before_emacs_byte_pos(point)
                    .is_some_and(|previous| {
                        syntax.char_syntax(previous) == super::syntax::SyntaxClass::Word
                    })
        });
    if !should_expand {
        return Ok(SelfInsertAbbrevOutcome::default());
    }

    let expanded = eval.apply(Value::symbol("expand-abbrev"), vec![])?;
    if expanded.is_nil() {
        return Ok(SelfInsertAbbrevOutcome::default());
    }

    let suppress_self_insert = match expanded.kind() {
        ValueKind::Symbol(abbrev_symbol) => eval
            .obarray()
            .symbol_function_id(abbrev_symbol)
            .and_then(|hook| match hook.kind() {
                ValueKind::Symbol(hook_symbol) => eval
                    .obarray()
                    .get_property_id(hook_symbol, intern("no-self-insert")),
                _ => None,
            })
            .is_some_and(|property| property.is_truthy()),
        _ => false,
    };

    Ok(SelfInsertAbbrevOutcome {
        expanded: true,
        suppress_self_insert,
    })
}

/// Port of the overwrite-mode block of GNU `internal_self_insert` (cmds.c).
///
/// Returns `(chars_to_delete, spaces_to_insert)` describing how many following
/// characters C overwrites and how many trailing spaces are needed to keep the
/// remaining text from shifting.  Returns `(0, 0)` when not in overwrite mode,
/// at end of buffer, or for the special cases (newline) GNU leaves alone, in
/// which case the caller inserts C normally.
fn self_insert_overwrite_plan(eval: &mut Context, c: char, n: i64) -> Result<(usize, usize), Flow> {
    // overwrite = BVAR (current_buffer, overwrite_mode)
    let overwrite = eval
        .eval_symbol_by_id(crate::emacs_core::intern::intern("overwrite-mode"))
        .unwrap_or(Value::NIL);
    if overwrite.is_nil() {
        return Ok((0, 0));
    }
    let Some(current_id) = eval.buffers.current_buffer_id() else {
        return Ok((0, 0));
    };
    // Require PT < ZV (there must be a character to overwrite).
    let (point_pos, accessible_end, c2) = {
        let Some(buf) = eval.buffers.get(current_id) else {
            return Ok((0, 0));
        };
        let pt = buf.point_emacs_byte_pos();
        let zv = buf.accessible_emacs_byte_region().end();
        let c2 = buf.char_after_emacs_byte_pos(pt);
        (pt, zv, c2)
    };
    if point_pos >= accessible_end {
        return Ok((0, 0));
    }
    let Some(c2) = c2 else {
        return Ok((0, 0));
    };

    let binary = overwrite == Value::symbol("overwrite-mode-binary");
    if binary {
        // chars_to_delete = min (n, PTRDIFF_MAX)
        return Ok((n.max(0) as usize, 0));
    }

    // Textual overwrite: newlines are inserted in the usual way.
    if c == '\n' || c2 == '\n' {
        return Ok((0, 0));
    }

    // cwidth = char-width (c); a zero-width char is inserted normally.
    let cwidth = eval
        .apply(Value::symbol("char-width"), vec![Value::fixnum(c as i64)])?
        .as_fixnum()
        .unwrap_or(0);
    if cwidth == 0 {
        return Ok((0, 0));
    }

    // pos = PT; curcol = current_column (); target_clm = curcol + n*cwidth.
    let curcol = eval
        .apply(Value::symbol("current-column"), vec![])?
        .as_fixnum()
        .unwrap_or(0);
    let target_clm = curcol + n * cwidth;

    // actual_clm = move-to-column (target_clm); this moves point.
    let actual_clm = eval
        .apply(
            Value::symbol("move-to-column"),
            vec![Value::fixnum(target_clm)],
        )?
        .as_fixnum()
        .unwrap_or(target_clm);

    // chars_to_delete = PT - pos (characters between the original point and the
    // column-target point).
    let (new_point, prev_is_tab) = {
        let Some(buf) = eval.buffers.get(current_id) else {
            return Ok((0, 0));
        };
        let np = buf.point_emacs_byte_pos();
        // The character immediately before the new point (used only when we
        // overshoot the target column, e.g. landing inside a tab).
        let prev_is_tab = buf
            .char_before_emacs_byte_len(np)
            .map(|len| np.saturating_sub_len(len))
            .and_then(|prev| buf.char_after_emacs_byte_pos(prev))
            .map(|ch| ch == '\t')
            .unwrap_or(false);
        (np, prev_is_tab)
    };
    let mut chars_to_delete = byte_pos_char_distance(eval, current_id, point_pos, new_point);
    let mut spaces_to_insert = 0usize;

    if actual_clm > target_clm {
        // We will delete too many columns.  Keep a trailing tab whole, else
        // fill with spaces so the remaining text won't shift.
        if prev_is_tab {
            chars_to_delete = chars_to_delete.saturating_sub(1);
        } else {
            spaces_to_insert = (actual_clm - target_clm).max(0) as usize;
        }
    }

    // SET_PT (pos): restore point to where the cursor was before the trial move.
    let start_char = eval
        .buffers
        .get(current_id)
        .map(|b| b.emacs_byte_pos_to_lisp_char_pos(point_pos).as_i64())
        .unwrap_or(1);
    eval.buffers
        .goto_buffer_emacs_byte_pos(current_id, point_pos);
    let _ = start_char;

    Ok((chars_to_delete, spaces_to_insert))
}

/// Number of characters between two byte positions in BUF (BEG <= END).
fn byte_pos_char_distance(
    eval: &Context,
    buf_id: crate::buffer::BufferId,
    beg: EmacsBytePos,
    end: EmacsBytePos,
) -> usize {
    let Some(buf) = eval.buffers.get(buf_id) else {
        return 0;
    };
    let beg_char = buf.emacs_byte_pos_to_lisp_char_pos(beg).as_i64();
    let end_char = buf.emacs_byte_pos_to_lisp_char_pos(end).as_i64();
    (end_char - beg_char).max(0) as usize
}

/// Whether a self-inserted character needs GNU's newline point straddle around
/// `internal-auto-fill' (`internal_self_insert', src/cmds.c:484-492).  Making
/// this a value rather than an `if ch == '\n'` at each of the two call sites
/// keeps the two halves of the straddle from drifting apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NewlineFillStraddle {
    /// Not a newline: point already sits where GNU wants the filler to see it.
    NotNeeded,
    /// A newline: point steps back over it before filling, forward after.
    Needed,
}

/// Which side of the `internal-auto-fill' call a straddle step happens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NewlineFillStep {
    BeforeFill,
    AfterFill,
}

impl NewlineFillStraddle {
    fn for_self_inserted_char(ch: char) -> Self {
        if ch == '\n' {
            Self::Needed
        } else {
            Self::NotNeeded
        }
    }

    /// GNU's `SET_PT_BOTH (PT - 1, PT_BYTE - 1)` before the filler and
    /// `SET_PT_BOTH (PT + 1, PT_BYTE + 1)` after it.  GNU guards only the
    /// forward step, with `PT < ZV`, because a strange `auto-fill-function' may
    /// have left point at the end of the buffer.
    fn step(self, eval: &mut Context, step: NewlineFillStep) -> Result<(), Flow> {
        if self == Self::NotNeeded {
            return Ok(());
        }
        let Some(buffer) = eval
            .buffers
            .current_buffer_id()
            .and_then(|buffer_id| eval.buffers.get(buffer_id))
        else {
            return Ok(());
        };
        let point = buffer.point_lisp_char_pos().as_i64();
        let point_min = buffer.point_min_lisp_char_pos().as_i64();
        let point_max = buffer.point_max_lisp_char_pos().as_i64();
        let target = match step {
            NewlineFillStep::BeforeFill if point > point_min => point - 1,
            NewlineFillStep::AfterFill if point < point_max => point + 1,
            _ => return Ok(()),
        };
        eval.apply(Value::symbol("goto-char"), vec![Value::fixnum(target)])?;
        Ok(())
    }
}

fn self_insert_should_auto_fill(eval: &Context, ch: char) -> bool {
    let Some(auto_fill_chars) = dynamic_or_global_symbol_value(eval, "auto-fill-chars") else {
        return ch == ' ' || ch == '\n';
    };
    if crate::emacs_core::chartable::is_char_table(&auto_fill_chars) {
        return crate::emacs_core::chartable::ct_lookup(&auto_fill_chars, ch as i64)
            .map(|value| !value.is_nil())
            .unwrap_or(false);
    }
    ch == ' ' || ch == '\n'
}

/// `(keyboard-quit)` -- cancel the current command sequence.
/// `(key-binding KEY &optional ACCEPT-DEFAULTS NO-REMAP POSITION)`
/// Return the binding for KEY in the current keymaps.
pub(crate) fn builtin_key_binding(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_key_binding_impl(eval, args)
}

pub(crate) fn builtin_key_binding_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("key-binding", &args, 1)?;
    expect_max_args("key-binding", &args, 4)?;
    // GNU `Fkey_binding` (`src/keymap.c`) validates POSITION before
    // checking the key designator: an out-of-range integer position
    // signals `(args-out-of-range BUFFER POS)` even if the key arg is
    // garbage. Mirror that early-exit so we don't shadow the position
    // error with a `wrong-type-argument arrayp` from
    // key_events_from_designator below.
    if let Some(position) = args.get(3)
        && let ValueKind::Fixnum(pos_int) = position.kind()
        && let Some(buf_id) = ctx.buffers.current_buffer_id()
        && let Some(buf) = ctx.buffers.get(buf_id)
    {
        // Lisp positions are 1-based character positions, so
        // valid range is `[char_min + 1, char_max + 1]`.
        let lisp_min = buf.point_min_lisp_char_pos().as_i64();
        let lisp_max = buf.point_max_lisp_char_pos().as_i64();
        if pos_int < lisp_min || pos_int > lisp_max {
            let buffer_value = Value::make_buffer(buf_id);
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![buffer_value, *position],
            ));
        }
    }
    let string_designator = args[0].is_string();
    let no_remap = args.get(2).is_some_and(|v| v.is_truthy());
    let events = match super::kbd::key_events_from_designator(&args[0]) {
        Ok(events) => events,
        Err(super::kbd::KeyDesignatorError::WrongType(other)) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("arrayp"), other],
            ));
        }
        Err(super::kbd::KeyDesignatorError::Parse(_)) => {
            return Ok(Value::NIL);
        }
    };
    if events.is_empty() {
        if !string_designator {
            return Ok(Value::NIL);
        }
        let active_maps = current_active_maps_for_position(ctx, true, args.get(3))?;
        return Ok(Value::list(active_maps));
    }

    let emacs_events: Vec<Value> = events.iter().map(key_event_to_emacs_event).collect();
    let default_binding_mode = DefaultBindingMode::from(args.get(1).is_some_and(|v| v.is_truthy()));
    Ok(resolve_active_key_binding(
        ctx,
        &emacs_events,
        default_binding_mode,
        no_remap,
        args.get(3),
    )?
    .binding)
}

/// `(local-key-binding KEY &optional ACCEPT-DEFAULTS)`
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_local_key_binding(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_local_key_binding_impl(eval, args)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_local_key_binding_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("local-key-binding", &args, 1)?;
    expect_max_args("local-key-binding", &args, 2)?;

    if ctx.buffers.current_local_map().is_nil() {
        return Ok(Value::NIL);
    }

    let events = match super::kbd::key_events_from_designator(&args[0]) {
        Ok(events) => events,
        Err(super::kbd::KeyDesignatorError::WrongType(other)) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("arrayp"), other],
            ));
        }
        Err(super::kbd::KeyDesignatorError::Parse(_)) => {
            return Ok(Value::NIL);
        }
    };
    let emacs_events: Vec<Value> = events.iter().map(key_event_to_emacs_event).collect();
    Ok(lookup_keymap_with_partial(
        &ctx.buffers.current_local_map(),
        &emacs_events,
    ))
}

/// `(minor-mode-key-binding KEY &optional ACCEPT-DEFAULTS)`
/// Look up KEY in active minor mode keymaps.
pub(crate) fn builtin_minor_mode_key_binding(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_minor_mode_key_binding_impl(eval, args)
}

pub(crate) fn builtin_minor_mode_key_binding_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("minor-mode-key-binding", &args, 1)?;
    expect_max_args("minor-mode-key-binding", &args, 2)?;

    // Emacs returns nil (not a type error) for non-array key designators here.
    let events = match super::kbd::key_events_from_designator(&args[0]) {
        Ok(events) => events,
        Err(_) => return Ok(Value::NIL),
    };
    let emacs_events: Vec<Value> = events.iter().map(key_event_to_emacs_event).collect();
    minor_mode_key_binding_in_context(ctx, &emacs_events)
}

/// DEFINITION's non-nil `:advertised-binding` property, GNU's
/// `Fget (definition, QCadvertised_binding)` (keymap.c:2672).
///
/// A property whose value is nil is indistinguishable from an absent one in
/// GNU, whose guard is `!NILP (tem = Fget (...))`; both answer `None` here.
fn where_is_advertised_binding_property(obarray: &Obarray, definition: Value) -> Option<Value> {
    let name = definition.as_symbol_name()?;
    obarray
        .get_property(name, ":advertised-binding")
        .filter(|property| !property.is_nil())
}

/// The candidate key sequences GNU offers to `shadow_lookup`, in GNU's order.
///
/// GNU walks the property as a DOTTED chain rather than as a list: every
/// `CONSP` car in turn, and then whatever the chain ends in
/// (keymap.c:2677-2683 -- `while (CONSP (tem)) ... XCAR (tem) ... tem = XCDR
/// (tem);` followed by one more check on `tem` itself).  Two consequences fall
/// straight out of that shape rather than needing special cases:
///
/// * a property that is not a list -- the usual `[?\C- ]` -- is its own only
///   candidate, and
/// * a proper LIST that matches nothing ends by offering `nil`, which is not
///   an array, which is why GNU signals `(wrong-type-argument arrayp nil)`
///   there instead of falling back to the reverse search.  Measured, not
///   inferred: GNU 31.0.90 signals exactly that.
fn where_is_advertised_binding_candidates(property: Value) -> impl Iterator<Item = Value> {
    let mut cursor = Some(property);
    std::iter::from_fn(move || {
        let value = cursor?;
        if value.is_cons() {
            cursor = Some(value.cons_cdr());
            Some(value.cons_car())
        } else {
            cursor = None;
            Some(value)
        }
    })
}

/// The first advertised sequence that still resolves to DEFINITION, or `None`
/// to fall through to the reverse search.
///
/// GNU verifies each candidate with `shadow_lookup (keymaps, KEY, Qnil, 0)`
/// and compares with `EQ` (keymap.c:2678), so an advertised key that has since
/// been rebound -- or that was never DEFINITION's -- is simply ignored.  A
/// too-long sequence makes `lookup-key` answer a fixnum, which `shadow_lookup`
/// maps to nil (keymap.c:2470) and which therefore cannot match.
fn where_is_advertised_binding_sequence(
    eval: &mut Context,
    keymaps: &[Value],
    property: Value,
    definition: Value,
) -> Result<Option<Value>, Flow> {
    for candidate in where_is_advertised_binding_candidates(property) {
        // GNU reaches `Flookup_key`'s `CHECK_VECTOR_OR_STRING` here, so a
        // candidate that is not a key sequence SIGNALS rather than being
        // skipped -- including the nil that ends an exhausted proper list.
        let events = crate::emacs_core::builtins::keymaps::expect_key_events(&candidate)?;
        let found = lookup_key_in_keymaps_in_obarray_runtime(eval, keymaps, &events, false)?;
        if !found.is_fixnum() && eq_value(&found, &definition) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// GNU's "filter out non key events" test (keymap.c:2745-2750): a ONE-element
/// sequence naming a symbol whose `non-key-event` property is non-nil.
///
/// `lisp/keymap.el:798`'s `make-non-key-event` sets that property, and
/// `lisp/term/ns-win.el:177-179` uses it for events like `ns-power-off` that
/// the system delivers through a keymap but that no user can type.  Reporting
/// one as a binding would tell the user to press a key that does not exist.
fn where_is_sequence_is_non_key_event(obarray: &Obarray, sequence: &[Value]) -> bool {
    let [event] = sequence else {
        return false;
    };
    let Some(name) = event.as_symbol_name() else {
        return false;
    };
    obarray
        .get_property(name, "non-key-event")
        .is_some_and(|value| !value.is_nil())
}

/// `(where-is-internal DEFINITION &optional KEYMAP FIRSTONLY NOINDIRECT NO-REMAP)`
/// Return list of key sequences that invoke DEFINITION.
pub(crate) fn builtin_where_is_internal(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("where-is-internal", &args, 1)?;
    expect_max_args("where-is-internal", &args, 5)?;

    // GNU `Fwhere_is_internal` parses `Vwhere_is_preferred_modifier` into its
    // C-side mask exactly once, before keymap discovery or menu-item filters
    // can run Lisp.  Carry the typed snapshot through candidate selection so
    // its identity and timing cannot drift into the inner sequence loop.
    let preferred_modifier = WhereIsPreferredModifier::snapshot(eval.obarray());
    let mut definition = args[0];
    let first_only = args.get(2).is_some_and(|v| !v.is_nil());
    let first_only_non_ascii = args
        .get(2)
        .and_then(|value| value.as_symbol_name())
        .is_some_and(|name| name == "non-ascii");
    let prefer_single_binding = first_only && !first_only_non_ascii;
    let no_menu_bindings = prefer_single_binding;
    // 4th arg NOINDIRECT: when non-nil, don't extract the command out of a
    // menu-item / `(STRING . DEFN)` wrapper before matching (GNU
    // `where_is_internal_1`: `if (!noindirect) binding = get_keyelt (binding, 0)`).
    let noindirect = args.get(3).is_some_and(|v| v.is_truthy());
    let no_remap = args.get(4).is_some_and(|v| v.is_truthy());

    let keymaps = where_is_keymaps_in_context(eval, args.get(1))?;
    if args.get(1).is_none() && keymaps.is_empty() {
        return Ok(Value::NIL);
    }

    // Mirror GNU `Fwhere_is_internal`: if DEFINITION is itself remapped to some
    // other command, search for the keys bound to that remap target instead.
    // (keymap.c: `tem = Fcommand_remapping (definition, Qnil, keymaps); if
    // (NILP (no_remap) && !NILP (tem)) definition = tem;`)
    if !no_remap && let Some(command_name) = command_remapping_command_name(&definition) {
        let target = if no_menu_bindings && !noindirect {
            where_is_indexed_command_remapping(eval, &keymaps, command_name)
        } else {
            command_remapping_lookup_in_keymaps(&keymaps, command_name)
        };
        if let Some(target) = target.filter(|target| !target.is_nil()) {
            definition = target;
        }
    }

    // GNU `Fwhere_is_internal` answers from the symbol's `:advertised-binding`
    // property before it ever runs the reverse search, whenever FIRSTONLY is
    // non-nil (keymap.c:2669-2684).  This is what decides how a command is
    // NAMED to the user: `lisp/bindings.el:1331-1334` binds `set-mark-command`
    // to BOTH `C-@` (ASCII NUL, 0) and `C-SPC` (32 with the 2**26 control bit,
    // 67108896) and advertises the second, so GNU renders
    // `\\[set-mark-command]` as `C-SPC` while a port without this block
    // renders `C-@`.
    //
    // Placement matters: it sits AFTER the remap substitution above, so the
    // property consulted is the remap TARGET's.
    let advertised = if first_only && definition.is_symbol() {
        where_is_advertised_binding_property(eval.obarray(), definition)
    } else {
        None
    };
    if let Some(property) = advertised
        && let Some(sequence) =
            where_is_advertised_binding_sequence(eval, &keymaps, property, definition)?
    {
        return Ok(sequence);
    }

    // Collect the raw candidate sequences (longest-to-shortest, possibly
    // including raw `[remap COMMAND]` pseudo-keys, matching GNU's
    // `where_is_internal`).
    let sequences =
        where_is_raw_sequences(eval, &keymaps, definition, no_menu_bindings, noindirect);

    // Now mirror `Fwhere_is_internal`'s post-processing: expand `[remap COMMAND]`
    // pseudo-keys into the real key sequences that run COMMAND, and never leak
    // the raw pseudo-key into the result.  Remapped sequences are processed
    // after the non-remapped ones, since non-remapped bindings are preferred.
    let mut found: Vec<Vec<Value>> = Vec::new();
    // Membership shadow for `found`: Value's Hash follows `equal`, so the
    // set replaces a linear deep-equal scan per candidate sequence.
    let mut found_set: std::collections::HashSet<Vec<Value>> = std::collections::HashSet::new();
    let mut remapped_sequences: Vec<Vec<Value>> = Vec::new();
    let mut work: std::collections::VecDeque<Vec<Value>> = sequences.into();
    let mut remapped = false;
    // GNU's `sequence` at the moment of its `firstonly = non-ascii` early
    // return: the first candidate that survived `shadow_lookup`, filtered or
    // not.
    let mut first_unshadowed: Option<Vec<Value>> = None;
    loop {
        if work.is_empty() {
            if remapped {
                break;
            }
            // Switch over to the sequences discovered via remapping.
            work = std::mem::take(&mut remapped_sequences).into();
            remapped = true;
            continue;
        }
        let sequence = work.pop_front().expect("work checked non-empty just above");

        // If this is a `[remap COMMAND]` pseudo-key, replace it with the key
        // sequences that actually run COMMAND (unless NO-REMAP suppresses it).
        if !no_remap
            && !remapped
            && let Some(function) = where_is_remap_pseudo_key_command(&sequence)
        {
            let mut seqs =
                where_is_raw_sequences(eval, &keymaps, function, no_menu_bindings, noindirect);
            // `collect_where_is_sequences_value` already returns sequences
            // in the public `where-is-internal` order.  GNU reverses here
            // because its lower-level `where_is_internal` helper returns
            // the internal cons order; reversing here would make later
            // global bindings outrank earlier active maps.
            seqs.append(&mut remapped_sequences);
            remapped_sequences = seqs;
            continue;
        }

        // GNU `Fwhere_is_internal` verifies every reverse-lookup candidate
        // through `shadow_lookup`, which calls `lookup-key` with autoloading
        // enabled.  Besides rejecting bindings shadowed by a higher-precedence
        // map, this evaluates `menu-item :filter` properties.  A conditional
        // binding whose filter returns nil must therefore not be advertised by
        // `substitute-command-keys`.
        let visible_binding =
            lookup_key_in_keymaps_in_obarray_runtime(eval, &keymaps, &sequence, false)?;
        let visible_binding =
            if remapped && !visible_binding.is_nil() && !visible_binding.is_fixnum() {
                key_binding_apply_remap_in_active_maps(eval, &keymaps, visible_binding, false)?
            } else {
                visible_binding
            };
        if !binding_matches_definition(&visible_binding, &definition) {
            continue;
        }

        let sequence = metize_key_sequence(&sequence);
        // GNU records the first unshadowed match before any filtering, because
        // its `firstonly = non-ascii` early return (keymap.c:2756-2757) sits
        // OUTSIDE the `non-key-event` filter below and hands back whatever it
        // is looking at.  Measured: with a `non-key-event` binding as the only
        // one, GNU answers nil for FIRSTONLY t and `[SYM]` for `non-ascii`.
        if first_unshadowed.is_none() {
            first_unshadowed = Some(sequence.clone());
        }
        // "Filter out non key events" (keymap.c:2745-2750): a one-element
        // sequence naming a symbol with a non-nil `non-key-event` property is
        // a signal delivered through the keymap (`lisp/keymap.el:798`'s
        // `make-non-key-event`, used by `term/ns-win.el` for `ns-power-off`
        // and friends), not something a user can type, so it must never be
        // reported as a way to run the command.
        if where_is_sequence_is_non_key_event(eval.obarray(), &sequence) {
            continue;
        }
        if found_set.insert(sequence.clone()) {
            found.push(sequence);
        }
    }

    if first_only
        && first_only_non_ascii
        && let Some(sequence) = first_unshadowed
    {
        return Ok(Value::vector(sequence));
    }

    if found.is_empty() {
        return Ok(Value::NIL);
    }

    if first_only {
        // Convert Vec<Value> events to a vector value
        if prefer_single_binding {
            return Ok(Value::vector(
                select_where_is_preferred_sequence(preferred_modifier, &found).clone(),
            ));
        }
        return Ok(Value::vector(found[0].clone()));
    }
    let out: Vec<Value> = found.iter().map(|seq| Value::vector(seq.clone())).collect();
    Ok(Value::list(out))
}

/// Collapse an internal `ESC`-prefixed key sequence into GNU's meta-bit form:
/// the `meta-prefix-char` (ESC, 27) immediately followed by a character event is
/// replaced by that character with the meta modifier set (dropping the ESC),
/// mirroring GNU `where_is_internal`'s `is_metized` handling. So `[27 120]`
/// (ESC x) becomes `[134217848]` (M-x) -- the same collapse GNU applies to both
/// `M-x` and explicitly-bound `ESC x` sequences in the where-is result.
fn metize_key_sequence(seq: &[Value]) -> Vec<Value> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        if i + 1 < seq.len()
            && seq[i].as_fixnum() == Some(27)
            && let Some(next) = seq[i + 1].as_fixnum()
            && next & KEY_CHAR_META == 0
        {
            out.push(Value::fixnum(next | KEY_CHAR_META));
            i += 2;
            continue;
        }
        out.push(seq[i]);
        i += 1;
    }
    out
}

/// If `sequence` is a raw `[remap COMMAND]` pseudo-key (a 2-element key
/// sequence whose first event is the `remap` symbol and whose second event is a
/// command symbol), return COMMAND.  This mirrors GNU's check in
/// `Fwhere_is_internal`:
/// `VECTORP (sequence) && ASIZE (sequence) == 2 && EQ (AREF (sequence, 0), Qremap)`.
fn where_is_remap_pseudo_key_command(sequence: &[Value]) -> Option<Value> {
    if sequence.len() != 2 {
        return None;
    }
    if !KeymapMarker::Remap.is_value(sequence[0]) {
        return None;
    }
    let command = sequence[1];
    if command.as_symbol_id().is_some() {
        Some(command)
    } else {
        None
    }
}

/// `(this-command-keys)` -> string of keys that invoked current command.
pub(crate) fn builtin_this_command_keys(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_this_command_keys_impl(eval.read_command_keys(), args)
}

pub(crate) fn builtin_this_command_keys_impl(
    read_command_keys: &[Value],
    args: Vec<Value>,
) -> EvalResult {
    expect_args("this-command-keys", &args, 0)?;
    if !read_command_keys.is_empty() {
        return Ok(make_event_array_value(read_command_keys));
    }
    Ok(Value::string(""))
}

/// `(this-command-keys-vector)` -> vector of keys that invoked current command.
pub(crate) fn builtin_this_command_keys_vector(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_this_command_keys_vector_impl(eval.read_command_keys(), args)
}

pub(crate) fn builtin_this_command_keys_vector_impl(
    read_command_keys: &[Value],
    args: Vec<Value>,
) -> EvalResult {
    expect_args("this-command-keys-vector", &args, 0)?;
    if !read_command_keys.is_empty() {
        return Ok(Value::vector(read_command_keys.to_vec()));
    }
    Ok(Value::vector(Vec::<Value>::new()))
}

fn single_command_key_vector_in_state(read_command_keys: &[Value]) -> Value {
    if !read_command_keys.is_empty() {
        return Value::vector(read_command_keys.to_vec());
    }
    Value::vector(Vec::<Value>::new())
}

pub(crate) fn builtin_this_single_command_keys_impl(
    read_command_keys: &[Value],
    args: Vec<Value>,
) -> EvalResult {
    expect_args("this-single-command-keys", &args, 0)?;
    Ok(single_command_key_vector_in_state(read_command_keys))
}

pub(crate) fn builtin_this_single_command_raw_keys_impl(
    read_raw_command_keys: &[Value],
    args: Vec<Value>,
) -> EvalResult {
    expect_args("this-single-command-raw-keys", &args, 0)?;
    Ok(single_command_key_vector_in_state(read_raw_command_keys))
}

/// `(this-single-command-keys)` -> vector of keys that invoked current command.
pub(crate) fn builtin_this_single_command_keys(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_this_single_command_keys_impl(eval.read_command_keys(), args)
}

/// `(this-single-command-raw-keys)` -> vector of raw keys for current command.
pub(crate) fn builtin_this_single_command_raw_keys(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_this_single_command_raw_keys_impl(eval.read_raw_command_keys(), args)
}

/// `(clear-this-command-keys &optional KEEP-RECORD)` -> nil.
///
/// Clears current command-key context used by `this-command-keys*`.
/// When KEEP-RECORD is nil or omitted, also clears recent input history used
/// by `recent-keys`.
pub(crate) fn builtin_clear_this_command_keys(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_clear_this_command_keys_in_runtime(eval, args)
}

pub(crate) trait CommandKeyRuntime {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn read_command_keys(&self) -> &[Value];
    fn clear_command_key_state(&mut self, keep_record: bool);
}

impl CommandKeyRuntime for Context {
    fn read_command_keys(&self) -> &[Value] {
        Context::read_command_keys(self)
    }

    fn clear_command_key_state(&mut self, keep_record: bool) {
        Context::clear_command_key_state(self, keep_record);
    }
}

pub(crate) fn builtin_clear_this_command_keys_in_runtime(
    runtime: &mut impl CommandKeyRuntime,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("clear-this-command-keys", &args, 1)?;
    let keep_record = args.first().is_some_and(|arg| arg.is_truthy());
    runtime.clear_command_key_state(keep_record);
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn command_remapping_keymap_arg_valid(value: &Value) -> bool {
    // Oracle accepts cons/list keymap-like objects in this slot, not just valid keymaps.
    // Non-keymap cons cells are silently treated as "no remap found".
    value.is_cons() || is_list_keymap(value)
}

fn command_remapping_explicit_keymaps(ctx: &Context, value: &Value) -> Vec<Value> {
    if is_list_keymap(value) {
        return vec![*value];
    }

    let Some(items) = list_to_vec(value) else {
        return Vec::new();
    };
    if items.is_empty() {
        return Vec::new();
    }
    if get_keymap_in_obarray(&ctx.obarray, &items[0], false)
        .ok()
        .filter(is_list_keymap)
        .is_none()
    {
        return Vec::new();
    }

    items
        .into_iter()
        .filter_map(|item| {
            get_keymap_in_obarray(&ctx.obarray, &item, false)
                .ok()
                .filter(is_list_keymap)
        })
        .collect()
}

fn command_remapping_lookup_in_keymaps(keymaps: &[Value], command_name: SymId) -> Option<Value> {
    keymap_command_remapping_lookup_in_keymaps(keymaps, command_name)
}

fn command_remapping_command_name(command: &Value) -> Option<SymId> {
    keymap_command_remapping_command_name(command)
}

fn command_remapping_lookup_in_lisp_keymap(keymap: &Value, command_name: SymId) -> Option<Value> {
    keymap_command_remapping_lookup_in_lisp_keymap(keymap, command_name)
}

fn command_remapping_normalize_target(raw: Value) -> Value {
    keymap_command_remapping_normalize_target(raw)
}

fn binding_matches_definition(binding: &Value, definition: &Value) -> bool {
    if binding.is_nil() {
        return false;
    }
    if let Some(command) = menu_item_command(binding) {
        return binding_matches_definition(&command, definition);
    }
    // If binding is a keymap (prefix), it doesn't match a command definition
    if is_list_keymap(binding) {
        return false;
    }
    // Symbol comparison
    if let (Some(bname), Some(dname)) = (binding.as_symbol_name(), definition.as_symbol_name()) {
        return bname == dname;
    }
    // Subr comparison
    if let (Some(bid), Some(did)) = (binding.as_subr_id(), definition.as_subr_id()) {
        return bid == did;
    }
    // Check if binding is a symbol matching a Subr definition name
    if let Some(bname) = binding.as_symbol_name()
        && let Some(id) = definition.as_subr_id()
    {
        return bname == resolve_sym(id);
    }
    binding == definition
}

fn menu_item_command(binding: &Value) -> Option<Value> {
    if !binding.is_cons() || !KeymapMarker::MenuItem.is_value(binding.cons_car()) {
        return None;
    }
    let tail = binding.cons_cdr();
    if !tail.is_cons() {
        return None;
    }
    let after_label = tail.cons_cdr();
    if after_label.is_cons() {
        Some(after_label.cons_car())
    } else {
        None
    }
}

/// Parsed, immutable form of GNU's `Vwhere_is_preferred_modifier`.
///
/// The Lisp variable accepts GNU `parse_solitary_modifier` spellings, but the
/// selection algorithm needs only this closed subset.  Keeping the parsed
/// state in an enum prevents arbitrary integers or a fresh Lisp lookup from
/// entering the candidate loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WhereIsPreferredModifier {
    Unspecified,
    Control,
    Meta,
    Shift,
    Super,
    Hyper,
    Alt,
}

impl WhereIsPreferredModifier {
    fn snapshot(obarray: &Obarray) -> Self {
        static VARIABLE: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
        let variable = *VARIABLE.get_or_init(|| intern("where-is-preferred-modifier"));
        match obarray
            .symbol_value_id_copied(variable)
            .and_then(Value::as_symbol_name)
        {
            Some("C" | "ctrl" | "control") => Self::Control,
            Some("M" | "meta") => Self::Meta,
            Some("S" | "shift") => Self::Shift,
            Some("s" | "super") => Self::Super,
            Some("H" | "hyper") => Self::Hyper,
            Some("A" | "alt") => Self::Alt,
            _ => Self::Unspecified,
        }
    }

    const fn mask(self) -> i64 {
        match self {
            Self::Unspecified => 0,
            Self::Control => KEY_CHAR_CTRL,
            Self::Meta => KEY_CHAR_META,
            Self::Shift => KEY_CHAR_SHIFT,
            Self::Super => KEY_CHAR_SUPER,
            Self::Hyper => KEY_CHAR_HYPER,
            Self::Alt => KEY_CHAR_ALT,
        }
    }

    const fn is_specified(self) -> bool {
        !matches!(self, Self::Unspecified)
    }
}

/// Result of applying GNU `preferred_sequence_p` to one candidate.
///
/// GNU represents these states as 0/1/2.  The enum makes it impossible to
/// confuse a rejected sequence with a usable fallback at Rust call sites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WhereIsSequencePreference {
    Rejected,
    Acceptable,
    Preferred,
}

fn where_is_sequence_preference(
    preferred_modifier: WhereIsPreferredModifier,
    seq: &[Value],
) -> WhereIsSequencePreference {
    let preferred_mask = preferred_modifier.mask();
    let mut result = WhereIsSequencePreference::Acceptable;

    for event in seq {
        let Some(code) = event.as_fixnum() else {
            return WhereIsSequencePreference::Rejected;
        };
        let modifiers = code & (KEY_CHAR_MOD_MASK & !KEY_CHAR_META);
        if modifiers == preferred_mask {
            result = WhereIsSequencePreference::Preferred;
        } else if modifiers != 0 {
            return WhereIsSequencePreference::Rejected;
        }
    }

    result
}

fn select_where_is_preferred_sequence<'a>(
    preferred_modifier: WhereIsPreferredModifier,
    sequences: &'a [Vec<Value>],
) -> &'a Vec<Value> {
    if let Some(seq) = sequences.iter().find(|seq| {
        where_is_sequence_preference(preferred_modifier, seq)
            == WhereIsSequencePreference::Preferred
    }) {
        return seq;
    }

    if preferred_modifier.is_specified() {
        for seq in sequences {
            if where_is_sequence_preference(preferred_modifier, seq)
                != WhereIsSequencePreference::Rejected
            {
                return seq;
            }
        }
    }

    &sequences[0]
}

fn where_is_prefix_starts_with_mouse_event(prefix: &[Value]) -> bool {
    let Some(name) = prefix.first().and_then(|event| event.as_symbol_name()) else {
        return false;
    };
    let base = event_symbol_base_for_mouse_event_filter(name);
    matches!(
        base.as_str(),
        "menu-bar"
            | "tab-bar"
            | "tool-bar"
            | "tab-line"
            | "header-line"
            | "mode-line"
            | "mouse-1"
            | "mouse-2"
            | "mouse-3"
            | "mouse-4"
            | "mouse-5"
    )
}

fn event_symbol_base_for_mouse_event_filter(mut name: &str) -> String {
    while let Some((prefix, rest)) = name.split_once('-') {
        if matches!(
            prefix,
            "A" | "alt"
                | "C"
                | "control"
                | "H"
                | "hyper"
                | "M"
                | "meta"
                | "S"
                | "shift"
                | "s"
                | "super"
        ) {
            name = rest;
        } else {
            break;
        }
    }

    for mouse_prefix in [
        "down-mouse-",
        "drag-mouse-",
        "double-mouse-",
        "triple-mouse-",
    ] {
        if let Some(button) = name.strip_prefix(mouse_prefix) {
            return format!("mouse-{button}");
        }
    }

    name.to_string()
}

fn collect_where_is_accessible_maps(
    obarray: &Obarray,
    keymap: &Value,
    prefix: &mut Vec<Value>,
    out: &mut Vec<(Vec<Value>, Value)>,
    seen: &mut Vec<Value>,
    depth: usize,
) {
    if depth > 50 {
        return;
    }
    for seen_map in seen.iter() {
        if where_is_keymap_value_eq(seen_map, keymap) {
            return;
        }
    }

    seen.push(*keymap);
    out.push((prefix.clone(), *keymap));

    let mut bindings = Vec::new();
    list_keymap_for_each_binding(keymap, Some(obarray), |event, binding| {
        bindings.push((event, binding))
    });
    for (event, binding) in &bindings {
        let Some(prefix_keymap) = where_is_binding_prefix_keymap(obarray, binding) else {
            continue;
        };
        prefix.push(*event);
        collect_where_is_accessible_maps(obarray, &prefix_keymap, prefix, out, seen, depth + 1);
        prefix.pop();
    }

    // Composed keymaps `(keymap SUBMAP... . PARENT)` embed sibling keymaps that
    // share this prefix, and plain keymaps carry their parent as the spine tail.
    // GNU's `map_keymap` / `Faccessible_keymaps` descend into all of them, so a
    // binding reachable only through a composed submap (e.g. evil/general active
    // state maps, whose leader `SPC` lives in a `make-composed-keymap` submap)
    // is still found by the reverse where-is scan. Scan each at the SAME prefix.
    for sibling in super::keymap::list_keymap_sibling_keymaps(keymap) {
        collect_where_is_accessible_maps(obarray, &sibling, prefix, out, seen, depth + 1);
    }

    seen.pop();
}

/// Return raw reverse-lookup candidates, selecting GNU's complete lazy index
/// only for the mode in which GNU enables it (`nomenus && !noindirect`).
fn where_is_raw_sequences(
    eval: &mut Context,
    keymaps: &[Value],
    definition: Value,
    no_menu_bindings: bool,
    noindirect: bool,
) -> Vec<Vec<Value>> {
    if no_menu_bindings && !noindirect {
        ensure_where_is_reverse_index(eval, keymaps);
        return eval
            .interactive
            .cached_where_is_sequences(keymaps, definition)
            .expect("where-is reverse index was just ensured");
    }

    // GNU clears the shared reverse cache when asked to use a lookup mode for
    // which the index is not semantically complete.
    eval.interactive.clear_where_is_reverse_index();
    let mut sequences = Vec::new();
    for keymap in keymaps {
        collect_where_is_sequences_value(
            eval.obarray(),
            keymap,
            &definition,
            &mut sequences,
            no_menu_bindings,
            noindirect,
            0,
        );
    }
    sequences
}

fn ensure_where_is_reverse_index(eval: &mut Context, keymaps: &[Value]) {
    if eval
        .interactive
        .cached_where_is_reverse_index(keymaps)
        .is_some()
    {
        return;
    }

    let index = build_where_is_reverse_index(eval.obarray(), keymaps);
    eval.interactive.install_where_is_reverse_index(index);
}

fn where_is_indexed_command_remapping(
    eval: &mut Context,
    keymaps: &[Value],
    command: SymId,
) -> Option<Value> {
    ensure_where_is_reverse_index(eval, keymaps);
    #[cfg(test)]
    eval.interactive
        .note_where_is_reverse_index_remapping_lookup();
    eval.interactive
        .cached_where_is_reverse_index(keymaps)
        .expect("where-is reverse index was just ensured")
        .remapping_for(command)
}

/// Scan every accessible binding once and group its key sequences by command.
/// In contrast, calling `collect_where_is_sequences_value` for every M-x
/// candidate scans the same map graph thousands of times.
fn build_where_is_reverse_index(obarray: &Obarray, keymaps: &[Value]) -> WhereIsReverseIndex {
    let mut sequences_by_definition: HashMap<WhereIsDefinitionKey, Vec<Vec<Value>>> =
        HashMap::new();
    let mut remapping_by_command = HashMap::new();

    for keymap in keymaps {
        let mut maps = Vec::new();
        let mut prefix = Vec::new();
        let mut seen = Vec::new();
        collect_where_is_accessible_maps(obarray, keymap, &mut prefix, &mut maps, &mut seen, 0);

        for (map_prefix, map) in maps {
            if where_is_prefix_starts_with_mouse_event(&map_prefix) {
                continue;
            }

            list_keymap_for_each_binding(&map, Some(obarray), |event, binding| {
                let mut sequence = map_prefix.clone();
                sequence.push(event);
                if let Some(command) = where_is_remap_pseudo_key_command(&sequence)
                    .and_then(|command| command.as_symbol_id())
                {
                    remapping_by_command
                        .entry(command)
                        .or_insert_with(|| command_remapping_normalize_target(binding));
                }
                let Some(definition) = WhereIsDefinitionKey::from_value(get_keyelt(binding)) else {
                    return;
                };
                let sequences = sequences_by_definition.entry(definition).or_default();
                if !sequences.contains(&sequence) {
                    sequences.push(sequence);
                }
            });
        }
    }

    WhereIsReverseIndex {
        state: WhereIsKeymapState::new(keymaps),
        sequences_by_definition,
        remapping_by_command,
    }
}

fn collect_where_is_sequences_value(
    obarray: &Obarray,
    keymap: &Value,
    definition: &Value,
    out: &mut Vec<Vec<Value>>,
    no_menu_bindings: bool,
    noindirect: bool,
    depth: usize,
) -> bool {
    let mut maps = Vec::new();
    let mut prefix = Vec::new();
    let mut seen = Vec::new();
    collect_where_is_accessible_maps(obarray, keymap, &mut prefix, &mut maps, &mut seen, depth);

    for (map_prefix, map) in maps {
        if no_menu_bindings && where_is_prefix_starts_with_mouse_event(&map_prefix) {
            continue;
        }

        let mut bindings: Vec<(Value, Value)> = Vec::new();
        list_keymap_for_each_binding(&map, Some(obarray), |event, binding| {
            bindings.push((event, binding))
        });
        for (event, binding) in bindings {
            // GNU `where_is_internal_1`: reduce the stored binding through
            // `get_keyelt` (unless NOINDIRECT), so an old-style `(STRING . DEFN)`
            // menu label or a `(menu-item NAME DEFN ...)` wrapper matches its
            // underlying command. Forward `lookup-key` already does this; the
            // reverse scan must too, or leader hints like Doom's `SPC f r`
            // (bound as `("Recent files" . recentf-open-files)`) never resolve.
            let candidate = if noindirect {
                binding
            } else {
                get_keyelt(binding)
            };
            if !binding_matches_definition(&candidate, definition) {
                continue;
            }
            let mut sequence = map_prefix.clone();
            sequence.push(event);
            if !out.contains(&sequence) {
                out.push(sequence);
            }
        }
    }

    false
}

fn where_is_keymap_value_eq(a: &Value, b: &Value) -> bool {
    matches!((a.kind(), b.kind()), (ValueKind::Cons, ValueKind::Cons)) && a == b
}

fn where_is_binding_prefix_keymap(obarray: &Obarray, binding: &Value) -> Option<Value> {
    // Mirror GNU `accessible_keymaps_1`: reduce the stored binding through
    // `get_keyelt` (stripping a `(menu-item NAME DEFN ...)` wrapper or an
    // old-style `(STRING . DEFN)` menu label), THEN resolve the result to a
    // keymap. This is what lets a string-labelled symbol prefix such as Doom's
    // evil-state leader entry `(?\s "<leader>" . doom/leader)` — whose value is
    // `("<leader>" . doom/leader)` — descend into `doom/leader`'s keymap. A bare
    // keymap, a symbol whose function cell is a keymap, and both wrapper forms
    // all collapse to the same path here.
    let reduced = get_keyelt(*binding);
    if is_list_keymap(&reduced) {
        return Some(reduced);
    }
    let sym_name = reduced.as_symbol_name()?;
    let func = obarray.indirect_function(sym_name)?;
    if is_list_keymap(&func) {
        Some(func)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Thing-at-point extraction helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
