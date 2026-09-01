//! Error and signal types for the evaluator.

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use super::intern::{SymId, format_symbol_name_for_diagnostic, intern, resolve_sym};
use super::print::PrintOptions;
use super::string_escape::{format_lisp_string_bytes_emacs, format_lisp_string_emacs};
use super::value::{Value, ValueKind, VecLikeType, get_string_text_properties_for_value};
use crate::buffer::EmacsBytePos;
use crate::emacs_core::eval::ResumeTarget;
use crate::window::WindowId;
use strum::{EnumString, IntoStaticStr};

thread_local! {
    static FORMAT_OBJECT_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// Public-facing evaluation error.
///
/// Both Lisp-carrying variants hold an [`InFlightRoots`] pin, for the same
/// reason [`Flow`]'s payloads do: a boundary that holds an `EvalError` while
/// more Lisp runs is holding Lisp values the precise collector cannot see. An
/// enum variant cannot have a private FIELD (a variant's fields are as visible
/// as the enum), so the pin is a field of a type that has no constructor
/// outside this module — which makes the struct literal unwritable elsewhere
/// and [`EvalError::signal`] / [`EvalError::uncaught_throw`] the only ways in.
/// Existing `EvalError::Signal { symbol, data, .. }` patterns keep working
/// unchanged; only construction sites move (DIVERGENCES.md 162).
#[derive(Clone, Debug)]
pub enum EvalError {
    Signal {
        symbol: SymId,
        data: Vec<Value>,
        raw_data: Option<Value>,
        /// Not constructible outside `error.rs`; see the type docs.
        pin: InFlightRoots,
    },
    UncaughtThrow {
        tag: Value,
        value: Value,
        /// Not constructible outside `error.rs`; see the type docs.
        pin: InFlightRoots,
    },
    /// `kill-emacs` reached this boundary: not an error, an exit request.
    Shutdown(super::eval::ShutdownRequest),
}

impl EvalError {
    /// The only way to build a signal error: pins the symbol and payload as GC
    /// roots for as long as the error (or any clone of it) lives.
    pub fn signal(symbol: SymId, data: Vec<Value>, raw_data: Option<Value>) -> Self {
        let pin = InFlightRoots::pin(
            std::iter::once(Value::from_sym_id(symbol))
                .chain(data.iter().copied())
                .chain(raw_data),
        );
        Self::Signal {
            symbol,
            data,
            raw_data,
            pin,
        }
    }

    /// The only way to build an uncaught-throw error; pins tag and value.
    pub fn uncaught_throw(tag: Value, value: Value) -> Self {
        Self::UncaughtThrow {
            tag,
            value,
            pin: InFlightRoots::pin([tag, value]),
        }
    }
}

impl Display for EvalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signal {
                symbol,
                data,
                raw_data,
                ..
            } => write!(
                f,
                "signal {} {}",
                format_symbol_name_for_diagnostic(*symbol),
                format_signal_payload(raw_data.as_ref(), data),
            ),
            Self::UncaughtThrow { tag, value, .. } => write!(
                f,
                "uncaught throw tag={} value={}",
                super::print::print_value(tag),
                super::print::print_value(value),
            ),
            Self::Shutdown(request) => write!(f, "kill-emacs {}", request.exit_code),
        }
    }
}

impl Error for EvalError {}

/// Re-enter internal control flow from a public error.
///
/// The inverse of [`map_flow`], for the boundaries that call `load_file` (and
/// friends) from inside the evaluator. It lives here as the single conversion
/// so a new [`Flow`] variant cannot be dropped by one of several hand-written
/// copies — which is how `kill-emacs` used to get lost. Deliberately a
/// function and not a `From` impl: a second `From<_> for Flow` makes the error
/// type ambiguous at every `?` in the crate.
pub(crate) fn flow_from_eval_error(err: EvalError) -> Flow {
    match err {
        EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        } => Flow::Signal(Box::new(SignalData::new(symbol, data, raw_data, false))),
        EvalError::UncaughtThrow { tag, value, .. } => Flow::throw(tag, value),
        EvalError::Shutdown(request) => Flow::Shutdown(request),
    }
}

/// Internal non-local control flow.
#[derive(Clone, Debug)]
pub enum Flow {
    Signal(Box<SignalData>),
    /// `throw` to a `catch` tag. The payload is an owned struct for the same
    /// reason `Signal`'s is: it carries a private [`InFlightRoots`] pin, so a
    /// throw whose tag or value is not a GC root is not representable
    /// (DIVERGENCES.md 162).
    Throw(Box<ThrowData>),
    /// The cooperative thread-yield handoff. Same shape, same reason.
    ThreadBlocked(Box<ThreadBlockedData>),
    /// `kill-emacs`: unwind everything and exit with this request.
    ///
    /// GNU's `Fkill_emacs` is `noreturn` — it runs the hooks and calls
    /// `exit()`, so no Lisp handler and no callback boundary ever sees it.
    /// `condition-case` cannot catch this (it is not a signal), and every
    /// boundary that matches `Flow` exhaustively must decide about it at
    /// compile time instead of silently absorbing it as a callback error.
    ///
    /// The exit itself is DEFERRED, not immediate, and that is the one place
    /// this diverges from GNU. `builtin_kill_emacs` records the request on the
    /// evaluator before returning this variant, and the exit happens when
    /// control reaches the evaluator's own return — so where GNU exits from
    /// inside an FFI call, this engine finishes unwinding the boundary first.
    /// A boundary with no shutdown exit kind of its own therefore reports
    /// something else meanwhile: `module_handle_nonlocal_exit`
    /// (`dynamic_module.rs`) hands the module a signal named `kill-emacs`, and
    /// a module that clears it still exits, because the recorded request — not
    /// the propagating variant — is what the evaluator acts on.
    Shutdown(super::eval::ShutdownRequest),
}

impl Flow {
    /// The only way to build a `throw`: pins `tag` and `value` as GC roots for
    /// as long as the flow (or any clone of it) lives.
    pub(crate) fn throw(tag: Value, value: Value) -> Self {
        Self::Throw(Box::new(ThrowData::new(tag, value)))
    }

    /// The only way to build a thread-yield handoff; pins both payloads.
    pub(crate) fn thread_blocked(blocker: Value, remaining_forms: Value) -> Self {
        Self::ThreadBlocked(Box::new(ThreadBlockedData::new(blocker, remaining_forms)))
    }

    /// Exhaustive proof that every variant's Lisp payload is pinned.
    ///
    /// This function exists to FAIL TO COMPILE. A new `Flow` variant must add
    /// an arm here, and the arm has to name a payload type implementing the
    /// sealed [`InFlightPinned`] trait — which only a struct with a private
    /// [`InFlightRoots`] field in this module can implement. A variant that
    /// carries a bare `Value` therefore cannot be added without either pinning
    /// it or deliberately declaring it root-free, and the `Shutdown` arm is
    /// exactly that declaration: `ShutdownRequest` is `{ exit_code: i32,
    /// restart: bool }` and holds no Lisp value at all.
    ///
    /// (`#[allow(dead_code)]`: the return value is not consumed anywhere —
    /// the compile-time exhaustiveness IS the product.)
    #[allow(dead_code)]
    pub(crate) fn pinned_payload(&self) -> Option<&dyn InFlightPinned> {
        match self {
            Self::Signal(data) => Some(&**data),
            Self::Throw(data) => Some(&**data),
            Self::ThreadBlocked(data) => Some(&**data),
            Self::Shutdown(_) => None,
        }
    }
}

