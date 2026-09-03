//! Context — special forms, function application, and dispatch.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::hash::Hash;
use std::path::Path;
use std::sync::OnceLock;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use strum::{EnumString, IntoStaticStr};

use super::abbrev::AbbrevManager;
use super::advice::VariableWatcherList;
use super::autoload::AutoloadManager;
use super::bookmark::BookmarkManager;
use super::builtins;
use super::builtins::from_value::FromValue;
use super::coding::CodingSystemManager;
use super::command_observation::{
    UserCommandIdentity, UserCommandObservation, UserCommandObservationStart, UserCommandOutcome,
};
use super::custom::CustomManager;
use super::debug_on_call::DebugOnCallCode;
pub use super::display_host::{
    DisplayHost, FrameFontRequest, FrameFontSize, GraphicalFaceAttribute, TerminalCreateRequest,
    TerminalDisplayTarget, TerminalFloatPlacement, TerminalGridSize, TerminalId,
    XwidgetScriptRequestId,
};
use super::error::*;
use super::interactive::InteractiveRegistry;
use super::intern::{
    SymId, format_symbol_name_for_diagnostic, intern, intern_uninterned, is_canonical_id,
    is_keyword_id, resolve_sym, symbol_name_id,
};
use super::keymap::{list_keymap_define, list_keymap_set_parent, make_sparse_list_keymap};
use super::kmacro::KmacroManager;
use super::minibuffer::MinibufferManager;
use super::mode::ModeRegistry;
use super::process::ProcessManager;
use super::rect::RectangleState;
use super::regex::MatchData;
use super::register::RegisterManager;
use super::symbol::{ConstantWrite, Obarray};
use super::terminal::pure::TtyFrameHostFactory;
use super::threads::ThreadManager;
use super::value::*;
use crate::buffer::{BufferId, BufferManager, CharPos0, EmacsBytePos, LispCharPos1};
use crate::face::{FaceTable, FontSlant, FontWeight, FontWidth};
use crate::gc_trace::GcTrace;
use crate::tagged::header::{
    CLOSURE_ARGLIST, SubrDispatchKind, SubrFn, SubrInteractivity, SubrObj,
};
use crate::window::{FrameFullscreen, FrameManager, WindowId, WindowLayoutQueryAdapter};

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
use subrs::{CallableHandler, EvaluatorHandler, SpecialFormHandler, evaluator_handler};
pub(crate) use subrs::{evaluator_dispatch_kind, register_public_subrs, register_subrs};

/// Stress-GC at every allocation-bearing safe point when `NEOVM_GC_STRESS=1`.
/// Mirrors the per-evaluator `gc_stress` test flag, exposed as an env hook so a
/// real binary run exercises the incremental/concurrent collectors hard (every
/// safe point collects). Default off — production behavior is unchanged.
fn gc_stress_from_env() -> bool {
    std::env::var("NEOVM_GC_STRESS").as_deref() == Ok("1")
}

/// Optional process-wide cap for controlled GC pacing experiments.
///
/// Lisp's `gc-cons-threshold` remains authoritative in normal runs. Setting
/// this hook lets the profiler measure the memory/time curve of configs that
/// deliberately defer GC (Doom uses `most-positive-fixnum` during startup)
/// without editing the user's configuration.
fn gc_threshold_cap_from_env() -> Option<usize> {
    static CAP: OnceLock<Option<usize>> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("NEOVM_GC_THRESHOLD_CAP_BYTES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|cap| *cap > 0)
    })
}

const EVAL_STACK_RED_ZONE: usize = 128 * 1024;
const EVAL_STACK_SEGMENT: usize = 2 * 1024 * 1024;
const STACK_GROWTH_PROBE_START_DEPTH: usize = 16;
const STACK_GROWTH_PROBE_INTERVAL: usize = 16;
/// Capacity of the per-Context cache mapping symbol → resolved call
/// target.  The cache is keyed by `function_epoch` and invalidated
/// whenever the obarray's function cells change.  GNU Emacs has no such
/// cache (its dispatcher walks the symbol's function cell directly per
/// call), but in NeoMacs's debug build a fast path that avoids
/// `resolve_sym`/`intern` lock acquisitions per call is a major win
/// for byte-compiler workloads.  4096 entries comfortably covers the
/// distinct functions called during batch-byte-compile so the cache
/// never thrashes once warmed.
const NAMED_CALL_CACHE_CAPACITY: usize = 4096;
const LEXENV_ASSQ_CACHE_CAPACITY: usize = 16;
const LEXENV_SPECIAL_CACHE_CAPACITY: usize = 16;
const GC_DEFAULT_THRESHOLD_BYTES: usize = 100_000 * std::mem::size_of::<usize>();
const GC_THRESHOLD_FLOOR_BYTES: usize = GC_DEFAULT_THRESHOLD_BYTES / 10;
/// Bound peak arena growth while startup configs deliberately defer Lisp GC.
/// The host releases the ceiling after its bounded startup settling window.
const GC_STARTUP_THRESHOLD_CEILING_BYTES: usize = 4 * 1024 * 1024;
const GC_HI_THRESHOLD_BYTES: usize = (i64::MAX as usize) / 2;
const GC_PERCENT_SCALE: u64 = 1_000_000;
/// Live-proportional adaptive trigger (`effective_gc_threshold_bytes`): do not
/// start the next cycle until at least `live_bytes × NUM/DEN` fresh bytes have
/// been allocated, so the O(live) full-mark cost amortizes as the live heap
/// grows (total mark work stays O(bytes allocated)) instead of re-marking the
/// whole heap every fixed `gc-cons-threshold` bytes. The elisp-derived value
/// (`gc-cons-threshold`/`gc-cons-percentage`) is a FLOOR this term can only
/// raise, never lower, and `GC_HI_THRESHOLD_BYTES` still caps the result.
const GC_LIVE_GROWTH_NUM: u128 = 1;
const GC_LIVE_GROWTH_DEN: u128 = 2;
pub(crate) const INTERNAL_COMPILER_FUNCTION_OVERRIDES: &str =
    "internal--compiler-function-overrides";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EchoMessageClearResult {
    ClearEchoArea,
    PreserveEchoArea,
}

/// Definition/provenance records accepted by GNU's `load-history`.
///
/// Keep the Lisp encoding and its duplicate policy behind this enum so
/// definition primitives cannot invent raw cons shapes or accidentally apply
/// `require`'s deduplication rule to ordinary definitions.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LoadHistoryEntry {
    Variable(SymId),
    Function {
        symbol: Value,
        definition_kind: FunctionDefinitionKind,
    },
    ProvidedFeature(Value),
    RequiredFeature(Value),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FunctionDefinitionKind {
    Concrete,
    Autoload,
}

#[derive(Clone, Copy, Debug, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum TaggedLoadHistoryKind {
    Defun,
    Provide,
    Require,
}

impl LoadHistoryEntry {
    pub(crate) fn function(symbol: Value, definition: Value) -> Self {
        let definition_kind = if super::autoload::is_autoload_value(&definition) {
            FunctionDefinitionKind::Autoload
        } else {
            FunctionDefinitionKind::Concrete
        };
        Self::Function {
            symbol,
            definition_kind,
        }
    }

    fn into_lisp_value(self) -> Value {
        let (kind, subject) = match self {
            Self::Variable(symbol) => return value_from_symbol_id(symbol),
            Self::Function { symbol, .. } => (TaggedLoadHistoryKind::Defun, symbol),
            Self::ProvidedFeature(feature) => (TaggedLoadHistoryKind::Provide, feature),
            Self::RequiredFeature(feature) => (TaggedLoadHistoryKind::Require, feature),
        };
        let kind_name: &'static str = kind.into();
        Value::cons(Value::symbol(kind_name), subject)
    }

    fn should_deduplicate(self) -> bool {
        matches!(self, Self::RequiredFeature(_))
    }

    fn is_autoload_definition(self) -> bool {
        matches!(
            self,
            Self::Function {
                definition_kind: FunctionDefinitionKind::Autoload,
                ..
            }
        )
    }
}

fn gnu_system_type() -> &'static str {
    std::cfg_select! {
        target_family = "wasm" => { "wasm" }
        target_os = "android" => { "android" }
        target_os = "windows" => { "windows-nt" }
        target_os = "macos" => { "darwin" }
        target_os = "linux" => { "gnu/linux" }
        _ => { std::env::consts::OS }
    }
}

/// GNU initializes `features' from the C subsystems a build actually linked:
/// one `configure` switch decides both the implementation and the `Fprovide`
/// that advertises it, so GNU cannot advertise what it did not build.  Ledger
/// 192 made that true here too -- the list is derived from
/// [`super::c_features::gnu_c_features`], a table whose every row names either
/// the implementation behind the feature or the reason there is none.
fn initial_feature_names() -> Vec<&'static str> {
    super::c_features::initial_feature_names()
}

fn initial_features_value() -> Value {
    Value::list(
        initial_feature_names()
            .into_iter()
            .map(Value::symbol)
            .collect(),
    )
}

fn initial_feature_ids() -> Vec<SymId> {
    initial_feature_names().into_iter().map(intern).collect()
}

/// GNU's `echo_buffer[2]`: the two buffers the echo area is allowed to display
/// (src/xdisp.c:785).
///
/// GNU holds them as Lisp OBJECTS and `ensure_echo_area_buffers'
/// (src/xdisp.c:12862-12884) replaces one only when it has DIED. Identity is
/// therefore the buffer itself, not its name, and two things follow that a
/// name lookup gets wrong in both directions: renaming an echo buffer must not
/// detach the echo area from it, and a user buffer that afterwards takes the
/// freed name must not become the echo area and be overwritten by the next
/// message. Both are measured against GNU Emacs 31.0.90 in
/// `scripts/l215-echo-area-identity-probe.el'.
///
/// A slot is filled by GNU's own `Fget_buffer_create', so a buffer already
/// standing at the canonical name when a slot needs filling DOES become the
/// echo buffer -- that is GNU's behaviour, and it is also what re-attaches
/// these slots to the buffers restored from a portable dump.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EchoAreaBuffers {
    slots: [Option<crate::buffer::BufferId>; 2],
}

impl EchoAreaBuffers {
    const NAMES: [&'static str; 2] = [" *Echo Area 0*", " *Echo Area 1*"];

    /// The buffer the inactive echo area is laid out from.
    ///
    /// GNU chooses between the two slots per call through
    /// `with_echo_area_buffer''s WHICH argument; this port displays and mirrors
    /// through slot 0 only, which is recorded as a divergence rather than
    /// hidden here (ledger 215).
    const fn display_slot(self) -> Option<crate::buffer::BufferId> {
        self.slots[0]
    }
}

/// GNU `set_message_1`'s two decisions about the echo-area buffer, taken
/// together (src/xdisp.c:13588-13615).
///
/// First the buffer's representation:
///
/// ```c
///   if (!message_enable_multibyte
///       && unibyte_display_via_language_environment
///       && !NILP (BVAR (current_buffer, enable_multibyte_characters)))
///     Fset_buffer_multibyte (Qnil);
///   else if (NILP (BVAR (current_buffer, enable_multibyte_characters)))
///     Fset_buffer_multibyte (Qt);
/// ```
///
/// `message_enable_multibyte` is `STRING_MULTIBYTE (string)`, set by
/// `set_message` (src/xdisp.c:13568). The rule is asymmetric on purpose, and
/// GNU says why in its own comment: the echo buffer is ALWAYS made multibyte,
/// and only `unibyte-display-via-language-environment` can make it unibyte,
/// "because in that case unibyte characters should not be displayed as octal
/// escapes". Taking the message string's own multibyteness as the buffer's --
/// what this port did -- is a different rule that consults no variable and
/// turns the echo buffer unibyte for every ASCII message, ASCII string
/// literals being unibyte.
///
/// Then the insert: `insert_from_string (string, 0, 0, SCHARS, SBYTES, true)`
/// (src/xdisp.c:13615), which as GNU's comment there says "takes care of
/// single/multibyte conversion". This port's
/// `replace_buffer_contents_lisp_string` requires the string and the buffer to
/// agree already, so the conversion happens here -- and because the flag and
/// the converted text come out of the same constructor, a mismatch between
/// them is not representable.
struct EchoAreaMessageText {
    /// What `enable-multibyte-characters` must be set to before the insert.
    buffer_is_multibyte: bool,
    /// The message text in that representation, text properties carried.
    text: crate::heap_types::LispString,
}

impl EchoAreaMessageText {
    fn resolve(
        message: &crate::heap_types::LispString,
        buffer_is_multibyte: bool,
        unibyte_display_via_language_environment: bool,
    ) -> Self {
        let buffer_is_multibyte = if !message.is_multibyte()
            && unibyte_display_via_language_environment
            && buffer_is_multibyte
        {
            false
        } else if !buffer_is_multibyte {
            true
        } else {
            // GNU's `if/else if` has no third arm: the buffer is already
            // multibyte and is left alone.
            buffer_is_multibyte
        };
        Self {
            text: Self::convert(message, buffer_is_multibyte),
            buffer_is_multibyte,
        }
    }

    /// GNU `copy_text` as `insert_from_string` reaches it: a unibyte byte >=
    /// 0x80 becomes the corresponding raw-byte character, so the CHARACTER
    /// count is preserved and the string's char-indexed text properties
    /// transfer unchanged.
    fn convert(
        message: &crate::heap_types::LispString,
        buffer_is_multibyte: bool,
    ) -> crate::heap_types::LispString {
        if message.is_multibyte() == buffer_is_multibyte {
            return message.clone();
        }
        let mut converted = if buffer_is_multibyte {
            crate::heap_types::LispString::from_emacs_bytes(
                crate::emacs_core::emacs_char::str_to_multibyte(message.as_bytes()),
            )
        } else {
            crate::heap_types::LispString::from_unibyte(
                crate::emacs_core::emacs_char::str_to_unibyte(message.as_bytes()),
            )
        };
        if message.has_intervals() {
            *converted.intervals_mut() = message.intervals().clone();
        }
        converted
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RedisplaySignature {
    selected_frame: Option<u64>,
    selected_window: Option<u64>,
    current_buffer: Option<u64>,
    current_message: Option<crate::heap_types::LispString>,
    active_minibuffer_window: Option<u64>,
    minibuffer_selected_window: Option<u64>,
    face_change_count: u64,
    obarray_function_epoch: u64,
    redisplay_generation: u64,
    frame: Option<RedisplayFrameSignature>,
}

/// Revision of GNU's frame menu-bar rebuild boundary.
///
/// This is intentionally distinct from the broader redisplay generation:
/// messages and ordinary buffer display changes can require a new frame
/// without satisfying `update_menu_bar`'s `windows_or_buffers_changed ||
/// update_mode_lines` predicate.  Keeping the revision opaque prevents a
/// menu projection from accidentally regressing to that broader cache key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MenuBarRebuildGeneration(u64);

/// Core events that cross GNU `update_menu_bar`'s rebuild boundary.
///
/// Callers cannot invalidate the menu with an unclassified boolean or a
/// generic redisplay revision: adding another owner requires extending this
/// enum and choosing its GNU-equivalent event explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuBarRebuildReason {
    UpdateModeLines,
    WindowsOrBuffersChanged,
    FullFrameRedraw,
}

/// Target of GNU `force-mode-line-update` (`buffer.c`).
///
/// The local form is conditional: an undisplayed current buffer has no mode
/// line to invalidate, so it must not raise the global `update_mode_lines`
/// menu-rebuild predicate.  Encoding the two Lisp call shapes as a sum type
/// keeps that distinction explicit at every Rust call site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModeLineUpdateTarget {
    CurrentBuffer(crate::buffer::BufferId),
    AllBuffers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RedisplayFrameSignature {
    layout: crate::window::FrameLayoutInputState,
    selected_window: u64,
    window_state_change: bool,
    windows: Vec<RedisplayWindowSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RedisplayWindowSignature {
    layout: crate::window::WindowLayoutInputState,
    window_end: crate::window::WindowEndState,
    old_point: LispCharPos1,
    buffer: Option<RedisplayBufferSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RedisplayBufferSignature {
    layout: crate::window::BufferLayoutInputState,
    save_modified_tick: i64,
    autosave_modified_tick: i64,
    point: CharPos0,
    point_emacs_byte: EmacsBytePos,
    last_window_start: LispCharPos1,
    last_selected_window: Option<u64>,
}

/// Authoritative builtin registration entry.  The Lisp-visible static
/// [`SubrObj`] mirrors its directly observable GNU object metadata; the table
/// retains Rust-only dispatch data and the complete interactive spec.
#[derive(Clone, Copy)]
pub(crate) struct SubrEntry {
    pub(crate) function: Option<crate::tagged::header::SubrFn>,
    pub(crate) min_args: u16,
    pub(crate) max_args: Option<u16>,
    pub(crate) dispatch_kind: crate::tagged::header::SubrDispatchKind,
    pub(crate) name_id: crate::emacs_core::intern::NameId,
    pub(crate) interactive_spec: Option<super::interactive::BuiltinInteractiveSpec>,
    pub(crate) portability: super::subr::SubrPortability,
}

thread_local! {
    // Static subrs are encoded directly from `SymId`, so the registry should
    // be indexed by that dense id rather than hashed again at dispatch time.
    static GLOBAL_SUBR_TABLE: RefCell<Vec<Option<SubrEntry>>> = const { RefCell::new(Vec::new()) };

    /// Test-only visibility into hot-path registry reads.  Primitive objects
    /// should carry the GNU `Lisp_Subr` metadata needed by `commandp` instead
    /// of re-entering this table for every M-x candidate.
    #[cfg(test)]
    static GLOBAL_SUBR_LOOKUP_COUNT: Cell<usize> = const { Cell::new(0) };

    /// Test-only observation of GNU bytecode backedge polling cadence.
    #[cfg(test)]
    static BYTECODE_BRANCH_POLL_COUNT: Cell<usize> = const { Cell::new(0) };

    /// Thread-local handle to the active `Context::quit_requested`
    /// atomic. Installed by `Context::setup_thread_locals`, read by
    /// leaf functions (e.g. the regex matcher) that need a cheap quit
    /// check without threading `&mut Context` through their signature.
    /// Mirrors the call site shape of GNU's `maybe_quit()` — reachable
    /// from anywhere without an explicit context pointer.
    static QUIT_REQUESTED_TLS: RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = const { RefCell::new(None) };
}

/// Check whether a quit is pending without needing `&mut Context`.
/// The regex matcher calls this at jump/fail sites, mirroring GNU's
/// `regex-emacs.c:4901,5236`. When it returns `true`, the caller
/// should unwind its work so the next `maybe_quit()` poll can promote
/// the pending flag to a `quit` signal.
pub(crate) fn tls_quit_pending() -> bool {
    QUIT_REQUESTED_TLS.with(|cell| {
        cell.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    })
}

/// Register a subr entry in the global static table.
pub(crate) fn register_global_subr_entry(sym_id: SymId, entry: SubrEntry) {
    GLOBAL_SUBR_TABLE.with(|table| {
        let idx = sym_id.0 as usize;
        let mut table = table.borrow_mut();
        if table.len() <= idx {
            table.resize_with(idx + 1, || None);
        }
        table[idx] = Some(entry);
    });
    crate::tagged::value::update_static_subr_object_entry(
        sym_id,
        entry.function,
        entry.min_args,
        entry.max_args,
        entry.dispatch_kind,
        SubrInteractivity::from(entry.interactive_spec.is_some()),
    );
}

/// Look up a subr entry by SymId.
pub(crate) fn lookup_global_subr_entry(sym_id: SymId) -> Option<SubrEntry> {
    #[cfg(test)]
    GLOBAL_SUBR_LOOKUP_COUNT.with(|count| count.set(count.get() + 1));
    GLOBAL_SUBR_TABLE.with(|table| table.borrow().get(sym_id.0 as usize).copied().flatten())
}

/// Snapshot the Rust primitive ABI compiled into the current runtime.
///
/// Portable runtime images use this to state their minimum consumer contract.
/// Returning entries rather than exposing the thread-local table keeps its
/// dense `SymId` indexing an evaluator implementation detail.
pub(crate) fn registered_global_subr_entries() -> Vec<SubrEntry> {
    GLOBAL_SUBR_TABLE.with(|table| table.borrow().iter().flatten().copied().collect())
}

#[cfg(test)]
pub(crate) fn reset_global_subr_lookup_count() {
    GLOBAL_SUBR_LOOKUP_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn global_subr_lookup_count() -> usize {
    GLOBAL_SUBR_LOOKUP_COUNT.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_bytecode_branch_poll_count() {
    BYTECODE_BRANCH_POLL_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn bytecode_branch_poll_count() -> usize {
    BYTECODE_BRANCH_POLL_COUNT.with(Cell::get)
}

#[inline(always)]
pub(crate) fn subr_entry_from_value(function: Value) -> Option<(SymId, SubrEntry)> {
    let ptr = function.as_veclike_ptr()?;
    let header = unsafe { &*ptr };
    if header.type_tag != VecLikeType::Subr {
        return None;
    }
    let subr = unsafe { &*(ptr as *const SubrObj) };
    if subr.function.is_none() && subr.dispatch_kind == SubrDispatchKind::Builtin {
        return None;
    }
    #[cfg(feature = "vm-profile")]
    crate::emacs_core::bytecode::vm::vm_profile::bump_subr(subr.sym_id);
    Some((
        subr.sym_id,
        SubrEntry {
            function: subr.function,
            min_args: subr.min_args,
            max_args: subr.max_args,
            dispatch_kind: subr.dispatch_kind,
            name_id: subr.name,
            interactive_spec: lookup_global_subr_entry(subr.sym_id)
                .and_then(|entry| entry.interactive_spec),
            portability: lookup_global_subr_entry(subr.sym_id)
                .map_or(super::subr::SubrPortability::AllTargets, |entry| {
                    entry.portability
                }),
        },
    ))
}

/// Access a subr entry by reference (avoids cloning).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn with_global_subr_entry<R>(
    sym_id: SymId,
    f: impl FnOnce(&SubrEntry) -> R,
) -> Option<R> {
    GLOBAL_SUBR_TABLE.with(|table| {
        table
            .borrow()
            .get(sym_id.0 as usize)
            .and_then(|entry| entry.as_ref().map(f))
    })
}

/// Clear all subr entries (used during heap reset).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn clear_global_subr_table() {
    GLOBAL_SUBR_TABLE.with(|table| table.borrow_mut().clear());
}

/// Cached SymId for `internal--compiler-function-overrides`.
///
/// Hot evaluator and bytecode dispatch paths cache whether this variable has a
/// cons value. Keep the SymId cached as well so the mutation paths can refresh
/// that flag without re-interning the string.
fn internal_compiler_function_overrides_sym() -> SymId {
    static SYM: OnceLock<SymId> = OnceLock::new();
    *SYM.get_or_init(|| intern(INTERNAL_COMPILER_FUNCTION_OVERRIDES))
}

#[inline]
fn internal_make_interpreted_closure_function_symbol() -> SymId {
    static SYM: OnceLock<SymId> = OnceLock::new();
    *SYM.get_or_init(|| intern("internal-make-interpreted-closure-function"))
}

pub(crate) fn compiler_function_override_in_obarray(
    obarray: &Obarray,
    sym_id: SymId,
) -> Option<Value> {
    let overrides_sym = internal_compiler_function_overrides_sym();
    let mut cursor = obarray.symbol_value_id_or_nil(overrides_sym);
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if entry.is_cons() && entry.cons_car().as_symbol_id() == Some(sym_id) {
            return Some(entry.cons_cdr());
        }
    }
    None
}

#[derive(Clone, Debug)]
struct ExecutingKbdMacroRuntimeScope {
    snapshot: crate::keyboard::ExecutingKbdMacroRuntimeSnapshot,
    real_this_command: Value,
}

/// Saved symbol-cell value using GNU's `Qunbound` sentinel for absence.
///
/// `Option<Value>` is two words because every `Value` bit pattern is valid.
/// GNU already defines `Qunbound` as the exact old-value marker on the
/// specpdl, so retaining that representation internally is both narrower and
/// more faithful than adding a Rust enum discriminant.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SavedBindingValue(Value);

impl SavedBindingValue {
    #[inline]
    fn from_option(value: Option<Value>) -> Self {
        Self(value.unwrap_or(Value::UNBOUND))
    }

    /// A plain value cell as stored: `Value::UNBOUND` already means unbound.
    #[inline]
    fn from_plain(value: Value) -> Self {
        Self(value)
    }

    /// The cell contents to store back: `Value::UNBOUND` restores "unbound".
    #[inline]
    fn as_plain(self) -> Value {
        self.0
    }

    #[inline]
    pub(crate) fn get(self) -> Option<Value> {
        (!self.0.is_unbound()).then_some(self.0)
    }

    #[inline]
    fn set(&mut self, value: Option<Value>) {
        *self = Self::from_option(value);
    }
}

/// Optional buffer identity with zero reserved for `None`.
///
/// BufferManager allocates IDs monotonically from one. Capturing that
/// invariant in `NonZeroU64` lets Rust use the null niche instead of storing a
/// second word for `Option<BufferId>`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SavedBufferId(Option<std::num::NonZeroU64>);

impl SavedBufferId {
    #[inline]
    fn from_option(buffer_id: Option<crate::buffer::BufferId>) -> Self {
        Self(buffer_id.map(|buffer_id| {
            std::num::NonZeroU64::new(buffer_id.0)
                .expect("live BufferId values are allocated from one")
        }))
    }

    #[inline]
    fn get(self) -> Option<crate::buffer::BufferId> {
        self.0
            .map(|buffer_id| crate::buffer::BufferId(buffer_id.get()))
    }
}

/// A single entry on the specpdl (special binding stack).
/// Matches GNU Emacs's `union specbinding` SPECPDL_LET / SPECPDL_LET_LOCAL.
#[derive(Clone, Debug)]
pub(crate) enum SpecBinding {
    /// Plain dynamic let-binding: saves old obarray (global/default) value.
    Let {
        sym_id: SymId,
        old_value: SavedBindingValue,
    },
    /// Buffer-local let-binding: saves old buffer-local value and which buffer.
    /// On unbind, restores the value in that specific buffer (if still live).
    /// Matches GNU's SPECPDL_LET_LOCAL.
    LetLocal {
        sym_id: SymId,
        old_value: Value,
        buffer_id: crate::buffer::BufferId,
    },
    /// Default-value let-binding for buffer-local variables without a local
    /// binding in the current buffer. Saves/restores the obarray default value.
    /// Matches GNU's SPECPDL_LET_DEFAULT.
    LetDefault {
        sym_id: SymId,
        old_value: SavedBindingValue,
        buffer_id: SavedBufferId,
    },
    /// Lexical environment save/restore. Mirrors GNU's
    /// `specbind(Qinternal_interpreter_environment, ...)` which saves
    /// the current `Vinternal_interpreter_environment` on the specpdl.
    /// `unbind_to` restores `self.lexenv` to this value.
    LexicalEnv { old_lexenv: Value },
    /// Temporary GC root carried on the specpdl itself, mirroring GNU's
    /// use of specpdl-owned runtime state for unwind/helper temporaries.
    GcRoot { value: Value },
    /// Call frame for backtrace. Matches GNU SPECPDL_BACKTRACE.
    /// unbind_to discards these (no-op).
    ///
    /// `args.is_unevalled()` mirrors GNU's
    /// `nargs == UNEVALLED` marker (eval.c:2585 for special forms).
    /// In that shape, the payload is the original cons list of
    /// un-evaluated argument forms. The walker emits
    /// `(nil FUNC FORMS FLAGS)` for these (`backtrace_frame_apply`,
    /// eval.c:3993-3994).
    Backtrace {
        function: Value,
        args: BacktraceArgs,
        debug_on_exit: bool,
    },
    /// Common evaluated one-argument call, stored directly in the specpdl
    /// entry so callback-heavy paths do not clone into the owned side stack.
    Backtrace1 {
        function: Value,
        arg: Value,
        debug_on_exit: bool,
    },
    /// Common evaluated two-argument call. Omitting `debug_on_exit` is a type-
    /// level statement that this compact form is the ordinary non-debug frame;
    /// a future debugger setter must promote it to owned [`Self::Backtrace`]
    /// before enabling exit debugging.
    Backtrace2 {
        function: Value,
        arg0: Value,
        arg1: Value,
    },
    /// Backtrace frame whose arguments live in the JIT caller's native
    /// frame — GNU `specbinding.bt` exactly (`Lisp_Object *args` +
    /// nargs pointing at the caller's stack). Pushed only by
    /// `push_backtrace_frame_from_native_args` for arities the inline
    /// variants can't hold; the args span outlives the entry (the frame
    /// is popped before the native caller's call-args slot dies), and
    /// the stop-the-world root snapshot may read through the pointer.
    /// Like the other inline variants, `debug_on_exit` is structurally
    /// false.
    BacktraceNative {
        function: Value,
        args_ptr: *const i64,
        nargs: u32,
    },
    /// unwind-protect cleanup. Matches GNU SPECPDL_UNWIND.
    /// For interpreter: forms is a cons list, unbind_to calls sf_progn_value.
    /// For VM: forms is a callable (bytecode fn), unbind_to calls apply.
    UnwindProtect { forms: Value, lexenv: Value },
    /// save-excursion state. Matches GNU SPECPDL_UNWIND_EXCURSION.
    SaveExcursion {
        buffer_id: crate::buffer::BufferId,
        marker_id: u64,
        marker: Value,
    },
    /// save-current-buffer state. Matches GNU record_unwind_current_buffer.
    SaveCurrentBuffer { buffer_id: crate::buffer::BufferId },
    /// save-restriction state. Matches GNU SPECPDL_UNWIND with save_restriction_restore.
    SaveRestriction { state: SavedRestrictionUnwind },
    /// Truncate `Context::loads_in_progress` back to `len` on unbind — the
    /// specpdl-carried form of GNU lread.c `Fload`'s
    /// `record_unwind_protect (record_load_unwind, Vloads_in_progress)`.
    /// Carried on the specpdl (not restored imperatively in `load_file_*`)
    /// so EVERY unwind pops it: `Err(Flow)` propagation, condition-case
    /// unwinds, and the panic-containment boundary restores. Truncate (not
    /// pop) keeps it a no-op if a bootstrap reset cleared the stack first.
    LoadsInProgress { len: usize },
    /// Truncate `Context::require_stack` back to `len` on unbind — the
    /// specpdl-carried form of GNU fns.c `Frequire`'s
    /// `record_unwind_protect (require_unwind, require_nesting_list)`.
    /// Same rationale as [`SpecBinding::LoadsInProgress`].
    RequireStack { len: usize },
    /// A typed native-runtime cleanup.  Unlike Lisp `unwind-protect`, each
    /// variant carries exactly the state its cleanup requires and is traced by
    /// the GC while live on the specpdl.
    NativeUnwind { action: NativeUnwindAction },
    /// Placeholder. Matches GNU SPECPDL_NOP.
    Nop,
}

/// Cold, owned payload for a `save-restriction` unwind entry.
///
/// `SavedRestrictionState` contains an optional `Vec` of labeled
/// restrictions. Keeping it inline made that rare payload set the stride of
/// every `SpecBinding`, including the backtrace entry pushed for every Lisp
/// call. The private box forces construction through
/// [`SpecBinding::save_restriction`] while retaining exhaustive typed unwind
/// handling.
#[derive(Clone, Debug)]
pub(crate) struct SavedRestrictionUnwind(Box<crate::buffer::SavedRestrictionState>);

impl SavedRestrictionUnwind {
    fn state(&self) -> &crate::buffer::SavedRestrictionState {
        &self.0
    }

    fn into_state(self) -> crate::buffer::SavedRestrictionState {
        *self.0
    }
}

impl SpecBinding {
    pub(crate) fn save_restriction(state: crate::buffer::SavedRestrictionState) -> Self {
        Self::SaveRestriction {
            state: SavedRestrictionUnwind(Box::new(state)),
        }
    }

    /// The symbol this entry dynamically rebinds, if it is one of GNU's
    /// "subkinds of LET".
    ///
    /// GNU asks the same question as `(--p)->kind >= SPECPDL_LET`
    /// (`src/eval.c:706`), which works only because `SPECPDL_LET`,
    /// `SPECPDL_LET_LOCAL` and `SPECPDL_LET_DEFAULT` are the last three
    /// enumerators and the comment on `src/lisp.h:3564` asks the next person
    /// to keep it that way.  An ordinal comparison is not a property the
    /// compiler checks; an exhaustive match is, so a new binding kind added
    /// below cannot silently answer "not a let-binding" -- it will not
    /// compile until this match says which it is.
    pub(crate) fn let_bound_symbol(&self) -> Option<SymId> {
        match *self {
            Self::Let { sym_id, .. }
            | Self::LetLocal { sym_id, .. }
            | Self::LetDefault { sym_id, .. } => Some(sym_id),
            Self::LexicalEnv { .. }
            | Self::GcRoot { .. }
            | Self::Backtrace { .. }
            | Self::Backtrace1 { .. }
            | Self::Backtrace2 { .. }
            | Self::BacktraceNative { .. }
            | Self::UnwindProtect { .. }
            | Self::SaveExcursion { .. }
            | Self::SaveCurrentBuffer { .. }
            | Self::SaveRestriction { .. }
            | Self::LoadsInProgress { .. }
            | Self::RequireStack { .. }
            | Self::NativeUnwind { .. }
            | Self::Nop => None,
        }
    }
}

/// Native cleanups that must participate in GNU's specpdl unwind ordering.
///
/// Keep this closed and exhaustive: adding a lifecycle that can signal or
/// allocate requires an explicit tracing and execution arm here, rather than
/// an untyped callback whose captures the GC cannot see.
#[derive(Clone, Debug)]
pub(crate) enum NativeUnwindAction {
    RestoreWindowConfiguration {
        configuration: super::builtins::SavedWindowConfiguration,
        options: super::builtins::WindowConfigurationRestoreOptions,
    },
    MinibufferSession {
        state: Box<super::reader::MinibufferSessionUnwind>,
    },
}

impl NativeUnwindAction {
    fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        match self {
            Self::RestoreWindowConfiguration { configuration, .. } => {
                visit(configuration.trace_value())
            }
            Self::MinibufferSession { state } => state.trace_roots(visit),
        }
    }

    fn run(self, context: &mut Context) -> EvalResult {
        // The action has already been popped from the specpdl, so explicitly
        // root its payload while cleanup hooks and window hooks may collect.
        let root_scope = context.save_vm_roots();
        self.trace_roots(&mut |value| context.push_vm_frame_root(value));
        let result = match self {
            Self::RestoreWindowConfiguration {
                configuration,
                options,
            } => configuration.restore(context, options),
            Self::MinibufferSession { state } => {
                super::reader::unwind_minibuffer_session(context, *state)
            }
        };
        context.restore_vm_roots(root_scope);
        result
    }
}

/// Stable handle for updating a typed native unwind before it fires.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeUnwindToken {
    index: usize,
}

/// A live argument range in the bytecode value stack.
///
/// GNU stores a pointer and a count in its four-word backtrace entry.  Neomacs
/// indexes a relocating `Vec<Value>` instead, so the equivalent identity is a
/// `(start, len)` pair.  The checked packed form fits in the payload of
/// [`BacktraceArgs`]; callers that cannot be represented fall back to the
/// owned argument stack, so this type never imposes a semantic limit.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BytecodeBacktraceSpan(usize);

impl BytecodeBacktraceSpan {
    const LEN_BITS: u32 = u16::BITS;
    const LEN_MASK: usize = (1usize << Self::LEN_BITS) - 1;
    const START_BITS: u32 = BacktraceArgs::PAYLOAD_BITS - Self::LEN_BITS;
    const START_MAX: usize = (1usize << Self::START_BITS) - 1;

