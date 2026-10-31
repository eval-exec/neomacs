//! Typed error and signal payloads for the evaluator.
//!
//! Formatting lives above this module (`print::error_format`) so the bottom
//! layer keeps only data. Internal control flow uses one owning machine word. Payload boxes and
//! Context-owned GC pins retain their existing ownership and lifetime.

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Formatter};

use super::intern::{SymId, intern, resolve_sym};
use super::value::{Value, ValueKind};
use crate::emacs_core::eval::ResumeTarget;
use crate::tagged::SymbolMode;
use strum::IntoStaticStr;

mod bytecode_backtrace;
use bytecode_backtrace::BytecodeErrorBacktrace;
pub(crate) use bytecode_backtrace::InlineErrorOp;

/// Public-facing evaluation error.
///
/// Both Lisp-carrying variants hold an [`InFlightRoots`] pin, for the same
/// reason [`Flow`]'s payloads do: a boundary that holds an `EvalError` while
/// more Lisp runs is holding Lisp values the precise collector cannot see. An
/// enum variant cannot have a private FIELD (a variant's fields are as visible
/// as the enum), so no owning state can be built from a struct literal outside
/// this module; [`EvalError::signal`] and [`EvalError::uncaught_throw`] remain
/// the public constructors. The printer rebuilds an error from an existing
/// [`Flow`] through the crate-internal `from_signal`/`from_throw` transfers,
/// which only accept a payload that was already pinned here. The signal state
/// also preserves delivery policy and completed handler selection.
/// Existing `EvalError::Signal { symbol, data, .. }` patterns keep working
/// unchanged; only construction sites move (DIVERGENCES.md 162).
#[derive(Clone, Debug)]
pub enum EvalError {
    Signal {
        symbol: SymId,
        data: Vec<Value>,
        raw_data: Option<Value>,
        /// Rooted signal state, not constructible outside this module.
        pin: EvalSignalState,
    },
    UncaughtThrow {
        tag: Value,
        value: Value,
        /// GNU's selected catch equality survives loader/public boundaries.
        symbol_mode: crate::tagged::SymbolMode,
        /// Not constructible outside `error.rs`; see the type docs.
        pin: InFlightRoots,
    },
    /// `kill-emacs` reached this boundary: not an error, an exit request.
    Shutdown(super::eval::ShutdownRequest),
}

/// Opaque signal state crossing the public error boundary. Its owning pin
/// covers the payload and any original memory-exhaustion descriptor; delivery
/// and completed handler selection stay with that pin through a roundtrip.
/// Private fields prevent construction or extraction of unrooted metadata.
#[derive(Clone, Debug)]
pub struct EvalSignalState {
    roots: InFlightRoots,
    delivery: SignalDelivery,
    selected_resume: Option<ResumeTarget>,
    search_complete: bool,
    bytecode_error_backtrace: Option<BytecodeErrorBacktrace>,
}

impl EvalError {
    /// The only public way to build a signal error: pins the symbol and payload
    /// as GC roots for as long as the error (or any clone of it) lives.
    pub fn signal(symbol: SymId, data: Vec<Value>, raw_data: Option<Value>) -> Self {
        Self::from_signal(SignalData::new(symbol, data, raw_data, false))
    }

    /// Transfer an existing signal without dropping its owning roots or
    /// restarting delivery after loader cleanups cross this public boundary.
    pub(crate) fn from_signal(signal: SignalData) -> Self {
        let SignalData {
            symbol,
            data,
            raw_data,
            delivery,
            selected_resume,
            search_complete,
            bytecode_error_backtrace,
            pin,
        } = signal;
        Self::Signal {
            symbol,
            data,
            raw_data,
            pin: EvalSignalState {
                roots: pin,
                delivery,
                selected_resume,
                search_complete,
                bytecode_error_backtrace,
            },
        }
    }

    /// The only way to build an uncaught-throw error; pins tag and value.
    pub fn uncaught_throw(tag: Value, value: Value) -> Self {
        Self::from_throw(ThrowData::new(tag, value))
    }