/// A `Flow` payload whose Lisp values are pinned as GC roots by construction.
///
/// Sealed in practice: implementing it means producing an [`InFlightRoots`],
/// and `InFlightRoots::pin` is private to this module — so the only
/// implementors are the payload structs below, and a new one has to be written
/// here, next to the pin. (The type itself is `pub` only because
/// [`EvalError`]'s variants name it in a public enum's interface; it has all-
/// private fields and no public constructor.)
pub(crate) trait InFlightPinned {
    fn in_flight_roots(&self) -> &InFlightRoots;
}

/// `(throw TAG VALUE)` in flight.
#[derive(Clone, Debug)]
pub struct ThrowData {
    pub tag: Value,
    pub value: Value,
    /// See [`SignalData::pin`]. PRIVATE, so `Flow::throw` is the only way in.
    pin: InFlightRoots,
}

impl ThrowData {
    fn new(tag: Value, value: Value) -> Self {
        let pin = InFlightRoots::pin([tag, value]);
        Self { tag, value, pin }
    }
}

impl InFlightPinned for ThrowData {
    fn in_flight_roots(&self) -> &InFlightRoots {
        &self.pin
    }
}

/// A cooperative thread yield in flight: the object being waited on, plus the
/// forms the scheduler must re-dispatch when the thread is resumed.
#[derive(Clone, Debug)]
pub struct ThreadBlockedData {
    pub blocker: Value,
    pub remaining_forms: Value,
    /// See [`SignalData::pin`]. PRIVATE, so `Flow::thread_blocked` is the only
    /// way in.
    pin: InFlightRoots,
}

impl ThreadBlockedData {
    fn new(blocker: Value, remaining_forms: Value) -> Self {
        let pin = InFlightRoots::pin([blocker, remaining_forms]);
        Self {
            blocker,
            remaining_forms,
            pin,
        }
    }
}

impl InFlightPinned for ThreadBlockedData {
    fn in_flight_roots(&self) -> &InFlightRoots {
        &self.pin
    }
}

#[derive(Clone, Debug)]
pub struct SignalData {
    pub symbol: SymId,
    pub data: Vec<Value>,
    /// Original cdr payload when a signal uses non-list data.
    pub raw_data: Option<Value>,
    pub(crate) suppress_signal_hook: bool,
    pub(crate) selected_resume: Option<ResumeTarget>,
    pub(crate) search_complete: bool,
    /// Keeps `data` and `raw_data` reachable for the collector while this
    /// signal is in flight. PRIVATE on purpose: it is what makes an unrooted
    /// signal payload unrepresentable outside this module — a struct with a
    /// private field cannot be built from a literal elsewhere, so every
    /// construction site has to go through [`SignalData::new`], which pins.
    /// See [`InFlightRoots`] for why the pin is needed at all.
    pin: InFlightRoots,
}

impl SignalData {
    /// The only way to build a signal payload: pins `data` and `raw_data` as
    /// GC roots for as long as the returned value (or any clone of it) lives.
    pub(crate) fn new(
        symbol: SymId,
        data: Vec<Value>,
        raw_data: Option<Value>,
        suppress_signal_hook: bool,
    ) -> Self {
        let pin = InFlightRoots::pin(
            std::iter::once(Value::from_sym_id(symbol))
                .chain(data.iter().copied())
                .chain(raw_data),
        );
        Self {
            symbol,
            data,
            raw_data,
            suppress_signal_hook,
            selected_resume: None,
            search_complete: false,
            pin,
        }
    }

    /// Resolve the signal symbol name via the interner.
    pub fn symbol_name(&self) -> &str {
        resolve_sym(self.symbol)
    }
}

impl InFlightPinned for SignalData {
    fn in_flight_roots(&self) -> &InFlightRoots {
        &self.pin
    }
}

// ---------------------------------------------------------------------------
// In-flight signal payload rooting
// ---------------------------------------------------------------------------
//
// GNU never needs this: `signal_or_quit` builds the `(SYMBOL . DATA)` pair and
// longjmps with it on the C stack, and `mark_stack` scans that stack
// conservatively, so the payload is a root for free the whole way out
// (src/eval.c `signal_or_quit`, src/alloc.c `mark_stack`).
//
// This collector is PRECISE — there is no conservative stack scan
// (`crates/neovm-core/src/tagged/CONCURRENT_GC.md`, "precise-rooting precondition";
// `set_stack_bottom` is a no-op) — and a signal does not longjmp here, it
// travels up the Rust stack as `Flow::Signal(Box<SignalData>)`. That journey
// is not quiet: every frame it passes runs `unbind_to`, which executes
// `unwind-protect` cleanups, buffer and binding restores, and (through
// `signal-hook-function` / `debug-on-error`) arbitrary Lisp. All of that
// allocates, and any allocation-bearing safe point may collect.
//
// Without a root the payload is unreachable at exactly that moment, so the
// collector reclaims it, and `condition-case` then binds a DANGLING cons to
// its variable. The damage surfaces arbitrarily far away and unrecognizably:
// a reclaimed cons's cdr holds `ConsCell::set_free_next`'s raw `*mut ConsCell`
// free-list link, whose low three bits are `TAG_SYMBOL`, so it decodes as a
// symbol with a garbage id (DIVERGENCES.md 161).
//
// The table is a thread-local slot arena rather than the `SCRATCH_GC_ROOTS`
// stack because a signal's lifetime is NOT stack-shaped: it is cloned, boxed,
// stored in a resume target, and converted to and from `EvalError`, so roots
// are released in an order a truncating stack cannot express.

thread_local! {
    static IN_FLIGHT_ROOTS: RefCell<InFlightRootTable> =
        RefCell::new(InFlightRootTable::default());
}

#[derive(Default)]
struct InFlightRootTable {
    /// One entry per live pin; `None` marks a reusable slot.
    slots: Vec<Option<Vec<Value>>>,
    free: Vec<usize>,
}

/// A pin on one in-flight payload's heap values. Owns a slot in the
/// thread-local table
/// for its whole life; `Clone` takes a fresh slot (a cloned `Flow` is a second
/// independent owner), `Drop` releases it.
pub struct InFlightRoots {
    /// `None` when the payload contained no heap object — the common case for
    /// `quit` and for arity/type errors whose data is symbols and fixnums —
    /// so the overwhelmingly frequent signal costs no table traffic at all.
    slot: Option<usize>,
    /// The slot index names a row in THIS thread's table, so the handle must
    /// not travel: dropped on another thread it would release a slot that
    /// thread pinned, unrooting a live payload. `PhantomData<*const ()>` is
    /// how that is enforced — it makes the handle (and with it `SignalData`
    /// and `Flow`) `!Send`, so a cross-thread move is a compile error rather
    /// than a rare unrooting. That costs nothing real: a `Value` belongs to one
    /// thread's heap already.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl InFlightRoots {
    /// Pin every traceable value in `payload` for the handle's lifetime.
    ///
    /// A signal passes its error SYMBOL through here too, not just its data:
    /// `signal` keeps the symbol's IDENTITY (an *uninterned* symbol given
    /// conditions by `define-error` is honoured — see `signal_internal_id`),
    /// and an uninterned symbol's value/function/plist cells survive only
    /// while something marks it. In flight, nothing else does.
    fn pin(payload: impl IntoIterator<Item = Value>) -> Self {
        let mut values: Vec<Value> = Vec::new();
        for value in payload {
            Self::push_if_traceable(&mut values, value);
        }
        Self {
            slot: Self::claim(values),
            _not_send: std::marker::PhantomData,
        }
    }