    #[inline]
    fn try_new(start: usize, len: usize) -> Option<Self> {
        (start <= Self::START_MAX && len <= Self::LEN_MASK)
            .then_some(Self((start << Self::LEN_BITS) | len))
    }

    #[inline]
    fn start(self) -> usize {
        self.0 >> Self::LEN_BITS
    }

    #[inline]
    fn len(self) -> usize {
        self.0 & Self::LEN_MASK
    }
}

/// Decoded view of the one-word backtrace argument descriptor.
#[derive(Clone, Copy, Debug)]
enum BacktraceArgsView {
    Unevalled(Value),
    Evaluated0,
    Evaluated(usize),
    EvaluatedBcStack(BytecodeBacktraceSpan),
}

/// One-word encoding of GNU's `(args, nargs)` backtrace fields.
///
/// Real Lisp values never use tagged-value tag `001` (it is reserved by GNU),
/// so a word with any other tag directly represents an UNEVALLED argument
/// form.  Internal descriptors use `001`, followed by a two-bit kind and a
/// checked payload. Evaluated argument vectors live in
/// `Context::backtrace_args_stack`; bytecode calls instead encode their live
/// caller-stack span directly. Keeping the bit protocol private makes an
/// invalid descriptor unconstructable outside this module.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub(crate) struct BacktraceArgs(usize);

impl BacktraceArgs {
    const DESCRIPTOR_TAG: usize = 0b001;
    const TAG_MASK: usize = 0b111;
    const KIND_SHIFT: u32 = 3;
    const KIND_BITS: u32 = 2;
    const KIND_MASK: usize = (1usize << Self::KIND_BITS) - 1;
    const PAYLOAD_SHIFT: u32 = Self::KIND_SHIFT + Self::KIND_BITS;
    const PAYLOAD_BITS: u32 = usize::BITS - Self::PAYLOAD_SHIFT;
    const PAYLOAD_MAX: usize = usize::MAX >> Self::PAYLOAD_SHIFT;
    const EVALUATED_0_KIND: usize = 0;
    const EVALUATED_KIND: usize = 1;
    const BYTECODE_STACK_KIND: usize = 2;

    #[inline]
    fn unevalled(value: Value) -> Self {
        assert_ne!(
            value.tag(),
            Self::DESCRIPTOR_TAG,
            "real Lisp values cannot use GNU's reserved tag 001"
        );
        Self(value.bits())
    }

    #[inline]
    fn evaluated0() -> Self {
        Self::descriptor(Self::EVALUATED_0_KIND, 0)
    }

    #[inline]
    fn evaluated(index: usize) -> Self {
        assert!(
            index <= Self::PAYLOAD_MAX,
            "a live Vec<LispArgVec> index must fit the descriptor payload"
        );
        Self::descriptor(Self::EVALUATED_KIND, index)
    }

    #[inline]
    fn evaluated_bc_stack(span: BytecodeBacktraceSpan) -> Self {
        Self::descriptor(Self::BYTECODE_STACK_KIND, span.0)
    }

    #[inline]
    fn descriptor(kind: usize, payload: usize) -> Self {
        debug_assert!(kind <= Self::KIND_MASK);
        debug_assert!(payload <= Self::PAYLOAD_MAX);
        Self((payload << Self::PAYLOAD_SHIFT) | (kind << Self::KIND_SHIFT) | Self::DESCRIPTOR_TAG)
    }

    #[inline]
    fn view(self) -> BacktraceArgsView {
        if self.0 & Self::TAG_MASK != Self::DESCRIPTOR_TAG {
            return BacktraceArgsView::Unevalled(Value::from_bits(self.0));
        }
        let kind = (self.0 >> Self::KIND_SHIFT) & Self::KIND_MASK;
        let payload = self.0 >> Self::PAYLOAD_SHIFT;
        match kind {
            Self::EVALUATED_0_KIND => BacktraceArgsView::Evaluated0,
            Self::EVALUATED_KIND => BacktraceArgsView::Evaluated(payload),
            Self::BYTECODE_STACK_KIND => {
                BacktraceArgsView::EvaluatedBcStack(BytecodeBacktraceSpan(payload))
            }
            _ => unreachable!("private backtrace descriptor kind must be valid"),
        }
    }

    #[inline]
    fn owned_index(self) -> Option<usize> {
        let is_descriptor = self.0 & Self::TAG_MASK == Self::DESCRIPTOR_TAG;
        let kind = (self.0 >> Self::KIND_SHIFT) & Self::KIND_MASK;
        (is_descriptor && kind == Self::EVALUATED_KIND).then_some(self.0 >> Self::PAYLOAD_SHIFT)
    }

    #[inline]
    pub(crate) fn is_unevalled(self) -> bool {
        matches!(self.view(), BacktraceArgsView::Unevalled(_))
    }

    #[inline]
    fn is_evaluated(self) -> bool {
        !self.is_unevalled()
    }

    #[inline]
    fn is_bytecode_storage(self) -> bool {
        matches!(
            self.view(),
            BacktraceArgsView::EvaluatedBcStack(_) | BacktraceArgsView::Evaluated(_)
        )
    }
}

impl std::fmt::Debug for BacktraceArgs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.view().fmt(formatter)
    }
}

const _: () = assert!(std::mem::size_of::<BacktraceArgs>() == std::mem::size_of::<usize>());
const _: () = {
    assert!(!std::mem::needs_drop::<Value>());
    assert!(!std::mem::needs_drop::<BacktraceArgs>());
};

/// Proof that one bytecode-call backtrace frame was pushed at `base`.
///
/// The token is deliberately non-`Copy`: consuming it makes a second fast pop
/// impossible through the typed API, without borrowing `Context` across the
/// call or introducing lifetimes into the interpreter driver.
#[must_use = "a pushed bytecode backtrace frame must be consumed by a matching pop"]
#[repr(transparent)]
#[derive(Debug)]
pub(crate) struct BytecodeBacktraceFrame(usize);

impl BytecodeBacktraceFrame {
    /// `Vec` allocations are bounded by `isize::MAX` bytes, so a live
    /// `specpdl` length can never use the high bit. Reserve it to tell the
    /// return path that the packed bytecode span overflowed and owns a cold
    /// `backtrace_args_stack` slot. The overwhelmingly common token remains
    /// exactly the raw base and therefore needs no decode before `set_len`.
    const OWNED_ARGS_FLAG: usize = 1usize << (usize::BITS - 1);
    const BASE_MASK: usize = !Self::OWNED_ARGS_FLAG;

    #[inline]
    fn new(base: usize, owns_args: bool) -> Self {
        debug_assert_eq!(
            base & Self::OWNED_ARGS_FLAG,
            0,
            "a Vec length cannot occupy the bytecode-frame ownership bit"
        );
        Self(base | usize::from(owns_args) * Self::OWNED_ARGS_FLAG)
    }

    #[inline]
    fn base(&self) -> usize {
        self.0 & Self::BASE_MASK
    }

    #[cfg(test)]
    pub(crate) fn base_for_test(&self) -> usize {
        self.base()
    }

    #[cfg(test)]
    pub(crate) fn word_for_test(&self) -> usize {
        self.0
    }
}

const _: () =
    assert!(std::mem::size_of::<BytecodeBacktraceFrame>() == std::mem::size_of::<usize>());

/// What [`Context::pop_fast_bytecode_backtrace_frame`] did.
///
/// GNU's `Breturn` cannot be a bare `specpdl_ptr--` for a frame carrying
/// `debug_on_exit`: the exit debugger's return value REPLACES the call's
/// (`src/bytecode.c:825-828`).  Handing the token back rather than returning a
/// bare `bool` is what makes the refusal actionable -- a caller cannot pop the
/// frame some other way without a token, and it cannot drop the token without
/// tripping `BytecodeBacktraceFrame`'s own `#[must_use]`.
#[must_use = "a refused fast pop leaves the frame on the specpdl owing a debugger entry"]
pub(crate) enum FastBytecodePop {
    /// The frame is gone: GNU's `specpdl_ptr--`.
    Popped,
    /// The frame owes `call_debugger (list2 (Qexit, val))` and is still on the
    /// specpdl.  Spend it with
    /// [`Context::pop_bytecode_backtrace_token_with_result`].
    OwesDebugOnExit(BytecodeBacktraceFrame),
}

#[derive(Clone, Debug)]
struct ThreadDynamicBindingState {
    lexenv: Value,
    specpdl: Vec<SpecBinding>,
    condition_stack: Vec<ConditionFrame>,
}

#[derive(Debug)]
pub(crate) struct ThreadDynamicBindingToken {
    suspended_depth: usize,
}

/// Copy-only state needed before discarding a trivially-unbound specpdl entry.
///
/// This is intentionally a separate closed enum: the fast pop below cannot
/// accidentally admit a new `SpecBinding` variant with an owned Rust payload.
#[derive(Clone, Copy)]
enum TrivialSpecBindingPop {
    NoOwnedArgs,
    BacktraceArgs(BacktraceArgs),
}

#[inline]
fn trivial_spec_binding_pop(binding: &SpecBinding) -> Option<TrivialSpecBindingPop> {
    match binding {
        SpecBinding::GcRoot { .. }
        | SpecBinding::Nop
        | SpecBinding::Backtrace1 {
            debug_on_exit: false,
            ..
        }
        | SpecBinding::Backtrace2 { .. }
        | SpecBinding::BacktraceNative { .. } => Some(TrivialSpecBindingPop::NoOwnedArgs),
        SpecBinding::Backtrace {
            args,
            debug_on_exit: false,
            ..
        } => Some(TrivialSpecBindingPop::BacktraceArgs(*args)),
        _ => None,
    }
}

#[inline]
fn spec_binding_has_trivial_unbind(binding: &SpecBinding) -> bool {
    trivial_spec_binding_pop(binding).is_some()
}

const _: () = assert!(!std::mem::needs_drop::<TrivialSpecBindingPop>());

#[derive(Clone, Debug, Default)]
pub(crate) struct VmRootFrame {
    pub(crate) roots: LispArgVec,
}

impl VmRootFrame {
    fn new() -> Self {
        Self {
            roots: LispArgVec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PendingSafeFuncall {
    pub(crate) function: Value,
    pub(crate) args: LispArgVec,
}

pub(crate) type LispArgVec = SmallVec<[Value; 8]>;
type LetBindingVec = SmallVec<[(SymId, Value); 8]>;

// `BacktraceArgs::evaluated` stores a Vec index in its descriptor payload.
// Rust cannot allocate enough non-zero-sized entries for a valid index to
// exceed that payload, on either 32- or 64-bit targets.
const _: () =
    assert!(isize::MAX as usize / std::mem::size_of::<LispArgVec>() <= BacktraceArgs::PAYLOAD_MAX);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GnuTimerTimestamp {
    pub(crate) high_seconds: i64,
    pub(crate) low_seconds: i64,
    pub(crate) usecs: i64,
    pub(crate) psecs: i64,
}

impl GnuTimerTimestamp {
    pub(crate) fn now() -> Self {
        let (secs, usecs) = match crate::host::time::wall_time_since_unix_epoch() {
            Ok(dur) => (dur.as_secs() as i64, dur.subsec_micros() as i64),
            Err(err) => {
                let dur = err.duration();
                (-(dur.as_secs() as i64), -(dur.subsec_micros() as i64))
            }
        };

        Self {
            high_seconds: secs >> 16,
            low_seconds: secs & 0xFFFF,
            usecs,
            psecs: 0,
        }
    }

    fn unix_seconds(self) -> i64 {
        (self.high_seconds << 16) + self.low_seconds
    }

    pub(crate) fn duration_until(self, now: Self) -> std::time::Duration {
        use std::time::Duration;

        if self <= now {
            return Duration::ZERO;
        }

        let mut secs = self.unix_seconds() - now.unix_seconds();
        let mut usecs = self.usecs - now.usecs;
        let mut psecs = self.psecs - now.psecs;

        if psecs < 0 {
            psecs += 1_000_000;
            usecs -= 1;
        }
        if usecs < 0 {
            usecs += 1_000_000;
            secs -= 1;
        }
        if secs < 0 {
            return Duration::ZERO;
        }

        let mut secs = secs as u64;
        let mut nanos = (usecs as u32) * 1_000 + (psecs.max(0) as u32).div_ceil(1_000);
        if nanos >= 1_000_000_000 {
            secs += 1;
            nanos -= 1_000_000_000;
        }

        Duration::new(secs, nanos)
    }

    pub(crate) fn overdue_duration(self, now: Self) -> std::time::Duration {
        use std::time::Duration;

        if self >= now {
            return Duration::ZERO;
        }

        let mut secs = now.unix_seconds() - self.unix_seconds();
        let mut usecs = now.usecs - self.usecs;
        let mut psecs = now.psecs - self.psecs;

        if psecs < 0 {
            psecs += 1_000_000;
            usecs -= 1;
        }
        if usecs < 0 {
            usecs += 1_000_000;
            secs -= 1;
        }

        let nanos = ((usecs as u32) * 1_000) + (psecs as u32 / 1_000);
        Duration::new(secs as u64, nanos)
    }

    pub(crate) fn from_duration(duration: std::time::Duration) -> Self {
        let secs = duration.as_secs() as i64;
        let usecs = duration.subsec_micros() as i64;
        Self {
            high_seconds: secs >> 16,
            low_seconds: secs & 0xFFFF,
            usecs,
            psecs: 0,
        }
    }

    pub(crate) fn add_duration(self, duration: std::time::Duration) -> Self {
        let mut secs = self.unix_seconds() + duration.as_secs() as i64;
        let mut usecs = self.usecs + duration.subsec_micros() as i64;
        let psecs = self.psecs;

        if usecs >= 1_000_000 {
            secs += usecs / 1_000_000;
            usecs %= 1_000_000;
        }

        Self {
            high_seconds: secs >> 16,
            low_seconds: secs & 0xFFFF,
            usecs,
            psecs,
        }
    }
}

#[derive(Clone, Debug)]
enum NamedCallTarget {
    Obarray(Value),
    Subr(Value),
    Void,
}

/// Continuation after loading one hop of a named autoload.
///
/// GNU `funcall_general` never applies the value returned by
/// `autoload-do-load` blindly.  It retries resolution through the original
/// symbol, because the loaded file may have installed another autoload.  Keep
/// that distinction in the type system so an autoload form cannot accidentally
/// flow into ordinary function-value dispatch.
#[derive(Clone, Copy, Debug)]
enum NamedAutoloadCallStep {
    RetrySymbol { autoload_form: Value },
    DispatchFunction { function: Value },
    Void,
}

#[derive(Clone, Debug)]
struct NamedCallCacheEntry {
    function_epoch: u64,
    target: NamedCallTarget,
}

#[derive(Clone, Copy, Debug)]
struct LexenvAssqCacheEntry {
    lexenv_bits: usize,
    symbol: SymId,
    cell: Value,
}

struct LexenvAssqCache {
    entries: [Cell<Option<LexenvAssqCacheEntry>>; LEXENV_ASSQ_CACHE_CAPACITY],
}

impl Default for LexenvAssqCache {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| Cell::new(None)),
        }
    }
}

impl LexenvAssqCache {
    #[inline]
    fn slot(lexenv_bits: usize, sym_id: SymId) -> usize {
        let mixed = lexenv_bits.rotate_left(7) ^ (sym_id.0 as usize).wrapping_mul(0x9E37_79B1);
        mixed & (LEXENV_ASSQ_CACHE_CAPACITY - 1)
    }

    #[inline]
    fn find(&self, lexenv_bits: usize, sym_id: SymId) -> Option<Value> {
        let entry = self.entries[Self::slot(lexenv_bits, sym_id)].get()?;
        (entry.lexenv_bits == lexenv_bits && entry.symbol == sym_id).then_some(entry.cell)
    }

    #[inline]
    fn push(&self, entry: LexenvAssqCacheEntry) {
        let index = Self::slot(entry.lexenv_bits, entry.symbol);
        self.entries[index].set(Some(entry));
    }

    #[inline]
    fn clear(&self) {
        for entry in &self.entries {
            entry.set(None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LexenvSpecialCacheEntry {
    lexenv_bits: usize,
    symbol: SymId,
    declared_special: bool,
}

struct LexenvSpecialCache {
    entries: [Cell<Option<LexenvSpecialCacheEntry>>; LEXENV_SPECIAL_CACHE_CAPACITY],
}

impl Default for LexenvSpecialCache {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| Cell::new(None)),
        }
    }
}

impl LexenvSpecialCache {
    #[inline]
    fn slot(lexenv_bits: usize, sym_id: SymId) -> usize {
        let mixed = lexenv_bits.rotate_left(7) ^ (sym_id.0 as usize).wrapping_mul(0x9E37_79B1);
        mixed & (LEXENV_SPECIAL_CACHE_CAPACITY - 1)
    }

    #[inline]
    fn find(&self, lexenv_bits: usize, sym_id: SymId) -> Option<bool> {
        let entry = self.entries[Self::slot(lexenv_bits, sym_id)].get()?;
        (entry.lexenv_bits == lexenv_bits && entry.symbol == sym_id)
            .then_some(entry.declared_special)
    }

    #[inline]
    fn push(&self, entry: LexenvSpecialCacheEntry) {
        let index = Self::slot(entry.lexenv_bits, entry.symbol);
        self.entries[index].set(Some(entry));
    }

    #[inline]
    fn clear(&self) {
        for entry in &self.entries {
            entry.set(None);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct MacroPerfCounter {
    calls: u64,
    total_us: u64,
    max_us: u64,
}

impl MacroPerfCounter {
    fn note_duration(&mut self, duration: std::time::Duration) {
        let elapsed_us = duration.as_micros() as u64;
        self.calls = self.calls.saturating_add(1);
        self.total_us = self.total_us.saturating_add(elapsed_us);
        self.max_us = self.max_us.max(elapsed_us);
    }

    fn summary(&self, label: &str) -> Option<String> {
        if self.calls == 0 {
            return None;
        }
        let avg_us = self.total_us / self.calls.max(1);
        Some(format!(
            "{label}=count:{} total:{:.2}ms avg:{:.3}ms max:{:.3}ms",
            self.calls,
            self.total_us as f64 / 1000.0,
            avg_us as f64 / 1000.0,
            self.max_us as f64 / 1000.0
        ))
    }
}

#[derive(Clone, Debug, Default)]
struct MacroPerfStats {
    scope_enter: MacroPerfCounter,
    scope_exit: MacroPerfCounter,
    macro_apply: MacroPerfCounter,
    expand_macro: MacroPerfCounter,
    eager_step1: MacroPerfCounter,
    eager_step3: MacroPerfCounter,
    eager_step4: MacroPerfCounter,
}

fn value_from_symbol_id(sym_id: SymId) -> Value {
    if is_canonical_id(sym_id) {
        if sym_id == nil_symbol() {
            return Value::NIL;
        }
        if sym_id == t_symbol() {
            return Value::T;
        }
        if is_keyword_id(sym_id) {
            return Value::from_kw_id(sym_id);
        }
    }
    Value::from_sym_id(sym_id)
}

fn hidden_internal_interpreter_environment_symbol() -> SymId {
    static HIDDEN_SYMBOL: OnceLock<SymId> = OnceLock::new();
    *HIDDEN_SYMBOL.get_or_init(|| intern_uninterned("internal-interpreter-environment"))
}

fn hidden_load_read_stream_token() -> LoadReadStreamToken {
    static HIDDEN_SYMBOL: OnceLock<LoadReadStreamToken> = OnceLock::new();
    *HIDDEN_SYMBOL.get_or_init(|| LoadReadStreamToken(intern_uninterned("get-file-char")))
}

fn default_directory_symbol() -> SymId {
    static SYMBOL: OnceLock<SymId> = OnceLock::new();
    *SYMBOL.get_or_init(|| intern("default-directory"))
}

fn lexical_binding_symbol() -> SymId {
    static SYMBOL: OnceLock<SymId> = OnceLock::new();
    *SYMBOL.get_or_init(|| intern("lexical-binding"))
}

fn nil_symbol() -> SymId {
    static SYMBOL: OnceLock<SymId> = OnceLock::new();
    *SYMBOL.get_or_init(|| intern("nil"))
}

fn t_symbol() -> SymId {
    static SYMBOL: OnceLock<SymId> = OnceLock::new();
    *SYMBOL.get_or_init(|| intern("t"))
}

fn buffer_undo_list_symbol() -> SymId {
    static SYMBOL: OnceLock<SymId> = OnceLock::new();
    *SYMBOL.get_or_init(|| intern("buffer-undo-list"))
}

fn macroexp_dynvars_symbol() -> SymId {
    static SYMBOL: OnceLock<SymId> = OnceLock::new();
    *SYMBOL.get_or_init(|| intern("macroexp--dynvars"))
}

macro_rules! cached_symbol_id {
    ($fn_name:ident, $name:literal) => {
        #[inline(always)]
        fn $fn_name() -> SymId {
            static SYMBOL: OnceLock<SymId> = OnceLock::new();
            if let Some(id) = SYMBOL.get() {
                *id
            } else {
                *SYMBOL.get_or_init(|| intern($name))
            }
        }
    };
}

cached_symbol_id!(quote_symbol, "quote");
cached_symbol_id!(function_symbol, "function");
cached_symbol_id!(let_symbol, "let");
cached_symbol_id!(let_star_symbol, "let*");
cached_symbol_id!(setq_symbol, "setq");
cached_symbol_id!(if_symbol, "if");
cached_symbol_id!(while_symbol, "while");
cached_symbol_id!(prog1_symbol, "prog1");
cached_symbol_id!(defvar_symbol, "defvar");
cached_symbol_id!(defconst_symbol, "defconst");
cached_symbol_id!(catch_symbol, "catch");
cached_symbol_id!(unwind_protect_symbol, "unwind-protect");
cached_symbol_id!(condition_case_symbol, "condition-case");
cached_symbol_id!(interactive_symbol_id, "interactive");
cached_symbol_id!(lambda_symbol, "lambda");
cached_symbol_id!(closure_symbol, "closure");
cached_symbol_id!(declare_symbol, "declare");
cached_symbol_id!(macro_symbol, "macro");
cached_symbol_id!(max_lisp_eval_depth_symbol, "max-lisp-eval-depth");
cached_symbol_id!(byte_code_literal_symbol, "byte-code-literal");
cached_symbol_id!(byte_code_symbol, "byte-code");
cached_symbol_id!(input_decode_map_symbol, "input-decode-map");
cached_symbol_id!(local_function_key_map_symbol, "local-function-key-map");
cached_symbol_id!(post_gc_hook_symbol, "post-gc-hook");
cached_symbol_id!(echo_area_clear_hook_symbol, "echo-area-clear-hook");
cached_symbol_id!(gc_elapsed_symbol, "gc-elapsed");
cached_symbol_id!(gcs_done_symbol, "gcs-done");
cached_symbol_id!(error_symbol, "error");
cached_symbol_id!(quit_symbol, "quit");
cached_symbol_id!(invalid_function_symbol, "invalid-function");
cached_symbol_id!(error_conditions_symbol, "error-conditions");

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn is_lambda_like_symbol_id(id: SymId) -> bool {
    id == lambda_symbol() || id == closure_symbol()
}

fn cons_head_symbol_id(value: &Value) -> Option<SymId> {
    if value.is_cons() {
        let car = value.cons_car();
        // Try bare symbol first, then transparently unwrap symbol-with-pos.
        car.as_symbol_id().or_else(|| {
            car.as_symbol_with_pos_sym()
                .and_then(|sym| sym.as_symbol_id())
        })
    } else {
        None
    }
}

struct CoreEvalSymbols {
    internal_interpreter_environment_symbol: SymId,
    load_read_stream_token: LoadReadStreamToken,
    compiler_function_overrides_symbol: SymId,
    quit_flag_symbol: SymId,
    inhibit_quit_symbol: SymId,
    throw_on_input_symbol: SymId,
    kill_emacs_symbol: SymId,
    noninteractive_symbol: SymId,
    symbols_with_pos_enabled_symbol: SymId,
    print_symbols_bare_symbol: SymId,
}

fn install_core_eval_symbols(obarray: &mut Obarray, reset_runtime_values: bool) -> CoreEvalSymbols {
    obarray.intern("internal-interpreter-environment");
    let internal_interpreter_environment_symbol = hidden_internal_interpreter_environment_symbol();
    obarray.set_symbol_value_id(internal_interpreter_environment_symbol, Value::NIL);
    obarray.make_special_id(internal_interpreter_environment_symbol);
    let load_read_stream_token = hidden_load_read_stream_token();

    let compiler_function_overrides_symbol = internal_compiler_function_overrides_sym();

    let quit_flag_symbol = intern("quit-flag");
    if reset_runtime_values {
        obarray.set_symbol_value_id(quit_flag_symbol, Value::NIL);
    }
    obarray.make_special_id(quit_flag_symbol);

    let inhibit_quit_symbol = intern("inhibit-quit");
    if reset_runtime_values {
        obarray.set_symbol_value_id(inhibit_quit_symbol, Value::NIL);
    }
    obarray.make_special_id(inhibit_quit_symbol);

    let throw_on_input_symbol = intern("throw-on-input");
    if reset_runtime_values {
        obarray.set_symbol_value_id(throw_on_input_symbol, Value::NIL);
    }
    obarray.make_special_id(throw_on_input_symbol);

    let kill_emacs_symbol = intern("kill-emacs");
    let noninteractive_symbol = intern("noninteractive");
    let symbols_with_pos_enabled_symbol = intern("symbols-with-pos-enabled");
    let print_symbols_bare_symbol = intern("print-symbols-bare");

    CoreEvalSymbols {
        internal_interpreter_environment_symbol,
        load_read_stream_token,
        compiler_function_overrides_symbol,
        quit_flag_symbol,
        inhibit_quit_symbol,
        throw_on_input_symbol,
        kill_emacs_symbol,
        noninteractive_symbol,
        symbols_with_pos_enabled_symbol,
        print_symbols_bare_symbol,
    }
}

fn is_runtime_dynamically_special(obarray: &Obarray, sym_id: SymId) -> bool {
    obarray.is_special_id(sym_id) && !obarray.is_constant_id(sym_id)
}

/// The name `(let ((SYM VALUE)) ...)` must report as `(setting-constant SYM)`,
/// or `None` when the binding is one GNU performs.
///
/// GNU's `let`/`let*` have no constant check of their own: `Flet`/`Flet_star`
/// just `specbind`, and the refusal comes from `do_specbind`
/// (`src/eval.c:3597-3604`) handing a trapped-write symbol to `set_internal`,
/// whose `SYMBOL_NOWRITE` arm (`src/data.c:1687-1697`) lets a KEYWORD be
/// re-bound to the value it already has.  `(let ((:text :text)) ...)` is
/// therefore legal in GNU while `(let ((:text 5)) ...)` is not — and dash's
/// `-let` plist destructuring emits exactly the legal shape, binding `:text`
/// to the `:text` it just popped off the plist.
fn let_constant_error_name(obarray: &Obarray, sym_id: SymId, value: Value) -> Option<String> {
    match obarray.classify_constant_write(sym_id, value) {
        ConstantWrite::Writable | ConstantWrite::KeywordSelfAssign => None,
        ConstantWrite::Refused => Some(resolve_sym(sym_id).to_owned()),
    }
}

pub(crate) fn sync_features_variable_in_state(obarray: &mut Obarray, features: &[SymId]) {
    let values: Vec<Value> = features.iter().map(|id| Value::from_sym_id(*id)).collect();
    obarray.set_symbol_value("features", Value::list(values));
}

pub(crate) fn refresh_features_from_variable_in_state(
    obarray: &Obarray,
    features: &mut Vec<SymId>,
) {
    let current = obarray
        .symbol_value("features")
        .cloned()
        .unwrap_or(Value::NIL);
    let mut parsed = Vec::new();
    if let Some(items) = list_to_vec(&current) {
        for item in items {
            if let Some(id) = item.as_symbol_id() {
                parsed.push(id);
            }
        }
    }
    *features = parsed;
}

pub(crate) fn feature_present_in_state(
    obarray: &Obarray,
    features: &mut Vec<SymId>,
    name: &str,
) -> bool {
    refresh_features_from_variable_in_state(obarray, features);
    let id = intern(name);
    features.contains(&id)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn add_feature_in_state(obarray: &mut Obarray, features: &mut Vec<SymId>, name: &str) {
    refresh_features_from_variable_in_state(obarray, features);
    let id = intern(name);
    add_feature_id_in_state(obarray, features, id);
}

pub(crate) fn add_feature_id_in_state(obarray: &mut Obarray, features: &mut Vec<SymId>, id: SymId) {
    refresh_features_from_variable_in_state(obarray, features);
    if features.contains(&id) {
        return;
    }
    let current = obarray
        .symbol_value("features")
        .cloned()
        .unwrap_or(Value::NIL);
    // Emacs pushes newly-provided features at the front.
    features.insert(0, id);
    obarray.set_symbol_value("features", Value::cons(Value::from_sym_id(id), current));
}

pub(crate) fn remove_feature_in_state(
    obarray: &mut Obarray,
    features: &mut Vec<SymId>,
    name: &str,
) {
    refresh_features_from_variable_in_state(obarray, features);
    let id = intern(name);
    features.retain(|feature| *feature != id);
    sync_features_variable_in_state(obarray, features);
}

pub(crate) fn provide_value_in_state(
    obarray: &mut Obarray,
    features: &mut Vec<SymId>,
    feature: Value,
    subfeatures: Option<Value>,
) -> EvalResult {
    // Use symbol_id to transparently handle symbol-with-pos wrappers.
    let sym_id = super::builtins::symbols::symbol_id(&feature).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), feature],
        )
    })?;
    if let Some(value) = subfeatures {
        if crate::emacs_core::value::list_to_vec(&value).is_none() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), value],
            ));
        }
        if value.is_nil() {
            add_feature_id_in_state(obarray, features, sym_id);
            return Ok(feature);
        }
        obarray.put_property_id(sym_id, intern("subfeatures"), value)?;
    }
    add_feature_id_in_state(obarray, features, sym_id);
    Ok(feature)
}

/// Limit for stored recent input events to match GNU Emacs: 300 entries.
pub(crate) const RECENT_INPUT_EVENT_LIMIT: usize = 300;

