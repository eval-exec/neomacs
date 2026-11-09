//! `call-interactively` planning data and the cold acquisition table
//! (stage-4 W1/E4; peer ruling 13:1xZ).
//!
//! GNU `Fcall_interactively` (callint.c) saves the caller command identity,
//! plans the interactive specification, acquires the arguments and only then
//! restores the identity. The evaluator owns the plan and its data (the VM
//! path roots every intermediate value), while the acquisition bodies stay in
//! the core command layer: they bind the minibuffer and reader Lisp entries
//! (S2). The VM reaches that core body through the one cold PC2 `&'static fn`
//! table installed at `Context::new_inner` and `Context::from_dump`.

use super::interactive_registry::{validate_call_interactively_args, value_list_to_vec};
use super::*;
use neovm_value::intern::WellKnown;
use neovm_value::tagged::value::SymbolMode;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

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
    #[doc(hidden)]
    pub fn for_callable(callable: Value) -> Self {
        if callable
            .closure_body_value()
            .is_some_and(|body| body.is_cons())
        {
            Self::InterpretedClosure(callable.closure_env().flatten().unwrap_or(Value::NIL))
        } else {
            Self::Dynamic
        }
    }

    #[doc(hidden)]
    pub fn lexical_arg(self) -> Value {
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
    StringCode(neovm_value::heap_types::LispString),
    Form(InteractiveFormSpec),
}

/// Operations preceding interactive argument acquisition, in source order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractivePrefixFlag {
    RequireWritableBuffer,
    SelectEventWindow,
    HandleShiftSelection,
}