    /// Keep what the mark phase can act on. Heap objects obviously; symbols
    /// because `seed_root` routes them to `mark_symbol`, which is what keeps a
    /// non-canonical symbol's cells. Fixnums, `nil` and `t` are immediates the
    /// collector never touches, and dropping them is what leaves the common
    /// signal — `quit`, and arity errors whose data is a symbol and a count —
    /// with a short vector or none at all.
    #[inline]
    fn push_if_traceable(values: &mut Vec<Value>, value: Value) {
        if value.is_nil() || value.is_t() {
            return;
        }
        if value.is_heap_object() || value.is_symbol() {
            values.push(value);
        }
    }

    fn claim(values: Vec<Value>) -> Option<usize> {
        if values.is_empty() {
            return None;
        }
        IN_FLIGHT_ROOTS.with(|table| {
            let mut table = table.borrow_mut();
            match table.free.pop() {
                Some(slot) => {
                    table.slots[slot] = Some(values);
                    Some(slot)
                }
                None => {
                    table.slots.push(Some(values));
                    Some(table.slots.len() - 1)
                }
            }
        })
    }
}

impl Clone for InFlightRoots {
    fn clone(&self) -> Self {
        let Some(slot) = self.slot else {
            return Self {
                slot: None,
                _not_send: std::marker::PhantomData,
            };
        };
        let values = IN_FLIGHT_ROOTS
            .with(|table| table.borrow().slots[slot].clone())
            .unwrap_or_default();
        Self {
            slot: Self::claim(values),
            _not_send: std::marker::PhantomData,
        }
    }
}

impl Drop for InFlightRoots {
    fn drop(&mut self) {
        let Some(slot) = self.slot else { return };
        // A thread-local can already be destroyed during thread teardown; a
        // failed access there means the table itself is gone, so there is
        // nothing left to release.
        let _ = IN_FLIGHT_ROOTS.try_with(|table| {
            let mut table = table.borrow_mut();
            table.slots[slot] = None;
            table.free.push(slot);
        });
    }
}

impl std::fmt::Debug for InFlightRoots {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("InFlightRoots")
    }
}

/// Seed every in-flight `Flow` payload — signal, throw and thread-yield —
/// into the collector's root set. Wired into `collect_thread_local_gc_roots`
/// (`eval.rs`) beside the other thread-local root groups.
pub(crate) fn collect_in_flight_flow_gc_roots(out: &mut Vec<Value>) {
    IN_FLIGHT_ROOTS.with(|table| {
        for slot in table.borrow().slots.iter().flatten() {
            out.extend(slot.iter().copied());
        }
    });
}

pub(crate) type EvalResult = Result<Value, Flow>;

/// Standard error conditions defined by the GNU C core (`define_error`
/// in data.c, eval.c, and friends). Signaling through the enum instead
/// of a string literal makes the condition name typo-proof at compile
/// time; user-defined conditions (`define-error` from Lisp) still
/// signal by name through the `&str` impl of [`IntoConditionSym`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
pub(crate) enum LispCondition {
    #[strum(serialize = "args-out-of-range")]
    ArgsOutOfRange,
    #[strum(serialize = "arith-error")]
    ArithError,
    #[strum(serialize = "beginning-of-buffer")]
    BeginningOfBuffer,
    #[strum(serialize = "buffer-read-only")]
    BufferReadOnly,
    #[strum(serialize = "circular-list")]
    CircularList,
    #[strum(serialize = "coding-system-error")]
    CodingSystemError,
    #[strum(serialize = "cyclic-function-indirection")]
    CyclicFunctionIndirection,
    #[strum(serialize = "cyclic-variable-indirection")]
    CyclicVariableIndirection,
    #[strum(serialize = "end-of-buffer")]
    EndOfBuffer,
    #[strum(serialize = "end-of-file")]
    EndOfFile,
    #[strum(serialize = "error")]
    // The generic condition. Existing call sites still signal("error", ...)
    // by name (the string arm is also the user-defined-condition escape
    // hatch); the variant exists so new code can stay typo-proof throughout.
    #[allow(dead_code)]
    Error,
    #[strum(serialize = "file-already-exists")]
    FileAlreadyExists,
    #[strum(serialize = "file-error")]
    FileError,
    #[strum(serialize = "file-missing")]
    FileMissing,
    #[strum(serialize = "invalid-function")]
    InvalidFunction,
    #[strum(serialize = "invalid-read-syntax")]
    InvalidReadSyntax,
    #[strum(serialize = "invalid-regexp")]
    InvalidRegexp,
    #[strum(serialize = "malformed-keyword-arg-list")]
    MalformedKeywordArgList,
    #[strum(serialize = "no-catch")]
    NoCatch,
    #[strum(serialize = "overflow-error")]
    OverflowError,
    #[strum(serialize = "quit")]
    Quit,
    #[strum(serialize = "scan-error")]
    ScanError,
    #[strum(serialize = "search-failed")]
    SearchFailed,
    #[strum(serialize = "setting-constant")]
    SettingConstant,
    #[strum(serialize = "sqlite-error")]
    SqliteError,
    #[strum(serialize = "sqlite-locked-error")]
    SqliteLockedError,
    #[strum(serialize = "text-read-only")]
    TextReadOnly,
    #[strum(serialize = "treesit-node-buffer-killed")]
    TreesitNodeBufferKilled,
    #[strum(serialize = "treesit-node-outdated")]
    TreesitNodeOutdated,
    #[strum(serialize = "treesit-parse-error")]
    TreesitParseError,
    #[strum(serialize = "treesit-parser-deleted")]
    TreesitParserDeleted,
    #[strum(serialize = "treesit-predicate-not-found")]
    TreesitPredicateNotFound,
    #[strum(serialize = "treesit-query-error")]
    TreesitQueryError,
    #[strum(serialize = "type-mismatch")]
    TypeMismatch,
    #[strum(serialize = "user-error")]
    UserError,
    #[strum(serialize = "void-function")]
    VoidFunction,
    #[strum(serialize = "void-variable")]
    VoidVariable,
    #[strum(serialize = "wrong-length-argument")]
    WrongLengthArgument,
    #[strum(serialize = "wrong-number-of-arguments")]
    WrongNumberOfArguments,
    #[strum(serialize = "wrong-type-argument")]
    WrongTypeArgument,
}

impl LispCondition {
    pub(crate) fn name(self) -> &'static str {
        self.into()
    }
}

/// A condition designator accepted by the signal constructors: a typed
/// standard condition, or any symbol name for user-defined conditions.
pub(crate) trait IntoConditionSym {
    fn condition_sym(self) -> SymId;
}

impl IntoConditionSym for LispCondition {
    fn condition_sym(self) -> SymId {
        intern(self.name())
    }
}

impl IntoConditionSym for &str {
    fn condition_sym(self) -> SymId {
        intern(self)
    }
}

/// Create a signal flow.
pub(crate) fn signal(symbol: impl IntoConditionSym, data: Vec<Value>) -> Flow {
    signal_internal_id(symbol.condition_sym(), data, None, false)
}

/// Create a signal flow without running `signal-hook-function`.
pub(crate) fn signal_suppressed(symbol: impl IntoConditionSym, data: Vec<Value>) -> Flow {
    signal_internal_id(symbol.condition_sym(), data, None, true)
}