thread_local! {
    static SCRATCH_GC_ROOTS: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// Collect GC roots from runtime-global side tables that hold Values.
///
/// These side tables are invisible to the normal GC root scan (which only
/// walks the Evaluator struct and its sub-managers).  This function calls each
/// module's `collect_*_gc_roots` helper to ensure those Values are marked as
/// live during garbage collection.
fn collect_thread_local_gc_roots(
    roots: &mut Vec<(Value, &'static str)>,
    heap_id: usize,
    stats: &mut Vec<crate::tagged::gc::RootGroup>,
) {
    fn collect_group(
        roots: &mut Vec<(Value, &'static str)>,
        origin: &'static str,
        stats: &mut Vec<crate::tagged::gc::RootGroup>,
        collect: impl FnOnce(&mut Vec<Value>),
    ) {
        // GC handshake instrumentation: per-side-table build cost + volume
        // (the JIT reloc walk in particular scales with the COMPILED cache).
        let t0 = crate::host::time::Instant::now();
        let mut group = Vec::new();
        collect(&mut group);
        stats.push((origin, t0.elapsed().as_micros() as u64, group.len()));
        roots.extend(group.into_iter().map(|root| (root, origin)));
    }

    // R1a: heap-object constants loaded by JIT-compiled leaves through their reloc
    // vectors — generated code holds only indices, so these must be rooted here.
    #[cfg(feature = "jit")]
    collect_group(
        roots,
        "jit-reloc-thread-local",
        stats,
        super::jit::cache::collect_jit_reloc_gc_roots,
    );
    // A signal, throw or thread-yield that is unwinding lives only in a Rust
    // `Flow`, which the precise collector cannot see; each variant's payload is
    // pinned by its own private root handle and seeded here (DIVERGENCES.md
    // 161 for the signal, 162 for the throw and the thread-yield).
    collect_group(
        roots,
        "in-flight-flow-thread-local",
        stats,
        super::error::collect_in_flight_flow_gc_roots,
    );
    collect_group(
        roots,
        "syntax-thread-local",
        stats,
        super::syntax::collect_syntax_gc_roots,
    );
    collect_group(
        roots,
        "casetab-thread-local",
        stats,
        super::casetab::collect_casetab_gc_roots,
    );
    collect_group(
        roots,
        "category-thread-local",
        stats,
        super::category::collect_category_gc_roots,
    );
    collect_group(
        roots,
        "terminal-thread-local",
        stats,
        super::terminal::pure::collect_terminal_gc_roots,
    );
    collect_group(
        roots,
        "font-thread-local",
        stats,
        super::xfaces::collect_font_gc_roots,
    );
    collect_group(
        roots,
        "charset-thread-local",
        stats,
        super::charset::collect_charset_gc_roots,
    );
    collect_group(
        roots,
        "ccl-thread-local",
        stats,
        super::ccl::collect_ccl_gc_roots,
    );
    collect_group(
        roots,
        "dynamic-module-thread-local",
        stats,
        super::dynamic_module::collect_dynamic_module_gc_roots,
    );
    collect_group(
        roots,
        "hash-table-test-thread-local",
        stats,
        super::builtins::collections::collect_hash_table_test_alias_gc_roots,
    );
    collect_group(
        roots,
        "file-notify-thread-local",
        stats,
        super::builtins::collect_file_notify_gc_roots,
    );
    collect_group(roots, "symbol-name-thread-local", stats, |group| {
        super::intern::collect_symbol_name_gc_roots(group, heap_id)
    });
    let scratch_t0 = crate::host::time::Instant::now();
    let mut scratch_count = 0usize;
    SCRATCH_GC_ROOTS.with(|scratch| {
        let scratch = scratch.borrow();
        scratch_count = scratch.len();
        roots.extend(
            scratch
                .iter()
                .copied()
                .map(|root| (root, "scratch-thread-local")),
        )
    });
    stats.push((
        "scratch-thread-local",
        scratch_t0.elapsed().as_micros() as u64,
        scratch_count,
    ));
}

pub fn save_scratch_gc_roots() -> usize {
    SCRATCH_GC_ROOTS.with(|scratch| scratch.borrow().len())
}

pub fn push_scratch_gc_root(value: Value) {
    SCRATCH_GC_ROOTS.with(|scratch| scratch.borrow_mut().push(value));
}

/// Root every value in `values` with ONE thread-local access (a list
/// builder rooted its elements one push each: ~30 Ir per element).
pub fn push_scratch_gc_roots(values: &[Value]) {
    SCRATCH_GC_ROOTS.with(|scratch| scratch.borrow_mut().extend_from_slice(values));
}

/// Push one root slot and return its index; `set_scratch_gc_root` re-points
/// it in place so an accumulator can stay rooted across a build loop without
/// a push per step.
pub fn push_scratch_gc_root_slot(value: Value) -> usize {
    SCRATCH_GC_ROOTS.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        scratch.push(value);
        scratch.len() - 1
    })
}

pub fn set_scratch_gc_root(slot: usize, value: Value) {
    SCRATCH_GC_ROOTS.with(|scratch| scratch.borrow_mut()[slot] = value);
}

pub fn restore_scratch_gc_roots(saved_len: usize) {
    SCRATCH_GC_ROOTS.with(|scratch| scratch.borrow_mut().truncate(saved_len));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuiFrameHostRequest {
    pub frame_id: crate::window::FrameId,
    pub width: u32,
    pub height: u32,
    pub title: crate::heap_types::LispString,
    pub geometry_hints: crate::window::GuiFrameGeometryHints,
    pub fullscreen: Option<FrameFullscreen>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuiFrameHostSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontSpecResolveRequest {
    pub frame_id: crate::window::FrameId,
    pub family: Option<crate::heap_types::LispString>,
    pub registry: Option<crate::heap_types::LispString>,
    pub lang: Option<crate::heap_types::LispString>,
    pub weight: Option<FontWeight>,
    pub slant: Option<FontSlant>,
    pub width: Option<FontWidth>,
}

/// One exact opened font shared by layout, frame geometry, and Lisp font
/// objects.  The protocol realization is the canonical identity: in
/// particular, native selectors and variable coordinates must survive this
/// boundary instead of being flattened to a file plus collection index.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedOpenedFont {
    pub resolved: neomacs_display_protocol::font::ResolvedFont,
    pub foundry: Option<crate::heap_types::LispString>,
    /// Emacs selector slant, which retains reverse slants that the render
    /// protocol intentionally normalizes to its three rasterizer slants.
    pub slant: FontSlant,
    /// Metrics captured from the exact opened font at realization time.
    pub metrics: FontPxProbeResult,
    /// Capability captured with the opened object; `query-font` must not
    /// reopen a file or depend on the currently selected frame.
    pub capability: Option<FontOtfCapability>,
}

impl ResolvedOpenedFont {
    pub fn weight(&self) -> FontWeight {
        FontWeight::from_css_weight(self.resolved.weight)
    }

    pub fn width(&self) -> FontWidth {
        match self.resolved.width {
            1 => FontWidth::UltraCondensed,
            2 => FontWidth::ExtraCondensed,
            3 => FontWidth::Condensed,
            4 => FontWidth::SemiCondensed,
            6 => FontWidth::SemiExpanded,
            7 => FontWidth::Expanded,
            8 => FontWidth::ExtraExpanded,
            9 => FontWidth::UltraExpanded,
            _ => FontWidth::Normal,
        }
    }

    /// Frame geometry comes from the same exact opened metrics as
    /// `query-font`; it must never run a second family-based selection.
    pub fn font_size_px(&self) -> f32 {
        self.metrics.pixel_size.max(1) as f32
    }

    pub fn char_width(&self) -> f32 {
        self.metrics.average_width.max(1) as f32
    }

    pub fn line_height(&self) -> f32 {
        self.metrics.height.max(1) as f32
    }
}

#[cfg(test)]
pub(crate) fn test_resolved_opened_font(
    family: &str,
    foundry: Option<&str>,
    file: Option<&str>,
    weight: FontWeight,
    slant: FontSlant,
    width: FontWidth,
    postscript_name: Option<&str>,
    metrics: FontPxProbeResult,
    capability: Option<FontOtfCapability>,
) -> ResolvedOpenedFont {
    use neomacs_display_protocol::font::{
        FontBackendKind, FontFileAsset, FontMemoryAsset, FontOutlineAsset, FontReplay,
        FontResolutionSource, FontSlantKind, ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
    };
    use std::sync::Arc;

    let identity = match file {
        Some(file) => ResolvedFontIdentity::from_file(file, 0, postscript_name.map(str::to_owned)),
        None => ResolvedFontIdentity::from_memory(
            FontBackendKind::Fontconfig,
            format!("test:{family}"),
            0,
            postscript_name.map(str::to_owned),
        ),
    };
    let replay = match file {
        Some(file) => FontReplay::Swash {
            asset: FontOutlineAsset::File(
                FontFileAsset::new(file, 0).expect("non-empty test font path"),
            ),
        },
        None => FontReplay::Swash {
            asset: FontOutlineAsset::Memory(
                FontMemoryAsset::new(format!("test:{family}"), Arc::new(vec![0, 1, 0, 0]), 0)
                    .expect("non-empty test memory font"),
            ),
        },
    };
    let slant_kind = match slant {
        FontSlant::Normal => FontSlantKind::Normal,
        FontSlant::Italic | FontSlant::ReverseItalic => FontSlantKind::Italic,
        FontSlant::Oblique | FontSlant::ReverseOblique => FontSlantKind::Oblique,
    };
    let width_class = match width {
        FontWidth::UltraCondensed => 1,
        FontWidth::ExtraCondensed => 2,
        FontWidth::Condensed => 3,
        FontWidth::SemiCondensed => 4,
        FontWidth::Normal => 5,
        FontWidth::SemiExpanded => 6,
        FontWidth::Expanded => 7,
        FontWidth::ExtraExpanded => 8,
        FontWidth::UltraExpanded => 9,
    };
    ResolvedOpenedFont {
        resolved: ResolvedFont {
            id: ResolvedFontId(0),
            identity,
            replay,
            family: family.to_owned(),
            full_name: None,
            postscript_name: postscript_name.map(str::to_owned),
            weight: weight.css_weight(),
            slant: slant_kind,
            width: width_class,
            pixel_size: metrics.pixel_size as f32,
            ascent_px: metrics.ascent as f32,
            descent_px: metrics.descent as f32,
            space_advance_px: metrics.space_width as f32,
            glyph_advance: Default::default(),
            source: FontResolutionSource::FacePrimary,
        },
        foundry: foundry.map(crate::heap_types::LispString::from_utf8),
        slant,
        metrics,
        capability,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedFontMatch {
    pub font: ResolvedOpenedFont,
    /// Glyph code (font-driver glyph index) from this exact realization — GNU
    /// `font->driver->encode_char`, the cdr of `internal-char-font`.
    pub glyph_code: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedFrameFont {
    pub font: ResolvedOpenedFont,
    /// GNU Lisp face height in 1/10 pt for this realized font.
    ///
    /// GNU `set_lface_from_font` stores
    /// `PIXEL_TO_POINT(font->pixel_size * 10, FRAME_RES(f))` in
    /// `LFACE_HEIGHT_INDEX`; keep the point-height value beside the pixel
    /// metrics so core code never has to guess a frame DPI from pixels.
    pub height_tenths: i32,
}

impl ResolvedFrameFont {
    /// Compare the stable font characteristics that determine a realized
    /// Lisp face, deliberately excluding transient host handles and metrics.
    ///
    /// Native materializers may return a fresh `ResolvedFontId` for the same
    /// opened font. GNU's face-support predicate compares realized font
    /// properties, so handle allocation must not turn fallback to the default
    /// font into an apparent successful selection.
    pub(crate) fn same_face_selection_as(&self, other: &Self) -> bool {
        self.font.resolved.identity == other.font.resolved.identity
            && self.font.weight() == other.font.weight()
            && self.font.slant == other.font.slant
            && self.font.width() == other.font.width()
            && self.height_tenths == other.height_tenths
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFontSpecMatch {
    pub family: crate::heap_types::LispString,
    pub foundry: Option<crate::heap_types::LispString>,
    pub registry: Option<crate::heap_types::LispString>,
    pub file: Option<crate::heap_types::LispString>,
    pub weight: Option<FontWeight>,
    pub slant: Option<FontSlant>,
    pub width: Option<FontWidth>,
    pub spacing: Option<i32>,
    pub postscript_name: Option<crate::heap_types::LispString>,
}

/// Metrics of a font file probed at an exact pixel size, following GNU
/// `font_open_entity` + `ftcrfont_open` semantics (the values `font-info`
/// reports for a font entity). Produced by the layout engine's FreeType
/// probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontPxProbeResult {
    pub pixel_size: u32,
    pub height: i32,
    pub ascent: i32,
    pub descent: i32,
    pub max_width: i32,
    pub space_width: i32,
    pub average_width: i32,
}

/// Request to open a platform font entity for the metrics reported by
/// `font-info`.
///
/// A native entity is not necessarily file-backed: CoreText can identify an
/// exact face by PostScript name and provide its metrics without exposing a
/// path that a portable parser can reopen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontEntityMetricsRequest {
    pub frame_id: crate::window::FrameId,
    pub family: Option<crate::heap_types::LispString>,
    pub registry: Option<crate::heap_types::LispString>,
    pub file: Option<crate::heap_types::LispString>,
    pub postscript_name: Option<crate::heap_types::LispString>,
    pub weight: Option<FontWeight>,
    pub slant: Option<FontSlant>,
    pub width: Option<FontWidth>,
    pub pixel_size: u32,
}

/// Complete native answer for `font-info` on a font entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFontEntityMetrics {
    pub metrics: FontPxProbeResult,
    pub file: Option<crate::heap_types::LispString>,
    pub capability: Option<FontOtfCapability>,
}

/// One GSUB/GPOS side of an OpenType capability report: per script
/// (table order), langsyses (`None` = default langsys, first) with their
/// feature tags. Tags keep trailing spaces ("MKD ").
pub type OtfSideCapability = Vec<(String, Vec<(Option<String>, Vec<String>)>)>;

/// GSUB/GPOS capability of a font file (GNU `hbfont_otf_capability`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontOtfCapability {
    pub gsub: OtfSideCapability,
    pub gpos: OtfSideCapability,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VideoResolveSource {
    File(crate::heap_types::LispString),
    Uri(crate::heap_types::LispString),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VideoResolveRequest {
    pub source: VideoResolveSource,
    pub loop_count: i32,
    pub autoplay: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedVideo {
    pub video_id: neomacs_display_protocol::VideoId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WebKitResolveSource {
    File(crate::heap_types::LispString),
    Uri(crate::heap_types::LispString),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WebKitResolveRequest {
    pub source: WebKitResolveSource,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedWebKit {
    pub webview_id: neomacs_display_protocol::WebViewId,
}

/// One named user uniform for a shader surface, in slot order
/// (`docs/display-engine/SHADER_SURFACES.md`). `components` (1..=4) selects
/// the WGSL accessor type (f32/vec2/vec3/vec4).
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderSurfaceUniformInit {
    pub name: String,
    pub value: [f32; 4],
    pub components: u8,
}

/// Shader source dialect: native WGSL, or Shadertoy-dialect GLSL
/// (`void mainImage(out vec4 fragColor, in vec2 fragCoord)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderSurfaceLanguage {
    Wgsl,
    Glsl,
}

/// Which media cache an `iChannel0` binding samples from
/// (`docs/display-engine/SHADER_SURFACES.md`): another shader surface, a
/// decoded image, or a (playing) video's current frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceChannelKind {
    Surface,
    Image,
    Video,
}

/// Content of a shader surface: user shader source rendered by the
/// compositor, or raw RGBA8 pixels uploaded once.
#[derive(Clone, Debug, PartialEq)]
pub enum ShaderSurfaceContent {
    Shader {
        language: ShaderSurfaceLanguage,
        source: String,
        uniforms: Vec<ShaderSurfaceUniformInit>,
        /// Media sampled as `iChannel0` in the shader.
        channel0: Option<(SurfaceChannelKind, u32)>,
    },
    Pixels {
        data: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderSurfaceCreateRequest {
    pub content: ShaderSurfaceContent,
    pub width: u32,
    pub height: u32,
    pub animate: bool,
    /// Per-surface animation frame-rate cap (`:fps`), if any. `None` renders
    /// at the display refresh rate; `Some(n)` re-renders at most n times/sec
    /// and lets the compositor idle between (battery).
    pub fps: Option<u32>,
}

/// Declarative shader-surface resolution: a `(surface :shader …)` display
/// spec resolved during redisplay, memoized by content like
/// [`VideoResolveRequest`] (the spec IS the identity; no Lisp-side id).
/// Uniform values are carried as `f32::to_bits` so the request derives
/// `Hash`/`Eq` for the host memo.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceResolveRequest {
    pub language: ShaderSurfaceLanguage,
    pub source: String,
    /// `(name, value bits, component count)` in slot order.
    pub uniforms: Vec<(String, [u32; 4], u8)>,
    pub width: u32,
    pub height: u32,
    pub animate: bool,
    /// Per-surface animation frame-rate cap (`:fps`), part of the memo key so
    /// specs differing only by cap are distinct surfaces.
    pub fps: Option<u32>,
    /// Media sampled as `iChannel0` (resolved to a cache id before memoizing,
    /// so the memo key distinguishes different sources).
    pub channel0: Option<(SurfaceChannelKind, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSurface {
    pub surface_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopupMenuEntry {
    pub label: String,
    pub shortcut: String,
    /// Echo-area help attached to this menu item.
    ///
    /// This is owned editor state, not renderer state: the display host may
    /// ignore it, while the modal popup controller publishes it through
    /// GNU's `show-help-function` contract as selection changes.
    pub help: Option<String>,
    pub enabled: bool,
    pub separator: bool,
    pub submenu: bool,
    pub depth: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopupMenuRequest {
    pub frame_id: crate::window::FrameId,
    pub placement: neomacs_display_protocol::PopupPlacement,
    pub title: Option<String>,
    pub entries: Vec<PopupMenuEntry>,
    pub selected: usize,
}

/// The Elisp evaluator.
///
/// # Safety: Send
/// Evaluator is inherently single-threaded (uses thread-local heap and caches).
/// # Safety: Send
/// Context is inherently single-threaded (uses thread-local heap and caches).
/// `neovm-worker` moves the Context to a worker thread inside
/// `Arc<Mutex<..>>`, which ensures exclusive access.
// SAFETY: Rc is !Send only because it uses non-atomic refcounting.
// Since Context is always used single-threaded (guarded by Mutex when
// transferred between threads), this is safe.
unsafe impl Send for Context {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OverlayModificationHook {
    pub(crate) hook_list: Value,
    pub(crate) overlay: Value,
}

/// How a `(throw 'exit VALUE)` unwinds a recursive command loop.
///
/// GNU's `recursive_edit_1` (keyboard.c:749-758) dispatches on the thrown
/// value's type, not on its truthiness: only `t` means "abort with a plain
/// `quit`".  A function is *called*, which is how
/// `minibuffer-quit-recursive-edit` raises `minibuffer-quit` rather than the
/// plain `quit` that `abort-recursive-edit` raises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandLoopExit {
    /// Any other value, notably `nil` (`exit-recursive-edit`).
    Normal,
    /// `t` — `abort-recursive-edit`.
    Quit,
    /// A string, re-signaled as `error` with the string as its datum.
    Error(Value),
    /// A function, called for effect.
    Call(Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ResumeTarget {
    CommandLoopExit,
    CommandLoopTopLevel,
    InterpreterCatch,
    InterpreterConditionCase {
        handler_index: usize,
        condition_stack_base: usize,
    },
    VmCatch {
        resume_id: u64,
        target: u32,
        stack_len: usize,
        spec_depth: usize,
        bind_stack_len: usize,
    },
    VmConditionCase {
        resume_id: u64,
        target: u32,
        stack_len: usize,
        spec_depth: usize,
        bind_stack_len: usize,
    },
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum ConditionFrame {
    Catch {
        tag: Value,
        resume: ResumeTarget,
    },
    ConditionCase {
        conditions: Value,
        resume: ResumeTarget,
    },
    HandlerBind {
        conditions: Value,
        handler: Value,
        mute_span: usize,
    },
    SkipConditions {
        remaining: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum ConditionControlSymbol {
    Debug,
}

impl ConditionControlSymbol {
    fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

fn condition_value_contains_debug(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Symbol(_) => {
            ConditionControlSymbol::from_lisp_value(value) == Some(ConditionControlSymbol::Debug)
        }
        ValueKind::Cons => {
            list_to_vec(value).is_some_and(|items| items.iter().any(condition_value_contains_debug))
        }
        _ => false,
    }
}

fn wants_debugger(setting: &Value, conditions: &Value) -> bool {
    if setting.is_nil() {
        return false;
    }
    let Some(entries) = list_to_vec(setting) else {
        return true;
    };
    let signal_conditions = list_to_vec(conditions).unwrap_or_else(|| vec![*conditions]);
    entries
        .iter()
        .any(|entry| signal_conditions.iter().any(|condition| condition == entry))
}

fn signal_hook_payload_value(sig: &SignalData) -> Value {
    if let Some(raw) = &sig.raw_data {
        *raw
    } else if sig.data.is_empty() {
        Value::NIL
    } else {
        Value::list(sig.data.clone())
    }
}

/// Metadata for a single active bytecode frame in the contiguous `bc_buf`.
pub(crate) struct BcFrame {
    /// Index in `Context::bc_buf` where this frame's stack region starts.
    pub base: usize,
    /// The function value — keeps the bytecode object (and its constants)
    /// reachable by GC.
    pub fun: Value,
}

/// Result of consulting the bytecode tier dispatcher for a stack-backed call.
///
/// The interpreter owns the `Interpret` transition so a bytecode caller can
/// install the callee frame without recursively constructing another VM.
/// Native execution remains hidden behind `Complete`; a deopt returns
/// `Interpret` and therefore rejoins the same Tier-0 frame protocol.
pub(crate) enum BytecodeStackCallDispatch {
    Interpret,
    Complete(EvalResult),
}

/// A unit of work to run synchronously on the Lisp thread at a safe point.
///
/// Other threads (e.g. the diagnostics server) send these over a channel and
/// wake the Lisp thread with a [`Context::wait_notifier`]; the Lisp thread
/// drains and runs them between evaluated forms. This is the generic "run on
/// the eval thread" seam — no diagnostics-specific type enters `neovm-core`.
pub type EvalThreadTask = Box<dyn FnOnce(&mut Context) + Send + 'static>;

/// Opaque identity bound to `standard-input` during a `load`/`eval-buffer`
/// readevalloop so `(read)` consumes the same stream as the loader.
///
/// GNU removes `Qget_file_char` from the obarray and recognizes it with
/// `BASE_EQ`, so Lisp cannot manufacture the internal stream by interning its
/// printed name.  Keep the `SymId` private behind a distinct Rust type for the
/// same reason: call sites can bind or recognize this token, but cannot
/// accidentally compare an arbitrary stream by symbol name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoadReadStreamToken(SymId);

impl LoadReadStreamToken {
    pub(crate) fn as_lisp_value(self) -> Value {
        Value::from_sym_id(self.0)
    }

    pub(crate) fn identifies(self, symbol: SymId) -> bool {
        self.0 == symbol
    }
}

/// One active load-read cursor: the heap `LispString` being read plus a byte
/// offset into it that BOTH the readevalloop and `(read)` advance.  See
/// [`LoadReadStreamToken`] and [`Context::load_read_cursors`].
pub(crate) struct LoadReadCursor {
    /// Heap `LispString` Value being read.  Rooted for the cursor's lifetime
    /// via `push_specpdl_root` when the cursor is pushed.
    pub(crate) source: Value,
    /// Lisp-visible source object used as `end-of-file' signal data.  Neomacs
    /// parses a string snapshot, while GNU retains the original buffer or file
    /// identity for reader diagnostics.
    pub(crate) eof_source: Option<Value>,
    /// Shared byte offset into `source`, advanced by both the readevalloop and
    /// `(read STREAM=standard-input)`.
    pub(crate) pos: usize,
    /// `read-symbol-shorthands` active for this source, if any — so `(read)`
    /// applies the same shorthand rewrites the loader does.
    pub(crate) shorthands: Option<super::value_reader::ReadSymbolShorthands>,
}

/// Result of reading a Lisp variable before choosing whether an unbound cell
/// should signal.  GNU's C hot paths frequently read predeclared `V...` state
/// as optional data, while Lisp evaluation must turn the same unbound state
/// into `void-variable`.  Keeping those outcomes distinct prevents optional
/// internal reads from constructing and then discarding a Lisp signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolValueLookup {
    Bound(Value),
    Unbound,
}

pub struct Context {
    /// Tagged pointer heap — sole GC and allocator.
    pub(crate) tagged_heap: Box<crate::tagged::gc::TaggedHeap>,
    /// Mmap-backed pdump image that owns any mapped heap payloads borrowed by
    /// this evaluator's Lisp objects.
    /// The loaded pdump mapping, LEAKED to 'static at install: process-global
    /// structures (the symbol interner's dump-name aliases — see
    /// `intern_dump_lisp_string` — and the mapped tagged-heap objects) hold
    /// pointers into it, so the mapping must outlive every Context. One
    /// bounded leak per load; production loads once per process.
    pub(crate) pdump_image: Option<&'static super::pdump::mmap_image::LoadedMmapImage>,
    /// One-shot runtime flag set by file pdump loads.  GNU keeps this as
    /// pdumper runtime state, not as a public obarray symbol.
    pub(crate) after_pdump_load_hook_pending: bool,
    /// Runtime-owned `system-name` object used to distinguish GNU's cached
    /// hostname from an explicit Lisp replacement.  This is transient process
    /// state and is deliberately reconstructed rather than serialized in a
    /// portable dump.
    pub(crate) cached_system_name: Value,
    /// The obarray — unified symbol table with value cells, function cells, plists.
    pub(crate) obarray: Obarray,
    /// Specpdl — special binding stack that writes directly to the obarray.
    /// Matches GNU Emacs's specpdl design.
    pub(crate) specpdl: Vec<SpecBinding>,
    /// Binding stacks parked while a nested simulated thread runs. GNU owns a
    /// separate specpdl per thread; keeping suspended stacks out of the active
    /// `specpdl` gives the single-threaded runtime the same isolation while
    /// retaining every parked value as a GC root.
    suspended_thread_bindings: Vec<ThreadDynamicBindingState>,
    /// GNU-compatible CPU and managed-allocation profiler state.
    pub(crate) profiler: super::profiler::ProfilerState,
    /// Lexical environment: flat cons alist mirroring GNU Emacs's
    /// `Vinternal_interpreter_environment`.
    pub(crate) lexenv: Value,
    /// GNU `eval.c` keeps `Vinternal_interpreter_environment` on a hidden
    /// symbol object by `Funintern`ing the public name from the obarray.
    /// NeoVM keeps the actual evaluator-owned symbol identity here so the
    /// public `internal-interpreter-environment` symbol can stay visible
    /// while remaining unbound and non-special.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) internal_interpreter_environment_symbol: SymId,
    /// GNU `eval.c` hot-path DEFVARs exposed via direct globals like
    /// `Vquit_flag`, `Vinhibit_quit`, and `Vthrow_on_input`.
    ///
    /// NeoVM still stores their values in the obarray's symbol cells so Lisp
    /// sees ordinary variables, but evaluator boundaries keep their symbol
    /// identities cached here to avoid repeated name interning/lookups.
    quit_flag_symbol: SymId,
    inhibit_quit_symbol: SymId,
    throw_on_input_symbol: SymId,
    kill_emacs_symbol: SymId,
    quit_flag: Value,
    inhibit_quit: Value,
    /// `throw-on-input`'s value, cached beside the two flags it is polled
    /// with.  GNU's `QUIT` reads C globals; reading this one out of the
    /// obarray instead put a symbol lookup on every safe point, because the
    /// poll's guard is true in any session that owns an input channel -- 8 M
    /// lookups in a 20-keystroke rust-lsp run.
    throw_on_input: Value,
    /// Nonzero while `unbind_to` is running unwind cleanup forms.
    ///
    /// GNU `unbind_to` clears `Vquit_flag` and then runs cleanup forms without
    /// polling the input layer again; input processing sets `Vquit_flag` before
    /// the evaluator observes it.  Neomacs has an evaluator-side
    /// `throw-on-input` poll to bridge host input, so suppress that extra poll
    /// during cleanup to preserve GNU's unwind semantics.
    unwind_cleanup_depth: usize,
    noninteractive_symbol: SymId,
    noninteractive: bool,
    symbols_with_pos_enabled_symbol: SymId,
    /// When true, `symbolp`/`eq`/hash operations transparently unwrap
    /// symbol-with-pos objects. Bound to `t` by the byte-compiler.
    pub(crate) symbols_with_pos_enabled: bool,
    print_symbols_bare_symbol: SymId,
    /// When true, the printer outputs bare symbol names for symbol-with-pos.
    pub(crate) print_symbols_bare: bool,
    /// Features list (for require/provide).
    pub(crate) features: Vec<SymId>,
    /// Features currently being resolved through `require`.
    pub(crate) require_stack: Vec<SymId>,
    /// Files currently being loaded (mirrors `Vloads_in_progress` in lread.c).
    pub(crate) loads_in_progress: Vec<crate::heap_types::LispString>,
    /// Uninterned, identity-only Lisp token for the active load stream.
    pub(crate) load_read_stream_token: LoadReadStreamToken,
    /// Stack of active load-read cursors (nested loads).  Each entry pairs the
    /// heap `LispString` being read with a byte offset that BOTH the
    /// readevalloop AND `(read STREAM=standard-input)` advance — mirroring GNU's
    /// `readcharfun`/`instream` shared cursor (lread.c `readevalloop`).  A file
    /// that calls `(read)` mid-load thus consumes the *next* top-level form and
    /// the loop resumes after it.  Transient process state: never serialized.
    /// The `source` Values are kept alive by a `push_specpdl_root` at push time,
    /// not by this Vec (which GC does not trace).
    pub(crate) load_read_cursors: Vec<LoadReadCursor>,
    /// Compact render of the live Lisp backtrace captured when the most recent
    /// *uncaught* signal was dispatched (specpdl still intact), for the command
    /// loop's error log.  Only populated when debug-level tracing is active, so
    /// it costs nothing in production.  Taken (cleared) when logged.  See
    /// `dispatch_signal` and `command_loop_2`.
    pub(crate) last_uncaught_signal_backtrace: Option<String>,
    /// Buffer manager — owns all live buffers and tracks current buffer.
    pub buffers: BufferManager,
    /// GNU xwidget runtime state: internal model/view lists and id counter.
    pub(crate) xwidgets: super::xwidget::XwidgetState,
    /// GNU `last_overlay_modification_hooks`: hook-list/overlay pairs recorded
    /// by the before-change overlay scan and replayed by the after-change scan.
    pub(crate) last_overlay_modification_hooks: Vec<OverlayModificationHook>,
    /// GNU `interval_insert_behind_hooks`: text-property hook list recorded
    /// by `verify_interval_modification` before an insertion and replayed by
    /// `report_interval_modification` after the inserted text exists.
    pub(crate) interval_insert_behind_hooks: Value,
    /// GNU `interval_insert_in_front_hooks`: text-property hook list recorded
    /// by `verify_interval_modification` before an insertion and replayed by
    /// `report_interval_modification` after the inserted text exists.
    pub(crate) interval_insert_in_front_hooks: Value,
    /// Match data from the last successful search/match operation.
    pub(crate) match_data: Option<MatchData>,
    /// Deferred after-change records, mirroring GNU Emacs's
    /// `combine_after_change_list` (insdel.c). When
    /// `combine-after-change-calls` is non-nil and no incompatible
    /// before-change-functions or overlays are installed,
    /// `signal_after_change` records the change here instead of running
    /// `after-change-functions` immediately. Each entry is the GNU triple
    /// `(charpos - BEG, Z - (charpos - lendel + lenins), lenins - lendel)`
    /// in 1-based character coordinates.
    pub(crate) combine_after_change_list: Vec<(i64, i64, i64)>,
    /// Buffer that owns the deferred after-change records, mirroring GNU
    /// Emacs's `combine_after_change_buffer` (insdel.c). When the change
    /// buffer differs, the pending list is flushed before recording the new
    /// change.
    pub(crate) combine_after_change_buffer: Option<crate::buffer::BufferId>,
    /// Process manager — owns all tracked processes.
    pub(crate) processes: ProcessManager,
    /// Network manager — owns network connections, filters, and sentinels.
    /// Variable watcher list — callbacks on variable changes.
    pub(crate) watchers: VariableWatcherList,
    /// Symbols whose variable watchers are currently running.
    ///
    /// GNU `notify_variable_watchers` temporarily sets the symbol's trapped
    /// write state to `SYMBOL_UNTRAPPED_WRITE` to suppress recursive watcher
    /// notification while a watcher callback mutates the same symbol.
    pub(crate) active_variable_watchers: HashSet<SymId>,
    /// Canonical Lisp object returned by `standard-syntax-table`.
    ///
    /// GNU Emacs stores this in `Vstandard_syntax_table`; NeoVM keeps the
    /// authoritative identity here and mirrors it into thread-local state for
    /// no-evaluator syntax builtins.
    pub(crate) standard_syntax_table: Value,
    /// GNU's `Vsyntax_code_object`: canonical `(CODE)` conses for the 16 bare
    /// syntax classes. Standard syntax tables and `string-to-syntax` share
    /// these objects, so `eq` identity is observable.
    pub(crate) syntax_code_objects: Value,
    /// Last `syntax-ppss` parser state for the current evaluator.
    ///
    /// GNU implements `syntax-ppss` in Lisp as an incremental cache over
    /// `parse-partial-sexp`.  Fields 2 and 6 of the returned state are
    /// intentionally cache-dependent, so keeping the last state is part of
    /// matching the observable behavior of repeated `syntax-ppss` calls.
    /// Canonical Lisp object returned by `standard-category-table`.
    ///
    /// Like `standard_syntax_table`, this is mirrored into thread-local state
    /// because the category-table helpers currently expose some no-evaluator
    /// entry points.
    pub(crate) standard_category_table: Value,
    /// Current buffer-local keymap (set by `use-local-map`).
    pub(crate) current_local_map: Value,
    /// Global keymap selected by `use-global-map`.
    ///
    /// GNU stores this separately from the dynamically bindable Lisp variable
    /// `global-map`; preserving that distinction is observable through
    /// `current-global-map`, active key lookup, and legacy `global-set-key`.
    selected_global_map: super::keymap::SelectedGlobalMap,
    /// Register manager — quick storage and retrieval of text, positions, etc.
    pub(crate) registers: RegisterManager,
    /// Bookmark manager — persistent named positions.
    pub(crate) bookmarks: BookmarkManager,
    /// Abbreviation manager — text abbreviation expansion.
    pub(crate) abbrevs: AbbrevManager,
    /// Autoload manager — deferred function loading.
    pub(crate) autoloads: AutoloadManager,
    /// Custom variable manager — defcustom/defgroup system.
    pub(crate) custom: CustomManager,
    /// Rectangle state — stores the last killed rectangle for yank-rectangle.
    pub(crate) rectangle: RectangleState,
    /// Interactive command registry — tracks interactive commands.
    pub(crate) interactive: InteractiveRegistry,
    /// Tree-sitter runtime manager — loaded grammars, parser state, node handles,
    /// and compiled queries backing `treesit-*` builtins.
    pub(crate) treesit: super::treesit::TreeSitterManager,
    /// Minibuffer runtime state — active minibuffer stack, prompt metadata, and history.
    pub(crate) minibuffers: MinibufferManager,
    /// Count of completed minibuffer reads observed by the evaluator.
    pub(crate) interactive_minibuffer_read_count: u64,
    /// Current echo-area message text, mirroring GNU `current-message`.
    pub(crate) current_message: Option<crate::heap_types::LispString>,
    /// GNU `echo_buffer[2]`, held by identity rather than by name.
    pub(crate) echo_area_buffers: EchoAreaBuffers,
    /// Pending request to resize the echo-area mini-window *exactly* to its
    /// content on the next redisplay, mirroring GNU `resize_echo_area_exactly`
    /// (src/xdisp.c:13228-13245). GNU's `command_loop_1` (src/keyboard.c:1344)
    /// runs `resize_echo_area_exactly` after every command when a message is
    /// displayed, passing `exact_p = (minibuf_level == 0)`. We set this flag at
    /// the same post-command point and consume it in the redisplay layout pass
    /// so a `grow-only` echo window shrinks back to fit a shorter (even
    /// non-empty) message once the command finishes with no active minibuffer.
    pub(crate) echo_area_resize_exact_pending: bool,
    /// Redirected debugging output stream. Mirrors GNU print.c's
    /// `redirect-debugging-output` redirection target for writes through
    /// `external-debugging-output`.
    pub(crate) debugging_output_file: Option<std::fs::File>,
    /// True after print output has selected the current echo area buffer.
    ///
    /// Mirrors GNU xdisp.c `message_buf_print`: `message`/clear reset it, and
    /// the next print-to-echo starts with a fresh echo buffer instead of
    /// appending to the previous message.
    pub(crate) message_buf_print: bool,
    /// Window that was selected when the active minibuffer session began.
    pub(crate) minibuffer_selected_window: Option<crate::window::WindowId>,
    /// Currently active minibuffer window, if any.
    pub(crate) active_minibuffer_window: Option<crate::window::WindowId>,
    /// Pending orderly shutdown requested by GNU C-owned primitives such as
    /// `kill-emacs`.
    pub(crate) shutdown_request: Option<ShutdownRequest>,
    /// Batch-compatible input-mode interrupt flag for `current-input-mode`.
    pub(crate) input_mode_interrupt: bool,
    /// Lisp-visible `quit_char` used by `current-input-mode` and low-level
    /// keyboard quit detection.
    pub(crate) quit_char: i64,
    /// True while the command loop is blocked waiting for external input.
    pub(crate) waiting_for_user_input: bool,
    /// Frame manager — owns all frames and windows.
    pub(crate) frames: FrameManager,
    /// Mode registry — major/minor modes.
    pub(crate) modes: ModeRegistry,
    /// Thread manager — cooperative threading primitives.
    pub(crate) threads: ThreadManager,
    /// Keyboard macro metadata — ring/counter state layered above the
    /// keyboard-owned live recording/playback runtime.
    pub(crate) kmacro: KmacroManager,
    /// Command loop state — event queue, prefix args, kbd macros, quit flag.
    /// Used by the interactive command loop (recursive-edit → command_loop).
    pub(crate) command_loop: crate::keyboard::CommandLoop,
    /// Input event receiver from the display/render thread.
    /// `None` in batch mode (tests, non-interactive evaluation).
    /// When `Some`, `read_char()` blocks on this channel for interactive input.
    pub input_rx: Option<crossbeam_channel::Receiver<crate::keyboard::InputEvent>>,
    /// Optional host-defined suspension point for input waits.
    ///
    /// Browser Workers install this to suspend the Wasm stack without
    /// replacing the typed input channel. Native sessions leave it unset and
    /// use the unified OS input/process poller.
    pub(crate) host_input_wait_backend:
        Option<Box<dyn crate::emacs_core::wait::HostInputWaitBackend>>,
    /// Read-only `lisp/`, `etc/`, `leim/`, and `info/` files supplied by an
    /// embedded product rather than a native filesystem.
    pub(crate) runtime_resource_store:
        Option<Box<dyn crate::emacs_core::fileio::RuntimeResourceStore>>,
    /// Tasks queued from other threads (e.g. the diagnostics server) to run on
    /// the Lisp thread at a safe point. Drained in the `read_char` loop.
    eval_task_rx: Option<crossbeam_channel::Receiver<EvalThreadTask>>,
    /// Cross-thread quit signal. The input-bridge thread flips this to
    /// `true` when it observes a `quit-char` keystroke; the evaluator
    /// drains it from `maybe_quit` into `Vquit_flag` on its next poll.
    ///
    /// GNU handles this case with `sys_longjmp` from the signal or
    /// keystroke handler straight into `read_char`'s `setjmp` target
    /// (`keyboard.c:12738`, `keyboard.c:3812`). Rust can't do that
    /// across owned borrows, so we use an atomic flag and rely on
    /// `maybe_quit` polling from `eval_sub` / `Ffuncall` / the bytecode
    /// VM to pick it up.
    pub quit_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Redisplay callback — called before blocking for input in `read_char()`.
    ///
    /// In GNU Emacs, `read_char()` calls `redisplay()` directly (keyboard.c
    /// calls xdisp.c, both in the same binary). In our crate structure,
    /// `neomacs-layout-engine` depends on `neovm-core`, so neovm-core cannot
    /// call the layout engine directly (circular dependency). Instead,
    /// `neomacs-bin` sets this callback to run the layout engine and send
    /// the resulting `FrameGlyphBuffer` to the render thread.
    ///
    /// `None` in batch mode (no display).
    #[allow(clippy::type_complexity)]
    // frontend callback seam avoids a core/layout dependency cycle
    pub redisplay_fn: Option<Box<dyn FnMut(&mut Self)>>,
    /// Frontend-installed frame snapshot hook (`neomacs--frame-snapshot`).
    /// Same seam pattern as `redisplay_fn`: neovm-core cannot reach the
    /// layout engine, so the frontend lays out the requested frames on
    /// demand and returns the serialized snapshot. Take/call/reinstall.
    #[allow(clippy::type_complexity)]
    pub frame_snapshot_fn: Option<
        Box<
            dyn FnMut(
                &mut Self,
                &crate::emacs_core::xdisp::SnapshotRequest,
            ) -> Result<String, String>,
        >,
    >,
    /// Frontend-installed synchronous window layout query.
    ///
    /// `window-end` with UPDATE non-nil, and every geometry query in the `posn`
    /// family, must use the same row producer as redisplay — GNU runs
    /// `start_display` + `move_it_to` for all of them and has no second
    /// algorithm. The layout engine lives above neovm-core in the dependency
    /// graph, so the frontend installs this typed seam. Taking the callback
    /// while invoking it is tracked explicitly: recursive/exclusively-borrowed
    /// queries report `LayoutBusy` instead of silently returning stale state.
    pub(crate) window_layout_query_adapter: WindowLayoutQueryAdapter,
    /// Smooth scroll accumulated for the next input-consuming redisplay.
    pub(crate) pending_pixel_scroll: Option<crate::keyboard::PendingPixelScroll>,
    /// Host-display bridge for GUI frame realization.
    pub display_host: Option<Box<dyn DisplayHost>>,
    /// Frontend-owned opener for additional text terminals requested by
    /// `make-terminal-frame`. The VM owns identities; platform code owns the
    /// device, raw-mode, input, renderer, and lifecycle resources.
    pub(crate) tty_frame_host_factory: Option<Box<dyn TtyFrameHostFactory>>,
    /// Desired visual configuration.  Lisp updates this snapshot atomically;
    /// attaching or rebuilding a display replays it as authoritative state.
    pub(crate) visual_config: neomacs_display_protocol::VisualConfig,
    /// Native anchor for the next Lisp-driven menu-bar popup.
    pub(crate) pending_menu_bar_popup_anchor: Option<super::MenuBarPopupAnchor>,
    /// Coding system manager — encoding/decoding registry.
    pub(crate) coding_systems: CodingSystemManager,
    /// Reusable scratch-buffer lifecycle shared by code-conversion entry
    /// points. This is transient runtime state, like GNU's
    /// `Vcode_conversion_reused_workbuf` and `reused_workbuf_in_use`.
    pub(crate) code_conversion_workspace: crate::code_conversion_workspace::CodeConversionWorkspace,
    /// Face table — global registry of named face definitions.
    pub(crate) face_table: FaceTable,
    /// Incremented when any face attribute changes; layout engine uses
    /// this to invalidate its resolved face cache.
    pub face_change_count: u64,
    /// Source identity for the display-facing `face_table` derived from a
    /// frame's authoritative Lisp face specifications.  Equal identity means
    /// redisplay can reuse the table without scanning every face again.
    materialized_face_table_source: Option<(crate::window::FrameId, u64)>,
    /// Incremented when any display-affecting buffer-local/global variable is
    /// set (truncate-lines, bidi-*, ctl-arrow, buffer-display-table,
    /// buffer-invisibility-spec, fill-column-indicator, overlay-arrow,
    /// display-line-numbers, …). These change layout with NO buffer-text/face/
    /// overlay tick, so the incremental fast paths key on this counter to force
    /// a full rebuild (adversarial-review fix). Rare event → a global counter
    /// (over-invalidating all windows) is acceptable and simpler than per-var keys.
    pub display_var_change_count: u64,
    /// Explicit redisplay invalidation generation, used for state that GNU
    /// marks with update_mode_lines/window redisplay flags.
    redisplay_generation: u64,
    /// GNU `update_menu_bar` invalidation boundary.  This is narrower than
    /// `redisplay_generation`; see [`MenuBarRebuildGeneration`].
    menu_bar_rebuild_generation: u64,
    /// Which windows' chrome (mode/header/tab line) must be re-generated on
    /// the next redisplay. See [`ChromeDirty`].
    chrome_dirty: crate::emacs_core::chrome_dirty::ChromeDirty,
    /// Process-unique id for THIS evaluator instance. Lets thread-local
    /// caches outside neovm-core (e.g. the layout engine's menu-bar item
    /// cache) refuse entries from a previous Context: tests create many
    /// evaluators per thread, and generation counters restart at 0 while
    /// heap addresses recycle, so without this a cache key could collide
    /// across instances.
    context_instance_id: u64,
    /// Bumped when asynchronously decoded media (images) reaches a terminal
    /// state, so a completed decode escalates past the retained-matrix reuse
    /// key as well as the redisplay signature.
    ///
    /// Redisplay has two independent gates: `redisplay_generation` decides
    /// whether to redisplay at all, while `RetainedWindowKey` decides whether a
    /// window may reuse its retained matrix. An image finishing its decode
    /// changed neither the buffer ticks nor the geometry in that key, so
    /// redisplay ran and then reused the matrix that had captured the image's
    /// 1x1 `Pending` placeholder — every async-decoded buffer image stayed one
    /// pixel for the lifetime of the buffer.
    media_generation: u64,
    /// Last visible state submitted to redisplay.  Mirrors GNU's fast
    /// `needs_no_redisplay` path by skipping layout when none of the visible
    /// inputs changed.
    last_redisplay_signature: Option<RedisplaySignature>,
    /// GNU `lisp_eval_depth`: one shared counter for interpreted cons-form
    /// evaluation, Lisp-visible `funcall`, and bytecode `Bcall`.
    pub(crate) depth: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    eval_counter: u64,
    /// Maximum recursion depth.
    pub(crate) max_depth: usize,
    /// Set when allocation crosses the GC threshold; cleared by `gc_collect`.
    pub(crate) gc_pending: bool,
    /// Total number of GC collections performed.
    pub(crate) gc_count: u64,
    /// Nested depth of explicit GC inhibition scopes.
    pub(crate) gc_inhibit_depth: usize,
    /// True while the mutator-side collection driver
    /// (`gc_collect_from_current_roots_impl`) is on the stack. Set and cleared
    /// INLINE, deliberately not via a Drop guard: a panic that unwinds out of
    /// the driver must LEAVE the flag set, so module-boundary panic containment
    /// can detect "this panic escaped GC machinery — heap invariants unknown"
    /// and re-raise instead of containing. A guard that cleared it on unwind
    /// would erase exactly the evidence the detector exists to preserve.
    pub(crate) gc_driver_active: bool,
    /// Stress-test mode: force GC at every safe point regardless of threshold.
    pub(crate) gc_stress: bool,
    /// Cached Lisp-visible GC tuning variables used on every safe point.
    ///
    /// GNU updates its low-level GC tuning state when the watched variables
    /// change, then keeps `maybe_gc` itself cheap.  Mirror that split here:
    /// refresh the cache on the mutation sites, and let safe points combine
    /// the cached values with current heap usage.
    gc_runtime_settings_cache: GcRuntimeSettingsCache,
    /// Active VM-local root frames. Mirrors GNU's model more closely than a
    /// single save/truncate side vector by keeping VM dynamic roots in explicit
    /// nested frames.
    vm_root_frames: Vec<VmRootFrame>,
    /// Evaluated arguments for active backtrace frames. GNU backtrace entries
    /// store an argument pointer/count; keep Neomacs' hot specpdl entry
    /// similarly compact while this side stack owns the exact-GC roots.
    backtrace_args_stack: Vec<LispArgVec>,
    /// Exact-GC mirror of GNU eval.c's transient C stack Lisp_Object slots.
    /// Examples include a sequence frame seeing the previous evaluated call's
    /// argument array while it evaluates the next form, and `Flet` retaining
    /// its `temps` array until `SAFE_FREE_UNBIND_TO`.
    eval_temp_roots: Vec<Value>,
    sequence_temp_root_frames: Vec<SequenceTempRootFrame>,
    /// Contiguous bytecode stack buffer, matching GNU Emacs's bc_thread_state.
    /// All bytecode frames share this single buffer. GC scans it directly.
    pub(crate) bc_buf: Vec<Value>,
    /// JIT residual-root window stack: generated code stores the operand-stack
    /// values live across a GC-capable shim call into `[top..top+N)` slots of
    /// this stack and bumps `jit_root_stack_top` for the call's duration (see
    /// `emit_cond_residual_roots_pre` in jit/compile.rs) — replacing the
    /// per-call scratch-root save/push/restore shim trio. Grow-only; every
    /// slot below `len` always holds a valid tagged Value (initialized NIL,
    /// only ever overwritten with tagged stores), so tracing `0..top` is
    /// always sound and no per-frame fill is needed.
    pub(crate) jit_root_stack: Vec<Value>,
    /// Mirror of `jit_root_stack.as_mut_ptr()`, republished on growth; read by
    /// generated code via a compile-time field offset.
    pub(crate) jit_root_stack_ptr: *mut Value,
    /// Live top: slots `0..top` are GC roots. Written by generated code around
    /// each rooted shim call; always back at its frame-entry value between
    /// sites (each site restores it), so a fresh load at any site sees the
    /// frame base.
    pub(crate) jit_root_stack_top: usize,
    /// Mirror of `jit_root_stack.len()` (the usable capacity), republished on
    /// growth; generated code compares `top + N` against it and calls the
    /// grow shim on overflow.
    pub(crate) jit_root_stack_cap: usize,
    /// Frame metadata for each active bytecode invocation.
    /// Each entry records where the frame's stack region starts in bc_buf
    /// and the function object (so GC can trace its constants).
    pub(crate) bc_frames: Vec<BcFrame>,
    /// Shared condition runtime mirror for active catch/condition handlers.
    pub(crate) condition_stack: Vec<ConditionFrame>,
    /// Stable identity source for VM resume targets stored in the shared
    /// condition runtime.
    next_resume_id: u64,
    /// GNU `pending_funcalls` equivalent for internal no-Lisp teardown paths.
    pub(crate) pending_safe_funcalls: Vec<PendingSafeFuncall>,
    /// Cached truth of `internal--compiler-function-overrides`.
    ///
    /// GNU's hot evaluator path reads the function cell directly. Neomacs only
    /// needs the override alist during compiler/macro machinery, so keep the
    /// nil/common case as a cached flag and refresh it through the same runtime
    /// binding paths that already maintain `quit-flag` and `noninteractive`.
    compiler_function_overrides_symbol: SymId,
    compiler_function_overrides_active: bool,
    /// Hot cache for named callable resolution in `funcall`/`apply`.
    /// Keyed by symbol id; entries are validated against the obarray's
    /// `function_epoch` so that any `defalias` / `fset` / autoload
    /// installation immediately invalidates stale lookups.
    named_call_cache: FxHashMap<SymId, NamedCallCacheEntry>,
    /// Small hot cache for GNU-shaped lexical env alist lookups.
    lexenv_assq_cache: LexenvAssqCache,
    /// Small hot cache for GNU-shaped lexical special declarations.
    lexenv_special_cache: LexenvSpecialCache,
    /// Nested depth of active macro-expansion scopes.
    macro_expansion_scope_depth: usize,
    /// Monotonic counter for Lisp-visible mutations performed while a macro
    /// expander is running. Eager-load caches use this to preserve GNU
    /// `eval-and-compile` side effects during replay.
    macro_expansion_mutation_epoch: u64,
    /// Diagnostic counters for eager/runtime macro expansion.
    pub(crate) macro_expand_calls: u64,
    pub(crate) macro_expand_total_us: u64,
    /// When true, collect detailed timing counters for macro/eager-load paths.
    macro_perf_enabled: bool,
    macro_perf_stats: MacroPerfStats,
    /// Bootstrapped standard interpreted-closure filter function object.
    /// Rooted so the dumped startup state's runtime closure hook remains live.
    interpreted_closure_filter_fn: Option<Value>,
    /// User-defined fringe bitmaps registered via `define-fringe-bitmap`.
    /// GC-safe: holds no raw `Value`s (bits are `Vec<u16>`, faces are names).
    pub(crate) fringe_bitmaps: super::builtins::fringe_bitmap::FringeBitmapRegistry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShutdownRequest {
    pub exit_code: i32,
    pub restart: bool,
}

/// Whether Lisp forms may still be evaluated in this session.
///
/// # Why this is a state here and is not one in GNU
///
/// GNU's `Fkill_emacs` (src/emacs.c:2954-3088) is declared
/// `attributes: noreturn` (:2974).  Its whole body is `safe_run_hooks
/// (Qkill_emacs_hook)` (:3015-3021), `shut_down_emacs` (:3028) and
/// `exit (exit_code)`.  **It never touches the specpdl.**  So every
/// `unwind-protect` cleanup form between the `kill-emacs` call and the top
/// level is ABANDONED: GNU's exit is an `exit(2)`, not a nonlocal exit, and
/// `unbind_to` is never reached for those frames.
///
/// That is load-bearing for `lisp/startup.el:784-818` (GNU `:774-808`), which is one
/// `unwind-protect` whose body is `(command-line)` and whose cleanup ends in
/// `(unless inhibit-startup-hooks (run-hooks 'emacs-startup-hook
/// 'term-setup-hook))`.  `command-line` ends every batch session at `:1757` (GNU `:1739`)
/// with `(if noninteractive (kill-emacs t))`, so **GNU never runs
/// `emacs-startup-hook` in `--batch`.**
///
/// This port cannot exit from inside the evaluator the way GNU exits from
/// inside a subr: control has to walk back out to `main`, so the specpdl
/// really is drained ([`Context::drain_unwind_to`]).  That makes the interval
/// between `kill-emacs` and the process exit a *state*, where GNU has none,
/// and this enum is that state's name.
///
/// # The invariant
///
/// [`Context::lisp_execution`] is the only place the state is derived, and
/// [`Context::unbind_to_result`]'s `SpecBinding::UnwindProtect` arm is the only
/// place a cleanup form is evaluated -- it matches on this enum exhaustively.
/// A third session state therefore cannot be added without deciding, at
/// compile time, whether Lisp still runs in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LispExecution {
    /// No shutdown has been requested.  GNU is still below `Fkill_emacs` and
    /// `unbind_to` runs cleanup forms: GNU `eval.c:3921-3945`.
    Live,
    /// A [`ShutdownRequest`] is recorded, so GNU has already called `exit`.
    /// Nothing written in Lisp anywhere can run again in this session.
    ExitedAlready,
}

#[derive(Clone, Copy, Debug)]
struct GcRuntimeSettingsCache {
    gc_cons_threshold_bytes: usize,
    gc_cons_percentage_scaled: Option<u64>,
    memory_full: bool,
    /// The four Lisp variables the threshold formula reads, resolved against
    /// the LIVE interner on every settings refresh (rare) instead of through
    /// process-lifetime `cached_symbol_id!` OnceLocks: these ids are first
    /// needed while activating a Context, which can precede a pdump load that
    /// remaps symbol ids, and four `intern()` lookups at GC time cost nothing.
    /// (Defensive: the 2026-08-28 "gc-cons-threshold ignored in --batch" bug
    /// turned out to be the startup GC ceiling never released in
    /// noninteractive sessions — see `configure_gnu_startup_state`.) `None`
    /// until first resolved.
    syms: Option<GcSettingSyms>,
}

/// See [`GcRuntimeSettingsCache::syms`].
#[derive(Clone, Copy, Debug)]
struct GcSettingSyms {
    threshold: SymId,
    percentage: SymId,
    memory_full: SymId,
    startup_ceiling: SymId,
}

impl GcSettingSyms {
    fn resolve() -> Self {
        Self {
            threshold: intern("gc-cons-threshold"),
            percentage: intern("gc-cons-percentage"),
            memory_full: intern("memory-full"),
            startup_ceiling: intern("neomacs--startup-gc-ceiling-active"),
        }
    }

    fn contains(&self, sym_id: SymId) -> bool {
        sym_id == self.threshold
            || sym_id == self.percentage
            || sym_id == self.memory_full
            || sym_id == self.startup_ceiling
    }
}

impl Default for GcRuntimeSettingsCache {
    fn default() -> Self {
        Self {
            gc_cons_threshold_bytes: GC_DEFAULT_THRESHOLD_BYTES,
            gc_cons_percentage_scaled: Some(100_000),
            memory_full: false,
            syms: None,
        }
    }
}

pub(crate) enum RequirePlan {
    Return(Value),
    Load {
        sym_id: SymId,
        name: String,
        path: std::path::PathBuf,
        missing_file: super::load::MissingFilePolicy,
    },
}

pub(crate) fn plan_require_in_state(
    obarray: &Obarray,
    buf: Option<&crate::buffer::Buffer>,
    runtime_resources: Option<&dyn crate::emacs_core::fileio::RuntimeResourceStore>,
    features: &mut Vec<SymId>,
    require_stack: &[SymId],
    feature: Value,
    filename: Option<Value>,
    noerror: Option<Value>,
) -> Result<RequirePlan, Flow> {
    refresh_features_from_variable_in_state(obarray, features);
    // Use symbol_id to transparently handle symbol-with-pos wrappers.
    let sym_id = super::builtins::symbols::symbol_id(&feature).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), feature],
        )
    })?;
    let name = resolve_sym(sym_id).to_owned();
    if features.contains(&sym_id) {
        return Ok(RequirePlan::Return(Value::symbol(&name)));
    }

    // GNU Emacs fns.c:Frequire tracks recursive requires in
    // require_nesting_list, but it does not treat an in-progress require as a
    // provided feature.  Recursive require is legitimate up to GNU's guard.
    let nesting = require_stack
        .iter()
        .filter(|stacked_sym_id| **stacked_sym_id == sym_id)
        .count();
    if nesting > 3 {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Recursive `require' for feature `{name}'"
            ))],
        ));
    }

    // GNU keys MUST-SUFFIX off whether a FILENAME was supplied, not off its value.
    let filename_given = matches!(&filename, Some(value) if !value.is_nil());
    let missing_file = super::load::MissingFilePolicy::from_noerror(
        noerror.as_ref().is_some_and(|value| value.is_truthy()),
    );
    let filename = match filename {
        Some(v) if v.is_nil() => name.clone(),
        Some(v) if v.is_string() => v
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .unwrap_or_default(),
        Some(other) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), other],
            ));
        }
        None => name.clone(),
    };
    let filename = super::load::expand_tilde(&filename);

    // GNU `Frequire` loads with MUST-SUFFIX = t unless the caller passed an
    // explicit FILENAME (src/fns.c), so a `require`d feature is never satisfied
    // by an extensionless file — e.g. Doom's `bin/org-capture` shell script,
    // which otherwise shadowed org's `org-capture.el` and was read as Lisp.
    let requirement = super::load::LoadSuffixRequirement::for_require(filename_given);
    let filename = crate::heap_types::LispString::from_utf8(&filename);
    match super::load::resolve_load_path_file_with_resources(
        obarray,
        buf,
        &filename,
        requirement,
        runtime_resources,
    )? {
        Some(path) => Ok(RequirePlan::Load {
            sym_id,
            name,
            path: super::load::load_path_buf(&path),
            missing_file,
        }),
        None => {
            if missing_file == super::load::MissingFilePolicy::ReturnNil {
                return Ok(RequirePlan::Return(Value::NIL));
            }
            Err(super::load::cannot_open_load_file_signal(&filename))
        }
    }
}

pub(crate) fn finish_require_in_state(
    features: &[SymId],
    sym_id: SymId,
    name: &str,
    loaded_path: Option<&Path>,
) -> EvalResult {
    if features.contains(&sym_id) {
        Ok(Value::symbol(name))
    } else {
        let message = if let Some(path) = loaded_path {
            format!(
                "Loading file {} failed to provide feature '{}'",
                path.display(),
                name
            )
        } else {
            format!("Required feature '{}' was not provided", name)
        };
        Err(signal("error", vec![Value::string(message)]))
    }
}

pub(crate) fn parse_eval_lexical_arg(arg: Option<Value>) -> Result<(bool, Option<Value>), Flow> {
    // GNU eval.c Feval (src/eval.c:2527):
    //   specbind(Qinternal_interpreter_environment,
    //            CONSP(lexical) || NILP(lexical) ? lexical : list_of_t);
    //
    // GNU ALWAYS specbinds — no case leaves the environment untouched.
    // We must always return Some(...) so the caller saves/restores lexenv.
    let Some(arg) = arg else {
        // No LEXICAL arg: clear lexical env (dynamic mode).
        return Ok((false, Some(Value::NIL)));
    };
    if arg.is_nil() {
        // LEXICAL is nil: clear lexical env (dynamic mode).
        return Ok((false, Some(Value::NIL)));
    }

    // Non-nil atom (like t) => lexical mode, env = (t)  [the list!]
    if !arg.is_cons() {
        return Ok((true, Some(Value::list(vec![Value::T]))));
    };

    // Cons (alist) => lexical mode, env = the alist
    if list_to_vec(&arg).is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), arg],
        ));
    }

    Ok((true, Some(arg)))
}