impl InteractivePrefixFlag {
    #[doc(hidden)]
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'*' => Some(Self::RequireWritableBuffer),
            b'@' => Some(Self::SelectEventWindow),
            b'^' => Some(Self::HandleShiftSelection),
            _ => None,
        }
    }

    pub fn requires_runtime(self) -> bool {
        match self {
            Self::RequireWritableBuffer => false,
            Self::SelectEventWindow | Self::HandleShiftSelection => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ParsedInteractiveStringCode {
    // The parser oracle reads these through the public `prefix_flags()` and
    // `entries()` accessors below; the fields stay private (rubric 7
    // re-point, LEAD 14:0xZ).
    prefix_flags: Vec<InteractivePrefixFlag>,
    entries: Vec<(char, neovm_value::heap_types::LispString)>,
}

#[derive(Clone, Debug, Default)]
pub struct InteractiveInvocationContext {
    command_keys: Vec<Value>,
    next_event_with_parameters_index: usize,
    has_command_keys_context: bool,
    pending_up_event: Option<Value>,
}

impl InteractiveInvocationContext {
    #[doc(hidden)]
    pub fn from_keys_arg_in_state(read_command_keys: &[Value], keys: Option<&Value>) -> Self {
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

impl ParsedInteractiveStringCode {
    /// Prefix flags parsed from the first spec line.
    pub fn prefix_flags(&self) -> &[InteractivePrefixFlag] {
        &self.prefix_flags
    }

    /// `(letter, prompt)` entries parsed from the spec string.
    pub fn entries(&self) -> &[(char, neovm_value::heap_types::LispString)] {
        &self.entries
    }
}

impl InteractiveInvocationContext {
    /// Whether `command_keys` was supplied by `call-interactively`'s KEYS arg.
    pub fn has_command_keys_context(&self) -> bool {
        self.has_command_keys_context
    }

    /// The explicit KEYS vector recorded for this invocation.
    pub fn command_keys(&self) -> &[Value] {
        &self.command_keys
    }

    /// The pending up-event captured before argument acquisition.
    pub fn pending_up_event(&self) -> Option<Value> {
        self.pending_up_event
    }

    /// Take the pending up-event for the `^`/`u` spec letters.
    pub fn take_pending_up_event(&mut self) -> Option<Value> {
        self.pending_up_event.take()
    }

    /// Set or clear the pending up-event.
    pub fn set_pending_up_event(&mut self, event: Option<Value>) {
        self.pending_up_event = event;
    }

    /// Advance over the next event-with-parameters entry of the KEYS vector.
    pub fn next_event_with_parameters(&mut self) -> Option<Value> {
        while self.next_event_with_parameters_index < self.command_keys.len() {
            let event = self.command_keys[self.next_event_with_parameters_index];
            self.next_event_with_parameters_index += 1;
            if event.is_cons() {
                return Some(event);
            }
        }
        None
    }
}
/// Parse a prepared interactive spec value.
///
/// The input may be an `(interactive SPEC)` form returned by
/// `interactive-form`, or the already-extracted SPEC stored by a closure.
fn parse_interactive_spec_from_value(
    spec: &Value,
    environment: InteractiveFormEnvironment,
    symbol_mode: SymbolMode,
) -> Option<ParsedInteractiveSpec> {
    if value_is_interactive_form(spec, symbol_mode) {
        let items = value_list_to_vec(spec)?;
        return match items.get(1) {
            Some(nested_spec) => {
                parse_interactive_spec_from_value(nested_spec, environment, symbol_mode)
            }
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
    symbol_mode: SymbolMode,
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

    parse_interactive_spec_from_value(&spec, environment, symbol_mode).ok_or_else(|| {
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

fn parse_interactive_prefix_flags(line: &[u8]) -> (Vec<InteractivePrefixFlag>, usize) {
    let mut flags = Vec::new();
    let mut offset = 0usize;
    while let Some(&byte) = line.get(offset) {
        if let Some(flag) = InteractivePrefixFlag::from_byte(byte) {
            flags.push(flag);
            offset += 1;
        } else {
            break;
        }
    }
    (flags, offset)
}

fn empty_lisp_string_like(
    template: &neovm_value::heap_types::LispString,
) -> neovm_value::heap_types::LispString {
    if template.is_multibyte() {
        neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new())
    } else {
        neovm_value::heap_types::LispString::from_unibyte(Vec::new())
    }
}

/// Whether VALUE is an `(interactive ...)` form.
///
/// GNU checks `CONSP (value) && EQ (XCAR (value), Qinteractive)`; the symbol
/// identity check is spelling-independent.
pub fn value_is_interactive_form(value: &Value, symbol_mode: SymbolMode) -> bool {
    match value.kind() {
        ValueKind::Cons => {
            value.cons_car().as_symbol_id(symbol_mode) == Some(INTERACTIVE_SYMBOL.id())
        }
        _ => false,
    }
}

static INTERACTIVE_SYMBOL: WellKnown = WellKnown::Interactive;

pub fn parse_interactive_code_entries(
    code: &neovm_value::heap_types::LispString,
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

pub fn eval_interactive_form_value(
    eval: &mut Context,
    form: Value,
    lexical_arg: Value,
) -> Result<Vec<Value>, Flow> {
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(form);
    eval.push_specpdl_root(lexical_arg);
    let result = (|| -> Result<Vec<Value>, Flow> {
        let value = eval.eval_value_with_lexical_arg(form, Some(lexical_arg))?;
        interactive_form_value_to_args(value)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

fn resolve_command_target_in_state(
    obarray: &Obarray,
    designator: &Value,
    symbol_mode: SymbolMode,
) -> Option<(Option<SymId>, Value)> {
    if let Some(symbol) = designator.as_symbol_id(symbol_mode) {
        if let Some((resolved_symbol, value)) =
            crate::emacs_core::builtins::symbols::resolve_indirect_symbol_by_id_in_obarray(
                obarray, symbol,
            )
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
        _ => Some((designator.as_symbol_id(symbol_mode), *designator)),
    }
}

/// Command identity that belongs to the caller of `call-interactively`.
///
/// GNU saves these four variables together in `Fcall_interactively` and
/// restores them together after interactive argument acquisition. Keeping
/// them in one Rust value prevents an interpreter or bytecode call path from
/// restoring only a subset and exposing a mixed command identity.
#[derive(Clone, Copy, Debug)]
pub struct CallInteractivelyCommandIdentity {
    this_command: Value,
    this_original_command: Value,
    real_this_command: Value,
    last_command: Value,
}

impl CallInteractivelyCommandIdentity {
    pub fn capture(eval: &Context) -> Self {
        // GNU's `Fcall_interactively` saves these four DEFVAR_LISP cells by
        // identity; the command-loop enum is their one native spelling set.
        use super::command_loop::CommandLoopSymbol;
        Self {
            this_command: eval
                .eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id())
                .unwrap_or(Value::NIL),
            this_original_command: eval
                .eval_symbol_by_id(CommandLoopSymbol::ThisOriginalCommand.id())
                .unwrap_or(Value::NIL),
            real_this_command: eval
                .eval_symbol_by_id(CommandLoopSymbol::RealThisCommand.id())
                .unwrap_or(Value::NIL),
            last_command: eval
                .eval_symbol_by_id(CommandLoopSymbol::LastCommand.id())
                .unwrap_or(Value::NIL),
        }
    }

    pub fn push_gc_roots(self, eval: &mut Context) {
        for value in self.values() {
            eval.push_specpdl_root(value);
        }
    }

    pub fn values(self) -> [Value; 4] {
        [
            self.this_command,
            self.this_original_command,
            self.real_this_command,
            self.last_command,
        ]
    }

    #[doc(hidden)]
    pub fn restore(self, eval: &mut Context) {
        use super::command_loop::CommandLoopSymbol;
        eval.assign_by_id(CommandLoopSymbol::ThisCommand.id(), self.this_command);
        eval.assign_by_id(
            CommandLoopSymbol::ThisOriginalCommand.id(),
            self.this_original_command,
        );
        eval.assign_by_id(
            CommandLoopSymbol::RealThisCommand.id(),
            self.real_this_command,
        );
        eval.assign_by_id(CommandLoopSymbol::LastCommand.id(), self.last_command);
    }
}

pub struct CallInteractivelyPlan {
    invocation_function: Value,
    func: Value,
    interactive_spec: ParsedInteractiveSpec,
    context: InteractiveInvocationContext,
    record_flag: bool,
    command_identity: CallInteractivelyCommandIdentity,
}

impl CallInteractivelyPlan {
    pub fn gc_roots(&self) -> Vec<Value> {
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
    pub fn restore_for_invocation(self, eval: &mut Context) -> RestoredCallInteractivelyInvocation {
        self.command_identity.restore(eval);
        RestoredCallInteractivelyInvocation {
            function: self.invocation_function,
        }
    }
}

/// A `call-interactively` target whose caller command identity is restored.
pub struct RestoredCallInteractivelyInvocation {
    function: Value,
}

impl RestoredCallInteractivelyInvocation {
    pub fn into_funcall_args(self, call_args: Vec<Value>) -> Vec<Value> {
        let mut funcall_args = Vec::with_capacity(call_args.len() + 1);
        funcall_args.push(self.function);
        funcall_args.extend(call_args);
        funcall_args
    }
}

pub fn plan_call_interactively_after_interactive_form_in_state(
    obarray: &Obarray,
    read_command_keys: &[Value],
    args: &[Value],
    interactive_form: Value,
    command_identity: CallInteractivelyCommandIdentity,
    symbol_mode: SymbolMode,
) -> Result<CallInteractivelyPlan, Flow> {
    validate_call_interactively_args(args)?;

    let func_val = args[0];
    if !interactive_form.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("commandp"), func_val],
        ));
    }
    let Some((_, func)) = resolve_command_target_in_state(obarray, &func_val, symbol_mode) else {
        return Err(signal(LispCondition::VoidFunction, vec![func_val]));
    };
    let interactive_spec = prepare_call_interactively_spec(
        func_val,
        interactive_form,
        InteractiveFormEnvironment::for_callable(func),
        symbol_mode,
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

/// The spec shape the core acquisition body must handle.
#[derive(Clone, Debug)]
pub enum CallInteractivelyArgsSpec {
    NoArgs,
    StringCode(neovm_value::heap_types::LispString),
    Form { value: Value, lexical_arg: Value },
}

impl CallInteractivelyPlan {
    /// The resolved target function.
    pub fn target_function(&self) -> Value {
        self.func
    }

    /// The designator the user passed to `call-interactively`.
    pub fn invocation_function(&self) -> Value {
        self.invocation_function
    }

    /// GNU's RECORD-FLAG argument.
    pub fn record_flag(&self) -> bool {
        self.record_flag
    }

    /// The spec shape argument acquisition must handle.
    pub fn args_spec(&self) -> CallInteractivelyArgsSpec {
        match &self.interactive_spec {
            ParsedInteractiveSpec::NoArgs => CallInteractivelyArgsSpec::NoArgs,
            ParsedInteractiveSpec::StringCode(code) => {
                CallInteractivelyArgsSpec::StringCode(code.clone())
            }
            ParsedInteractiveSpec::Form(spec) => CallInteractivelyArgsSpec::Form {
                value: spec.form,
                lexical_arg: spec.environment.lexical_arg(),
            },
        }
    }

    /// The string spec, for command-history encoding, when this is a string spec.
    pub fn spec_string_code(&self) -> Option<&neovm_value::heap_types::LispString> {
        match &self.interactive_spec {
            ParsedInteractiveSpec::StringCode(code) => Some(code),
            _ => None,
        }
    }

    /// Mutable access to the KEYS/up-event acquisition state.
    pub fn context_mut(&mut self) -> &mut InteractiveInvocationContext {
        &mut self.context
    }
}

/// The cold argument-acquisition upcall implemented by the core command layer.
pub type CallInteractivelyResolver =
    fn(&mut Context, &mut CallInteractivelyPlan) -> Result<(Value, Vec<Value>), Flow>;

/// PC2 table: one process-lifetime resolver descriptor installed by core.
/// Copying the descriptor copies one function pointer; GC never sees it.
#[derive(Clone, Copy)]
pub struct CallInteractivelyHooks {
    resolve: CallInteractivelyResolver,
}

impl CallInteractivelyHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(resolve: CallInteractivelyResolver) -> Self {
        Self { resolve }
    }

    /// The installed acquisition body.
    pub fn resolve(&self) -> CallInteractivelyResolver {
        self.resolve
    }
}

static CALL_INTERACTIVELY_HOOKS: OnceLock<CallInteractivelyHooks> = OnceLock::new();
static CALL_INTERACTIVELY_HOOKS_IDENTITY: OnceLock<&'static CallInteractivelyHooks> =
    OnceLock::new();

/// Publish the core command layer's argument-acquisition body.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a); core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting call-interactively publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_call_interactively_hooks(
    hooks: &'static CallInteractivelyHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(
        &CALL_INTERACTIVELY_HOOKS,
        &CALL_INTERACTIVELY_HOOKS_IDENTITY,
        hooks,
    )
}

/// The installed table, if core has run one of the two pinned install seams.
pub fn call_interactively_hooks() -> Option<&'static CallInteractivelyHooks> {
    CALL_INTERACTIVELY_HOOKS.get()
}

/// Acquire `call-interactively` arguments through the installed core body.
///
/// The missing-table refusal is a typed error, not a default: this path is
/// unreachable after construction (`Context::new_inner` and
/// `Context::from_dump` install the table before any form can evaluate), so
/// reaching it means the construction path itself is broken.
pub fn resolve_call_interactively_target_and_args(
    eval: &mut Context,
    plan: &mut CallInteractivelyPlan,
) -> Result<(Value, Vec<Value>), Flow> {
    let Some(hooks) = call_interactively_hooks() else {
        return Err(signal(
            "error",
            vec![Value::string(
                "call-interactively argument acquisition is not installed",
            )],
        ));
    };
    (hooks.resolve())(eval, plan)
}