/// Like `signal_internal` but takes the error symbol by identity.  GNU's
/// `Fsignal`/`signal_or_quit` operate on the actual symbol object, so an
/// *uninterned* error symbol (created by `make-symbol` and given conditions by
/// `define-error') keeps its identity all the way to condition matching.
/// Re-interning by name would resolve to a different symbol with no
/// `error-conditions' and wrongly canonicalize to "Invalid error symbol".
pub(crate) fn signal_internal_id(
    symbol: SymId,
    data: Vec<Value>,
    raw_data: Option<Value>,
    suppress_signal_hook: bool,
) -> Flow {
    Flow::Signal(Box::new(SignalData::new(
        symbol,
        data,
        raw_data,
        suppress_signal_hook,
    )))
}

/// Create a signal where DATA is used as the raw cdr payload.
///
/// This preserves dotted signal data shapes such as `(foo . 1)`.
pub(crate) fn signal_with_data(symbol: impl IntoConditionSym, data: Value) -> Flow {
    signal_with_data_internal(symbol, data, false)
}

fn signal_with_data_internal(
    symbol: impl IntoConditionSym,
    data: Value,
    suppress_signal_hook: bool,
) -> Flow {
    let normalized = super::value::list_to_vec(&data).unwrap_or_else(|| vec![data]);
    signal_internal_id(
        symbol.condition_sym(),
        normalized,
        Some(data),
        suppress_signal_hook,
    )
}

/// Identity-preserving signal flow with a raw cdr payload.
pub(crate) fn signal_with_data_id(symbol: SymId, data: Value) -> Flow {
    let normalized = super::value::list_to_vec(&data).unwrap_or_else(|| vec![data]);
    signal_internal_id(symbol, normalized, Some(data), false)
}

/// Convert internal flow to public EvalError.
pub fn map_flow(flow: Flow) -> EvalError {
    match flow {
        Flow::Signal(sig) => {
            // `sig` (and with it the SignalData pin) stays alive until the new
            // pin is taken, so the payload is never momentarily unrooted.
            EvalError::signal(sig.symbol, sig.data.clone(), sig.raw_data)
        }
        Flow::Throw(thrown) => EvalError::uncaught_throw(thrown.tag, thrown.value),
        Flow::Shutdown(request) => EvalError::Shutdown(request),
        Flow::ThreadBlocked(blocked) => EvalError::signal(
            intern("error"),
            vec![Value::string(format!(
                "Thread blocked on {}",
                super::print::print_value(&blocked.blocker)
            ))],
            None,
        ),
    }
}

/// Build the binding value for condition-case variable: (symbol . data)
pub(crate) fn make_signal_binding_value(sig: &SignalData) -> Value {
    if let Some(raw) = &sig.raw_data {
        return Value::cons(Value::symbol(sig.symbol), *raw);
    }
    let mut values = Vec::with_capacity(sig.data.len() + 1);
    values.push(Value::symbol(sig.symbol));
    values.extend(sig.data.clone());
    Value::list(values)
}

/// Reconstruct a signal flow from a condition-case/thread error binding form.
pub(crate) fn signal_from_binding_value(value: Value) -> Option<Flow> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    let tail = pair_cdr;
    let symbol_id = pair_car.as_symbol_id()?;
    Some(signal_with_data_id(symbol_id, tail))
}

/// Format an eval result for the compat test harness (TSV output).
pub fn format_eval_result(result: &Result<Value, EvalError>) -> String {
    match result {
        Ok(value) => format!("OK {}", super::print::print_value(value)),
        Err(EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        }) => {
            let payload = format_signal_payload(raw_data.as_ref(), data);
            format!(
                "ERR ({} {})",
                format_symbol_name_for_diagnostic(*symbol),
                payload
            )
        }
        Err(EvalError::UncaughtThrow { tag, value, .. }) => {
            format!(
                "ERR (no-catch ({} {}))",
                super::print::print_value(tag),
                super::print::print_value(value),
            )
        }
        Err(EvalError::Shutdown(request)) => format!("ERR (kill-emacs {})", request.exit_code),
    }
}

fn format_signal_payload(raw_data: Option<&Value>, data: &[Value]) -> String {
    if let Some(raw) = raw_data {
        return super::print::print_value(raw);
    }
    if data.is_empty() {
        "nil".to_string()
    } else {
        super::print::print_value(&Value::list(data.to_vec()))
    }
}

fn format_opaque_handle_in_state(
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
) -> Option<String> {
    if let Some(handle) = super::terminal::pure::print_terminal_handle(value) {
        return Some(handle);
    }
    if super::marker::is_marker(value)
        && let Some(marker) = value.as_marker_data()
    {
        let mut out = String::from("#<marker ");
        if marker.insertion_type {
            out.push_str("(moves after insertion) ");
        }
        // T7: read authoritative charpos (1-based Lisp shape) directly
        // from LispMarker, not the deleted stale `position` cache.
        if let Some(buffer_id) = marker.buffer
            && let Some(buffer) = buffers.get(buffer_id)
        {
            out.push_str(&format!(
                "at {} in {}",
                marker.charpos + 1,
                buffer.name_runtime_string_owned()
            ));
        } else {
            out.push_str("in no buffer");
        }
        out.push('>');
        return Some(out);
    }
    if let Some(overlay) = value.as_overlay_data() {
        if let Some(buffer_id) = overlay.buffer
            && let Some(buffer) = buffers.get(buffer_id)
        {
            let (start, end) = overlay.current_range();
            return Some(format!(
                "#<overlay from {} to {} in {}>",
                buffer
                    .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(start))
                    .as_i64(),
                buffer
                    .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(end))
                    .as_i64(),
                buffer.name_runtime_string_owned()
            ));
        }
        return Some("#<overlay in no buffer>".to_string());
    }
    if let Some(id) = value.as_window_id() {
        return Some(format_window_handle_in_state(buffers, frames, id));
    }
    if let Some(id) = threads.thread_id_from_handle(value) {
        return Some(format!("#<thread {id}>"));
    }
    if let Some(id) = threads.mutex_id_from_handle(value) {
        return Some(format!("#<mutex {id}>"));
    }
    if let Some(id) = threads.condition_variable_id_from_handle(value) {
        return Some(format!("#<condvar {id}>"));
    }
    if let Some(buf_id) = value.as_buffer_id() {
        if let Some(buf) = buffers.get(buf_id) {
            return Some(format!("#<buffer {}>", buf.name_runtime_string_owned()));
        }
        if buffers.dead_buffer_last_name_value(buf_id).is_some() {
            return Some("#<killed buffer>".to_string());
        }
    }
    None
}

fn format_window_handle_in_state(
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    id: u64,
) -> String {
    let window_id = WindowId(id);
    if let Some(frame_id) = frames.find_window_frame_id(window_id)
        && let Some(frame) = frames.get(frame_id)
        && let Some(window) = frame.find_window(window_id)
    {
        if let Some(buffer_id) = window.buffer_id()
            && let Some(buffer) = buffers.get(buffer_id)
        {
            return format!("#<window {id} on {}>", buffer.name_runtime_string_owned());
        }
        return format!("#<window {id} on {}>", frame.name_runtime_string_owned());
    }
    format!("#<window {id}>")
}