fn lexical_binding_in_obarray(obarray: &Obarray) -> bool {
    obarray
        .symbol_value_id(lexical_binding_symbol())
        .is_some_and(|v| v.is_truthy())
}

#[inline]
fn top_level_lexenv_sentinel() -> Value {
    Value::list(vec![Value::T])
}

#[inline]
fn lexenv_is_active(lexenv: Value) -> bool {
    !lexenv.is_nil()
}

#[inline]
fn is_top_level_lexenv_sentinel(lexenv: Value) -> bool {
    lexenv.is_cons() && lexenv.cons_car().is_t() && lexenv.cons_cdr().is_nil()
}

pub(crate) struct ActiveEvalLexicalArgState {
    specpdl_count: usize,
}

pub(crate) fn begin_eval_with_lexical_arg_in_state(
    _obarray: &mut Obarray,
    lexenv: &mut Value,
    specpdl: &mut Vec<SpecBinding>,
    lexical_arg: Option<Value>,
) -> Result<ActiveEvalLexicalArgState, Flow> {
    let (_use_lexical, lexenv_value) = parse_eval_lexical_arg(lexical_arg)?;
    // Mirrors GNU eval.c Feval:
    //   specbind(Qinternal_interpreter_environment, new_env);
    //   return unbind_to(count, eval_sub(form));
    //
    // We push a SpecBinding::LexicalEnv entry (saving the old lexenv)
    // and set lexenv to the new value. unbind_to restores it
    // automatically, providing unwind-safe cleanup on non-local exits.
    let specpdl_count = specpdl.len();
    if let Some(env) = lexenv_value {
        specpdl.push(SpecBinding::LexicalEnv {
            old_lexenv: *lexenv,
        });
        *lexenv = env;
    }
    Ok(ActiveEvalLexicalArgState { specpdl_count })
}

pub(crate) fn finish_eval_with_lexical_arg_in_state(
    _obarray: &mut Obarray,
    lexenv: &mut Value,
    specpdl: &mut Vec<SpecBinding>,
    state: ActiveEvalLexicalArgState,
) {
    // Mirrors GNU: unbind_to(count, result) which pops the
    // SpecBinding::LexicalEnv entry and restores self.lexenv.
    while specpdl.len() > state.specpdl_count {
        let binding = specpdl.pop().unwrap();
        match binding {
            SpecBinding::LexicalEnv { old_lexenv } => {
                *lexenv = old_lexenv;
            }
            other => {
                // Should not happen — begin only pushes LexicalEnv.
                // Put it back if it does.
                specpdl.push(other);
                break;
            }
        }
    }
}

pub(crate) struct ActiveLambdaCallState {
    specpdl_count: usize,
}

pub(crate) struct ActiveMacroExpansionScopeState {
    saved_specpdl_len: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EvalTempRootScopeState {
    saved_len: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SequenceTempRootScopeState {
    saved_len: usize,
}

#[derive(Clone, Debug)]
struct SequenceTempRootFrame {
    saved_len: usize,
    call_roots: Vec<Value>,
    let_temp_roots: Vec<Value>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VmRootScopeState {
    pushed_vm_root_frame: bool,
    saved_vm_root_frame_len: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpecpdlRootScopeState {
    saved_len: usize,
}

/// Handle to an updatable specpdl GcRoot entry; see
/// [`Context::push_specpdl_root_slot`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpecpdlRootSlot {
    index: usize,
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn bind_lexical_value_rooted_in_specpdl(
    lexenv: &mut Value,
    specpdl: &mut Vec<SpecBinding>,
    sym: SymId,
    value: Value,
) {
    specpdl.push(SpecBinding::GcRoot { value });
    let binding = Value::make_cons(lexenv_binding_symbol_value(sym), value);
    match specpdl.last_mut() {
        Some(SpecBinding::GcRoot { value }) => *value = binding,
        other => panic!("expected temporary specpdl gc root entry, got {other:?}"),
    }
    *lexenv = Value::make_cons(binding, *lexenv);
    match specpdl.pop() {
        Some(SpecBinding::GcRoot { .. }) => {}
        other => panic!("expected temporary specpdl gc root entry, got {other:?}"),
    }
}

fn prepend_lexical_binding_in_specpdl_rooted_env(
    lexenv: &mut Value,
    specpdl: &mut Vec<SpecBinding>,
    env_root_index: usize,
    sym: SymId,
    value: Value,
) {
    specpdl.push(SpecBinding::GcRoot { value });
    let current_env = match specpdl.get(env_root_index) {
        Some(SpecBinding::GcRoot { value }) => *value,
        other => panic!("expected specpdl gc root entry for lexical env, got {other:?}"),
    };
    let binding = Value::make_cons(lexenv_binding_symbol_value(sym), value);
    match specpdl.last_mut() {
        Some(SpecBinding::GcRoot { value }) => *value = binding,
        other => panic!("expected temporary specpdl gc root entry, got {other:?}"),
    }
    let new_env = Value::make_cons(binding, current_env);
    match specpdl.get_mut(env_root_index) {
        Some(SpecBinding::GcRoot { value }) => *value = new_env,
        other => panic!("expected mutable specpdl gc root entry for lexical env, got {other:?}"),
    }
    *lexenv = new_env;
    match specpdl.pop() {
        Some(SpecBinding::GcRoot { .. }) => {}
        other => panic!("expected temporary specpdl gc root entry, got {other:?}"),
    }
}

fn bare_lambda_arg_symbol_id(value: Value) -> Option<SymId> {
    let value = if value.is_symbol_with_pos() {
        value.as_symbol_with_pos_sym().unwrap()
    } else {
        value
    };
    if value.is_nil() {
        Some(intern("nil"))
    } else {
        value.as_symbol_id()
    }
}

#[derive(Clone, Copy, Debug)]
enum LambdaArgumentBinding {
    Dynamic,
    Lexical { env_root_index: usize },
}

/// Panic-safe scope for [`Context::gc_inhibit_depth`]: construction increments
/// the depth, `Drop` decrements it, so the inhibition rebalances even when the
/// wrapped code unwinds — a leaked increment would disable safe-point GC for
/// the rest of the session once panics become catchable. Same ctor-sets /
/// Drop-restores shape as
/// [`crate::emacs_core::symbol::ObarraySymbolCellSkipGuard`]. The guard holds
/// the only live `&mut Context` for its scope; reach it via
/// [`GcInhibitGuard::context`].
struct GcInhibitGuard<'a>(&'a mut Context);

impl<'a> GcInhibitGuard<'a> {
    fn enter(cx: &'a mut Context) -> Self {
        cx.gc_inhibit_depth += 1;
        Self(cx)
    }

    fn context(&mut self) -> &mut Context {
        self.0
    }
}

impl Drop for GcInhibitGuard<'_> {
    fn drop(&mut self) {
        self.0.gc_inhibit_depth -= 1;
    }
}

/// Panic-safe scope for [`Context::unwind_cleanup_depth`], the flag that stops
/// `throw-on-input` polling from throwing out of an `unwind-protect` cleanup
/// body. Construction increments, `Drop` decrements, so a cleanup body that
/// unwinds cannot leave the depth stuck nonzero — which would permanently
/// disable `throw-on-input` once panics become catchable. Same shape as
/// [`GcInhibitGuard`].
struct UnwindCleanupGuard<'a>(&'a mut Context);

impl<'a> UnwindCleanupGuard<'a> {
    fn enter(cx: &'a mut Context) -> Self {
        cx.unwind_cleanup_depth += 1;
        Self(cx)
    }

    fn context(&mut self) -> &mut Context {
        self.0
    }
}

impl Drop for UnwindCleanupGuard<'_> {
    fn drop(&mut self) {
        self.0.unwind_cleanup_depth -= 1;
    }
}

/// Boundary-entry snapshot of the evaluator state a propagating `Err(Flow)`
/// would have restored frame-by-frame. The module ABI takes one before running
/// Lisp (or module code) under `catch_unwind`; a caught panic skipped all of
/// that per-frame restoration, so [`Context::restore_module_boundary`] replays
/// it wholesale — the same recovery GNU performs in `unwind_to_catch`. The
/// JIT dispatch seam (`CompiledLeaf::invoke_native` in jit/compile.rs) records
/// the same snapshot once per native call, at LEAF entry; a panic contained at
/// a shim boundary (`jit_shim_contain!`) is healed against it through
/// [`Context::restore_jit_shim_boundary`], the truncation-only subset
/// appropriate there.
///
/// Deliberately NOT covered (see the PS-T4 design): `gc_inhibit_depth` /
/// `unwind_cleanup_depth` (Drop-guarded, already rebalanced by the unwind),
/// `MODULE_CTX` (Drop-guarded), and every piece of heap/GC protocol state —
/// in particular `TAGGED_HEAP_CONCURRENT_ACTIVE`, whose recovery point is the
/// `set_tagged_heap` resync, never a catch handler.
#[derive(Clone, Copy)]
pub(crate) struct ModuleBoundarySnapshot {
    spec_depth: usize,
    condition_len: usize,
    bc_frames_len: usize,
    bc_buf_len: usize,
    backtrace_args_len: usize,
    eval_temp_roots_len: usize,
    sequence_temp_root_frames_len: usize,
    vm_root_frames_len: usize,
    scratch_gc_roots_len: usize,
    depth: usize,
    lexenv: Value,
    macro_expansion_scope_depth: usize,
}

impl ModuleBoundarySnapshot {
    /// Condition-stack length at the boundary — the base the JIT healing
    /// points compute their truncation floor from (`entry + ours` at the
    /// match shim, `entry` at leaf exit).
    pub(crate) fn condition_len(&self) -> usize {
        self.condition_len
    }

    /// Scratch-GC-root depth at the boundary. The module restore truncates
    /// to it directly; the JIT boundary reads it as the leaf-entry floor for
    /// its deferred root sweep (`restore_jit_shim_boundary` itself must NOT
    /// truncate roots — the pending-root-sweep floor in jit/compile.rs owns
    /// that lifecycle).
    pub(crate) fn scratch_gc_roots_len(&self) -> usize {
        self.scratch_gc_roots_len
    }
}

/// FrameManager wired for the Lisp runtime: every new frame gets its
/// lface vectors seeded, mirroring GNU init_frame_faces in the frame.c
/// creation paths.
fn lisp_frame_manager() -> FrameManager {
    let mut frames = FrameManager::new();
    frames.set_frame_init_hook(super::xfaces::init_frame_lisp_faces);
    frames
}

/// The evaluator owns its `TaggedHeap` (a `Box` field) and publishes a raw
/// pointer to it in the thread-local allocation slot (`setup_thread_locals` /
/// the constructors). That publication has no lifetime tied to the box, so the
/// owner must retract it: without this hook the slot outlived the storage and
/// the next `Value::` constructor on the thread allocated into freed memory.
///
/// Retraction is by pointer identity, so a thread that has since installed a
/// different evaluator's heap keeps that newer installation. Once the slot is
/// empty the next allocation re-derives one (the `cfg(test)` fallback heap) or
/// panics loudly in production, rather than corrupting silently.
impl Drop for Context {
    fn drop(&mut self) {
        crate::tagged::gc::clear_tagged_heap_if_installed(&self.tagged_heap);
    }
}

impl Context {
    pub(crate) fn module_boundary_snapshot(&self) -> ModuleBoundarySnapshot {
        ModuleBoundarySnapshot {
            spec_depth: self.specpdl.len(),
            condition_len: self.condition_stack.len(),
            bc_frames_len: self.bc_frames.len(),
            bc_buf_len: self.bc_buf.len(),
            backtrace_args_len: self.backtrace_args_stack.len(),
            eval_temp_roots_len: self.eval_temp_roots.len(),
            sequence_temp_root_frames_len: self.sequence_temp_root_frames.len(),
            vm_root_frames_len: self.vm_root_frames.len(),
            scratch_gc_roots_len: save_scratch_gc_roots(),
            depth: self.depth,
            lexenv: self.lexenv,
            macro_expansion_scope_depth: self.macro_expansion_scope_depth,
        }
    }

    /// Restore the evaluator to `snap` after a panic was caught at a module
    /// boundary. Mirrors GNU `unwind_to_catch`: pop dead handler frames, run
    /// the specpdl unwind (unwind-protect cleanups, binding/buffer/lexenv
    /// restoration), then truncate the bytecode and root side stacks and reset
    /// the scalar depths.
    pub(crate) fn restore_module_boundary(&mut self, snap: &ModuleBoundarySnapshot) {
        // Handler frames above the boundary carry resume targets into frames
        // the panic destroyed. Drop them BEFORE running cleanups so a signal
        // raised inside a cleanup can never select a dead resume target.
        self.condition_stack.truncate(snap.condition_len);
        // `unbind_to_result` returns early (Err) when an unwind-protect
        // cleanup itself signals; the failing entry was already popped, so
        // looping makes progress and terminates. The cleanup's signal has no
        // handler here — recovery swallows it, like GNU dropping a second
        // error raised while unwinding to a catch.
        while self.specpdl.len() > snap.spec_depth {
            let before = self.specpdl.len();
            let _ = self.unbind_to_result(snap.spec_depth);
            if self.specpdl.len() >= before {
                debug_assert!(false, "specpdl unwind must make progress");
                self.specpdl.truncate(snap.spec_depth);
                break;
            }
        }
        self.bc_frames.truncate(snap.bc_frames_len);
        self.bc_buf.truncate(snap.bc_buf_len);
        // Normally already synced by the Backtrace arm of the unwind above;
        // truncate again in case the panic hit between an args push and its
        // owning specpdl entry.
        self.backtrace_args_stack.truncate(snap.backtrace_args_len);
        self.eval_temp_roots.truncate(snap.eval_temp_roots_len);
        self.sequence_temp_root_frames
            .truncate(snap.sequence_temp_root_frames_len);
        self.vm_root_frames.truncate(snap.vm_root_frames_len);
        // The panicked extent's skipped scratch-root pops: dead pushes above
        // the boundary would pin their objects forever (and grow without
        // bound over repeated contained panics). The cleanups above have
        // finished — their own scratch usage is balanced — so the entry
        // depth is exact. Safe direction: extra roots only ever pin.
        restore_scratch_gc_roots(snap.scratch_gc_roots_len);
        self.depth = snap.depth;
        self.lexenv = snap.lexenv;
        self.macro_expansion_scope_depth = snap.macro_expansion_scope_depth;
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
    }