    /// Transfer the selected catch equality and owning roots together.
    /// GNU eval.c:1477 selects the catch before eval.c:1449 unbinds.
    pub(crate) fn from_throw(thrown: ThrowData) -> Self {
        let ThrowData {
            tag,
            value,
            symbol_mode,
            pin,
        } = thrown;
        Self::UncaughtThrow {
            tag,
            value,
            symbol_mode,
            pin,
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
            pin,
        } => {
            let EvalSignalState {
                roots,
                delivery,
                selected_resume,
                search_complete,
                bytecode_error_backtrace,
            } = pin;
            Flow::signal_boxed(Box::new(SignalData {
                symbol,
                data,
                raw_data,
                delivery,
                selected_resume,
                search_complete,
                bytecode_error_backtrace,
                pin: roots,
            }))
        }
        EvalError::UncaughtThrow {
            tag,
            value,
            symbol_mode,
            pin,
        } => Flow::from_kind(FlowKind::Throw(Box::new(ThrowData {
            tag,
            value,
            symbol_mode,
            pin,
        }))),
        EvalError::Shutdown(request) => Flow::shutdown(request),
    }
}

/// Internal non-local control flow: the owned view of a flow.
///
/// Match [`FlowKind`] after [`Flow::into_kind`], or inspect [`FlowRef`] after
/// [`Flow::kind`]. The carrier is a one-word owning pointer; this enum is its
/// owned view. Payload ownership and Context-owned GC pins remain unchanged.
#[derive(Clone, Debug)]
pub enum FlowKind {
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

mod flow_word;
/// The one-word owning carrier of internal control flow.
pub use flow_word::Flow;

/// The BORROWED view of a flow, for `match flow.kind()` and `if let`
/// guards. Built by [`Flow::kind`] and [`FlowResultExt::kinded_ref`].
#[derive(Clone, Copy, Debug)]
pub enum FlowRef<'a> {
    Signal(&'a SignalData),
    Throw(&'a ThrowData),
    ThreadBlocked(&'a ThreadBlockedData),
    Shutdown(super::eval::ShutdownRequest),
}

/// Views of a `Result<T, Flow>` whose error side is a [`FlowKind`] /
/// [`FlowRef`], for `match` scrutinees that name flow variants while keeping
/// the one-word carrier opaque.
pub trait FlowResultExt<T> {
    /// The owned view: `match result.kinded() { Err(FlowKind::Throw(t)) => .. }`.
    fn kinded(self) -> Result<T, FlowKind>;
    /// The borrowed view: `matches!(result.kinded_ref(), Err(FlowRef::Signal(_)))`.
    fn kinded_ref(&self) -> Result<&T, FlowRef<'_>>;
}

impl<T> FlowResultExt<T> for Result<T, Flow> {
    #[inline(always)]
    fn kinded(self) -> Result<T, FlowKind> {
        self.map_err(Flow::into_kind)
    }

    #[inline]
    fn kinded_ref(&self) -> Result<&T, FlowRef<'_>> {
        match self {
            Ok(value) => Ok(value),
            Err(flow) => Err(flow.kind()),
        }
    }
}

// These private result views inspect the borrowed carrier before consuming
// a signal payload. Callers keep the carrier opaque and evaluate each result
// once. The macros introduce no state or threading assumptions.
macro_rules! is_signal_result {
    ($result:expr) => {
        matches!(&$result, Err(flow) if flow.is_signal())
    };
}
pub(crate) use is_signal_result;

macro_rules! with_signal_result {
    ($result:expr, $signal:ident => $body:expr) => {
        match $result {
            Err(flow) if flow.is_signal() => {
                let $crate::emacs_core::error::FlowKind::Signal($signal) = flow.into_kind() else {
                    unreachable!("signal result was checked before consuming its carrier")
                };
                $body
            }
            other => other,
        }
    };
}
pub(crate) use with_signal_result;

impl Flow {
    /// Keep GNU's throw-time EQ policy while dynamic bindings unwind.
    pub(crate) fn throw_with_symbols(
        tag: Value,
        value: Value,
        mode: crate::tagged::SymbolMode,
    ) -> Self {
        Self::from_kind(FlowKind::Throw(Box::new(ThrowData::new_with_symbols(
            tag, value, mode,
        ))))
    }
}

impl Flow {
    /// Pin the throw payload for the entire carrier lifetime.
    pub(crate) fn throw(tag: Value, value: Value) -> Self {
        Self::from_kind(FlowKind::Throw(Box::new(ThrowData::new(tag, value))))
    }

    /// Pin the cooperative yield payload for the entire carrier lifetime.
    pub(crate) fn thread_blocked(blocker: Value, remaining_forms: Value) -> Self {
        Self::from_kind(FlowKind::ThreadBlocked(Box::new(ThreadBlockedData::new(
            blocker,
            remaining_forms,
        ))))
    }
}

impl FlowKind {
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
    // Never called: this method is the seal (its return type is only
    // constructible here), and the compile-time guarantee IS the product.
    #[allow(dead_code)]
    fn in_flight_roots(&self) -> &InFlightRoots;
}

impl InFlightPinned for EvalSignalState {
    fn in_flight_roots(&self) -> &InFlightRoots {
        &self.roots
    }
}

/// `(throw TAG VALUE)` in flight.
#[derive(Clone, Debug)]
pub struct ThrowData {
    pub tag: Value,
    pub value: Value,
    symbol_mode: crate::tagged::SymbolMode,
    /// See [`SignalData::pin`]. PRIVATE, so `Flow::throw` is the only way in.
    #[allow(dead_code)] // held for its Drop (the GC pin); read only via the sealed trait
    pin: InFlightRoots,
}

impl ThrowData {
    fn new(tag: Value, value: Value) -> Self {
        Self::new_with_symbols(tag, value, crate::tagged::SymbolMode::Bare)
    }

    fn new_with_symbols(tag: Value, value: Value, symbol_mode: crate::tagged::SymbolMode) -> Self {
        let pin = InFlightRoots::pin([tag, value]);
        Self {
            tag,
            value,
            symbol_mode,
            pin,
        }
    }

    pub(crate) fn symbol_mode(&self) -> crate::tagged::SymbolMode {
        self.symbol_mode
    }

    /// GNU eval.c:1477 selects the catch before unbinding the throwing extent.
    pub(crate) fn matches_tag(&self, tag: Value) -> bool {
        self.tag.bits() == tag.bits()
            || (self.symbol_mode == crate::tagged::SymbolMode::Positioned
                && self.symbol_mode.unwrap(self.tag).bits() == self.symbol_mode.unwrap(tag).bits())
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
    #[allow(dead_code)] // held for its Drop (the GC pin); read only via the sealed trait
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

/// A validated (SYMBOL . DATA) memory-signal-data object, retained unchanged
/// through GNU's allocation-exhaustion delivery. This copied witness is
/// confined to its mutator; SignalData pins the object while it is in flight.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MemorySignalBinding {
    original: Value,
    symbol: SymId,
    tail: Value,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(crate) enum MemorySignalBindingError {
    #[error("memory-signal-data is not a cons")]
    NotCons,
    #[error("memory-signal-data has a non-symbol condition")]
    NotSymbol,
}

impl TryFrom<(Value, SymbolMode)> for MemorySignalBinding {
    type Error = MemorySignalBindingError;

    fn try_from((original, symbol_mode): (Value, SymbolMode)) -> Result<Self, Self::Error> {
        if !original.is_cons() {
            return Err(MemorySignalBindingError::NotCons);
        }
        let symbol = original
            .cons_car()
            .as_symbol_id(symbol_mode)
            .ok_or(MemorySignalBindingError::NotSymbol)?;
        Ok(Self {
            original,
            symbol,
            tail: original.cons_cdr(),
        })
    }
}

impl MemorySignalBinding {
    fn from_signal_parts(symbol: SymId, tail: Value) -> Self {
        Self {
            original: Value::cons(Value::from_sym_id(symbol), tail),
            symbol,
            tail,
        }
    }

    pub(crate) fn original(self) -> Value {
        self.original
    }
}

/// GNU eval.c:1948-1953 distinguishes allocation exhaustion from ordinary
/// signals and hook-suppressed signals. Memory exhaustion retains the original
/// error object and permits neither hook nor debugger reentry. This delivery
/// policy travels with its signal; the memory-exhaustion variant retains
/// mutator-bound Values whose lifetime is covered by SignalData's pin.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SignalDelivery {
    Ordinary,
    HookSuppressed,
    MemoryExhausted(MemorySignalBinding),
}

#[derive(Clone, Debug)]
pub struct SignalData {
    pub symbol: SymId,
    pub data: Vec<Value>,
    /// Original cdr payload when a signal uses non-list data.
    pub raw_data: Option<Value>,
    delivery: SignalDelivery,
    pub(crate) selected_resume: Option<ResumeTarget>,
    pub(crate) search_complete: bool,
    pub(crate) bytecode_error_backtrace: Option<BytecodeErrorBacktrace>,
    /// Keeps `data` and `raw_data` reachable for the collector while this
    /// signal is in flight. PRIVATE on purpose: it is what makes an unrooted
    /// signal payload unrepresentable outside this module — a struct with a
    /// private field cannot be built from a literal elsewhere, so every
    /// construction site has to go through the constructors below, which pin.
    /// See [`InFlightRoots`] for why the pin is needed at all.
    #[allow(dead_code)] // held for its Drop (the GC pin); read only via the sealed trait
    pin: InFlightRoots,
}

impl SignalData {
    /// Compatibility constructor for ordinary or hook-suppressed signals.
    /// Every constructor pins its payload for the lifetime of this value.
    pub(crate) fn new(
        symbol: SymId,
        data: Vec<Value>,
        raw_data: Option<Value>,
        suppress_signal_hook: bool,
    ) -> Self {
        let delivery = if suppress_signal_hook {
            SignalDelivery::HookSuppressed
        } else {
            SignalDelivery::Ordinary
        };
        Self::new_with_delivery(symbol, data, raw_data, delivery)
    }

    /// Build a signal with its typed delivery policy, pinning both the payload
    /// and any original memory-exhaustion binding for the entire signal lifetime.
    pub(crate) fn new_with_delivery(
        symbol: SymId,
        data: Vec<Value>,
        raw_data: Option<Value>,
        delivery: SignalDelivery,
    ) -> Self {
        let binding = match delivery {
            SignalDelivery::Ordinary | SignalDelivery::HookSuppressed => None,
            SignalDelivery::MemoryExhausted(binding) => Some(binding.original()),
        };
        let pin = InFlightRoots::pin(
            std::iter::once(Value::from_sym_id(symbol))
                .chain(data.iter().copied())
                .chain(raw_data)
                .chain(binding),
        );
        Self {
            symbol,
            data,
            raw_data,
            delivery,
            selected_resume: None,
            search_complete: false,
            bytecode_error_backtrace: None,
            pin,
        }
    }

    pub(crate) fn delivery(&self) -> SignalDelivery {
        self.delivery
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
// This collector is PRECISE — there is no conservative stack scan at all
// (`crates/neovm-core/src/tagged/CONCURRENT_GC.md`, "precise-rooting precondition")
// — and a signal does not longjmp here, it
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
// The table is a Context-owned slot arena rather than the `SCRATCH_GC_ROOTS`
// stack because a signal's lifetime is NOT stack-shaped: it is cloned, boxed,
// stored in a resume target, and converted to and from `EvalError`, so roots
// are released in an order a truncating stack cannot express.

thread_local! {
    static IN_FLIGHT_ROOTS: RefCell<Option<InFlightRegistryHandle>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct InFlightRootTable {
    /// One entry per live pin; `None` marks a reusable slot.
    slots: Vec<Option<Vec<PinnedWord>>>,
    free: Vec<usize>,
}

/// The word of one pinned in-flight value.
///
/// The registry is shared through a mutex, so it holds words rather than
/// thread-confined values. Its table roots each word for the registry's heap,
/// and the word becomes a value again only in that heap's root walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct PinnedWord(usize);

impl PinnedWord {
    fn of(value: Value) -> Self {
        Self(value.bits())
    }

    fn value(self) -> Value {
        Value::from_bits(self.0)
    }
}

static_assertions::assert_impl_all!(InFlightRegistryHandle: Send, Sync);

/// Heap-identified registry storage for one Context's in-flight roots.
/// The slot arena uses a mutex so root publication and collection may access
/// registry metadata concurrently. Context and public EvalError stay on their
/// owner threads; sharing registry storage does not transfer either owner.
#[derive(Clone)]
pub(crate) struct InFlightRegistryHandle {
    heap_identity: Option<usize>,
    table: std::sync::Arc<std::sync::Mutex<InFlightRootTable>>,
}

impl InFlightRegistryHandle {
    fn new(heap_identity: Option<usize>) -> Self {
        Self {
            heap_identity,
            table: std::sync::Arc::new(std::sync::Mutex::new(InFlightRootTable::default())),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InFlightRootTable> {
        self.table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

pub(crate) fn current_in_flight_registry_handle() -> InFlightRegistryHandle {
    let heap_identity = crate::tagged::gc::current_tagged_heap_identity();
    IN_FLIGHT_ROOTS.with(|active| {
        let mut active = active.borrow_mut();
        if active
            .as_ref()
            .is_none_or(|registry| registry.heap_identity != heap_identity)
        {
            *active = Some(InFlightRegistryHandle::new(heap_identity));
        }
        active
            .as_ref()
            .expect("installed in-flight registry")
            .clone()
    })
}

pub(crate) fn install_in_flight_registry_handle(registry: &InFlightRegistryHandle) {
    IN_FLIGHT_ROOTS.with(|active| *active.borrow_mut() = Some(registry.clone()));
}

struct InFlightRootPin {
    registry: InFlightRegistryHandle,
    slot: usize,
}

/// A pin on one in-flight payload's heap values. Owns a slot in its Context's
/// registry for its whole life; clones take independent slots in that same
/// registry, and Drop releases the owning slot even after local Context
/// activation changes. The payload and this pin remain on their owner thread.
pub struct InFlightRoots {
    /// `None` for a payload with no traceable values, avoiding registry traffic.
    pin: Option<InFlightRootPin>,
    /// Flow payloads and Context remain local to their evaluator call stack.
    /// Shared registry metadata does not make a payload transferable.
    _not_send: std::marker::PhantomData<*const ()>,
}

/// A mutator-local Lisp value retained independently of temporary evaluator
/// root scopes. The existing Context registry follows Context activation;
/// the handle releases its root when dropped. The pin stays private so the
/// in-flight payload construction contract remains sealed to this module.
#[derive(Debug)]
#[must_use = "dropping this handle ends retention of its Lisp value"]
pub(crate) struct RetainedLispValue {
    value: Value,
    _pin: InFlightRoots,
}

impl From<Value> for RetainedLispValue {
    fn from(value: Value) -> Self {
        Self {
            value,
            _pin: InFlightRoots::pin([value]),
        }
    }
}

impl AsRef<Value> for RetainedLispValue {
    fn as_ref(&self) -> &Value {
        &self.value
    }
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
        let mut values: Vec<PinnedWord> = Vec::new();
        for value in payload {
            Self::push_if_traceable(&mut values, value);
        }
        if values.is_empty() {
            return Self {
                pin: None,
                _not_send: std::marker::PhantomData,
            };
        }
        Self::claim(current_in_flight_registry_handle(), values)
    }

    /// Keep what the mark phase can act on. Heap objects obviously; symbols
    /// because `seed_root` routes them to `mark_symbol`, which is what keeps a
    /// non-canonical symbol's cells. Fixnums, `nil` and `t` are immediates the
    /// collector never touches.
    #[inline]
    fn push_if_traceable(values: &mut Vec<PinnedWord>, value: Value) {
        if value.is_nil() || value.is_t() {
            return;
        }
        if value.is_heap_object() || value.is_bare_symbol() {
            values.push(PinnedWord::of(value));
        }
    }

    fn claim(registry: InFlightRegistryHandle, values: Vec<PinnedWord>) -> Self {
        let slot = {
            let mut table = registry.lock();
            match table.free.pop() {
                Some(slot) => {
                    table.slots[slot] = Some(values);
                    slot
                }
                None => {
                    table.slots.push(Some(values));
                    table.slots.len() - 1
                }
            }
        };
        Self {
            pin: Some(InFlightRootPin { registry, slot }),
            _not_send: std::marker::PhantomData,
        }
    }
}

impl Clone for InFlightRoots {
    fn clone(&self) -> Self {
        let Some(pin) = &self.pin else {
            return Self {
                pin: None,
                _not_send: std::marker::PhantomData,
            };
        };
        let values = pin.registry.lock().slots[pin.slot]
            .as_ref()
            .expect("live in-flight pin owns its slot")
            .clone();
        Self::claim(pin.registry.clone(), values)
    }
}

impl Drop for InFlightRoots {
    fn drop(&mut self) {
        let Some(pin) = &self.pin else { return };
        let mut table = pin.registry.lock();
        table.slots[pin.slot] = None;
        table.free.push(pin.slot);
    }
}

impl std::fmt::Debug for InFlightRoots {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("InFlightRoots")
    }
}

/// Collect the owning Context's pins, independent of the active TLS view.
pub(crate) fn collect_in_flight_registry_gc_roots(
    out: &mut Vec<Value>,
    registry: &InFlightRegistryHandle,
    heap_id: usize,
) {
    if registry.heap_identity.is_some_and(|owner| owner != heap_id) {
        return;
    }
    let table = registry.lock();
    for values in table.slots.iter().flatten() {
        for value in values.iter().map(|word| word.value()) {
            if registry.heap_identity.is_some() || !value.is_heap_object() {
                out.push(value);
            }
        }
    }
}

/// The active-view root collector, retained for ownership proof tests.
#[cfg(test)]
pub(crate) fn collect_in_flight_flow_gc_roots(out: &mut Vec<Value>, heap_id: usize) {
    IN_FLIGHT_ROOTS.with(|active| {
        if let Some(registry) = &*active.borrow() {
            collect_in_flight_registry_gc_roots(out, registry, heap_id);
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
    #[strum(serialize = "file-date-error")]
    FileDateError,
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
    #[strum(serialize = "missing-module-init-function")]
    MissingModuleInitFunction,
    #[strum(serialize = "module-init-failed")]
    ModuleInitFailed,
    #[strum(serialize = "module-not-gpl-compatible")]
    ModuleNotGplCompatible,
    #[strum(serialize = "module-open-failed")]
    ModuleOpenFailed,
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
    #[inline(always)]
    fn condition_sym(self) -> SymId {
        intern(self.name())
    }
}

/// A Lisp condition selected dynamically; retains the actual symbol identity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UserCondition(SymId);

impl From<SymId> for UserCondition {
    fn from(symbol: SymId) -> Self {
        Self(symbol)
    }
}

impl IntoConditionSym for UserCondition {
    fn condition_sym(self) -> SymId {
        self.0
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

/// GNU alloc.c:7474-7477's bootstrap memory exhaustion condition.
/// Context-aware allocation paths use the live `memory-signal-data` instead.
#[cold]
pub(crate) fn memory_exhausted_error() -> Flow {
    let symbol = LispCondition::Error.condition_sym();
    let data = vec![Value::string(
        "Memory exhausted--use M-x save-some-buffers then exit and restart Emacs",
    )];
    let tail = Value::list_from_slice(&data);
    let binding = MemorySignalBinding::from_signal_parts(symbol, tail);
    Flow::signal_boxed(Box::new(SignalData::new_with_delivery(
        symbol,
        data,
        Some(tail),
        SignalDelivery::MemoryExhausted(binding),
    )))
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
#[inline(never)]
pub(crate) fn signal_internal_id(
    symbol: SymId,
    data: Vec<Value>,
    raw_data: Option<Value>,
    suppress_signal_hook: bool,
) -> Flow {
    Flow::signal_boxed(Box::new(SignalData::new(
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

/// GNU `eval.c` `signal_error`: signal `error` with MESSAGE and ARG.
///
/// A proper-list ARG contributes its elements to the signal data; any other
/// ARG is a single datum.  GNU's `signal_error` is used by special forms that
/// pass a malformed subform (for example `let`'s multi-value binding), and the
/// flattened data is observable through `condition-case`'s error object.
pub(crate) fn signal_error_with_arg(message: &str, arg: Value) -> Flow {
    let mut data = vec![Value::string(message)];
    match super::value::list_to_vec(&arg) {
        Some(items) => data.extend(items),
        None => data.push(arg),
    }
    signal(LispCondition::Error, data)
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

/// Build the binding value for condition-case variable: (symbol . data)
pub(crate) fn make_signal_binding_value(sig: &SignalData) -> Value {
    match sig.delivery {
        SignalDelivery::MemoryExhausted(binding) => return binding.original(),
        SignalDelivery::Ordinary | SignalDelivery::HookSuppressed => {}
    }
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
    let symbol_id = pair_car.as_bare_symbol_id()?;
    Some(signal_with_data_id(symbol_id, tail))
}

/// Deliver GNU alloc.c:4142's live memory-signal-data, keeping its original
/// error object (including a dotted cdr) and the OOM delivery policy together.
#[cold]
pub(crate) fn memory_signal_from_binding_value(value: Value, symbol_mode: SymbolMode) -> Flow {
    let binding = match MemorySignalBinding::try_from((value, symbol_mode)) {
        Ok(binding) => binding,
        // Fsignal turns a nil error symbol and non-cons data into an
        // ordinary error (GNU eval.c:1924-1925), preserving its raw cdr.
        Err(MemorySignalBindingError::NotCons) => {
            return signal_with_data(LispCondition::Error, value);
        }
        // For an OOM cons, Fget checks its actual condition designator
        // (GNU eval.c:1978; fns.c:2653). Validation must not silently replace
        // an invalid user binding with the bootstrap exhaustion descriptor.
        Err(MemorySignalBindingError::NotSymbol) => {
            return signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), value.cons_car()],
            );
        }
    };
    let data = super::value::list_to_vec(&binding.tail).unwrap_or_else(|| vec![binding.tail]);
    Flow::signal_boxed(Box::new(SignalData::new_with_delivery(
        binding.symbol,
        data,
        Some(binding.tail),
        SignalDelivery::MemoryExhausted(binding),
    )))
}

#[cfg(test)]
#[path = "tests/flow_kind_test.rs"]
mod flow_kind_tests;
#[cfg(test)]
#[path = "tests/error_test.rs"]
mod tests;

/// Signal wrong-number-of-arguments unless `args` has exactly `n` items.
/// The shared cold arm of the four `expect_*_args` checks below. Each check
/// is a length compare, but with the signal built inline — a `vec!`, an
/// interned symbol, a boxed fixnum — the body was too big for LLVM to inline,
/// so every `Many`-arity builtin paid a real call frame (17 Ir, ~280K times
/// per org edit iteration across the three) to compare two integers. Same
/// split as [`expect_fixnum`].
#[cold]
#[inline(never)]
fn wrong_number_of_arguments(name: &str, nargs: usize) -> Flow {
    signal(
        LispCondition::WrongNumberOfArguments,
        vec![Value::symbol(name), Value::fixnum(nargs as i64)],
    )
}

/// Signal wrong-number-of-arguments unless `args` has exactly `n` items.
#[inline]
pub(crate) fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(wrong_number_of_arguments(name, args.len()))
    } else {
        Ok(())
    }
}

/// Signal wrong-number-of-arguments unless `args` has at least `min` items.
#[inline]
pub(crate) fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(wrong_number_of_arguments(name, args.len()))
    } else {
        Ok(())
    }
}

/// Signal wrong-number-of-arguments unless `args` has at most `max` items.
#[inline]
pub(crate) fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(wrong_number_of_arguments(name, args.len()))
    } else {
        Ok(())
    }
}

/// Signal wrong-number-of-arguments unless `min <= args.len() <= max`.
#[inline]
pub(crate) fn expect_args_range(
    name: &str,
    args: &[Value],
    min: usize,
    max: usize,
) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(wrong_number_of_arguments(name, args.len()))
    } else {
        Ok(())
    }
}

/// Extract a fixnum or signal wrong-type-argument (fixnump).
///
/// The success arm is a tag test, but building the signal (a `vec!` plus an
/// interned symbol) made the whole body too big for LLVM to inline at any of
/// its call sites — so `aref` paid an out-of-line call at 18 Ir for what GNU's
/// `CHECK_FIXNUM` does in a compare and a branch, 243K times per org edit
/// iteration. Splitting the cold arm out is the same shape the JIT already
/// uses for `retier_heat_init` and `direct_call_cold`.
#[inline]
pub(crate) fn expect_fixnum(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(expect_fixnum_failed(val)),
    }
}

#[cold]
#[inline(never)]
pub(crate) fn expect_fixnum_failed(val: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("fixnump"), *val],
    )
}

#[cfg(test)]
#[path = "tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;

#[cfg(test)]
#[path = "tests/flow_word_test.rs"]
mod flow_word_tests;

#[cfg(test)]
#[path = "tests/flow_word_gc_test.rs"]
mod flow_word_gc_tests;

#[cfg(test)]
#[path = "tests/memory_signal_policy_test.rs"]
mod memory_signal_policy_tests;

#[cfg(test)]
#[path = "tests/issue554_throw_roundtrip.rs"]
mod issue554_throw_roundtrip_tests;

/// Carrier park: forget the owner's in-flight root registry handle. The
/// handle points into a Context-owned root table, not into the heap, but the
/// next owner installs its own handle, so the parked carrier keeps nothing.
pub(crate) fn park_carrier_tls() {
    IN_FLIGHT_ROOTS.with(|slot| *slot.borrow_mut() = None);
}

/// Debug-only carrier TLS fingerprint for this module's classified record.
/// `FORMAT_OBJECT_STACK` moved to the print error-format module; its probe
/// rides along `print::carrier_tls_fingerprint`.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    crate::tls_fingerprint::mix(
        h,
        IN_FLIGHT_ROOTS.with(|slot| slot.borrow().is_some() as u64),
    );
}