/// Build print options the way GNU's printer reads them.
///
/// Every `print-*` name here is a `DEFVAR_LISP` / `DEFVAR_BOOL` that GNU
/// dereferences as a bare C global -- `Vprint_level` at `src/print.c:2531`,
/// `Vprint_length` at `:2256`, and so on -- and the swap-in
/// (`src/data.c:1573-1603`) keeps every one of those globals equal to the
/// *current buffer's* binding. So the reads name the buffer (ledger 196);
/// `lisp/eshell/esh-mode.el` localises `print-level` and `print-length`.
///
/// This is narrower than it looks: when the print STREAM is a buffer, GNU's
/// `PRINTPREPARE` does `set_buffer_internal` on the stream's buffer first,
/// which swaps the binding back OUT -- which is why `prin1-to-string` and
/// `error-message-string` (`src/print.c:1058`, printing into
/// `Vprin1_to_string_buffer`) do not honour a buffer-local `print-level` in
/// either editor. Only a non-buffer stream keeps it swapped in.
///
/// With specbind, dynamic let-bindings are written directly to the obarray,
/// so this correctly handles (let ((print-escape-newlines t)) (format "%S" ...)).
pub(crate) fn print_options_from_state(
    obarray: &super::symbol::Obarray,
    buf: Option<&crate::buffer::Buffer>,
) -> PrintOptions {
    let print_gensym = obarray
        .value_in_buffer(buf, "print-gensym")
        .is_some_and(|v| v.is_truthy());
    let print_circle = obarray
        .value_in_buffer(buf, "print-circle")
        .is_some_and(|v| v.is_truthy());
    let print_quoted = obarray
        .value_in_buffer(buf, "print-quoted")
        .is_none_or(|v| v.is_truthy());
    let print_symbols_bare = obarray
        .value_in_buffer(buf, "print-symbols-bare")
        .is_some_and(|v| v.is_truthy());
    let print_escape_newlines = obarray
        .value_in_buffer(buf, "print-escape-newlines")
        .is_some_and(|v| v.is_truthy());
    let print_level = obarray
        .value_in_buffer(buf, "print-level")
        .and_then(|v| v.as_fixnum())
        .filter(|&n| n >= 0);
    let print_length = obarray
        .value_in_buffer(buf, "print-length")
        .and_then(|v| v.as_fixnum())
        .filter(|&n| n >= 0);
    let print_escape_nonascii = obarray
        .value_in_buffer(buf, "print-escape-nonascii")
        .is_some_and(|v| v.is_truthy());
    let print_escape_multibyte = obarray
        .value_in_buffer(buf, "print-escape-multibyte")
        .is_some_and(|v| v.is_truthy());
    let print_escape_control_characters = obarray
        .value_in_buffer(buf, "print-escape-control-characters")
        .is_some_and(|v| v.is_truthy());
    let print_integers_as_characters = obarray
        .value_in_buffer(buf, "print-integers-as-characters")
        .is_some_and(|v| v.is_truthy());
    let print_continuous_numbering = obarray
        .value_in_buffer(buf, "print-continuous-numbering")
        .is_some_and(|v| v.is_truthy());
    let print_number_table = if print_continuous_numbering {
        obarray
            .value_in_buffer(buf, "print-number-table")
            .filter(|v| v.is_hash_table())
    } else {
        None
    };
    let mut opts = PrintOptions::new(print_gensym, print_circle, print_level, print_length);
    opts.print_quoted = print_quoted;
    opts.print_symbols_bare = print_symbols_bare;
    opts.print_escape_newlines = print_escape_newlines;
    opts.print_escape_nonascii = print_escape_nonascii;
    opts.print_escape_multibyte = print_escape_multibyte;
    opts.print_escape_control_characters = print_escape_control_characters;
    opts.print_integers_as_characters = print_integers_as_characters;
    opts.print_continuous_numbering = print_continuous_numbering;
    opts.print_number_table = print_number_table;
    opts.float_output_format = obarray
        .value_in_buffer(buf, "float-output-format")
        .filter(|v| v.is_string());
    opts
}

pub(crate) fn print_value_in_state(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
) -> String {
    // GNU renders a value to a Rust-side String the way `Ferror_message_string`
    // does: through `Vprin1_to_string_buffer` (`src/print.c:1058`).  That is a
    // buffer stream, so `PRINTPREPARE` makes it current and the caller's
    // buffer-local `print-level` / `print-length` are swapped out. Ledger 196.
    print_value_in_state_with_options(ctx, value, print_options_from_state(&ctx.obarray, None))
}

pub(crate) fn print_value_in_state_with_options(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
    options: PrintOptions,
) -> String {
    format_value_in_state(
        &ctx.obarray,
        &ctx.buffers,
        &ctx.frames,
        &ctx.threads,
        value,
        options,
    )
}

fn format_cycle_stack_index(value: &Value) -> Option<usize> {
    let key = super::print::default_cycle_candidate_key(value)?;
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow().iter().position(|entry| *entry == key))
}

fn push_format_cycle_object(value: &Value) -> bool {
    let Some(key) = super::print::default_cycle_candidate_key(value) else {
        return false;
    };
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow_mut().push(key));
    true
}

fn pop_format_cycle_object(pushed: bool) {
    if pushed {
        FORMAT_OBJECT_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn format_object_stack_len() -> usize {
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow().len())
}

fn truncate_format_object_stack(len: usize) {
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow_mut().truncate(len));
}

fn format_value_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> String {
    let _print_guard = super::print::enter_print_call(&options);
    if let Some(handle) = format_opaque_handle_in_state(buffers, frames, threads, value) {
        return handle;
    }
    // Use the stateful printer when print-circle, print-level, or print-length
    // are active. This ensures correct handling of shared structure, depth
    // limiting, and length limiting throughout the entire value tree.
    if options.print_circle || options.print_level.is_some() || options.print_length.is_some() {
        return super::print::print_value_stateful_with_buffers(value, Some(buffers), options);
    }
    match value.kind() {
        ValueKind::Cons
        | ValueKind::String
        | ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record) => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}");
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_value_in_state_slow(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        _ => super::print::print_value_with_options(value, options),
    }
}

fn format_value_in_state_slow(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> String {
    match value.kind() {
        ValueKind::String => {
            let ls = value.as_lisp_string().expect("checked string");
            if options.print_noescape {
                crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())
            } else if let Some(runs) = get_string_text_properties_for_value(*value) {
                let mut out = String::from("#(");
                out.push_str(&format_lisp_string_emacs(ls, &options));
                for run in runs {
                    out.push(' ');
                    out.push_str(&run.start.to_string());
                    out.push(' ');
                    out.push_str(&run.end.to_string());
                    out.push(' ');
                    out.push_str(&format_value_in_state(
                        obarray, buffers, frames, threads, &run.plist, options,
                    ));
                }
                out.push(')');
                out
            } else {
                format_lisp_string_emacs(ls, &options)
            }
        }
        ValueKind::Cons => {
            if let Some(shorthand) =
                format_list_shorthand_in_state(obarray, buffers, frames, threads, value, options)
            {
                return shorthand;
            }
            let mut out = String::from("(");
            format_cons_in_state(obarray, buffers, frames, threads, value, &mut out, options);
            out.push(')');
            out
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if super::chartable::bool_vector_length(value).is_some()
                || super::chartable::char_table_external_slots(value).is_some()
                || super::chartable::sub_char_table_external_slots(value).is_some()
            {
                return super::print::print_value_with_options(value, options);
            }
            let mut out = String::from("[");
            let items = value.as_vector_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(' ');
                }
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, item, options,
                ));
            }
            out.push(']');
            out
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            let mut out = String::from("#s(");
            let items = value.as_record_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if let Some(length) = options.print_length
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
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, item, options,
                ));
            }
            out.push(')');
            out
        }
        _ => super::print::print_value_with_options(value, options),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