    /// Heal the evaluator after a panic was contained at a JIT-SHIM boundary
    /// (`jit_shim_contain!` in jit/compile.rs): the TRUNCATION subset of
    /// [`restore_module_boundary`] — condition frames, the bytecode and root
    /// side stacks, the scalars, and the lexenv caches — with deliberately
    /// NO specpdl unwind, run against the LEAF-ENTRY snapshot the dispatch
    /// seam (`CompiledLeaf::invoke_native`) recorded once per native call.
    /// Leaf-entry bases suffice because every field here is BALANCED across
    /// each individual shim call the leaf makes (the leaf itself only touches
    /// them through shims, and callee extents restore them on every non-panic
    /// exit) — so its leaf-entry value IS its value at every shim entry.
    ///
    /// Called from the two points that see a contained panic, never from the
    /// catch handler itself (which must stay free of per-call cost and of
    /// lisp/allocation):
    /// - `neovm_jit_match_handler` entry, with `cond_floor = entry + ours`:
    ///   the match shim pops the leaf's own frames by COUNT, so the panicked
    ///   extent's leaked frames above the leaf's own must go, while the
    ///   leaf's own (the `ours` directly on the entry base) must stay.
    /// - the leaf-exit path in `invoke_native`, with `cond_floor = entry`:
    ///   the leaf is dead, everything above its entry goes.
    ///
    /// The panicked extent's specpdl entries — unwind-protect cleanups
    /// included — are NOT unwound here; they are swept by the depth-based
    /// unwind that runs immediately after, at rooted-or-discarded points:
    /// the match shim's `unbind_to`, the leaf-exit parity unwind in
    /// `invoke_native`, or the enclosing frame's cleanup. (Control never
    /// returns to foreign code in the panicked extent — it goes straight
    /// into the signal plumbing, so deferring the unwind is sound.)
    ///
    /// Why each truncation cannot wait for that later unwind:
    /// - `condition_stack` FIRST: `neovm_jit_match_handler` pops the leaf's
    ///   own frames by COUNT, and signal dispatch selects the innermost
    ///   matching frame — leaked frames would desynchronize the count and
    ///   could select a dead resume target.
    /// - `bc_frames`: context-rooted interpreter entries release their frame by
    ///   `pop()`, so panic-skipped entries would permanently corrupt the stack.
    ///   Iterative children instead root in consumed `bc_buf` operands and are
    ///   healed by the following `bc_buf` truncation.
    /// - `bc_buf` + the root side stacks: owned by Rust frames the panic
    ///   destroyed; nothing else would ever pop them (safe-direction leak,
    ///   but unbounded over repeated contained panics).
    /// - `backtrace_args_stack`: safe to truncate WITHOUT the specpdl unwind
    ///   because release is index-based — the later unwind of surviving
    ///   Backtrace entries above the boundary degrades to a no-op.
    /// - `depth` / `macro_expansion_scope_depth`: managed relatively
    ///   (`+= 1` / `-= 1`); skipped decrements would drift them permanently.
    /// - `lexenv` (+ cache clears): the boundary value is authoritative for
    ///   the continuation, same argument as the module restore.
    ///
    /// Scratch GC roots are deliberately not handled here: on the match path
    /// the generated code's own paired `gc_restore` already swept the
    /// residue (and the dispatch block's live roots must survive), while the
    /// leaf-exit path sweeps them against the recorded entry depth (see the
    /// pending-root-sweep floor in jit/compile.rs).
    pub(crate) fn restore_jit_shim_boundary(
        &mut self,
        snap: &ModuleBoundarySnapshot,
        cond_floor: usize,
    ) {
        self.condition_stack.truncate(cond_floor);
        self.bc_frames.truncate(snap.bc_frames_len);
        self.bc_buf.truncate(snap.bc_buf_len);
        self.backtrace_args_stack.truncate(snap.backtrace_args_len);
        self.eval_temp_roots.truncate(snap.eval_temp_roots_len);
        self.sequence_temp_root_frames
            .truncate(snap.sequence_temp_root_frames_len);
        self.vm_root_frames.truncate(snap.vm_root_frames_len);
        self.depth = snap.depth;
        self.lexenv = snap.lexenv;
        self.macro_expansion_scope_depth = snap.macro_expansion_scope_depth;
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
    }