enum PrintShorthandSymbol {
    #[strum(serialize = "quote")]
    Quote,
    #[strum(serialize = "function")]
    Function,
    #[strum(serialize = "`")]
    Backquote,
    #[strum(serialize = ",")]
    Comma,
    #[strum(serialize = ",@")]
    CommaAt,
}

impl PrintShorthandSymbol {
    fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    fn string_form(
        self,
        payload: &Value,
        options: PrintOptions,
    ) -> Option<(&'static str, &Value, PrintOptions)> {
        match self {
            Self::Quote => Some(("'", payload, options)),
            Self::Function => Some(("#'", payload, options)),
            Self::Backquote => Some(("`", payload, options.enter_backquote())),
            Self::Comma => options.allow_unquote_shorthand().then_some((
                ",",
                payload,
                options.exit_backquote(),
            )),
            Self::CommaAt => options.allow_unquote_shorthand().then_some((
                ",@",
                payload,
                options.exit_backquote(),
            )),
        }
    }

    fn bytes_form(
        self,
        payload: &Value,
        options: PrintOptions,
    ) -> Option<(&'static [u8], &Value, PrintOptions)> {
        match self {
            Self::Quote => Some((b"'" as &[u8], payload, options)),
            Self::Function => Some((b"#'" as &[u8], payload, options)),
            Self::Backquote => Some((b"`" as &[u8], payload, options.enter_backquote())),
            Self::Comma => options.allow_unquote_shorthand().then_some((
                b"," as &[u8],
                payload,
                options.exit_backquote(),
            )),
            Self::CommaAt => options.allow_unquote_shorthand().then_some((
                b",@" as &[u8],
                payload,
                options.exit_backquote(),
            )),
        }
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

fn format_list_shorthand_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Option<String> {
    let items = super::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    if items[0].as_symbol_name() == Some("make-hash-table-from-literal") {
        let payload = quote_payload(&items[1])?;
        return Some(format!(
            "#s{}",
            format_value_in_state(obarray, buffers, frames, threads, &payload, options)
        ));
    }

    let shorthand = PrintShorthandSymbol::from_lisp_value(&items[0])?;
    if !options.print_quoted {
        return None;
    }

    let (prefix, quoted, nested_options) = shorthand.string_form(&items[1], options)?;

    Some(format!(
        "{prefix}{}",
        format_value_in_state(obarray, buffers, frames, threads, quoted, nested_options)
    ))
}

fn format_cons_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    out: &mut String,
    options: PrintOptions,
) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = options.print_length.unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut n: i64 = 2;
    let mut m: i64 = 2;
    let mut tortoise_idx: i64 = 0;
    let stack_len = format_object_stack_len();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if first {
                    if maxlen == 0 {
                        out.push_str("...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                } else {
                    out.push(' ');
                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.push_str("...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }

                    n -= 1;
                    if n == 0 {
                        tortoise_idx = tortoise_idx.saturating_add(m);
                        m = m.saturating_mul(2);
                        n = m;
                        tortoise = cursor;
                    } else if cursor == tortoise {
                        out.push_str(&format!(". #{tortoise_idx}"));
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                }
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, &pair_car, options,
                ));
                cursor = pair_cdr;
                first = false;
            }
            ValueKind::Nil => {
                truncate_format_object_stack(stack_len);
                return;
            }
            _ => {
                if !first {
                    out.push_str(" . ");
                }
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, &cursor, options,
                ));
                truncate_format_object_stack(stack_len);
                return;
            }
        }
    }
}

pub(crate) fn print_value_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
) -> Vec<u8> {
    if let Some(handle) = format_opaque_handle_in_state(buffers, frames, threads, value) {
        return handle.into_bytes();
    }
    format_value_bytes_in_state_with_options(
        obarray,
        buffers,
        frames,
        threads,
        value,
        // Same `Vprin1_to_string_buffer` stream as `print_value_in_state`.
        print_options_from_state(obarray, None),
    )
}

/// Byte-producing sibling of [`print_value_in_state_with_options`].  Renders a
/// value as canonical Emacs internal-encoding bytes (eight-bit/non-Unicode as
/// disjoint extended sequences), the form `prin1`/`print` feed straight to the
/// byte print sink (issue #131).
pub(crate) fn print_value_bytes_in_state_with_options(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    format_value_bytes_in_state_with_options(
        &ctx.obarray,
        &ctx.buffers,
        &ctx.frames,
        &ctx.threads,
        value,
        options,
    )
}

pub(crate) fn format_value_bytes_in_state_with_options(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let _print_guard = super::print::enter_print_call(&options);
    if let Some(handle) = format_opaque_handle_in_state(buffers, frames, threads, value) {
        return handle.into_bytes();
    }
    // Use the stateful printer when print-circle, print-level, or print-length
    // are active. Its canonical sink is byte-based so byte8/non-Unicode
    // characters survive graph preprocessing and depth/length limiting.
    if options.print_circle || options.print_level.is_some() || options.print_length.is_some() {
        return super::print::print_value_stateful_bytes_with_buffers(
            value,
            Some(buffers),
            options,
        );
    }
    match value.kind() {
        ValueKind::Cons => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_cons_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_vector_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        ValueKind::String => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_string_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_record_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        _ => super::print::print_value_bytes_with_options(value, options),
    }
}

fn format_string_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let ls = value.as_lisp_string().expect("checked string");
    if options.print_noescape {
        return ls.as_bytes().to_vec();
    }
    let str_bytes = format_lisp_string_bytes_emacs(ls, &options);
    let Some(runs) = get_string_text_properties_for_value(*value) else {
        return str_bytes;
    };
    let mut out = Vec::new();
    out.extend_from_slice(b"#(");
    out.extend_from_slice(&str_bytes);
    for run in runs {
        out.push(b' ');
        out.extend_from_slice(run.start.to_string().as_bytes());
        out.push(b' ');
        out.extend_from_slice(run.end.to_string().as_bytes());
        out.push(b' ');
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, &run.plist, options,
        ));
    }
    out.push(b')');
    out
}

fn format_cons_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    if let Some(shorthand) =
        format_list_shorthand_bytes_in_state(obarray, buffers, frames, threads, value, options)
    {
        return shorthand;
    }
    let mut out = Vec::new();
    out.push(b'(');
    append_cons_bytes_in_state(obarray, buffers, frames, threads, value, &mut out, options);
    out.push(b')');
    out
}

fn format_vector_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    if super::chartable::bool_vector_length(value).is_some()
        || super::chartable::char_table_external_slots(value).is_some()
        || super::chartable::sub_char_table_external_slots(value).is_some()
    {
        return super::print::print_value_bytes_with_options(value, options);
    }
    let mut out = Vec::new();
    out.push(b'[');
    let Some(values) = value.as_vector_data() else {
        out.push(b']');
        return out;
    };
    for (idx, item) in values.iter().enumerate() {
        if idx > 0 {
            out.push(b' ');
        }
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, item, options,
        ));
    }
    out.push(b']');
    out
}

fn format_record_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"#s(");
    let Some(values) = value.as_record_data() else {
        out.push(b')');
        return out;
    };
    for (idx, item) in values.iter().enumerate() {
        if let Some(length) = options.print_length
            && idx as i64 >= length
        {
            if idx > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(b"...");
            break;
        }
        if idx > 0 {
            out.push(b' ');
        }
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, item, options,
        ));
    }
    out.push(b')');
    out
}

fn format_list_shorthand_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Option<Vec<u8>> {
    let items = super::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    if items[0].as_symbol_name() == Some("make-hash-table-from-literal") {
        let payload = quote_payload(&items[1])?;
        let mut out = Vec::new();
        out.extend_from_slice(b"#s");
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, &payload, options,
        ));
        return Some(out);
    }

    let shorthand = PrintShorthandSymbol::from_lisp_value(&items[0])?;
    if !options.print_quoted {
        return None;
    }

    let (prefix, quoted, nested_options) = shorthand.bytes_form(&items[1], options)?;

    let mut out = Vec::new();
    out.extend_from_slice(prefix);
    out.extend(format_value_bytes_in_state_with_options(
        obarray,
        buffers,
        frames,
        threads,
        quoted,
        nested_options,
    ));
    Some(out)
}

fn quote_payload(value: &Value) -> Option<Value> {
    let items = super::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }
    if PrintShorthandSymbol::from_lisp_value(&items[0]) == Some(PrintShorthandSymbol::Quote) {
        Some(items[1])
    } else {
        None
    }
}

fn append_cons_bytes_in_state(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &super::threads::ThreadManager,
    value: &Value,
    out: &mut Vec<u8>,
    options: PrintOptions,
) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = options.print_length.unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut n: i64 = 2;
    let mut m: i64 = 2;
    let mut tortoise_idx: i64 = 0;
    let stack_len = format_object_stack_len();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if first {
                    if maxlen == 0 {
                        out.extend_from_slice(b"...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                } else {
                    out.push(b' ');
                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.extend_from_slice(b"...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }

                    n -= 1;
                    if n == 0 {
                        tortoise_idx = tortoise_idx.saturating_add(m);
                        m = m.saturating_mul(2);
                        n = m;
                        tortoise = cursor;
                    } else if cursor == tortoise {
                        out.extend_from_slice(format!(". #{tortoise_idx}").as_bytes());
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                }
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                out.extend(format_value_bytes_in_state_with_options(
                    obarray, buffers, frames, threads, &pair_car, options,
                ));
                cursor = pair_cdr;
                first = false;
            }
            ValueKind::Nil => {
                truncate_format_object_stack(stack_len);
                return;
            }
            _ => {
                if !first {
                    out.extend_from_slice(b" . ");
                }
                out.extend(format_value_bytes_in_state_with_options(
                    obarray, buffers, frames, threads, &cursor, options,
                ));
                truncate_format_object_stack(stack_len);
                return;
            }
        }
    }
}

/// Render a value with evaluator-context-aware opaque handle formatting.
pub fn print_value_with_eval(eval: &super::eval::Context, value: &Value) -> String {
    print_value_in_state(eval, value)
}

/// Render a value as bytes with evaluator-context-aware opaque handle formatting.
pub fn print_value_bytes_with_eval(eval: &super::eval::Context, value: &Value) -> Vec<u8> {
    print_value_bytes_in_state(
        &eval.obarray,
        &eval.buffers,
        &eval.frames,
        &eval.threads,
        value,
    )
}

/// Like [`print_value_bytes_with_eval`], but for a printer whose result becomes a
/// multibyte string (e.g. `format`/`message`'s `%S`, which GNU implements via
/// `Fprin1_to_string` printing into the multibyte `Vprin1_to_string_buffer`).
/// `print_prepare` binds `print-escape-nonascii' for a multibyte target, so a
/// unibyte string's raw high bytes are octal-escaped rather than emitted raw.
pub fn print_value_bytes_escaped_with_eval(eval: &super::eval::Context, value: &Value) -> Vec<u8> {
    if let Some(handle) =
        format_opaque_handle_in_state(&eval.buffers, &eval.frames, &eval.threads, value)
    {
        return handle.into_bytes();
    }
    let mut options = print_options_from_state(&eval.obarray, None);
    options.print_escape_nonascii = true;
    format_value_bytes_in_state_with_options(
        &eval.obarray,
        &eval.buffers,
        &eval.frames,
        &eval.threads,
        value,
        options,
    )
}

fn print_data_payload_with_eval(eval: &super::eval::Context, data: &[Value]) -> String {
    if data.is_empty() {
        "nil".to_string()
    } else {
        let parts = data
            .iter()
            .map(|v| print_value_with_eval(eval, v))
            .collect::<Vec<_>>();
        format!("({})", parts.join(" "))
    }
}

fn print_signal_payload_with_eval(
    eval: &super::eval::Context,
    raw_data: Option<&Value>,
    data: &[Value],
) -> String {
    if let Some(raw) = raw_data {
        return print_value_with_eval(eval, raw);
    }
    print_data_payload_with_eval(eval, data)
}

/// Render one signal in Lisp-readable form for diagnostics.
///
/// This intentionally avoids `Debug` on `Value`, which prints heap object
/// identities such as `String@0x...`.  Runtime diagnostics should show the
/// same string payloads a Lisp user would see.
pub(crate) fn format_signal_data_with_eval(
    eval: &super::eval::Context,
    sig: &SignalData,
) -> String {
    let payload = print_signal_payload_with_eval(eval, sig.raw_data.as_ref(), &sig.data);
    format!(
        "({} {})",
        format_symbol_name_for_diagnostic(sig.symbol),
        payload
    )
}

/// Render non-local control flow in Lisp-readable form for diagnostics.
pub(crate) fn format_flow_with_eval(eval: &super::eval::Context, flow: &Flow) -> String {
    match flow {
        Flow::Signal(sig) => format_signal_data_with_eval(eval, sig),
        Flow::Throw(thrown) => format!(
            "(no-catch ({} {}))",
            print_value_with_eval(eval, &thrown.tag),
            print_value_with_eval(eval, &thrown.value)
        ),
        Flow::ThreadBlocked(blocked) => {
            format!(
                "(thread-blocked {})",
                print_value_with_eval(eval, &blocked.blocker)
            )
        }
        Flow::Shutdown(request) => format!("(kill-emacs {})", request.exit_code),
    }
}

fn append_print_value_bytes_with_eval(
    eval: &super::eval::Context,
    value: &Value,
    out: &mut Vec<u8>,
) {
    out.extend_from_slice(&print_value_bytes_with_eval(eval, value));
}

/// Format an eval result for harnesses that have evaluator context and need
/// opaque handle rendering for thread/mutex/condvar/terminal values.
pub fn format_eval_result_with_eval(
    eval: &super::eval::Context,
    result: &Result<Value, EvalError>,
) -> String {
    match result {
        Ok(value) => format!("OK {}", print_value_with_eval(eval, value)),
        Err(EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        }) => {
            let payload = print_signal_payload_with_eval(eval, raw_data.as_ref(), data);
            format!(
                "ERR ({} {})",
                format_symbol_name_for_diagnostic(*symbol),
                payload
            )
        }
        Err(EvalError::UncaughtThrow { tag, value, .. }) => {
            format!(
                "ERR (no-catch ({} {}))",
                print_value_with_eval(eval, tag),
                print_value_with_eval(eval, value),
            )
        }
        Err(EvalError::Shutdown(request)) => format!("ERR (kill-emacs {})", request.exit_code),
    }
}