    /// True when a panic caught at a module boundary must NOT be contained:
    /// it escaped the collection driver, or poisoned a GC lock — either way
    /// heap invariants are unknown and converting the panic into a Lisp error
    /// would keep a possibly-torn heap mutating. Callers re-raise instead
    /// (aborting at the `extern "C"` shim, i.e. pre-containment behavior).
    /// The JIT shim boundary reuses this probe unchanged.
    pub(crate) fn module_panic_recovery_blocked(&self) -> bool {
        self.gc_driver_active || self.tagged_heap.gc_locks_poisoned()
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub(crate) fn current_global_map(&self) -> Value {
        self.selected_global_map.value()
    }

    pub(crate) fn select_global_map(&mut self, keymap: Value) {
        self.selected_global_map.select(keymap);
    }

    #[inline]
    pub(crate) fn subr_dispatch_kind(&self, sym_id: SymId) -> Option<SubrDispatchKind> {
        lookup_global_subr_entry(sym_id).map(|e| e.dispatch_kind)
    }

    #[inline]
    pub(crate) fn subr_dispatch_kind_or_builtin(&self, sym_id: SymId) -> SubrDispatchKind {
        self.subr_dispatch_kind(sym_id)
            .unwrap_or(SubrDispatchKind::Builtin)
    }

    #[inline]
    fn subr_is_special_form_id(&self, sym_id: SymId) -> bool {
        self.subr_dispatch_kind_or_builtin(sym_id) == SubrDispatchKind::SpecialForm
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn subr_is_context_callable_id(&self, sym_id: SymId) -> bool {
        self.subr_dispatch_kind_or_builtin(sym_id) == SubrDispatchKind::ContextCallable
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn has_registered_subr(&self, sym_id: SymId) -> bool {
        lookup_global_subr_entry(sym_id).is_some_and(|e| e.function.is_some())
    }
}

impl Context {
    #[inline]
    fn maybe_grow_eval_stack<R>(&mut self, callback: impl FnOnce(&mut Self) -> R) -> R {
        let depth = self.depth;
        if depth < STACK_GROWTH_PROBE_START_DEPTH
            || !depth.is_multiple_of(STACK_GROWTH_PROBE_INTERVAL)
        {
            return callback(self);
        }
        super::stack_growth::maybe_grow(EVAL_STACK_RED_ZONE, EVAL_STACK_SEGMENT, || callback(self))
    }

    /// Whether lexical-binding is currently enabled.
    pub fn lexical_binding(&self) -> bool {
        lexenv_is_active(self.lexenv)
    }

    pub(crate) fn current_input_mode_tuple(&self) -> (bool, bool, bool, i64) {
        // Batch oracle compatibility: flow-control and meta are fixed to
        // nil/t respectively; quit-char remains mutable like GNU Emacs.
        (self.input_mode_interrupt, false, true, self.quit_char)
    }

    pub(crate) fn set_input_mode_interrupt(&mut self, interrupt: bool) {
        self.input_mode_interrupt = interrupt;
    }

    #[inline]
    pub(crate) fn sync_cached_runtime_binding_by_id(&mut self, sym_id: SymId, value: Value) {
        if sym_id == self.quit_flag_symbol {
            self.quit_flag = value;
        } else if sym_id == self.inhibit_quit_symbol {
            self.inhibit_quit = value;
        } else if sym_id == self.throw_on_input_symbol {
            self.throw_on_input = value;
        } else if sym_id == self.compiler_function_overrides_symbol {
            self.compiler_function_overrides_active = value.is_cons();
        } else if sym_id == self.noninteractive_symbol {
            self.noninteractive = value.is_truthy();
        } else if sym_id == self.symbols_with_pos_enabled_symbol {
            self.symbols_with_pos_enabled = value.is_truthy();
        } else if sym_id == self.print_symbols_bare_symbol {
            self.print_symbols_bare = value.is_truthy();
        } else if sym_id == max_lisp_eval_depth_symbol()
            && let Some(depth) = value.as_fixnum()
        {
            self.max_depth = depth.max(100) as usize;
        }
    }

    /// Test-only view of the cached `throw-on-input`, so a test can assert the
    /// cache still agrees with the obarray after each write path.
    #[cfg(test)]
    pub(crate) fn cached_throw_on_input_for_test(&self) -> Value {
        self.throw_on_input
    }

    /// Publish a completed runtime-variable write to every derived subsystem.
    ///
    /// The obarray/buffer slot is the canonical value.  Evaluator fast fields,
    /// keyboard translation state, GC pacing, and redisplay are projections of
    /// that value and must move together after any `set`/bytecode `varset`.
    /// Keeping that fan-out behind one boundary prevents compiled assignment
    /// from updating the Lisp-visible cell while leaving a stale host cache.
    pub(crate) fn publish_runtime_binding_write_by_id(&mut self, sym_id: SymId, value: Value) {
        self.sync_cached_runtime_binding_by_id(sym_id, value);
        self.sync_keyboard_runtime_binding_by_id(sym_id, value);
        self.refresh_gc_runtime_settings_after_change_by_id(sym_id);
        self.mark_redisplay_dirty_if_display_var(sym_id);
    }

    /// Whether `publish_runtime_binding_write_by_id` would do anything for
    /// `resolved` (an alias-resolved symbol): the union of the four
    /// projections' own tests.  Lets a writer skip computing the value Lisp
    /// sees -- a lexenv scan plus a full variable lookup -- for the vast
    /// majority of symbols, which project to nothing.  GNU has no such
    /// projection layer at all (its C globals ARE the value).
    pub(crate) fn runtime_binding_has_projection(&self, resolved: SymId) -> bool {
        resolved == self.quit_flag_symbol
            || resolved == self.inhibit_quit_symbol
            || resolved == self.compiler_function_overrides_symbol
            || resolved == self.noninteractive_symbol
            || resolved == self.symbols_with_pos_enabled_symbol
            || resolved == self.print_symbols_bare_symbol
            || resolved == max_lisp_eval_depth_symbol()
            || resolved == input_decode_map_symbol()
            || resolved == local_function_key_map_symbol()
            || self.is_gc_runtime_setting_symbol(resolved)
            || crate::buffer::buffer::variable_affects_display_by_sym_id(resolved)
    }

    #[inline(always)]
    pub(crate) fn compiler_function_overrides_active(&self) -> bool {
        self.compiler_function_overrides_active
    }

    fn sync_keyboard_runtime_binding_by_id(&mut self, sym_id: SymId, value: Value) {
        if sym_id == input_decode_map_symbol() {
            self.command_loop.keyboard.set_input_decode_map(value);
        } else if sym_id == local_function_key_map_symbol() {
            self.command_loop.keyboard.set_local_function_key_map(value);
        }
    }

    pub(crate) fn sync_keyboard_runtime_from_obarray(&mut self) {
        let input_decode_map = self
            .obarray
            .symbol_value("input-decode-map")
            .copied()
            .unwrap_or(Value::NIL);
        let local_function_key_map = self
            .obarray
            .symbol_value("local-function-key-map")
            .copied()
            .unwrap_or(Value::NIL);
        self.command_loop
            .keyboard
            .set_terminal_translation_maps(input_decode_map, local_function_key_map);
    }

    pub(crate) fn waiting_for_user_input(&self) -> bool {
        self.waiting_for_user_input
    }

    pub(crate) fn set_waiting_for_user_input(&mut self, waiting: bool) {
        self.waiting_for_user_input = waiting;
    }

    pub(crate) fn has_input_receiver(&self) -> bool {
        self.input_rx.is_some()
    }

    pub(crate) fn pop_unread_command_event(&mut self) -> Option<Value> {
        let event = self.pop_unread_command_event_unrecorded()?;
        self.record_input_event(event);
        Some(event)
    }

    pub(crate) fn pop_unread_command_event_unrecorded(&mut self) -> Option<Value> {
        let current = match self.eval_symbol("unread-command-events") {
            Ok(value) => value,
            Err(_) => Value::NIL,
        };
        match current.kind() {
            ValueKind::Cons => {
                let mut head = current.cons_car();
                let tail = current.cons_cdr();
                self.assign("unread-command-events", tail);
                if head.is_cons() && head.cons_car() == Value::T {
                    head = head.cons_cdr();
                }
                Some(head)
            }
            _ => None,
        }
    }

    pub(crate) fn peek_unread_command_event(&self) -> Option<Value> {
        let current = match self.eval_symbol("unread-command-events") {
            Ok(value) => value,
            Err(_) => Value::NIL,
        };
        match current.kind() {
            ValueKind::Cons => Some(current.cons_car()),
            _ => None,
        }
    }

    /// Whether any Lisp-visible input-processing queue has an event to replay.
    ///
    /// Mirrors GNU `requeued_events_pending_p` (keyboard.c): ending a keyboard
    /// macro must wait for ordinary command events and both input-method queues
    /// that were populated while consuming that macro.
    pub(crate) fn has_pending_requeued_events(&self) -> bool {
        self.eval_symbol("unread-command-events")
            .is_ok_and(|value| value.is_cons())
            || [
                "unread-post-input-method-events",
                "unread-input-method-events",
            ]
            .into_iter()
            .any(|symbol| {
                self.eval_symbol(symbol)
                    .is_ok_and(|value| value.is_truthy())
            })
    }

    /// Prepend an event to the `unread-command-events` list so that the next
    /// `read_char` / `pop_unread_command_event` will consume it first.
    pub(crate) fn push_unread_command_event(&mut self, event: Value) {
        let current = match self.eval_symbol("unread-command-events") {
            Ok(value) => value,
            Err(_) => Value::NIL,
        };
        let new_list = Value::cons(event, current);
        self.assign("unread-command-events", new_list);
    }

    /// Queue a low-level special event on the keyboard event path.
    ///
    /// GNU's `kbd_buffer_store_event` feeds DBus, file-notify, and similar
    /// events through `special-event-map` even when no terminal input is
    /// available.
    pub(crate) fn queue_special_event(&mut self, event: Value) {
        self.command_loop.keyboard.unread_event(event);
    }

    pub(crate) fn replace_unread_command_event_with_singleton(&mut self, event: Value) {
        self.assign("unread-command-events", Value::list(vec![event]));
    }

    /// Set the file-level `lexical-binding` (per-buffer) and sync the
    /// top-level lexical environment.
    ///
    /// Called at file-loading boundaries (load.rs, lread.rs) and test
    /// setup. Mirrors GNU Emacs where the file loader both sets the
    /// `lexical-binding` buffer-local AND specbinds
    /// `Vinternal_interpreter_environment` to `(t)` or `nil`.
    ///
    /// Uses the runtime assignment path so the visible binding is
    /// updated even when a caller has dynamically let-bound
    /// `lexical-binding`. This matches GNU `Fset`, which mutates the
    /// current binding cell before `readevalloop`.
    ///
    /// Note: `Feval` (begin_eval_with_lexical_arg) does NOT call this.
    /// `Feval` only saves/restores `self.lexenv` without touching the
    /// per-buffer `lexical-binding`, matching GNU where nested eval
    /// calls never clobber the file-level setting.
    pub fn set_lexical_binding(&mut self, enabled: bool) {
        let _ =
            self.try_set_runtime_binding_by_id(intern("lexical-binding"), Value::bool_val(enabled));
        if enabled {
            if self.lexenv.is_nil() {
                self.lexenv = top_level_lexenv_sentinel();
            }
        } else if is_top_level_lexenv_sentinel(self.lexenv) {
            self.lexenv = Value::NIL;
        }
    }

    /// Reset transient evaluator state at a completed top-level boundary.
    ///
    /// GNU reaches interactive/runtime boundaries by unwinding dynamic state
    /// back to the top level, not by discarding the binding stack.  NeoVM's
    /// source bootstrap can transiently accumulate bindings, lexical
    /// environments, and catch state while loading `loadup.el` and early
    /// startup Lisp, but those structures must be unwound before the
    /// evaluator is reused.
    pub(crate) fn clear_top_level_eval_state(&mut self) {
        self.unbind_to(0);
        self.lexenv = if lexical_binding_in_obarray(&self.obarray) {
            top_level_lexenv_sentinel()
        } else {
            Value::NIL
        };
        self.condition_stack.clear();
        self.depth = 0;
        // Named-call resolution is a runtime memoization layer, not part of
        // GNU's persisted Lisp surface. If it survives bootstrap/pdump
        // boundaries it can disagree with restored function cells while still
        // carrying a matching function epoch.
        self.named_call_cache.clear();
    }

    #[cfg(test)]
    pub(crate) fn top_level_eval_state_is_clean(&self) -> bool {
        let clean_lexenv = self.lexenv.is_nil()
            || (self.lexical_binding() && is_top_level_lexenv_sentinel(self.lexenv));
        self.specpdl.is_empty()
            && clean_lexenv
            && self.vm_root_frames.is_empty()
            && self.condition_stack.is_empty()
            && self.depth == 0
    }

    #[cfg(test)]
    pub(crate) fn condition_stack_depth_for_test(&self) -> usize {
        self.condition_stack.len()
    }

    pub(crate) fn set_interpreted_closure_filter_fn(&mut self, filter_fn: Option<Value>) {
        self.interpreted_closure_filter_fn = filter_fn;
    }

    /// Load a file with a typed caller policy, converting EvalError back to
    /// Flow for use in special forms.
    pub(crate) fn load_file_internal_with_options(
        &mut self,
        path: &std::path::Path,
        options: super::load::LoadOptions,
    ) -> EvalResult {
        super::load::load_file_with_options(self, path, options)
            .map_err(super::error::flow_from_eval_error)
    }

    pub(crate) fn eval_value_with_lexical_arg(
        &mut self,
        form: Value,
        lexical_arg: Option<Value>,
    ) -> EvalResult {
        let state = begin_eval_with_lexical_arg_in_state(
            &mut self.obarray,
            &mut self.lexenv,
            &mut self.specpdl,
            lexical_arg,
        )?;
        let eval_result = self.eval_value(&form);
        let result = self.dispatch_signal_result_if_needed(eval_result);
        finish_eval_with_lexical_arg_in_state(
            &mut self.obarray,
            &mut self.lexenv,
            &mut self.specpdl,
            state,
        );
        result
    }

    pub(crate) fn eval_lambda_body_value(&mut self, body: Value) -> EvalResult {
        self.maybe_grow_eval_stack(|ctx| {
            let mut cursor = body;
            let mut last = Value::NIL;
            while cursor.is_cons() {
                match ctx.eval_sub(cursor.cons_car()) {
                    Ok(value) => last = value,
                    Err(Flow::ThreadBlocked(blocked)) => {
                        let remaining_forms = if blocked.remaining_forms.is_nil() {
                            cursor.cons_cdr()
                        } else {
                            blocked.remaining_forms
                        };
                        return Err(Flow::thread_blocked(blocked.blocker, remaining_forms));
                    }
                    Err(flow) => return Err(flow),
                }
                cursor = cursor.cons_cdr();
            }
            Ok(last)
        })
    }

    pub(crate) fn begin_lambda_call(
        &mut self,
        fun: Value,
        arglist: Value,
        env: Option<Value>,
        args: &[Value],
    ) -> Result<ActiveLambdaCallState, Flow> {
        let specpdl_count = self.specpdl.len();
        let argument_binding = if let Some(env) = env {
            let old_lexenv = std::mem::replace(&mut self.lexenv, env);
            // Mirrors GNU funcall_lambda:
            //   specbind (Qinternal_interpreter_environment, lexenv);
            self.specpdl.push(SpecBinding::LexicalEnv { old_lexenv });

            let env_root_index = self.specpdl.len();
            self.specpdl.push(SpecBinding::GcRoot { value: env });
            LambdaArgumentBinding::Lexical { env_root_index }
        } else {
            if !self.lexenv.is_nil() {
                let old_lexenv = std::mem::replace(&mut self.lexenv, Value::NIL);
                // GNU funcall_lambda computes a nil local `lexenv` for a
                // dynamically scoped lambda and saves the caller's lexical
                // environment before evaluating its body.
                self.specpdl.push(SpecBinding::LexicalEnv { old_lexenv });
            }
            LambdaArgumentBinding::Dynamic
        };

        if let Err(flow) = self.bind_lambda_args_from_arglist(argument_binding, fun, arglist, args)
        {
            return match self.unbind_to_with_result(specpdl_count, Err(flow)) {
                Err(flow) => Err(flow),
                Ok(_) => unreachable!("unwinding an error cannot produce a value"),
            };
        }

        // GNU never writes `lexical-binding` during lambda/closure calls.
        // The closure's captured env is installed in self.lexenv (above),
        // which is the single source of truth for "is lexical mode active?"
        // via lexical_binding() -> !self.lexenv.is_nil().
        Ok(ActiveLambdaCallState { specpdl_count })
    }

    pub(crate) fn finish_lambda_call(
        &mut self,
        state: ActiveLambdaCallState,
        result: EvalResult,
    ) -> EvalResult {
        // Dynamic arguments must unwind through the same typed specpdl path
        // as every other special binding. In particular, LetLocal records the
        // buffer whose slot was shadowed and LetDefault records a localized
        // variable's shared default.
        self.unbind_to_with_result(state.specpdl_count, result)
    }

    fn bind_lambda_args_from_arglist(
        &mut self,
        binding: LambdaArgumentBinding,
        fun: Value,
        arglist: Value,
        args: &[Value],
    ) -> Result<(), Flow> {
        let optional_sym = intern("&optional");
        let rest_sym = intern("&rest");
        let mut syms_left = arglist;
        let mut arg_index = 0;
        let mut optional = false;
        let mut rest = false;
        let mut previous_rest = false;

        while syms_left.is_cons() {
            let next = syms_left.cons_car();
            syms_left = syms_left.cons_cdr();
            let Some(next_id) = bare_lambda_arg_symbol_id(next) else {
                return Err(signal(LispCondition::InvalidFunction, vec![fun]));
            };

            if next_id == rest_sym {
                if rest || previous_rest {
                    return Err(signal(LispCondition::InvalidFunction, vec![fun]));
                }
                rest = true;
                previous_rest = true;
            } else if next_id == optional_sym {
                if optional || rest || previous_rest {
                    return Err(signal(LispCondition::InvalidFunction, vec![fun]));
                }
                optional = true;
            } else {
                let arg = if rest {
                    let rest_value = Value::list_from_slice(&args[arg_index..]);
                    arg_index = args.len();
                    rest_value
                } else if arg_index < args.len() {
                    let arg = args[arg_index];
                    arg_index += 1;
                    arg
                } else if !optional {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![fun, Value::fixnum(args.len() as i64)],
                    ));
                } else {
                    Value::NIL
                };

                match binding {
                    LambdaArgumentBinding::Dynamic => self.try_specbind(next_id, arg)?,
                    LambdaArgumentBinding::Lexical { env_root_index } => {
                        prepend_lexical_binding_in_specpdl_rooted_env(
                            &mut self.lexenv,
                            &mut self.specpdl,
                            env_root_index,
                            next_id,
                            arg,
                        );
                    }
                }
                previous_rest = false;
            }
        }

        if !syms_left.is_nil() || previous_rest {
            return Err(signal(LispCondition::InvalidFunction, vec![fun]));
        }
        if arg_index < args.len() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![fun, Value::fixnum(args.len() as i64)],
            ));
        }

        Ok(())
    }

    /// Keep the Lisp-visible `features` variable in sync with the evaluator's
    /// internal feature set.
    pub(crate) fn sync_features_variable(&mut self) {
        sync_features_variable_in_state(&mut self.obarray, &self.features);
    }

    pub(crate) fn refresh_features_from_variable(&mut self) {
        refresh_features_from_variable_in_state(&self.obarray, &mut self.features);
    }

    /// Rebind target-owned Lisp identity after restoring a portable image.
    ///
    /// A portable snapshot carries editor state, not the producer binary's
    /// platform contract. This consumer therefore owns `system-type` and the
    /// C-level feature tail, just as its compiled primitive catalog owns the
    /// callable subr surface.
    pub(crate) fn rebind_compiled_target_identity(&mut self) {
        self.set_variable("system-type", Value::symbol(gnu_system_type()));

        let target_owned_features = super::c_features::gnu_c_features()
            .into_iter()
            .map(|feature| intern(feature.name))
            .collect::<HashSet<_>>();
        self.features
            .retain(|feature| !target_owned_features.contains(feature));
        self.features.extend(initial_feature_ids());
        self.sync_features_variable();
    }

    fn has_feature(&mut self, name: &str) -> bool {
        feature_present_in_state(&self.obarray, &mut self.features, name)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn add_feature(&mut self, name: &str) {
        add_feature_in_state(&mut self.obarray, &mut self.features, name);
    }

    pub(crate) fn feature_present(&mut self, name: &str) -> bool {
        self.has_feature(name)
    }

    /// Remove a feature (used to undo temporary provides during bootstrap).
    pub(crate) fn remove_feature(&mut self, name: &str) {
        remove_feature_in_state(&mut self.obarray, &mut self.features, name);
    }

    /// Access the obarray (for builtins that need it).
    pub fn obarray(&self) -> &Obarray {
        &self.obarray
    }

    /// Resolve a fringe-bitmap symbol value to its registry index and data.
    /// Used by the display pipeline to turn a `(left-fringe SYMBOL FACE)` spec
    /// into a renderable bitmap. Returns `None` for a non-symbol or a symbol
    /// with no registered user bitmap.
    pub fn fringe_bitmap_for_symbol(
        &self,
        symbol: Value,
    ) -> Option<(u32, &super::builtins::fringe_bitmap::FringeBitmap)> {
        let sym = symbol.as_symbol_id()?;
        let index = self.fringe_bitmaps.index_of(sym)?;
        let bitmap = self.fringe_bitmaps.get(sym)?;
        Some((index, bitmap))
    }

    /// Borrow the fringe-bitmap registry (read-only) for per-frame snapshots.
    pub fn fringe_bitmap_registry(&self) -> &super::builtins::fringe_bitmap::FringeBitmapRegistry {
        &self.fringe_bitmaps
    }

    /// Resolve a fringe-bitmap symbol NAME (e.g. `"empty-line"`) to its registry
    /// index, the way the display pipeline needs when it produces a synthetic
    /// indicator row (empty-line `~` markers) rather than reacting to an explicit
    /// `(left-fringe SYM)` display spec. Returns `None` if no bitmap is
    /// registered under that name.
    pub fn fringe_bitmap_index_for_name(&self, name: &str) -> Option<u32> {
        let sym = super::intern::intern(name);
        self.fringe_bitmaps.index_of(sym)
    }

    /// The buffer-default value of a variable (GNU `BVAR(&buffer_defaults, …)`).
    /// For a slot-backed per-buffer variable this reads `buffer_defaults[slot]`
    /// (the value seen by buffers that haven't set a local override); for any
    /// other variable it falls back to the obarray value cell. Used by the
    /// layout-side fringe-indicator resolver, which mirrors GNU's two-level
    /// lookup (buffer-local, then `buffer_defaults`) — the obarray value cell of
    /// a forwarded slot var is always nil, so the resolver must read the default
    /// slot directly.
    pub fn buffer_default_value(&self, name: &str) -> Option<Value> {
        if let Some(info) = crate::buffer::buffer::lookup_buffer_slot(name) {
            return Some(self.buffers.buffer_defaults[info.offset.index()]);
        }
        self.obarray.symbol_value(name).copied()
    }

    /// Seed the 24 GNU standard built-in fringe bitmaps into the registry and
    /// set each bitmap symbol's `'fringe` plist property to its index.
    ///
    /// Mirrors GNU `syms_of_fringe` (which installs `standard_bitmaps[]` and the
    /// `'fringe` indices). This is the authoritative seed in Rust: it makes the
    /// standard bitmaps resolvable even in contexts that never load
    /// `lisp/fringe.el` (e.g. unit-test `Context::new()`), so an explicit
    /// `(left-fringe right-arrow …)` display spec resolves to a real bitmap. In a
    /// fully lisp-loaded runtime, fringe.el's `(put SYM 'fringe N)` loop runs too
    /// (we bind `fringe-bitmaps` so its `(boundp 'fringe-bitmaps)` guard passes)
    /// and re-puts the SAME indices — idempotent because the orders agree.
    /// Called from `Context::new` after the obarray is populated.
    pub(crate) fn pre_register_standard_fringe_bitmaps(&mut self) {
        let assigned = self.fringe_bitmaps.pre_register_standard_bitmaps();
        let symbols_with_pos_enabled = self.symbols_with_pos_enabled;
        let fringe_prop = Value::symbol("fringe");
        for (sym, index) in assigned {
            let sym_value = Value::from_sym_id(sym);
            // Ignore the (unreachable) plist error path: these are freshly
            // interned symbols with nil plists.
            let _ = super::builtins::symbols::put_in_obarray_values(
                &mut self.obarray,
                sym_value,
                fringe_prop,
                Value::fixnum(i64::from(index)),
                symbols_with_pos_enabled,
            );
        }
        self.pre_register_standard_fringe_indicator_alist();
    }

    /// Seed the GNU default `fringe-indicator-alist` / `fringe-cursor-alist`
    /// (`lisp/fringe.el`'s `(boundp 'fringe-bitmaps)`-guarded `setq-default`
    /// block, ~lines 65-84), so the logical-indicator resolver
    /// (`get_logical_fringe_bitmap`) finds the standard truncation / continuation
    /// / empty-line bitmaps even in a bare `Context::new()` that never loads
    /// `lisp/fringe.el`. In a fully lisp-loaded runtime fringe.el runs the same
    /// `setq-default`, which simply overwrites this identical default. Only seeds
    /// when the current default is still nil so a loaded fringe.el (or user
    /// customization that ran first) is never clobbered.
    fn pre_register_standard_fringe_indicator_alist(&mut self) {
        let Some(info) = crate::buffer::buffer::lookup_buffer_slot("fringe-indicator-alist") else {
            return;
        };
        let offset = info.offset.index();
        // Don't clobber a default already installed (loaded fringe.el / user).
        if !self.buffers.buffer_defaults[offset].is_nil() {
            return;
        }
        // Build the literal GNU default alist (`lisp/fringe.el` ~65-77). Evaluate
        // the quoted form so the cons cells are heap-allocated and rooted by the
        // evaluator; `setq-default`'s per-buffer-default plumbing is too early to
        // rely on during init, so write the resolved default slot directly.
        let Ok(alist) = self.eval_str(
            "'((truncation . (left-arrow right-arrow)) \
               (continuation . (left-curly-arrow right-curly-arrow)) \
               (overlay-arrow . right-triangle) \
               (up . up-arrow) \
               (down . down-arrow) \
               (top . (top-left-angle top-right-angle)) \
               (bottom . (bottom-left-angle bottom-right-angle \
                          top-right-angle top-left-angle)) \
               (top-bottom . (left-bracket right-bracket \
                              top-right-angle top-left-angle)) \
               (empty-line . empty-line) \
               (unknown . question-mark))",
        ) else {
            return;
        };
        // Set the per-buffer default (GNU `buffer_defaults`) and propagate to
        // every existing buffer still using the default (its slot was copied
        // from `buffer_defaults` at creation, before this seed ran). GNU buffers
        // read `buffer_defaults` live; neomacs copies, so update the copies that
        // are still nil (no explicit local override).
        self.buffers.buffer_defaults[offset] = alist;
        self.buffers
            .seed_default_slot_into_unset_buffers(info.offset, alist);
    }

    /// Access the obarray mutably.
    pub fn obarray_mut(&mut self) -> &mut Obarray {
        &mut self.obarray
    }

    /// Public read access to the buffer manager.
    pub fn buffer_manager(&self) -> &BufferManager {
        &self.buffers
    }

    /// Public mutable access to the buffer manager.
    pub fn buffer_manager_mut(&mut self) -> &mut BufferManager {
        &mut self.buffers
    }

    /// Public read access to the frame manager.
    pub fn frame_manager(&self) -> &FrameManager {
        &self.frames
    }

    /// Public mutable access to the frame manager.
    pub fn frame_manager_mut(&mut self) -> &mut FrameManager {
        &mut self.frames
    }

    /// Move a window's point marker during redisplay (GNU force_start branch
    /// moving point into the window). The buffer point for the selected
    /// window is the caller's responsibility.
    pub fn set_window_point_for_redisplay(
        &mut self,
        frame_id: crate::window::FrameId,
        window_id: crate::window::WindowId,
        point_lisp: LispCharPos1,
    ) {
        let buffers = &mut self.buffers;
        if let Some(window) = self
            .frames
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
        {
            crate::window::window_markers::set_window_point_with_marker(
                buffers, window, point_lisp,
            );
        }
    }

    pub fn create_window_markers_for_root(
        &mut self,
        frame_id: crate::window::FrameId,
        buffer_id: crate::buffer::BufferId,
    ) {
        let root = &mut self.frames.get_mut(frame_id).unwrap().root_window;
        debug_assert_eq!(root.buffer_id(), Some(buffer_id));
        crate::window::window_markers::attach_window_position_markers(&mut self.buffers, root);
    }

    pub fn create_window_markers_for_minibuffer(
        &mut self,
        frame_id: crate::window::FrameId,
        buffer_id: crate::buffer::BufferId,
    ) {
        let mini = self
            .frames
            .get_mut(frame_id)
            .unwrap()
            .minibuffer_leaf
            .as_mut();
        if let Some(mini) = mini {
            debug_assert_eq!(mini.buffer_id(), Some(buffer_id));
            crate::window::window_markers::attach_window_position_markers(&mut self.buffers, mini);
        }
    }

    pub fn sync_window_positions(&mut self, buffer_id: crate::buffer::BufferId) {
        for frame in self.frames.frames_mut() {
            crate::window::window_markers::sync_window_positions_from_markers(
                frame,
                &self.buffers,
                buffer_id,
            );
        }
    }

    pub fn current_message_text(&self) -> Option<String> {
        self.current_message
            .as_ref()
            .map(|message| crate::emacs_core::emacs_char::to_utf8_lossy(message.as_bytes()))
    }

    /// Whether redisplay should move the active cursor into an inactive echo
    /// area while displaying the current message.
    ///
    /// GNU's `get_window_cursor_type` reads the dynamically visible value of
    /// `cursor-in-echo-area`; exposing the semantic request here keeps layout
    /// from bypassing specbindings through the obarray's global value cell.
    /// An active minibuffer already owns the live selected-window cursor and
    /// therefore is not an inactive echo-area redirection.
    pub fn inactive_echo_area_cursor_requested(&self) -> bool {
        self.current_message.is_some()
            && !self.minibuffer_is_active()
            && self
                .visible_variable_value_or_nil("cursor-in-echo-area")
                .is_truthy()
    }

    pub fn minibuffer_is_active(&self) -> bool {
        self.minibuffers.is_active()
    }

    pub fn active_minibuffer_window_id(&self) -> Option<WindowId> {
        if let Some(wid) = self.active_minibuffer_window {
            return Some(wid);
        }
        let state = self.minibuffers.current()?;

        for frame_id in self.frames.frame_list() {
            let Some(frame) = self.frames.get(frame_id) else {
                continue;
            };
            if let Some(minibuffer_wid) = frame.minibuffer_window
                && let Some(window) = frame.find_window(minibuffer_wid)
                && window.buffer_id() == Some(state.buffer_id)
            {
                return Some(minibuffer_wid);
            }
        }
        None
    }

    pub fn minibuffer_window_is_active(&self, window_id: WindowId) -> bool {
        self.active_minibuffer_window_id() == Some(window_id)
    }

    /// Window that invoked the currently active minibuffer.
    ///
    /// GNU keeps this window's mode/header line active while the minibuffer
    /// owns input selection (`minibuffer-selected-window`).
    pub fn minibuffer_selected_window_id(&self) -> Option<WindowId> {
        self.active_minibuffer_window_id()?;
        self.minibuffer_selected_window
    }

    pub fn activate_minibuffer_window_for_buffer(
        &mut self,
        minibuf_id: BufferId,
        prompt: crate::heap_types::LispString,
        initial_input: Option<crate::heap_types::LispString>,
    ) -> Result<Option<WindowId>, Flow> {
        let entry_level = {
            let state = self.minibuffers.read_from_minibuffer_lisp(
                minibuf_id,
                &prompt,
                initial_input.as_ref(),
                None,
            )?;
            let depth = std::num::NonZeroUsize::new(state.depth)
                .expect("an active minibuffer has nonzero depth");
            super::minibuffer::MinibufferEntryLevel::from_depth(depth)
        };

        let frame_id = super::window_cmds::ensure_selected_frame_id_in_state(
            &mut self.frames,
            &mut self.buffers,
        );
        let Some(frame) = self.frames.get(frame_id) else {
            self.buffers.switch_current(minibuf_id);
            return Ok(None);
        };
        let Some(minibuffer_window_id) = frame.minibuffer_window else {
            self.buffers.switch_current(minibuf_id);
            return Ok(None);
        };
        let previous_selected_window = frame.selected_window;

        super::window_cmds::remember_selected_window_point_in_state(
            &mut self.frames,
            &mut self.buffers,
            frame_id,
        );
        if let Some(frame) = self.frames.get_mut(frame_id) {
            if let Some(window) = frame.find_window_mut(minibuffer_window_id) {
                window.set_buffer(minibuf_id);
                crate::window::window_markers::attach_window_position_markers(
                    &mut self.buffers,
                    window,
                );
            }
            let _ = frame.select_window(minibuffer_window_id);
        }
        self.buffers.switch_current(minibuf_id);
        super::reader::MinibufferSelectedWindowUpdate::for_entry(
            entry_level,
            previous_selected_window,
            minibuffer_window_id,
        )
        .apply(&mut self.minibuffer_selected_window);
        self.active_minibuffer_window = Some(minibuffer_window_id);
        Ok(Some(minibuffer_window_id))
    }

    pub fn current_message_value(&self) -> Option<Value> {
        self.current_message
            .as_ref()
            .map(|message| Value::heap_string(message.clone()))
    }

    /// Whether the next redisplay should resize the echo-area mini-window
    /// exactly to its content (GNU `resize_echo_area_exactly`, the post-command
    /// `exact_p = minibuf_level == 0` case in src/xdisp.c:13235). Read by the
    /// layout engine's grow-only mini-window shrink check. The flag is cleared
    /// once per redisplay in `redisplay_with_force` so a later mid-command
    /// redisplay does not keep shrinking a freshly grown message (GNU only
    /// resizes exactly at the command boundary, not on every `redisplay_window`).
    pub fn echo_area_resize_exact_pending(&self) -> bool {
        self.echo_area_resize_exact_pending
    }

    pub fn set_current_message(&mut self, message: Option<crate::heap_types::LispString>) {
        // An ordinary message takes ownership of the echo area away from the
        // keyboard reader. Keyboard echo publication restores its typed state
        // only after installing its own message.
        self.cancel_key_echo_state();
        self.message_buf_print = false;
        if self.current_message != message {
            // GNU keeps no copy: `current_message` (src/xdisp.c:13420) reads the
            // echo buffer back with `make_buffer_string (BEG, Z, true)`
            // (`current_message_1`, src/xdisp.c:13437-13446), so what Lisp sees
            // is the text as the buffer holds it -- multibyte whenever the
            // buffer is. Store what was written, not what was handed in.
            let stored = self.mirror_message_to_echo_area_buffer(message.as_ref());
            self.current_message = stored.or(message);
            self.invalidate_redisplay();
        }
    }

    /// Mirror the current echo message into the ` *Echo Area 0*` buffer, the way
    /// GNU `set_message_1` (`src/xdisp.c`) does: clear the echo buffer and insert
    /// the message text at BEG. This keeps the echo-area buffer as the single
    /// source of truth for the message text so redisplay can render it as
    /// ordinary buffer text (the GNU `display_echo_area_1` model). The echo
    /// reroute has landed: the layout engine renders the inactive echo area
    /// through this ` *Echo Area 0*` buffer via the normal buffer walk (not from
    /// `current_message`), so keeping this buffer in sync is load-bearing.
    ///
    /// Returns the text as the echo buffer now holds it, which is what GNU
    /// `current_message` reads back out (src/xdisp.c:13437-13446); `None` when
    /// there was nothing to write or no echo buffer to write it to.
    fn mirror_message_to_echo_area_buffer(
        &mut self,
        message: Option<&crate::heap_types::LispString>,
    ) -> Option<crate::heap_types::LispString> {
        match message {
            Some(message) => {
                // GNU `set_message_1` runs inside `with_echo_area_buffer`
                // (xdisp.c:12904), which calls `ensure_echo_area_buffers ()`
                // first — so setting a message always materializes the echo
                // buffers. Creation order stays correct because `builtin_message`
                // logs *Messages* (message_dolog) BEFORE set_current_message.
                self.ensure_echo_area_buffers();
                let id = self.echo_area_display_buffer()?;
                // GNU `with_echo_area_buffer` clears the echo buffer
                // (`del_range (BEG, Z)`) BEFORE setting its multibyteness, then
                // inserts. Order matters: `set_buffer_multibyte_flag` only flips
                // the flag, so toggling it while the buffer still holds the
                // previous message — encoded in the OTHER multibyteness — makes
                // the subsequent full-range delete in
                // `replace_buffer_contents_lisp_string` miscompute its position
                // adjustment and panic ("buffer text edit position underflow").
                // Clear first (with the flag still matching the existing
                // content), then toggle on the now-empty buffer, then insert.
                let _ = self.buffers.replace_buffer_contents(id, "");
                let buffer_is_multibyte =
                    self.buffers.get(id).map(|buffer| buffer.get_multibyte())?;
                let resolved = EchoAreaMessageText::resolve(
                    message,
                    buffer_is_multibyte,
                    self.visible_variable_value_or_nil("unibyte-display-via-language-environment")
                        .is_truthy(),
                );
                let _ = self
                    .buffers
                    .set_buffer_multibyte_flag(id, resolved.buffer_is_multibyte);
                let _ = self
                    .buffers
                    .replace_buffer_contents_lisp_string(id, &resolved.text);
                Some(resolved.text)
            }
            None => {
                // Clearing the message: only touch the echo buffer if it already
                // exists; do not materialize it just to empty it.
                let id = self.echo_area_display_buffer()?;
                let _ = self.buffers.replace_buffer_contents(id, "");
                None
            }
        }
    }

    /// GNU `ensure_echo_area_buffers` (src/xdisp.c:12862-12884).
    ///
    /// A slot is (re)filled only when it holds no buffer or the buffer it holds
    /// has died -- never merely because no buffer answers to the canonical
    /// name. That is what keeps the echo area attached to its buffer across a
    /// rename, and what keeps an unrelated user buffer standing at the name
    /// from being adopted and overwritten. Filling uses `Fget_buffer_create`
    /// semantics, as GNU does, so a dump-restored buffer is re-adopted once and
    /// identified by id from then on.
    pub fn ensure_echo_area_buffers(&mut self) {
        for index in 0..EchoAreaBuffers::NAMES.len() {
            let live =
                self.echo_area_buffers.slots[index].filter(|id| self.buffers.get(*id).is_some());
            let id = match live {
                Some(id) => id,
                None => {
                    let name = EchoAreaBuffers::NAMES[index];
                    let id = self.buffers.find_buffer_by_name(name).unwrap_or_else(|| {
                        let id = self.buffers.create_buffer(name);
                        let _ = self.buffers.set_buffer_local_property(
                            id,
                            "truncate-lines",
                            Value::NIL,
                        );
                        id
                    });
                    self.echo_area_buffers.slots[index] = Some(id);
                    id
                }
            };
            let _ = self.buffers.configure_buffer_undo_list(id, Value::T);
        }
    }

    /// The buffer an inactive mini-window displays the current message from.
    ///
    /// GNU reaches it through `with_echo_area_buffer', which installs it in the
    /// window for the duration of display and restores `w->contents` on unwind
    /// (src/xdisp.c:12961, :13038). Callers that need it to exist must call
    /// [`Self::ensure_echo_area_buffers`] first, exactly as GNU does.
    pub fn echo_area_display_buffer(&self) -> Option<crate::buffer::BufferId> {
        self.echo_area_buffers
            .display_slot()
            .filter(|id| self.buffers.get(*id).is_some())
    }

    pub(crate) fn append_current_message_runtime_text(&mut self, text: &str) {
        let multibyte = self
            .current_message
            .as_ref()
            .map(crate::heap_types::LispString::is_multibyte)
            .unwrap_or(true);
        let piece = crate::emacs_core::builtins::plain_str_to_lisp_string(text, multibyte);
        self.append_current_message_lisp_string(&piece);
    }

    pub(crate) fn append_current_message_lisp_string(
        &mut self,
        text: &crate::heap_types::LispString,
    ) {
        match self.current_message.as_mut() {
            Some(message) => *message = message.concat(text),
            None => self.current_message = Some(text.clone()),
        }
        let current = self.current_message.clone();
        if let Some(stored) = self.mirror_message_to_echo_area_buffer(current.as_ref()) {
            self.current_message = Some(stored);
        }
        self.invalidate_redisplay();
    }

    pub(crate) fn append_echo_area_print_runtime_text(&mut self, text: &str) {
        if !self.noninteractive() {
            self.ensure_echo_area_buffers();
        }
        if !self.message_buf_print {
            self.current_message = None;
            self.message_buf_print = true;
        }
        self.append_current_message_runtime_text(text);
    }

    /// Emacs-bytes echo-area sibling of [`Self::append_echo_area_print_runtime_text`].
    /// Used by the byte-faithful print sink (`prin1`/`print`/`write-char`) so a
    /// real Private-Use glyph in the printer output is not reinterpreted as a
    /// raw byte by the storage-string echo path (issue #131).
    pub(crate) fn append_echo_area_print_lisp_string(
        &mut self,
        text: &crate::heap_types::LispString,
    ) {
        if !self.noninteractive() {
            self.ensure_echo_area_buffers();
        }
        if !self.message_buf_print {
            self.current_message = None;
            self.message_buf_print = true;
        }
        self.append_current_message_lisp_string(text);
    }

    pub(crate) fn discard_current_message_without_clear_hook(&mut self) {
        self.message_buf_print = false;
        if self.current_message.take().is_some() {
            let _ = self.mirror_message_to_echo_area_buffer(None);
            self.invalidate_redisplay();
        }
    }

    fn clear_echo_area_message_with_hook(
        &mut self,
        run_echo_area_clear_hook: bool,
    ) -> EchoMessageClearResult {
        self.message_buf_print = false;
        if self
            .visible_variable_value_or_nil("inhibit-message")
            .is_truthy()
        {
            return EchoMessageClearResult::PreserveEchoArea;
        }

        let had_current_message = self.current_message.is_some();
        let mut called_clear_function = false;
        let mut clear_result = EchoMessageClearResult::ClearEchoArea;

        let clear_message_function = self.visible_variable_value_or_nil("clear-message-function");
        if !clear_message_function.is_nil()
            && self.gc_inhibit_depth == 0
            && self.function_value_is_callable(&clear_message_function)
        {
            called_clear_function = true;
            let specpdl_count = self.specpdl.len();
            if let Err(err) =
                self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)
            {
                tracing::warn!(
                    "inhibit-quit watcher signaled while clearing echo message: {:?}",
                    err
                );
                return EchoMessageClearResult::PreserveEchoArea;
            }
            let result = self.funcall_general(clear_message_function, vec![]);
            let result = self.unbind_to_with_result(specpdl_count, result);

            match result {
                Ok(value) if value.is_symbol_named("dont-clear-message") => {
                    clear_result = EchoMessageClearResult::PreserveEchoArea;
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(
                        "clear-message-function signaled while clearing echo message: {:?}",
                        err
                    );
                }
            }
        }

        if clear_result == EchoMessageClearResult::PreserveEchoArea {
            if called_clear_function {
                self.invalidate_redisplay();
            }
            return clear_result;
        }

        if had_current_message && run_echo_area_clear_hook {
            let hook = crate::emacs_core::hook_runtime::hook_symbol_by_id(
                self,
                echo_area_clear_hook_symbol(),
            );
            let _ = crate::emacs_core::hook_runtime::safe_run_named_hook(self, hook, &[]);
        }

        let changed = self.current_message.take().is_some();
        if changed {
            let _ = self.mirror_message_to_echo_area_buffer(None);
        }
        if changed || called_clear_function {
            self.invalidate_redisplay();
        }
        clear_result
    }

    pub(crate) fn clear_echo_area_message(&mut self) -> EchoMessageClearResult {
        self.clear_echo_area_message_with_hook(true)
    }

    /// GNU `clear_message (current_p, last_displayed_p)` (src/xdisp.c:13620) as
    /// every non-keyboard caller issues it: `clear-message-function' is
    /// consulted, `echo-area-clear-hook' is NOT run.
    ///
    /// That hook is the keyboard reader's alone -- GNU runs it only from
    /// `src/keyboard.c:1399`, `:3235` and `:3288`, never from `clear_message` --
    /// so a caller such as `read_minibuf' (src/minibuf.c:894) must not fire it.
    ///
    /// GNU's two flags select the two slots of `echo_area_buffer[2]'
    /// (src/xdisp.c:785): the current message and the last displayed one. This
    /// port keeps no last-displayed slot (ledger 215 residual 1, still open), so
    /// there is no flag to pass and none is invented: the method is named for
    /// what it does here rather than carrying a parameter that is always
    /// ignored.
    pub(crate) fn clear_echo_area_message_without_clear_hook(&mut self) -> EchoMessageClearResult {
        self.clear_echo_area_message_with_hook(false)
    }

    /// Clear a message produced by key echoing without running
    /// `echo-area-clear-hook`. GNU's `echo_update` uses
    /// `message3_nolog(nil)` -> `clear_message`, which consults
    /// `clear-message-function` but does not run the keyboard reader's
    /// separate echo-area-clear hook.
    pub(crate) fn clear_key_echo_message(&mut self) {
        self.cancel_key_echo_state();
        let _ = self.clear_echo_area_message_with_hook(false);
    }

    pub fn clear_current_message(&mut self) {
        self.cancel_key_echo_state();
        if self.clear_echo_area_message() == EchoMessageClearResult::PreserveEchoArea {}
    }

    /// Clear stale echo-area cells while an input event is being ingested,
    /// without surrendering keyboard-echo ownership. GNU `read_char` clears
    /// the old message before `echo_add_key`, then immediately rebuilds it when
    /// `immediate_echo` is active.
    pub(crate) fn clear_current_message_for_keyboard_input(&mut self) {
        if self.clear_echo_area_message() == EchoMessageClearResult::PreserveEchoArea {}
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn current_message_slot(&mut self) -> &mut Option<crate::heap_types::LispString> {
        &mut self.current_message
    }

    pub(crate) fn sync_keyboard_terminal_owner(&mut self) {
        let terminal_id = self
            .frames
            .selected_frame()
            .map(|frame| frame.terminal_id)
            .unwrap_or(crate::emacs_core::terminal::pure::TERMINAL_ID);
        self.command_loop.keyboard.select_terminal(terminal_id);
    }

    pub(crate) fn route_keyboard_input_to_frame(&mut self, emacs_frame_id: u64) {
        let frame_id = if emacs_frame_id == 0 {
            self.frames.selected_frame().map(|frame| frame.id)
        } else {
            Some(crate::window::FrameId(emacs_frame_id))
        };
        if let Some(frame_id) = frame_id {
            self.observe_keyboard_input_frame(frame_id);
        } else {
            self.command_loop
                .keyboard
                .select_terminal(crate::emacs_core::terminal::pure::TERMINAL_ID);
        }
    }

    pub(crate) fn route_tty_keyboard_input(&mut self, target: crate::keyboard::TtyInputTarget) {
        use crate::keyboard::TtyInputTarget;

        let frame_id = match target {
            TtyInputTarget::SelectedFrame => self.frames.selected_frame().map(|frame| frame.id),
            TtyInputTarget::Frame(frame_id) => self
                .frames
                .get(frame_id)
                .map(|frame| frame.id)
                .or_else(|| self.frames.selected_frame().map(|frame| frame.id)),
            TtyInputTarget::Terminal(terminal_id) => self
                .frames
                .selected_frame()
                .filter(|frame| frame.terminal_id == terminal_id)
                .map(|frame| frame.id)
                .or_else(|| self.frames.top_frame_on_terminal(terminal_id)),
        };
        if let Some(frame_id) = frame_id {
            self.observe_keyboard_input_frame(frame_id);
        }
    }

    /// Route one keyboard event to its terminal-local kboard and record the
    /// frame GNU's keyboard buffer attached to that event.  A character from a
    /// different frame must yield `switch-frame' first; otherwise a secondary
    /// TTY can execute its keystroke in whichever GUI/TTY frame was globally
    /// selected most recently.
    pub(crate) fn observe_keyboard_input_frame(&mut self, frame_id: crate::window::FrameId) {
        if let Some(frame) = self.frames.get(frame_id) {
            self.command_loop
                .keyboard
                .select_terminal(frame.terminal_id);
        }

        let selected_frame = self.frames.selected_frame().map(|frame| frame.id);
        let last_event_frame = self
            .command_loop
            .keyboard
            .kboard
            .internal_last_event_frame();
        let switching = Some(frame_id) != last_event_frame && Some(frame_id) != selected_frame;
        self.command_loop
            .keyboard
            .kboard
            .set_internal_last_event_frame(frame_id);

        let frame_value = Value::make_frame(frame_id.0);
        self.obarray
            .set_symbol_value("last-event-frame", frame_value);
        if switching || self.command_loop.keyboard.has_unread_selection_event() {
            self.command_loop
                .keyboard
                .set_unread_selection_event(Value::list(vec![
                    Value::symbol("switch-frame"),
                    frame_value,
                ]));
        }
    }

    /// Public read access to the face table.
    pub fn face_table(&self) -> &FaceTable {
        &self.face_table
    }

    /// Public mutable access to the face table.
    pub fn face_table_mut(&mut self) -> &mut FaceTable {
        &mut self.face_table
    }

    /// Refresh the render-facing face table from this frame's Lisp face
    /// vectors before redisplay.
    pub fn sync_runtime_faces_for_frame(&mut self, frame_id: crate::window::FrameId) -> bool {
        let source = (frame_id, self.face_change_count);
        if self.materialized_face_table_source == Some(source) {
            return false;
        }
        super::xfaces::sync_runtime_face_table_from_frame_lisp_faces(self, frame_id);
        self.materialized_face_table_source = Some(source);
        true
    }

    /// Set a face attribute and bump the change counter.
    /// This is the canonical way to modify face definitions at runtime.
    pub fn set_face_attribute(
        &mut self,
        face_name: &str,
        attr: crate::face::LFaceAttr,
        value: crate::face::FaceAttrValue,
    ) -> bool {
        // GNU Emacs stores the internal face ID as the symbol's `face`
        // property during `internal-make-lisp-face`.  Ensure this is set
        // so that `check-face`, `face-id`, `face-equal`, etc. work.
        let _ = super::xfaces::ensure_lisp_face_id_property(self, face_name);
        let changed = self.face_table.set_attribute(face_name, attr, value);
        if changed {
            self.face_change_count += 1;
        }
        changed
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Evaluate a Lisp expression string. Convenience for tests.
    /// Reads via the Value-native reader and evaluates via eval_sub.
    pub fn eval_str(&mut self, source: &str) -> Result<Value, EvalError> {
        crate::tagged::gc::set_tagged_heap(&mut self.tagged_heap);
        let forms = super::value_reader::read_all(source, &self.obarray).map_err(|e| {
            EvalError::signal(
                crate::emacs_core::intern::intern("error"),
                vec![Value::string(format!("Read error: {}", e.message))],
                None,
            )
        })?;
        if forms.is_empty() {
            return Ok(Value::NIL);
        }
        // Root every parsed form: each `eval_sub` call may trigger GC, and
        // the un-iterated forms still sitting in the heap-allocated Vec are
        // otherwise invisible to the exact root walk.
        let specpdl_root_scope = self.save_specpdl_roots();
        for form in &forms {
            self.push_specpdl_root(*form);
        }
        let mut result = Value::NIL;
        let mut error = None;
        for form in &forms {
            let eval_result = self.eval_sub(*form);
            match self.finalize_public_eval_result(eval_result) {
                Ok(v) => result = v,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        self.restore_specpdl_roots(specpdl_root_scope);
        match error {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }

    /// Evaluate a single Value form and return a public EvalError on failure.
    /// Evaluate a single Value form, mapping Flow errors to EvalError.
    pub fn eval_form(&mut self, form: Value) -> Result<Value, EvalError> {
        crate::tagged::gc::set_tagged_heap(&mut self.tagged_heap);
        let eval_result = self.eval_sub(form);
        self.finalize_public_eval_result(eval_result)
    }

    fn finalize_public_eval_result(&mut self, result: EvalResult) -> Result<Value, EvalError> {
        match result {
            Ok(value) => Ok(value),
            Err(Flow::Signal(sig)) => match self.dispatch_signal_if_needed(sig) {
                Ok(dispatched) => Err(map_flow(Flow::Signal(dispatched))),
                Err(flow) => Err(map_flow(flow)),
            },
            Err(flow) => Err(map_flow(flow)),
        }
    }

    /// Evaluate a runtime Value form, matching GNU Emacs's `eval_sub` in eval.c.
    ///
    /// Dispatch order (matching GNU eval.c:2552-2766):
    /// 1. Symbol → lexenv lookup or symbol-value
    /// 2. Non-cons → self-evaluating (return as-is)
    /// 3. Cons → special form / macro / function call
    pub fn eval_sub(&mut self, form: Value) -> EvalResult {
        // 1. Symbol → variable lookup (GNU eval.c:2554-2562)
        // Also unwrap symbol-with-pos when symbols-with-pos-enabled is true.
        let form_unwrapped = self.unwrap_symbol(form);
        if let Some(sym_id) = form_unwrapped.as_symbol_id() {
            // Route the variable-lookup result through the signal dispatcher so a
            // void-variable enters the debugger (debug-on-error) at signal time,
            // while dynamic bindings are still active — symmetric with the cons
            // path (eval_sub_cons) and GNU's Fsignal. `search_complete` keeps this
            // idempotent, so an already-dispatched signal is not re-dispatched.
            let result = self.eval_symbol_by_id(sym_id);
            return self.dispatch_signal_result_if_needed(result);
        }

        // 2. Non-cons → self-evaluating (GNU eval.c:2564-2565)
        if !form_unwrapped.is_cons() {
            return Ok(form_unwrapped);
        }

        self.enter_interpreted_eval_depth()?;

        let result = self.maybe_grow_eval_stack(|ctx| {
            ctx.maybe_quit_before_gc()?;
            if ctx.gc_safe_point_exact_should_collect() {
                let specpdl_root_scope = ctx.save_specpdl_roots();
                ctx.push_specpdl_root(form);
                ctx.gc_collect_from_current_roots();
                ctx.restore_specpdl_roots(specpdl_root_scope);
            }
            ctx.eval_sub_cons(form)
        });
        self.depth -= 1;
        result
    }

    /// GNU's `max_lisp_eval_depth` -- the `DEFVAR_INT` cell (`src/eval.c:4405`)
    /// that `eval_sub` dereferences on every entry (`src/eval.c:2585`).
    ///
    /// `self.max_depth` is this port's cache of that cell, kept fresh on write
    /// by [`Self::sync_cached_runtime_binding_by_id`]. A cache has no swap-in,
    /// though, so it is only GNU's cell while nothing has localised the name;
    /// `lisp/eshell/esh-mode.el` localises it deliberately. When the symbol IS
    /// localized the read names the buffer, exactly as GNU's swapped-in cell
    /// does (ledger 196). The gate is one `Vec` index and one flag byte, on a
    /// path that then dispatches a whole form.
    #[inline]
    fn current_max_lisp_eval_depth(&self) -> Option<usize> {
        if !self.obarray.is_localized(max_lisp_eval_depth_symbol()) {
            return None;
        }
        self.obarray
            .value_in_buffer(self.buffers.current_buffer(), "max-lisp-eval-depth")
            .and_then(|value| value.as_fixnum())
            // GNU raises a limit below 100 before it signals
            // (`src/eval.c:2587-2588`) so a handler has room to run.
            .map(|n| n.max(100) as usize)
    }

    fn enter_interpreted_eval_depth(&mut self) -> Result<(), Flow> {
        self.depth += 1;
        if let Some(buffer_limit) = self.current_max_lisp_eval_depth() {
            if self.depth > buffer_limit {
                let overflow_depth = self.depth as i64;
                self.depth -= 1;
                return Err(signal(
                    "excessive-lisp-nesting",
                    vec![Value::fixnum(overflow_depth)],
                ));
            }
            return Ok(());
        }
        // Refresh the cached limit by SYMBOL, not by name: `symbol_value`
        // interns its `&str` on every call, and this runs whenever the depth
        // passes the cached limit -- which is every entry once Lisp raises
        // `max-lisp-eval-depth`, not the rare event the shape suggests.
        if self.depth > self.max_depth
            && let Some(v) = self.obarray.symbol_value_id(max_lisp_eval_depth_symbol())
            && let Some(n) = v.as_fixnum()
        {
            let new_max = n.max(100) as usize;
            if new_max != self.max_depth {
                self.max_depth = new_max;
            }
        }
        if self.depth > self.max_depth {
            let overflow_depth = self.depth as i64;
            self.depth -= 1;
            return Err(signal(
                "excessive-lisp-nesting",
                vec![Value::fixnum(overflow_depth)],
            ));
        }
        Ok(())
    }

    fn eval_sub_cons(&mut self, form: Value) -> EvalResult {
        let original_fun = self.unwrap_symbol(form.cons_car());
        let original_args = form.cons_cdr();

        // GNU eval.c:2583-2585 records an UNEVALLED backtrace frame on
        // every `eval_sub` cons-form evaluation. The frame starts in
        // UNEVALLED shape holding the surface function symbol and the
        // raw argument-form cons list, then transitions to EVALD in
        // place via `set_backtrace_args` once arguments have been
        // evaluated (eval.c:2638, 2660, 3299). Special forms leave
        // the frame UNEVALLED throughout.
        let outer_bt_count = self.specpdl.len();
        self.push_unevalled_backtrace_frame(original_fun, original_args);
        // GNU eval.c:2601-2602, immediately after `record_in_backtrace` and
        // before any dispatch: `if (debug_on_next_call) do_debug_on_call (Qt,
        // count)`.  Taking the arm IS the disarm (see `debug_on_call`), and
        // the same call flags this frame's `debug_on_exit`.
        let dispatch_result = match self.take_debug_on_call_arm(DebugOnCallCode::EvalForm) {
            Some(arm) => self.do_debug_on_call(arm).and_then(|()| {
                self.eval_sub_cons_dispatch(original_fun, original_args, outer_bt_count)
            }),
            None => self.eval_sub_cons_dispatch(original_fun, original_args, outer_bt_count),
        };
        let result = self.dispatch_signal_result_if_needed(dispatch_result);
        self.record_sequence_temp_roots_from_backtrace(outer_bt_count);
        self.unbind_to_with_result(outer_bt_count, result)
    }

    fn eval_sub_cons_dispatch(
        &mut self,
        original_fun: Value,
        original_args: Value,
        outer_bt_count: usize,
    ) -> EvalResult {
        // Resolve function (GNU eval.c:2600-2605)
        let sym_id = original_fun.as_symbol_id();

        // Keep only evaluator-internal literal forms on the pre-resolution
        // fast path. GNU decides public special-form dispatch from the
        // function cell's UNEVALLED subr, so user-visible special forms
        // should flow through the resolved subr surface below.
        if let Some(sym_id) = sym_id
            && matches!(
                sym_id,
                id if id == lambda_symbol()
                    || id == byte_code_literal_symbol()
                    || id == byte_code_symbol()
            )
            && let Some(result) = self.try_special_form_value_id(sym_id, original_args)
        {
            return result;
        }

        // Resolve function value
        let func = if let Some(sym_id) = sym_id {
            if let Some(override_func) = self
                .compiler_function_overrides_active()
                .then(|| compiler_function_override_in_obarray(&self.obarray, sym_id))
                .flatten()
            {
                override_func
            } else {
                match self.obarray.symbol_function_id(sym_id) {
                    Some(f) => {
                        let mut f = f;
                        // Follow symbol indirection (GNU eval.c:2604)
                        if let Some(alias_id) = f.as_symbol_id()
                            && let Some(resolved) = self.obarray.indirect_function_id(alias_id)
                        {
                            f = resolved;
                        }
                        loop {
                            if !super::autoload::is_autoload_value(&f) {
                                break f;
                            }

                            match self.load_named_autoload_call_step(sym_id, f)? {
                                NamedAutoloadCallStep::RetrySymbol { autoload_form } => {
                                    // GNU `eval_sub` jumps back to named
                                    // function resolution after each autoload
                                    // hop.  The returned form is the current
                                    // indirect function cell for that symbol.
                                    f = autoload_form;
                                }
                                NamedAutoloadCallStep::DispatchFunction { function } => {
                                    break function;
                                }
                                NamedAutoloadCallStep::Void => {
                                    return Err(signal(
                                        LispCondition::VoidFunction,
                                        vec![original_fun],
                                    ));
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::VoidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ));
                    }
                }
            }
        } else {
            // GNU eval_sub runs every non-symbol function position through
            // Ffunction(list1(fun)).  `function` only transforms literal
            // `(lambda ...)` forms; byte-code objects, subrs, and malformed
            // values are quoted through to the normal callable validation
            // below.
            if original_fun.is_cons() && cons_head_symbol_id(&original_fun) == Some(lambda_symbol())
            {
                self.instantiate_callable_cons_form(original_fun)?
            } else {
                original_fun
            }
        };

        if let Some(surface_sym_id) = sym_id
            && let Some(target_sym_id) = func.as_subr_id()
            && self.subr_is_special_form_id(target_sym_id)
        {
            // GNU eval.c:2624 runs `list_length (args_left)` for *every*
            // SUBRP `fun` — including UNEVALLED special forms — BEFORE
            // dispatching to the special-form C function. `list_length`
            // ends in `CHECK_LIST_END`, so an improper top-level argument
            // list (e.g. `(progn a . b)`, `(if t a . b)`, `(when t . b)`)
            // signals `(wrong-type-argument listp BAD-CDR)` up front,
            // *before* any body form is evaluated. Neo otherwise validated
            // lazily and evaluated the first element first (wrong error /
            // no error). Match GNU: validate the arg-list structure here.
            if list_length(&original_args).is_none() {
                return Err(self.listp_error(original_args));
            }
            // The outer eval_sub_cons UNEVALLED frame (pushed by the
            // wrapper) already records the surface function and raw
            // argument forms. Special forms leave the frame UNEVALLED
            // throughout (no `set_backtrace_args_evalled` call),
            // matching GNU eval.c:2618-2619.
            let result = if surface_sym_id == target_sym_id {
                self.try_special_form_value_id(surface_sym_id, original_args)
            } else {
                self.try_aliased_special_form_value_id(surface_sym_id, target_sym_id, original_args)
            };
            if let Some(result) = result {
                return result;
            }
        }

        // Check for macro (GNU eval.c:2730-2755)
        if func.is_macro() {
            // GNU expands a macro via `apply1 (Fcdr (fun), original_args)`
            // (eval.c:2766), and `apply1` -> `Fapply` -> `list_length`
            // (eval.c:3065/fns.c:115) validates the argument-list structure
            // up front. An improper macro-call tail (e.g. `(when t . b)`)
            // therefore signals `(wrong-type-argument listp BAD-CDR)` rather
            // than silently dropping the bad cdr. `value_list_to_values`
            // walks lazily and would otherwise swallow the improper tail.
            if list_length(&original_args).is_none() {
                return Err(self.listp_error(original_args));
            }
            let arg_values = value_list_to_values(&original_args);
            let bt_count = self.specpdl.len();
            self.push_backtrace_frame(original_fun, &arg_values);
            let expanded =
                self.with_macro_expansion_scope(|eval| eval.apply_lambda(func, arg_values));
            let expanded = self.unbind_to_with_result(bt_count, expanded);
            let expanded = expanded?;
            let expanded_root_count = self.specpdl.len();
            self.push_specpdl_root(expanded);
            let result = self.eval_sub(expanded);
            return self.unbind_to_with_result(expanded_root_count, result);
        }
        if cons_head_symbol_id(&func) == Some(macro_symbol()) {
            // Cons-cell macro: (macro . fn) — GNU eval.c:2730
            // Same up-front `apply1`/`list_length` validation as the
            // `func.is_macro()` branch above (GNU eval.c:2766).
            if list_length(&original_args).is_none() {
                return Err(self.listp_error(original_args));
            }
            let macro_fn = func.cons_cdr();
            let arg_values = value_list_to_values(&original_args);
            let bt_count = self.specpdl.len();
            self.push_backtrace_frame(original_fun, &arg_values);
            let expanded = self.with_macro_expansion_scope(|eval| eval.apply(macro_fn, arg_values));
            let expanded = self.unbind_to_with_result(bt_count, expanded);
            let expanded = expanded?;
            let expanded_root_count = self.specpdl.len();
            self.push_specpdl_root(expanded);
            let result = self.eval_sub(expanded);
            return self.unbind_to_with_result(expanded_root_count, result);
        }

        // GNU eval.c:2606-2614: for SUBRP `fun`, check arity
        // against the raw `original_args` count BEFORE any arg
        // evaluation, and on mismatch signal
        // `(wrong-number-of-arguments original_fun numargs)` where
        // `original_fun` is the XCAR of the form (the surface
        // symbol, not the resolved subr value). This is how GNU
        // gets `(wrong-number-of-arguments car 0)` for a direct
        // `(car)` call -- the arity check runs inline in eval_sub
        // and never reaches `funcall_subr` which would have emitted
        // `#<subr car>` via `XSETSUBR`.
        //
        // For non-subrs (closures, bytecode, lambdas, cons forms)
        // the dispatch falls through to the normal apply path,
        // which signals with `fun` itself -- also matching GNU
        // funcall_lambda and funcall_subr.
        // GNU keeps the resolved XSUBR in `fun` across argument
        // evaluation and calls it directly. Preserve the SubrEntry we
        // resolved for the direct eval_sub arity check instead of
        // looking it up again after evaluating args.
        let direct_subr_entry = if let Some((sym_id, entry)) = subr_entry_from_value(func) {
            if entry.dispatch_kind != SubrDispatchKind::SpecialForm {
                let numargs = match list_length(&original_args) {
                    Some(n) => n,
                    None => return Err(self.listp_error(original_args)),
                };
                let min = entry.min_args as usize;
                let max_ok = match entry.max_args {
                    Some(m) => numargs <= m as usize,
                    None => true, // &rest / MANY
                };
                if numargs < min || !max_ok {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![original_fun, Value::fixnum(numargs as i64)],
                    ));
                }
                Some((sym_id, entry))
            } else {
                None
            }
        } else {
            None
        };

        // GNU eval.c:2716-2726: when `fun` is not a subr, closure,
        // bytecode, or cons-shaped lambda/autoload/macro, signal
        // `(invalid-function original_fun)` with the SURFACE
        // symbol. Verified against emacs 31.0.50:
        //   (fset 'vm-fsetint 1)
        //   (condition-case e (vm-fsetint) (error e))
        //     → (invalid-function vm-fsetint)
        //
        // The check runs inline in eval_sub so the dispatcher
        // `funcall_general` never sees the invalid value and
        // never emits the resolved fncell contents as signal data.
        if !self.function_value_is_callable(&func) {
            if func.is_nil() {
                return Err(signal(LispCondition::VoidFunction, vec![original_fun]));
            }
            return Err(signal(LispCondition::InvalidFunction, vec![original_fun]));
        }

        // Regular function call: evaluate args, promote the outer
        // UNEVALLED frame to EVALD in place, then dispatch directly.
        // Matches GNU `eval_sub` non-UNEVALLED SUBRP path
        // (eval.c:2631-2640) and CLOSUREP → apply_lambda
        // (eval.c:2715, 3292-3300) which both mutate the outer
        // record_in_backtrace entry via `set_backtrace_args`.
        //
        // `func` and each evaluated arg are rooted on the specpdl via
        // `push_specpdl_root`. GNU relies on conservative stack
        // scanning of `SAFE_ALLOCA_LISP (vals, numargs)` plus the
        // `fun` C local; neomacs uses exact GC, so a local
        // `Vec<Value>` and the Rust-local `func` Value are invisible
        // to the tracer.
        //
        // `func` is rooted BEFORE the arg loop so it survives GC
        // triggered by any arg evaluator, and stays rooted through
        // `funcall_general_untraced` below -- it only gets popped by
        // the outer `eval_sub_cons` `unbind_to(outer_bt_count)`. This
        // is specifically needed when `original_fun` is a cons
        // (lambda-literal head): the resolved Lambda Value lives only
        // on the Rust stack, and the outer UNEVALLED frame records
        // `original_fun`, not `func`.
        //
        // Per-arg roots are popped once `set_backtrace_args_evalled`
        // transfers ownership to the outer frame's args slot.
        // GNU uses SAFE_ALLOCA_LISP for evaluated arguments here. Keep the
        // common arities inline instead of allocating a heap Vec per call.
        // GNU validates the argument-list structure UP FRONT, before
        // evaluating any argument: the subr path runs a single
        // `list_length (args_left)` (eval.c:2624) and `apply_lambda` runs
        // `list_length (args)` (eval.c:3302). Both end in `CHECK_LIST_END`,
        // so an improper arg list (e.g. `((lambda (a &rest b) b) x . y)`)
        // signals `(wrong-type-argument listp BAD-CDR)` *before* `x` is ever
        // evaluated. Neo previously evaluated args lazily and only checked
        // the tail afterwards, leaking a void-variable error for `x` first.
        // Subrs already walked the spine once for the arity check above
        // (`direct_subr_entry` is only Some when that walk returned a
        // length), so re-walking here would make the spine cost 3x per
        // interpreted subr call where GNU pays 1x + the eval walk. Only
        // the closure/bytecode/lambda paths still need the up-front walk.
        if direct_subr_entry.is_none() && list_length(&original_args).is_none() {
            return Err(self.listp_error(original_args));
        }
        let mut args = LispArgVec::new();
        self.push_specpdl_root(func);
        let args_roots_base = self.specpdl.len();
        let mut cursor = original_args;
        while cursor.is_cons() {
            let arg_form = cursor.cons_car();
            let arg_val = self.eval_sub(arg_form)?;
            self.push_specpdl_root(arg_val);
            args.push(arg_val);
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(cursor));
        }
        if let Some((sym_id, entry)) = direct_subr_entry
            && Self::subr_entry_uses_fixed_value_call(entry)
        {
            self.set_backtrace_args_evalled_owned(outer_bt_count, args);

            let result = self.maybe_grow_eval_stack(|ctx| {
                ctx.dispatch_subr_entry_from_backtrace_unchecked(entry, outer_bt_count)
                    .unwrap_or_else(|| {
                        Err(signal(
                            LispCondition::VoidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    })
            });
            return self.unbind_to_with_result(args_roots_base, result);
        }

        self.set_backtrace_args_evalled(outer_bt_count, &args);

        if let Some((sym_id, entry)) = direct_subr_entry {
            let result = self.maybe_grow_eval_stack(|ctx| {
                if entry.dispatch_kind == SubrDispatchKind::ContextCallable {
                    return ctx.apply_evaluator_callable_by_id(sym_id, args);
                }
                ctx.dispatch_subr_entry_unchecked(entry, args)
                    .unwrap_or_else(|| {
                        Err(signal(
                            LispCondition::VoidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    })
            });
            return self.unbind_to_with_result(args_roots_base, result);
        }

        let result = self.maybe_grow_eval_stack(|ctx| ctx.funcall_general_untraced(func, args));
        self.unbind_to_with_result(args_roots_base, result)
    }

    /// Legacy eval_value: delegates to eval_sub.
    pub fn eval_value(&mut self, value: &Value) -> EvalResult {
        self.eval_sub(*value)
    }

    /// Evaluate all forms in a source string and return per-form results.
    /// Uses the Value-native reader.
    pub fn eval_str_each(&mut self, source: &str) -> Vec<Result<Value, EvalError>> {
        crate::tagged::gc::set_tagged_heap(&mut self.tagged_heap);
        let forms = match super::value_reader::read_all(source, &self.obarray) {
            Ok(f) => f,
            Err(e) => {
                return vec![Err(EvalError::signal(
                    intern("error"),
                    vec![Value::string(format!("Read error: {}", e.message))],
                    None,
                ))];
            }
        };
        // Root every parsed form upfront. The previous version only rooted
        // successful results; un-iterated parsed forms still sitting in the
        // heap-allocated Vec were otherwise invisible to exact GC.
        let specpdl_root_scope = self.save_specpdl_roots();
        for form in &forms {
            self.push_specpdl_root(*form);
        }
        let mut results = Vec::with_capacity(forms.len());
        for form in &forms {
            let result = self.eval_sub(*form).map_err(map_flow);
            if let Ok(ref val) = result {
                self.push_specpdl_root(*val);
            }
            results.push(result);
        }
        self.restore_specpdl_roots(specpdl_root_scope);
        results
    }

    /// Set a global variable.
    pub fn set_variable(&mut self, name: &str, value: Value) {
        let sym_id = intern(name);
        self.note_macro_expansion_mutation();
        // GNU set_internal (data.c:1762) for SYMBOL_FORWARDED routes
        // the write through `store_symval_forwarding` which for the
        // BUFFER_OBJFWD arm writes to the current buffer's slot.
        // Mirror that here so callers like
        // `obarray.set_symbol_value("default-directory", ...)`
        // (and the test surface that uses set_variable) actually
        // update the visible per-buffer slot rather than just the
        // obarray symbol value (which a FORWARDED symbol no longer
        // consults at read time).
        use super::symbol::SymbolRedirect;
        if let Some(sym) = self.obarray.get_by_id(sym_id)
            && sym.flags.redirect() == SymbolRedirect::Forwarded
            && let Some(buf_id) = self.buffers.current_buffer_id()
        {
            use super::forward::{LispBufferObjFwd, LispFwdType};
            // Safety: install_buffer_objfwd leaks a 'static
            // descriptor; the symbol's redirect tag and val.fwd
            // pointer are immutable once installed.
            let fwd_ptr = unsafe { sym.val.fwd };
            let header = unsafe { &*fwd_ptr };
            if matches!(header.ty, LispFwdType::BufferObj) {
                let buf_fwd = unsafe { &*(fwd_ptr as *const LispBufferObjFwd) };
                let offset = buf_fwd.offset as usize;
                if let Some(buf) = self.buffers.get_mut(buf_id)
                    && offset < buf.slots.len()
                {
                    buf.slots[offset] = value;
                    self.refresh_gc_runtime_settings_after_change_by_id(sym_id);
                    self.mark_redisplay_dirty_if_display_var(sym_id);
                    return;
                }
            }
        }
        self.obarray.set_symbol_value(name, value);
        self.sync_cached_runtime_binding_by_id(sym_id, value);
        self.refresh_gc_runtime_settings_after_change_by_id(sym_id);
        self.mark_redisplay_dirty_if_display_var(sym_id);
    }

    #[inline]
    pub(crate) fn noninteractive(&self) -> bool {
        self.noninteractive
    }

    /// If `symbols-with-pos-enabled` and `val` is a symbol-with-pos,
    /// return the bare symbol. Otherwise return `val` unchanged.
    #[inline]
    pub fn unwrap_symbol(&self, val: Value) -> Value {
        if self.symbols_with_pos_enabled && val.is_symbol_with_pos() {
            val.as_symbol_with_pos_sym().unwrap()
        } else {
            val
        }
    }

    pub(crate) fn sync_thread_runtime_bindings(&mut self) {
        if let Some(main_thread) = self.threads.thread_handle(0) {
            // thread.c:1307 DEFVAR_LISP -- GNU installs the main thread object
            // (and DEFVAR specialness) at C init.
            self.obarray
                .define_special_variable("main-thread", main_thread);
        }
    }

    /// Set a function binding.
    pub fn set_function(&mut self, name: &str, value: Value) {
        self.note_macro_expansion_mutation();
        self.obarray.set_symbol_function(name, value);
    }

    /// GNU `do_symval_forwarding` (`src/data.c:1337-1360`) for the descriptor
    /// variants whose storage is the descriptor itself: `Lisp_Intfwd`,
    /// `Lisp_Boolfwd` and `Lisp_Objfwd` are one load each.
    /// `Lisp_Kboard_Objfwd` resolves in keyboard context and
    /// `Lisp_Buffer_Objfwd` in buffer context, so both answer `None` and
    /// leave their reads to [`Self::forwarded_buffer_obj_value`] and the
    /// full walk.
    #[inline]
    fn forwarded_descriptor_value(
        &self,
        sym: &crate::emacs_core::symbol::LispSymbol,
    ) -> Option<Value> {
        use crate::emacs_core::forward::LispFwdType;

        let fwd = unsafe { &*sym.val.fwd };
        match fwd.ty {
            LispFwdType::Int | LispFwdType::Bool | LispFwdType::Obj => fwd.load(),
            LispFwdType::BufferObj | LispFwdType::KboardObj => None,
        }
    }

    #[inline]
    fn forwarded_buffer_obj_value(
        &self,
        sym: &crate::emacs_core::symbol::LispSymbol,
    ) -> Option<Value> {
        use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};

        let fwd = unsafe { &*sym.val.fwd };
        if !matches!(fwd.ty, LispFwdType::BufferObj) {
            return None;
        }

        let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
        let slot = crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)?;
        let off = slot.index();
        if let Some(buf) = self.buffers.current_buffer() {
            let local = buf_fwd.local_flags_idx < 0 || buf.slot_local_flag(slot);
            if local && off < buf.slots.len() {
                return Some(buf.slots[off]);
            }
        }

        if off < self.buffers.buffer_defaults.len() {
            Some(self.buffers.buffer_defaults[off])
        } else {
            Some(buf_fwd.default)
        }
    }

    pub(crate) fn set_buffer_local_binding_by_id(
        &mut self,
        buffer_id: crate::buffer::BufferId,
        sym_id: SymId,
        value: Value,
    ) -> Result<(), Flow> {
        let resolved = builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id)?;
        if crate::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved).is_some()
            || resolved == buffer_undo_list_symbol()
        {
            let _ = self
                .buffers
                .set_buffer_local_property_by_sym_id(buffer_id, resolved, value);
            // Finding 6: `setq-local`/`set` on a display-affecting slot.
            self.mark_redisplay_dirty_if_display_var(resolved);
            return Ok(());
        }

        if !self.obarray.get_by_id(resolved).is_some_and(|sym| {
            sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Localized
        }) {
            let default = self
                .obarray
                .find_symbol_value(resolved)
                .unwrap_or(Value::UNBOUND);
            self.obarray.make_symbol_localized(resolved, default);
        }

        let _ = self
            .buffers
            .set_buffer_local_property_by_sym_id(buffer_id, resolved, value);
        let target_buf = Value::make_buffer(buffer_id);
        let alist = self
            .buffers
            .get(buffer_id)
            .map(|buf| buf.local_var_alist_value())
            .unwrap_or(Value::NIL);
        let _ = self.obarray.find_symbol_value_in_buffer(
            resolved,
            Some(buffer_id),
            target_buf,
            alist,
            None,
            0,
            None,
        );
        // Finding 6: a LOCALIZED display var set buffer-locally.
        self.mark_redisplay_dirty_if_display_var(resolved);
        Ok(())
    }

    /// Look up a symbol by its SymId without deciding that an unbound cell is
    /// an error. Uses the SymId directly for lexenv lookup (preserving
    /// uninterned symbol identity, like Emacs's EQ-based Fassq on
    /// Vinternal_interpreter_environment).
    pub(crate) fn lookup_symbol_value_by_id(
        &self,
        sym_id: SymId,
    ) -> Result<SymbolValueLookup, Flow> {
        // GNU eval.c checks the lexenv for the ORIGINAL symbol BEFORE
        // resolving variable aliases and does not rescan declared-special
        // flags on ordinary reads. Declared-special affects how bindings are
        // created, not whether an existing lexical cell is readable.
        if self.lexical_binding()
            && let Some(value) = self.lexenv_lookup_cached_in(self.lexenv, sym_id)
        {
            return Ok(SymbolValueLookup::Bound(value));
        }
        self.find_symbol_value_by_id(sym_id)
    }

    /// GNU `find_symbol_value`: the dynamic value alone, no lexenv consult.
    /// Internal state reads (search options, change hooks) call this — a
    /// DEFVAR'd special can never have a lexical cell, so the probe
    /// `lookup_symbol_value_by_id` runs first is pure per-read cost there.
    /// Value of a variable known to be special (a `defvar`/`DEFVAR_*`
    /// symbol): GNU reads these through `find_symbol_value`, never through a
    /// lexical environment, so the `eval_symbol_by_id` lexenv scan is skipped.
    /// `None` when void.
    pub(crate) fn special_variable_value_by_id(&self, sym_id: SymId) -> Option<Value> {
        match self.find_symbol_value_by_id(sym_id) {
            Ok(SymbolValueLookup::Bound(value)) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn find_symbol_value_by_id(&self, sym_id: SymId) -> Result<SymbolValueLookup, Flow> {
        // Fast path — GNU `find_symbol_value`'s SYMBOL_PLAINVAL leaf: an
        // ordinary global with a bound plain cell answers from one slot
        // read. Aliases, localized/forwarded symbols, unbound cells (where
        // the keyword/t/nil fallbacks below may still bind), and
        // `buffer-undo-list` (stored in SharedUndoState, special-cased
        // below) all fall through to the full walk. Everything the slow
        // path consults per read (keyword memo, alias resolution, canonical
        // checks) is per-call TLS traffic this leaf never needs.
        if sym_id != buffer_undo_list_symbol()
            && let Some(sym) = self.obarray.get_by_id(sym_id)
        {
            match sym.redirect() {
                crate::emacs_core::symbol::SymbolRedirect::Plainval => {
                    let value = unsafe { sym.val.plain };
                    if !value.is_unbound() {
                        return Ok(SymbolValueLookup::Bound(value));
                    }
                }
                // Localized/Forwarded leaves: a non-Varalias symbol IS its own
                // alias resolution, so the per-read alias walk + canonical
                // checks below are pure overhead for these (GNU
                // `find_symbol_value` dispatches on the redirect tag
                // directly). `read_localized`'s same-buffer epoch check makes
                // the common read one compare + one cdr.
                crate::emacs_core::symbol::SymbolRedirect::Localized => {
                    if let Some(buf) = self.buffers.current_buffer() {
                        if let Some(value) = self.obarray.read_localized_for_buffer(
                            sym_id,
                            buf.id,
                            buf.local_var_alist_value(),
                        ) {
                            if value.is_unbound() {
                                return Ok(SymbolValueLookup::Unbound);
                            }
                            return Ok(SymbolValueLookup::Bound(value));
                        }
                    }
                }
                crate::emacs_core::symbol::SymbolRedirect::Forwarded => {
                    if let Some(value) = self.forwarded_buffer_obj_value(sym) {
                        return Ok(SymbolValueLookup::Bound(value));
                    }
                    // The remaining descriptor variants are GNU's one-load
                    // `do_symval_forwarding` cases.  Without them every read
                    // of a C-defined global (`char-script-table`,
                    // `inhibit-changing-match-data`, `default-text-properties`
                    // ...) walked the keyword memo, the alias chain and the
                    // canonical checks to reach the same load.
                    if let Some(value) = self.forwarded_descriptor_value(sym) {
                        return Ok(SymbolValueLookup::Bound(value));
                    }
                }
                crate::emacs_core::symbol::SymbolRedirect::Varalias => {}
            }
        }

        // GNU keywords are self-valued constants installed by `intern_sym`;
        // keep lexenv lookup first, then use the same self-value directly.
        if is_keyword_id(sym_id) {
            return Ok(SymbolValueLookup::Bound(Value::from_kw_id(sym_id)));
        }

        let resolved = super::builtins::resolve_variable_alias_id(self, sym_id)?;

        if resolved != sym_id && is_keyword_id(resolved) {
            return Ok(SymbolValueLookup::Bound(Value::from_kw_id(resolved)));
        }

        use crate::emacs_core::symbol::SymbolRedirect;
        if let Some(sym) = self.obarray.get_by_id(resolved) {
            match sym.redirect() {
                // GNU `find_symbol_value` switches on the symbol
                // redirect tag and only walks `local_var_alist` for
                // `SYMBOL_LOCALIZED`.
                SymbolRedirect::Localized => {
                    if let Some(buf) = self.buffers.current_buffer() {
                        if let Some(value) = self.obarray.read_localized_for_buffer(
                            resolved,
                            buf.id,
                            buf.local_var_alist_value(),
                        ) {
                            if value.is_unbound() {
                                return Ok(SymbolValueLookup::Unbound);
                            }
                            return Ok(SymbolValueLookup::Bound(value));
                        }
                    }
                }
                SymbolRedirect::Forwarded => {
                    if let Some(value) = self.forwarded_buffer_obj_value(sym) {
                        return Ok(SymbolValueLookup::Bound(value));
                    }
                }
                SymbolRedirect::Plainval | SymbolRedirect::Varalias => {}
            }
        }

        // Neomacs still stores `buffer-undo-list` in SharedUndoState
        // rather than a BUFFER_OBJFWD slot. Keep that special storage
        // out of the generic symbol-read path so ordinary PLAINVAL
        // symbols do not scan `local_var_alist`.
        if resolved == buffer_undo_list_symbol()
            && is_canonical_id(resolved)
            && let Some(buf) = self.buffers.current_buffer()
            && let Some(binding) = buf.get_buffer_local_binding_by_sym_id(resolved)
        {
            return Ok(match binding.as_value() {
                Some(value) => SymbolValueLookup::Bound(value),
                None => SymbolValueLookup::Unbound,
            });
        }

        // Obarray value cell. Use `find_symbol_value` (not the
        // legacy `symbol_value_id`) so FORWARDED reads land on the
        // forwarder descriptor's default rather than returning None
        // and signalling void-variable.
        if let Some(value) = self.obarray.find_symbol_value(resolved) {
            return Ok(SymbolValueLookup::Bound(value));
        }

        // Task #36: canonical constant fallback. When `t` / `nil`
        // aren't explicitly stored in the obarray and aren't
        // specbound, they resolve to their canonical values.
        // Mirrors the vm.rs `lookup_var` fallback path.
        if sym_id == nil_symbol() && is_canonical_id(sym_id) {
            return Ok(SymbolValueLookup::Bound(Value::NIL));
        }
        if sym_id == t_symbol() && is_canonical_id(sym_id) {
            return Ok(SymbolValueLookup::Bound(Value::T));
        }
        if resolved == nil_symbol() && is_canonical_id(resolved) {
            return Ok(SymbolValueLookup::Bound(Value::NIL));
        }
        if resolved == t_symbol() && is_canonical_id(resolved) {
            return Ok(SymbolValueLookup::Bound(Value::T));
        }

        Ok(SymbolValueLookup::Unbound)
    }

    /// Lisp-visible variable evaluation: unlike optional internal state reads,
    /// an unbound cell signals `void-variable`.
    pub(crate) fn eval_symbol_by_id(&self, sym_id: SymId) -> EvalResult {
        match self.lookup_symbol_value_by_id(sym_id)? {
            SymbolValueLookup::Bound(value) => Ok(value),
            SymbolValueLookup::Unbound => Err(signal(
                LispCondition::VoidVariable,
                vec![value_from_symbol_id(sym_id)],
            )),
        }
    }

    pub(crate) fn eval_symbol(&self, symbol: &str) -> EvalResult {
        self.eval_symbol_by_id(intern(symbol))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn apply_symbol_callable(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        if super::builtins::is_canonical_symbol_id(sym_id) {
            let invalid_fn = if self.subr_is_special_form_id(sym_id) {
                Value::subr_from_sym_id(sym_id)
            } else {
                value_from_symbol_id(sym_id)
            };
            return self.apply_named_callable_by_id(
                sym_id,
                args,
                invalid_fn,
                rewrite_builtin_wrong_arity,
            );
        }

        if self.obarray.is_function_unbound_id(sym_id) {
            return Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            ));
        }

        let Some(function) = self.obarray.symbol_function_id(sym_id) else {
            return Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            ));
        };

        // Handle autoloads for non-canonical symbols the same as canonical
        // ones: trigger autoload-do-load before calling apply, so the raw
        // autoload cons never reaches apply_inner's Value::Cons path.
        if super::autoload::is_autoload_value(&function) {
            let name = resolve_sym(sym_id);
            return self.apply_named_autoload_callable(
                name,
                function,
                args,
                rewrite_builtin_wrong_arity,
            );
        }

        let function_is_callable = self.function_value_is_callable(&function);
        let result = self.apply_untraced(function, args);
        match &result {
            Err(Flow::Signal(sig))
                if !function_is_callable && sig.symbol == invalid_function_symbol() =>
            {
                Err(signal(
                    LispCondition::InvalidFunction,
                    vec![Value::from_sym_id(sym_id)],
                ))
            }
            _ => result,
        }
    }

    fn apply_symbol_callable_untraced(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        if super::builtins::is_canonical_symbol_id(sym_id) {
            return self.apply_symbol_callable_untraced_resolved_id(
                sym_id,
                args,
                rewrite_builtin_wrong_arity,
            );
        }

        if self.obarray.is_function_unbound_id(sym_id) {
            return Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            ));
        }

        let Some(function) = self.obarray.symbol_function_id(sym_id) else {
            return Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            ));
        };

        if super::autoload::is_autoload_value(&function) {
            let name = resolve_sym(sym_id);
            return self.apply_named_autoload_callable(
                name,
                function,
                args,
                rewrite_builtin_wrong_arity,
            );
        }

        let function_is_callable = self.function_value_is_callable(&function);
        let result = self.funcall_general_untraced(function, args);
        match &result {
            Err(Flow::Signal(sig))
                if !function_is_callable && sig.symbol == invalid_function_symbol() =>
            {
                Err(signal(
                    LispCondition::InvalidFunction,
                    vec![Value::from_sym_id(sym_id)],
                ))
            }
            _ => result,
        }
    }

    fn apply_symbol_callable_untraced_resolved_id(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        match self.resolve_named_call_target_by_id(sym_id) {
            NamedCallTarget::Obarray(func) => {
                if super::autoload::is_autoload_value(&func) {
                    return self.apply_named_autoload_callable_by_id(
                        sym_id,
                        func,
                        args,
                        rewrite_builtin_wrong_arity,
                    );
                }
                let function_is_callable = self.function_value_is_callable(&func);
                let result = self.funcall_general_untraced(func, args);
                match &result {
                    Err(Flow::Signal(sig))
                        if !function_is_callable && sig.symbol == invalid_function_symbol() =>
                    {
                        Err(signal(
                            LispCondition::InvalidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    }
                    _ => result,
                }
            }
            NamedCallTarget::Subr(func) => {
                let Some((sym_id, entry)) = subr_entry_from_value(func) else {
                    return Err(signal(
                        LispCondition::InvalidFunction,
                        vec![Value::from_sym_id(sym_id)],
                    ));
                };
                if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
                    return Err(signal(LispCondition::InvalidFunction, vec![func]));
                }
                self.apply_subr_object_with_entry(sym_id, func, args, entry)
            }
            NamedCallTarget::Void => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            )),
        }
    }

    pub(crate) fn function_value_is_callable(&self, function: &Value) -> bool {
        match function.kind() {
            ValueKind::Veclike(VecLikeType::Lambda)
            | ValueKind::Veclike(VecLikeType::ByteCode)
            | ValueKind::Veclike(VecLikeType::Macro)
            | ValueKind::Veclike(VecLikeType::ModuleFunction) => true,
            ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
                super::subr_info::subr_is_callable_function_value(function)
            }
            ValueKind::Cons => {
                super::autoload::is_autoload_value(function)
                    || matches!(
                        cons_head_symbol_id(function),
                        Some(id) if id == lambda_symbol() || id == macro_symbol()
                    )
            }
            ValueKind::Symbol(id) => {
                super::builtins::symbols::resolve_indirect_symbol_by_id(self, id)
                    .is_some_and(|(_, resolved)| self.function_value_is_callable(&resolved))
            }
            _ => false,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn maybe_writeback_mutating_first_arg(
        &mut self,
        called_name: &str,
        alias_target: Option<&str>,
        call_args: &[Value],
        result: &Value,
    ) {
        let mutates_fillarray =
            called_name == "fillarray" || alias_target.is_some_and(|name| name == "fillarray");
        let mutates_aset = called_name == "aset" || alias_target.is_some_and(|name| name == "aset");
        if !mutates_fillarray && !mutates_aset {
            return;
        }
        let Some(first_arg) = call_args.first() else {
            return;
        };
        if !first_arg.is_string() {
            return;
        }

        let replacement = if mutates_fillarray {
            if !result.is_string() || eq_value(first_arg, result) {
                return;
            }
            *result
        } else {
            if call_args.len() < 3 {
                return;
            }
            let Ok(updated) =
                super::builtins::aset_string_replacement(first_arg, &call_args[1], &call_args[2])
            else {
                return;
            };
            if eq_value(first_arg, &updated) {
                return;
            }
            updated
        };

        if crate::emacs_core::value::equal_value(first_arg, &replacement, 0) {
            return;
        }

        let mut visited = HashSet::new();
        // Walk the lexenv cons alist and replace alias refs in binding values
        {
            let mut lexenv_val = self.lexenv;
            Self::replace_alias_refs_in_value(
                &mut lexenv_val,
                first_arg,
                &replacement,
                &mut visited,
            );
            self.lexenv = lexenv_val;
        }
        // Dynamic bindings are now in the obarray (via specbind), so
        // the obarray iteration below handles them.
        if let Some(current_id) = self.buffers.current_buffer_id()
            && let Some(buf) = self.buffers.get_mut(current_id)
        {
            for value in buf.bound_buffer_local_values_mut() {
                Self::replace_alias_refs_in_value(value, first_arg, &replacement, &mut visited);
            }
        }

        self.obarray.for_each_value_cell_mut(|value| {
            Self::replace_alias_refs_in_value(value, first_arg, &replacement, &mut visited);
        });
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn replace_alias_refs_in_value(
        value: &mut Value,
        from: &Value,
        to: &Value,
        visited: &mut HashSet<usize>,
    ) {
        if eq_value(value, from) {
            *value = *to;
            return;
        }

        match value.kind() {
            ValueKind::Cons => {
                let key = value.bits() ^ 0x1;
                if !visited.insert(key) {
                    return;
                }
                let mut new_car = value.cons_car();
                let mut new_cdr = value.cons_cdr();
                Self::replace_alias_refs_in_value(&mut new_car, from, to, visited);
                Self::replace_alias_refs_in_value(&mut new_cdr, from, to, visited);
                value.set_car(new_car);
                value.set_cdr(new_cdr);
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let key = value.bits() ^ 0x2;
                if !visited.insert(key) {
                    return;
                }
                let mut values = value.as_vector_data().unwrap().clone();
                for item in values.iter_mut() {
                    Self::replace_alias_refs_in_value(item, from, to, visited);
                }
                let _ = value.replace_vector_data(values);
            }
            ValueKind::Veclike(VecLikeType::Record) => {
                let key = value.bits() ^ 0x2;
                if !visited.insert(key) {
                    return;
                }
                let mut values = value.as_record_data().unwrap().clone();
                for item in values.iter_mut() {
                    Self::replace_alias_refs_in_value(item, from, to, visited);
                }
                let _ = value.replace_record_data(values);
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let key = value.bits() ^ 0x4;
                if !visited.insert(key) {
                    return;
                }
                let mut ht = value.as_hash_table().unwrap().clone();
                let old_ptr = if from.is_string() {
                    Some(from.bits())
                } else {
                    None
                };
                let new_ptr = if to.is_string() {
                    Some(to.bits())
                } else {
                    None
                };
                if matches!(ht.test, HashTableTest::Eq | HashTableTest::Eql)
                    && let (Some(old_ptr), Some(new_ptr)) = (old_ptr, new_ptr)
                {
                    ht.replace_pointer_key(old_ptr, new_ptr, *to);
                }
                for item in ht.data.values_mut() {
                    Self::replace_alias_refs_in_value(item, from, to, visited);
                }
                let _ = value.replace_hash_table(ht);
            }
            _ => {}
        }
    }
}

fn default_toplevel_binding(specpdl: &[SpecBinding], sym_id: SymId) -> Option<&SpecBinding> {
    specpdl.iter().find(|binding| match binding {
        SpecBinding::Let {
            sym_id: binding_sym,
            ..
        }
        | SpecBinding::LetDefault {
            sym_id: binding_sym,
            ..
        } => *binding_sym == sym_id,
        SpecBinding::LetLocal { .. }
        | SpecBinding::LexicalEnv { .. }
        | SpecBinding::GcRoot { .. }
        | SpecBinding::Backtrace { .. }
        | SpecBinding::Backtrace1 { .. }
        | SpecBinding::Backtrace2 { .. }
        | SpecBinding::BacktraceNative { .. }
        | SpecBinding::Nop
        | SpecBinding::UnwindProtect { .. }
        | SpecBinding::SaveExcursion { .. }
        | SpecBinding::SaveCurrentBuffer { .. }
        | SpecBinding::SaveRestriction { .. }
        | SpecBinding::LoadsInProgress { .. }
        | SpecBinding::NativeUnwind { .. }
        | SpecBinding::RequireStack { .. } => false,
    })
}

pub(crate) fn default_toplevel_value_in_state(
    obarray: &Obarray,
    specpdl: &[SpecBinding],
    buffer_defaults: Option<&[Value]>,
    sym_id: SymId,
) -> Option<Value> {
    match default_toplevel_binding(specpdl, sym_id) {
        Some(SpecBinding::Let { old_value, .. })
        | Some(SpecBinding::LetDefault { old_value, .. }) => old_value.get(),
        Some(SpecBinding::LetLocal { .. })
        | Some(SpecBinding::LexicalEnv { .. })
        | Some(SpecBinding::GcRoot { .. })
        | Some(SpecBinding::Backtrace { .. })
        | Some(SpecBinding::Backtrace1 { .. })
        | Some(SpecBinding::Backtrace2 { .. })
        | Some(SpecBinding::BacktraceNative { .. })
        | Some(SpecBinding::Nop)
        | Some(SpecBinding::UnwindProtect { .. })
        | Some(SpecBinding::SaveExcursion { .. })
        | Some(SpecBinding::SaveCurrentBuffer { .. })
        | Some(SpecBinding::SaveRestriction { .. })
        | Some(SpecBinding::LoadsInProgress { .. })
        | Some(SpecBinding::NativeUnwind { .. })
        | Some(SpecBinding::RequireStack { .. }) => {
            unreachable!("non-variable bindings are excluded above")
        }
        None => super::data::default_value_in_state(obarray, buffer_defaults, sym_id),
    }
}

/// `(default-toplevel-value SYMBOL)`.
pub(crate) fn default_toplevel_value(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("default-toplevel-value", &args, 1)?;
    let symbol = SymId::from_value(ctx, args[0])?;
    if let Some(binding) = default_toplevel_binding(ctx.specpdl.as_slice(), symbol) {
        let value = match binding {
            SpecBinding::Let { old_value, .. } | SpecBinding::LetDefault { old_value, .. } => {
                old_value.get()
            }
            _ => unreachable!("default_toplevel_binding returns only variable bindings"),
        };
        return value.ok_or_else(|| signal(LispCondition::VoidVariable, vec![args[0]]));
    }

    // GNU scans the specpdl by exact symbol identity before Fdefault_value
    // resolves aliases. A let through an alias records the resolved target,
    // so querying the alias itself intentionally falls back to its currently
    // visible default instead of exposing the target's saved outer value.
    super::data::default_value(ctx, args)
}

/// `(set-default-toplevel-value SYMBOL VALUE)`.
///
/// Mirrors GNU `Fset_default_toplevel_value` (`src/eval.c`): only the saved
/// value of an outer dynamic binding is eval-owned. With no such binding, the
/// complete write delegates to data.c's `set_default_internal` path.
pub(crate) fn set_default_toplevel_value(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-default-toplevel-value", &args, 2)?;
    let symbol = SymId::from_value(ctx, args[0])?;
    let value = args[1];

    if set_default_toplevel_value_in_state(ctx.specpdl.as_mut_slice(), symbol, value) {
        // The visible binding has not changed yet, so GNU neither notifies
        // watchers nor publishes a runtime-variable update at this point.
        ctx.note_macro_expansion_mutation();
    } else {
        super::data::set_default_internal(
            ctx,
            args[0],
            value,
            super::symbol::SetInternalBind::Set,
        )?;
    }

    Ok(Value::NIL)
}

pub(crate) fn set_default_toplevel_value_in_state(
    specpdl: &mut [SpecBinding],
    sym_id: SymId,
    value: Value,
) -> bool {
    for binding in specpdl.iter_mut() {
        match binding {
            SpecBinding::Let {
                sym_id: binding_sym,
                old_value,
            }
            | SpecBinding::LetDefault {
                sym_id: binding_sym,
                old_value,
                ..
            } if *binding_sym == sym_id => {
                old_value.set(Some(value));
                return true;
            }
            SpecBinding::Let { .. }
            | SpecBinding::LetDefault { .. }
            | SpecBinding::LetLocal { .. }
            | SpecBinding::LexicalEnv { .. }
            | SpecBinding::GcRoot { .. }
            | SpecBinding::Backtrace { .. }
            | SpecBinding::Backtrace1 { .. }
            | SpecBinding::Backtrace2 { .. }
            | SpecBinding::BacktraceNative { .. }
            | SpecBinding::Nop
            | SpecBinding::UnwindProtect { .. }
            | SpecBinding::SaveExcursion { .. }
            | SpecBinding::SaveCurrentBuffer { .. }
            | SpecBinding::SaveRestriction { .. }
            | SpecBinding::LoadsInProgress { .. }
            | SpecBinding::NativeUnwind { .. }
            | SpecBinding::RequireStack { .. } => {}
        }
    }
    false
}

pub(crate) fn set_runtime_binding_in_state(
    ctx: &mut Context,
    sym_id: SymId,
    value: Value,
) -> Result<Option<crate::buffer::BufferId>, Flow> {
    let locus = set_runtime_binding(
        &mut ctx.obarray,
        &mut ctx.buffers,
        &ctx.custom,
        ctx.specpdl.as_slice(),
        sym_id,
        value,
    )?;
    // The bytecode VM (`assign_var_id`/`assign_var`) and other raw-state
    // callers route writes through this entry point. Mark redisplay dirty for
    // display-affecting vars so byte-compiled assignment repaints exactly like
    // the tree-walk interpreter.
    ctx.mark_redisplay_dirty_if_display_var(sym_id);
    Ok(locus)
}

fn let_shadows_buffer_binding_p_in_state(
    specpdl: &[SpecBinding],
    buffers: &BufferManager,
    sym_id: SymId,
) -> bool {
    let current = buffers.current_buffer_id();
    specpdl.iter().rev().any(|entry| match entry {
        SpecBinding::LetDefault {
            sym_id: s,
            buffer_id,
            ..
        } => *s == sym_id && buffer_id.get() == current,
        SpecBinding::LetLocal { .. }
        | SpecBinding::Let { .. }
        | SpecBinding::LexicalEnv { .. }
        | SpecBinding::GcRoot { .. }
        | SpecBinding::Backtrace { .. }
        | SpecBinding::Backtrace1 { .. }
        | SpecBinding::Backtrace2 { .. }
        | SpecBinding::BacktraceNative { .. }
        | SpecBinding::Nop
        | SpecBinding::UnwindProtect { .. }
        | SpecBinding::SaveExcursion { .. }
        | SpecBinding::SaveCurrentBuffer { .. }
        | SpecBinding::SaveRestriction { .. }
        | SpecBinding::LoadsInProgress { .. }
        | SpecBinding::NativeUnwind { .. }
        | SpecBinding::RequireStack { .. } => false,
    })
}

/// The storage half of GNU `set_internal` (`src/data.c:1644-1830`): pick the
/// cell the assignment lands in and write it.
///
/// Takes a [`ForwardChecked`] rather than a `Value` so that the forward type's
/// rule cannot be skipped by adding another assignment path here.
fn store_runtime_binding(
    obarray: &mut Obarray,
    buffers: &mut BufferManager,
    _custom: &CustomManager,
    specpdl: &[SpecBinding],
    sym_id: SymId,
    checked: ForwardChecked,
) -> Option<crate::buffer::BufferId> {
    use crate::emacs_core::symbol::{SetInternalBind, SymbolRedirect};

    let value = checked.value();
    let symbol = obarray.get_by_id(sym_id);
    let symbol_is_interned_global = symbol.is_some_and(|s| s.is_interned_global());

    // Phase 10E: route writes for LOCALIZED symbols through the BLV
    // machinery. Mirrors GNU `set_internal` SYMBOL_LOCALIZED arm
    // (`data.c:1687-1762`) and the vm.rs assign_var_id LOCALIZED
    // path — keeps the eval.rs and vm.rs hot paths semantically
    // identical so a buffer-local visible from the bytecode VM is
    // also visible from the tree-walk interpreter and the `set`
    // builtin.
    let redirect = symbol.map(|s| s.redirect());
    if matches!(redirect, Some(SymbolRedirect::Localized))
        && let Some(buf_id) = buffers.current_buffer_id()
    {
        let (cur_val, alist) = match buffers.get(buf_id) {
            Some(buf) => (Value::make_buffer(buf.id), buf.local_var_alist_value()),
            None => (Value::NIL, Value::NIL),
        };
        let let_shadows = let_shadows_buffer_binding_p_in_state(specpdl, buffers, sym_id);
        let new_alist = obarray.set_internal_localized(
            sym_id,
            value,
            cur_val,
            alist,
            SetInternalBind::Set,
            let_shadows,
        );
        if let Some(buf) = buffers.get_mut(buf_id) {
            buf.replace_local_var_alist(new_alist);
        }
        return Some(buf_id);
    }

    // Phase 10D: ordinary `setq` on a FORWARDED BUFFER_OBJFWD symbol
    // mirrors GNU `set_internal` (`data.c:1774-1784`):
    //
    //   - always-local slots write the current buffer slot directly
    //   - conditional slots with an existing local flag write the
    //     current buffer slot directly
    //   - conditional slots without a local flag auto-create a local
    //     binding, unless a surrounding `let` is shadowing the buffer
    //     binding, in which case the write targets the default path
    //     (`set_default_internal`)
    if symbol_is_interned_global
        && matches!(redirect, Some(SymbolRedirect::Forwarded))
        && let Some(current_id) = buffers.current_buffer_id()
        && let Some(info) = crate::buffer::buffer::lookup_buffer_slot_by_sym_id(sym_id)
    {
        let has_local = buffers
            .get(current_id)
            .map(|buf| info.local_flags_idx < 0 || buf.slot_local_flag(info.offset))
            .unwrap_or(false);
        if has_local {
            let _ = buffers.set_buffer_local_property_by_sym_id(current_id, sym_id, value);
            return Some(current_id);
        }

        let let_shadows = let_shadows_buffer_binding_p_in_state(specpdl, buffers, sym_id);
        if let_shadows {
            buffers.set_buffer_default_slot(info, value);
            return None;
        }

        let _ = buffers.set_buffer_local_property_by_sym_id(current_id, sym_id, value);
        return Some(current_id);
    }

    // Non-forwarded per-buffer variables like `buffer-undo-list`
    // still live behind the generic buffer-local storage helpers.
    // Preserve the pre-Phase-10 behavior for those names: if the
    // current buffer already reports the variable as local, write the
    // current buffer binding instead of the obarray cell.
    // Non-localized globals are never in any `local_var_alist`, so skip the
    // per-buffer scan for them (slot/undo names still resolve inside the gated
    // call). `redirect` was fetched above. See `Obarray::is_localized`.
    let sym_is_localized = matches!(redirect, Some(SymbolRedirect::Localized));
    if symbol_is_interned_global
        && let Some(current_id) = buffers.current_buffer_id()
        && let Some(buf) = buffers.get(current_id)
        && buf.has_buffer_local_by_sym_id_gated(sym_id, sym_is_localized)
    {
        let _ = buffers.set_buffer_local_property_by_sym_id(current_id, sym_id, value);
        return Some(current_id);
    }

    obarray.set_symbol_value_id(sym_id, value);
    None
}

/// Map the buffer module's typed predicate failure into GNU-compatible Lisp
/// signal data. Keeping this conversion at the evaluator boundary lets the
/// buffer storage layer remain independent of non-local control flow.
pub(crate) fn validate_buffer_slot_write(
    predicate: crate::buffer::buffer::BufferSlotPredicate,
    value: Value,
) -> Result<(), Flow> {
    use crate::buffer::buffer::BufferSlotPredicateError;

    match predicate.check(value) {
        Ok(()) => Ok(()),
        Err(BufferSlotPredicateError::WrongType(predicate_name)) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol(predicate_name), value],
        )),
        Err(BufferSlotPredicateError::Choice(message))
        | Err(BufferSlotPredicateError::Range(message)) => Err(signal(
            LispCondition::Error,
            vec![Value::string(message), value],
        )),
    }
}

/// Map a forwarded slot's refusal onto GNU's signal data.
pub(crate) fn forward_store_signal(
    error: crate::emacs_core::forward::ForwardStoreError,
    value: Value,
) -> Flow {
    use crate::buffer::buffer::BufferSlotPredicateError;
    use crate::emacs_core::forward::ForwardStoreError;

    match error {
        ForwardStoreError::WrongType(predicate_name)
        | ForwardStoreError::Predicate(BufferSlotPredicateError::WrongType(predicate_name)) => {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol(predicate_name), value],
            )
        }
        // GNU `xsignal1 (Qoverflow_error, newval)` (`src/data.c:1480`).
        ForwardStoreError::Overflow => signal("overflow-error", vec![value]),
        ForwardStoreError::Predicate(
            BufferSlotPredicateError::Choice(message) | BufferSlotPredicateError::Range(message),
        ) => signal(LispCondition::Error, vec![Value::string(message), value]),
    }
}

/// A value the forward type governing an assignment has accepted, in the form
/// that assignment will store.
///
/// [`check_forwarded_store`] is the only constructor and
/// [`store_runtime_binding`] takes nothing else, so no assignment path can
/// reach a forwarded symbol's storage without the type's rule having run.
/// That is the whole point of GNU putting `store_symval_forwarding` *below*
/// `set_internal` rather than beside it (`src/data.c:1469-1530`): the rule is
/// not something `Fset`, `set_default`, `specbind` and the bytecode `varset`
/// each have to remember.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ForwardChecked(Value);