fn append_signal_payload_bytes_with_eval(
    eval: &super::eval::Context,
    raw_data: Option<&Value>,
    data: &[Value],
    out: &mut Vec<u8>,
) {
    if let Some(raw) = raw_data {
        append_print_value_bytes_with_eval(eval, raw, out);
    } else if data.is_empty() {
        out.extend_from_slice(b"nil");
    } else {
        out.push(b'(');
        for (idx, item) in data.iter().enumerate() {
            if idx > 0 {
                out.push(b' ');
            }
            append_print_value_bytes_with_eval(eval, item, out);
        }
        out.push(b')');
    }
}

/// Byte-preserving variant of `format_eval_result_with_eval`.
///
/// This preserves non-UTF-8 byte payloads in printed string literals used by
/// vm-compat corpus checks while still applying evaluator-aware opaque-handle
/// rendering for thread/mutex/condvar/terminal values.
pub fn format_eval_result_bytes_with_eval(
    eval: &super::eval::Context,
    result: &Result<Value, EvalError>,
) -> Vec<u8> {
    let mut out = Vec::new();
    match result {
        Ok(value) => {
            out.extend_from_slice(b"OK ");
            append_print_value_bytes_with_eval(eval, value, &mut out);
        }
        Err(EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        }) => {
            out.extend_from_slice(b"ERR (");
            append_print_value_bytes_with_eval(eval, &Value::from_sym_id(*symbol), &mut out);
            out.push(b' ');
            append_signal_payload_bytes_with_eval(eval, raw_data.as_ref(), data, &mut out);
            out.push(b')');
        }
        Err(EvalError::Shutdown(request)) => {
            out.extend_from_slice(format!("ERR (kill-emacs {})", request.exit_code).as_bytes());
        }
        Err(EvalError::UncaughtThrow { tag, value, .. }) => {
            out.extend_from_slice(b"ERR (no-catch (");
            append_print_value_bytes_with_eval(eval, tag, &mut out);
            out.push(b' ');
            append_print_value_bytes_with_eval(eval, value, &mut out);
            out.extend_from_slice(b"))");
        }
    }
    out
}
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

/// Signal wrong-number-of-arguments unless `args` has exactly `n` items.
pub(crate) fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Signal wrong-number-of-arguments unless `args` has at least `min` items.
pub(crate) fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Signal wrong-number-of-arguments unless `args` has at most `max` items.
pub(crate) fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Signal wrong-number-of-arguments unless `min <= args.len() <= max`.
pub(crate) fn expect_args_range(
    name: &str,
    args: &[Value],
    min: usize,
    max: usize,
) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Extract a fixnum or signal wrong-type-argument (fixnump).
pub(crate) fn expect_fixnum(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), *val],
        )),
    }
}

/// GNU's `cmd_error_internal` / `command-error-default-function` (keyboard.c:
/// 1030-1101): report an error that no Lisp handler caught, under a context
/// string naming where it happened.
///
/// This is the shared reporter. Its callers are the command loop and the
/// process filter/sentinel boundaries, which GNU routes through the very same
/// function so that "error in process filter: ..." reads like every other
/// unhandled-error report and obeys the same batch fatality.
impl super::eval::Context {
    /// The `(SYMBOL . DATA)` value GNU hands to `command-error-function`.
    pub(crate) fn signal_error_data_value(&self, sig: &SignalData) -> Value {
        let payload = match (&sig.raw_data, sig.data.is_empty()) {
            (Some(raw), _) => *raw,
            (None, true) => Value::NIL,
            (None, false) => Value::list(sig.data.clone()),
        };
        Value::cons(Value::from_sym_id(sig.symbol), payload)
    }

    /// GNU `cmd_error_internal`: hand DATA and CONTEXT to
    /// `command-error-function`, which reports it.
    ///
    /// Returns `Flow::Shutdown` when the report is fatal, which is how a batch
    /// session dies on an unhandled error; the caller must propagate it rather
    /// than resume the work the error escaped from.
    pub(crate) fn report_command_error(&mut self, data: Value, context: &str) -> Result<(), Flow> {
        // GNU clears quit-flag and sets inhibit-quit around the report, so a
        // pending C-g cannot interrupt the reporting of an earlier error.
        self.assign("quit-flag", Value::NIL);
        self.assign("inhibit-quit", Value::T);

        // GNU `cmd_error_internal` dereferences `Vcommand_error_function`
        // (`src/keyboard.c:1041-1042`), which the swap-in keeps equal to the
        // current buffer's binding; `lisp/simple.el` localises it. Ledger 196.
        let handler = self
            .obarray
            .value_in_buffer(self.buffers.current_buffer(), "command-error-function")
            .unwrap_or(Value::NIL);
        let context_value = Value::string(context);
        // GNU's variable defaults to the C function itself, so its handler is
        // always callable. Ours is preset to help.el's wrapper at bootstrap,
        // which only becomes callable once help.el is loaded -- before that
        // (and in a bare evaluator) the default report IS the behavior the
        // variable names, so call it directly rather than signalling
        // void-function while reporting an error.
        if handler.is_truthy() && self.function_value_is_callable(&handler) {
            self.apply(handler, vec![data, context_value, Value::NIL])
                .map(|_| ())
        } else {
            self.command_error_default_report(data, context_value)
        }
    }

    /// GNU `command-error-default-function` (keyboard.c:1049-1101). Batch and
    /// pre-display sessions write the diagnostic to stderr and exit -1 (status
    /// 255); a live session messages it and carries on.
    pub(crate) fn command_error_default_report(
        &mut self,
        data: Value,
        context: Value,
    ) -> Result<(), Flow> {
        let context_text = context.as_utf8_str().unwrap_or_default().to_string();
        let rendered = self.error_data_message(data);
        // GNU guards this branch with `!is_minibuffer_quit` (keyboard.c:1064):
        // aborting a minibuffer reports like any other quit but must never take
        // the session down, not even before the first frame is displayed.
        let is_minibuffer_quit = data.is_cons()
            && data.cons_car().as_symbol_id().is_some_and(|symbol| {
                super::errors::signal_matches_hierarchical_sym(
                    &self.obarray,
                    symbol,
                    intern("minibuffer-quit"),
                )
            });
        if !is_minibuffer_quit && self.noninteractive() {
            eprintln!("{context_text}{rendered}");
            // GNU calls Fkill_emacs (-1) here, which runs kill-emacs-hook and
            // exits with status 255. Same path as the kill-emacs builtin, so
            // the exit code is recorded before the flow unwinds.
            let _ = self.run_hook_if_bound("kill-emacs-hook");
            self.request_shutdown(-1, false);
            return Err(Flow::Shutdown(super::eval::ShutdownRequest {
                exit_code: -1,
                restart: false,
            }));
        }
        let text = format!("{context_text}{rendered}");
        super::builtins::misc_pure::builtin_message(self, vec![Value::string(text)])?;
        Ok(())
    }

    /// GNU `print_error_message`'s message half: `error-message-string` of the
    /// `(SYMBOL . DATA)` pair.
    fn error_data_message(&mut self, data: Value) -> String {
        match super::errors::builtin_error_message_string(self, vec![data]) {
            Ok(text) => text
                .as_utf8_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "peculiar error".to_string()),
            Err(_) => "peculiar error".to_string(),
        }
    }
}