impl ForwardChecked {
    /// The value to store -- which is not always the value handed in: a
    /// `Lisp_Fwd_Bool` slot coerces to `!NILP (newval)` instead of signalling.
    #[inline]
    pub(crate) fn value(self) -> Value {
        self.0
    }
}

/// The forward descriptor an assignment to `sym_id` has to satisfy, if any.
///
/// GNU reaches `store_symval_forwarding` two ways for an ordinary `setq`: the
/// SYMBOL_FORWARDED arm (`src/data.c:1766-1830`) and the SYMBOL_LOCALIZED arm
/// via `blv->fwd` (`src/data.c:1794`), which is how a `DEFVAR_INT` variable
/// that some buffer made local keeps its integer slot.  Both are covered here.
fn assignment_forwarder(
    obarray: &Obarray,
    sym_id: SymId,
) -> Option<&'static crate::emacs_core::forward::LispFwd> {
    use crate::emacs_core::symbol::SymbolRedirect;

    match obarray.get_by_id(sym_id).map(|symbol| symbol.redirect()) {
        // Safety: `install_*fwd` leaks every descriptor it installs.
        Some(SymbolRedirect::Forwarded) => obarray.forwarder(sym_id),
        Some(SymbolRedirect::Localized) => obarray.blv(sym_id).and_then(|blv| blv.fwd),
        _ => None,
    }
}

/// Which of GNU's two stores an assignment performs.
///
/// It matters for exactly one forward variant.  A per-buffer slot that has no
/// local value in the current buffer is written by `set_default_internal`,
/// which reaches `set_per_buffer_default` WITHOUT going through
/// `store_symval_forwarding` (`src/data.c:2080-2113`), so the slot predicate
/// does not apply there; `do_specbind` routes exactly that case to it
/// (`src/eval.c:3606-3617`).  Every other variant is checked either way.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ForwardStoreSite {
    /// An ordinary `setq` / `set`, GNU `set_internal`.
    Set,
    /// A `let` binding, GNU `do_specbind`.
    Bind,
    /// `set-default` / `setq-default`, GNU `set_default_internal`, which
    /// writes a per-buffer slot's shared default without the predicate.
    SetDefault,
}

/// Run GNU `store_symval_forwarding`'s type switch for this assignment.
pub(crate) fn check_forwarded_store_at(
    obarray: &Obarray,
    buffers: &BufferManager,
    specpdl: &[SpecBinding],
    sym_id: SymId,
    value: Value,
    site: ForwardStoreSite,
) -> Result<ForwardChecked, Flow> {
    use crate::emacs_core::forward::LispFwdType;

    let Some(fwd) = assignment_forwarder(obarray, sym_id) else {
        return Ok(ForwardChecked(value));
    };
    if fwd.ty == LispFwdType::BufferObj {
        if site == ForwardStoreSite::SetDefault {
            return Ok(ForwardChecked(value));
        }
        let Some(current_id) = buffers.current_buffer_id() else {
            return Ok(ForwardChecked(value));
        };
        let Some(info) = crate::buffer::buffer::lookup_buffer_slot_by_sym_id(sym_id) else {
            return Ok(ForwardChecked(value));
        };
        let has_local = buffers
            .get(current_id)
            .is_some_and(|buffer| info.local_flags_idx < 0 || buffer.slot_local_flag(info.offset));
        let writes_live_slot = match site {
            ForwardStoreSite::Set => {
                has_local || !let_shadows_buffer_binding_p_in_state(specpdl, buffers, sym_id)
            }
            ForwardStoreSite::Bind => has_local,
            ForwardStoreSite::SetDefault => unreachable!("returned above"),
        };
        if !writes_live_slot {
            return Ok(ForwardChecked(value));
        }
    }
    match fwd.store(value) {
        Ok(store) => Ok(ForwardChecked(store.canonical_value())),
        Err(error) => Err(forward_store_signal(error, value)),
    }
}

/// [`check_forwarded_store_at`] for an ordinary assignment.
pub(crate) fn check_forwarded_store(
    obarray: &Obarray,
    buffers: &BufferManager,
    specpdl: &[SpecBinding],
    sym_id: SymId,
    value: Value,
) -> Result<ForwardChecked, Flow> {
    check_forwarded_store_at(
        obarray,
        buffers,
        specpdl,
        sym_id,
        value,
        ForwardStoreSite::Set,
    )
}

/// The Lisp-visible assignment entry point: check the forward type, then
/// store. Every `setq` spelling funnels through here.
pub(crate) fn set_runtime_binding(
    obarray: &mut Obarray,
    buffers: &mut BufferManager,
    custom: &CustomManager,
    specpdl: &[SpecBinding],
    sym_id: SymId,
    value: Value,
) -> Result<Option<crate::buffer::BufferId>, Flow> {
    let checked = check_forwarded_store(obarray, buffers, specpdl, sym_id, value)?;
    Ok(store_runtime_binding(
        obarray, buffers, custom, specpdl, sym_id, checked,
    ))
}

/// GNU's `set_internal` refuses `unbinding_p` for any symbol whose storage is
/// a forwarder -- `error ("Built-in variable may not be unbound : %s")` at
/// `src/data.c:1725-1728` (localized-with-forwarder) and `:1805-1807`
/// (forwarded).  There is no "unbound" bit pattern in a C slot, so the state
/// simply does not exist; the same is true of a [`crate::emacs_core::forward::LispIntFwd`]
/// here, which is why this has to be a signal rather than a silent no-op.
pub(crate) fn check_forwarded_unbind(
    obarray: &Obarray,
    sym_id: SymId,
    reported: Value,
) -> Result<(), Flow> {
    if assignment_forwarder(obarray, sym_id).is_none() {
        return Ok(());
    }
    Err(signal(
        LispCondition::Error,
        vec![Value::string(format!(
            "Built-in variable may not be unbound : {}",
            crate::emacs_core::intern::resolve_sym(reported.as_symbol_id().unwrap_or(sym_id))
        ))],
    ))
}

pub(crate) fn makunbound_runtime_binding_in_state(
    obarray: &mut Obarray,
    buffers: &mut BufferManager,
    _custom: &CustomManager,
    _specpdl: &[SpecBinding],
    sym_id: SymId,
) {
    let symbol_is_canonical = super::builtins::is_canonical_symbol_id(sym_id);

    // specbind writes directly to obarray, so no dynamic frame lookup needed.

    // Non-localized globals are never in any `local_var_alist`; skip the scan.
    let sym_is_localized = obarray.is_localized(sym_id);
    if symbol_is_canonical
        && let Some(current_id) = buffers.current_buffer_id()
        && let Some(buf) = buffers.get(current_id)
        && buf.has_buffer_local_by_sym_id_gated(sym_id, sym_is_localized)
    {
        let _ = buffers.set_buffer_local_void_property_by_sym_id(current_id, sym_id);
        return;
    }

    // Mirrors GNU `set_internal` SYMBOL_LOCALIZED arm with
    // `unbinding_p = true` (`src/data.c:1687-1762`). The BLV's
    // `local_if_set` flag determines whether to create a per-buffer
    // void binding; LOCALIZED symbols carry a BLV so this fires only
    // for them.
    let local_if_set = obarray
        .blv(sym_id)
        .map(|blv| blv.local_if_set)
        .unwrap_or(false);
    if symbol_is_canonical
        && local_if_set
        && let Some(current_id) = buffers.current_buffer_id()
    {
        let _ = buffers.set_buffer_local_void_property_by_sym_id(current_id, sym_id);
        return;
    }

    obarray.makunbound_id(sym_id);
}

impl Context {
    /// Call a registered subr value directly. Returns None if VALUE is not a
    /// fully registered subr.
    pub fn dispatch_subr_value(&mut self, function: Value, args: Vec<Value>) -> Option<EvalResult> {
        let sym_id = function.as_subr_id()?;
        let wrong_arity_callee = Value::symbol(resolve_sym(sym_id));
        self.dispatch_subr_value_internal(function, args.into(), wrong_arity_callee)
    }

    /// Resolve a symbol identity to its canonical subr object and call it.
    /// Returns None if the symbol's canonical name has no registered subr.
    /// Supports uninterned symbols: falls back to canonical SymId via NameId lookup.
    pub fn dispatch_subr_id(&mut self, sym_id: SymId, args: Vec<Value>) -> Option<EvalResult> {
        // Try the sym_id directly first
        let resolved = if lookup_global_subr_entry(sym_id).is_some() {
            sym_id
        } else {
            // Fall back to canonical symbol for this name (handles uninterned SymIds)
            let name_id = symbol_name_id(sym_id);
            let canonical = crate::emacs_core::intern::canonical_symbol_for_name(name_id)?;
            lookup_global_subr_entry(canonical)?;
            canonical
        };
        let function = Value::subr_from_sym_id(resolved);
        self.dispatch_subr_value(function, args)
    }

    pub fn dispatch_subr(&mut self, name: &str, args: Vec<Value>) -> Option<EvalResult> {
        self.dispatch_subr_id(intern(name), args)
    }

    // -----------------------------------------------------------------------

    pub(crate) fn lexenv_assq_cached_in(&self, lexenv: Value, sym_id: SymId) -> Option<Value> {
        let lexenv_bits = lexenv.bits();
        let cache = &self.lexenv_assq_cache;
        if let Some(cell) = cache.find(lexenv_bits, sym_id) {
            return Some(cell);
        }

        let cell = lexenv_assq(lexenv, sym_id)?;
        cache.push(LexenvAssqCacheEntry {
            lexenv_bits,
            symbol: sym_id,
            cell,
        });
        Some(cell)
    }

    pub(crate) fn lexenv_lookup_cached_in(&self, lexenv: Value, sym_id: SymId) -> Option<Value> {
        self.lexenv_assq_cached_in(lexenv, sym_id)
            .map(|cell| cell.cons_cdr())
    }

    pub(crate) fn lexenv_declares_special_cached_in(&self, lexenv: Value, sym_id: SymId) -> bool {
        let lexenv_bits = lexenv.bits();
        let cache = &self.lexenv_special_cache;
        if let Some(declared_special) = cache.find(lexenv_bits, sym_id) {
            return declared_special;
        }

        let declared_special = lexenv_declares_special(lexenv, sym_id);
        cache.push(LexenvSpecialCacheEntry {
            lexenv_bits,
            symbol: sym_id,
            declared_special,
        });
        declared_special
    }

    pub(crate) fn lexbound_p_in_specpdl(&self, sym_id: SymId) -> bool {
        // Mirrors GNU eval.c `lexbound_p`: scan saved
        // `internal-interpreter-environment` values on the specpdl, not the
        // current lexical environment.
        for binding in self.specpdl.iter().rev() {
            if let SpecBinding::LexicalEnv { old_lexenv } = binding
                && lexenv_assq(*old_lexenv, sym_id).is_some()
            {
                return true;
            }
        }
        false
    }

    /// Assign a value to a variable identified by SymId.
    /// Uses the SymId directly for lexenv/dynamic lookup, preserving
    /// uninterned symbol identity (like Emacs's EQ-based setq).
    pub(crate) fn assign_by_id(&mut self, sym_id: SymId, value: Value) {
        let _ = self.assign_by_id_with_locus(sym_id, value);
    }

    /// Mutate the lexical cell for the exact source symbol, if one exists.
    ///
    /// GNU `eval_sub`/`Fsetq` performs this EQ-based lookup before entering
    /// `Fset`, where variable aliases, watchers, and runtime storage apply.
    /// Keeping that boundary explicit prevents a `defvaralias` target from
    /// stealing reads or writes from a lexical binding of the alias itself.
    fn try_assign_lexical_binding_by_id(&mut self, sym_id: SymId, value: Value) -> bool {
        if self.lexical_binding()
            && let Some(cell_id) = self.lexenv_assq_cached_in(self.lexenv, sym_id)
        {
            lexenv_set(cell_id, value);
            return true;
        }
        false
    }

    pub(crate) fn assign_by_id_with_locus(
        &mut self,
        sym_id: SymId,
        value: Value,
    ) -> Result<Option<crate::buffer::BufferId>, Flow> {
        // GNU `setq` follows the same rule as `eval_sub`: if a lexical binding
        // cell exists, mutate it directly. Declared-special affects whether
        // that cell was created, not whether assignment should reuse it.
        if self.try_assign_lexical_binding_by_id(sym_id, value) {
            return Ok(None);
        }

        self.try_set_runtime_binding_by_id(sym_id, value)
    }

    /// Implement GNU `setq`'s two-stage assignment protocol.
    ///
    /// Stage 1 mutates an exact lexical binding directly. Stage 2, reached
    /// only when no such binding exists, delegates to the runtime variable
    /// model: resolve aliases, enforce constants, notify watchers, and write
    /// the global/buffer-local/forwarded storage.
    pub(crate) fn assign_setq_by_id(&mut self, sym_id: SymId, value: Value) -> EvalResult {
        if self.try_assign_lexical_binding_by_id(sym_id, value) {
            return Ok(value);
        }

        let resolved_id = super::builtins::resolve_variable_alias_id(self, sym_id)?;
        if self.obarray.is_constant_id(resolved_id)
            && !self.has_local_binding_by_id(sym_id)
            && (resolved_id == sym_id || !self.has_local_binding_by_id(resolved_id))
            && let Some(result) = super::builtins::constant_set_outcome_in_obarray(
                self.obarray(),
                resolved_id,
                value_from_symbol_id(sym_id),
                value,
            )
        {
            return result;
        }

        let where_value = self.variable_watcher_where_for_set_by_id(resolved_id);
        self.run_variable_watchers_by_id_with_where(
            resolved_id,
            &value,
            &Value::NIL,
            "set",
            &where_value,
        )?;
        self.try_set_runtime_binding_by_id(resolved_id, value)?;
        Ok(value)
    }

    pub(crate) fn assign(&mut self, name: &str, value: Value) {
        self.assign_by_id(intern(name), value);
    }

    pub(crate) fn try_set_runtime_binding_by_id(
        &mut self,
        sym_id: SymId,
        value: Value,
    ) -> Result<Option<crate::buffer::BufferId>, Flow> {
        let checked =
            check_forwarded_store(&self.obarray, &self.buffers, &self.specpdl, sym_id, value)?;
        // A `Lisp_Fwd_Bool` slot stores `!NILP (newval)`, so every mirror of
        // this write has to see what the forwarder accepted, not what the
        // caller passed.
        let value = checked.value();
        let locus = store_runtime_binding(
            &mut self.obarray,
            &mut self.buffers,
            &self.custom,
            &self.specpdl,
            sym_id,
            checked,
        );
        self.publish_runtime_binding_write_by_id(sym_id, value);
        Ok(locus)
    }

    pub(crate) fn makunbound_runtime_binding_by_id(&mut self, sym_id: SymId) {
        makunbound_runtime_binding_in_state(
            &mut self.obarray,
            &mut self.buffers,
            &self.custom,
            &[],
            sym_id,
        );
        self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
        self.sync_keyboard_runtime_binding_by_id(sym_id, Value::NIL);
        self.refresh_gc_runtime_settings_after_change_by_id(sym_id);
    }

    /// Return whether an exact lexical or dynamic binding is active.
    ///
    /// The dynamic case matters for GNU-compatible lambda parameters named
    /// `nil` or `t`: their specpdl binding is assignable even though the
    /// corresponding global symbol is constant.
    fn has_local_binding_by_id(&self, sym_id: SymId) -> bool {
        self.lexenv_assq_cached_in(self.lexenv, sym_id).is_some()
            || self
                .specpdl
                .iter()
                .rev()
                .any(|entry| matches!(entry, SpecBinding::Let { sym_id: s, .. } if *s == sym_id))
    }

    pub(crate) fn visible_variable_value_or_nil(&self, name: &str) -> Value {
        self.visible_variable_value_or_nil_by_id(intern(name))
    }

    pub(crate) fn visible_variable_value_or_nil_by_id(&self, sym_id: SymId) -> Value {
        if let Some(value) = self.lexenv_lookup_cached_in(self.lexenv, sym_id) {
            return value;
        }
        if let Ok(Some(value)) = self.visible_runtime_variable_value_by_id(sym_id) {
            return value;
        }
        Value::NIL
    }

    /// Read one of the variables `truncate_undo_list' consults, the way GNU's
    /// C code sees it.
    ///
    /// GNU reads C globals reached through `DEFVAR_INT' / `DEFVAR_LISP'
    /// symbols, so no lexical binding of the same name can shadow them; only
    /// the dynamic value visible in the current buffer counts. Void reads back
    /// as nil, which is the "no limit" / "no function" case at
    /// `src/undo.c:352-356`.
    pub(crate) fn undo_truncation_variable(&self, name: &str) -> Value {
        self.visible_runtime_variable_value_by_id(intern(name))
            .ok()
            .flatten()
            .unwrap_or(Value::NIL)
    }

    /// What `inhibit-eol-conversion` holds right now, for the coding
    /// conversion that is about to run.
    ///
    /// GNU's `inhibit_eol_conversion` is a `DEFVAR_BOOL` C global
    /// (src/coding.c:12022), so it is read through the same dynamic value in
    /// every conversion and no lexical binding of the name can shadow it --
    /// hence `visible_runtime_variable_value_by_id` and not
    /// `visible_variable_value_or_nil`.  Void reads back as nil, which is the
    /// variable's own initial value (src/coding.c:12027).
    ///
    /// Call this at the point of CONVERSION, never at the point where a coding
    /// system is chosen: see
    /// [`EolConversion`](crate::emacs_core::coding::EolConversion) for the
    /// measurement that pins the difference.
    pub(crate) fn eol_conversion(&self) -> crate::emacs_core::coding::EolConversion {
        crate::emacs_core::coding::EolConversion::from_lisp(
            self.visible_runtime_variable_value_by_id(intern("inhibit-eol-conversion"))
                .ok()
                .flatten()
                .unwrap_or(Value::NIL),
        )
    }

    pub(crate) fn visible_runtime_variable_value_by_id(
        &self,
        sym_id: SymId,
    ) -> Result<Option<Value>, Flow> {
        let resolved = builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id)?;
        Ok(self.visible_runtime_variable_value_by_id_resolved(resolved))
    }

    pub(crate) fn visible_runtime_variable_value_by_id_resolved(
        &self,
        resolved: SymId,
    ) -> Option<Value> {
        // Canonicality is only consulted by the rare fallback arms below;
        // check the (2-instruction) id compares first so the common read
        // skips the epoch-checked TLS canonical lookup entirely.
        use crate::emacs_core::symbol::SymbolRedirect;
        if let Some(sym) = self.obarray.get_by_id(resolved) {
            match sym.redirect() {
                SymbolRedirect::Localized => {
                    if let Some(buf) = self.buffers.current_buffer() {
                        if let Some(value) = self.obarray.read_localized_for_buffer(
                            resolved,
                            buf.id,
                            buf.local_var_alist_value(),
                        ) {
                            if value.is_unbound() {
                                return None;
                            }
                            return Some(value);
                        }
                    }
                }
                SymbolRedirect::Forwarded => {
                    if let Some(value) = self.forwarded_buffer_obj_value(sym) {
                        return Some(value);
                    }
                }
                SymbolRedirect::Plainval | SymbolRedirect::Varalias => {}
            }
        }

        if resolved == buffer_undo_list_symbol()
            && is_canonical_id(resolved)
            && let Some(buf) = self.buffers.current_buffer()
            && let Some(binding) = buf.get_buffer_local_binding_by_sym_id(resolved)
        {
            return binding.as_value();
        }

        if let Some(value) = self.obarray.symbol_value_id(resolved).copied() {
            return Some(value);
        }

        if resolved == nil_symbol() && is_canonical_id(resolved) {
            return Some(Value::NIL);
        }
        if resolved == t_symbol() && is_canonical_id(resolved) {
            return Some(Value::T);
        }
        if is_keyword_id(resolved) {
            return Some(Value::from_kw_id(resolved));
        }

        None
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn run_unlet_watchers(&mut self, bindings: &[(String, Value, Value)]) -> Result<(), Flow> {
        for (name, _, restored_value) in bindings.iter().rev() {
            self.run_variable_watchers(name, restored_value, &Value::NIL, "unlet")?;
        }
        Ok(())
    }

    pub(crate) fn run_variable_watchers_by_id(
        &mut self,
        sym_id: SymId,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
    ) -> Result<(), Flow> {
        self.run_variable_watchers_by_id_with_where(
            sym_id,
            new_value,
            old_value,
            operation,
            &Value::NIL,
        )
    }

    pub(crate) fn run_variable_watchers_by_id_with_where(
        &mut self,
        sym_id: SymId,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
        where_value: &Value,
    ) -> Result<(), Flow> {
        if !self.watchers.has_watchers(sym_id) {
            return Ok(());
        }
        if self.active_variable_watchers.contains(&sym_id) {
            return Ok(());
        }
        let calls =
            self.watchers
                .notify_watchers(sym_id, new_value, old_value, operation, where_value);
        self.active_variable_watchers.insert(sym_id);
        // The snapshotted (callback, args) pairs live only in this Rust Vec
        // while earlier watchers run; a watcher that remove-variable-watchers
        // a later one unlinks it from the watcher table (its only root) and a
        // GC frees it before its call. Thread every snapshot Value onto one
        // heap list under a single root for the loop's span.
        let mut holder = Value::NIL;
        for (callback, args) in calls.iter().rev() {
            for value in args.iter().rev() {
                holder = Value::cons(*value, holder);
            }
            holder = Value::cons(*callback, holder);
        }
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(holder);
        for (callback, args) in calls {
            if let Err(err) = self.apply(callback, args) {
                self.restore_specpdl_roots(root_scope);
                self.active_variable_watchers.remove(&sym_id);
                return Err(err);
            }
        }
        self.restore_specpdl_roots(root_scope);
        self.active_variable_watchers.remove(&sym_id);
        Ok(())
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn run_variable_watchers(
        &mut self,
        name: &str,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
    ) -> Result<(), Flow> {
        self.run_variable_watchers_by_id(intern(name), new_value, old_value, operation)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn run_variable_watchers_with_where(
        &mut self,
        name: &str,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
        where_value: &Value,
    ) -> Result<(), Flow> {
        self.run_variable_watchers_by_id_with_where(
            intern(name),
            new_value,
            old_value,
            operation,
            where_value,
        )
    }

    pub(crate) fn variable_watcher_where_for_set_by_id(&self, sym_id: SymId) -> Value {
        use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
        use crate::emacs_core::symbol::SymbolRedirect;

        let Some(current_id) = self.buffers.current_buffer_id() else {
            return Value::NIL;
        };
        if sym_id == buffer_undo_list_symbol() {
            return Value::make_buffer(current_id);
        }
        let Some(sym) = self.obarray.get_by_id(sym_id) else {
            return Value::NIL;
        };
        match sym.redirect() {
            SymbolRedirect::Localized => {
                if self.obarray.blv(sym_id).is_some_and(|blv| blv.local_if_set)
                    || self
                        .buffers
                        .get(current_id)
                        .is_some_and(|buf| buf.has_buffer_local_by_sym_id(sym_id))
                {
                    Value::make_buffer(current_id)
                } else {
                    Value::NIL
                }
            }
            SymbolRedirect::Forwarded => {
                let fwd = unsafe { &*sym.val.fwd };
                if matches!(fwd.ty, LispFwdType::BufferObj) {
                    let _buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                    return Value::make_buffer(current_id);
                }
                Value::NIL
            }
            _ => Value::NIL,
        }
    }
}

fn format_startup_value(value: Option<&Value>) -> String {
    value
        .map(super::print::print_value)
        .unwrap_or_else(|| "<unbound>".to_string())
}

/// Convert a Value cons list to the evaluator's inline argument buffer.
fn value_list_to_values(list: &Value) -> LispArgVec {
    let mut result = LispArgVec::new();
    let mut cursor = *list;
    while cursor.is_cons() {
        result.push(cursor.cons_car());
        cursor = cursor.cons_cdr();
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
mod gc_pacing;

mod pdump_reconstruct;

mod macroexpand;

mod specpdl;

mod special_forms;

mod apply;

mod command_loop;

mod vm_shared;

mod signal_dispatch;

mod construct;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

// task3-jitcrash-diag: diagnostic repros for the pre-existing JIT
// heap-corruption crash (no fix here).
#[cfg(test)]
#[path = "tests/jit_crash_repro.rs"]
mod jit_crash_repro_tests;

/// Allocator for [`Context::context_instance_id`].
fn next_context_instance_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// How the command loop's log treats an uncaught command signal; see
/// `Context::command_error_severity'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandErrorSeverity {
    /// What the user just did: a quit, or a condition GNU's debugger ignores
    /// (`debug-ignored-errors').  Shown in the echo area, logged at debug.
    Routine,
    /// Anything else: shown the same way, logged as an error so a bug
    /// report carries the condition and payload.
    Diagnostic,
}

/// An owned snapshot of one uncaught command signal, captured before Lisp
/// presentation code can alter the state used to classify or render it.
struct CommandLoopDiagnostic {
    severity: CommandErrorSeverity,
    condition: String,
    message: String,
    signal: String,
    backtrace: String,
}

impl CommandLoopDiagnostic {
    fn emit(self) {
        let Self {
            severity,
            condition,
            message,
            signal,
            backtrace,
        } = self;

        // A tracing callsite's level is static metadata, so selecting a level
        // through a runtime variable would not compile.  Expanding the shared
        // fields with a literal level keeps two compile-time callsites without
        // duplicating the diagnostic schema.
        macro_rules! emit_at {
            ($level:expr) => {
                tracing::event!(
                    $level,
                    condition = %condition,
                    signal = %signal,
                    backtrace = %backtrace,
                    "Command loop condition: {message}"
                )
            };
        }

        match severity {
            CommandErrorSeverity::Routine => emit_at!(tracing::Level::DEBUG),
            CommandErrorSeverity::Diagnostic => emit_at!(tracing::Level::ERROR),
        }
    }
}
