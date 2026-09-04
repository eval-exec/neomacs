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
    if cfg!(target_os = "windows") {
        "windows-nt"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "gnu/linux"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        std::env::consts::OS
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
        use std::time::{SystemTime, UNIX_EPOCH};

        let (secs, usecs) = match SystemTime::now().duration_since(UNIX_EPOCH) {
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
        let t0 = std::time::Instant::now();
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
    let scratch_t0 = std::time::Instant::now();
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
    match super::load::resolve_load_path_file_in_state(obarray, buf, &filename, requirement)? {
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

    pub fn new() -> Self {
        let mut ctx = Self::new_inner(true);
        ctx.initialize_gc_stack_bottom();
        // Register builtins AFTER new_inner returns — the function is too
        // large (1500+ lines) for reliable codegen in debug mode when
        // combined with the full native subr manifest in the same frame.
        builtins::init_builtins(&mut ctx);
        // Seed GNU's 24 standard built-in fringe bitmaps (right-arrow, left-arrow,
        // continuation/truncation markers, …) and their `'fringe` indices into
        // the registry, AFTER the obarray is populated by init_builtins.
        ctx.pre_register_standard_fringe_bitmaps();
        ctx
    }

    pub(crate) fn ensure_startup_messages_buffer(&mut self) {
        // GNU's initialized batch/runtime state has a live `*Messages*`
        // buffer before user Lisp runs: `emacs.c` clears pre-dump messages via
        // `message_dolog`, whose xdisp.c path creates `messages-buffer-name`.
        // Keep it after the initial minibuffer in buffer-list order and do not
        // select it.
        if self.buffers.find_buffer_by_name("*Messages*").is_none() {
            self.buffers.create_buffer("*Messages*");
        }
    }

    #[cfg(test)]
    pub(crate) fn new_vm_runtime_harness() -> Self {
        // GNU bytecode executes inside the same callable runtime surface as the
        // ordinary evaluator. Keep the default VM harness on that full surface.
        Self::new()
    }

    #[cfg(test)]
    pub(crate) fn new_minimal_vm_harness() -> Self {
        // Keep this reduced constructor only for low-level VM/opcode tests
        // that intentionally do not depend on the full builtin surface.
        let mut ev = Self::new_inner(true);
        ev.obarray = Obarray::new();
        super::errors::init_standard_errors(&mut ev.obarray);
        ev.obarray
            .set_symbol_value("most-positive-fixnum", Value::fixnum(i64::MAX >> 2));
        ev.obarray.make_special("most-positive-fixnum");
        ev.obarray.set_constant("most-positive-fixnum");
        ev.obarray
            .set_symbol_value("most-negative-fixnum", Value::fixnum(-(i64::MAX >> 2) - 1));
        ev.obarray.make_special("most-negative-fixnum");
        ev.obarray.set_constant("most-negative-fixnum");
        ev.specpdl.clear();
        ev.backtrace_args_stack.clear();
        ev.lexenv = Value::NIL;
        ev.features.clear();
        ev.require_stack.clear();
        ev.loads_in_progress.clear();
        ev.load_read_cursors.clear();
        ev.last_uncaught_signal_backtrace = None;
        ev.buffers = BufferManager::new();
        ev.xwidgets = super::xwidget::XwidgetState::new();
        ev.last_overlay_modification_hooks.clear();
        ev.interval_insert_behind_hooks = Value::NIL;
        ev.interval_insert_in_front_hooks = Value::NIL;
        ev.match_data = None;
        ev.processes = ProcessManager::new();
        ev.watchers = VariableWatcherList::new();
        ev.current_local_map = Value::NIL;
        ev.selected_global_map = super::keymap::SelectedGlobalMap::default();
        ev.registers = RegisterManager::new();
        ev.bookmarks = BookmarkManager::new();
        ev.abbrevs = AbbrevManager::new();
        ev.autoloads = AutoloadManager::new();
        ev.custom = CustomManager::new();
        ev.rectangle = RectangleState::new();
        ev.interactive = InteractiveRegistry::new();
        ev.input_mode_interrupt = false;
        ev.frames = lisp_frame_manager();
        ev.modes = ModeRegistry::new();
        ev.threads = ThreadManager::new();
        ev.kmacro = KmacroManager::new();
        ev.command_loop = crate::keyboard::CommandLoop::default();
        ev.input_rx = None;
        ev.eval_task_rx = None;
        ev.redisplay_fn = None;
        ev.frame_snapshot_fn = None;
        ev.window_layout_query_adapter = WindowLayoutQueryAdapter::Unavailable;
        ev.display_host = None;
        ev.coding_systems = CodingSystemManager::new();
        ev.face_table = FaceTable::new();
        ev.face_change_count = 0;
        ev.display_var_change_count = 0;
        ev.redisplay_generation = 0;
        ev.menu_bar_rebuild_generation = 0;
        ev.media_generation = 0;
        ev.last_redisplay_signature = None;
        ev.depth = 0;
        ev.max_depth = 1600;
        ev.gc_pending = false;
        ev.gc_count = 0;
        ev.gc_stress = gc_stress_from_env();
        ev.condition_stack.clear();
        ev.next_resume_id = 1;
        ev.named_call_cache.clear();

        ev.macro_expand_calls = 0;
        ev.macro_expand_total_us = 0;
        ev.macro_perf_enabled = std::env::var_os("NEOVM_TRACE_MACRO_PERF").is_some();
        ev.macro_perf_stats = MacroPerfStats::default();
        ev.interpreted_closure_filter_fn = None;
        register_subrs(&mut ev);
        ev.finish_runtime_activation(false);
        ev
    }

    pub(crate) fn push_condition_frame(&mut self, frame: ConditionFrame) {
        self.condition_stack.push(frame);
    }

    pub(crate) fn pop_condition_frame(&mut self) -> Option<ConditionFrame> {
        self.condition_stack.pop()
    }

    pub(crate) fn truncate_condition_stack(&mut self, len: usize) {
        self.condition_stack.truncate(len);
    }

    /// Rebase the `stack_len` of the topmost `count` bytecode catch/condition-case
    /// handlers from FRAME-RELATIVE to ABSOLUTE `bc_buf` positions by adding
    /// `frame_base`.
    ///
    /// The JIT `push-catch`/`push-condition-case` shims record `stack_len` as the
    /// native model operand-stack DEPTH (frame-relative — a native frame keeps no
    /// operands on `bc_buf`), whereas the interpreter records the ABSOLUTE
    /// `bc_buf.len()`. When a native frame deopts and resumes via
    /// [`Vm::run_resumed_frame`], its operands are seeded at `bc_buf[frame_base..]`,
    /// so its transferred handlers must be rebased to absolute — otherwise a later
    /// throw/signal caught by such a handler would `bc_buf.truncate(relative_len)`
    /// and collapse the caller's live operand stack (the native frame's handlers
    /// are exactly the topmost `count` Vm catch/condition-case frames).
    pub(crate) fn rebase_resumed_vm_handler_stack_lens(&mut self, count: usize, frame_base: usize) {
        if count == 0 || frame_base == 0 {
            return;
        }
        let mut remaining = count;
        for frame in self.condition_stack.iter_mut().rev() {
            if remaining == 0 {
                break;
            }
            let resume = match frame {
                ConditionFrame::Catch { resume, .. }
                | ConditionFrame::ConditionCase { resume, .. } => resume,
                _ => continue,
            };
            match resume {
                ResumeTarget::VmCatch { stack_len, .. }
                | ResumeTarget::VmConditionCase { stack_len, .. } => {
                    *stack_len += frame_base;
                    remaining -= 1;
                }
                _ => continue,
            }
        }
    }

    pub(crate) fn condition_stack_len(&self) -> usize {
        self.condition_stack.len()
    }

    pub(crate) fn allocate_resume_id(&mut self) -> u64 {
        let resume_id = self.next_resume_id;
        self.next_resume_id += 1;
        resume_id
    }

    pub(crate) fn matching_catch_resume(&self, tag: &Value) -> Option<ResumeTarget> {
        if tag.is_nil() {
            return None;
        }

        self.condition_stack
            .iter()
            .rev()
            .find_map(|frame| match frame {
                ConditionFrame::Catch {
                    tag: catch_tag,
                    resume,
                } if eq_value(catch_tag, tag) => Some(resume.clone()),
                _ => None,
            })
    }

    pub(crate) fn has_active_catch(&self, tag: &Value) -> bool {
        self.matching_catch_resume(tag).is_some()
    }

    pub(crate) fn has_active_condition_handler_for_signal(&self, sig: &SignalData) -> bool {
        self.condition_stack.iter().rev().any(|frame| match frame {
            ConditionFrame::ConditionCase { conditions, .. }
            | ConditionFrame::HandlerBind { conditions, .. } => {
                crate::emacs_core::errors::signal_matches_condition_value_sym(
                    &self.obarray,
                    sig.symbol,
                    conditions,
                )
            }
            _ => false,
        })
    }

    pub(crate) fn dispatch_signal_if_needed(
        &mut self,
        sig: Box<SignalData>,
    ) -> Result<Box<SignalData>, Flow> {
        if sig.search_complete {
            return Ok(sig);
        }
        self.dispatch_signal(*sig).map(Box::new)
    }

    /// `#[inline]`: this runs on every call return; the Ok arm is a pure
    /// pass-through that should vanish into the caller instead of paying an
    /// out-of-line 24-byte Result round trip per call (measured ~2.3% flat
    /// of a call-heavy interpreter benchmark as a standalone function).
    #[inline]
    pub(crate) fn dispatch_signal_result_if_needed(&mut self, result: EvalResult) -> EvalResult {
        match result {
            Err(Flow::Signal(sig)) => match self.dispatch_signal_if_needed(sig) {
                Ok(dispatched) => Err(Flow::Signal(dispatched)),
                Err(flow) => Err(flow),
            },
            other => other,
        }
    }

    fn dispatch_signal(&mut self, mut sig: SignalData) -> Result<SignalData, Flow> {
        self.run_signal_hook(&sig)?;
        sig = self.canonicalize_signal_symbol(sig);

        let mut idx = self.condition_stack.len();
        let mut seen_condition_entries = 0usize;

        while let Some(next_idx) = idx.checked_sub(1) {
            idx = next_idx;
            match self.condition_stack[idx].clone() {
                ConditionFrame::Catch { .. } => {}
                ConditionFrame::SkipConditions { remaining } => {
                    let mut to_skip = remaining;
                    while idx > 0 && to_skip > 0 {
                        idx -= 1;
                        if matches!(
                            self.condition_stack[idx],
                            ConditionFrame::ConditionCase { .. }
                                | ConditionFrame::HandlerBind { .. }
                        ) {
                            to_skip -= 1;
                        }
                    }
                }
                ConditionFrame::ConditionCase { conditions, resume } => {
                    seen_condition_entries += 1;
                    if crate::emacs_core::errors::signal_matches_condition_value_sym(
                        &self.obarray,
                        sig.symbol,
                        &conditions,
                    ) {
                        self.maybe_call_debugger_for_signal(&sig, Some(&conditions))?;
                        sig.selected_resume = Some(resume);
                        sig.search_complete = true;
                        return Ok(sig);
                    }
                }
                ConditionFrame::HandlerBind {
                    conditions,
                    handler,
                    mute_span,
                } => {
                    seen_condition_entries += 1;
                    if !crate::emacs_core::errors::signal_matches_condition_value_sym(
                        &self.obarray,
                        sig.symbol,
                        &conditions,
                    ) {
                        continue;
                    }

                    let specpdl_root_scope = self.save_specpdl_roots();
                    for value in &sig.data {
                        self.push_specpdl_root(*value);
                    }
                    if let Some(raw) = &sig.raw_data {
                        self.push_specpdl_root(*raw);
                    }

                    self.push_condition_frame(ConditionFrame::SkipConditions {
                        remaining: seen_condition_entries + mute_span,
                    });

                    let handler_result = self.apply(handler, vec![make_signal_binding_value(&sig)]);

                    match handler_result {
                        Ok(_) => {
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            continue;
                        }
                        Err(Flow::Signal(next_sig)) => {
                            let dispatched =
                                self.dispatch_signal_if_needed(next_sig).map(|sig| *sig);
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            return dispatched;
                        }
                        Err(flow @ Flow::Throw(_)) => {
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            return Err(flow);
                        }
                        Err(flow @ (Flow::ThreadBlocked(_) | Flow::Shutdown(_))) => {
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            return Err(flow);
                        }
                    }
                }
            }
        }

        self.maybe_call_debugger_for_signal(&sig, None)?;
        // No handler matched: this signal will propagate to the command loop.
        // Capture the live Lisp backtrace NOW, while the specpdl is still intact
        // (unwinding happens as `Ok(sig)` propagates up), so the command loop's
        // error log can show WHERE it was signaled without the reporter needing
        // `debug-on-error`. Gated on debug-level tracing — the default filter is
        // `warn`, so production only pays a cheap `enabled!` check, and only
        // truly-uncaught signals are ever rendered.
        let captured_backtrace = if tracing::enabled!(tracing::Level::DEBUG) {
            Some(self.render_uncaught_signal_backtrace(64))
        } else {
            None
        };
        self.last_uncaught_signal_backtrace = captured_backtrace;
        sig.search_complete = true;
        sig.selected_resume = None;
        Ok(sig)
    }

    /// Render a compact snapshot of the live Lisp backtrace (innermost frame
    /// first), like GNU `backtrace`, for the command-loop error log. Bounded to
    /// `max_frames`. Only invoked under a debug-tracing gate — it prints every
    /// live frame's function and arguments.
    /// Render the current Lisp backtrace for diagnostics (public alias of the
    /// uncaught-signal renderer, used by env-gated observability hooks).
    pub(crate) fn render_lisp_backtrace(&self, max_frames: usize) -> String {
        self.render_uncaught_signal_backtrace(max_frames)
    }

    fn render_uncaught_signal_backtrace(&self, max_frames: usize) -> String {
        let mut lines: Vec<String> = Vec::new();
        for entry in self.specpdl.iter().rev() {
            let Some((function, args, _, _)) = self.backtrace_entry_values(entry) else {
                continue;
            };
            if lines.len() >= max_frames {
                lines.push("    ...".to_string());
                break;
            }
            let fn_str = crate::emacs_core::print_value_with_eval(self, &function);
            let arg_strs: Vec<String> = args
                .iter()
                .map(|v| crate::emacs_core::print_value_with_eval(self, v))
                .collect();
            lines.push(if arg_strs.is_empty() {
                format!("    ({fn_str})")
            } else {
                format!("    ({fn_str} {})", arg_strs.join(" "))
            });
        }
        lines.join("\n")
    }

    fn run_signal_hook(&mut self, sig: &SignalData) -> Result<(), Flow> {
        if sig.suppress_signal_hook {
            return Ok(());
        }

        let hook = self
            .obarray
            .symbol_value("signal-hook-function")
            .copied()
            .unwrap_or(Value::NIL);
        if hook.is_nil() {
            return Ok(());
        }

        self.apply(
            hook,
            vec![
                Value::from_sym_id(sig.symbol),
                signal_hook_payload_value(sig),
            ],
        )
        .map(|_| ())
    }

    fn canonicalize_signal_symbol(&self, sig: SignalData) -> SignalData {
        if sig.symbol == error_symbol() || sig.symbol == quit_symbol() {
            return sig;
        }
        // GNU `signal_or_quit` reads `error-conditions` directly from the
        // signalled symbol object (`Fget (real_error_symbol, Qerror_conditions)`,
        // src/eval.c:1959), so an *uninterned* error symbol created via
        // `make-symbol` and given conditions by `define-error` is honoured.
        // Looking the property up by name (which interns) would resolve to a
        // *different*, interned symbol with no conditions and spuriously
        // canonicalize to "Invalid error symbol".  Use identity instead.
        if self
            .obarray
            .get_property_id(sig.symbol, error_conditions_symbol())
            .is_some()
        {
            return sig;
        }

        SignalData::new(
            error_symbol(),
            vec![
                Value::string("Invalid error symbol"),
                Value::from_sym_id(sig.symbol),
            ],
            None,
            sig.suppress_signal_hook,
        )
    }

    fn maybe_call_debugger_for_signal(
        &mut self,
        sig: &SignalData,
        matched_clause: Option<&Value>,
    ) -> Result<(), Flow> {
        if self
            .obarray
            .symbol_value("inhibit-debugger")
            .is_some_and(|value| !value.is_nil())
        {
            return Ok(());
        }

        let debug_on_signal = self
            .obarray
            .symbol_value("debug-on-signal")
            .is_some_and(|value| !value.is_nil());
        let should_consider_debugger = debug_on_signal
            || matched_clause.is_none()
            || matched_clause.is_some_and(condition_value_contains_debug);
        if !should_consider_debugger {
            return Ok(());
        }

        let conditions = self.signal_conditions_value(sig);
        let debug_setting = if crate::emacs_core::errors::signal_matches_condition_value_sym(
            &self.obarray,
            sig.symbol,
            &Value::from_sym_id(quit_symbol()),
        ) {
            self.obarray
                .symbol_value("debug-on-quit")
                .copied()
                .unwrap_or(Value::NIL)
        } else {
            self.obarray
                .symbol_value("debug-on-error")
                .copied()
                .unwrap_or(Value::NIL)
        };
        if !wants_debugger(&debug_setting, &conditions) {
            return Ok(());
        }
        if self.skip_debugger(sig, &conditions)? {
            return Ok(());
        }
        // GNU's last conjunct, and it is last there too: "See commentary on
        // definition of `internal-when-entered-debugger'" (`src/eval.c:2210-2212`).
        // A debugger that signals must not re-enter itself, so one entry per
        // non-macro input event is the budget -- which in batch, where no such
        // event ever arrives, is one entry per session.
        if !self.debugger_reentry_is_permitted() {
            return Ok(());
        }

        self.call_debugger_for_signal(sig)
    }

    fn signal_conditions_value(&self, sig: &SignalData) -> Value {
        // Read `error-conditions' by identity so an uninterned error symbol
        // (created via `make-symbol' + `define-error') yields its real
        // condition list for `condition-case' clause matching, instead of the
        // empty/`(SYMBOL)' fallback a name-based lookup of a different interned
        // symbol would give.
        self.obarray
            .get_property_id(sig.symbol, error_conditions_symbol())
            .unwrap_or_else(|| Value::list(vec![Value::from_sym_id(sig.symbol)]))
    }

    fn skip_debugger(&mut self, sig: &SignalData, conditions: &Value) -> Result<bool, Flow> {
        let ignored = self
            .obarray
            .symbol_value("debug-ignored-errors")
            .copied()
            .unwrap_or(Value::NIL);
        let Some(entries) = list_to_vec(&ignored) else {
            return Ok(false);
        };
        if entries.is_empty() {
            return Ok(false);
        }

        let mut error_message = None;
        let error_data = make_signal_binding_value(sig);
        let signal_conditions = list_to_vec(conditions).unwrap_or_else(|| vec![*conditions]);

        for entry in entries {
            if entry.is_string() {
                let message = if let Some(message) = error_message {
                    message
                } else {
                    let rendered = crate::emacs_core::errors::builtin_error_message_string(
                        self,
                        vec![error_data],
                    )?;
                    error_message = Some(rendered);
                    rendered
                };

                let current_buffer = self.buffers.current_buffer();
                let syntax_table =
                    current_buffer.map(crate::emacs_core::syntax::SyntaxTable::for_buffer);
                let category_table = Some(
                    crate::emacs_core::category::active_category_table_for_buffer(current_buffer)?,
                );
                let word_boundary = builtins::search::current_word_boundary_lookup(self);
                let syntax_properties = builtins::search::current_string_match_syntax_properties(
                    self,
                    &self.obarray,
                    &self.buffers,
                    Some(&message),
                );
                if builtins::search::builtin_string_match_p_with_case_fold(
                    false,
                    None,
                    syntax_table.as_ref(),
                    category_table,
                    word_boundary,
                    syntax_properties,
                    &[entry, message],
                )?
                .as_fixnum()
                .is_some()
                {
                    return Ok(true);
                }
                continue;
            }

            if signal_conditions.contains(&entry) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn call_debugger_for_signal(&mut self, sig: &SignalData) -> Result<(), Flow> {
        let rendered = super::error::format_signal_data_with_eval(self, sig);
        tracing::error!(
            "entering Lisp debugger for signal: symbol={} data={}",
            format_symbol_name_for_diagnostic(sig.symbol),
            rendered
        );
        // GNU `call_debugger (list2 (Qdebug, ...))` from `maybe_call_debugger`:
        // one shared entry point with the `debug-on-next-call` and
        // `debug-on-exit` sites, so the bindings it installs -- and the
        // `debug_on_next_call = 0` at `src/eval.c:298` -- cannot be one thing
        // for a signal and another thing for a call.
        self.call_debugger(vec![Value::symbol("error"), make_signal_binding_value(sig)])
            .map(|_| ())
    }

    /// GNU emacs.c / data.c / fns.c-level startup globals: version and
    /// platform identity, invocation paths, subprocess program names, the
    /// load/exec path environment, and process/terminal defaults. Pulled
    /// out of new_inner so the constructor reads as a sequence of phases
    /// (and stays small enough for reliable debug codegen; see the
    /// init_builtins note in Context::new).
    fn seed_startup_platform_variables(obarray: &mut Obarray, default_directory: String) {
        // Set up standard global variables
        // Match GNU data.c: DEFVAR_LISP marks these symbols declared-special,
        // then make_symbol_constant installs the SYMBOL_NOWRITE trap.
        obarray.set_symbol_value("most-positive-fixnum", Value::fixnum(i64::MAX >> 2));
        obarray.make_special("most-positive-fixnum");
        obarray.set_constant("most-positive-fixnum");
        obarray.set_symbol_value("most-negative-fixnum", Value::fixnum(-(i64::MAX >> 2) - 1));
        obarray.make_special("most-negative-fixnum");
        obarray.set_constant("most-negative-fixnum");
        // Mathematical constants (defconst in float-sup.el)
        obarray.set_symbol_value("float-e", Value::make_float(std::f64::consts::E));
        obarray.set_symbol_value("float-pi", Value::make_float(std::f64::consts::PI));
        obarray.set_symbol_value("pi", Value::make_float(std::f64::consts::PI));
        obarray.set_symbol_value("emacs-version", Value::string(crate::GNU_EMACS_VERSION));
        obarray.make_special("emacs-version");
        obarray.set_symbol_value(
            "emacs-copyright",
            Value::string("Copyright (C) 2026 Free Software Foundation, Inc."),
        );
        obarray.make_special("emacs-copyright");
        obarray.set_symbol_value("emacs-major-version", Value::fixnum(31));
        obarray.set_symbol_value("emacs-minor-version", Value::fixnum(0));
        obarray.set_symbol_value("emacs-build-number", Value::fixnum(1));
        obarray.set_symbol_value("system-type", Value::symbol(gnu_system_type()));
        obarray.make_special("system-type");
        // GNU Emacs uses unibyte for default-directory during dump because
        // the locale isn't set up yet (see init_buffer in buffer.c).
        obarray.set_symbol_value(
            "default-directory",
            Value::unibyte_string(default_directory.clone()),
        );
        obarray.set_symbol_value(
            "command-line-default-directory",
            Value::unibyte_string(default_directory),
        );
        let obarray_object = Value::vector(vec![Value::NIL]);
        obarray.set_symbol_value("obarray", obarray_object);
        obarray.set_symbol_value("neovm--obarray-object", obarray_object);
        obarray.make_special("obarray");
        obarray.set_symbol_value("standard-input", Value::T);
        obarray.make_special("standard-input");
        obarray.set_symbol_value(
            "command-line-args",
            Value::list(vec![
                Value::string("neovm-worker"),
                Value::string("--batch"),
            ]),
        );
        obarray.make_special("command-line-args");
        obarray.set_symbol_value("command-line-args-left", Value::NIL);
        obarray.set_symbol_value("command-line-functions", Value::NIL);
        obarray.set_symbol_value("command-line-processed", Value::T);
        obarray.set_symbol_value("command-switch-alist", Value::NIL);
        obarray.set_symbol_value(
            "pdumper-fingerprint",
            Value::string(crate::emacs_core::pdump::fingerprint_hex()),
        );
        obarray.make_special("pdumper-fingerprint");
        // GNU emacs.c: set from argv[0]. NeoVM uses current exe path.
        let exe_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok());
        let invocation_name = exe_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "neomacs".to_string());
        let invocation_directory = exe_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| format!("{}/", d.to_string_lossy()))
            .unwrap_or_else(|| "./".to_string());
        obarray.set_symbol_value("invocation-name", Value::string(invocation_name));
        obarray.make_special("invocation-name");
        obarray.set_symbol_value("invocation-directory", Value::string(invocation_directory));
        obarray.make_special("invocation-directory");
        obarray.set_symbol_value("installation-directory", Value::NIL);
        obarray.make_special("installation-directory");
        // GNU `callproc.c` initializes this from the build-time `PATH_INFO`
        // (`epaths.h`, default "/usr/local/share/info"), never nil.  Lisp
        // assumes it is a string: `Info--default-directory-list` runs
        // `(file-name-as-directory configure-info-directory)`, which errors
        // with `(wrong-type-argument stringp nil)` when nil and breaks
        // `doom sync` (GitHub issue #127).  Mirror GNU's default constant.
        obarray.set_symbol_value(
            "configure-info-directory",
            Value::string("/usr/local/share/info"),
        );
        // GNU keyboard.c: internal--top-level-message for command loop entry
        obarray.set_symbol_value(
            "internal--top-level-message",
            Value::string("Back to top level"),
        );
        // charset.c:2426 DEFVAR_LISP, init nil.
        obarray.define_special_variable("charset-map-path", Value::NIL);
        obarray.set_symbol_value("doc-directory", Value::NIL);
        // warnings.el defcustom — needed before warnings.el loads
        obarray.set_symbol_value("warning-minimum-log-level", Value::keyword(":warning"));
        obarray.set_symbol_value("warning-minimum-level", Value::keyword(":warning"));
        // GNU callproc.c defines these with DEFVAR_LISP, so lexical-binding
        // Lisp must treat them as dynamically scoped special variables.
        obarray.set_symbol_value("process-environment", Value::NIL);
        obarray.make_special("process-environment");
        obarray.set_symbol_value("initial-environment", Value::NIL);
        obarray.make_special("initial-environment");
        // GNU uses "emacsclient" here because the matching client is part of
        // its installation.  Neomacs must advertise the client it owns, so
        // package probes do not accidentally select a host GNU emacsclient.
        for (name, program) in [
            ("ctags-program-name", "ctags"),
            ("etags-program-name", "etags"),
            ("hexl-program-name", "hexl"),
            ("emacsclient-program-name", "neomacsclient"),
            ("movemail-program-name", "movemail"),
            ("ebrowse-program-name", "ebrowse"),
            ("rcs2log-program-name", "rcs2log"),
        ] {
            obarray.set_symbol_value(name, Value::unibyte_string(program));
            obarray.make_special(name);
        }
        obarray.set_symbol_value("path-separator", Value::string(":"));
        obarray.make_special("path-separator");
        obarray.set_symbol_value("shared-game-score-directory", Value::NIL);
        obarray.set_symbol_value("system-messages-locale", Value::NIL);
        obarray.make_special("system-messages-locale");
        obarray.set_symbol_value("system-time-locale", Value::NIL);
        obarray.make_special("system-time-locale");
        obarray.set_symbol_value("before-init-time", Value::NIL);
        obarray.make_special("before-init-time");
        obarray.set_symbol_value("after-init-time", Value::NIL);
        obarray.make_special("after-init-time");
        obarray.set_symbol_value(
            "system-configuration",
            super::builtins_extra::system_configuration_value(),
        );
        obarray.make_special("system-configuration");
        obarray.set_symbol_value(
            "system-configuration-options",
            super::builtins_extra::system_configuration_options_value(),
        );
        obarray.make_special("system-configuration-options");
        obarray.set_symbol_value(
            "system-configuration-features",
            super::builtins_extra::system_configuration_features_value(),
        );
        obarray.make_special("system-configuration-features");
        // GNU `keyboard.c` defines this with DEFVAR_LISP, so lexical-binding
        // Lisp must treat it as dynamically scoped.
        obarray.set_symbol_value("delayed-warnings-list", Value::NIL);
        obarray.make_special("delayed-warnings-list");
        // GNU `subr.el` defines this with `defvar`; seed it for early warning
        // paths while preserving the same special-variable semantics.
        obarray.set_symbol_value("delayed-warnings-hook", Value::NIL);
        obarray.make_special("delayed-warnings-hook");
        obarray.set_symbol_value(
            "command-line-ns-option-alist",
            Value::list(vec![Value::list(vec![
                Value::string("-NSOpen"),
                Value::fixnum(1),
                Value::symbol("ns-handle-nxopen"),
            ])]),
        );
        obarray.set_symbol_value(
            "command-line-x-option-alist",
            Value::list(vec![Value::list(vec![
                Value::string("-display"),
                Value::fixnum(1),
                Value::symbol("x-handle-display"),
            ])]),
        );
        obarray.set_symbol_value("load-path", Value::NIL);
        obarray.make_special("load-path");
        obarray.set_symbol_value("load-history", Value::NIL);
        obarray.set_symbol_value(
            "fontset-alias-alist",
            super::builtins::symbols::fontset_alias_alist_startup_value(),
        );
        // GNU Emacs with module support includes the module suffixes before
        // compiled and source Lisp suffixes, secondary suffix first -- on darwin
        // `(".so" ".dylib" ".elc" ".el")`, see `load_suffixes_startup_values_for_os`.
        obarray.set_symbol_value(
            "load-suffixes",
            Value::list(
                super::lread::load_suffixes_startup_values_for_os(std::env::consts::OS)
                    .into_iter()
                    .map(Value::string)
                    .collect(),
            ),
        );
        obarray.make_special("load-suffixes");
        obarray.set_symbol_value(
            "module-file-suffix",
            Value::make_string(super::lread::module_file_suffix()),
        );
        obarray.make_special("module-file-suffix");
        obarray.set_symbol_value(
            "dynamic-library-suffixes",
            Value::list(
                super::lread::dynamic_library_suffixes_for_os(std::env::consts::OS)
                    .into_iter()
                    .map(Value::string)
                    .collect(),
            ),
        );
        obarray.make_special("dynamic-library-suffixes");
        obarray.set_symbol_value("dynamic-library-alist", Value::NIL);
        obarray.make_special("dynamic-library-alist");
        let dynamic_library_alist = intern("dynamic-library-alist");
        obarray
            .put_property_id(
                dynamic_library_alist,
                intern("risky-local-variable"),
                Value::T,
            )
            .expect("setting dynamic-library-alist property should not fail");
        // load-file-rep-suffixes: suffixes for alternate representations of
        // the same file (e.g., compressed ".gz").  Default is just ("").
        obarray.set_symbol_value(
            "load-file-rep-suffixes",
            Value::list(vec![Value::string("")]),
        );
        obarray.make_special("load-file-rep-suffixes");
        // file-coding-system-alist: needed by jka-cmpr-hook.el and others.
        obarray.set_symbol_value("file-coding-system-alist", Value::NIL);
        // GNU fns.c initializes `features' to include `emacs', and
        // thread.c:syms_of_threads provides `threads' when thread builtins
        // are installed.
        obarray.set_symbol_value("features", initial_features_value());
        super::xwidget::init_xwidget_variables(obarray);
        obarray.set_symbol_value_id(lexical_binding_symbol(), Value::NIL);
        obarray.set_symbol_value("load-file-name", Value::NIL);
        obarray.make_special("load-file-name");
        obarray.set_symbol_value("inhibit-quit", Value::NIL);
        obarray.set_symbol_value("float-output-format", Value::NIL);
        obarray.make_special("float-output-format");
        // GNU Emacs print.c: all print-* variables are DEFVAR_BOOL or
        // DEFVAR_LISP, making them dynamically scoped (special).
        // This is essential so `(let ((print-escape-newlines t)) ...)`
        // affects the C print code via dynamic binding.
        for name in [
            "print-length",
            "print-level",
            "print-circle",
            "print-gensym",
            "print-continuous-numbering",
            "print-number-table",
            "print-charset-text-property",
            "print-unreadable-function",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        obarray.set_symbol_value("text-quoting-style", Value::NIL);
        obarray.make_special("text-quoting-style");
        // GNU DEFVAR_LISP variables needed by loadup.el and early .el files.
        // chartab.c:1375 DEFVAR_LISP, init nil.
        obarray.define_special_variable("char-code-property-alist", Value::NIL);
        // redisplay--inhibit-bidi and resize-mini-windows are registered (with
        // GNU xdisp.c inits) by xdisp::register_bootstrap_vars.

        // GNU C variables checked by cus-start.el during bootstrap.
        // 178 DEFVAR_LISP/DEFVAR_INT/DEFVAR_BOOL variables extracted from
        // GNU Emacs -Q. Default values match GNU's init_*() functions.
        for name in [
            "alter-fullscreen-frames",
            "auto-save-visited-file-name",
            "blink-cursor-alist",
            "default-frame-alist",
            "display-fill-column-indicator-character",
            "display-line-numbers",
            "display-line-numbers-width",
            "enable-character-translation",
            "focus-follows-mouse",
            "line-number-display-limit",
            "make-pointer-invisible",
            "menu-bar-mode",
            "mode-line-compact",
            "mouse-autoselect-window",
            "resize-mini-frames",
            "ring-bell-function",
            "scalable-fonts-allowed",
            "scroll-preserve-screen-position",
            "show-trailing-whitespace",
            "tab-bar-mode",
            "tab-bar-position",
            "temp-buffer-show-function",
            "tool-bar-mode",
            "tool-bar-style",
            "treesit-extra-load-path",
            "treesit-auto-install-grammar",
            "treesit-enabled-modes",
            "treesit-language-remap-alist",
            "treesit-load-name-override-list",
            "treesit-languages-require-line-column-tracking",
            "treesit-major-mode-remap-alist",
            "treesit-thing-settings",
            // undo-outer-limit is registered (with its GNU src/undo.c init and
            // the src/emacs.c batch override) by undo::register_bootstrap_vars.
            "window-combination-resize",
            // Mouse pointer shapes — GNU defines these in
            // src/xfns.c (and parallel files w32fns.c, pgtkfns.c,
            // haikufns.c, androidfns.c) as integer Lisp_Object
            // variables that hold X cursor font codes. neomacs has
            // no native window-system bindings for these yet, so
            // they default to nil. Cursor audit Finding 9 in
            // drafts/cursor-audit.md flagged the symbols as
            // missing entirely; Lisp code that tried
            // (setq x-pointer-shape ...) hit void-variable.
            //
            // `x-nontext-pointer-shape' and `x-mode-pointer-shape' are NOT in
            // this list, and the omission is the point.  Every `DEFVAR_LISP'
            // GNU has for either is inside
            // `#if false /* This doesn't really do anything.  */' --
            // `src/xfns.c:10333-10338' and `10347-10352', and the same pair in
            // `src/androidfns.c'; `w32fns.c' and `haikufns.c' do not declare
            // them at all.  A declaration in a dead preprocessor branch is not
            // a declaration, so no GNU build binds the symbol and
            // `(boundp 'x-mode-pointer-shape)' is nil under GNU 31.0.90.
            // Seeding one here is entry 138's invented existence, reached
            // through a case that is not about a platform: the C global
            // `Vx_mode_pointer_shape' still exists and is still assigned
            // `Qnil' on the line after the `#endif', which is what makes the
            // seed look justified from the C side.  Nothing in GNU's `lisp/',
            // this tree's `lisp/', or either editor's own sources reads
            // either name.  (Ledger 168.)
            "x-pointer-shape",
            "x-sensitive-text-pointer-shape",
            "x-hourglass-pointer-shape",
            "x-window-horizontal-drag-cursor",
            "x-window-vertical-drag-cursor",
            "x-window-left-edge-cursor",
            "x-window-top-left-corner-cursor",
            "x-window-top-edge-cursor",
            "x-window-top-right-corner-cursor",
            "x-window-right-edge-cursor",
            "x-window-bottom-right-corner-cursor",
            "x-window-bottom-edge-cursor",
            "x-window-bottom-left-corner-cursor",
            "x-cursor-fore-pixel",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        // GNU `frame.c` initializes these global minor-mode variables in C:
        //   Vmenu_bar_mode = Qt
        //   Vtool_bar_mode = Qt   (when built with window-system support)
        // neomacs is a window-system-capable build, so match GNU's defaults
        // instead of starting graphical sessions with both modes forced off.
        obarray.set_symbol_value("menu-bar-mode", Value::T);
        obarray.set_symbol_value("tool-bar-mode", Value::T);
        for name in [
            "auto-hscroll-mode",
            "display-fill-column-indicator-column",
            "display-line-numbers-current-absolute",
            "make-cursor-line-fully-visible",
            "mouse-highlight",
            "overflow-newline-into-fringe",
            "select-active-regions",
            "x-select-enable-clipboard-manager",
        ] {
            obarray.set_symbol_value(name, Value::T);
            obarray.make_special(name);
        }
        // auto-save-interval/timeout, double-click-fuzz/time, meta-prefix-char
        // and polling-period are registered (with GNU keyboard.c values and
        // DEFVAR specialness) by keyboard::pure::register_bootstrap_vars.
        obarray.define_int_variable("display-line-numbers-major-tick", 0);
        obarray.define_int_variable("display-line-numbers-minor-tick", 0);
        obarray.define_special_variable("echo-keystrokes", Value::fixnum(1));
        obarray.define_int_variable("gc-cons-threshold", 800_000);
        obarray.set_symbol_value("help-char", Value::fixnum(8));
        // hourglass-delay, hscroll-margin/step, line-number-display-limit-width,
        // maximum-scroll-margin, messages-buffer-name, scroll-* and the
        // tool-bar label size are registered (with GNU xdisp.c values and
        // DEFVAR specialness) by xdisp::register_bootstrap_vars.
        obarray.set_symbol_value("message-log-max", Value::fixnum(1000));
        // next-screen-context-lines is registered by
        // window_cmds::register_bootstrap_vars; overline-margin by
        // xdisp::register_bootstrap_vars.
        obarray.define_int_variable("process-error-pause-time", 1);
        obarray.set_symbol_value("eol-mnemonic-dos", Value::string("\\"));
        obarray.set_symbol_value("eol-mnemonic-mac", Value::string("/"));
        obarray.set_symbol_value("eol-mnemonic-undecided", Value::string(":"));
        obarray.set_symbol_value("eol-mnemonic-unix", Value::string(":"));
        obarray.set_symbol_value(
            "report-emacs-bug-address",
            Value::string("bug-gnu-emacs@gnu.org"),
        );
        obarray.make_special("report-emacs-bug-address");
        // fns.c:6867 DEFVAR_LISP, build_unibyte_string ("(yes or no) ").
        obarray.define_special_variable("yes-or-no-prompt", Value::string("(yes or no) "));
        // Float-valued C variables
        obarray.set_symbol_value("gc-cons-percentage", Value::make_float(0.1));
        // max-mini-window-height is registered by xdisp::register_bootstrap_vars.
        // `max-image-size', `image-scaling-factor', `image-cache-eviction-delay',
        // `image-types' and `x-bitmap-file-path' are registered by
        // image::register_bootstrap_vars, GNU's `syms_of_image'.
        // Display engine C variables (xdisp.c)
        obarray.define_special_variable("global-mode-string", Value::NIL);
        // Fringe C variable (fringe.c `syms_of_fringe`: `Vfringe_bitmaps = Qnil`).
        // GNU binds this to nil; `lisp/fringe.el` then guards its standard-bitmap
        // seeding and `fringe-indicator-alist`/`fringe-cursor-alist` defaults on
        // `(boundp 'fringe-bitmaps)`, and `push`es each bitmap symbol onto it.
        // Binding it here lets fringe.el install those defaults in Lisp (GNU's
        // own path) instead of hardcoding the alists in Rust.
        obarray.set_symbol_value("fringe-bitmaps", Value::NIL);
        obarray.make_special("fringe-bitmaps");
        // File loading C variables (lread.c)
        // Process/daemon C variables (process.c)
        obarray.set_symbol_value("internal--daemon-sockname", Value::NIL);
        // Other missing C variables cus-start.el checks
        obarray.set_symbol_value("history-length", Value::fixnum(100));
        obarray.make_special("history-length");
        // minibuf.c:2538 DEFVAR_LISP, init Qt.
        obarray.define_special_variable("minibuffer-follows-selected-frame", Value::T);
        obarray.set_symbol_value("recenter-redisplay", Value::symbol("tty"));
        // frame.c:7733 DEFVAR_LISP, init Qiconify_top_level.
        obarray.define_special_variable("iconify-child-frame", Value::symbol("iconify-top-level"));
        // frame-inhibit-implied-resize is registered by
        // frame_vars::register_bootstrap_vars with GNU's GUI default.
        obarray.set_symbol_value("mark-even-if-inactive", Value::T);
        // minibuf.c:2533 DEFVAR_LISP, init nil.
        obarray.define_special_variable("read-buffer-function", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-prompt-properties",
            Value::list(vec![Value::symbol("read-only"), Value::T]),
        );
        obarray.set_symbol_value("help-event-list", Value::NIL);
        // GNU `keyboard.c:14127`:
        //   DEFVAR_LISP ("prefix-help-command", Vprefix_help_command, ...);
        //   Vprefix_help_command = intern_c_string ("describe-prefix-bindings");
        // The default is consulted by `read_key_sequence` when the
        // help-char fires after a prefix. Keyboard audit Finding 5
        // in `drafts/keyboard-command-loop-audit.md`.
        obarray.define_special_variable(
            "prefix-help-command",
            Value::symbol("describe-prefix-bindings"),
        );
        obarray.set_symbol_value("debug-ignored-errors", Value::NIL);
        // debug-on-event is registered (init sigusr2, keyboard.c:14358) by
        // keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("debug-on-signal", Value::NIL);
        // Remaining cus-start.el variables (general + platform names).
        // `temporary-file-directory' is not one of the platform names -- GNU
        // declares it in `filelock.c:814' for every build, with the same nil
        // init -- so it keeps its own seed here.
        obarray.set_symbol_value("temporary-file-directory", Value::NIL);
        // The 32 names whose C declaration belongs to a platform are a table
        // in `cus_start_platform_vars', and that table seeds NOTHING.  25 of
        // them are ones GNU leaves UNBOUND in a build like this one, so
        // seeding those made `boundp' disagree with GNU (entry 138); the other
        // 7 GNU declares with a `DEFVAR_LISP' that supplies a value AND the
        // `declared_special' bit, so a nil seed disagreed with GNU on both
        // (entry 141).  Each of the 7 is declared at the Neomacs counterpart
        // of its `syms_of_*', named in its table row.

        // GNU DEFVAR_LISP variables from lread.c that must be bound to nil
        // before any Elisp runs (code may test `boundp` or read them directly).
        //
        // Keep GNU's exception for `values`: `lread.c` defines it via
        // `DEFVAR_LISP` and then explicitly clears the declared-special bit,
        // so it remains an ordinary variable even under lexical binding.
        obarray.set_symbol_value("values", Value::NIL);
        obarray.set_symbol_value("eval-buffer-list", Value::NIL);
        obarray.make_special("eval-buffer-list");
        obarray.set_symbol_value("lread--unescaped-character-literals", Value::NIL);
        obarray.make_special("lread--unescaped-character-literals");
        obarray.set_symbol_value("load-read-function", Value::symbol("read"));
        obarray.make_special("load-read-function");
        obarray.set_symbol_value("load-source-file-function", Value::NIL);
        obarray.make_special("load-source-file-function");
        obarray.set_symbol_value("load-true-file-name", Value::NIL);
        obarray.make_special("load-true-file-name");
        obarray.set_symbol_value("user-init-file", Value::NIL);
        obarray.make_special("user-init-file");
        obarray.set_symbol_value("source-directory", Value::NIL);
        obarray.make_special("source-directory");
        obarray.set_symbol_value("after-load-alist", Value::NIL);
        obarray.make_special("after-load-alist");
        obarray.set_symbol_value("load-history", Value::NIL);
        obarray.make_special("load-history");
        obarray.set_symbol_value("current-load-list", Value::NIL);
        obarray.make_special("current-load-list");
        obarray.set_symbol_value("preloaded-file-list", Value::NIL);
        obarray.make_special("preloaded-file-list");
        // `Obarray::define_bool_variable` conses onto this list, the way GNU's
        // `defvar_bool` does (`src/lread.c:5261`), so only seed the empty list
        // when nothing has registered yet -- otherwise bootstrap ordering would
        // decide whether the registrations survive.
        if obarray
            .find_symbol_value(intern("byte-boolean-vars"))
            .is_none()
        {
            obarray.set_symbol_value("byte-boolean-vars", Value::NIL);
        }
        obarray.make_special("byte-boolean-vars");
        obarray.set_symbol_value(
            "bytecomp-version-regexp",
            Value::string(r#"^;;;.\(in Emacs version\|bytecomp version FSF\)"#),
        );
        obarray.make_special("bytecomp-version-regexp");
        obarray.set_symbol_value("load-path-filter-function", Value::NIL);
        obarray.make_special("load-path-filter-function");
        obarray.set_symbol_value("internal--get-default-lexical-binding-function", Value::NIL);
        obarray.make_special("internal--get-default-lexical-binding-function");
        obarray.set_symbol_value("read-symbol-shorthands", Value::NIL);
        obarray.make_special("read-symbol-shorthands");
        obarray.set_symbol_value("macroexp--dynvars", Value::NIL);
        obarray.make_special("macroexp--dynvars");
    }

    /// Reader, printer, keyboard, minibuffer, and display DEFVAR globals
    /// (GNU lread.c / print.c / keyboard.c / minibuf.c / xdisp.c
    /// syms_of_* territory).
    fn seed_reader_keyboard_variables(
        obarray: &mut Obarray,
        standard_syntax_table: Value,
        minibuffer_local_map: Value,
    ) {
        obarray.set_symbol_value("inhibit-debugger", Value::NIL);
        obarray.make_special("inhibit-debugger");
        obarray.set_symbol_value("debug-on-error", Value::NIL);
        obarray.make_special("debug-on-error");
        obarray.set_symbol_value("debug-on-signal", Value::NIL);
        obarray.make_special("debug-on-signal");
        obarray.set_symbol_value("debug-ignored-errors", Value::NIL);
        obarray.make_special("debug-ignored-errors");
        obarray.define_int_variable("internal-when-entered-debugger", -1);
        obarray.set_symbol_value("signal-hook-function", Value::NIL);
        obarray.make_special("signal-hook-function");
        // GNU `eval.c` defines `internal-interpreter-environment` and then
        // immediately `Funintern`s that symbol, so Lisp-visible lookup sees a
        // separate ordinary symbol while the evaluator keeps a hidden special
        // variable for its own lexical-environment bookkeeping.
        obarray.set_symbol_value("internal-make-interpreted-closure-function", Value::NIL);
        obarray.make_special("internal-make-interpreted-closure-function");
        // GNU seeds `debugger` from eval.c before Lisp startup.
        // `eval-expression` relies on it.
        obarray.set_symbol_value("debugger", Value::symbol("debug-early"));
        obarray.make_special("debugger");
        obarray.set_symbol_value("standard-output", Value::T);
        // GNU DEFVAR_INT from dispnew.c — used by bytecomp.el
        // `src/dispnew.c:7488' DEFVAR_INT -- declared with NO initializer
        // beside it and no `init_*' that supplies one either, which is the
        // whole point: the C global lives in `globals' and starts at 0, and the
        // only things that
        // ever write it are `init_baud_rate' from `init_tty'
        // (`src/term.c:4755', `4923') and the `baud_rate = 19200' a window
        // system's terminal init does (`src/xterm.c:32279',
        // `src/pgtkterm.c:7034').  `--batch' creates no terminal, so GNU
        // reports 0 there.  `neomacs-bin' does those two assignments at the
        // same two places; this seed is the zero underneath them.
        obarray.define_int_variable("baud-rate", 0);
        obarray.set_symbol_value("search-slow-speed", Value::fixnum(1200));
        // GNU startup.el sets these based on --debug-init
        obarray.set_symbol_value("init-file-debug", Value::NIL);
        // `src/callproc.c:2240-2252' DEFVAR_INT: `sysconf (_SC_ARG_MAX) / 4'
        // where that is available, else 4096.  GNU divides by four "as a crude
        // way to go bytes->characters"; `multiple-command-partition-arguments'
        // is the caller.  Computed here rather than pinned to a constant for
        // the same reason GNU asks the C library: it is a property of the
        // machine, not of the editor.
        obarray.define_int_variable(
            "command-line-max-length",
            super::callproc::command_line_max_length(),
        );
        // GNU callproc.c: exec-path is built from PATH env var.
        // exec-directory is the directory containing helper programs.
        let exec_path: Vec<Value> = super::load::exec_path_dirs_from_env()
            .into_iter()
            .map(Value::unibyte_string)
            .collect();
        obarray.set_symbol_value("exec-path", Value::list(exec_path));
        obarray.make_special("exec-path");
        obarray.set_symbol_value(
            "exec-directory",
            Value::unibyte_string(
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "/usr/bin/".to_string()),
            ),
        );
        obarray.set_symbol_value(
            "exec-suffixes",
            Value::list(vec![Value::unibyte_string("")]),
        );
        obarray.make_special("exec-suffixes");
        obarray.set_symbol_value("buffer-read-only", Value::NIL);
        obarray.set_symbol_value("left-margin-width", Value::NIL);
        obarray.set_symbol_value("right-margin-width", Value::NIL);
        obarray.set_symbol_value("left-fringe-width", Value::NIL);
        obarray.set_symbol_value("right-fringe-width", Value::NIL);
        obarray.set_symbol_value("fringes-outside-margins", Value::NIL);
        obarray.set_symbol_value("scroll-bar-width", Value::NIL);
        obarray.set_symbol_value("scroll-bar-height", Value::NIL);
        obarray.set_symbol_value("vertical-scroll-bar", Value::T);
        obarray.set_symbol_value("horizontal-scroll-bar", Value::T);
        obarray.set_symbol_value("kill-ring", Value::NIL);
        obarray.set_symbol_value("kill-ring-yank-pointer", Value::NIL);
        obarray.set_symbol_value("last-command", Value::NIL);
        obarray.set_symbol_value("current-fill-column--has-warned", Value::NIL);
        obarray.set_symbol_value("current-input-method", Value::NIL);
        obarray.set_symbol_value("current-input-method-title", Value::NIL);
        // charset.c:2438 DEFVAR_LISP, init nil.
        obarray.define_special_variable("current-iso639-language", Value::NIL);
        // current-key-remap-sequence is registered by
        // keyboard::pure::register_bootstrap_vars.
        // GNU's `current-language-environment` defcustom defaults to "English"
        // (mule-cmds.el:1812), and the dumped image / `-Q` keeps it there.  This
        // value matters during loadup: `set-language-info` (mule-cmds.el:1181)
        // re-applies `set-charset-priority` whenever a language-info KEY is set
        // for the *current* language environment.  Seeding "UTF-8" here made
        // utf-8-lang.el's `(set-language-info-alist "UTF-8" ...)` reorder the
        // charset priority list at dump time (unicode-bmp/unicode to the front),
        // diverging from GNU's raw definition order.  Match GNU's default.
        obarray.set_symbol_value("current-language-environment", Value::string("English"));
        obarray.set_symbol_value(
            "current-load-list",
            Value::list(vec![
                Value::symbol("comp--no-native-compile"),
                Value::cons(
                    Value::symbol("defun"),
                    Value::symbol("load--fixup-all-elns"),
                ),
                Value::symbol("load--eln-dest-dir"),
                Value::symbol("load--bin-dest-dir"),
            ]),
        );
        obarray.set_symbol_value("current-locale-environment", Value::string("C.UTF-8"));
        obarray.set_symbol_value("current-minibuffer-command", Value::NIL);
        obarray.make_special("current-minibuffer-command");
        obarray.set_symbol_value("current-transient-input-method", Value::NIL);
        obarray.set_symbol_value("real-last-command", Value::NIL);
        // last-repeatable-command, this-original-command and defining-kbd-macro
        // are registered by keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("prefix-arg", Value::NIL);
        obarray.set_symbol_value("executing-kbd-macro", Value::NIL);
        obarray.make_special("executing-kbd-macro");
        obarray.define_int_variable("executing-kbd-macro-index", 0);
        obarray.define_c_hook_variable("kbd-macro-termination-hook");
        obarray.set_symbol_value("command-history", Value::NIL);
        obarray.make_special("command-history");
        obarray.set_symbol_value("extended-command-history", Value::NIL);
        obarray.set_symbol_value("read-file-name-completion-ignore-case", Value::NIL);
        obarray.make_special("read-file-name-completion-ignore-case");
        obarray.set_symbol_value("completion-regexp-list", Value::NIL);
        obarray.make_special("completion-regexp-list");
        obarray.set_symbol_value("completion--all-sorted-completions-location", Value::NIL);
        obarray.set_symbol_value("completion--capf-misbehave-funs", Value::NIL);
        obarray.set_symbol_value("completion--capf-safe-funs", Value::NIL);
        obarray.set_symbol_value(
            "completion--embedded-envvar-re",
            Value::string(
                "\\(?:^\\|[^$]\\(?:\\$\\$\\)*\\)\\$\\([[:alnum:]_]*\\|{\\([^}]*\\)\\)\\'",
            ),
        );
        obarray.set_symbol_value("completion--flex-score-last-md", Value::NIL);
        obarray.set_symbol_value("completion-all-sorted-completions", Value::NIL);
        obarray.set_symbol_value(
            "completion--cycling-threshold-type",
            Value::list(vec![Value::symbol("choice")]),
        );
        obarray.set_symbol_value(
            "completion--styles-type",
            Value::list(vec![Value::symbol("repeat")]),
        );
        obarray.set_symbol_value(
            "completion-at-point-functions",
            Value::list(vec![Value::symbol("tags-completion-at-point-function")]),
        );
        obarray.set_symbol_value(
            "completion-setup-hook",
            Value::list(vec![Value::symbol("completion-setup-function")]),
        );
        obarray.set_symbol_value("completion-list-mode-hook", Value::NIL);
        // completion-ignored-extensions is a dired.c DEFVAR_LISP; see
        // `dired::register_bootstrap_vars' below.
        obarray.set_symbol_value(
            "completion-styles",
            Value::list(vec![
                Value::symbol("basic"),
                Value::symbol("partial-completion"),
                Value::symbol("emacs22"),
            ]),
        );
        obarray.set_symbol_value(
            "completion-category-defaults",
            Value::list(vec![
                Value::list(vec![
                    Value::symbol("buffer"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("substring"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("unicode-name"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("substring"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("project-file"),
                    Value::list(vec![Value::symbol("styles"), Value::symbol("substring")]),
                ]),
                Value::list(vec![
                    Value::symbol("xref-location"),
                    Value::list(vec![Value::symbol("styles"), Value::symbol("substring")]),
                ]),
                Value::list(vec![
                    Value::symbol("info-menu"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("substring"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("symbol-help"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("shorthand"),
                        Value::symbol("substring"),
                    ]),
                ]),
                // NB: GNU's `completion-category-defaults' defvar
                // (lisp/minibuffer.el) ends at `symbol-help'.  The
                // `calendar-month' entry is added at runtime by calendar.el's
                // `add-to-list', which is not loaded under `emacs -Q'; don't
                // hardcode it here.
            ]),
        );
        // Do NOT hardcode completion-styles-alist here.
        // GNU defines it via (defvar completion-styles-alist ...)
        // in lisp/minibuffer.el:1158 with all 8 styles including
        // flex, substring, initials, shorthand. defvar only sets
        // the value when the symbol is void, so pre-setting it
        // here would shadow the Lisp definition and lose styles
        // like flex — breaking fido-vertical-mode which requires
        // the flex completion style.
        obarray.set_symbol_value("completion-category-overrides", Value::NIL);
        obarray.set_symbol_value("completion-cycle-threshold", Value::NIL);
        obarray.set_symbol_value("completions-detailed", Value::NIL);
        obarray.set_symbol_value("completions-format", Value::symbol("horizontal"));
        obarray.set_symbol_value("completions-group", Value::NIL);
        obarray.set_symbol_value("completions-group-format", Value::string("     %s  "));
        obarray.set_symbol_value("completions-group-sort", Value::NIL);
        obarray.set_symbol_value(
            "completions-header-format",
            Value::string("%s possible completions:\n"),
        );
        obarray.set_symbol_value(
            "completions-highlight-face",
            Value::symbol("completions-highlight"),
        );
        obarray.set_symbol_value("completions-max-height", Value::NIL);
        obarray.set_symbol_value("completions-sort", Value::symbol("alphabetical"));
        obarray.set_symbol_value("completion-auto-help", Value::T);
        obarray.set_symbol_value("completion-auto-deselect", Value::T);
        obarray.set_symbol_value("completion-auto-select", Value::NIL);
        obarray.set_symbol_value("completion-auto-wrap", Value::T);
        obarray.set_symbol_value("completion-base-position", Value::NIL);
        obarray.set_symbol_value("completion-cycling", Value::NIL);
        obarray.set_symbol_value("completion-extra-properties", Value::NIL);
        obarray.set_symbol_value("completion-fail-discreetly", Value::NIL);
        obarray.set_symbol_value("completion-flex-nospace", Value::NIL);
        obarray.set_symbol_value("completion-in-region--data", Value::NIL);
        obarray.set_symbol_value(
            "completion-in-region-function",
            Value::symbol("completion--in-region"),
        );
        obarray.set_symbol_value("completion-in-region-functions", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode--predicate", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode-hook", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode-predicate", Value::NIL);
        obarray.set_symbol_value("completion-show-help", Value::T);
        obarray.set_symbol_value("completion-show-inline-help", Value::T);
        obarray.set_symbol_value("completion-lazy-hilit", Value::NIL);
        obarray.set_symbol_value("completion-lazy-hilit-fn", Value::NIL);
        obarray.set_symbol_value(
            "completion-list-insert-choice-function",
            Value::symbol("completion--replace"),
        );
        obarray.set_symbol_value("completion-no-auto-exit", Value::NIL);
        obarray.set_symbol_value(
            "completion-pcm--delim-wild-regex",
            Value::string("[-_./:| *]"),
        );
        obarray.set_symbol_value("completion-pcm--regexp", Value::NIL);
        obarray.set_symbol_value(
            "completion-pcm-complete-word-inserts-delimiters",
            Value::NIL,
        );
        obarray.set_symbol_value("completion-pcm-word-delimiters", Value::string("-_./:| "));
        obarray.set_symbol_value("completion-reference-buffer", Value::NIL);
        obarray.set_symbol_value("completion-tab-width", Value::NIL);
        obarray.set_symbol_value("history-length", Value::fixnum(100));
        obarray.make_special("history-length");
        obarray.set_symbol_value("history-add-new-input", Value::T);
        obarray.make_special("history-add-new-input");
        // read-buffer-function is registered above (minibuf.c:2533).
        obarray.set_symbol_value(
            "read-file-name-function",
            Value::symbol("read-file-name-default"),
        );
        // minibuf.c:2528 DEFVAR_LISP, init nil.
        obarray.define_special_variable("read-expression-history", Value::NIL);
        obarray.set_symbol_value("read-number-history", Value::NIL);
        obarray.set_symbol_value("read-char-history", Value::NIL);
        obarray.set_symbol_value("read-answer-short", Value::symbol("auto"));
        obarray.set_symbol_value("read-char-by-name-sort", Value::NIL);
        obarray.set_symbol_value("read-char-choice-use-read-key", Value::NIL);
        obarray.set_symbol_value("read-circle", Value::T);
        obarray.make_special("read-circle");
        obarray.set_symbol_value("read-envvar-name-history", Value::NIL);
        obarray.set_symbol_value("read-face-name-sample-text", Value::string("SAMPLE"));
        obarray.set_symbol_value("read-key-delay", Value::make_float(0.01));
        obarray.set_symbol_value(
            "read-answer-map--memoize",
            Value::hash_table(HashTableTest::Equal),
        );
        obarray.set_symbol_value("read-extended-command-mode", Value::NIL);
        obarray.set_symbol_value("read-extended-command-mode-hook", Value::NIL);
        obarray.set_symbol_value("read-extended-command-predicate", Value::NIL);
        obarray.set_symbol_value("read-hide-char", Value::NIL);
        obarray.set_symbol_value("read-mail-command", Value::symbol("rmail"));
        obarray.set_symbol_value("read-only-mode-hook", Value::NIL);
        obarray.define_int_variable("read-process-output-max", 65536);
        obarray.set_symbol_value("read-quoted-char-radix", Value::fixnum(8));
        obarray.set_symbol_value("read-regexp--case-fold", Value::NIL);
        obarray.set_symbol_value("read-regexp-defaults-function", Value::NIL);
        obarray.set_symbol_value("read-symbol-shorthands", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-frame-alist",
            Value::list(vec![
                Value::cons(Value::symbol("width"), Value::fixnum(80)),
                Value::cons(Value::symbol("height"), Value::fixnum(2)),
            ]),
        );
        obarray.set_symbol_value("minibuffer-inactive-mode-hook", Value::NIL);
        obarray.set_symbol_value("minibuffer-mode-hook", Value::NIL);
        obarray.set_symbol_value("minibuffer-local-map", minibuffer_local_map);
        obarray.set_symbol_value("minibuffer-local-filename-syntax", standard_syntax_table);
        obarray.set_symbol_value("minibuffer-history", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-history-variable",
            Value::symbol("minibuffer-history"),
        );
        obarray.set_symbol_value("minibuffer-history-position", Value::NIL);
        obarray.set_symbol_value("minibuffer-history-isearch-message-overlay", Value::NIL);
        obarray.set_symbol_value("minibuffer-history-search-history", Value::NIL);
        obarray.set_symbol_value("minibuffer-history-sexp-flag", Value::NIL);
        obarray.set_symbol_value("minibuffer-default", Value::NIL);
        obarray.set_symbol_value("minibuffer-default-add-done", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-default-add-function",
            Value::symbol("minibuffer-default-add-completions"),
        );
        obarray.set_symbol_value("minibuffer--original-buffer", Value::NIL);
        obarray.set_symbol_value("minibuffer--regexp-primed", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer--regexp-prompt-regexp",
            Value::string(
                "\\(?:Posix search\\|RE search\\|Search for regexp\\|Query replace regexp\\)",
            ),
        );
        obarray.set_symbol_value("minibuffer--require-match", Value::NIL);
        // minibuffer-follows-selected-frame is registered earlier in bootstrap.
        // GNU src/minibuf.c:2557-2559 DEFVARs this hook and sets it to Qnil.
        // minibuffer.el's `minibuffer--regexp-exit', `minibuffer--nonselected-exit'
        // and `minibuffer-exit-on-screen-keyboard', plus `minibuffer-restore-windows',
        // are all put here by `add-hook' while loadup runs.
        obarray.define_c_hook_variable("minibuffer-exit-hook");
        obarray.set_symbol_value("minibuffer-completion-table", Value::NIL);
        obarray.set_symbol_value("minibuffer-completion-predicate", Value::NIL);
        obarray.set_symbol_value("minibuffer-completion-confirm", Value::NIL);
        // `minibuffer-completion-auto-choose` belongs to minibuffer.el.  Do
        // not pre-bind it here: `defcustom` preserves an existing value, so a
        // Rust seed would override GNU Emacs's Lisp default.
        obarray.set_symbol_value("minibuffer-completion-base", Value::NIL);
        obarray.set_symbol_value("minibuffer-help-form", Value::NIL);
        obarray.set_symbol_value("minibuffer-completing-file-name", Value::NIL);
        // `minibuffer-regexp-mode` belongs to lisp/minibuffer.el:5641, a global
        // `define-minor-mode` whose `defcustom` is initialized by
        // `custom-initialize-after-file-load`.  That initializer ends in
        // `custom-initialize-set` (lisp/custom.el:68-82), which returns without
        // doing anything when the symbol already has a default top-level value.
        // A Rust seed here does not merely duplicate the Lisp default: it
        // suppresses the `:set` function, so the mode body never runs and the
        // mode never installs `minibuffer--regexp-setup` /
        // `minibuffer--regexp-exit`, while the variable still reads t.
        obarray.set_symbol_value("minibuffer-regexp-mode-hook", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-regexp-prompts",
            Value::list(vec![
                Value::string("Posix search"),
                Value::string("RE search"),
                Value::string("Search for regexp"),
                Value::string("Query replace regexp"),
            ]),
        );
        obarray.set_symbol_value("minibuffer-message-clear-timeout", Value::NIL);
        obarray.set_symbol_value("minibuffer-message-overlay", Value::NIL);
        obarray.set_symbol_value("minibuffer-message-properties", Value::NIL);
        // minibuffer-message-timeout is registered by
        // keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("minibuffer-message-timer", Value::NIL);
        obarray.set_symbol_value("minibuffer-lazy-count-format", Value::string("%s "));
        obarray.set_symbol_value("minibuffer-text-before-history", Value::NIL);
        // GNU src/minibuf.c declares these with DEFVAR_LISP/DEFVAR_BOOL.
        // They must be special so lexical-binding Lisp sees dynamic
        // minibuffer/completion bindings inside byte-compiled functions.
        for name in [
            "minibuffer-auto-raise",
            "minibuffer-completion-table",
            "minibuffer-completion-predicate",
            "minibuffer-completion-confirm",
            "minibuffer-completing-file-name",
            "minibuffer-help-form",
            "minibuffer-history-variable",
            "minibuffer-history-position",
            "minibuffer-allow-text-properties",
            "minibuffer-prompt-properties",
            "read-hide-char",
            "inhibit-interaction",
            "read-minibuffer-restore-windows",
        ] {
            obarray.make_special(name);
        }
        obarray.set_symbol_value(
            "minibuffer-prompt-properties",
            Value::list(vec![Value::symbol("read-only"), Value::T]),
        );
        obarray.set_symbol_value("minibuffer-scroll-window", Value::NIL);
        obarray.make_special("minibuffer-scroll-window");
        obarray.set_symbol_value("other-window-scroll-buffer", Value::NIL);
        obarray.make_special("other-window-scroll-buffer");
        obarray.set_symbol_value("other-window-scroll-default", Value::NIL);
        obarray.make_special("other-window-scroll-default");
        obarray.set_symbol_value("minibuffer-visible-completions", Value::NIL);
        obarray.set_symbol_value("minibuffer-visible-completions--always-bind", Value::NIL);
        obarray.set_symbol_value("minibuffer-depth-indicate-mode", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-default-prompt-format",
            Value::string(" (default %s)"),
        );
        obarray.set_symbol_value("minibuffer-beginning-of-buffer-movement", Value::NIL);
        obarray.set_symbol_value("minibuffer-electric-default-mode", Value::NIL);
        obarray.set_symbol_value("minibuffer-temporary-goal-position", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-confirm-exit-commands",
            Value::list(vec![
                Value::symbol("completion-at-point"),
                Value::symbol("minibuffer-complete"),
                Value::symbol("minibuffer-complete-word"),
            ]),
        );
        obarray.set_symbol_value("minibuffer-history-case-insensitive-variables", Value::NIL);
        obarray.set_symbol_value("minibuffer-on-screen-keyboard-displayed", Value::NIL);
        obarray.set_symbol_value("minibuffer-on-screen-keyboard-timer", Value::NIL);
        // GNU src/minibuf.c:2553-2555 DEFVARs this hook and sets it to Qnil.
        // rfn-eshadow.el, minibuffer.el and simple.el `add-hook' their entries
        // onto it while loadup runs, and `add-hook' conses onto the front, so
        // the resulting order is a record of that preload order.
        obarray.define_c_hook_variable("minibuffer-setup-hook");
        obarray.set_symbol_value("regexp-search-ring", Value::NIL);
        obarray.set_symbol_value("regexp-search-ring-max", Value::fixnum(16));
        obarray.set_symbol_value("regexp-search-ring-yank-pointer", Value::NIL);
        obarray.set_symbol_value("search-ring", Value::NIL);
        obarray.set_symbol_value("search-ring-max", Value::fixnum(16));
        obarray.set_symbol_value("search-ring-update", Value::NIL);
        obarray.set_symbol_value("search-ring-yank-pointer", Value::NIL);
        obarray.set_symbol_value("last-abbrev", Value::NIL);
        obarray.set_symbol_value("last-abbrev-location", Value::fixnum(0));
        obarray.set_symbol_value("last-abbrev-text", Value::NIL);
        obarray.set_symbol_value("last-command-event", Value::NIL);
        // last-event-frame, last-event-device, last-nonmenu-event and
        // last-kbd-macro are registered by keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("last-input-event", Value::NIL);
        obarray.set_symbol_value("last-prefix-arg", Value::NIL);
        obarray.set_symbol_value("last-code-conversion-error", Value::NIL);
        obarray.set_symbol_value("last-coding-system-specified", Value::NIL);
        obarray.set_symbol_value("last-coding-system-used", Value::symbol("undecided-unix"));
        obarray.set_symbol_value("last-next-selection-coding-system", Value::NIL);
        obarray.set_symbol_value("command-debug-status", Value::NIL);
        obarray.make_special("command-debug-status");
        obarray.set_symbol_value(
            "command-error-function",
            Value::symbol("help-command-error-confusable-suggestions"),
        );
        obarray.set_symbol_value("key-substitution-in-progress", Value::NIL);
        obarray.set_symbol_value("this-command", Value::NIL);
        obarray.set_symbol_value("real-this-command", Value::NIL);
        obarray.set_symbol_value("this-command-keys-shift-translated", Value::NIL);
        obarray.set_symbol_value("current-prefix-arg", Value::NIL);
        obarray.set_symbol_value("track-mouse", Value::NIL);
        obarray.make_special("track-mouse");
        obarray.set_symbol_value(
            "while-no-input-ignore-events",
            // GNU's `init_while_no_input_ignore_events'
            // (src/keyboard.c:13315-13336) builds an eleven-name base list
            // UNCONDITIONALLY, then conses `dbus-event' on under
            // `#ifdef HAVE_DBUS', `file-notify' under `USE_FILE_NOTIFY',
            // `thread-event' under `THREADS_ENABLED', and `sleep-event'
            // unconditionally last.  This build has the file-notify and
            // threads options and not HAVE_DBUS (ledger 192), so it is GNU's
            // list minus exactly `dbus-event'.  `sleep-event' and
            // `toolkit-theme-changed' were missing from the base list here and
            // are guarded by nothing in GNU; both restored, ledger 192.
            Value::list(vec![
                Value::symbol("sleep-event"),
                Value::symbol("thread-event"),
                Value::symbol("file-notify"),
                Value::symbol("select-window"),
                Value::symbol("help-echo"),
                Value::symbol("move-frame"),
                Value::symbol("iconify-frame"),
                Value::symbol("make-frame-visible"),
                Value::symbol("focus-in"),
                Value::symbol("focus-out"),
                Value::symbol("config-changed-event"),
                Value::symbol("selection-request"),
                Value::symbol("monitors-changed"),
                Value::symbol("toolkit-theme-changed"),
            ]),
        );
        obarray.make_special("while-no-input-ignore-events");
        // GNU's nine `dbusbind.c' DEFVARs -- `dbus-message-type-*',
        // `dbus-debug', `dbus-compiled-version', `dbus-runtime-version',
        // `dbus-registered-objects-table' (src/dbusbind.c:2069-2159) -- are
        // inside `#ifdef HAVE_DBUS' with the rest of the file, and this build
        // has no D-Bus transport, so it declares none of them (ledger 192).
        // `lisp/net/dbus.el:40-46' declares them bare for exactly this build,
        // and `:53' supplies `dbus-debug'.
        obarray.set_symbol_value("deactivate-mark", Value::NIL);
        obarray.make_special("deactivate-mark");
        obarray.make_buffer_local("deactivate-mark", true);
        // GNU `keyboard.c` DEFVARs.  `command_loop_1` resets
        // `disable-point-adjustment` to nil before each command; commands may
        // set it non-nil to suppress the post-command `adjust_point_for_property`
        // step.  `global-disable-point-adjustment` is the permanent override.
        obarray.set_symbol_value("disable-point-adjustment", Value::NIL);
        obarray.make_special("disable-point-adjustment");
        obarray.set_symbol_value("global-disable-point-adjustment", Value::NIL);
        obarray.make_special("global-disable-point-adjustment");
        obarray.set_symbol_value("mark-active", Value::NIL);
        obarray.set_symbol_value("mark-even-if-inactive", Value::T);
        obarray.make_special("mark-even-if-inactive");
        obarray.set_symbol_value("mark-ring", Value::NIL);
        obarray.set_symbol_value("mark-ring-max", Value::fixnum(16));
        // saved-region-selection is set by keyboard::pure::register_bootstrap_vars
        obarray.set_symbol_value("transient-mark-mode", Value::NIL);
        obarray.set_symbol_value("transient-mark-mode-hook", Value::NIL);
        // post-select-region-hook and display-monitors-changed-functions are
        // registered by keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("echo-area-clear-hook", Value::NIL);
        // terminal.c:700 / term.c:5233 / term.c:5240 DEFVAR_LISP, init nil.
        obarray.define_c_hook_variable("delete-terminal-functions");
        obarray.define_c_hook_variable("suspend-tty-functions");
        obarray.define_c_hook_variable("resume-tty-functions");
        obarray.set_symbol_value("overriding-local-map", Value::NIL);
        obarray.make_special("overriding-local-map");
        obarray.set_symbol_value("overriding-local-map-menu-flag", Value::NIL);
        obarray.make_special("overriding-local-map-menu-flag");
        obarray.set_symbol_value("overriding-plist-environment", Value::NIL);
        obarray.make_special("overriding-plist-environment");
        obarray.set_symbol_value("overriding-terminal-local-map", Value::NIL);
        // GNU uses DEFVAR_KBOARD here. NeoVM does not yet split keyboard state
        // per terminal, so model it as a dynamically scoped runtime variable.
        obarray.make_special("overriding-terminal-local-map");
        // textconv.c:2621 DEFVAR_LISP, init Qlambda.
        obarray
            .define_special_variable("overriding-text-conversion-style", Value::symbol("lambda"));
    }

    /// Core eval.c / keyboard.c DEFVAR globals plus the standard error
    /// hierarchy and indentation/font variable seeding.
    fn seed_core_eval_variables(obarray: &mut Obarray) {
        // Core eval variables (stay in eval.rs)
        obarray.set_symbol_value("purify-flag", Value::NIL);
        obarray.make_special("purify-flag");
        obarray.define_int_variable("max-lisp-eval-depth", 1600);
        obarray.define_int_variable("lisp-eval-depth-reserve", 200);

        // Terminal/display variables (C-level DEFVAR in official Emacs)
        // `standard-display-table' is a DEFVAR_LISP in dispnew.c (default nil),
        // hence special: `(let ((standard-display-table ...)) ...)' must bind it
        // dynamically so the `standard-display-*' functions (disp-table.el) see
        // and mutate the binding instead of the global default.
        obarray.set_symbol_value("standard-display-table", Value::NIL);
        obarray.make_special("standard-display-table");
        // `glyph-table' is a DEFVAR_LISP in dispnew.c, default nil. It must be
        // bound (and special) so `boundp'/`special-variable-p' agree with GNU.
        obarray.set_symbol_value("glyph-table", Value::NIL);
        obarray.make_special("glyph-table");
        obarray.set_symbol_value(
            "image-load-path",
            Value::list(vec![
                Value::string("/usr/share/emacs/30.1/etc/images/"),
                Value::symbol("data-directory"),
            ]),
        );
        // `image-types' and `image-scaling-factor' are registered by
        // image::register_bootstrap_vars, GNU's `syms_of_image'.

        // User init / startup (C DEFVAR in official Emacs)
        obarray.set_symbol_value("user-init-file", Value::NIL);
        obarray.set_symbol_value("user-emacs-directory", Value::string("~/.emacs.d/"));

        // Frame parameters (C DEFVAR in official Emacs)
        obarray.set_symbol_value("frame--special-parameters", Value::NIL);

        // Initialize distributed bootstrap variables.
        //
        // GNU's `DEFVAR_BOOL' table comes first, for the reason `main' runs
        // every `syms_of_*' before Lisp: `Fmake_variable_buffer_local' copies
        // the symbol's forwarder into the BLV (`src/data.c:2112-2140'), so a
        // variable that is going to be localized below -- `indent-tabs-mode',
        // `display-fill-column-indicator', `display-line-numbers-widen' --
        // has to be forwarded before that happens or the coercion is dropped.
        super::defvar_bool::register_bootstrap_vars(obarray);
        super::alloc::register_bootstrap_vars(obarray);
        super::load::register_bootstrap_vars(obarray);
        super::fileio::register_bootstrap_vars(obarray);
        super::process::register_bootstrap_vars(obarray);
        super::undo::register_bootstrap_vars(obarray);
        super::category::register_bootstrap_vars(obarray);
        super::window_cmds::register_bootstrap_vars(obarray);
        super::keyboard::pure::register_bootstrap_vars(obarray);
        super::composite::register_bootstrap_vars(obarray);
        super::coding::register_bootstrap_vars(obarray);
        super::dired::register_bootstrap_vars(obarray);
        super::xdisp::register_bootstrap_vars(obarray);
        super::textprop::register_bootstrap_vars(obarray);
        super::xfaces::register_bootstrap_vars(obarray);
        super::frame_vars::register_bootstrap_vars(obarray);
        super::buffer_vars::register_bootstrap_vars(obarray);
        super::image::register_bootstrap_vars(obarray);
        super::fontset::register_bootstrap_vars(obarray);

        // ---- end C-level bootstrap variables ----

        obarray.set_symbol_value("unread-input-method-events", Value::NIL);
        obarray.set_symbol_value("unread-post-input-method-events", Value::NIL);
        obarray.set_symbol_value("input-method-alist", Value::NIL);
        obarray.set_symbol_value("input-method-activate-hook", Value::NIL);
        obarray.set_symbol_value("input-method-after-insert-chunk-hook", Value::NIL);
        obarray.set_symbol_value("input-method-deactivate-hook", Value::NIL);
        obarray.set_symbol_value("input-method-exit-on-first-char", Value::NIL);
        obarray.set_symbol_value("input-method-exit-on-invalid-key", Value::NIL);
        // GNU `src/keyboard.c` initializes this DEFVAR_LISP to Qlist.
        obarray.set_symbol_value("input-method-function", Value::symbol("list"));
        obarray.make_special("input-method-function");
        obarray.set_symbol_value("input-method-highlight-flag", Value::T);
        obarray.set_symbol_value("input-method-history", Value::NIL);
        // input-method-previous-message is set by keyboard::pure::register_bootstrap_vars
        obarray.set_symbol_value("input-method-use-echo-area", Value::NIL);
        obarray.set_symbol_value("input-method-verbose-flag", Value::symbol("default"));
        obarray.set_symbol_value("unread-command-events", Value::NIL);
        // No `variable-documentation` is seeded here, and ledger 178 is why.
        //
        // This used to write one for all 1972 names of two hand-typed tables
        // in `doc.rs`, under the comment "GNU Emacs seeds core startup vars
        // with integer `variable-documentation` offsets in the DOC table".
        // GNU does no such thing.  Every `variable-documentation` GNU installs
        // is downstream of the variable existing: `Fsnarf_documentation` puts
        // one on a name this build BINDS (`src/doc.c:606-613`, where the
        // `Fput` is the entire branch), Lisp `defvar` puts one on a name it is
        // defining and only when the docstring is non-nil (`src/eval.c:911`),
        // and `Fdefvaralias` copies one across an alias edge
        // (`src/eval.c:723`).  There is no fourth writer and nothing runs
        // before the variable is there.
        //
        // The 70 offset rows made the point a second time by seeding
        // `(fixnum 0)`, which is precisely the value GNU reserves to mean
        // "there is no doc" -- `if (BASE_EQ (tem, make_fixnum (0))) tem =
        // Qnil;` (`src/doc.c:433-434`) -- and which `make-docfile` can never
        // emit, the smallest real offset being `end + 1 - buf`.
        //
        // A seeded row landed on the symbol's plist, which is the FIRST arm
        // `documentation_property_plan` consults, so it answered ahead of
        // `Fsnarf_documentation`'s `Fboundp` gate: 35 unbound names carried a
        // doc where GNU carries none.  Measured GNU 31.0.90 `-Q --batch`:
        // 18815 symbols, zero unbound-yet-documented, zero holding the
        // reserved `0`.  `no_unbound_symbol_carries_a_variable_documentation`
        // is the guard.
        // Bootstrap primitive function cells that GNU `simple.el` references
        // before its own Elisp defs overwrite them. Without these placeholders,
        // loaded GNU bytecode can capture `nil` for forward/runtime calls into
        // Builtin function cells are set by SubrSpec registration during init_builtins().
        for name in ["mark-marker", "region-beginning", "region-end"] {
            obarray.set_symbol_function(name, Value::subr_from_sym_id(intern(name)));
        }

        // `word-at-point` is defined in GNU Emacs Lisp by `thingatpt.el`,
        // not as a startup builtin.
        obarray.clear_function_silent("word-at-point");

        // Mark standard variables as special (dynamically bound)
        for name in &[
            "debug-on-error",
            "debugger",
            // "lexical-binding" is registered below like GNU lread.c:
            // DEFVAR_LISP plus make-variable-buffer-local.
            "load-prefer-newer",
            "load-path",
            "load-history",
            "default-directory",
            "load-file-name",
            "set-auto-coding-for-load",
            "noninteractive",
            "inhibit-quit",
            "inhibit-read-only",
            "inhibit-modification-hooks",
            "internal-make-interpreted-closure-function",
            "print-length",
            "print-level",
            "standard-output",
            "case-fold-search",
            "buffer-read-only",
            "current-prefix-arg",
            "prefix-arg",
            "last-prefix-arg",
            "last-command-event",
            "last-input-event",
            "last-command",
            "real-last-command",
            "this-command",
            "real-this-command",
            "this-command-keys-shift-translated",
            "unread-command-events",
            "unread-input-method-events",
            "unread-post-input-method-events",
            // transient-mark-mode is a C-level variable in GNU (buffer.c),
            // always dynamically scoped. Must be special so (let ((transient-mark-mode t)) ...)
            // creates a dynamic binding visible to called functions like region-active-p.
            "transient-mark-mode",
        ] {
            obarray.make_special(name);
        }

        // Initialize the standard error hierarchy (error, user-error, etc.)
        super::errors::init_standard_errors(obarray);

        // Initialize indentation variables (tab-width, indent-tabs-mode, etc.)
        super::indent::init_indent_vars(obarray);
        super::font::init_font_vars(obarray);
    }

    /// C-level DEFVAR registrations mirroring GNU's per-file syms_of_*()
    /// functions, plus buffer-local bootstrap variables. If a variable is
    /// declared via DEFVAR in GNU C, it must be registered here or elisp
    /// reading or let-binding it gets void-variable.
    fn seed_c_level_defvars(obarray: &mut Obarray, custom: &mut CustomManager) {
        // `case-fold-search` is DEFVAR_LISP + Fmake_variable_buffer_local
        // in GNU `buffer.c:5971-5975`. Install it as a LOCALIZED symbol
        // with `local_if_set = 1` at init time so reads/writes route
        // through the BLV + local_var_alist path instead of the legacy
        // `BufferLocals::lisp_bindings` fallback. Default is `t`.
        {
            let id = crate::emacs_core::intern::intern("case-fold-search");
            obarray.set_symbol_value("case-fold-search", Value::T);
            obarray.make_symbol_localized(id, Value::T);
            obarray.set_blv_local_if_set(id, true);
        }

        // `indent-tabs-mode` is DEFVAR_BOOL + make-variable-buffer-local
        // (bindings.el:1032). GNU's DEFVAR_BOOL installs a C-backed
        // forwarder; NeoMacs stores it as a plain Lisp value and
        // then hoists it to LOCALIZED at init. Default is `t`
        // (matches `init_indent_vars`).
        {
            let id = crate::emacs_core::intern::intern("indent-tabs-mode");
            obarray.make_symbol_localized(id, Value::T);
            obarray.set_blv_local_if_set(id, true);
        }

        super::textprop::init_textprop_vars(obarray, custom);
        super::syntax::init_syntax_vars(obarray, custom);
        // Register all DEFVAR_PER_BUFFER variables from GNU Emacs buffer.c.
        // These are C-level buffer-local variables that must exist before
        // any .el file loads.  Default values match init_buffer_once().
        macro_rules! defvar_per_buffer {
            ($name:expr, $val:expr) => {
                obarray.make_special($name);
                obarray.set_symbol_value($name, $val);
            };
        }
        {
            // Core buffer identity
            defvar_per_buffer!("buffer-file-name", Value::NIL);
            defvar_per_buffer!("buffer-file-truename", Value::NIL);
            // GNU buffer.c:5381 — default-directory defaults to cwd.
            // This sets the GLOBAL default; new buffers inherit it.
            {
                let cwd = std::env::current_dir()
                    .map(|p| {
                        let mut s = p.to_string_lossy().into_owned();
                        if !s.ends_with('/') {
                            s.push('/');
                        }
                        s
                    })
                    .unwrap_or_else(|_| "/".to_string());
                // GNU Emacs uses make_unibyte_string for default-directory
                // because the locale isn't set up yet during dump.  loadup.el
                // checks (multibyte-string-p default-directory) and errors
                // if it's multibyte.
                defvar_per_buffer!("default-directory", Value::unibyte_string(cwd));
            }
            defvar_per_buffer!("buffer-read-only", Value::NIL);
            defvar_per_buffer!("buffer-undo-list", Value::NIL);
            defvar_per_buffer!("buffer-saved-size", Value::fixnum(0));
            defvar_per_buffer!("buffer-backed-up", Value::NIL);
            defvar_per_buffer!("buffer-file-format", Value::NIL);
            defvar_per_buffer!("buffer-auto-save-file-name", Value::NIL);
            defvar_per_buffer!("buffer-auto-save-file-format", Value::T);
            defvar_per_buffer!("buffer-file-coding-system", Value::NIL);
            defvar_per_buffer!("buffer-display-count", Value::fixnum(0));
            defvar_per_buffer!("buffer-display-time", Value::NIL);

            // Modes
            defvar_per_buffer!("major-mode", Value::symbol("fundamental-mode"));
            defvar_per_buffer!("mode-name", Value::NIL);
            defvar_per_buffer!("mode-line-format", Value::string("%-"));
            defvar_per_buffer!("header-line-format", Value::NIL);
            defvar_per_buffer!("tab-line-format", Value::NIL);
            defvar_per_buffer!("local-abbrev-table", Value::NIL);
            defvar_per_buffer!("local-minor-modes", Value::NIL);
            defvar_per_buffer!("abbrev-mode", Value::NIL);
            defvar_per_buffer!("overwrite-mode", Value::NIL);
            defvar_per_buffer!("auto-fill-function", Value::NIL);

            // Search (GNU buffer.c DEFVAR_PER_BUFFER)
            defvar_per_buffer!("case-fold-search", Value::T);
            defvar_per_buffer!("indent-tabs-mode", Value::T);

            // Display
            defvar_per_buffer!("tab-width", Value::fixnum(8));
            defvar_per_buffer!("fill-column", Value::fixnum(70));
            defvar_per_buffer!("left-margin", Value::fixnum(0));
            defvar_per_buffer!("truncate-lines", Value::NIL);
            defvar_per_buffer!("word-wrap", Value::NIL);
            defvar_per_buffer!("ctl-arrow", Value::T);
            defvar_per_buffer!("selective-display", Value::NIL);
            defvar_per_buffer!("selective-display-ellipses", Value::T);
            defvar_per_buffer!("enable-multibyte-characters", Value::T);
            defvar_per_buffer!("buffer-display-table", Value::NIL);
            defvar_per_buffer!("buffer-invisibility-spec", Value::NIL);
            defvar_per_buffer!("line-spacing", Value::NIL);
            defvar_per_buffer!("cache-long-scans", Value::T);
            defvar_per_buffer!("point-before-scroll", Value::NIL);

            // Cursor
            defvar_per_buffer!("cursor-type", Value::T);
            defvar_per_buffer!("neomacs-cursor-effect", Value::NIL);
            defvar_per_buffer!("cursor-in-non-selected-windows", Value::T);

            // Marks
            defvar_per_buffer!("mark-active", Value::NIL);

            // Bidi
            defvar_per_buffer!("bidi-display-reordering", Value::T);
            defvar_per_buffer!("bidi-paragraph-direction", Value::NIL);
            defvar_per_buffer!("bidi-paragraph-start-re", Value::NIL);
            defvar_per_buffer!("bidi-paragraph-separate-re", Value::NIL);

            // Fringes and margins
            defvar_per_buffer!("left-fringe-width", Value::NIL);
            defvar_per_buffer!("right-fringe-width", Value::NIL);
            defvar_per_buffer!("left-margin-width", Value::fixnum(0));
            defvar_per_buffer!("right-margin-width", Value::fixnum(0));
            defvar_per_buffer!("fringes-outside-margins", Value::NIL);
            defvar_per_buffer!("fringe-indicator-alist", Value::NIL);
            defvar_per_buffer!("fringe-cursor-alist", Value::NIL);
            defvar_per_buffer!("indicate-empty-lines", Value::NIL);
            defvar_per_buffer!("indicate-buffer-boundaries", Value::NIL);

            // Scroll bars
            defvar_per_buffer!("scroll-bar-width", Value::NIL);
            defvar_per_buffer!("scroll-bar-height", Value::NIL);
            defvar_per_buffer!("vertical-scroll-bar", Value::T);
            defvar_per_buffer!("horizontal-scroll-bar", Value::T);
            defvar_per_buffer!("scroll-up-aggressively", Value::NIL);
            defvar_per_buffer!("scroll-down-aggressively", Value::NIL);

            // Other
            defvar_per_buffer!("text-conversion-style", Value::NIL);

            // Phase 10B/C: install BUFFER_OBJFWD descriptors for
            // every entry in BUFFER_SLOT_INFO. After this point
            // each of these symbols has redirect=Forwarded with a
            // descriptor that resolves reads/writes to
            // `Buffer::slots[offset]`. The earlier
            // `defvar_per_buffer!` left them as LOCALIZED; we
            // overwrite that with the FORWARDED tag here so the
            // VM lookup/assign hot path takes the slot fast path.
            //
            // Mirrors GNU's `defvar_per_buffer` in `buffer.c`,
            // which always uses BUFFER_OBJFWD for these C-side
            // BVAR slots (`buffer.h:319-329`).
            {
                use crate::buffer::buffer::BUFFER_SLOT_INFO;
                use crate::emacs_core::forward::alloc_buffer_objfwd;
                use crate::emacs_core::intern::intern;

                for info in BUFFER_SLOT_INFO {
                    if !info.install_as_forwarder {
                        // Internal BVAR-only slot (syntax-table /
                        // category-table / case-table). Mirrors GNU's
                        // handling of `syntax_table_` etc. which
                        // occupy BVAR slot positions but are not
                        // DEFVAR_PER_BUFFER'd. Reads/writes happen
                        // exclusively through dedicated builtins.
                        continue;
                    }
                    let id = intern(info.name);
                    let fwd = alloc_buffer_objfwd(
                        info.offset.as_u16(),
                        info.local_flags_idx,
                        info.predicate,
                        info.default.to_value(),
                    );
                    obarray.install_buffer_objfwd(id, fwd);
                }
            }
        }

        // GNU lread.c registers `lexical-binding` with DEFVAR_LISP and
        // then calls Fmake_variable_buffer_local. It is not a BVAR
        // BUFFER_OBJFWD slot, but ordinary `set` in a buffer must
        // auto-create a buffer-local binding.
        {
            let id = crate::emacs_core::intern::intern("lexical-binding");
            obarray.set_symbol_value("lexical-binding", Value::NIL);
            obarray.make_special("lexical-binding");
            obarray.make_symbol_localized(id, Value::NIL);
            obarray.set_blv_local_if_set(id, true);
        }

        // -----------------------------------------------------------------
        // C-level DEFVAR registrations: mirrors GNU's syms_of_*() functions.
        //
        // GNU Emacs declares hundreds of C-backed Lisp variables via
        // DEFVAR_LISP / DEFVAR_BOOL / DEFVAR_INT in its src/*.c files.
        // Each becomes a globally-visible symbol with a default value.
        // Elisp code reads/writes them freely; many are let-bound in
        // standard .el files during bootstrap and normal operation.
        //
        // If a variable is declared via DEFVAR in GNU's C code, it
        // MUST be registered here. Otherwise any elisp code that
        // reads or let-binds it will get void-variable.
        // -----------------------------------------------------------------

        // --- src/search.c: syms_of_search ---
        // DEFVAR_LISP, default nil. Let-bound extensively in subr.el,
        // custom.el, widget.el, mule.el, etc. to freeze match data
        // during internal string-match calls.
        obarray.set_symbol_value("inhibit-changing-match-data", Value::NIL);
        obarray.make_special("inhibit-changing-match-data");

        // --- src/search.c: syms_of_search ---
        // DEFVAR_LISP, default nil. When non-nil, a regexp substituted for
        // bunches of spaces in a regexp search. Has no elisp defvar (sibling
        // `search-whitespace-regexp` is an isearch.el defcustom), so it must be
        // seeded here; hi-lock.el (highlight-regexp) let-binds it.
        obarray.set_symbol_value("search-spaces-regexp", Value::NIL);
        obarray.make_special("search-spaces-regexp");

        // --- src/xdisp.c: syms_of_xdisp ---
        // DEFVAR_LISP, default nil. Abnormal hook run before redisplaying a
        // window with scrolling; neomacs drives it from the explicit
        // run-window-scroll-functions callsites in window_cmds, so seeding the
        // symbol only makes `boundp` true before any setq/let.
        obarray.set_symbol_value("window-scroll-functions", Value::NIL);
        obarray.make_special("window-scroll-functions");

        // --- src/alloc.c: syms_of_alloc ---
        // GC accounting DEFVAR_INTs (monotonic allocation counters). neomacs
        // does not track them yet, so seed 0 so `boundp' agrees with GNU.
        for name in [
            "cons-cells-consed",
            "floats-consed",
            "vector-cells-consed",
            "symbols-consed",
            "string-chars-consed",
            "intervals-consed",
        ] {
            obarray.define_int_variable(name, 0);
        }
        // --- src/profiler.c: syms_of_profiler ---
        obarray.define_int_variable("profiler-max-stack-depth", 16);
        obarray.define_int_variable("profiler-log-size", 10_000);
        // DEFVAR_INT, default 65536 (bignum digit-width limit).
        obarray.define_int_variable("integer-width", 65536);

        // --- src/frame.c: syms_of_frame ---
        // DEFVAR_LISP, default 20 (minimum frame alpha/opacity).
        obarray.set_symbol_value("frame-alpha-lower-limit", Value::fixnum(20));
        obarray.make_special("frame-alpha-lower-limit");
        // DEFVAR_LISP, default nil (function to adjust reported mouse position).
        obarray.set_symbol_value("mouse-position-function", Value::NIL);
        obarray.make_special("mouse-position-function");
        //
        // TWO entries found this independently, from two different consumers,
        // and both accounts are kept because each names a different invariant
        // that depends on this one fix:

        // `frame.c:7555' DEFVAR_KBOARD, and the kboard slot starts nil
        // (`keyboard.c:13129', `kset_default_minibuffer_frame (kb, Qnil)').
        // This port models kboard variables as globals, as it does for
        // `last-kbd-macro' and `defining-kbd-macro' in `keyboard::pure'.
        //
        // It was assigned only by `post_image_init' and by the frame setup in
        // `neomacs-bin', both of which run AFTER `loadup', so the name was
        // unbound for the whole of loadup where GNU has it bound from
        // `syms_of_frame' on.  Ledger 182 found it because
        // `Fsnarf_documentation' asks `Fboundp' once, at the end of loadup:
        // the variable was the only one of the DOC table's 766 bound names
        // that the snarf could not see, so it was the only one left with no
        // documentation.  The lazy lookup the snarf replaced asked the same
        // question at query time and so could not see the gap.
        //
        // DEFVAR_KBOARD, default nil (`src/frame.c:7555`).  It has to be bound
        // HERE and not from `post_image_init`'s reset table, because
        // `defvar_object::adopt` runs at the end of this bootstrap and can
        // only tag names that already exist: bound later, the symbol stays
        // `SYMBOL_PLAINVAL` and answers `special-variable-p` nil and
        // `makunbound` yes, where GNU answers t and refuses.  Measured,
        // `-Q --batch`: GNU `(t nil t)`, this port `(t nil nil)` (ledger 183).
        obarray.set_symbol_value("default-minibuffer-frame", Value::NIL);
        obarray.make_special("default-minibuffer-frame");

        // --- src/keymap.c: syms_of_keymap ---
        // DEFVAR_LISP, default nil (preferred modifier for `where-is').
        obarray.set_symbol_value("where-is-preferred-modifier", Value::NIL);
        obarray.make_special("where-is-preferred-modifier");

        // --- src/coding.c: syms_of_coding ---
        // `coding-category-utf-8' holds the coding system for the UTF-8 detection
        // category; its default is the `utf-8' coding system symbol.
        obarray.set_symbol_value("coding-category-utf-8", Value::symbol("utf-8"));
        obarray.make_special("coding-category-utf-8");

        // --- src/charset.c: syms_of_charset ---
        // `charset-list' is a DEFVAR_LISP (the list of defined charsets), NOT a
        // function -- GNU signals void-function for `(charset-list)'. Seed the
        // variable so `boundp' agrees; the neomacs registry populates the
        // ordered list separately.
        obarray.set_symbol_value("charset-list", Value::NIL);
        obarray.make_special("charset-list");

        // --- src/minibuf.c: read-buffer history ---
        // `buffer-name-history' is the minibuffer history list for buffer names,
        // default nil.
        obarray.set_symbol_value("buffer-name-history", Value::NIL);
        obarray.make_special("buffer-name-history");

        // --- src/casefiddle.c: syms_of_casefiddle ---
        // DEFVAR_BOOL + Fmake_variable_buffer_local, default 0 (nil).
        // Checked by case-conversion functions. Buffer-local via
        // make-variable-buffer-local (NOT defvar_per_buffer).
        {
            let id = crate::emacs_core::intern::intern("case-symbols-as-words");
            // DEFVAR_BOOL marks the symbol special like every C DEFVAR.
            obarray.make_symbol_localized(id, Value::NIL);
            obarray.set_blv_local_if_set(id, true);
        }

        // --- src/emacs.c: syms_of_emacs ---
        // DEFVAR_LISP, default nil. Run by kill-emacs.
        obarray.define_c_hook_variable("kill-emacs-hook");

        // --- src/cmds.c: syms_of_cmds ---
        // DEFVAR_LISP, default nil. `newline' dynamically binds this in
        // simple.el so noninteractive newline insertion runs only its local
        // postprocessor, matching GNU Emacs.
        obarray.set_symbol_value("post-self-insert-hook", Value::NIL);
        obarray.make_special("post-self-insert-hook");

        // --- src/buffer.c: syms_of_buffer ---
        // The three long-line DEFVAR_INTs, in GNU's declaration order
        // (`src/buffer.c:6007', `6025', `6043').  `long-line-optimizations-p'
        // consults the first two through `narrow-to-region' around the command
        // hooks and the third through the hscroll shortcut.
        obarray.define_int_variable("long-line-optimizations-region-size", 500_000);
        obarray.define_int_variable("long-line-optimizations-bol-search-limit", 128);
        obarray.define_int_variable("large-hscroll-threshold", 10_000);
        // GNU registers overlay hook property names with DEFSYM.  They are
        // globally interned symbols, not variables.
        for name in ["insert-in-front-hooks", "insert-behind-hooks"] {
            let id = crate::emacs_core::intern::intern(name);
            obarray.ensure_interned_global_id(id);
        }

        // --- src/keyboard.c: syms_of_keyboard ---
        // These are all DEFVAR_LISP variables in GNU.  They must exist and be
        // special before Lisp loadup: package functions compiled with lexical
        // binding rely on surrounding `let` forms remaining dynamically
        // visible while add-hook/remove-hook update the active value cell.
        obarray.define_c_hook_variable("pre-command-hook");
        obarray.define_c_hook_variable("post-command-hook");

        // GNU registers this command-loop restriction label with DEFSYM.
        {
            let id = crate::emacs_core::intern::intern("long-line-optimizations-in-command-hooks");
            obarray.ensure_interned_global_id(id);
        }

        // --- src/lread.c: syms_of_lread ---
        // GNU registers these names with DEFSYM while initializing the reader.
        // They are globally interned symbols even when they have no value or
        // function binding.
        for name in ["hash-table", "data", "test", "size", "purecopy", "weakness"] {
            let id = crate::emacs_core::intern::intern(name);
            obarray.ensure_interned_global_id(id);
        }

        // --- src/callint.c: syms_of_callint ---
        // DEFVAR_LISP, default nil.
        obarray.define_c_hook_variable("mouse-leave-buffer-hook");

        // --- src/xterm.c: syms_of_xterm / src/pgtkterm.c: syms_of_pgtkterm ---
        // GNU defines these from the compiled window-system backend before
        // Lisp loadup.  `lisp/loadup.el' deliberately checks only `boundp' for
        // some of them, and `term/x-win.el' mutates `x-keysym-table' while
        // installing the X keysym map.
        obarray.set_symbol_value("x-keysym-table", Value::hash_table(HashTableTest::Eql));
        obarray.make_special("x-keysym-table");
        obarray.set_symbol_value(
            "x-toolkit-scroll-bars",
            if cfg!(target_os = "windows") {
                Value::T
            } else {
                Value::symbol("gtk")
            },
        );
        obarray.make_special("x-toolkit-scroll-bars");
        // `gtk-version-string' and `cairo-version-string' used to be declared
        // here, holding the literals "3.24.51" and "1.18.4".  Ledger 199
        // removed them: GNU declares each one INSIDE the same conditional
        // block as the `Fprovide' that advertises its toolkit --
        // `src/xfns.c:10539-10549' pairs `Fprovide ("gtk")' with
        // `DEFVAR_LISP ("gtk-version-string", ...)' under one `#ifdef USE_GTK',
        // and `10552-10558' pairs `Fprovide ("cairo")' with
        // `cairo-version-string' under `#ifdef USE_CAIRO'
        // (`src/pgtkfns.c:3786-3802' repeats both for PGTK,
        // `src/haikuterm.c:4886' / `src/haikufns.c:3312' for Haiku).  One
        // `configure' switch compiles both statements, so no GNU build can
        // bind the variable while `featurep' answers nil -- and this port's
        // display stack is winit + wgpu + WPE, with `(featurep 'gtk)' and
        // `(featurep 'cairo)' both nil.  `lisp/version.el:113-127' and
        // `lisp/erc/erc.el:5466-5468' read them only behind those `featurep'
        // guards, which is why the invented values were inert AND undetected.
        // `emacs_core::provide_coupled_vars' holds the rule and the scan.
        obarray.define_int_variable("x-selection-timeout", 0);
        // `src/xterm.c:32704' / `32922' DEFVAR_INT, inits 200 and 128.
        obarray.define_int_variable("x-mouse-click-focus-ignore-time", 200);
        obarray.define_int_variable("x-color-cache-bucket-size", 128);
        // `src/xterm.c:32833' DEFVAR_LISP, `make_float (1.0)' -- a float, not
        // the fixnum 1: `handle_one_xevent' scales the XInput 2 scroll unit by
        // it with `XFLOATINT' after a `NUMBERP' test
        // (`src/xterm.c:22802-22803').
        obarray.define_special_variable("x-scroll-event-delta-factor", Value::make_float(1.0));
        // `src/xterm.c:32976' DEFVAR_LISP, `list2 (QCLIPBOARD, QPRIMARY)'.  The
        // list is not decoration: `x_should_preserve_selection' preserves only
        // the selections named in it when the value is a cons, and nothing at
        // all when the value is nil (`src/xselect.c:1385-1401'), so a nil
        // default is the opposite of GNU's behaviour rather than a milder
        // version of it.
        obarray.define_special_variable(
            "x-auto-preserve-selections",
            Value::list(vec![Value::symbol("CLIPBOARD"), Value::symbol("PRIMARY")]),
        );
        obarray.set_symbol_value("x-session-id", Value::NIL);
        obarray.make_special("x-session-id");
        obarray.set_symbol_value("x-session-previous-id", Value::NIL);
        obarray.make_special("x-session-previous-id");
        for name in [
            "x-ctrl-keysym",
            "x-alt-keysym",
            "x-hyper-keysym",
            "x-meta-keysym",
            "x-super-keysym",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        // The rest of `syms_of_xterm', entry 173.  This port already declared
        // 24 of `xterm.c''s 39 names before this block grew; the fifteen below
        // are the remainder, each with GNU's own initializer.
        //
        // `xterm.c:33013' DEFVAR_LISP, `Vx_allow_focus_stealing = Qnewer_time'
        // at `33037' -- a SYMBOL naming one of four policies, dispatched by
        // `EQ' against `Qimitate_pager', `Qnewer_time' and `Qraise_and_focus'
        // (`xterm.c:28876-28894', again at `29097').  nil is a fourth policy,
        // not the absence of one, so a nil seed would have chosen differently
        // rather than more weakly.
        obarray.define_special_variable("x-allow-focus-stealing", Value::symbol("newer-time"));
        // `xterm.c:33000' DEFVAR_LISP, `Vx_fast_selection_list = list1 (QCLIPBOARD)',
        // with GNU's own comment saying the default is chosen so tool-bar
        // updates need no `_XReply'.
        obarray.define_special_variable(
            "x-fast-selection-list",
            Value::list(vec![Value::symbol("CLIPBOARD")]),
        );
        // `xterm.c:32797' DEFVAR_LISP, `make_float (0.1)' -- a float, like
        // `polling-period' and `x-scroll-event-delta-factor'.
        obarray.define_special_variable("x-wait-for-event-timeout", Value::make_float(0.1));
        for name in [
            // `xterm.c:33054', `33064', `33076', `32845', `33039' -- five
            // policy flags, all `Qnil' in `syms_of_xterm'.
            "x-detect-server-trust",
            "x-lax-frame-positioning",
            "x-quit-keysym",
            "x-set-frame-visibility-more-laxly",
            "x-use-fast-mouse-position",
            // `xterm.c:32885', `32892', `32901', `32927', `32934' -- the
            // drag-and-drop callbacks, all `Qnil' in C.  GNU reports function
            // symbols for three of them only because `lisp/x-dnd.el' assigns
            // them at load time, not because the declaration does.
            "x-dnd-movement-function",
            "x-dnd-wheel-function",
            "x-dnd-unsupported-drop-function",
            "x-dnd-targets-list",
            "x-dnd-native-test-function",
            // `xterm.c:32986', `32993' -- the X input-method coding pair.
            "x-input-coding-system",
            "x-input-coding-function",
        ] {
            obarray.define_special_variable(name, Value::NIL);
        }
        // --- src/xfns.c: syms_of_xfns ---
        // The three `syms_of_xfns' names this port was short of; of the 23
        // names GNU binds from that file it already declares 20.  All three
        // are `Qnil' in C:
        // `xfns.c:10479' (`x_gtk_resize_child_frames'), `10436'
        // (`Vx_max_tooltip_size') and `10441' (`Vx_no_window_manager', whose
        // own comment reads "We don't have any way to find this out, so set it
        // to nil and maybe the user would like to set it to t").
        for name in [
            "x-gtk-resize-child-frames",
            "x-max-tooltip-size",
            "x-no-window-manager",
        ] {
            obarray.define_special_variable(name, Value::NIL);
        }
        // --- src/xselect.c: syms_of_xselect ---
        // GNU exposes these X selection notification hooks as DEFVAR_LISP
        // globals with nil defaults.
        for name in [
            "x-lost-selection-functions",
            "x-sent-selection-functions",
            // `xselect.c:3434' / `3442' DEFVAR_LISP, both `Qnil'.
            "x-treat-local-requests-remotely",
            "x-selection-alias-alist",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        // --- src/xsettings.c: syms_of_xsettings ---
        // `xsettings.c:1402' DEFVAR_LISP, `Vxft_settings = empty_unibyte_string'
        // -- the empty STRING, not nil: `Fx_get_font_settings' concatenates it
        // and `xsettings.el' passes it to `read'.  The other name in this file,
        // `font-use-system-font', is already declared here.
        obarray.define_special_variable("xft-settings", Value::unibyte_string(""));
        // --- src/ccl.c: syms_of_ccl ---
        // `ccl.c:2378' DEFVAR_LISP, `make_nil_vector (16)'.  A 16-slot vector,
        // not nil: `Fregister_code_conversion_map' and `ccl.el' index into it
        // and grow it, and `aset' on nil signals.
        obarray.define_special_variable(
            "code-conversion-map-vector",
            Value::vector(vec![Value::NIL; 16]),
        );
        // --- src/doc.c: syms_of_doc ---
        // `doc.c:691' / `695' DEFVAR_LISP, both `Qnil' at declaration time.
        // `Fsnarf_documentation' is what gives either one a value, and it does
        // so only when there is a DOC file: `Vbuild_files' is filled from
        // `buildobj.h' (`doc.c:542-553') and `Vdoc_file_name = filename' is
        // assigned *after* `doc_open' succeeds (`doc.c:555-566'), so a failed
        // open signals and leaves the name alone.  This port has no
        // `make-docfile', no `buildobj.h' and no `etc/DOC' -- `doc.rs''s
        // `Snarf-documentation' is a shim that opens nothing -- so nil is what
        // is true here as well as what GNU's declaration ships.  Writing "DOC"
        // would name a file that does not exist.
        obarray.define_special_variable("internal-doc-file-name", Value::NIL);
        obarray.define_special_variable("build-files", Value::NIL);
        // --- src/syntax.c: syms_of_syntax ---
        // `syntax.c:3747' DEFVAR_LISP, `Vcomment_use_syntax_ppss = Qt' at
        // `3749'.  t, not nil, and the two readers take opposite branches on
        // it: `find_defun_start' calls out to `syntax-ppss' only while it is
        // non-nil (`syntax.c:600'), and `back_comment' honours
        // `open-paren-in-column-0-is-defun-start' only while it is nil
        // (`syntax.c:889').  So nil is a different parser, not a disabled one.
        // Neomacs's `forward-comment' does not read it yet.
        obarray.define_special_variable("comment-use-syntax-ppss", Value::T);
        // --- src/keymap.c: syms_of_keymap ---
        // `keymap.c:3400' DEFVAR_LISP, `Qnil'.
        obarray.define_special_variable("describe-bindings-check-shadowing-in-ranges", Value::NIL);
        // --- src/textconv.c: syms_of_textconv ---
        // `textconv.c:2593' DEFVAR_LISP `Qnil', and `2631' DEFVAR_LISP
        // `Qunderline' -- a face NAME, so nil would mean "no face" rather than
        // GNU's underline.  `overriding-text-conversion-style', the third name
        // in this file, is declared above.
        obarray.define_special_variable("text-conversion-edits", Value::NIL);
        obarray.define_special_variable("text-conversion-face", Value::symbol("underline"));
        // --- src/menu.c: syms_of_menu ---
        // `menu.c:1629' DEFVAR_LISP, `Qnil'.  `x-pre-popup-menu-hook', the only
        // other name in `menu.c', is already declared here.
        obarray.define_special_variable("x-popup-menu-function", Value::NIL);
        // --- src/dispnew.c: syms_of_display ---
        // `dispnew.c:7567' DEFVAR_LISP, `make_fixnum (5)'.  Spelled `x-' but
        // declared in `dispnew.o', which is in GNU's unconditional `base_obj'.
        obarray.define_special_variable("x-show-tooltip-timeout", Value::fixnum(5));
    }

    fn new_inner(reset_thread_locals: bool) -> Self {
        // Create the heap and set thread-locals so tagged constructors work
        // during evaluator initialization.
        let mut tagged_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
        crate::tagged::gc::set_tagged_heap(&mut tagged_heap);

        // Clear any caches that hold heap-allocated Values (tagged pointers) from a
        // previous heap. Critical for test isolation when multiple Contexts
        // are created sequentially on the same thread.
        if reset_thread_locals {
            super::pdump::runtime::reset_runtime_for_new_heap(
                super::pdump::runtime::HeapResetMode::FreshContext,
            );
        }

        let mut obarray = Obarray::new();
        // Builtin names are interned by SubrSpec registration during init_builtins(),
        // which runs after Context construction.
        let default_directory = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .map(|mut s| {
                if !s.ends_with('/') {
                    s.push('/');
                }
                s
            })
            .unwrap_or_else(|| "./".to_string());
        // Create all keymaps as Emacs-compatible cons-list values
        let minibuffer_local_map = make_sparse_list_keymap();
        // Keep only the base minibuffer map here. GNU Lisp defines
        // `read-expression-map` / `read--expression-map` itself in simple.el via
        // `defvar-keymap`; prebinding them here causes those definitions to be
        // skipped, which leaves RET/C-j handling diverged from GNU Emacs.
        // Standard keymaps required by loadup.el files (normally created by C code)
        // `global-map`, `esc-map`, `ctl-x-map`, and `help-map` are defined in GNU Lisp,
        // so keep them unbound here and let the Lisp `defvar` / `defvar-keymap`
        // initializers run.  Prebinding them here causes GNU definitions like
        // help.el's `defvar-keymap help-map ...` to skip installing their real
        // bindings.
        let special_event_map = make_sparse_list_keymap();
        let mode_line_window_dedicated_keymap = make_sparse_list_keymap();
        let indent_rigidly_map = make_sparse_list_keymap();
        let text_mode_map = make_sparse_list_keymap();
        let image_slice_map = make_sparse_list_keymap();
        let tool_bar_map = make_sparse_list_keymap();
        let key_translation_map = make_sparse_list_keymap();
        let function_key_map = make_sparse_list_keymap();
        let input_decode_map = make_sparse_list_keymap();
        let local_function_key_map = make_sparse_list_keymap();
        // GNU Emacs: local-function-key-map inherits from function-key-map
        // (keyboard.c:13097). Without this, bindings in function-key-map
        // (like [backspace] → [?\C-?]) are not found during key translation.
        list_keymap_set_parent(local_function_key_map, function_key_map);
        // GNU keyboard.c seeds special-event-map with delete-frame and focus
        // handlers at C bootstrap time and leaves hook semantics to frame.el.
        list_keymap_define(
            special_event_map,
            Value::symbol("delete-frame"),
            Value::symbol("handle-delete-frame"),
        );
        list_keymap_define(
            special_event_map,
            Value::symbol("focus-in"),
            Value::symbol("handle-focus-in"),
        );
        list_keymap_define(
            special_event_map,
            Value::symbol("focus-out"),
            Value::symbol("handle-focus-out"),
        );
        // GNU's `dbus-event' entry (src/keyboard.c:14572-14576) is inside
        // `#ifdef HAVE_DBUS', as are its three neighbours -- the DEFSYM
        // (`:13477'), the `while-no-input-ignore-events' cons (`:13325') and
        // the `DBUS_EVENT' ignore-event case (`:13370').  This build has no
        // D-Bus transport, so it installs none of them (ledger 192).
        // GNU keyboard.c installs file notification events in
        // `special-event-map` when file notification support is present.
        list_keymap_define(
            special_event_map,
            Value::symbol("file-notify"),
            Value::symbol("file-notify-handle-event"),
        );

        let standard_syntax_table = super::syntax::builtin_standard_syntax_table(Vec::new())
            .expect("startup seeding requires standard syntax table");
        let syntax_code_objects = super::syntax::snapshot_syntax_code_objects()
            .unwrap_or_else(super::syntax::ensure_syntax_code_objects);
        let standard_category_table = super::category::ensure_standard_category_table_object()
            .expect("startup seeding requires standard category table");

        Self::seed_startup_platform_variables(&mut obarray, default_directory);
        // GNU DEFVAR_LISP variables from eval.c / keyboard.c.
        let core_eval_symbols = install_core_eval_symbols(&mut obarray, true);
        Self::seed_reader_keyboard_variables(
            &mut obarray,
            standard_syntax_table,
            minibuffer_local_map,
        );
        // ---- C-level bootstrap variables required by loadup.el files ----

        // Standard keymaps (C creates these in keyboard.c:init_kboard)
        // keyboard.c:14130 DEFVAR_LISP -- special like every C DEFVAR.
        obarray.define_special_variable("special-event-map", special_event_map);
        obarray.set_symbol_value(
            "mode-line-window-dedicated-keymap",
            mode_line_window_dedicated_keymap,
        );
        obarray.set_symbol_value("indent-rigidly-map", indent_rigidly_map);
        obarray.set_symbol_value("text-mode-map", text_mode_map);
        obarray.set_symbol_value("image-slice-map", image_slice_map);
        obarray.set_symbol_value("tool-bar-map", tool_bar_map);
        // keyboard.c:14210 / 14202 DEFVAR_LISP -- special like every C DEFVAR.
        obarray.define_special_variable("key-translation-map", key_translation_map);
        obarray.define_special_variable("function-key-map", function_key_map);
        obarray.set_symbol_value("input-decode-map", input_decode_map);
        obarray.make_special("input-decode-map");
        obarray.set_symbol_value("local-function-key-map", local_function_key_map);
        obarray.make_special("local-function-key-map");
        obarray.set_symbol_value("keyboard-translate-table", Value::NIL);
        // GNU uses DEFVAR_KBOARD here. NeoVM does not yet split keyboard state
        // per terminal, so model it as a dynamically scoped runtime variable.
        obarray.make_special("keyboard-translate-table");

        Self::seed_core_eval_variables(&mut obarray);
        let mut custom = CustomManager::new();
        Self::seed_c_level_defvars(&mut obarray, &mut custom);

        #[cfg(target_os = "windows")]
        super::w32::register_bootstrap_symbols(&mut obarray);

        let mut command_loop = crate::keyboard::CommandLoop::new();
        command_loop
            .keyboard
            .set_terminal_translation_maps(input_decode_map, local_function_key_map);
        let noninteractive = obarray
            .symbol_value_id_or_nil(core_eval_symbols.noninteractive_symbol)
            .is_truthy();
        let symbols_with_pos_enabled = obarray
            .symbol_value_id_or_nil(core_eval_symbols.symbols_with_pos_enabled_symbol)
            .is_truthy();
        let print_symbols_bare = obarray
            .symbol_value_id_or_nil(core_eval_symbols.print_symbols_bare_symbol)
            .is_truthy();
        let compiler_function_overrides_active = obarray
            .symbol_value_id_or_nil(core_eval_symbols.compiler_function_overrides_symbol)
            .is_cons();
        let quit_flag = obarray.symbol_value_id_or_nil(core_eval_symbols.quit_flag_symbol);
        let inhibit_quit = obarray.symbol_value_id_or_nil(core_eval_symbols.inhibit_quit_symbol);
        let throw_on_input =
            obarray.symbol_value_id_or_nil(core_eval_symbols.throw_on_input_symbol);

        let mut ev = Self {
            tagged_heap,
            pdump_image: None,
            after_pdump_load_hook_pending: false,
            cached_system_name: Value::NIL,
            obarray,
            specpdl: Vec::new(),
            suspended_thread_bindings: Vec::new(),
            profiler: super::profiler::ProfilerState::default(),
            lexenv: Value::NIL,
            internal_interpreter_environment_symbol: core_eval_symbols
                .internal_interpreter_environment_symbol,
            load_read_stream_token: core_eval_symbols.load_read_stream_token,
            quit_flag_symbol: core_eval_symbols.quit_flag_symbol,
            inhibit_quit_symbol: core_eval_symbols.inhibit_quit_symbol,
            throw_on_input_symbol: core_eval_symbols.throw_on_input_symbol,
            kill_emacs_symbol: core_eval_symbols.kill_emacs_symbol,
            quit_flag,
            inhibit_quit,
            throw_on_input,
            unwind_cleanup_depth: 0,
            noninteractive_symbol: core_eval_symbols.noninteractive_symbol,
            noninteractive,
            symbols_with_pos_enabled_symbol: core_eval_symbols.symbols_with_pos_enabled_symbol,
            symbols_with_pos_enabled,
            print_symbols_bare_symbol: core_eval_symbols.print_symbols_bare_symbol,
            print_symbols_bare,
            features: initial_feature_ids(),
            require_stack: Vec::new(),
            loads_in_progress: Vec::new(),
            load_read_cursors: Vec::new(),
            last_uncaught_signal_backtrace: None,
            buffers: BufferManager::new(),
            xwidgets: super::xwidget::XwidgetState::new(),
            last_overlay_modification_hooks: Vec::new(),
            interval_insert_behind_hooks: Value::NIL,
            interval_insert_in_front_hooks: Value::NIL,
            match_data: None,
            combine_after_change_list: Vec::new(),
            combine_after_change_buffer: None,
            processes: ProcessManager::new(),
            watchers: VariableWatcherList::new(),
            active_variable_watchers: HashSet::new(),
            standard_syntax_table,
            syntax_code_objects,
            standard_category_table,
            current_local_map: Value::NIL,
            selected_global_map: super::keymap::SelectedGlobalMap::default(),
            registers: RegisterManager::new(),
            bookmarks: BookmarkManager::new(),
            abbrevs: AbbrevManager::new(),
            autoloads: AutoloadManager::new(),
            custom,
            rectangle: RectangleState::new(),
            interactive: InteractiveRegistry::new(),
            treesit: super::treesit::TreeSitterManager::new(),
            minibuffers: MinibufferManager::new(),
            interactive_minibuffer_read_count: 0,
            current_message: None,
            echo_area_buffers: EchoAreaBuffers::default(),
            echo_area_resize_exact_pending: false,
            debugging_output_file: None,
            message_buf_print: false,
            minibuffer_selected_window: None,
            active_minibuffer_window: None,
            shutdown_request: None,
            input_mode_interrupt: true,
            quit_char: 7,
            waiting_for_user_input: false,
            frames: lisp_frame_manager(),
            modes: ModeRegistry::new(),
            threads: ThreadManager::new(),
            kmacro: KmacroManager::new(),
            command_loop,
            input_rx: None,
            eval_task_rx: None,
            quit_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            redisplay_fn: None,
            frame_snapshot_fn: None,
            window_layout_query_adapter: WindowLayoutQueryAdapter::Unavailable,
            pending_pixel_scroll: None,
            display_host: None,
            tty_frame_host_factory: None,
            visual_config: neomacs_display_protocol::VisualConfig::default(),
            pending_menu_bar_popup_anchor: None,
            coding_systems: CodingSystemManager::new(),
            code_conversion_workspace:
                crate::code_conversion_workspace::CodeConversionWorkspace::default(),
            face_table: FaceTable::new(),
            face_change_count: 0,
            materialized_face_table_source: None,
            display_var_change_count: 0,
            redisplay_generation: 0,
            menu_bar_rebuild_generation: 0,
            chrome_dirty: Default::default(),
            context_instance_id: next_context_instance_id(),
            media_generation: 0,
            last_redisplay_signature: None,
            depth: 0,
            eval_counter: 0,
            max_depth: 1600,
            gc_pending: false,
            gc_count: 0,
            gc_inhibit_depth: 0,
            gc_driver_active: false,
            gc_stress: gc_stress_from_env(),
            gc_runtime_settings_cache: GcRuntimeSettingsCache::default(),
            vm_root_frames: Vec::new(),
            backtrace_args_stack: Vec::new(),
            eval_temp_roots: Vec::new(),
            sequence_temp_root_frames: Vec::new(),
            bc_buf: Vec::with_capacity(4096),
            jit_root_stack: Vec::new(),
            jit_root_stack_ptr: std::ptr::null_mut(),
            jit_root_stack_top: 0,
            jit_root_stack_cap: 0,
            bc_frames: Vec::new(),
            condition_stack: Vec::new(),
            next_resume_id: 1,
            pending_safe_funcalls: Vec::new(),
            compiler_function_overrides_symbol: core_eval_symbols
                .compiler_function_overrides_symbol,
            compiler_function_overrides_active,
            named_call_cache: FxHashMap::with_capacity_and_hasher(
                NAMED_CALL_CACHE_CAPACITY,
                Default::default(),
            ),
            lexenv_assq_cache: LexenvAssqCache::default(),
            lexenv_special_cache: LexenvSpecialCache::default(),

            macro_expansion_scope_depth: 0,
            macro_expansion_mutation_epoch: 0,
            macro_expand_calls: 0,
            macro_expand_total_us: 0,
            macro_perf_enabled: std::env::var_os("NEOVM_TRACE_MACRO_PERF").is_some(),
            macro_perf_stats: MacroPerfStats::default(),
            interpreted_closure_filter_fn: None,
            fringe_bitmaps: super::builtins::fringe_bitmap::FringeBitmapRegistry::new(),
        };
        super::runtime_identity::install(&mut ev);
        ev.provide_value(
            Value::symbol("make-network-process"),
            Some(super::process::make_network_process_subfeatures()),
        )
        .expect("startup make-network-process provide should succeed");
        ev.finish_runtime_activation(false);
        ev
    }

    // -----------------------------------------------------------------------
    // pdump reconstruction
    // -----------------------------------------------------------------------

    /// Reconstruct an Context from pdump data.
    ///
    /// Thread-local heap pointers and caches must already be set by the caller
    /// before calling this.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_dump(
        tagged_heap: Box<crate::tagged::gc::TaggedHeap>,
        obarray: Obarray,
        lexenv: Value,
        features: Vec<SymId>,
        require_stack: Vec<SymId>,
        loads_in_progress: Vec<crate::heap_types::LispString>,
        buffers: BufferManager,
        autoloads: AutoloadManager,
        custom: CustomManager,
        modes: ModeRegistry,
        coding_systems: CodingSystemManager,
        face_table: FaceTable,
        abbrevs: AbbrevManager,
        interactive: InteractiveRegistry,
        rectangle: RectangleState,
        standard_syntax_table: Value,
        syntax_code_objects: Value,
        standard_category_table: Value,
        current_local_map: Value,
        selected_global_map: super::keymap::SelectedGlobalMap,
        kmacro: KmacroManager,
        registers: RegisterManager,
        bookmarks: BookmarkManager,
        watchers: VariableWatcherList,
    ) -> Self {
        let dumped_function_surface = obarray.clone();
        let mut obarray = obarray;
        let core_eval_symbols = install_core_eval_symbols(&mut obarray, false);
        let mut tagged_heap = tagged_heap;
        crate::tagged::gc::set_tagged_heap(&mut tagged_heap);
        let noninteractive = obarray
            .symbol_value_id_or_nil(core_eval_symbols.noninteractive_symbol)
            .is_truthy();
        let symbols_with_pos_enabled = obarray
            .symbol_value_id_or_nil(core_eval_symbols.symbols_with_pos_enabled_symbol)
            .is_truthy();
        let print_symbols_bare = obarray
            .symbol_value_id_or_nil(core_eval_symbols.print_symbols_bare_symbol)
            .is_truthy();
        let compiler_function_overrides_active = obarray
            .symbol_value_id_or_nil(core_eval_symbols.compiler_function_overrides_symbol)
            .is_cons();
        let quit_flag = obarray.symbol_value_id_or_nil(core_eval_symbols.quit_flag_symbol);
        let inhibit_quit = obarray.symbol_value_id_or_nil(core_eval_symbols.inhibit_quit_symbol);
        let throw_on_input =
            obarray.symbol_value_id_or_nil(core_eval_symbols.throw_on_input_symbol);

        let mut ev = Self {
            tagged_heap,
            pdump_image: None,
            after_pdump_load_hook_pending: false,
            cached_system_name: Value::NIL,
            obarray,
            specpdl: Vec::new(),
            suspended_thread_bindings: Vec::new(),
            profiler: super::profiler::ProfilerState::default(),
            lexenv,
            internal_interpreter_environment_symbol: core_eval_symbols
                .internal_interpreter_environment_symbol,
            load_read_stream_token: core_eval_symbols.load_read_stream_token,
            quit_flag_symbol: core_eval_symbols.quit_flag_symbol,
            inhibit_quit_symbol: core_eval_symbols.inhibit_quit_symbol,
            throw_on_input_symbol: core_eval_symbols.throw_on_input_symbol,
            kill_emacs_symbol: core_eval_symbols.kill_emacs_symbol,
            quit_flag,
            inhibit_quit,
            throw_on_input,
            unwind_cleanup_depth: 0,
            noninteractive_symbol: core_eval_symbols.noninteractive_symbol,
            noninteractive,
            symbols_with_pos_enabled_symbol: core_eval_symbols.symbols_with_pos_enabled_symbol,
            symbols_with_pos_enabled,
            print_symbols_bare_symbol: core_eval_symbols.print_symbols_bare_symbol,
            print_symbols_bare,
            features,
            require_stack,
            loads_in_progress,
            load_read_cursors: Vec::new(),
            last_uncaught_signal_backtrace: None,
            buffers,
            xwidgets: super::xwidget::XwidgetState::new(),
            last_overlay_modification_hooks: Vec::new(),
            interval_insert_behind_hooks: Value::NIL,
            interval_insert_in_front_hooks: Value::NIL,
            match_data: None,
            combine_after_change_list: Vec::new(),
            combine_after_change_buffer: None,
            processes: ProcessManager::new(),
            watchers,
            active_variable_watchers: HashSet::new(),
            standard_syntax_table,
            syntax_code_objects,
            standard_category_table,
            current_local_map,
            selected_global_map,
            registers,
            bookmarks,
            abbrevs,
            autoloads,
            custom,
            rectangle,
            interactive,
            treesit: super::treesit::TreeSitterManager::new(),
            minibuffers: MinibufferManager::new(),
            interactive_minibuffer_read_count: 0,
            current_message: None,
            echo_area_buffers: EchoAreaBuffers::default(),
            echo_area_resize_exact_pending: false,
            debugging_output_file: None,
            message_buf_print: false,
            minibuffer_selected_window: None,
            active_minibuffer_window: None,
            shutdown_request: None,
            input_mode_interrupt: true,
            quit_char: 7,
            waiting_for_user_input: false,
            frames: lisp_frame_manager(),
            modes,
            threads: ThreadManager::new(),
            kmacro,
            command_loop: crate::keyboard::CommandLoop::new(),
            input_rx: None,
            eval_task_rx: None,
            quit_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            redisplay_fn: None,
            frame_snapshot_fn: None,
            window_layout_query_adapter: WindowLayoutQueryAdapter::Unavailable,
            pending_pixel_scroll: None,
            display_host: None,
            tty_frame_host_factory: None,
            visual_config: neomacs_display_protocol::VisualConfig::default(),
            pending_menu_bar_popup_anchor: None,
            coding_systems,
            code_conversion_workspace:
                crate::code_conversion_workspace::CodeConversionWorkspace::default(),
            face_table,
            face_change_count: 0,
            materialized_face_table_source: None,
            display_var_change_count: 0,
            redisplay_generation: 0,
            menu_bar_rebuild_generation: 0,
            chrome_dirty: Default::default(),
            context_instance_id: next_context_instance_id(),
            media_generation: 0,
            last_redisplay_signature: None,
            depth: 0,
            eval_counter: 0,
            max_depth: 1600,
            gc_pending: false,
            gc_count: 0,
            gc_inhibit_depth: 0,
            gc_driver_active: false,
            gc_stress: gc_stress_from_env(),
            gc_runtime_settings_cache: GcRuntimeSettingsCache::default(),
            vm_root_frames: Vec::new(),
            backtrace_args_stack: Vec::new(),
            eval_temp_roots: Vec::new(),
            sequence_temp_root_frames: Vec::new(),
            bc_buf: Vec::with_capacity(4096),
            jit_root_stack: Vec::new(),
            jit_root_stack_ptr: std::ptr::null_mut(),
            jit_root_stack_top: 0,
            jit_root_stack_cap: 0,
            bc_frames: Vec::new(),
            condition_stack: Vec::new(),
            next_resume_id: 1,
            pending_safe_funcalls: Vec::new(),
            compiler_function_overrides_symbol: core_eval_symbols
                .compiler_function_overrides_symbol,
            compiler_function_overrides_active,
            named_call_cache: FxHashMap::with_capacity_and_hasher(
                NAMED_CALL_CACHE_CAPACITY,
                Default::default(),
            ),
            lexenv_assq_cache: LexenvAssqCache::default(),
            lexenv_special_cache: LexenvSpecialCache::default(),

            macro_expansion_scope_depth: 0,
            macro_expansion_mutation_epoch: 0,
            macro_expand_calls: 0,
            macro_expand_total_us: 0,
            macro_perf_enabled: std::env::var_os("NEOVM_TRACE_MACRO_PERF").is_some(),
            macro_perf_stats: MacroPerfStats::default(),
            interpreted_closure_filter_fn: None,
            fringe_bitmaps: super::builtins::fringe_bitmap::FringeBitmapRegistry::new(),
        };
        ev.initialize_gc_stack_bottom();
        ev.setup_thread_locals();

        // Rebuild the builtin subr registry after pdump restore. The dumped
        // obarray already carries the authoritative runtime function-cell
        // surface, so restore that surface immediately afterward.
        builtins::init_builtins(&mut ev);
        for (sym_id, symbol) in dumped_function_surface.iter_symbols() {
            if !symbol.function.is_nil() {
                ev.obarray.set_symbol_function_id(sym_id, symbol.function);
            } else if dumped_function_surface.is_function_unbound_id(sym_id) {
                ev.obarray.fmakunbound_id(sym_id);
            } else {
                ev.obarray.clear_function_silent_id(sym_id);
            }
        }

        ev.provide_value(
            Value::symbol("make-network-process"),
            Some(super::process::make_network_process_subfeatures()),
        )
        .expect("startup make-network-process provide should succeed");

        // The fringe-bitmap registry is reconstructed empty by `from_dump` (it
        // is not part of the dump image), so re-seed GNU's standard built-in
        // bitmaps. The `'fringe` indices may already be set on the dumped
        // symbols; re-`put`ting the same value is idempotent.
        ev.pre_register_standard_fringe_bitmaps();

        ev.finish_runtime_activation(true);

        ev
    }

    pub(crate) fn install_pdump_image(&mut self, image: super::pdump::mmap_image::LoadedMmapImage) {
        // Leak: see the field doc — global interner aliases and mapped heap
        // objects reference the image for the remainder of the process.
        self.pdump_image = Some(&*Box::leak(Box::new(image)));
    }

    #[cfg(test)]
    pub(crate) fn pdump_image_contains_ptr(&self, ptr: *const u8) -> bool {
        self.pdump_image
            .as_ref()
            .is_some_and(|image| image.contains_ptr(ptr))
    }

    // -----------------------------------------------------------------------
    // Garbage collection
    // -----------------------------------------------------------------------

    /// Enumerate every live `Value` reference in the evaluator and all
    /// sub-managers without materializing a single temporary root vector.
    /// Enumerate every evaluator/context root into `visit`, announcing each
    /// root GROUP boundary via `group(name)` immediately before that group's
    /// values are visited. The group seam is diagnostics-only: the GC
    /// handshake instrumentation brackets per-group timings around the
    /// boundaries; enumeration order and content are unchanged.
    fn trace_roots(&self, group: &mut dyn FnMut(&'static str), visit: &mut dyn FnMut(Value)) {
        group("vm_frames");
        for frame in &self.vm_root_frames {
            for root in frame.roots.iter().copied() {
                visit(root);
            }
        }
        group("eval_temp");
        for root in self.eval_temp_roots.iter().copied() {
            visit(root);
        }
        group("treesit");
        for root in self.treesit.roots() {
            visit(root);
        }
        group("bc");
        for root in self.bc_buf.iter().copied() {
            visit(root);
        }
        group("jit_window");
        for root in self.jit_root_stack[..self.jit_root_stack_top]
            .iter()
            .copied()
        {
            visit(root);
        }
        for frame in &self.bc_frames {
            if frame.fun.is_heap_object() {
                visit(frame.fun);
            }
        }
        group("handlers");
        for frame in self.condition_stack.iter().chain(
            self.suspended_thread_bindings
                .iter()
                .flat_map(|state| state.condition_stack.iter()),
        ) {
            match frame {
                ConditionFrame::Catch { tag, .. } => visit(*tag),
                ConditionFrame::ConditionCase { conditions, .. } => visit(*conditions),
                ConditionFrame::HandlerBind {
                    conditions,
                    handler,
                    ..
                } => {
                    visit(*conditions);
                    visit(*handler);
                }
                ConditionFrame::SkipConditions { .. } => {}
            }
        }
        group("specpdl");
        for state in &self.suspended_thread_bindings {
            visit(state.lexenv);
        }
        for entry in self.specpdl.iter().chain(
            self.suspended_thread_bindings
                .iter()
                .flat_map(|state| state.specpdl.iter()),
        ) {
            match entry {
                SpecBinding::Let { old_value, .. } => {
                    if let Some(value) = old_value.get() {
                        visit(value);
                    }
                }
                SpecBinding::LetLocal { old_value, .. } => visit(*old_value),
                SpecBinding::LetDefault { old_value, .. } => {
                    if let Some(value) = old_value.get() {
                        visit(value);
                    }
                }
                SpecBinding::LexicalEnv { old_lexenv } => visit(*old_lexenv),
                SpecBinding::GcRoot { value } => visit(*value),
                SpecBinding::Backtrace { function, args, .. } => {
                    visit(*function);
                    self.trace_backtrace_args(args, visit);
                }
                SpecBinding::Backtrace1 { function, arg, .. } => {
                    visit(*function);
                    visit(*arg);
                }
                SpecBinding::Backtrace2 {
                    function,
                    arg0,
                    arg1,
                } => {
                    visit(*function);
                    visit(*arg0);
                    visit(*arg1);
                }
                SpecBinding::BacktraceNative {
                    function,
                    args_ptr,
                    nargs,
                } => {
                    visit(*function);
                    // SAFETY: the variant's contract — the caller's
                    // call-args slot stays alive (and unmutated) while
                    // this entry exists, and root seeding runs with the
                    // mutator stopped.
                    for i in 0..*nargs as usize {
                        visit(Value::from_bits(unsafe { *args_ptr.add(i) } as usize));
                    }
                }
                SpecBinding::UnwindProtect { forms, lexenv } => {
                    visit(*forms);
                    visit(*lexenv);
                }
                SpecBinding::SaveRestriction { state } => {
                    let mut roots = Vec::new();
                    state.state().trace_roots(&mut roots);
                    // The saved bounds live as marker ids only; root the
                    // marker objects so restore still finds them (see
                    // SavedRestrictionState::trace_marker_roots).
                    state.state().trace_marker_roots(&self.buffers, &mut roots);
                    for root in roots {
                        visit(root);
                    }
                }
                SpecBinding::SaveExcursion { marker, .. } => visit(*marker),
                SpecBinding::NativeUnwind { action } => action.trace_roots(visit),
                // EXHAUSTIVE ON PURPOSE — no catch-all arm. These four carry
                // no Lisp value (a buffer id, two lengths, nothing), and a new
                // `SpecBinding` variant must state which group it belongs to
                // instead of being absorbed by a `_ => {}`. A root walk is the
                // one match where "the compiler did not complain" and "the
                // value is marked" must be the same sentence
                // (DIVERGENCES.md 161's residual, closed by 162).
                SpecBinding::SaveCurrentBuffer { .. }
                | SpecBinding::LoadsInProgress { .. }
                | SpecBinding::RequireStack { .. }
                | SpecBinding::Nop => {}
            }
        }
        group("profiler");
        self.trace_profiler_roots(visit);
        group("misc");
        visit(self.lexenv);
        visit(self.quit_flag);
        visit(self.inhibit_quit);
        if self.cached_system_name.is_heap_object() {
            visit(self.cached_system_name);
        }
        if let Some(filter_fn) = self.interpreted_closure_filter_fn {
            visit(filter_fn);
        }
        for entry in self.named_call_cache.values() {
            if let NamedCallTarget::Obarray(val) = &entry.target {
                visit(*val);
            }
        }
        for funcall in &self.pending_safe_funcalls {
            visit(funcall.function);
            for arg in funcall.args.iter().copied() {
                visit(arg);
            }
        }
        for hook in &self.last_overlay_modification_hooks {
            visit(hook.hook_list);
            visit(hook.overlay);
        }
        if !self.interval_insert_behind_hooks.is_nil() {
            visit(self.interval_insert_behind_hooks);
        }
        if !self.interval_insert_in_front_hooks.is_nil() {
            visit(self.interval_insert_in_front_hooks);
        }
        if !self.current_local_map.is_nil() {
            visit(self.current_local_map);
        }
        let selected_global_map = self.selected_global_map.value();
        if !selected_global_map.is_nil() {
            visit(selected_global_map);
        }
        if self.standard_syntax_table.is_heap_object() {
            visit(self.standard_syntax_table);
        }
        if self.syntax_code_objects.is_heap_object() {
            visit(self.syntax_code_objects);
        }
        if self.standard_category_table.is_heap_object() {
            visit(self.standard_category_table);
        }
        // Full ~all-interned-symbols walk on STW collections; only the
        // BLV-pool residual under `ObarraySymbolCellSkipGuard` (both
        // concurrent handshakes).
        group("obarray");
        self.obarray.trace_roots_with(visit);
        group("proc_timer");
        self.processes.trace_roots_with(visit);
        self.watchers.trace_roots_with(visit);
        group("reg_custom");
        self.registers.trace_roots_with(visit);
        self.custom.trace_roots_with(visit);
        self.autoloads.trace_roots_with(visit);
        self.interactive.trace_roots_with(visit);
        group("buffers");
        self.buffers.trace_roots_with(visit);
        group("ui_misc");
        self.xwidgets.trace_roots_with(visit);
        self.face_table.trace_roots_with(visit);
        self.threads.trace_roots_with(visit);
        self.kmacro.trace_roots_with(visit);
        crate::gc_trace::GcTrace::trace_roots_with(&self.command_loop, visit);
        self.modes.trace_roots_with(visit);
        self.frames.trace_roots_with(visit);
        self.coding_systems.trace_roots_with(visit);
        group("match_data");
        if let Some(ref md) = self.match_data
            && let Some(crate::emacs_core::regex::SearchedString::Heap(val)) = md.searched_string()
        {
            visit(*val);
        }
    }

    /// Get the current GC threshold.
    pub fn gc_threshold(&self) -> usize {
        self.tagged_heap.gc_threshold()
    }

    /// Whether `sym_id` is one of the GC-setting variables (compared against
    /// the live-resolved ids; `false` until the first settings refresh has
    /// resolved them — the GC-end/decision-point refresh covers that window).
    fn is_gc_runtime_setting_symbol(&self, sym_id: SymId) -> bool {
        self.gc_runtime_settings_cache
            .syms
            .is_some_and(|syms| syms.contains(sym_id))
    }

    pub(crate) fn refresh_gc_runtime_settings_after_change_by_id(&mut self, sym_id: SymId) {
        if self.is_gc_runtime_setting_symbol(sym_id) {
            self.refresh_gc_runtime_settings_cache();
            self.sync_gc_threshold_from_runtime_settings();
        }
    }

    fn refresh_gc_runtime_settings_cache(&mut self) {
        // Re-resolve the variable names against the LIVE interner every time
        // (four hash lookups on a rare path): see `GcRuntimeSettingsCache::syms`.
        let syms = GcSettingSyms::resolve();
        self.gc_runtime_settings_cache.syms = Some(syms);
        self.gc_runtime_settings_cache.gc_cons_threshold_bytes = self
            .obarray
            .symbol_value_id(syms.threshold)
            .copied()
            .and_then(|value| value.as_fixnum())
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(GC_DEFAULT_THRESHOLD_BYTES);
        self.gc_runtime_settings_cache.gc_cons_percentage_scaled = self
            .obarray
            .symbol_value_id_or_nil(syms.percentage)
            .as_number_f64()
            .filter(|float| float.is_finite() && *float > 0.0)
            .map(|float| ((float * GC_PERCENT_SCALE as f64).ceil() as u64).clamp(1, u64::MAX));
        self.gc_runtime_settings_cache.memory_full = !self
            .obarray
            .symbol_value_id_or_nil(syms.memory_full)
            .is_nil();
    }

    fn effective_gc_threshold_bytes(&mut self) -> usize {
        if self.gc_runtime_settings_cache.memory_full {
            return self.tagged_heap.gc_threshold();
        }

        let mut threshold = self
            .gc_runtime_settings_cache
            .gc_cons_threshold_bytes
            .max(GC_THRESHOLD_FLOOR_BYTES);
        if let Some(percentage_scaled) = self.gc_runtime_settings_cache.gc_cons_percentage_scaled {
            let live_estimate = self
                .tagged_heap
                .live_bytes()
                .saturating_add(self.tagged_heap.bytes_since_gc() / 2);
            let pct_threshold = ((live_estimate as u128)
                .saturating_mul(percentage_scaled as u128)
                .saturating_add((GC_PERCENT_SCALE - 1) as u128)
                / GC_PERCENT_SCALE as u128)
                .min(GC_HI_THRESHOLD_BYTES as u128) as usize;
            threshold = threshold.max(pct_threshold);
        }
        // Internal live-proportional growth term: trigger only once at least
        // GC_LIVE_GROWTH_NUM/GC_LIVE_GROWTH_DEN of the live heap has been
        // allocated since the last cycle, so the full-mark cost (O(live))
        // amortizes as the heap grows. Invariants: strict max — the
        // elisp-derived value above stays a floor this term never lowers (user
        // settings and the defaults keep their meaning as minimum budgets);
        // overridden thresholds (`set_gc_threshold`) are unaffected because
        // this value only flows through `set_gc_threshold_from_runtime`; the
        // GC_HI clamp below still bounds the result. `live_bytes` only grows
        // between sweeps (it is recomputed exactly at each sweep), which is
        // safe for a max term.
        let live_growth = ((self.tagged_heap.live_bytes() as u128)
            .saturating_mul(GC_LIVE_GROWTH_NUM)
            / GC_LIVE_GROWTH_DEN)
            .min(GC_HI_THRESHOLD_BYTES as u128) as usize;
        threshold = threshold.max(live_growth);
        let mut threshold = threshold.clamp(1, GC_HI_THRESHOLD_BYTES);
        if self.gc_runtime_settings_cache.syms.is_some_and(|syms| {
            !self
                .obarray
                .symbol_value_id_or_nil(syms.startup_ceiling)
                .is_nil()
        }) {
            threshold = threshold.min(GC_STARTUP_THRESHOLD_CEILING_BYTES);
        }
        gc_threshold_cap_from_env().map_or(threshold, |cap| threshold.min(cap))
    }

    fn sync_gc_threshold_from_runtime_settings(&mut self) {
        // Read the Lisp variables LIVE here, like GNU's `garbage_collect` end
        // (`consing_until_gc = consing_threshold (gc_cons_threshold,
        // Vgc_cons_percentage, 0)`), instead of trusting a cache that only the
        // setter paths routed through `refresh_gc_runtime_settings_after_
        // change_by_id` keep current: any write that bypasses them (a direct
        // forwarder store, a future setter) is then honored at the next GC,
        // which is exactly GNU's contract for a changed threshold.
        self.refresh_gc_runtime_settings_cache();
        let threshold = self.effective_gc_threshold_bytes();
        if self.tagged_heap.gc_threshold() != threshold {
            self.tagged_heap.set_gc_threshold_from_runtime(threshold);
        }
    }

    fn update_gc_runtime_stats(&mut self, elapsed: std::time::Duration) {
        self.obarray
            .set_symbol_value_id(gcs_done_symbol(), Value::fixnum(self.gc_count as i64));

        let old_elapsed = self
            .obarray
            .symbol_value_id(gc_elapsed_symbol())
            .copied()
            .and_then(|value| value.as_number_f64())
            .unwrap_or(0.0);
        self.obarray.set_symbol_value_id(
            gc_elapsed_symbol(),
            Value::make_float(old_elapsed + elapsed.as_secs_f64()),
        );

        // Publish a cross-thread snapshot for the diagnostics server. Sampled
        // here, once per GC cycle, so the diagnostics thread never touches the
        // heap; values between collections are the last post-sweep reading.
        let counts = self.tagged_heap.memory_use_counts_snapshot();
        crate::emacs_core::gc_stats::publish(crate::emacs_core::gc_stats::GcStatsSnapshot {
            collections: self.gc_count,
            live_bytes: self.tagged_heap.live_bytes() as u64,
            total_allocated_bytes: self.tagged_heap.total_allocated_bytes(),
            cons_cells: counts[0],
            vector_cells: counts[2],
            strings: counts[6],
        });
    }

    /// Set the GC threshold. Use usize::MAX to effectively disable GC.
    pub fn set_gc_threshold(&mut self, threshold: usize) {
        self.tagged_heap.set_gc_threshold(threshold);
    }

    /// Set the maximum eval recursion depth.
    pub fn set_max_depth(&mut self, depth: usize) {
        self.max_depth = depth;
    }

    /// Set the thread-local heap pointers for the current thread.
    ///
    /// Must be called when using an Context from a thread other than the one
    /// that created it (e.g., in worker thread pools).
    pub fn setup_thread_locals(&mut self) {
        crate::tagged::gc::set_tagged_heap(&mut self.tagged_heap);
        super::syntax::restore_standard_syntax_table_object(self.standard_syntax_table);
        super::syntax::restore_syntax_code_objects(self.syntax_code_objects);
        super::category::restore_standard_category_table_object(self.standard_category_table);
        // Install this Context's quit-request flag so leaf functions
        // (regex matcher, other long-running scans) can poll it
        // without `&mut Context` access.
        QUIT_REQUESTED_TLS.with(|cell| {
            *cell.borrow_mut() = Some(std::sync::Arc::clone(&self.quit_requested));
        });
    }

    fn initialize_gc_stack_bottom(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if let Some(stack_end) = crate::tagged::gc::read_stack_end_from_proc() {
                self.tagged_heap.set_stack_bottom(stack_end as *const u8);
            }
        }
    }

    fn finish_runtime_activation(&mut self, sync_keyboard: bool) {
        self.setup_thread_locals();
        self.refresh_gc_runtime_settings_cache();
        self.sync_gc_threshold_from_runtime_settings();
        if sync_keyboard {
            self.sync_keyboard_runtime_from_obarray();
        }
        self.sync_thread_runtime_bindings();
        self.sync_current_thread_buffer_state();
        // Every name GNU's C declares with `DEFVAR_LISP' or `DEFVAR_KBOARD'
        // gets GNU's redirect tag here, at the last point before the evaluator
        // is live: the same boundary GNU's `main' crosses when the last
        // `syms_of_*'/`init_*' returns, reached from the other side.  GNU
        // declares first and assigns after; this port assigns from several
        // hundred scattered sites -- including `runtime_identity::install' and
        // `sync_thread_runtime_bindings' just above, which is why this cannot
        // sit with the `register_bootstrap_vars' calls -- and declares once,
        // here.  Idempotent, so the pdump-restored path (whose image already
        // carries the descriptors) finds every row settled and the six names
        // an image cannot carry get theirs.  See `defvar_object' for what the
        // tag buys and why the store rule -- the thing `DEFVAR_BOOL' and
        // `DEFVAR_INT' are declared for -- is not it.
        super::defvar_object::adopt(&mut self.obarray);
    }

    pub(crate) fn sync_current_thread_buffer_state(&mut self) {
        let current_thread_id = self.threads.current_thread_id();
        let current_buffer_id = self.buffers.current_buffer_id();
        self.threads
            .set_thread_current_buffer(current_thread_id, current_buffer_id);
    }

    fn sync_current_buffer_runtime_state(&mut self) -> Result<(), Flow> {
        self.sync_current_thread_buffer_state();
        super::casetab::sync_current_buffer_case_table_state(self)?;
        super::syntax::sync_current_buffer_syntax_table_state(self)?;
        Ok(())
    }

    pub(crate) fn switch_current_buffer(
        &mut self,
        id: crate::buffer::BufferId,
    ) -> Result<(), Flow> {
        if !self.buffers.switch_current(id) {
            return Err(signal(
                "error",
                vec![Value::string("Selecting deleted buffer")],
            ));
        }
        self.sync_current_buffer_runtime_state()
    }

    pub(crate) fn set_current_buffer_unrecorded(
        &mut self,
        id: crate::buffer::BufferId,
    ) -> Result<(), Flow> {
        if !self.buffers.switch_current_unrecorded(id) {
            return Err(signal(
                "error",
                vec![Value::string("Selecting deleted buffer")],
            ));
        }
        self.sync_current_buffer_runtime_state()
    }

    /// GNU `set_buffer_if_live` for an unwind: re-select `id` unless it was
    /// killed meanwhile.
    ///
    /// Mirrors `set_buffer_internal_1`'s first line, `if (current_buffer ==
    /// b) return;`: the overwhelmingly common unwind (every
    /// `save-current-buffer` body that never switched, every
    /// `put-text-property` on the current buffer) restores the buffer that
    /// is still current.  Its runtime state was synced when it became
    /// current, and the sync below is a thread-slot write plus two
    /// seed-if-nil table reads, so redoing it bought nothing — at ~1,500
    /// instructions a call it was 7% of a whole-buffer Org fontify (44,265
    /// restores per five fontifies, all of them same-buffer).
    pub fn restore_current_buffer_if_live(&mut self, id: crate::buffer::BufferId) {
        if self.buffers.current_buffer_id() == Some(id) {
            return;
        }
        if self.buffers.get(id).is_none() {
            return;
        }
        let _ = self.buffers.switch_current_unrecorded(id);
        let _ = self.sync_current_buffer_runtime_state();
    }

    /// Connect the input system for interactive mode.
    ///
    /// This mirrors GNU Emacs's `init_keyboard()` — it connects the evaluator
    /// to the render thread's input channel so that `read_char()` can block
    /// waiting for user input instead of returning immediately (batch mode).
    ///
    /// # Arguments
    /// * `input_rx` — Receiver end of the crossbeam channel from the render thread
    pub fn init_input_system(
        &mut self,
        input_rx: crossbeam_channel::Receiver<crate::keyboard::InputEvent>,
    ) {
        self.input_rx = Some(input_rx);
        self.command_loop.running = true;
    }

    /// Install the receiver for cross-thread [`EvalThreadTask`]s (e.g. from the
    /// diagnostics server). The sender side wakes the Lisp thread via
    /// [`Context::wait_notifier`]; queued tasks run at the next safe point.
    pub fn init_eval_task_system(&mut self, rx: crossbeam_channel::Receiver<EvalThreadTask>) {
        self.eval_task_rx = Some(rx);
    }

    /// Run any queued cross-thread tasks synchronously. Called at a Lisp-safe
    /// point (the `read_char` loop); a no-op when no channel is installed.
    pub(crate) fn drain_eval_tasks(&mut self) {
        // Clone the Receiver handle so we don't borrow `self.eval_task_rx`
        // across the `&mut self` task call.
        if let Some(rx) = self.eval_task_rx.clone() {
            while let Ok(task) = rx.try_recv() {
                task(self);
            }
        }
    }

    /// Cross-platform handle producers use to wake the wait loop after
    /// publishing work (see [`WaitNotifier`]). Returns `None` only if the
    /// platform poller could not be created. Frontend input, diagnostics, and
    /// asynchronous process work share this mechanism.
    pub fn wait_notifier(&self) -> Option<crate::emacs_core::process::WaitNotifier> {
        self.processes.wait_notifier()
    }

    pub fn set_display_host(&mut self, mut host: Box<dyn DisplayHost>) {
        let _ = host.set_visual_config(self.visual_config.clone());
        self.display_host = Some(host);
    }

    pub fn set_tty_frame_host_factory(&mut self, factory: Box<dyn TtyFrameHostFactory>) {
        self.tty_frame_host_factory = Some(factory);
    }

    // -----------------------------------------------------------------------
    // Command loop (mirrors keyboard.c)
    // -----------------------------------------------------------------------

    /// Enter a recursive edit level.
    ///
    /// Mirrors GNU Emacs `Frecursive_edit()` (keyboard.c:772).
    /// Increments recursive depth, enters the command loop, decrements on exit.
    /// If the command loop exits via `abort-recursive-edit` (throw 'exit t),
    /// signals quit.  If via `exit-recursive-edit` (throw 'exit nil), returns
    /// normally.
    ///
    /// In batch mode (no input_rx), returns nil immediately.
    /// Enter a recursive edit level (public API).
    ///
    /// Returns `Ok(())` on normal exit, `Err(description)` on error.
    #[tracing::instrument(skip_all)]
    pub fn recursive_edit(&mut self) -> Result<(), String> {
        match self.recursive_edit_inner() {
            Ok(_) => Ok(()),
            // kill-emacs unwinds the recursive edit; the pending shutdown
            // request carries the exit code to the caller.
            Err(Flow::Shutdown(_)) => Ok(()),
            Err(flow) => Err(super::error::format_flow_with_eval(self, &flow)),
        }
    }

    pub(crate) fn request_shutdown(&mut self, exit_code: i32, restart: bool) {
        self.shutdown_request = Some(ShutdownRequest { exit_code, restart });
        self.command_loop.running = false;
    }

    pub fn shutdown_request(&self) -> Option<ShutdownRequest> {
        self.shutdown_request
    }

    /// GNU `Fkill_emacs`'s `attributes: noreturn`, asked as a question.
    ///
    /// The recorded [`ShutdownRequest`] is the authority, not the propagating
    /// `Flow::Shutdown`: `module_handle_nonlocal_exit` (`dynamic_module.rs`)
    /// hands a module a signal named `kill-emacs`, and a module that clears it
    /// still exits, because the request is what the evaluator acts on.  See
    /// [`LispExecution`].
    pub(crate) fn lisp_execution(&self) -> LispExecution {
        match self.shutdown_request {
            None => LispExecution::Live,
            Some(_) => LispExecution::ExitedAlready,
        }
    }

    #[tracing::instrument(skip_all, fields(depth = self.command_loop.recursive_depth, has_input = self.input_rx.is_some()))]
    pub(crate) fn recursive_edit_inner(&mut self) -> EvalResult {
        self.run_exit_wrapped_command_loop(true)
    }

    #[tracing::instrument(skip_all, fields(depth = self.command_loop.recursive_depth, has_input = self.input_rx.is_some()))]
    pub(crate) fn minibuffer_command_loop_inner(&mut self) -> EvalResult {
        self.run_exit_wrapped_command_loop(false)
    }

    /// Classify the value carried by a `(throw 'exit VALUE)` that unwound a
    /// recursive command loop.
    ///
    /// Mirrors GNU `recursive_edit_1` (keyboard.c:749-758), which dispatches on
    /// the thrown value's *type* rather than its truthiness.
    fn classify_command_loop_exit(&mut self, value: Value) -> Result<CommandLoopExit, Flow> {
        if value == Value::T {
            return Ok(CommandLoopExit::Quit);
        }
        if value.is_string() {
            return Ok(CommandLoopExit::Error(value));
        }
        if super::builtins::types::builtin_functionp_1(self, value)?.is_truthy() {
            return Ok(CommandLoopExit::Call(value));
        }
        Ok(CommandLoopExit::Normal)
    }

    fn run_exit_wrapped_command_loop(&mut self, increment_depth: bool) -> EvalResult {
        // Interactive command loops need an input source. Batch mode is
        // different: GNU still runs `top-level`/`normal-top-level` and lets
        // `read_char` terminate the loop via noninteractive EOF, even when
        // there is no input channel at all.
        if self.input_rx.is_none() && !self.command_loop_noninteractive() {
            tracing::info!("recursive_edit_inner: no input receiver, returning immediately");
            return Ok(Value::NIL);
        }

        // Recursive edits and minibuffer readers enter the command loop even
        // when the outer loop has not been started through init_input_system().
        // GNU's recursive/minibuffer entry points do not consult an external
        // "running" gate before dispatching the first key. Preserve the
        // previous flag so explicit shutdown still unwinds correctly.
        let saved_running = self.command_loop.running;
        if !saved_running {
            self.command_loop.running = true;
        }

        if increment_depth {
            self.command_loop.recursive_depth += 1;
        }

        // GNU `recursive_edit_1` owns these bindings around the entire
        // `command_loop`, outside `command_loop_2`'s error/restart boundary.
        // Keeping them here also leaves execute-kbd-macro free to borrow its
        // caller's dynamic environment, as GNU macros.c does.
        let specpdl_count = self.specpdl.len();
        let result = (|| -> EvalResult {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-redisplay"), Value::NIL)?;
            self.try_specbind_or_unwind_to(
                specpdl_count,
                intern("undo-auto--undoably-changed-buffers"),
                Value::NIL,
            )?;

            // GNU `command_loop` installs its `exit` catch only for a recursive
            // command loop or an active minibuffer (`command_loop_level > 0 ||
            // minibuf_level > 0`).  The outermost loop must leave `exit`
            // unmatched, so `(throw 'exit ...)` there signals `no-catch`.
            let catches_exit =
                self.recursive_command_loop_depth() > 0 || self.minibuffers.depth() > 0;
            if catches_exit {
                self.push_condition_frame(ConditionFrame::Catch {
                    tag: Value::symbol("exit"),
                    resume: ResumeTarget::CommandLoopExit,
                });
            }

            let result = self.command_loop_inner();

            if catches_exit {
                self.pop_condition_frame();
            }

            match result {
                Ok(val) => Ok(val),
                // exit-recursive-edit: throw 'exit nil → normal return
                Err(Flow::Throw(ref thrown))
                    if catches_exit && thrown.tag.is_symbol_named("exit") =>
                {
                    let value = thrown.value;
                    match self.classify_command_loop_exit(value)? {
                        // abort-recursive-edit: throw 'exit t → signal quit
                        CommandLoopExit::Quit => {
                            Err(super::error::signal(LispCondition::Quit, vec![]))
                        }
                        // read_minibuf's cross-window abort (minibuf.c:646).
                        CommandLoopExit::Error(message) => {
                            Err(super::error::signal(LispCondition::Error, vec![message]))
                        }
                        // minibuffer-quit-recursive-edit throws a thunk that
                        // signals `minibuffer-quit`; GNU calls it here.
                        CommandLoopExit::Call(function) => {
                            self.apply(function, vec![])?;
                            Ok(Value::NIL)
                        }
                        CommandLoopExit::Normal => Ok(Value::NIL),
                    }
                }
                Err(flow) => Err(flow),
            }
        })();
        if increment_depth {
            self.command_loop.recursive_depth -= 1;
        }
        if !saved_running {
            self.command_loop.running = false;
        }

        self.unbind_to_with_result(specpdl_count, result)
    }

    /// Inner command loop; only the outermost loop catches `top-level`.
    ///
    /// Mirrors GNU Emacs `command_loop()` (keyboard.c:1104).
    /// The outermost invocation wraps command_loop_2 in a catch for
    /// 'top-level.
    #[tracing::instrument(skip_all)]
    fn command_loop_inner(&mut self) -> EvalResult {
        let outermost_command_loop =
            self.command_loop.recursive_depth == 1 && self.minibuffers.depth() == 0;
        loop {
            if outermost_command_loop {
                // Catch 'top-level throws (from (top-level) function).
                let top_level_tag = Value::symbol("top-level");
                self.push_condition_frame(ConditionFrame::Catch {
                    tag: top_level_tag,
                    resume: ResumeTarget::CommandLoopTopLevel,
                });
            }

            // GNU keyboard.c command_loop():
            //   internal_catch (Qtop_level, top_level_1, Qnil);
            //   internal_catch (Qtop_level, command_loop_2, Qerror);
            // Both top_level_1 and command_loop_2 run unconditionally per
            // outer loop iteration. The catch around top_level_1 turns any
            // 'top-level throw into a normal return so the next line — the
            // command_loop_2 catch — still runs. The previous NeoMacs
            // implementation gated command_loop_2 on
            // `self.command_loop.running`, which incorrectly skipped the
            // interactive loop entirely whenever (normal-top-level) raised
            // an error caught inside command_loop_top_level_1: the GUI
            // would create its window, hit the error, return Ok(NIL), and
            // immediately exit before the first redisplay. Match GNU and
            // always run command_loop_2 after top_level_1.
            let result = if outermost_command_loop {
                match self.command_loop_top_level_1() {
                    Ok(_) => self.command_loop_2(),
                    Err(Flow::Throw(ref thrown)) if thrown.tag.is_symbol_named("top-level") => {
                        // top-level throw inside top_level_1 — fall through
                        // to command_loop_2 just like GNU's two-catch flow.
                        self.command_loop_2()
                    }
                    Err(flow) => Err(flow),
                }
            } else {
                self.command_loop_2()
            };

            if outermost_command_loop {
                self.pop_condition_frame();
            }

            match result {
                // top-level throw → restart the loop
                Err(Flow::Throw(ref thrown))
                    if outermost_command_loop && thrown.tag.is_symbol_named("top-level") =>
                {
                    tracing::debug!("command_loop_inner: top-level throw, restarting loop");
                    continue;
                }
                Ok(value) if outermost_command_loop && self.command_loop_noninteractive() => {
                    // GNU keyboard.c:1145 — end of file in batch run
                    tracing::info!("command_loop_inner: noninteractive EOF, calling kill-emacs");
                    match super::builtins::symbols::builtin_kill_emacs(self, vec![Value::T]) {
                        Err(Flow::Shutdown(_)) | Ok(_) => {}
                        Err(flow) => return Err(flow),
                    }
                    return Ok(value);
                }
                // Any other result propagates up
                other => {
                    tracing::debug!(
                        "command_loop_inner: result={:?}, propagating",
                        other.is_ok()
                    );
                    return other;
                }
            }
        }
    }

    fn command_loop_noninteractive(&self) -> bool {
        self.noninteractive
    }

    fn command_loop_top_level_1(&mut self) -> EvalResult {
        let top_level = self
            .obarray
            .symbol_value("top-level")
            .copied()
            .unwrap_or(Value::NIL);

        tracing::debug!("command_loop_top_level_1: top-level={}", top_level);

        if top_level.is_nil() {
            tracing::debug!("command_loop_top_level_1: top-level is nil, skipping");
            self.log_startup_state("top-level-nil");
            return Ok(Value::NIL);
        }

        tracing::debug!("command_loop_top_level_1: evaluating top-level form");
        self.log_startup_state("top-level-before");
        match self.eval_value(&top_level) {
            Ok(_) => {
                tracing::debug!("command_loop_top_level_1: top-level completed OK");
                self.log_startup_state("top-level-after");
                Ok(Value::NIL)
            }
            Err(Flow::Signal(sig)) => {
                let rendered = super::error::format_signal_data_with_eval(self, &sig);
                tracing::warn!("command_loop_top_level_1: top-level SIGNALED: {}", rendered);
                let error_msg = self.command_error_message(&sig);
                let data = self.signal_error_data_value(&sig);
                self.report_command_error(data, "")?;
                if cfg!(test) {
                    let last_phase = self
                        .obarray
                        .symbol_value("neomacs--startup-last-phase")
                        .copied()
                        .map(|value| crate::emacs_core::print_value_with_eval(self, &value))
                        .unwrap_or_else(|| "nil".to_string());
                    let last_call = self
                        .obarray
                        .symbol_value("neomacs--startup-last-call")
                        .copied()
                        .map(|value| crate::emacs_core::print_value_with_eval(self, &value))
                        .unwrap_or_else(|| "nil".to_string());
                    eprintln!(
                        "top-level startup signal: {} last-phase={} last-call={}",
                        error_msg, last_phase, last_call
                    );
                }
                self.log_startup_state("top-level-signal");
                tracing::warn!("Top-level startup error: {}", error_msg);
                if self.command_loop_noninteractive() {
                    // GNU keyboard.c:cmd_error treats noninteractive
                    // startup/eval errors as fatal: it prints the error and
                    // calls (kill-emacs -1), which exits with status 255.
                    self.request_shutdown(-1, false);
                    return Err(Flow::Shutdown(ShutdownRequest {
                        exit_code: -1,
                        restart: false,
                    }));
                }
                Ok(Value::NIL)
            }
            Err(flow) => Err(flow),
        }
    }

    fn trace_startup_state_enabled(&self) -> bool {
        std::env::var("NEOMACS_TRACE_STARTUP_STATE")
            .ok()
            .is_some_and(|value| value == "1")
    }

    fn log_startup_state(&self, phase: &str) {
        if !self.trace_startup_state_enabled() {
            return;
        }

        let current_buffer = self
            .buffers
            .current_buffer()
            .map(|buffer| buffer.name_runtime_string_owned())
            .unwrap_or_else(|| "<none>".to_string());
        let selected_frame = self.frames.selected_frame().map(|frame| {
            let selected_window_buffer = frame
                .selected_window()
                .and_then(|window| window.buffer_id())
                .and_then(|buffer_id| self.buffers.get(buffer_id))
                .map(|buffer| buffer.name_runtime_string_owned())
                .unwrap_or_else(|| "<missing>".to_string());
            format!(
                "id=0x{:x} size={}x{} selected-window=0x{:x} selected-window-buffer={}",
                frame.id.0,
                frame.width,
                frame.height,
                frame.selected_window.0,
                selected_window_buffer
            )
        });
        let frames = self
            .frames
            .frame_list()
            .into_iter()
            .map(|fid| format!("0x{:x}", fid.0))
            .collect::<Vec<_>>();

        tracing::info!(
            "startup-state phase={} command-line-args={} command-line-args-left={} command-line-processed={} window-system={} initial-window-system={} current-buffer={} selected-frame={:?} frames={:?}",
            phase,
            format_startup_value(self.obarray.symbol_value("command-line-args")),
            format_startup_value(self.obarray.symbol_value("command-line-args-left")),
            format_startup_value(self.obarray.symbol_value("command-line-processed")),
            format_startup_value(self.obarray.symbol_value("window-system")),
            format_startup_value(self.obarray.symbol_value("initial-window-system")),
            current_buffer,
            selected_frame,
            frames
        );
    }

    /// Command loop with error recovery.
    ///
    /// Mirrors GNU Emacs `command_loop_2()` (keyboard.c:1146).
    /// Wraps command_loop_1 with condition-case error handling.
    #[tracing::instrument(skip_all)]
    fn command_loop_2(&mut self) -> EvalResult {
        loop {
            match self.command_loop_1() {
                Ok(val) => return Ok(val),
                Err(flow @ Flow::Throw(_)) => {
                    // Throws propagate (exit, top-level, etc.) without
                    // re-entering the command loop.  Re-running command_loop_1
                    // here traps minibuffer exit throws and blocks waiting for
                    // another key instead of unwinding like GNU Emacs.
                    return Err(flow);
                }
                // A shutdown unwinds the command loop instead of restarting it:
                // GNU never returns from Fkill_emacs to command_loop_2.
                Err(flow @ (Flow::ThreadBlocked(_) | Flow::Shutdown(_))) => return Err(flow),
                Err(flow @ Flow::Signal(_))
                    if self
                        .command_loop
                        .keyboard
                        .kboard
                        .executing_kbd_macro
                        .is_some() =>
                {
                    return Err(flow);
                }
                Err(Flow::Signal(sig)) => {
                    // GNU `command_loop_2' is the sole recovery owner:
                    // `internal_condition_case (command_loop_1, ..., cmd_error)'.
                    // Keeping reporting here ensures the current buffer's
                    // buffer-local `command-error-function' decides how the
                    // error is presented (notably `minibuffer-error-function').
                    let sym_name = format_symbol_name_for_diagnostic(sig.symbol);
                    let error_msg = self.command_error_message(&sig);
                    // Render the *condition symbol* and full signal payload, not
                    // just the human message: a bare "peculiar error" (an error
                    // whose condition has no `error-message`) is otherwise
                    // undiagnosable in a bug report. `condition=` names the
                    // symbol; `signal=` is the Lisp-readable `(SYMBOL . DATA)`.
                    let rendered_signal = super::error::format_signal_data_with_eval(self, &sig);
                    // Backtrace captured at signal-dispatch time (debug tracing
                    // only); shows where it was raised without `debug-on-error`.
                    let backtrace_suffix = self
                        .last_uncaught_signal_backtrace
                        .take()
                        .map(|bt| format!("\nLisp backtrace (innermost first):\n{bt}"))
                        .unwrap_or_default();
                    // GNU `cmd_error' clears both prefix arguments and key
                    // echoing before calling `cmd_error_internal'.
                    self.assign("prefix-arg", Value::NIL);
                    self.assign("last-prefix-arg", Value::NIL);
                    self.cancel_key_echo_state();

                    let data = self.signal_error_data_value(&sig);
                    self.report_command_error(data, "")?;

                    tracing::error!(
                        condition = %sym_name,
                        "Command loop error: {error_msg} [signal={rendered_signal}]{backtrace_suffix}"
                    );

                    // Restart the command loop.
                    continue;
                }
            }
        }
    }

    /// Render a signal for diagnostics without choosing a presentation path.
    /// Presentation belongs exclusively to `report_command_error', which
    /// dispatches through Lisp's buffer-local `command-error-function'.
    fn command_error_message(&mut self, sig: &SignalData) -> String {
        let error_data = make_signal_binding_value(sig);
        crate::emacs_core::errors::builtin_error_message_string(self, vec![error_data])
            .ok()
            .and_then(|value| {
                value
                    .as_lisp_string()
                    .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            })
            .unwrap_or_else(|| format_symbol_name_for_diagnostic(sig.symbol))
    }

    /// Main command loop — read key sequence, look up binding, execute.
    ///
    /// Mirrors GNU Emacs `command_loop_1()` (keyboard.c:1306).
    /// This is the core interactive loop: read → dispatch → redisplay.
    #[tracing::instrument(skip_all)]
    fn command_loop_1(&mut self) -> EvalResult {
        if !self.command_loop.running {
            return Ok(Value::NIL);
        }

        self.command_loop_1_entry_prologue()?;

        loop {
            if !self.command_loop.running {
                return Ok(Value::NIL);
            }

            self.flush_pending_safe_funcalls();
            self.sync_current_buffer_to_selected_window();

            // Save the outgoing `current-prefix-arg` into
            // `last-prefix-arg` before reading the next command.
            //
            // Do NOT also transfer `prefix-arg` here: GNU's Lisp
            // `command-execute` does that itself right before it
            // calls `call-interactively`, and prefix commands such as
            // `universal-argument` rely on `prefix-arg` surviving
            // until that point.
            let outgoing_prefix_arg = self.eval_symbol("current-prefix-arg").unwrap_or(Value::NIL);
            self.assign("last-prefix-arg", outgoing_prefix_arg);

            // Reset this-command and related variables before reading
            // the next key sequence.  GNU keyboard.c:1416-1419 clears
            // Vthis_command, Vreal_this_command, Vthis_original_command,
            // and Vthis_command_keys_shift_translated to nil so that idle
            // timer callbacks (e.g. which-key) running inside
            // read_key_sequence observe (null this-command) => t.
            self.assign("this-command", Value::NIL);
            self.assign("real-this-command", Value::NIL);
            self.assign("this-original-command", Value::NIL);

            // Read a complete key sequence (may be multi-key, e.g. C-x C-f).
            //
            // Bind `inhibit-quit` to t around the command-loop read, the way
            // GNU `command_loop_1` keeps C-g out of the quit machinery while
            // reading the next key (keyboard.c binds Qinhibit_quit around the
            // input wait, and `read_char` clears `Vquit_flag` when the
            // quit_char is returned as a key, keyboard.c:2811-2812). Without
            // this, neomacs's per-iteration `maybe_quit` in the wait loop
            // (process/wait.rs) would observe the cross-thread `quit_requested`
            // atomic an idle C-g raises and signal `quit` DIRECTLY — bypassing
            // the `keyboard-quit` command the C-g is bound to (so advice and
            // remaps never run) and leaving the C-g KeyPress queued for a
            // second quit. With `inhibit-quit` bound, `maybe_quit` returns Ok,
            // the C-g flows through as an ordinary key, and
            // `read_key_sequence` returns it bound to `keyboard-quit`.
            //
            // This binding is scoped strictly to the command-loop read.
            // `sleep-for` / `accept-process-output` run as commands (outside
            // this binding) and bind no `inhibit-quit`, so their waits stay
            // interruptible by C-g — the sleep-for quit fix is preserved.
            let read_specpdl_count = self.specpdl.len();
            self.try_specbind_or_unwind_to(read_specpdl_count, intern("inhibit-quit"), Value::T)?;
            let read_result = self.read_command_key_sequence_with_options(
                crate::keyboard::ReadKeySequenceOptions::new(Value::NIL, false, false, true),
            );

            // The read result itself is not an EvalResult, but it can carry
            // Lisp Values just like one. Keep those values in the VM root
            // window while an `inhibit-quit` unlet watcher runs arbitrary
            // Lisp during cleanup. Error payloads travel through the ordinary
            // result-carrying unwinder.
            let read_root_scope = self.save_vm_roots();
            if let Ok(crate::keyboard::CommandKeySequenceRead::Command { keys, binding }) =
                &read_result
            {
                for key in keys.iter().copied() {
                    self.push_vm_frame_root(key);
                }
                self.push_vm_frame_root(*binding);
            }
            let unwind_result = match read_result.as_ref() {
                Ok(_) => self.unbind_to_with_result(read_specpdl_count, Ok(Value::NIL)),
                Err(flow) => self.unbind_to_with_result(read_specpdl_count, Err(flow.clone())),
            };
            self.restore_vm_roots(read_root_scope);
            unwind_result?;

            let (keys, binding, input_end) = match read_result? {
                crate::keyboard::CommandKeySequenceRead::Command { keys, binding } => {
                    (keys, binding, None)
                }
                crate::keyboard::CommandKeySequenceRead::End(end) => {
                    (Vec::new(), Value::NIL, Some(end))
                }
            };

            // Reconcile a quit that became pending DURING the command-loop read.
            //
            // The input bridge raises the cross-thread `quit_requested` atomic
            // EAGERLY the instant it sees a C-g in the byte stream — even while
            // earlier keystrokes are still queued AHEAD of that C-g on the
            // ordered input channel (crates/neomacs/src/main.rs:2260; the atomic
            // is set ONLY for the quit char). With `inhibit-quit` bound around
            // the read above, a `maybe_quit` during the wait drains that eager
            // atomic into `quit-flag` while an EARLIER key is being read, so on
            // return `quit-flag` can be set even though the C-g itself has not
            // been read yet.
            //
            // GNU has no such eager cross-thread atomic: its `Vquit_flag` comes
            // from the SIGINT handler and the quit_char arrives in-stream;
            // `read_char` clears `Vquit_flag` exactly when it returns the
            // quit_char as a key under `inhibit-quit` (keyboard.c:2810-2811),
            // and the residual-quit -> `unread-command-events = (quit_char)`
            // conversion runs ONLY where the input WAIT returned no key (after
            // `sit_for` showing a minibuffer message, keyboard.c:1409-1416) —
            // never after an ordinary key read.
            //
            // We mirror that, accounting for the eager atomic:
            //
            //  * If a quit is pending, CLEAR `quit-flag` and the atomic. The
            //    pending quit corresponds to a C-g; either it was just returned
            //    as the read key (lone C-g -> `keys` is the C-g, bound to
            //    `keyboard-quit`, run below), or it is still queued IN-STREAM
            //    behind keys the bridge sent ahead of it and will be read as an
            //    ordinary key on a later iteration. Leaving `quit-flag` set
            //    would fire a spurious quit at the next `maybe_quit` (e.g. mid
            //    self-insert), aborting a minibuffer read partway and leaking
            //    the remaining keys into the buffer (the `megaalpha` bug).
            //
            //  * Re-deliver the C-g via `unread-command-events` ONLY when the
            //    read returned NO key — the genuine GNU case where a quit
            //    interrupted a wait with nothing else queued, so the C-g must
            //    become the next key exactly once. When a real key was read the
            //    in-stream C-g is still coming, so injecting a quit_char here
            //    would deliver the quit OUT OF ORDER, ahead of the queued keys.
            //
            // This keeps the single-idle-C-g fix intact: a lone C-g is read as
            // a key bound to `keyboard-quit` (run below, exactly once) and the
            // flag/atomic are cleared here; `sleep-for`/`accept-process-output`
            // run as commands outside the read's `inhibit-quit` binding and
            // stay C-g-interruptible.
            //
            // `while-no-input` is left untouched: when `quit-flag` equals
            // `throw-on-input` the pending value is while-no-input's bail-out
            // sentinel (NOT an eager C-g), so clearing it would defeat
            // while-no-input — mirror the same guard used by
            // `clear_quit_flag_after_read_key_sequence_event`.
            let throw_on_input = self
                .obarray
                .symbol_value_id_or_nil(self.throw_on_input_symbol);
            let quit_flag = self.quit_flag_value();
            let is_while_no_input =
                !throw_on_input.is_nil() && equal_value(&quit_flag, &throw_on_input, 0);
            let quit_pending = !quit_flag.is_nil()
                || self
                    .quit_requested
                    .load(std::sync::atomic::Ordering::Relaxed);
            if quit_pending && !is_while_no_input {
                self.set_quit_flag_value(Value::NIL);
                self.quit_requested
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                if keys.is_empty() {
                    let quit_char = Value::fixnum(self.quit_char());
                    self.push_unread_command_event(quit_char);
                }
            }

            self.sync_current_buffer_to_selected_window();

            if input_end.is_some() {
                self.assign("this-command", Value::NIL);
                return Ok(Value::NIL);
            }

            // Start only after the complete key sequence has arrived.  This
            // excludes the user's inter-key think time while retaining the
            // command-loop work below, including command remapping.
            let command_observation_start = UserCommandObservationStart::capture_if_enabled();

            // A non-empty key sequence with a nil binding is a truly-unbound
            // key. GNU `command_loop_1` does NOT short-circuit this case: it
            // sets `Vthis_command = cmd` (= nil) at keyboard.c:1506, runs
            // `pre-command-hook` (1509), then `if (NILP (Vthis_command))
            // call0 (Qundefined);` (1512-1514) — the `undefined` command in
            // subr.el dings and echoes "<key> is undefined" — and finally
            // runs `post-command-hook` (1563) plus the deactivate-mark /
            // recent-keys bookkeeping like any other command. Routing the
            // nil-binding case through the SAME finalize tail below (rather
            // than a bare `continue`) restores those per-command hooks and
            // the user-visible "is undefined" feedback. Keyboard/command-loop
            // audit Finding 1. (The `undefined` command itself sets
            // `prefix-arg`, so we no longer reset it here.)
            if binding.is_nil() {
                let desc: Vec<String> = keys.iter().map(|v| format!("{:?}", v)).collect();
                tracing::info!("Undefined key sequence: {}", desc.join(" "));
            }

            // The unmapped command (real-this-command) is the binding
            // we read from the keymap, before any remapping is applied.
            // Do not touch `real-last-command` here: GNU updates it only after
            // the preceding command's `post-command-hook` and leaves that
            // value visible to the next `pre-command-hook` (keyboard.c's
            // `kset_real_last_command` near the command-loop finalize tail).
            self.assign("real-this-command", binding);

            // Apply command remapping per GNU
            // `keyboard.c:1340-1343`. The remapped command becomes
            // this-command for execution. Finding 4.
            let remapped = self.command_remapping_for_loop(binding);
            self.assign("this-command", remapped);
            let mut command_observation = command_observation_start.map(|start| {
                UserCommandObservation::begin(
                    start,
                    UserCommandIdentity::new(
                        self.context_instance_id(),
                        self.command_loop.recursive_depth,
                        Self::command_keys_for_log(&keys),
                        Self::command_value_for_log(binding),
                        Self::command_value_for_log(remapped),
                        self.frames.selected_frame().map(|frame| frame.id),
                        self.buffers.current_buffer_id(),
                    ),
                )
            });

            // Finding 2: this-original-command stays at the original
            // (pre-remap) command for the duration of the iteration
            // unless a pre-command-hook explicitly cleared it.
            if self
                .eval_symbol("this-original-command")
                .unwrap_or(Value::NIL)
                .is_nil()
            {
                self.assign("this-original-command", binding);
            }

            if let Some(last) = keys.last() {
                self.assign("last-command-event", *last);
            }
            tracing::debug!(
                "command_loop_1: binding={} current_buffer={:?} active_minibuffer_window={:?}",
                self.this_command_name_for_log(),
                self.buffers.current_buffer_id(),
                self.active_minibuffer_window
            );

            // GNU `command_loop_1` resets `Vdeactivate_mark = Qnil` at the top
            // of each iteration (keyboard.c:1471), before `pre-command-hook`, so
            // the flag reflects only the command about to run; the post-command
            // block then deactivates the region iff a command (re)set it.
            // Without this per-command reset a stale buffer-local `deactivate-mark`
            // (left by an earlier buffer-modifying command such as self-insert)
            // leaks forward and immediately kills a freshly `set-mark`ed region,
            // so e.g. `C-SPC M-> M-;` sees no active region.
            self.assign("deactivate-mark", Value::NIL);

            // GNU `keyboard.c:1500-1506` records the command pseudo-event
            // before `pre-command-hook`, so `recent-keys 'include-cmds` can
            // describe the command currently being run.
            self.record_recent_command(remapped);

            // Run pre-command-hook via safe-run-hooks so a broken
            // hook function is removed instead of re-firing on every
            // command. Finding 7 — GNU `keyboard.c:1510`
            // (`safe_run_hooks_maybe_narrowed (Qpre_command_hook, ...)`).
            self.safe_run_hook_if_bound("pre-command-hook")?;

            // GNU `keyboard.c:1530-1534` adds undo boundaries here, after
            // `pre-command-hook` and before command execution, so the
            // previous command's edits are grouped before the next command
            // mutates any buffer state.
            if self.obarray.fboundp("undo-auto--add-boundary") {
                let _ = self.apply(Value::symbol("undo-auto--add-boundary"), vec![]);
            }
            if let Some(current_id) = self.buffers.current_buffer_id() {
                let _ = self.buffers.record_undo_point_before_command(current_id);
            }

            // GNU `keyboard.c:1477-1486` snapshots prev-buffer/modiff and
            // `last_point_position = PT` here, then resets
            // `disable-point-adjustment` to nil so a command must opt back in
            // to suppress the post-command point adjustment.
            let apfp_prev_buffer = self.buffers.current_buffer_id();
            let apfp_last_pt = apfp_prev_buffer.map(|id| self.apfp_point(id)).unwrap_or(0);
            let apfp_prev_modiff = apfp_prev_buffer
                .and_then(|id| self.buffers.get(id))
                .map(|b| b.modified_tick())
                .unwrap_or(0);
            self.assign("disable-point-adjustment", Value::NIL);

            // Execute the remapped command, matching GNU's
            // `calln (Qcommand_execute, Vthis_command)`.
            let command_execution_start = command_observation
                .as_ref()
                .map(UserCommandObservation::begin_execution);
            let exec_result = self.dispatch_command_in_loop(remapped);
            if let (Some(observation), Some(start)) =
                (command_observation.as_mut(), command_execution_start)
            {
                let outcome = match &exec_result {
                    Ok(_) => UserCommandOutcome::Completed,
                    Err(flow) => UserCommandOutcome::from_flow(flow),
                };
                observation.finish_execution(start, outcome);
            }

            // Keep the selected window's point and current buffer/runtime view
            // aligned before post-command work and redisplay observe state.
            self.sync_current_buffer_to_selected_window();

            // GNU does not recover inside `command_loop_1'. Any non-local
            // result unwinds the unfinished command (so post-command hooks and
            // history finalization do not run) and lets `command_loop_2' make
            // the exhaustive Flow decision. In particular, a Signal must not
            // be flattened into a plain echo-area `message' here.
            if exec_result.is_err() {
                return exec_result;
            }

            // Run post-command-hook via safe-run-hooks (Finding 7).
            // GNU `command_loop_1` calls `safe_run_hooks (Qpost_command_hook)`
            // at keyboard.c:1563.
            self.safe_run_hook_if_bound("post-command-hook")?;

            // GNU `command_loop_1` (src/keyboard.c:1342-1345): "If displaying a
            // message, resize the echo area window to fit that message's size
            // exactly." It calls `resize_echo_area_exactly` whenever
            // `echo_area_buffer[0]` is non-nil; that passes
            // `exact_p = (minibuf_level == 0 ? Qt : Qnil)` (xdisp.c:13235) so
            // with NO active minibuffer the grow-only echo window shrinks to
            // fit even a shorter NON-EMPTY message (xdisp.c:13401). We can't
            // resize the mini-window here (geometry is computed lazily in the
            // layout engine), so we record the request and the next redisplay's
            // layout pass consumes it. `minibuf_level == 0` maps to "no active
            // minibuffer window".
            self.echo_area_resize_exact_pending =
                self.current_message.is_some() && self.active_minibuffer_window_id().is_none();

            // GNU runs the deactivate-mark / select-active-regions block
            // strictly AFTER post-command-hook: keyboard.c:1597-1648, with
            // `call0 (Qdeactivate_mark)` at 1611. (The earlier
            // `Vdeactivate_mark = Qnil` at keyboard.c:1471/1490 is only the
            // pre-command RESET of the flag, not the deactivation.) So a
            // command that sets `deactivate-mark` must still observe an
            // active region from inside `post-command-hook`. Finding —
            // keyboard/command-loop audit.
            let _ = self.update_active_region_selection_after_command();

            // GNU `keyboard.c:1650-1671` finalize block: adjust point out of
            // invisible/intangible text after the command.  Gated like GNU on
            // same-buffer, the selected window showing that buffer, point
            // having actually moved, and neither disable var being set.
            {
                let cur_buffer = self.buffers.current_buffer_id();
                let win_buffer = self
                    .frames
                    .selected_frame()
                    .and_then(|f| f.selected_window())
                    .and_then(|w| w.buffer_id());
                let cur_pt = cur_buffer.map(|id| self.apfp_point(id)).unwrap_or(0);
                let disabled = self
                    .eval_symbol("disable-point-adjustment")
                    .unwrap_or(Value::NIL)
                    .is_truthy()
                    || self
                        .eval_symbol("global-disable-point-adjustment")
                        .unwrap_or(Value::NIL)
                        .is_truthy();
                if cur_buffer.is_some()
                    && cur_buffer == apfp_prev_buffer
                    && cur_buffer == win_buffer
                    && apfp_last_pt != cur_pt
                    && !disabled
                {
                    let modified = cur_buffer
                        .and_then(|id| self.buffers.get(id))
                        .map(|b| b.modified_tick())
                        .unwrap_or(apfp_prev_modiff)
                        != apfp_prev_modiff;
                    self.adjust_point_for_property(apfp_last_pt, modified)?;
                    // Re-align the selected window with the adjusted point.
                    self.sync_current_buffer_to_selected_window();
                }
            }

            // GNU updates the command-history variables after
            // post-command-hook (`keyboard.c`: kset_last_command and
            // kset_real_last_command near the bottom of command_loop_1).
            // Undo uses `last-command` to decide whether a following undo
            // continues the same undo chain or starts a redo.
            if let Ok(this_cmd) = self.eval_symbol("this-command") {
                self.assign("last-command", this_cmd);
            }
            let real_this = self.eval_symbol("real-this-command").unwrap_or(Value::NIL);
            self.assign("real-last-command", real_this);

            // GNU records the real command as last-repeatable-command for
            // ordinary key events.
            let last_event = self.eval_symbol("last-command-event").unwrap_or(Value::NIL);
            if !last_event.is_cons() {
                self.assign("last-repeatable-command", real_this);
            }

            // Reset this-original-command for the next iteration so
            // a fresh command starts the cycle clean (mirroring
            // GNU's clear at the bottom of command_loop_1).
            self.assign("this-original-command", Value::NIL);

            if exec_result.is_ok()
                && self.command_loop.keyboard.kboard.defining_kbd_macro
                && self
                    .eval_symbol("prefix-arg")
                    .unwrap_or(Value::NIL)
                    .is_nil()
            {
                self.finalize_kbd_macro_runtime_chars();
            }

            // GNU `command_loop_1` calls `cancel_echoing` at the command
            // boundary: the rendered key sequence remains visible, but the
            // next ordinary input no longer treats it as keyboard-owned and
            // cannot append another command's events to it.
            self.cancel_key_echo_state();

            // Keyboard audit Finding 9: auto-save-interval check.
            // GNU `keyboard.c:1491-1506`:
            //
            //   if (INTEGERP (Vauto_save_interval)
            //       && num_nonmacro_input_events - last_auto_save
            //          > max (XFIXNUM (Vauto_save_interval), 20)
            //       && !detect_input_pending_run_timers (0))
            //     {
            //       Fdo_auto_save (Qnil, Qnil);
            //       last_auto_save = num_nonmacro_input_events;
            //       ...
            //     }
            //
            // The lower floor of 20 prevents saving too often if
            // a user sets `auto-save-interval` to a tiny value.
            // The `detect_input_pending` gate defers the save
            // when the user is typing faster than the check
            // interval — we approximate that with a "no pending
            // events in the unread queue" probe.
            self.command_loop_1_maybe_auto_save();
            if let Some(observation) = command_observation.as_mut() {
                observation.complete_finalization();
            }
        }
    }

    /// One-time entry prologue for `command_loop_1`.
    ///
    /// GNU `keyboard.c:1313-1349` runs this before the first
    /// `read_key_sequence` after entering `command_loop_1`, not after the
    /// first command. Doom relies on that ordering: it sets
    /// `inhibit-redisplay` during startup and clears it from an initial
    /// `post-command-hook` before the first input wait/redisplay.
    fn command_loop_1_entry_prologue(&mut self) -> EvalResult {
        self.assign("prefix-arg", Value::NIL);
        self.assign("last-prefix-arg", Value::NIL);
        self.assign("deactivate-mark", Value::NIL);

        // GNU `command_loop_1` clears `this_command_key_count` and
        // `this_single_command_key_start` before its initial
        // `post-command-hook` (keyboard.c:1316-1327).  In a recursive
        // minibuffer command loop, the outer command's translated key
        // sequence is therefore hidden from that hook.  Keep the raw sequence:
        // GNU does not clear `raw_keybuf_count` until immediately before
        // `read_key_sequence` (keyboard.c:1416-1424).
        self.set_translated_command_keys(Vec::new());

        if self
            .eval_symbol("memory-full")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            self.safe_run_hook_if_bound("post-command-hook")?;

            if self
                .eval_symbol("delayed-warnings-list")
                .unwrap_or(Value::NIL)
                .is_truthy()
            {
                self.safe_run_hook_if_bound("delayed-warnings-hook")?;
            }
        }

        let this_command = self.eval_symbol("this-command").unwrap_or(Value::NIL);
        self.assign("last-command", this_command);

        let real_this_command = self.eval_symbol("real-this-command").unwrap_or(Value::NIL);
        self.assign("real-last-command", real_this_command);

        let last_command_event = self.eval_symbol("last-command-event").unwrap_or(Value::NIL);
        if !last_command_event.is_cons() {
            self.assign("last-repeatable-command", real_this_command);
        }

        Ok(Value::NIL)
    }

    /// Per-iteration `auto-save-interval` check, mirroring GNU
    /// `keyboard.c:1491-1506`. Keyboard audit Finding 9.
    fn command_loop_1_maybe_auto_save(&mut self) {
        let interval = match self.eval_symbol("auto-save-interval").ok() {
            Some(v) => match v.as_fixnum() {
                Some(n) if n > 0 => n,
                _ => return,
            },
            None => return,
        };
        let threshold = interval.max(20);
        let current = self.num_nonmacro_input_events();
        let last = self.command_loop.last_auto_save_input_events;
        if current.saturating_sub(last) <= threshold {
            return;
        }
        // Defer if input is pending (same spirit as GNU's
        // `detect_input_pending_run_timers (0)` gate). A fast
        // typist should not be interrupted by a save.
        if self.input_pending_for_auto_save() {
            return;
        }
        self.run_command_loop_auto_save("input interval");
    }

    /// Run GNU's command-input auto-save boundary for either the event-count
    /// or idle-time trigger. Both paths must pass `auto-save-no-message`, call
    /// the same `do-auto-save` primitive, and throttle a failing attempt so a
    /// broken hook cannot spin the command loop.
    pub(crate) fn run_command_loop_auto_save(&mut self, trigger: &'static str) {
        self.command_loop.last_auto_save_input_events = self.num_nonmacro_input_events();
        let no_message = if self
            .eval_symbol("auto-save-no-message")
            .unwrap_or(Value::NIL)
            .is_truthy()
        {
            Value::T
        } else {
            Value::NIL
        };
        if let Err(flow) = self.apply(Value::symbol("do-auto-save"), vec![no_message, Value::NIL]) {
            let rendered = super::error::format_flow_with_eval(self, &flow);
            tracing::warn!("auto-save from {trigger} failed: {rendered}");
        }
    }

    /// Approximation of GNU `detect_input_pending_run_timers (0)`
    /// used by the command-loop auto-save gate. Returns true when
    /// there is already-queued input that should run before an
    /// expensive auto-save.
    fn input_pending_for_auto_save(&mut self) -> bool {
        self.service_leading_internal_frontend_events();
        if self.peek_unread_command_event().is_some() {
            return true;
        }
        self.has_pending_frontend_input_with_configured_filter()
    }

    /// Apply `command-remapping` for the command-loop dispatch
    /// path. Mirrors GNU `keyboard.c:1340-1343` calling
    /// `Fcommand_remapping (cmd, Qnil, Qnil)` and substituting the
    /// result when non-nil. Keyboard audit Finding 4.
    fn command_remapping_for_loop(&mut self, command: Value) -> Value {
        if command.is_nil() {
            return command;
        }
        match self.apply(Value::symbol("command-remapping"), vec![command]) {
            Ok(remapped) if !remapped.is_nil() => remapped,
            _ => command,
        }
    }

    /// Dispatch the current `this-command` via GNU's
    /// `command-execute` command-loop path.
    fn dispatch_command_in_loop(&mut self, command: Value) -> EvalResult {
        // Re-resolve `this-command` from the obarray so a
        // pre-command-hook that mutated the symbol takes effect.
        let cmd = self.eval_symbol("this-command").unwrap_or(command);
        if cmd.is_nil() {
            // GNU `command_loop_1` keyboard.c:1512-1514:
            //   if (NILP (Vthis_command))
            //     /* nil means key is undefined.  */
            //     call0 (Qundefined);
            // The `undefined` command (subr.el) dings, echoes
            // "<key> is undefined", forces a mode-line update, and sets
            // `prefix-arg` for down-mouse events. Invoke it so an unbound
            // key gives the same feedback as GNU instead of silently doing
            // nothing. If `undefined` is not yet defined (minimal runtimes),
            // fall back to a bare ding so the key is still audible.
            if self.obarray.fboundp("undefined") {
                return self.apply(Value::symbol("undefined"), vec![]);
            }
            let _ = super::builtins::dispatch_builtin(self, "ding", vec![]);
            return Ok(Value::NIL);
        }
        self.apply(Value::symbol("command-execute"), vec![cmd])
    }

    /// Run a hook with `safe-run-hooks` semantics: each hook
    /// function is wrapped in a `condition-case` so a broken
    /// function is removed from the hook instead of re-firing on
    /// every subsequent command. Mirrors GNU
    /// `safe_run_hooks (Qhook_name)` at
    /// `src/keyboard.c:1361,1485` and `src/eval.c:2779-2830`.
    /// Keyboard audit Finding 7.
    pub(crate) fn safe_run_hook_if_bound(&mut self, hook_name: &str) -> EvalResult {
        // GNU `keyboard.c:1970-1978` (`safe_run_hooks`):
        //
        //   void safe_run_hooks (Lisp_Object hook) {
        //     specbind (Qinhibit_quit, Qt);
        //     run_hook_with_args (2, {hook, hook}, safe_run_hook_funcall);
        //     unbind_to (count, Qnil);
        //   }
        //
        // This is a C function — NOT the Lisp `safe-run-hooks` from
        // `subr.el`. It calls `run_hook_with_args` with a custom
        // funcall wrapper (`safe_run_hook_funcall`) that wraps each
        // hook function in `internal_condition_case_n` and removes
        // broken entries on error.
        //
        // neomacs mirrors this by calling
        // `hook_runtime::safe_run_named_hook` directly from Rust,
        // which resolves the hook value (including buffer-local
        // bindings + the `t` global marker), calls each hook
        // function, and swallows Signal errors. This never goes
        // through Lisp — matching GNU's keyboard.c which calls the
        // C function, not the Lisp wrapper.
        let hook_sym = super::intern::intern(hook_name);
        // `safe_run_hook_funcall` only swallows ordinary `error`
        // signals.  Nonlocal exits like `throw`/`quit` still escape
        // the command loop, and `read-char-from-minibuffer` relies on
        // that when its local `post-command-hook` calls
        // `exit-minibuffer`.
        let specpdl_count = self.specpdl.len();
        self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)?;
        let result = super::hook_runtime::safe_run_named_hook(self, hook_sym, &[]);
        self.unbind_to_with_result(specpdl_count, result)
    }

    pub(crate) fn execute_kbd_macro_iteration_via_command_loop(&mut self) -> EvalResult {
        let saved_running = self.command_loop.running;
        if !saved_running {
            self.command_loop.running = true;
        }
        self.assign("prefix-arg", Value::NIL);
        let result = self.command_loop_2();
        if !saved_running && self.command_loop.running {
            self.command_loop.running = false;
        }
        result
    }

    pub(crate) fn with_executing_kbd_macro_runtime<F>(
        &mut self,
        macro_events: Vec<Value>,
        run: F,
    ) -> EvalResult
    where
        F: FnOnce(&mut Self) -> EvalResult,
    {
        let scope = ExecutingKbdMacroRuntimeScope {
            snapshot: self.snapshot_executing_kbd_macro_runtime(),
            real_this_command: self.eval_symbol("real-this-command").unwrap_or(Value::NIL),
        };
        self.begin_executing_kbd_macro_runtime(macro_events);
        let result = run(self);
        let cleanup = self.finish_executing_kbd_macro_runtime_scope(scope);
        match cleanup {
            Ok(v) if v.is_nil() => result,
            Ok(other) => Ok(other),
            Err(flow) => Err(flow),
        }
    }

    pub(crate) fn reset_executing_kbd_macro_runtime_iteration(&mut self) {
        self.set_executing_kbd_macro_runtime_index(0);
    }

    fn finish_executing_kbd_macro_runtime_scope(
        &mut self,
        scope: ExecutingKbdMacroRuntimeScope,
    ) -> EvalResult {
        self.restore_executing_kbd_macro_runtime(scope.snapshot);
        self.assign("real-this-command", scope.real_this_command);
        self.run_hook_if_bound("kbd-macro-termination-hook")
    }

    /// Run a named hook if it is bound and non-nil.
    pub(crate) fn run_hook_if_bound(&mut self, hook_name: &str) -> EvalResult {
        match self.eval_symbol(hook_name) {
            Ok(hook_val) if !hook_val.is_nil() => {
                // (run-hooks 'HOOK)
                super::builtins::dispatch_builtin(self, "run-hooks", vec![Value::symbol(hook_name)])
                    .unwrap_or(Ok(Value::NIL))
            }
            _ => Ok(Value::NIL),
        }
    }

    pub(crate) fn queue_pending_safe_funcall(&mut self, function: Value, args: Vec<Value>) {
        self.pending_safe_funcalls.push(PendingSafeFuncall {
            function,
            args: args.into_iter().collect(),
        });
    }

    pub(crate) fn queue_pending_safe_hook(&mut self, hook_name: &str, args: &[Value]) {
        self.queue_pending_safe_funcall(
            Value::symbol("run-hook-with-args"),
            std::iter::once(Value::symbol(hook_name))
                .chain(args.iter().copied())
                .collect(),
        );
    }

    pub(crate) fn flush_pending_safe_funcalls(&mut self) {
        while let Some(funcall) = self.pending_safe_funcalls.pop() {
            let _ = self.apply(funcall.function, funcall.args);
        }
    }

    fn update_active_region_selection_after_command(&mut self) -> EvalResult {
        if self
            .eval_symbol("mark-active")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            return Ok(Value::NIL);
        }

        let transient_mark_mode = self
            .eval_symbol("transient-mark-mode")
            .unwrap_or(Value::NIL);
        if transient_mark_mode == Value::symbol("identity") {
            self.assign("transient-mark-mode", Value::NIL);
        } else if transient_mark_mode == Value::symbol("only") {
            self.assign("transient-mark-mode", Value::symbol("identity"));
        }

        if !self
            .eval_symbol("deactivate-mark")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            let _ = self.apply(Value::symbol("deactivate-mark"), vec![])?;
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        if self
            .apply(Value::symbol("display-selections-p"), vec![])?
            .is_nil()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        if self
            .eval_symbol("select-active-regions")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        if self
            .apply(Value::symbol("region-active-p"), vec![])?
            .is_nil()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        let this_command = self.eval_symbol("this-command").unwrap_or(Value::NIL);
        let inhibited_commands = self
            .eval_symbol("selection-inhibit-update-commands")
            .unwrap_or(Value::NIL);
        if self
            .apply(
                Value::symbol("memq"),
                vec![this_command, inhibited_commands],
            )?
            .is_truthy()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        let region_extract = self
            .eval_symbol("region-extract-function")
            .unwrap_or(Value::symbol("buffer-substring"));
        let text = self.apply(region_extract, vec![Value::NIL])?;
        let text_len = match self.apply(Value::symbol("length"), vec![text])?.kind() {
            ValueKind::Fixnum(len) => len,
            _ => 0,
        };
        if text_len > 0 {
            let _ = self.apply(
                Value::symbol("gui-set-selection"),
                vec![Value::symbol("PRIMARY"), text],
            )?;
        }
        let _ = super::builtins::dispatch_builtin(
            self,
            "run-hook-with-args",
            vec![Value::symbol("post-select-region-hook"), text],
        )
        .unwrap_or(Ok(Value::NIL))?;
        self.assign("saved-region-selection", Value::NIL);
        Ok(Value::NIL)
    }

    /// Trigger redisplay — calls the layout engine and sends frame to render thread.
    ///
    /// Mirrors GNU Emacs `redisplay()` (dispnew.c:5259).
    /// In batch mode (no callback), this is a no-op.
    pub(crate) fn redisplay(&mut self) {
        self.redisplay_with_force(false);
    }

    pub(crate) fn redisplay_for_input_wait(&mut self) {
        self.redisplay_with_force(false);
    }

    /// Generation of asynchronously decoded media state; see
    /// [`Self::invalidate_media`].
    pub fn media_generation(&self) -> u64 {
        self.media_generation
    }

    /// Record that async media reached a terminal state (an image finished
    /// decoding), forcing the next redisplay to rebuild rather than reuse a
    /// retained matrix holding the placeholder geometry.
    pub fn invalidate_media(&mut self) {
        self.media_generation = self.media_generation.wrapping_add(1);
        self.invalidate_redisplay();
    }

    /// Monotonic redisplay-invalidation counter — the analogue of GNU's
    /// `update_mode_lines || windows_or_buffers_changed` trigger family
    /// (bumped by `force-mode-line-update`, display-variable writes, media
    /// changes). Caches of redisplay-derived data key on it.
    pub fn redisplay_generation(&self) -> u64 {
        self.redisplay_generation
    }

    /// Generation of GNU `update_menu_bar` rebuild requests.
    pub fn menu_bar_rebuild_generation(&self) -> MenuBarRebuildGeneration {
        MenuBarRebuildGeneration(self.menu_bar_rebuild_generation)
    }

    /// See the `context_instance_id` field.
    pub fn context_instance_id(&self) -> u64 {
        self.context_instance_id
    }

    /// The chrome dirty set — which windows must re-generate their mode /
    /// header / tab line. See [`crate::emacs_core::chrome_dirty::ChromeDirty`]
    /// for the GNU flags this ports and for why nothing consults it as a skip
    /// yet.
    pub fn chrome_dirty(&self) -> &crate::emacs_core::chrome_dirty::ChromeDirty {
        &self.chrome_dirty
    }

    /// GNU `bset_update_mode_line`: a buffer-scoped event that invalidates
    /// chrome everywhere the buffer might be shown.
    pub fn mark_chrome_dirty_all(&mut self) {
        self.chrome_dirty.mark_all();
    }

    /// GNU `wset_update_mode_line`: a window-scoped event.
    pub fn mark_chrome_dirty_window(&mut self, window: WindowId) {
        self.chrome_dirty.mark_window(window);
        // `wset_update_mode_line` ends in GNU `wset_redisplay`.  That helper
        // raises `windows_or_buffers_changed` exactly when W is not the
        // globally selected window (xdisp.c), which also makes
        // `update_menu_bar` rebuild.  Preserve that selected/nonselected
        // distinction here instead of making every window-local chrome event
        // a broad menu invalidation.
        let selected = self
            .frames
            .selected_frame()
            .is_some_and(|frame| frame.selected_window == window);
        if !selected {
            self.request_menu_bar_rebuild(MenuBarRebuildReason::WindowsOrBuffersChanged);
        }
    }

    /// Called by redisplay for each window whose chrome it actually generated.
    /// GNU's analogue is `mark_window_display_accurate_1`. A window that
    /// SKIPPED its chrome must not be acknowledged here — see
    /// [`crate::emacs_core::chrome_dirty::ChromeDirty`] for why the
    /// acknowledgement is per window rather than a blanket clear.
    pub fn note_chrome_generated(&mut self, window: WindowId) {
        self.chrome_dirty.note_chrome_generated(window);
    }

    /// Drop a deleted window's chrome acknowledgement.
    pub fn forget_chrome_window(&mut self, window: WindowId) {
        self.chrome_dirty.forget_window(window);
    }

    pub(crate) fn invalidate_redisplay(&mut self) {
        tracing::debug!(target: "neomacs::redisplay_sig", "invalidate_redisplay");
        self.redisplay_generation = self.redisplay_generation.wrapping_add(1);
        self.last_redisplay_signature = None;
    }

    /// Cross GNU `update_menu_bar`'s rebuild boundary and schedule redisplay.
    pub(crate) fn request_menu_bar_rebuild(&mut self, reason: MenuBarRebuildReason) {
        tracing::debug!(?reason, "request menu-bar rebuild");
        self.menu_bar_rebuild_generation = self.menu_bar_rebuild_generation.wrapping_add(1);
        self.invalidate_redisplay();
    }

    /// Raise GNU's global `update_mode_lines` flag.
    ///
    /// `bset_update_mode_line` does this unconditionally for buffer-owned
    /// mutations such as `rename-buffer`, even when that buffer is not yet
    /// displayed.  Keep the menu and chrome effects inseparable here: both
    /// are consumers of the same GNU flag.
    pub(crate) fn request_global_mode_line_update(&mut self) {
        self.request_menu_bar_rebuild(MenuBarRebuildReason::UpdateModeLines);
        self.mark_chrome_dirty_all();
    }

    /// Apply GNU's local/global `force-mode-line-update` boundary.
    pub(crate) fn request_mode_line_update(&mut self, target: ModeLineUpdateTarget) {
        let has_mode_line_to_update = match target {
            ModeLineUpdateTarget::CurrentBuffer(buffer) => {
                self.frames.buffer_window_count(&self.buffers, buffer) != 0
            }
            ModeLineUpdateTarget::AllBuffers => true,
        };
        if !has_mode_line_to_update {
            return;
        }

        self.request_global_mode_line_update();
    }

    /// Mark redisplay dirty when a display-affecting variable is set.
    ///
    /// GNU Emacs has no per-variable redisplay flag in the `set`/`setq`
    /// store path: `redisplay_window` re-reads every live display slot
    /// each cycle and the current-matrix diff repaints any change
    /// (`src/xdisp.c:20535-20566`). Neomacs adds an aggressive
    /// optimization GNU lacks — `redisplay_with_force` early-returns on
    /// an unchanged `RedisplaySignature`, which captures buffer/overlay/
    /// text-property ticks, point and window geometry but NOT the
    /// per-buffer display slots (`truncate-lines`, `tab-width`,
    /// `header-line-format`, `cursor-type`, …). So a bare
    /// `(setq truncate-lines t)` left the screen stale until the next
    /// keystroke bumped the signature (Finding 6 in the command-loop
    /// audit; the "Doom blank pane" class of bug).
    ///
    /// To stay faithful to GNU's *observable* behavior we mark redisplay
    /// dirty here — the analogue of GNU `bset_redisplay` /
    /// `windows_or_buffers_changed` — when the variable being set is in
    /// the curated display-affecting set
    /// ([`crate::buffer::buffer::variable_affects_display_by_sym_id`]).
    /// This is checked at the single variable-set chokepoint so the
    /// answer is identical for every write path (tree-walk interpreter,
    /// bytecode VM, `set-default`, custom). The curated set keeps us
    /// from over-triggering redisplay on ordinary non-display variables.
    ///
    /// `sym_id` is resolved through `defvaralias` first so an alias of a
    /// display variable (e.g. an obsolete alias) still nudges redisplay.
    pub(crate) fn mark_redisplay_dirty_if_display_var(&mut self, sym_id: SymId) {
        let resolved =
            builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id).unwrap_or(sym_id);
        if crate::buffer::buffer::variable_affects_display_by_sym_id(resolved) {
            self.invalidate_redisplay();
            // GNU covers the three chrome formats with an
            // `add-variable-watcher` calling `set-buffer-redisplay`
            // (lisp/frame.el:3752-3779 -> xdisp.c:922-931), which raises the
            // mode-line dirty flag. The curated display-variable set here is
            // the same list by another name, so the chrome members of it get
            // the chrome flag too.
            if crate::buffer::buffer::variable_affects_chrome_by_sym_id(resolved) {
                self.chrome_dirty.mark_all();
            }
            // A display-affecting variable changed: the incremental fast paths
            // key on this counter so they re-lay instead of reusing rows shaped
            // under the old setting (the four buffer/face ticks do not move here).
            self.display_var_change_count = self.display_var_change_count.wrapping_add(1);
        }
    }

    pub(crate) fn redisplay_with_force(&mut self, force: bool) {
        // Mirrors GNU `redisplay_internal` (xdisp.c:17242-17245): bail out
        // when `inhibit-redisplay` is non-nil. `run_window_change_functions`
        // (window.c:4116) specbinds this to t so any nested redisplay
        // triggered by a window-change hook is a no-op. Without this check
        // a hook that indirectly calls `redisplay` infinitely recurses.
        let inhibit_redisplay = self.obarray.symbol_value("inhibit-redisplay");
        if !force && inhibit_redisplay.as_ref().is_some_and(|v| v.is_truthy()) {
            tracing::debug!(
                "redisplay inhibited by inhibit-redisplay={}",
                inhibit_redisplay.as_ref().unwrap()
            );
            return;
        }
        self.sync_pending_resize_events();
        // Sync window position caches from markers.  After text edits,
        // markers have auto-adjusted but the usize caches on Window::Leaf
        // may be stale.  Refresh them before redisplay reads positions.
        if let Some(buffer) = self.buffers.current_buffer() {
            let buf_id = buffer.id;
            crate::window::window_markers::sync_all_frames_for_buffer(
                &mut self.frames,
                &self.buffers,
                buf_id,
            );
        }
        // GNU's selected-window point belongs to the selected window's buffer,
        // even when Lisp has temporarily made another buffer current.  Refresh
        // only the selected window cache from its own buffer; redisplay must
        // not realign `current-buffer` with the selected window here.
        if let Some(frame_id) = self.frames.selected_frame().map(|frame| frame.id) {
            super::window_cmds::remember_selected_window_point_in_state(
                &mut self.frames,
                &mut self.buffers,
                frame_id,
            );
        }
        let before_signature = self.redisplay_signature();
        // A pending exact echo-area resize (GNU `resize_echo_area_exactly`)
        // must still drive a redisplay even when the visible signature is
        // otherwise unchanged: the message text can be identical while the
        // mini-window is still grown from a previous longer message and needs
        // to shrink back to fit. Don't skip while the request is pending.
        if tracing::enabled!(target: "neomacs::redisplay_sig", tracing::Level::DEBUG) {
            let captured: Vec<String> = before_signature
                .frame
                .as_ref()
                .map(|frame| {
                    frame
                        .windows
                        .iter()
                        .map(|window| match &window.buffer {
                            Some(buffer) => format!(
                                "w{}:b{}:tick{}:chars{}:total{}",
                                window.layout.id.0,
                                buffer.layout.id.0,
                                buffer.layout.modified_tick,
                                buffer.layout.chars_modified_tick,
                                buffer.layout.total_chars.get()
                            ),
                            None => format!(
                                "w{}:b{}:NO-BUFFER-SIG",
                                window.layout.id.0, window.layout.buffer_id.0
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default();
            tracing::debug!(
                target: "neomacs::redisplay_sig",
                "signature windows=[{}] last_is_some={}",
                captured.join(" "),
                self.last_redisplay_signature.is_some()
            );
        }
        if !force
            && !self.echo_area_resize_exact_pending
            && self.last_redisplay_signature.as_ref() == Some(&before_signature)
        {
            tracing::debug!("redisplay skipped: visible state unchanged");
            return;
        }
        // GNU `prepare_menu_bars` (xdisp.c:14230-14246) runs
        // `pre-redisplay-function` just before the window layout so hooks on
        // `pre-redisplay-functions` (e.g. `global-hl-line-mode` with sticky-flag
        // 'window, the region overlay) can refresh their overlays before
        // redisplay reads them. Placed AFTER the visible-state skip check so it
        // never runs on a skipped redisplay; `last_redisplay_signature` is
        // recomputed at the end of this function and absorbs any overlay change,
        // keeping the next unchanged redisplay skippable (no thrash).
        self.run_pre_redisplay_function();
        self.resize_minibuffer_only_frames();
        // GNU `redisplay_internal` calls `hscroll_window_tree` (src/xdisp.c)
        // before laying out windows so each window's `hscroll` follows point;
        // for a truncated line whose point has moved off the right edge (the
        // `C-e` case, issue #140) this keeps the cursor visible. Updating
        // `Window::Leaf.hscroll` here makes both the layout render and
        // `(window-hscroll)` reflect the new value (no post-layout write-back).
        crate::emacs_core::hscroll::update_auto_hscroll_before_redisplay(self);
        let has_fn = self.redisplay_fn.is_some();
        tracing::debug!("redisplay called (has_fn={})", has_fn);
        if let Some(mut f) = self.redisplay_fn.take() {
            let saved = self.buffers.reset_outermost_restrictions();
            f(self);
            // The layout pass inside `f` consumes any pending exact echo-area
            // resize (GNU `resize_echo_area_exactly`). Clear it now, once per
            // redisplay, so a later mid-command redisplay does not keep
            // shrinking a freshly grown message — GNU only resizes exactly at
            // the command boundary, not on every `redisplay_window`.
            self.echo_area_resize_exact_pending = false;
            let _ = super::builtins::run_redisplay_window_change_hooks(self);
            self.buffers.restore_outermost_restrictions(saved);
            self.redisplay_fn = Some(f);
        } else {
            self.echo_area_resize_exact_pending = false;
            let _ = super::builtins::run_redisplay_window_change_hooks(self);
        }
        self.last_redisplay_signature = Some(self.redisplay_signature());
    }

    /// Run `pre-redisplay-function` (the driver of the `pre-redisplay-functions`
    /// hook) just before laying out, mirroring GNU `prepare_menu_bars`
    /// (xdisp.c:14230-14246). Features such as `global-hl-line-mode`
    /// (`global-hl-line-sticky-flag` = 'window) and the region overlay register
    /// on `pre-redisplay-functions` and depend on this to refresh their overlays
    /// before redisplay reads them; without it hl-line never highlights the
    /// current line.
    ///
    /// `inhibit-redisplay` is bound to t (GNU's redisplay is already
    /// `redisplaying_p`, and `run_redisplay_window_change_hooks` does the same)
    /// so a nested redisplay triggered by a hook is a no-op; an error from the
    /// hook is demoted (GNU calls via `dsafe_calln`, and the lisp driver wraps
    /// each hook in `with-demoted-errors`).
    fn run_pre_redisplay_function(&mut self) {
        let Some(function) = self.obarray.symbol_value("pre-redisplay-function").copied() else {
            return;
        };
        if function.is_nil() {
            return;
        }
        let specpdl_count = self.specpdl.len();
        if let Err(flow) = self.try_specbind_or_unwind_to(
            specpdl_count,
            crate::emacs_core::intern::intern("inhibit-redisplay"),
            Value::T,
        ) {
            tracing::debug!("pre-redisplay binding signalled (ignored): {flow:?}");
            return;
        }
        // GNU passes the list of windows being redisplayed; `t` makes
        // `redisplay--pre-redisplay-functions` iterate every live window.
        let result = self.funcall_general(function, vec![Value::T]);
        let result = self.unbind_to_with_result(specpdl_count, result);
        if let Err(flow) = result {
            tracing::debug!("pre-redisplay-function signalled (ignored): {flow:?}");
        }
    }

    fn resize_minibuffer_only_frames(&mut self) {
        if !self
            .obarray
            .symbol_value("resize-mini-frames")
            .is_some_and(|value| value.is_truthy())
        {
            return;
        }
        let frames: Vec<Value> = self
            .frames
            .frame_list()
            .into_iter()
            .filter_map(|frame_id| {
                self.frames.get(frame_id).and_then(|frame| {
                    (frame.visibility.is_visible()
                        && frame.minibuffer_window == Some(frame.root_window.id()))
                    .then_some(Value::make_frame(frame_id.0))
                })
            })
            .collect();
        for frame in frames {
            let _ = self.safe_funcall(Value::symbol("window--resize-mini-frame"), vec![frame]);
        }
    }

    fn redisplay_signature(&self) -> RedisplaySignature {
        let selected_frame = self.frames.selected_frame().map(|frame| frame.id.0);
        let selected_window = self
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window.0);
        let frame = self.frames.selected_frame().map(|frame| {
            let mut window_ids = frame.window_list();
            if let Some(minibuffer_window) = frame.minibuffer_window {
                window_ids.push(minibuffer_window);
            }
            let mut windows = Vec::with_capacity(window_ids.len());
            for window_id in window_ids {
                let Some(window) = frame.find_window(window_id) else {
                    continue;
                };
                let Some(state) = window.redisplay_state() else {
                    continue;
                };
                let Some(layout) = frame.window_layout_inputs(window_id) else {
                    continue;
                };
                windows.push(RedisplayWindowSignature {
                    layout,
                    window_end: state.window_end,
                    old_point: state.old_point,
                    buffer: self.redisplay_buffer_signature(state.buffer_id),
                });
            }
            RedisplayFrameSignature {
                layout: frame.layout_inputs(),
                selected_window: frame.selected_window.0,
                window_state_change: frame.window_state_change,
                windows,
            }
        });
        RedisplaySignature {
            selected_frame,
            selected_window,
            current_buffer: self.buffers.current_buffer_id().map(|id| id.0),
            current_message: self.current_message.clone(),
            active_minibuffer_window: self.active_minibuffer_window.map(|id| id.0),
            minibuffer_selected_window: self.minibuffer_selected_window.map(|id| id.0),
            face_change_count: self.face_change_count,
            obarray_function_epoch: self.obarray.function_epoch(),
            redisplay_generation: self.redisplay_generation,
            frame,
        }
    }

    fn redisplay_buffer_signature(
        &self,
        id: crate::buffer::BufferId,
    ) -> Option<RedisplayBufferSignature> {
        let buffer = self.buffers.get(id)?;
        Some(RedisplayBufferSignature {
            layout: self.buffer_layout_inputs(id)?,
            save_modified_tick: buffer.save_modified_tick(),
            autosave_modified_tick: buffer.autosave_modified_tick,
            point: buffer.point_char_pos(),
            point_emacs_byte: buffer.point_emacs_byte_pos(),
            last_window_start: buffer.last_window_start,
            last_selected_window: buffer.last_selected_window.map(|id| id.0),
        })
    }

    fn this_command_name_for_log(&self) -> String {
        self.eval_symbol("this-command")
            .map(|value| format!("{}", value))
            .unwrap_or_else(|_| "<unbound>".to_string())
    }

    fn command_value_for_log(value: Value) -> String {
        value
            .as_symbol_name()
            .map(str::to_owned)
            .unwrap_or_else(|| crate::emacs_core::print::print_value(&value))
    }

    fn command_keys_for_log(keys: &[Value]) -> String {
        keys.iter()
            .map(|key| {
                crate::emacs_core::keyboard::pure::describe_single_key_value(key, false)
                    .map(|description| crate::emacs_core::emacs_char::to_utf8_lossy(&description))
                    .unwrap_or_else(|_| crate::emacs_core::print::print_value(key))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Perform a full mark-and-sweep garbage collection.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn gc_collect(&mut self) {
        self.gc_collect_exact();
    }

    /// Perform a full mark-and-sweep garbage collection using only explicit
    /// roots. Always runs to completion synchronously (force-completes any
    /// in-flight incremental mark), matching GNU `garbage-collect` semantics.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn gc_collect_exact(&mut self) {
        self.profiler_gc_start();
        self.gc_collect_from_current_roots_impl(true);
        self.profiler_gc_finish();
    }

    /// Safe-point GC entry. Uses concurrent marking after the heap's
    /// bootstrap cycle (and, for dump heaps, once the pdump is blackened);
    /// the first cycle runs a stop-the-world collection.
    ///
    /// Exact-GC stress mode always collects synchronously to completion: its
    /// purpose is a deterministic missing-root shakeout at every
    /// allocation-bearing safe point, which an asynchronous concurrent cycle
    /// would both defer and de-randomize.
    fn gc_collect_from_current_roots(&mut self) {
        self.profiler_gc_start();
        self.gc_collect_from_current_roots_impl(self.gc_stress);
        self.profiler_gc_finish();
    }

    /// Drive a collection from the current roots.
    ///
    /// `force_complete` (explicit `garbage-collect`) runs synchronously to a
    /// full sweep. Otherwise, on partitioned cycles with incremental marking
    /// enabled, marking is sliced across safe points: each call advances one
    /// bounded slice and only the slice that drains the gray queue runs mark
    /// termination + sweep. The first cycle and non-incremental builds take the
    /// stop-the-world path.
    fn gc_collect_from_current_roots_impl(&mut self, force_complete: bool) {
        // A6 publication discipline: collecting while run_loop's operand-stack
        // cursor holds an unpublished length would mark a stale bc_buf prefix.
        #[cfg(debug_assertions)]
        crate::emacs_core::bytecode::vm::debug_assert_no_live_stack_cursor();
        // Inline set/restore, NOT a Drop guard (see the `gc_driver_active`
        // field doc): the body is infallible, so the trailing restore runs on
        // every normal exit (including the body's early `return`s), while a
        // panic escaping the body leaves the flag set for the module-boundary
        // containment probe to see. Save/restore (not set/clear) so a nested
        // collection — e.g. `(garbage-collect)` reached from a finalizer —
        // cannot clear the OUTER driver extent's flag on its way out.
        let prev = self.gc_driver_active;
        self.gc_driver_active = true;
        self.gc_collect_from_current_roots_body(force_complete);
        self.gc_driver_active = prev;
    }

    fn gc_collect_from_current_roots_body(&mut self, force_complete: bool) {
        // GNU `garbage_collect' shortens every live buffer's undo list before
        // it marks anything: "Don't keep undo information around forever. Do
        // this early on, so it is no problem if the user quits."
        // (src/alloc.c:5796-5800). This is the only place undo lists are
        // truncated -- `undo-boundary' does not do it.
        //
        // GNU compacts once per `garbage_collect' call. This collector slices
        // one collection across several safe points, so compact only when a
        // new cycle is about to start: an in-flight mark or sweep is the
        // continuation of a collection whose compaction already ran.
        if force_complete
            || !(self.tagged_heap.mark_in_progress() || self.tagged_heap.sweep_in_progress())
        {
            crate::emacs_core::undo::compact_buffers_for_gc(self);
        }
        let start = std::time::Instant::now();
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        // Per-slice sweep budget in cons blocks (each ~4096 cells); the slice
        // reclaims proportionally more non-cons objects internally.
        const INCREMENTAL_SWEEP_BUDGET: usize = 8;
        let heap_ptr: *mut crate::tagged::gc::TaggedHeap = &mut *self.tagged_heap;
        // True only when a whole mark+sweep cycle finishes in this call, gating
        // the once-per-collection bookkeeping below.
        let cycle_completed;
        // Safety: GC is stop-the-world with exclusive `&mut self`. Root
        // enumeration only reads Context state while seeding the collector via
        // the raw heap pointer, which aliases `self.tagged_heap`.
        unsafe {
            if force_complete {
                // Explicit GC: drive any in-flight cycle to completion, then run
                // a fresh full stop-the-world collection of the current state
                // (GNU `garbage-collect` semantics).
                if (*heap_ptr).concurrent_mark_running() {
                    self.terminate_concurrent_mark(heap_ptr);
                }
                if (*heap_ptr).sweep_in_progress() {
                    (*heap_ptr).finish_incremental_sweep_now();
                }
                (*heap_ptr).begin_collection();
                self.seed_all_context_roots(heap_ptr);
                (*heap_ptr).complete_collection();
                cycle_completed = true;
            } else if (*heap_ptr).sweep_in_progress() {
                // Phase 3: drain the deferred sweep started at mark termination.
                if (*heap_ptr).incremental_sweep_slice(INCREMENTAL_SWEEP_BUDGET) {
                    cycle_completed = true; // sweep drained -> cycle done
                } else {
                    return; // more sweep to do; defer bookkeeping
                }
            } else if (*heap_ptr).concurrent_mark_running() {
                // Phase 5: the background GC thread is marking while we run. If it
                // has drained, run the (short) stop-the-world termination; else
                // return immediately and keep mutating — this is the pause win.
                // A hard cap forces termination if allocation outruns marking.
                let cap = (*heap_ptr).gc_threshold().saturating_mul(4);
                let must_finish = (*heap_ptr).bytes_since_gc() > cap;
                let mark_done = (*heap_ptr).concurrent_mark_done();
                if mark_done || must_finish {
                    if must_finish && !mark_done {
                        // Cap-forced: the GC thread had NOT drained — the
                        // residual mark now runs synchronously in the
                        // termination below. Record it so the pacing
                        // instrumentation escalates its lead and the trace
                        // attributes the pause.
                        (*heap_ptr).note_must_finish();
                    }
                    self.terminate_concurrent_mark(heap_ptr);
                    return; // sweep deferred; cycle not done yet
                }
                return; // GC thread still marking; mutator continues
            } else if (*heap_ptr).should_run_concurrent() {
                // Concurrent start handshake: snapshot roots, hand the gray queue
                // to the GC thread, and return — marking now overlaps the mutator.
                self.start_concurrent_mark(heap_ptr);
                return; // marking concurrent; cycle not done yet
            } else if (*heap_ptr).is_partition_first_cycle() {
                // FIRST PARTITION CYCLE, CONCURRENT: the dump-blackening
                // bootstrap used to be the one big STW pause (a full trace of
                // the mapped image — ~12-50ms). Armed, `begin_collection`
                // seeds the veclike/string mapped children in the handshake
                // and stages the (bulk) cons ranges for the GC thread, whose
                // claim job DROPS span-inside children instead of deferring
                // them. Promotion + blackening run when this cycle's sweep
                // drains (`finish_first_partition_cycle` below).
                (*heap_ptr).arm_first_cycle_concurrent();
                self.start_concurrent_mark(heap_ptr);
                return; // marking concurrent; cycle not done yet
            } else {
                // Stop-the-world full collection (dump-less bootstrap): the
                // only remaining non-concurrent threshold path, sized by the
                // young heap alone.
                (*heap_ptr).begin_collection();
                self.seed_all_context_roots(heap_ptr);
                (*heap_ptr).complete_collection();
                cycle_completed = true;
            }
        }
        if !cycle_completed {
            return;
        }
        // First partition cycle (concurrent bootstrap): with the sweep now
        // drained, promote survivors + blacken the image + build the initial
        // remembered set. No-op for every other cycle (including the STW
        // paths, which promote inside `complete_collection`).
        unsafe {
            (*heap_ptr).finish_first_partition_cycle();
        }
        self.gc_pending = false;
        self.gc_count += 1;
        self.update_gc_runtime_stats(start.elapsed());
        self.sync_gc_threshold_from_runtime_settings();
        // Destroy the GPU objects of shader-surface handles this cycle swept.
        // Every completed collection funnels through this block (explicit
        // `garbage-collect` and the safe-point paths above), so this is the
        // single drain point for `TaggedHeap::pending_surface_destroys`.
        self.drain_pending_surface_destroys();
        self.drain_pending_video_destroys();
        // GNU `garbage_collect` runs the doomed finalizers before
        // `post-gc-hook`.
        self.run_doomed_finalizers();
        self.run_post_gc_hook();
        if self.gc_stress {
            // GNU resets `consing_until_gc` before running post-gc-hook and
            // runs the hook with GC inhibited.  Keep Neomacs' exact-GC stress
            // mode from treating hook bookkeeping allocation as a fresh
            // allocation-bearing safe point.
            self.tagged_heap.reset_bytes_since_gc();
        }
    }

    /// Seed every evaluator/context root into the collector's gray queue and
    /// install the per-buffer marker-chain head slots used by sweep. Does NOT
    /// clear marks, so it is safe to call both at incremental start and again
    /// at mark termination (re-snapshotting roots).
    ///
    /// Returns the per-group cost/volume breakdown of this seed (diagnostics
    /// only — seeding order and content are unchanged). Groups are the
    /// `trace_roots` sections, the per-side-table thread-local collects, the
    /// thread-local seed loop (`tl_seed`), and the marker-chain-head install
    /// (`marker_heads`, whose count is the live-buffer count).
    ///
    /// Safety: caller holds exclusive `&mut self`; `heap_ptr` aliases
    /// `self.tagged_heap`. Root enumeration only reads Context state.
    unsafe fn seed_all_context_roots(
        &mut self,
        heap_ptr: *mut crate::tagged::gc::TaggedHeap,
    ) -> crate::tagged::gc::RootSeedBreakdown {
        use std::cell::{Cell, RefCell};
        let seed_t0 = std::time::Instant::now();
        // Per-group recorder shared by the two `trace_roots` closures via
        // interior mutability (both need it: the boundary closure closes the
        // running group, the visit closure counts values).
        let groups: RefCell<Vec<crate::tagged::gc::RootGroup>> =
            RefCell::new(Vec::with_capacity(32));
        let cur_name: Cell<Option<&'static str>> = Cell::new(None);
        let cur_t0: Cell<std::time::Instant> = Cell::new(seed_t0);
        let cur_count: Cell<usize> = Cell::new(0);
        let close_group = || {
            if let Some(name) = cur_name.get() {
                groups.borrow_mut().push((
                    name,
                    cur_t0.get().elapsed().as_micros() as u64,
                    cur_count.get(),
                ));
            }
        };
        #[cfg(debug_assertions)]
        let mut root_index = 0usize;
        self.trace_roots(
            &mut |name| {
                close_group();
                cur_name.set(Some(name));
                cur_count.set(0);
                cur_t0.set(std::time::Instant::now());
            },
            &mut |root| {
                cur_count.set(cur_count.get() + 1);
                #[cfg(debug_assertions)]
                {
                    let origin = format!("context-root#{root_index}");
                    root_index += 1;
                    unsafe {
                        (*heap_ptr).seed_root_with_origin(root, &origin);
                    }
                }
                #[cfg(not(debug_assertions))]
                {
                    unsafe {
                        (*heap_ptr).seed_root(root);
                    }
                }
            },
        );
        close_group();
        let mut groups = groups.into_inner();
        let heap_identity = unsafe { (*heap_ptr).identity() };
        let mut thread_local_roots = Vec::new();
        collect_thread_local_gc_roots(&mut thread_local_roots, heap_identity, &mut groups);
        let tl_seed_t0 = std::time::Instant::now();
        let tl_seed_count = thread_local_roots.len();
        for (root, origin) in thread_local_roots {
            unsafe {
                (*heap_ptr).seed_root_with_origin(root, origin);
            }
        }
        groups.push((
            "tl_seed",
            tl_seed_t0.elapsed().as_micros() as u64,
            tl_seed_count,
        ));
        // Install per-buffer marker-chain head slots so `unchain_dead_markers`
        // can splice unmarked markers out of every live chain before sweep.
        // Mirrors GNU `sweep_buffer → unchain_dead_markers` (alloc.c).
        let heads_t0 = std::time::Instant::now();
        // Safety: stop-the-world GC — no concurrent borrows of the buffer
        // storage exist (the pre-refactor body relied on the enclosing
        // `unsafe fn` for this same call).
        let chain_heads = unsafe { self.buffers.collect_marker_chain_head_slots() };
        let heads_count = chain_heads.len();
        unsafe {
            (*heap_ptr).set_marker_chain_head_slots(chain_heads);
        }
        groups.push((
            "marker_heads",
            heads_t0.elapsed().as_micros() as u64,
            heads_count,
        ));
        crate::tagged::gc::RootSeedBreakdown {
            total_us: seed_t0.elapsed().as_micros() as u64,
            groups,
        }
    }

    /// Start a non-blocking concurrent mark (Phase 5): clear marks + seed the
    /// complete root snapshot into the gray queue, then hand it to the GC thread.
    /// Returns immediately — the mutator runs while the GC thread marks conses.
    ///
    /// The whole handshake and each phase are timed into `HandshakeStats`
    /// (clear/runtime/remembered are recorded heap-side by `concurrent_begin`;
    /// conssnap/vecsnap/jobasm by `launch_concurrent_mark`) and printed under
    /// `NEOVM_GC_TRACE=1`. Size probes are refreshed AFTER the pause is
    /// stamped so probe collection never inflates the measured pause.
    ///
    /// Safety: as `seed_all_context_roots`.
    unsafe fn start_concurrent_mark(&mut self, heap_ptr: *mut crate::tagged::gc::TaggedHeap) {
        let start_t0 = std::time::Instant::now();
        let (obsnap_us, roots_breakdown, ob_slots, ob_chunks);
        unsafe {
            (*heap_ptr).concurrent_begin();
            // CONCURRENT OBARRAY SCAN (Stage 1b). Capture the obarray chunk snapshot
            // at THIS world-stopped point — the same instant the cons snapshot is
            // taken (inside `launch_concurrent_mark`) and the roots are seeded — so
            // `n_slots`/`n_chunks` reflect the start-of-cycle obarray. The heap can't
            // reach the Context-side obarray, so we build it here and stage it on the
            // heap for the launch to move into the job. The symbol-cell skip guard,
            // scoped to just the seed, keeps the start seed from also pushing the
            // symbol cells the GC thread now owns (the BLV pool + non-obarray roots
            // still seed normally).
            let obsnap_t0 = std::time::Instant::now();
            let snap = self.obarray.scan_snapshot();
            obsnap_us = obsnap_t0.elapsed().as_micros() as u64;
            ob_slots = snap.n_slots();
            ob_chunks = snap.n_chunks();
            (*heap_ptr).set_pending_obarray_scan(snap);
            {
                let _skip = crate::emacs_core::symbol::ObarraySymbolCellSkipGuard::new();
                roots_breakdown = self.seed_all_context_roots(heap_ptr);
            }
            (*heap_ptr).launch_concurrent_mark();
        }
        let total_us = start_t0.elapsed().as_micros() as u64;
        // Pause is stamped; stats bookkeeping + probes below are off-pause.
        let pace_lead = self.tagged_heap.pace_probe();
        let pace_bytes = self.tagged_heap.bytes_since_gc();
        let pace_thr = self.tagged_heap.gc_threshold();
        #[cfg(feature = "jit")]
        let (jit_entries, jit_slots) = crate::emacs_core::jit::cache::compiled_cache_probe();
        #[cfg(not(feature = "jit"))]
        let (jit_entries, jit_slots) = (0usize, 0usize);
        let bc_depth = self.bc_buf.len();
        let specpdl_depth = self.specpdl.len();
        let hs = unsafe { (*heap_ptr).handshake_stats_mut() };
        hs.last_start_obsnap_us = obsnap_us;
        hs.last_start_roots = roots_breakdown;
        hs.last_start_total_us = total_us;
        hs.max_start_total_us = hs.max_start_total_us.max(total_us);
        hs.probe_obarray_slots = ob_slots;
        hs.probe_obarray_chunks = ob_chunks;
        hs.probe_bc_buf_depth = bc_depth;
        hs.probe_specpdl_depth = specpdl_depth;
        hs.probe_jit_compiled_entries = jit_entries;
        hs.probe_jit_reloc_slots = jit_slots;
        hs.probe_buffer_count = hs
            .last_start_roots
            .groups
            .iter()
            .find(|(name, _, _)| *name == "marker_heads")
            .map(|&(_, _, count)| count)
            .unwrap_or(0);
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "NEOVM_GC concurrent_start {total_us}us \
                 pace[bytes={pace_bytes} thr={pace_thr} lead={pace_lead}] \
                 [clear={}us[cons={} noncons={} \
                 mapped={}] runtime={}us({}) \
                 remembered={}us({}) obsnap={obsnap_us}us roots={}us conssnap={}us \
                 vecsnap={}us jobasm={}us groups[{}] probes[{}]]",
                hs.last_start_clear_us,
                hs.last_start_clear_cons_us,
                hs.last_start_clear_noncons_us,
                hs.last_start_clear_mapped_us,
                hs.last_start_runtime_us,
                hs.last_start_runtime_roots,
                hs.last_start_remembered_us,
                hs.last_start_remembered_roots,
                hs.last_start_roots.total_us,
                hs.last_start_conssnap_us,
                hs.last_start_vecsnap_us,
                hs.last_start_jobasm_us,
                hs.last_start_roots.format_nonzero(),
                hs.format_probes(),
            );
        }
    }

    /// Terminate a concurrent mark stop-the-world: stop the GC thread and reclaim
    /// the heap, re-snapshot the COMPLETE root set (covering root->white edges the
    /// barrier cannot observe), drain the residual marking (deferred non-cons +
    /// SATB + roots) to a fixpoint, then start the deferred sweep. The expensive
    /// cons-spine traversal already happened concurrently; this pause finishes the
    /// veclike/string traces and any roots that appeared during the window.
    ///
    /// Safety: as `seed_all_context_roots`.
    /// Every phase of the pre-drain `roots=` lump is timed into
    /// `HandshakeStats` (join/fold heap-side; runtime+remembered by
    /// `reseed_runtime_and_remembered_roots`; the context re-seed per group;
    /// the Stage 1b new-symbol residual here) along with the post-drain
    /// finalizer/weak/unchain passes (heap-side, in `incremental_finish`).
    /// Probes are refreshed after the pause is stamped.
    unsafe fn terminate_concurrent_mark(&mut self, heap_ptr: *mut crate::tagged::gc::TaggedHeap) {
        let term_t0 = std::time::Instant::now();
        let (roots_us, drain_us);
        let (ctxroots_breakdown, newsyms_us);
        let mut newsyms_roots = 0usize;
        unsafe {
            // Reclaim exclusive heap ownership: stop the GC thread and fold its
            // residual SATB + deferred work back into the gray queue.
            (*heap_ptr).join_concurrent_mark();
            (*heap_ptr).reseed_runtime_and_remembered_roots();
            {
                // Stage 1a: the symbol-cell SATB barrier retained every
                // value/function/plist overwrite during the mark window, so this
                // TERMINATION re-seed skips the ~450k-symbol obarray walk (the
                // dominant root pause). The guard still seeds the BLV-pool residual
                // + every non-obarray Context root, and restores full-scan on drop
                // so the start seed + STW full-collection seeds are unaffected.
                let _skip = crate::emacs_core::symbol::ObarraySymbolCellSkipGuard::new();
                ctxroots_breakdown = self.seed_all_context_roots(heap_ptr);
            }
            // Stage 1b CONCURRENT OBARRAY SCAN termination residual: the GC thread's
            // scan covered only the symbol cells present at the start snapshot (slots
            // [0, start_slots)). Symbols interned MID-CYCLE live in slots
            // >= start_slots and were never scanned, and the symbol-cell SATB barrier
            // only retains OVERWRITES of pre-existing heap values (it does not seed a
            // brand-new symbol's initial val/function/plist). So at this STW point,
            // bounded-re-seed exactly the new range. Chosen over the "seed the FULL
            // obarray un-skipped" fallback: it preserves the Stage 1a win (no full
            // ~450k-symbol walk) while staying correct. `None` only if no start
            // snapshot was captured, in which case the residual is skipped.
            let newsyms_t0 = std::time::Instant::now();
            if let Some(start_slots) = (*heap_ptr).take_concurrent_obarray_start_slots() {
                self.obarray
                    .trace_new_symbol_cells(start_slots, &mut |root| {
                        newsyms_roots += 1;
                        #[cfg(debug_assertions)]
                        {
                            (*heap_ptr).seed_root_with_origin(root, "stage1b-new-symbol");
                        }
                        #[cfg(not(debug_assertions))]
                        {
                            (*heap_ptr).seed_root(root);
                        }
                    });
            }
            newsyms_us = newsyms_t0.elapsed().as_micros() as u64;
            roots_us = term_t0.elapsed().as_micros();
            let bytes_before = (*heap_ptr).live_bytes();
            let pause_t0 = std::time::Instant::now();
            (*heap_ptr).incremental_drain_all();
            drain_us = pause_t0.elapsed().as_micros();
            (*heap_ptr).incremental_finish(bytes_before, pause_t0);
        }
        // Pause work done; stats bookkeeping + probes below are off-pause.
        #[cfg(feature = "jit")]
        let (jit_entries, jit_slots) = crate::emacs_core::jit::cache::compiled_cache_probe();
        #[cfg(not(feature = "jit"))]
        let (jit_entries, jit_slots) = (0usize, 0usize);
        let bc_depth = self.bc_buf.len();
        let specpdl_depth = self.specpdl.len();
        let ob_slots = self.obarray.current_slot_len();
        let hs = unsafe { (*heap_ptr).handshake_stats_mut() };
        hs.last_term_ctxroots = ctxroots_breakdown;
        hs.last_term_newsyms_us = newsyms_us;
        hs.last_term_newsyms_roots = newsyms_roots;
        hs.last_term_roots_total_us = roots_us as u64;
        hs.max_term_roots_total_us = hs.max_term_roots_total_us.max(roots_us as u64);
        hs.probe_bc_buf_depth = bc_depth;
        hs.probe_specpdl_depth = specpdl_depth;
        hs.probe_obarray_slots = ob_slots;
        hs.probe_jit_compiled_entries = jit_entries;
        hs.probe_jit_reloc_slots = jit_slots;
        hs.probe_buffer_count = hs
            .last_term_ctxroots
            .groups
            .iter()
            .find(|(name, _, _)| *name == "marker_heads")
            .map(|&(_, _, count)| count)
            .unwrap_or(0);
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            let stats = self.tagged_heap.sweep_stats();
            let hs = self.tagged_heap.handshake_stats();
            eprintln!(
                "NEOVM_GC concurrent_termination {}us [roots={roots_us}us drain={drain_us}us \
                 fold={}us deferred={} satb={} str_claimed={} f_claimed={} sub_dropped={} \
                 v_claimed={} bc_claimed={} kinds[{}] join={}us \
                 runtime={}us({}) remembered={}us({}) ctxroots={}us newsyms={newsyms_us}us({}) \
                 finalizer={}us weak={}us unchain={}us groups[{}] probes[{}]]",
                term_t0.elapsed().as_micros(),
                stats.last_termination_fold_us,
                stats.last_termination_deferred,
                stats.last_termination_satb,
                stats.last_concurrent_str_claimed,
                stats.last_concurrent_float_claimed,
                stats.last_concurrent_subr_dropped,
                stats.last_concurrent_vec_claimed,
                stats.last_concurrent_bc_claimed,
                stats.last_termination_kinds,
                hs.last_term_join_us,
                hs.last_term_runtime_us,
                hs.last_term_runtime_roots,
                hs.last_term_remembered_us,
                hs.last_term_remembered_roots,
                hs.last_term_ctxroots.total_us,
                newsyms_roots,
                hs.last_term_finalizer_us,
                hs.last_term_weak_us,
                hs.last_term_unchain_us,
                hs.last_term_ctxroots.format_nonzero(),
                hs.format_probes(),
            );
        }
    }

    pub(crate) fn with_gc_inhibited<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let mut guard = GcInhibitGuard::enter(self);
        f(guard.context())
    }

    fn run_post_gc_hook(&mut self) {
        let hook = crate::emacs_core::hook_runtime::hook_symbol_by_id(self, post_gc_hook_symbol());
        let _ = self.with_gc_inhibited(|eval| {
            crate::emacs_core::hook_runtime::safe_run_named_hook(eval, hook, &[])
        });
    }

    /// Run the functions queued for finalizer objects the just-finished cycle
    /// found unreachable — GNU `run_finalizers` (alloc.c). The whole batch is
    /// taken up front, so a finalizer created (and doomed) during one of these
    /// calls lands in a later batch after a later cycle. Each function is
    /// called with zero args; errors are ignored so one failing finalizer
    /// cannot block the rest (GNU wraps each call in a catch-all
    /// `internal_condition_case`).
    fn run_doomed_finalizers(&mut self) {
        let functions = self.tagged_heap.take_doomed_finalizer_functions();
        if functions.is_empty() {
            return;
        }
        // The taken batch left the heap-side queue root; keep it rooted for
        // the duration — `with_gc_inhibited` blocks safe-point GCs, but an
        // explicit `garbage-collect` inside a finalizer still collects.
        let saved_roots = save_scratch_gc_roots();
        for function in functions.iter().copied() {
            push_scratch_gc_root(function);
        }
        self.with_gc_inhibited(|eval| {
            for function in functions {
                let _ = eval.funcall_general(function, Vec::<Value>::new());
            }
        });
        restore_scratch_gc_roots(saved_roots);
    }

    /// Destroy the GPU objects of every shader-surface handle the
    /// just-finished cycle swept (`SurfaceObj` — see
    /// `TaggedHeap::pending_surface_destroys`). Best-effort: errors are
    /// ignored, and without a display host the batch is simply dropped (the
    /// ids were host-allocated, so no host means no GPU objects to free — the
    /// host was torn down). A handle already freed by an explicit
    /// `neomacs-surface-destroy` re-queues its id here when the dead handle
    /// is later swept; the render-thread free of a missing id is a no-op, so
    /// the double destroy is harmless.
    fn drain_pending_surface_destroys(&mut self) {
        let ids = self.tagged_heap.take_pending_surface_destroys();
        if ids.is_empty() {
            return;
        }
        let Some(host) = self.display_host.as_ref() else {
            return;
        };
        for id in ids {
            if let Err(err) = host.destroy_shader_surface(id) {
                tracing::debug!("gc surface destroy {id} failed: {err}");
            }
        }
    }

    /// Close every video session whose final Lisp handle was swept by the
    /// just-finished collection. As with shader surfaces, this is best-effort:
    /// losing the GUI host already tears down the renderer that owns the
    /// corresponding sessions.
    fn drain_pending_video_destroys(&mut self) {
        let ids = self.tagged_heap.take_pending_video_destroys();
        if ids.is_empty() {
            return;
        }
        let Some(host) = self.display_host.as_ref() else {
            return;
        };
        for id in ids {
            if let Err(err) = host.destroy_video(id) {
                tracing::debug!(video_id = id.get(), "gc video destroy failed: {err}");
            }
        }
    }

    /// Borrow VALUE's string payload for as long as the collector provably
    /// cannot run.
    ///
    /// This is the ergonomic front door to [`Value::lisp_string_in`], and the
    /// reason it lives on `Context` rather than on `Value` is the whole
    /// mechanism: the borrow it returns holds `&self`, and EVERY safepoint in
    /// this engine is a `&mut self` method on `Context`
    /// (`gc_safe_point`/`gc_safe_point_exact` here, `eval_sub`'s collect at
    /// the interpreted-eval boundary, `apply_internal`'s at the funcall
    /// boundary, `bytecode_branch_maybe_gc_and_quit` from the VM and the JIT,
    /// and `builtin_garbage_collect`). So
    ///
    /// ```ignore
    /// let s = ctx.lisp_string(v).unwrap();
    /// ctx.apply(f, args)?;   // error[E0502]: cannot borrow `*ctx` as mutable
    /// use_bytes(s.as_bytes());
    /// ```
    ///
    /// does not compile, whereas the same code written with
    /// `v.as_lisp_string()` compiles and reads freed memory. "Is this borrow
    /// held across a safepoint" becomes a compile question.
    ///
    /// When a value genuinely must survive a safepoint, this is the wrong
    /// tool: root it (DIVERGENCES.md 161/162) or copy the bytes out.
    #[inline]
    pub(crate) fn lisp_string(&self, value: Value) -> Option<&crate::heap_types::LispString> {
        value.lisp_string_in(&self.tagged_heap)
    }

    /// [`Context::lisp_string`] with GNU's `CHECK_STRING` signal — the
    /// drop-in replacement for `builtins::expect_lisp_string(&args[i])?` at a
    /// site that must not hold the borrow across a safepoint.
    #[inline]
    pub(crate) fn expect_lisp_string(
        &self,
        value: Value,
    ) -> Result<&crate::heap_types::LispString, Flow> {
        self.lisp_string(value).ok_or_else(|| {
            crate::emacs_core::error::signal(
                crate::emacs_core::error::LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), value],
            )
        })
    }

    /// GC safe point used at evaluator boundaries.
    pub fn gc_safe_point(&mut self) {
        self.gc_safe_point_exact();
    }

    /// Trigger a safe-point collection using only explicit evaluator roots.
    pub(crate) fn gc_safe_point_exact(&mut self) {
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
    }

    fn gc_safe_point_exact_should_collect(&mut self) -> bool {
        if self.gc_inhibit_depth > 0 {
            return false;
        }
        // An in-flight incremental mark or deferred sweep must keep getting
        // slices at every safe point until it finishes, regardless of the
        // allocation threshold.
        if self.tagged_heap.mark_in_progress() || self.tagged_heap.sweep_in_progress() {
            return true;
        }
        if self.gc_pending {
            return true;
        }
        if self.gc_stress {
            // GNU `maybe_gc` only reaches collection after the consing
            // countdown crosses zero.  Stress exact GC at every boundary that
            // follows allocation, but do not spin full-heap collections across
            // boundaries where the heap has not changed.
            return self.tagged_heap.bytes_since_gc() > 0;
        }
        if self.tagged_heap.gc_threshold_is_overridden() {
            return self.tagged_heap.should_collect();
        }

        if !self.tagged_heap.should_collect() {
            return false;
        }

        // GNU's maybe_gc hot path only checks consing_until_gc and defers
        // percentage-based threshold recalculation until the countdown crosses
        // zero.  Keep Neomacs' allocation fast path in the same shape.
        // Rare path (the byte counter already crossed the current threshold):
        // re-read the Lisp settings before deciding, so a threshold raised since
        // the last GC — including one restored by unbinding a `let` — is honored
        // without waiting for a GC the raise was meant to prevent (see
        // `sync_gc_threshold_from_runtime_settings`).
        self.refresh_gc_runtime_settings_cache();
        let threshold = self.effective_gc_threshold_bytes();
        if self.tagged_heap.gc_threshold() != threshold {
            self.tagged_heap.set_gc_threshold_from_runtime(threshold);
        }
        self.tagged_heap.should_collect()
    }

    /// GNU-style quit processing used from evaluator boundaries.
    ///
    /// Mirrors `process_quit_flag` in GNU `eval.c`: clear `quit-flag`, then
    /// honor `throw-on-input`, `kill-emacs`, or signal `quit`.
    fn process_quit_flag(&mut self) -> Result<(), Flow> {
        let flag = self.quit_flag;
        self.set_quit_flag_value(Value::NIL);

        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);

        if flag.as_symbol_id() == Some(self.kill_emacs_symbol) {
            // GNU keyboard.c process_quit_flag: (setq quit-flag 'kill-emacs)
            // calls Fkill_emacs, which runs the hooks and exits. Unwinding as
            // a quit signal instead would let condition-case swallow the exit.
            return super::builtins::symbols::builtin_kill_emacs(self, vec![]).map(|_| ());
        }

        if !throw_on_input.is_nil() && equal_value(&flag, &throw_on_input, 0) {
            tracing::debug!(
                target: "neomacs::throw_on_input",
                ?flag,
                ?throw_on_input,
                condition_stack_len = self.condition_stack.len(),
                specpdl_len = self.specpdl.len(),
                has_matching_catch = self.has_active_catch(&throw_on_input),
                "process_quit_flag: throwing for pending input"
            );
            return Err(Flow::throw(throw_on_input, Value::T));
        }

        Err(signal(LispCondition::Quit, vec![]))
    }

    /// GNU `maybe_quit`: promote frontend input for `throw-on-input`, then do
    /// nothing when `quit-flag` is nil or `inhibit-quit` is non-nil;
    /// otherwise process the quit request.
    ///
    /// GNU's low-level input machinery updates `quit-flag` before evaluator
    /// safe points run.  Neomacs receives ordinary frontend events through a
    /// host channel, so this semantic safe point must perform that promotion
    /// itself.  Restrict the channel poll to an active `throw-on-input`
    /// binding; the normal `maybe_quit` hot path remains a flag/atomic check.
    /// The pure fast-path condition of [`Self::maybe_quit`]: true when the
    /// poll would do nothing. Loads only — no mutation, no allocation, no
    /// Lisp — so bytecode dispatch may evaluate it with its operand-stack
    /// cursor still live and only publish for the cold slow path.
    #[inline(always)]
    pub(crate) fn maybe_quit_hot_ok(&self) -> bool {
        !crate::emacs_core::profiler::profiler_sample_due()
            && !crate::emacs_core::os_signal::pending()
            && self.quit_flag.is_nil()
            && !self
                .quit_requested
                .load(std::sync::atomic::Ordering::Relaxed)
            && (self.throw_on_input.is_nil() || !self.has_throw_on_input_poll_source())
    }

    #[inline(always)]
    pub(crate) fn maybe_quit(&mut self) -> Result<(), Flow> {
        // Profiler sampling rides the quit poll (GNU samples in a SIGPROF
        // handler; SIGPROF belongs to the native profiler here, so the Lisp
        // profiler's watchdog raises a flag that this — the canonical safe
        // point every engine polls — consumes). One 'static relaxed load
        // when no profiler runs, replacing the per-call profiler_poll the
        // backtrace push/pop helpers used to pay.
        if crate::emacs_core::profiler::profiler_sample_due() {
            self.profiler_sample_tick();
        }
        // GNU's safe point is `if (!NILP (Vquit_flag) || pending_signals)`
        // (src/lisp.h:3896-3900), so an OS signal costs exactly one more
        // relaxed `'static` load here -- GNU's own hot-path shape and cost.
        if self.quit_flag.is_nil()
            && !crate::emacs_core::os_signal::pending()
            && !self
                .quit_requested
                .load(std::sync::atomic::Ordering::Relaxed)
            && (self.throw_on_input.is_nil() || !self.has_throw_on_input_poll_source())
        {
            return Ok(());
        }
        self.maybe_quit_slow()
    }

    #[cold]
    fn maybe_quit_slow(&mut self) -> Result<(), Flow> {
        if self.has_throw_on_input_poll_source() {
            self.poll_pending_input_for_throw_on_input()?;
        }

        // Drain the cross-thread quit-request atomic into `Vquit_flag`.
        // Set by the input-bridge thread when it observes a `quit-char`
        // keystroke while the evaluator is busy (e.g. deep in bytecode
        // and not reading from `input_rx`). See
        // `Context::quit_requested` for the design rationale.
        if self
            .quit_requested
            .load(std::sync::atomic::Ordering::Relaxed)
            && self
                .quit_requested
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            && self.quit_flag.is_nil()
        {
            self.set_quit_flag_value(Value::T);
        }
        let quit_flag = self.quit_flag;
        if quit_flag.is_nil() || self.inhibit_quit.is_truthy() {
            // GNU's `probably_quit` (src/eval.c:1868-1876):
            //
            //     if (!NILP (Vquit_flag) && NILP (Vinhibit_quit))
            //       process_quit_flag ();
            //     else if (pending_signals)
            //       process_pending_signals ();
            //
            // -- an `else if`, so a pending quit wins and a pending OS signal
            // is handled only when there is none.
            if crate::emacs_core::os_signal::pending() {
                crate::emacs_core::os_signal::drain_pending_os_signals(self);
            }
            return Ok(());
        }

        self.process_quit_flag()
    }

    /// The printed name of `debug-on-event`, or `None` when it does not hold a
    /// symbol.
    ///
    /// GNU's `handle_user_signal` opens with
    /// `if (SYMBOLP (Vdebug_on_event)) special_event_name = SSDATA (SYMBOL_NAME
    /// (Vdebug_on_event));` (src/keyboard.c:8492-8493) and then `strcmp`s it
    /// against the signal's `add_user_signal` NAME, so the comparison really is
    /// on the printed name and a non-symbol really does select no arm.
    pub(crate) fn debug_on_event_signal_name(&self) -> Option<String> {
        let value = self.obarray.symbol_value("debug-on-event").copied()?;
        let name = value.as_symbol_lisp_string()?;
        Some(crate::emacs_core::emacs_char::to_utf8_lossy(
            name.as_bytes(),
        ))
    }

    /// GNU's `handle_user_signal` debugger arm, all four writes
    /// (src/keyboard.c:8500-8506):
    ///
    /// ```c
    ///   /* Enter the debugger in many ways.  */
    ///   debug_on_next_call = true;
    ///   debug_on_quit = true;
    ///   Vquit_flag = Qt;
    ///   Vinhibit_quit = Qnil;
    /// ```
    ///
    /// They are four writes and not one because they cover the three ways the
    /// debugger can be reached: the next call, the quit that is about to be
    /// signalled, and the `inhibit-quit` binding that would otherwise swallow
    /// it.
    pub(crate) fn arm_debugger_for_debug_on_event(&mut self) {
        self.set_variable("debug-on-next-call", Value::T);
        self.set_variable("debug-on-quit", Value::T);
        self.set_variable("inhibit-quit", Value::NIL);
        self.set_quit_flag_value(Value::T);
    }

    #[inline(always)]
    pub(crate) fn quit_flag_value(&self) -> Value {
        self.quit_flag
    }

    /// GNU `QUITP`: true only when a quit is pending and `inhibit-quit` is nil.
    #[inline(always)]
    pub(crate) fn quit_pending(&self) -> bool {
        !self.quit_flag.is_nil() && self.inhibit_quit.is_nil()
    }

    #[inline(always)]
    pub(crate) fn set_quit_flag_value(&mut self, value: Value) {
        self.quit_flag = value;
        self.obarray
            .set_symbol_value_id(self.quit_flag_symbol, value);
    }

    pub(crate) fn quit_char(&self) -> i64 {
        self.quit_char
    }

    pub(crate) fn set_quit_char(&mut self, quit: i64) {
        self.quit_char = quit & 0o377;
    }

    pub(crate) fn event_is_quit_char(&self, event: &Value) -> bool {
        event.as_fixnum() == Some(self.quit_char)
    }

    pub(crate) fn request_quit_from_keyboard_input(&mut self) {
        if self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(Value::T);
        }
    }

    pub(crate) fn clear_quit_flag_after_read_key_sequence_event(&mut self, event: &Value) {
        if !self.event_is_quit_char(event) {
            return;
        }

        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);

        let quit_flag = self.quit_flag_value();
        // while-no-input is active iff `throw-on-input` is non-nil AND the
        // pending quit equals it; in that case leave BOTH the flag and the
        // atomic alone so while-no-input can still bail out.
        let is_while_no_input =
            !throw_on_input.is_nil() && equal_value(&quit_flag, &throw_on_input, 0);

        // GNU `read_char` keyboard.c:2811-2812: when the quit_char is being
        // returned as an ordinary key event, `if (!NILP (Vinhibit_quit))
        // Vquit_flag = Qnil;` — the C-g is consumed as a key, so the pending
        // quit is dropped rather than fired a second time.
        if !quit_flag.is_nil() && !is_while_no_input {
            self.set_quit_flag_value(Value::NIL);
        }

        // The cross-thread `quit_requested` atomic is the neomacs analogue of
        // the same pending C-g (the input bridge sets it in lockstep with
        // queueing the C-g KeyPress, crates/neomacs/src/main.rs:2260/2569). When
        // that very C-g is now consumed as a key here, the atomic MUST be
        // cleared too — otherwise the next `maybe_quit` poll (e.g. inside
        // pre-command-hook or the command dispatch) drains it into `quit-flag`
        // and signals a SECOND, spurious `quit`, pre-empting the
        // `keyboard-quit` command this key is bound to (the "double-quit"
        // bug). This is the root of Finding 3 and lives at the shared
        // key-consumption helper so every read path (channel or unread queue)
        // is covered. Skipped under while-no-input so its throw still fires.
        if !is_while_no_input {
            self.quit_requested
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn input_pending_filter(&self) -> crate::keyboard::InputPendingFilter {
        let configured = self
            .obarray
            .symbol_value("input-pending-p-filter-events")
            .copied()
            .unwrap_or(Value::T)
            .is_truthy();
        crate::keyboard::InputPendingFilter::from_filter_events_variable(configured)
    }

    /// GNU's `track_mouse` (`DEFVAR_LISP`, `src/keyboard.c:14134`), which every
    /// terminal back-end dereferences as a bare global -- `src/term.c:3465`,
    /// `src/androidterm.c:558`, `src/haikuterm.c:425`, `src/w32fns.c:5118`.
    ///
    /// The swap-in makes that global the *current buffer's* binding whenever a
    /// buffer has localised it (dframe, dictionary and gud all do), so the read
    /// names the buffer. Ledger 196.
    pub(crate) fn track_mouse_enabled(&self) -> bool {
        self.obarray
            .value_in_buffer(self.buffers.current_buffer(), "track-mouse")
            .unwrap_or(Value::NIL)
            .is_truthy()
    }

    fn should_ignore_while_no_input_symbol(&self, ignore_symbol: &str) -> bool {
        let ignore_list = self
            .obarray
            .symbol_value("while-no-input-ignore-events")
            .copied()
            .unwrap_or(Value::NIL);
        super::value::list_to_vec(&ignore_list)
            .into_iter()
            .flatten()
            .any(|value| value.is_symbol_named(ignore_symbol))
    }

    pub(crate) fn has_pending_command_input_for_query(&self) -> bool {
        let filter = self.input_pending_filter();
        self.command_loop
            .keyboard
            .has_pending_command_input_for_query(filter, self.track_mouse_enabled(), |symbol| {
                self.should_ignore_while_no_input_symbol(symbol)
            })
    }

    pub(crate) fn has_pending_frontend_input_with_configured_filter(&self) -> bool {
        self.command_loop
            .keyboard
            .pending_input_events
            .has_pending_input(
                crate::keyboard::InputPendingFilter::ConfiguredIgnoreList,
                self.track_mouse_enabled(),
                |symbol| self.should_ignore_while_no_input_symbol(symbol),
            )
    }

    pub(crate) fn open_channel_for_module(&self, process: Value) -> Result<std::ffi::c_int, Flow> {
        self.processes.open_channel_for_module(process)
    }

    #[inline(always)]
    fn has_throw_on_input_poll_source(&self) -> bool {
        // GNU's evaluator-side `maybe_quit` is a cheap flag/signal check; the
        // input path sets `quit-flag` when real keyboard input is available.
        // Neomacs has to poll the host channel for `throw-on-input`, but only
        // when such a channel exists or the command loop is interactive.
        self.input_rx.is_some() || !self.command_loop_noninteractive()
    }

    fn poll_pending_input_for_throw_on_input(&mut self) -> Result<(), Flow> {
        debug_assert!(self.has_throw_on_input_poll_source());

        if self.unwind_cleanup_depth != 0 {
            return Ok(());
        }

        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);
        if throw_on_input.is_nil() {
            return Ok(());
        }

        if !self.quit_flag.is_nil() {
            return Ok(());
        }

        while self.stage_next_host_input_event_if_available()? {}

        self.service_leading_internal_frontend_events();

        if self.has_pending_frontend_input_with_configured_filter() {
            tracing::debug!(
                target: "neomacs::throw_on_input",
                ?throw_on_input,
                condition_stack_len = self.condition_stack.len(),
                specpdl_len = self.specpdl.len(),
                has_matching_catch = self.has_active_catch(&throw_on_input),
                pending_input_events = self.command_loop.keyboard.pending_input_events.len(),
                "poll_pending_input_for_throw_on_input: setting quit-flag"
            );
            self.set_quit_flag_value(throw_on_input);
        }

        Ok(())
    }

    /// Interrupt on input for GNU-style `throw-on-input` users such as
    /// `while-no-input`, while preserving the input event for later reads.
    pub(crate) fn interrupt_for_input_event_if_requested(
        &mut self,
        event: crate::keyboard::InputEvent,
    ) -> Result<bool, Flow> {
        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);
        if throw_on_input.is_nil() {
            return Ok(false);
        }

        if self.inhibit_quit.is_truthy() {
            return Ok(false);
        }

        self.command_loop
            .keyboard
            .pending_input_events
            .push_front(event);
        self.set_quit_flag_value(throw_on_input);
        self.maybe_quit()?;
        Ok(true)
    }

    fn maybe_quit_before_gc(&mut self) -> Result<(), Flow> {
        self.maybe_quit()
    }

    /// Match GNU `eval_sub` / `funcall_general`: quit check first, then GC.
    ///
    /// The remaining evaluator entry points either root their live Values
    /// explicitly or run before materializing heap-backed Values, so this path
    /// now uses exact roots rather than conservative stack scanning.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn maybe_gc_and_quit(&mut self) -> Result<(), Flow> {
        self.maybe_quit_before_gc()?;
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
        Ok(())
    }

    /// Match GNU `bytecode.c:op_branch`: after the bytecode loop's unsigned
    /// quit counter wraps, run `maybe_gc (); maybe_quit ();`.
    pub(crate) fn bytecode_branch_maybe_gc_and_quit(&mut self) -> Result<(), Flow> {
        #[cfg(test)]
        BYTECODE_BRANCH_POLL_COUNT.with(|count| count.set(count.get() + 1));
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
        // Concurrent root feeding: young data reachable only from the
        // operand stacks (a loop building a list) is otherwise invisible to
        // the concurrent marker until the STW termination fold — which then
        // pays a full young-generation mark as pause. Both tiers funnel
        // through here (the interpreter's backward branch and the native
        // loop's `neovm_jit_backedge`, whose shims have already spilled live
        // values to `bc_buf` / the root window), once per ~256 iterations.
        if crate::tagged::gc::concurrent_mark_active() {
            crate::tagged::gc::feed_concurrent_roots(&self.bc_buf);
            crate::tagged::gc::feed_concurrent_roots(
                &self.jit_root_stack[..self.jit_root_stack_top],
            );
        }
        self.maybe_quit()
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
        stacker::maybe_grow(EVAL_STACK_RED_ZONE, EVAL_STACK_SEGMENT, || callback(self))
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

    // -----------------------------------------------------------------------
    // Special forms
    // -----------------------------------------------------------------------

    fn try_special_form_value_id(&mut self, sym_id: SymId, tail: Value) -> Option<EvalResult> {
        self.try_special_form_with_surface(sym_id, sym_id, tail)
    }

    fn try_aliased_special_form_value_id(
        &mut self,
        surface_id: SymId,
        target_id: SymId,
        tail: Value,
    ) -> Option<EvalResult> {
        self.try_special_form_with_surface(surface_id, target_id, tail)
    }

    /// The single special-form dispatch table. `target_id` selects the form
    /// (already resolved through any defalias chain); `surface_id` is the
    /// symbol the source form actually named, threaded into the forms whose
    /// errors report a call name — GNU signals with the surface symbol
    /// object itself, so an uninterned alias keeps its identity. The direct
    /// (non-aliased) path passes the same id for both.
    fn try_special_form_with_surface(
        &mut self,
        surface_id: SymId,
        target_id: SymId,
        tail: Value,
    ) -> Option<EvalResult> {
        let saved_depth = self.depth;
        let result = match evaluator_handler(target_id) {
            Some(EvaluatorHandler::SpecialForm(handler)) => Some(match handler {
                // Forms that report the surface name in their signals.
                SpecialFormHandler::Quote => self.sf_quote_value_named(surface_id, tail),
                SpecialFormHandler::Function => self.sf_function_value_named(surface_id, tail),
                SpecialFormHandler::Let => self.sf_let_value_named(surface_id, tail),
                SpecialFormHandler::LetStar => self.sf_let_star_value_named(surface_id, tail),
                SpecialFormHandler::Setq => self.sf_setq_value_named(surface_id, tail),
                SpecialFormHandler::If => self.sf_if_value_named(surface_id, tail),
                SpecialFormHandler::While => self.sf_while_value_named(surface_id, tail),
                SpecialFormHandler::Prog1 => self.sf_prog1_value_named(surface_id, tail),
                SpecialFormHandler::Defvar => self.sf_defvar_value_named(surface_id, tail),
                SpecialFormHandler::Defconst => self.sf_defconst_value_named(surface_id, tail),
                SpecialFormHandler::Catch => self.sf_catch_value_named(surface_id, tail),
                SpecialFormHandler::UnwindProtect => {
                    self.sf_unwind_protect_value_named(surface_id, tail)
                }
                SpecialFormHandler::ConditionCase => {
                    self.sf_condition_case_value_named(surface_id, tail)
                }
                // Forms whose signals never carry a call name.
                SpecialFormHandler::And => self.sf_and_value(tail),
                SpecialFormHandler::Or => self.sf_or_value(tail),
                SpecialFormHandler::Cond => self.sf_cond_value(tail),
                SpecialFormHandler::Progn => self.sf_progn_value(tail),
                SpecialFormHandler::SaveExcursion => self.sf_save_excursion_value(tail),
                SpecialFormHandler::SaveCurrentBuffer => self.sf_save_current_buffer_value(tail),
                SpecialFormHandler::SaveRestriction => self.sf_save_restriction_value(tail),
                SpecialFormHandler::Interactive => Ok(Value::NIL),
            }),
            Some(EvaluatorHandler::Callable(_)) => None,
            None => match target_id {
                // These evaluator-internal forms have no public subr
                // declaration and are reachable only by their canonical name.
                id if id == lambda_symbol() && surface_id == target_id => {
                    Some(self.sf_lambda_value(tail))
                }
                id if id == byte_code_literal_symbol() && surface_id == target_id => {
                    Some(self.sf_byte_code_literal_value(tail))
                }
                id if id == byte_code_symbol() && surface_id == target_id => {
                    Some(self.sf_byte_code_value(tail))
                }
                _ => None,
            },
        };
        self.depth = saved_depth;
        result
    }

    fn listp_error(&self, value: Value) -> Flow {
        // GNU `CHECK_LIST` walks the cdr chain until it finds the
        // non-cons tail and signals
        // `(wrong-type-argument listp TAIL)` with the offending
        // tail element, not the whole input. Verified against
        // emacs 31.0.50 via:
        //   (condition-case e (length '(1 . 2)) (error e))
        //     -> (wrong-type-argument listp 2)
        //   (condition-case e (let ((x 1) . 2) x) (error e))
        //     -> (wrong-type-argument listp 2)
        let mut tail = value;
        while tail.is_cons() {
            tail = tail.cons_cdr();
        }
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        )
    }

    fn value_list_len_or_error(&self, list: Value) -> Result<usize, Flow> {
        list_length(&list).ok_or_else(|| self.listp_error(list))
    }

    fn one_unevalled_arg(&self, name: SymId, tail: Value) -> Result<Value, Flow> {
        let mut cursor = tail;
        if !cursor.is_cons() {
            return if cursor.is_nil() {
                Err(signal(
                    LispCondition::WrongNumberOfArguments,
                    vec![Value::from_sym_id(name), Value::fixnum(0)],
                ))
            } else {
                Err(self.listp_error(tail))
            };
        }
        let arg = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if !cursor.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![
                    Value::from_sym_id(name),
                    Value::fixnum(self.value_list_len_or_error(tail)? as i64),
                ],
            ));
        }
        Ok(arg)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_quote_value(&mut self, tail: Value) -> EvalResult {
        self.sf_quote_value_named(quote_symbol(), tail)
    }

    fn sf_quote_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        self.one_unevalled_arg(call_name, tail)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_function_value(&mut self, tail: Value) -> EvalResult {
        self.sf_function_value_named(function_symbol(), tail)
    }

    fn sf_function_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        let arg = self.one_unevalled_arg(call_name, tail)?;
        if cons_head_symbol_id(&arg) == Some(lambda_symbol()) {
            return self.instantiate_callable_cons_form(arg);
        }
        Ok(arg)
    }

    fn sf_lambda_value(&mut self, tail: Value) -> EvalResult {
        self.instantiate_callable_cons_form(Value::cons(Value::from_sym_id(lambda_symbol()), tail))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_let_value(&mut self, tail: Value) -> EvalResult {
        self.sf_let_value_named(let_symbol(), tail)
    }

    fn sf_let_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }

        let varlist = tail.cons_car();
        let body = tail.cons_cdr();
        let mut lexical_bindings = LetBindingVec::new();
        let mut dynamic_sym_ids = LetBindingVec::new();
        let use_lexical = self.lexical_binding();
        let mut constant_binding_error: Option<String> = None;
        let specpdl_root_scope = self.save_specpdl_roots();
        let mut bindings = varlist;

        while bindings.is_cons() {
            let binding = self.unwrap_symbol(bindings.cons_car());
            bindings = bindings.cons_cdr();
            if let Some(id) = binding.as_symbol_id() {
                // A bare binder binds nil, which is never a keyword's own value.
                if let Some(name) = let_constant_error_name(&self.obarray, id, Value::NIL) {
                    if constant_binding_error.is_none() {
                        constant_binding_error = Some(name);
                    }
                    continue;
                }
                if use_lexical
                    && !self.obarray.is_special_id(id)
                    && !self.lexenv_declares_special_cached_in(self.lexenv, id)
                {
                    lexical_bindings.push((id, Value::NIL));
                } else {
                    dynamic_sym_ids.push((id, Value::NIL));
                }
                continue;
            }
            if !binding.is_cons() {
                self.restore_specpdl_roots(specpdl_root_scope);
                // GNU takes `(car elt)` of a non-symbol binding, so a non-list
                // element signals `(wrong-type-argument listp ELT)`.
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), binding],
                ));
            }
            let head = self.unwrap_symbol(binding.cons_car());
            let Some(id) = head.as_symbol_id() else {
                self.restore_specpdl_roots(specpdl_root_scope);
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), head],
                ));
            };
            let mut value_tail = binding.cons_cdr();
            let value = if value_tail.is_nil() {
                Value::NIL
            } else if value_tail.is_cons() {
                let init_form = value_tail.cons_car();
                value_tail = value_tail.cons_cdr();
                if !value_tail.is_nil() {
                    self.restore_specpdl_roots(specpdl_root_scope);
                    return Err(signal(
                        "error",
                        vec![
                            Value::string("`let' bindings can have only one value-form"),
                            binding,
                        ],
                    ));
                }
                match self.eval_sub(init_form) {
                    Ok(value) => value,
                    Err(err) => {
                        self.restore_specpdl_roots(specpdl_root_scope);
                        return Err(err);
                    }
                }
            } else {
                self.restore_specpdl_roots(specpdl_root_scope);
                return Err(self.listp_error(binding));
            };
            self.push_specpdl_root(value);
            if let Some(name) = let_constant_error_name(&self.obarray, id, value) {
                if constant_binding_error.is_none() {
                    constant_binding_error = Some(name);
                }
                continue;
            }
            if use_lexical
                && !self.obarray.is_special_id(id)
                && !self.lexenv_declares_special_cached_in(self.lexenv, id)
            {
                lexical_bindings.push((id, value));
            } else {
                dynamic_sym_ids.push((id, value));
            }
        }
        if !bindings.is_nil() {
            self.restore_specpdl_roots(specpdl_root_scope);
            return Err(self.listp_error(varlist));
        }
        if let Some(name) = constant_binding_error {
            self.restore_specpdl_roots(specpdl_root_scope);
            return Err(signal(
                LispCondition::SettingConstant,
                vec![Value::symbol(name)],
            ));
        }

        // CRITICAL: Restore specpdl roots (drop init-form GcRoot entries) BEFORE
        // pushing LexicalEnv/Let entries. Otherwise `restore_specpdl_roots`
        // drains from `saved_len` and re-extends with non-GcRoot entries,
        // MOVING our LexicalEnv to a lower index. Then `unbind_to(specpdl_count)`
        // becomes a no-op because specpdl.len() already matches, and the stale
        // LexicalEnv leaks below. This caused lexical binding leaks — closures
        // created in the body captured oversized environments.
        self.restore_specpdl_roots(specpdl_root_scope);

        // Save lexenv AFTER init forms run (matches GNU eval.c:1167:
        //   `lexenv = Vinternal_interpreter_environment;`).
        // Capture specpdl_count AFTER restoring so LexicalEnv sits exactly at
        // specpdl[specpdl_count] and unbind_to will pop it.
        let lexenv_at_entry = self.lexenv;
        let specpdl_count = self.specpdl.len();

        // Always save the entry-point lexenv on the specpdl when in lexical
        // mode, so unbind_to restores it regardless of what the body does.
        // Matches GNU's specbind(Qinternal_interpreter_environment).
        if use_lexical {
            self.specpdl.push(SpecBinding::LexicalEnv {
                old_lexenv: lexenv_at_entry,
            });
        }

        // Build new lexenv locally by consing bindings onto the ENTRY-POINT
        // lexenv (not self.lexenv which may have been modified by init forms).
        // Matches GNU eval.c:1167-1186.
        let mut new_lexenv = lexenv_at_entry;
        for (sym_id, val) in &lexical_bindings {
            let binding_pair = Value::make_cons(
                crate::emacs_core::eval::lexenv_binding_symbol_value(*sym_id),
                *val,
            );
            self.specpdl.push(SpecBinding::GcRoot {
                value: binding_pair,
            });
            new_lexenv = Value::make_cons(binding_pair, new_lexenv);
            match self.specpdl.last_mut() {
                Some(SpecBinding::GcRoot { value }) => *value = new_lexenv,
                _ => unreachable!(),
            }
        }
        // Install the new lexenv atomically.
        self.lexenv = new_lexenv;

        let temp_scope = self.save_eval_temp_roots();
        for (_, value) in lexical_bindings.iter().chain(dynamic_sym_ids.iter()) {
            self.push_eval_temp_root(*value);
        }
        for (sym_id, value) in &dynamic_sym_ids {
            if let Err(flow) = self.try_specbind(*sym_id, *value) {
                let result = self.unbind_to_with_result(specpdl_count, Err(flow));
                self.restore_eval_temp_roots_to_sequence(temp_scope);
                return result;
            }
        }

        let result = self.sf_progn_value(body);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_eval_temp_roots_to_sequence(temp_scope);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_let_star_value(&mut self, tail: Value) -> EvalResult {
        self.sf_let_star_value_named(let_star_symbol(), tail)
    }

    fn sf_let_star_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }

        let varlist = tail.cons_car();
        let body = tail.cons_cdr();
        let use_lexical = self.lexical_binding();
        let specpdl_count = self.specpdl.len();
        // Mirrors GNU Flet_star: specbind(Qinternal_interpreter_environment, lexenv)
        // before any per-variable specbinds. unbind_to pops everything.
        if use_lexical {
            self.specpdl.push(SpecBinding::LexicalEnv {
                old_lexenv: self.lexenv,
            });
        }

        let temp_scope = self.save_eval_temp_roots();
        let val_temp_slot = self.push_eval_temp_root_slot(Value::NIL);
        let init_result: Result<(), Flow> = (|| {
            let mut bindings = varlist;
            while bindings.is_cons() {
                let binding = self.unwrap_symbol(bindings.cons_car());
                bindings = bindings.cons_cdr();
                let (id, value) = if let Some(id) = binding.as_symbol_id() {
                    (id, Value::NIL)
                } else if binding.is_cons() {
                    let head = self.unwrap_symbol(binding.cons_car());
                    let Some(id) = head.as_symbol_id() else {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("symbolp"), head],
                        ));
                    };
                    let mut value_tail = binding.cons_cdr();
                    let value = if value_tail.is_nil() {
                        Value::NIL
                    } else if value_tail.is_cons() {
                        let init_form = value_tail.cons_car();
                        value_tail = value_tail.cons_cdr();
                        if !value_tail.is_nil() {
                            return Err(signal(
                                "error",
                                vec![
                                    Value::string("`let' bindings can have only one value-form"),
                                    binding,
                                ],
                            ));
                        }
                        self.eval_sub(init_form)?
                    } else {
                        return Err(self.listp_error(binding));
                    };
                    (id, value)
                } else {
                    // GNU takes `(car elt)`, so a non-list element signals
                    // `(wrong-type-argument listp ELT)`.
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("listp"), binding],
                    ));
                };
                self.set_eval_temp_root_slot(val_temp_slot, value);

                if let Some(name) = let_constant_error_name(&self.obarray, id, value) {
                    return Err(signal(
                        LispCondition::SettingConstant,
                        vec![Value::symbol(&name)],
                    ));
                }
                if use_lexical
                    && !self.obarray.is_special_id(id)
                    && !self.lexenv_declares_special_cached_in(self.lexenv, id)
                {
                    // Matches GNU Flet_star (eval.c:1113-1120):
                    // Direct cons onto Vinternal_interpreter_environment.
                    // The LexicalEnv entry at specpdl_count saves the pre-let*
                    // state; unbind_to restores it.
                    let binding = Value::make_cons(lexenv_binding_symbol_value(id), value);
                    self.lexenv = Value::make_cons(binding, self.lexenv);
                } else {
                    self.try_specbind(id, value)?;
                }
            }
            if !bindings.is_nil() {
                return Err(self.listp_error(varlist));
            }
            Ok(())
        })();
        if let Err(error) = init_result {
            let result = self.unbind_to_with_result(specpdl_count, Err(error));
            self.restore_eval_temp_roots_to_sequence(temp_scope);
            return result;
        }

        let result = self.sf_progn_value(body);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_eval_temp_roots_to_sequence(temp_scope);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_setq_value(&mut self, tail: Value) -> EvalResult {
        self.sf_setq_value_named(setq_symbol(), tail)
    }

    fn sf_setq_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Ok(Value::NIL);
        }
        let mut cursor = tail;
        let mut last = Value::NIL;
        let mut nargs: usize = 0;
        while cursor.is_cons() {
            let symbol = cursor.cons_car();
            cursor = cursor.cons_cdr();
            nargs += 1;
            if cursor.is_nil() {
                return Err(signal(
                    LispCondition::WrongNumberOfArguments,
                    vec![Value::from_sym_id(call_name), Value::fixnum(nargs as i64)],
                ));
            }
            if !cursor.is_cons() {
                return Err(self.listp_error(tail));
            }
            let value_form = cursor.cons_car();
            cursor = cursor.cons_cdr();
            nargs += 1;
            let symbol = self.unwrap_symbol(symbol);
            let Some(sym_id) = symbol.as_symbol_id() else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), symbol],
                ));
            };
            let value = self.eval_sub(value_form)?;
            // Debug probe for multibyte assignments to default-directory.
            // Kept at debug level so it doesn't pollute normal error
            // output (Doom always fires this with pure-ASCII paths that
            // happen to carry the multibyte flag from string decoding).
            if sym_id == default_directory_symbol()
                && value.is_string()
                && value.string_is_multibyte()
            {
                tracing::debug!(
                    "SETQ default-directory to MULTIBYTE string: {:?}",
                    value
                        .as_lisp_string()
                        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                        .unwrap_or_default(),
                );
            }
            self.assign_setq_by_id(sym_id, value)?;
            last = value;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(last)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_if_value(&mut self, tail: Value) -> EvalResult {
        self.sf_if_value_named(if_symbol(), tail)
    }

    fn sf_if_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let cond_form = tail.cons_car();
        let mut rest = tail.cons_cdr();
        if rest.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(1)],
            ));
        }
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let then_form = rest.cons_car();
        rest = rest.cons_cdr();
        if self.eval_sub(cond_form)?.is_truthy() {
            self.eval_sub(then_form)
        } else {
            self.sf_progn_value(rest)
        }
    }

    fn sf_and_value(&mut self, tail: Value) -> EvalResult {
        let mut cursor = tail;
        let mut last = Value::T;
        while cursor.is_cons() {
            last = self.eval_sub(cursor.cons_car())?;
            if last.is_nil() {
                return Ok(Value::NIL);
            }
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(last)
    }

    fn sf_or_value(&mut self, tail: Value) -> EvalResult {
        let mut cursor = tail;
        while cursor.is_cons() {
            let value = self.eval_sub(cursor.cons_car())?;
            if value.is_truthy() {
                return Ok(value);
            }
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(Value::NIL)
    }

    fn sf_cond_value(&mut self, tail: Value) -> EvalResult {
        let mut clauses = tail;
        while clauses.is_cons() {
            let clause = clauses.cons_car();
            clauses = clauses.cons_cdr();
            if clause.is_nil() {
                continue;
            }
            if !clause.is_cons() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), clause],
                ));
            }
            let test = clause.cons_car();
            let body = clause.cons_cdr();
            let test_value = self.eval_sub(test)?;
            if test_value.is_truthy() {
                if body.is_nil() {
                    return Ok(test_value);
                }
                return self.sf_progn_value(body);
            }
        }
        if !clauses.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(Value::NIL)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_while_value(&mut self, tail: Value) -> EvalResult {
        self.sf_while_value_named(while_symbol(), tail)
    }

    fn sf_while_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let test_form = tail.cons_car();
        let body = tail.cons_cdr();
        let mut iters: u64 = 0;
        loop {
            if self.eval_sub(test_form)?.is_nil() {
                return Ok(Value::NIL);
            }
            self.sf_progn_value(body)?;
            iters += 1;
            if iters == 1_000_000 {
                let cond_str = super::print::print_value(&test_form);
                tracing::warn!(
                    "while loop exceeded 1M iterations, cond: {}",
                    &cond_str[..cond_str.len().min(300)]
                );
            }
            self.maybe_quit()?;
        }
    }

    fn sf_progn_value(&mut self, forms: Value) -> EvalResult {
        let temp_scope = self.save_sequence_temp_roots();
        let result = (|| {
            let mut cursor = forms;
            let mut last = Value::NIL;
            while cursor.is_cons() {
                match self.eval_sub(cursor.cons_car()) {
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
            if !cursor.is_nil() {
                return Err(self.listp_error(forms));
            }
            Ok(last)
        })();
        self.restore_sequence_temp_roots(temp_scope);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_prog1_value(&mut self, tail: Value) -> EvalResult {
        self.sf_prog1_value_named(prog1_symbol(), tail)
    }

    fn sf_prog1_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let first_form = tail.cons_car();
        let rest = tail.cons_cdr();
        let first = self.eval_sub(first_form)?;
        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(first);
        let result = self.sf_progn_value(rest);
        self.restore_specpdl_roots(specpdl_root_scope);
        result?;
        Ok(first)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_defvar_value(&mut self, tail: Value) -> EvalResult {
        self.sf_defvar_value_named(defvar_symbol(), tail)
    }

    fn sf_defvar_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }

        let symbol = self.unwrap_symbol(tail.cons_car());
        let Some(sym_id) = symbol.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), symbol],
            ));
        };
        let mut rest = tail.cons_cdr();

        if rest.is_nil() {
            if self.lexical_binding()
                && !self.lexenv.is_nil()
                && !self.obarray.is_special_id(sym_id)
            {
                self.lexenv = Value::cons(Value::from_sym_id(sym_id), self.lexenv);
            }
            return Ok(Value::from_sym_id(sym_id));
        }
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let init_form = rest.cons_car();
        rest = rest.cons_cdr();
        let documentation = if rest.is_nil() {
            Value::NIL
        } else if rest.is_cons() {
            let doc = rest.cons_car();
            rest = rest.cons_cdr();
            if !rest.is_nil() {
                return Err(signal("error", vec![Value::string("Too many arguments")]));
            }
            doc
        } else {
            return Err(self.listp_error(tail));
        };

        let mut define_args = vec![symbol];
        if !documentation.is_nil() {
            define_args.push(documentation);
        }
        super::builtins::symbols::builtin_internal_define_uninitialized_variable(
            self,
            define_args,
        )?;

        let was_bound = default_toplevel_value_in_state(
            &self.obarray,
            self.specpdl.as_slice(),
            Some(&self.buffers.buffer_defaults),
            sym_id,
        )
        .is_some()
            || self.obarray.is_constant_id(sym_id);
        if !was_bound {
            let value = self.eval_sub(init_form)?;
            set_default_toplevel_value(self, vec![symbol, value])?;
        }

        Ok(Value::from_sym_id(sym_id))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_defconst_value(&mut self, tail: Value) -> EvalResult {
        self.sf_defconst_value_named(defconst_symbol(), tail)
    }

    fn sf_defconst_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let symbol = self.unwrap_symbol(tail.cons_car());
        let Some(sym_id) = symbol.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), symbol],
            ));
        };
        let mut rest = tail.cons_cdr();
        if rest.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(1)],
            ));
        }
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let init_form = rest.cons_car();
        rest = rest.cons_cdr();
        let documentation = if rest.is_nil() {
            Value::NIL
        } else if rest.is_cons() {
            let doc = rest.cons_car();
            rest = rest.cons_cdr();
            if !rest.is_nil() {
                return Err(signal("error", vec![Value::string("Too many arguments")]));
            }
            doc
        } else {
            return Err(self.listp_error(tail));
        };

        let mut define_args = vec![symbol];
        if !documentation.is_nil() {
            define_args.push(documentation);
        }
        super::builtins::symbols::builtin_internal_define_uninitialized_variable(
            self,
            define_args,
        )?;

        let value = self.eval_sub(init_form)?;
        super::data::set_default(self, vec![symbol, value])?;
        self.obarray.make_special_id(sym_id);
        self.obarray
            .put_property_id(sym_id, intern("risky-local-variable"), Value::T)?;
        Ok(Value::from_sym_id(sym_id))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_catch_value(&mut self, tail: Value) -> EvalResult {
        self.sf_catch_value_named(catch_symbol(), tail)
    }

    fn sf_catch_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let tag = self.eval_sub(tail.cons_car())?;
        self.push_condition_frame(ConditionFrame::Catch {
            tag,
            resume: ResumeTarget::InterpreterCatch,
        });
        let specpdl_count = self.specpdl.len();
        let result = match self.sf_progn_value(tail.cons_cdr()) {
            Ok(value) => Ok(value),
            Err(Flow::Signal(sig)) => match self.dispatch_signal_if_needed(sig) {
                Ok(dispatched) => Err(Flow::Signal(dispatched)),
                Err(Flow::Throw(thrown)) if eq_value(&tag, &thrown.tag) => Ok(thrown.value),
                Err(flow) => Err(flow),
            },
            Err(Flow::Throw(thrown)) if eq_value(&tag, &thrown.tag) => Ok(thrown.value),
            Err(flow) => Err(flow),
        };
        self.pop_condition_frame();
        // Catching moves the value out of pinned ThrowData. Carry it through
        // all cleanup so it stays rooted and a cleanup nonlocal exit can
        // replace it, matching GNU's `unbind_to (count, value)`.
        self.unbind_to_with_result(specpdl_count, result)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_unwind_protect_value(&mut self, tail: Value) -> EvalResult {
        self.sf_unwind_protect_value_named(unwind_protect_symbol(), tail)
    }

    fn sf_unwind_protect_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        // GNU eval.c:1461 declares `unwind-protect` with min_args=1.
        // The generic arity check in GNU `eval_sub` (eval.c:2612) runs
        // for every SUBRP including UNEVALLED. Neomacs skips that check
        // for special forms (dispatch_kind != SpecialForm at
        // eval.rs:6599) so each special form validates itself -- see
        // `sf_condition_case_value_named`.
        let nargs = self.value_list_len_or_error(tail)?;
        if nargs < 1 {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(nargs as i64)],
            ));
        }
        let body = tail.cons_car();
        let cleanup_forms = tail.cons_cdr();
        let specpdl_count = self.specpdl.len();
        self.specpdl.push(SpecBinding::UnwindProtect {
            forms: cleanup_forms,
            lexenv: self.lexenv,
        });
        let result = self.eval_sub(body);
        self.unbind_to_with_result(specpdl_count, result)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sf_condition_case_value(&mut self, tail: Value) -> EvalResult {
        self.sf_condition_case_value_named(condition_case_symbol(), tail)
    }

    fn sf_condition_case_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        let nargs = self.value_list_len_or_error(tail)?;
        if nargs < 2 {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(nargs as i64)],
            ));
        }
        let var = self.unwrap_symbol(tail.cons_car());
        let Some(var_id) = var.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), var],
            ));
        };
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let body = rest.cons_car();
        let handlers = rest.cons_cdr();

        let mut handlers_vec = Vec::new();
        let mut success_handler_idx: Option<usize> = None;
        let mut cursor = handlers;
        while cursor.is_cons() {
            let handler = cursor.cons_car();
            let handler_index = handlers_vec.len();
            handlers_vec.push(handler);
            cursor = cursor.cons_cdr();
            if handler.is_nil() {
                continue;
            }
            if !handler.is_cons() {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid condition handler: {}",
                        super::print::print_value(&handler)
                    ))],
                ));
            }
            let head = handler.cons_car();
            if !(head.is_symbol() || head.is_symbol_with_pos() || head.is_cons()) {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid condition handler: {}",
                        super::print::print_value(&handler)
                    ))],
                ));
            }
            let head_unwrapped = self.unwrap_symbol(head);
            if head_unwrapped.is_symbol_named(":success") {
                success_handler_idx = Some(handler_index);
            }
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(handlers));
        }

        self.run_condition_case_body(var, var_id, &handlers_vec, success_handler_idx, |ctx| {
            ctx.eval_sub(body)
        })
    }

    fn run_condition_case_body(
        &mut self,
        var: Value,
        var_id: SymId,
        handlers_vec: &[Value],
        success_handler_idx: Option<usize>,
        eval_body: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        let condition_stack_base = self.condition_stack_len();
        for (idx, handler) in handlers_vec.iter().enumerate().rev() {
            if success_handler_idx == Some(idx) || handler.is_nil() {
                continue;
            }
            if !handler.is_cons() {
                continue;
            }
            let conditions = handler.cons_car();
            self.push_condition_frame(ConditionFrame::ConditionCase {
                conditions,
                resume: ResumeTarget::InterpreterConditionCase {
                    handler_index: idx,
                    condition_stack_base,
                },
            });
        }

        match eval_body(self) {
            Ok(value) => {
                self.truncate_condition_stack(condition_stack_base);
                if let Some(idx) = success_handler_idx {
                    let handler = handlers_vec[idx];
                    let bind_var = !var.is_nil();
                    // Mirror the error-handler arm: bind VAR lexically when
                    // lexical binding is in effect and VAR is not special, else
                    // dynamically (GNU condition-case binds the :success var the
                    // same way it binds an error handler's var).
                    let use_lexical_binding = bind_var
                        && self.lexical_binding()
                        && !is_runtime_dynamically_special(&self.obarray, var_id)
                        && !self.lexenv_declares_special_cached_in(self.lexenv, var_id);
                    let specpdl_count = self.specpdl.len();
                    if use_lexical_binding {
                        self.specpdl.push(SpecBinding::LexicalEnv {
                            old_lexenv: self.lexenv,
                        });
                        let binding = Value::make_cons(lexenv_binding_symbol_value(var_id), value);
                        self.lexenv = Value::make_cons(binding, self.lexenv);
                    } else if bind_var {
                        if let Err(flow) = self.try_specbind(var_id, value) {
                            return self.unbind_to_with_result(specpdl_count, Err(flow));
                        }
                    }
                    let result = self.sf_progn_value(handler.cons_cdr());
                    return self.unbind_to_with_result(specpdl_count, result);
                }
                Ok(value)
            }
            Err(Flow::Signal(sig)) => {
                let sig = match self.dispatch_signal_if_needed(sig) {
                    Ok(dispatched) => dispatched,
                    Err(flow) => {
                        self.truncate_condition_stack(condition_stack_base);
                        return Err(flow);
                    }
                };
                self.truncate_condition_stack(condition_stack_base);
                if let Some(ResumeTarget::InterpreterConditionCase {
                    handler_index,
                    condition_stack_base: selected_stack_base,
                }) = sig.selected_resume.clone()
                    && selected_stack_base == condition_stack_base
                {
                    let handler = handlers_vec[handler_index];
                    let bind_var = !var.is_nil();
                    let binding_value = make_signal_binding_value(&sig);
                    let use_lexical_binding = bind_var
                        && self.lexical_binding()
                        && !is_runtime_dynamically_special(&self.obarray, var_id)
                        && !self.lexenv_declares_special_cached_in(self.lexenv, var_id);

                    let specpdl_count = self.specpdl.len();
                    if use_lexical_binding {
                        // Match GNU: specbind the lexenv, then cons the
                        // binding directly.
                        self.specpdl.push(SpecBinding::LexicalEnv {
                            old_lexenv: self.lexenv,
                        });
                        let binding =
                            Value::make_cons(lexenv_binding_symbol_value(var_id), binding_value);
                        self.lexenv = Value::make_cons(binding, self.lexenv);
                    } else if bind_var {
                        if let Err(flow) = self.try_specbind(var_id, binding_value) {
                            return self.unbind_to_with_result(specpdl_count, Err(flow));
                        }
                    }
                    let result = self.sf_progn_value(handler.cons_cdr());
                    return self.unbind_to_with_result(specpdl_count, result);
                }
                Err(Flow::Signal(sig))
            }
            Err(flow @ Flow::ThreadBlocked(_)) => {
                self.truncate_condition_stack(condition_stack_base);
                if let Flow::ThreadBlocked(ref blocked) = flow
                    && !blocked.remaining_forms.is_nil()
                {
                    return Err(Flow::thread_blocked(
                        blocked.blocker,
                        crate::emacs_core::threads::make_thread_condition_case_continuation(
                            var,
                            blocked.remaining_forms,
                            Value::list(handlers_vec.to_vec()),
                            self.lexenv,
                        ),
                    ));
                }
                Err(flow)
            }
            // A shutdown is not a condition: condition-case cannot handle it,
            // matching GNU, where Fkill_emacs exits and no handler ever runs.
            Err(flow @ (Flow::Throw(_) | Flow::Shutdown(_))) => {
                self.truncate_condition_stack(condition_stack_base);
                Err(flow)
            }
        }
    }

    pub(crate) fn resume_thread_condition_case_continuation(
        &mut self,
        var: Value,
        body: Value,
        handlers: Value,
        lexenv: Value,
    ) -> EvalResult {
        let Some(var_id) = var.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), var],
            ));
        };
        let Some(handlers_vec) = list_to_vec(&handlers) else {
            return Err(self.listp_error(handlers));
        };
        let success_handler_idx = handlers_vec.iter().position(|handler| {
            handler.is_cons()
                && self
                    .unwrap_symbol(handler.cons_car())
                    .is_symbol_named(":success")
        });

        let specpdl_count = self.specpdl.len();
        self.specpdl.push(SpecBinding::LexicalEnv {
            old_lexenv: self.lexenv,
        });
        self.lexenv = lexenv;
        let thread_id = self.threads.current_thread_id();
        let pending = self.threads.take_pending_thread_signal(thread_id);
        let result =
            self.run_condition_case_body(var, var_id, &handlers_vec, success_handler_idx, |ctx| {
                if let Some(flow) = pending {
                    Err(flow)
                } else {
                    ctx.sf_progn_value(body)
                }
            });
        self.unbind_to_with_result(specpdl_count, result)
    }

    fn sf_save_excursion_value(&mut self, tail: Value) -> EvalResult {
        let count = self.specpdl.len();
        self.record_save_excursion();
        let result = self.sf_progn_value(tail);
        self.unbind_to_with_result(count, result)
    }

    fn sf_save_current_buffer_value(&mut self, tail: Value) -> EvalResult {
        // Specpdl-carried like the VM arm and GNU's
        // record_unwind_current_buffer, so a panic contained at a module/JIT
        // boundary inside the body restores the buffer via the boundary
        // unwind (an imperative restore here would be skipped, leaving the
        // wrong buffer current). PS fix-wave sweep hit.
        let count = self.specpdl.len();
        if let Some(buf) = self.buffers.current_buffer() {
            self.specpdl
                .push(SpecBinding::SaveCurrentBuffer { buffer_id: buf.id });
        }
        let result = self.sf_progn_value(tail);
        self.unbind_to_with_result(count, result)
    }

    fn sf_save_restriction_value(&mut self, tail: Value) -> EvalResult {
        let count = self.specpdl.len();
        if let Some(state) = self.buffers.save_current_restriction_state() {
            self.specpdl.push(SpecBinding::save_restriction(state));
        }
        let result = self.sf_progn_value(tail);
        self.unbind_to_with_result(count, result)
    }

    fn validate_throw(&self, flow: Flow) -> Flow {
        match flow {
            Flow::Throw(ref thrown) => {
                if self.has_active_catch(&thrown.tag) {
                    flow
                } else {
                    signal(LispCondition::NoCatch, vec![thrown.tag, thrown.value])
                }
            }
            other => other,
        }
    }

    /// Recursively walk a `Value`, treating everything as literal data
    /// except `(byte-code-literal ...)` cons cells which are converted to
    /// `Value::ByteCode` via `sf_byte_code_literal_value`.
    fn quote_value_with_bytecode(&mut self, value: Value) -> EvalResult {
        if value.is_cons() && cons_head_symbol_id(&value) == Some(byte_code_literal_symbol()) {
            return self.sf_byte_code_literal_value(value.cons_cdr());
        }

        match value.kind() {
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = value.as_vector_data().unwrap();
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.quote_value_with_bytecode(*item)?);
                }
                Ok(Value::vector(values))
            }
            _ => Ok(value),
        }
    }

    fn sf_byte_code_literal_value(&mut self, tail: Value) -> EvalResult {
        let vector = self.one_unevalled_arg(byte_code_literal_symbol(), tail)?;
        let Some(items) = vector.as_vector_data() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("vectorp"), vector],
            ));
        };

        if items.len() < 4 {
            return Ok(vector);
        }

        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.quote_value_with_bytecode(*item)?);
        }

        crate::emacs_core::builtins::make_byte_code_from_slots(&values)
    }

    fn sf_byte_code_value(&mut self, tail: Value) -> EvalResult {
        let args = list_to_vec(&tail).ok_or_else(|| self.listp_error(tail))?;
        if args.len() != 3 {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::symbol("byte-code"), Value::fixnum(args.len() as i64)],
            ));
        }
        let trace_toplevel_bytecode = std::env::var_os("NEOVM_TRACE_TOPLEVEL_BYTECODE").is_some();
        let load_file_name = if trace_toplevel_bytecode {
            self.obarray()
                .symbol_value("load-file-name")
                .and_then(|value| {
                    value
                        .as_lisp_string()
                        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                })
                .unwrap_or_else(|| "<unknown>".to_string())
        } else {
            String::new()
        };
        let decode_start = trace_toplevel_bytecode.then(std::time::Instant::now);

        let bytecode_str = args[0];
        let constants_vec = self.quote_value_with_bytecode(args[1])?;
        let maxdepth = args[2];

        use crate::emacs_core::bytecode::ByteCodeFunction;
        use crate::emacs_core::bytecode::decode::decode_gnu_bytecode_with_offset_map;
        use crate::emacs_core::value::LambdaParams;

        // Bytecode strings are unibyte and may contain non-UTF-8 bytes.
        // Access raw bytes directly, same fix as make_byte_code_from_parts.
        let raw_bytes = if let Some(ls) = bytecode_str.as_lisp_string() {
            ls.as_bytes().to_vec()
        } else {
            Vec::new()
        };

        let mut constants: Vec<Value> = match constants_vec.kind() {
            ValueKind::Veclike(VecLikeType::Vector) => {
                constants_vec.as_vector_data().unwrap().clone()
            }
            _ => Vec::new(),
        };

        for constant in &mut constants {
            *constant = crate::emacs_core::builtins::try_convert_nested_compiled_literal(*constant);
        }

        let (ops, gnu_byte_offset_map) =
            decode_gnu_bytecode_with_offset_map(&raw_bytes, &mut constants).map_err(|e| {
                signal(
                    "error",
                    vec![Value::string(format!("bytecode decode error: {}", e))],
                )
            })?;
        if let Some(start) = decode_start {
            tracing::info!(
                "TOPLEVEL-BYTECODE decode file={} bytes={} consts={} ops={} elapsed={:.2?}",
                load_file_name,
                raw_bytes.len(),
                constants.len(),
                ops.len(),
                start.elapsed()
            );
        }

        let max_stack = match maxdepth.kind() {
            ValueKind::Fixnum(n) => n as u16,
            _ => 16,
        };

        let bc = ByteCodeFunction {
            source_id: super::bytecode::fresh_bytecode_source_id(),
            ops,
            // The instructions above came straight from the sealing decoder;
            // the stack proof is recomputed below once every shape field
            // (params/lexical/arglist/env/max_stack) is in place.
            ops_sealed: true,
            stack_verified: false,
            constants: constants.into(),
            max_stack,
            params: LambdaParams::simple(vec![]),
            arglist: Value::NIL,
            lexical: false,
            env: None,
            gnu_byte_offset_map: Some(gnu_byte_offset_map),
            gnu_bytecode_bytes: None,
            docstring: None,
            doc_form: None,
            interactive: None,
            closure_slot_count: 4,
            extra_slots: Vec::new(),
            #[cfg(feature = "jit")]
            runtime: Some(crate::emacs_core::jit::Runtime::new()),
            lazy_gnu_code: None,
        };

        let mut vm = super::bytecode::Vm::from_context(self);
        let exec_start = trace_toplevel_bytecode.then(std::time::Instant::now);
        let result = vm.execute(&bc, vec![]);
        if let Some(start) = exec_start {
            tracing::info!(
                "TOPLEVEL-BYTECODE exec   file={} ops={} elapsed={:.2?}",
                load_file_name,
                bc.executable_ops().len(),
                start.elapsed()
            );
        }
        result
    }

    pub(crate) fn defalias_value(&mut self, sym: Value, def: Value) -> EvalResult {
        let plan = builtins::plan_defalias_in_obarray(self.obarray(), &[sym, def])?;
        let builtins::DefaliasPlan { action, result, .. } = plan;
        self.record_load_history_entry(LoadHistoryEntry::function(result, def));
        self.record_defalias_function_history(result);
        match action {
            builtins::DefaliasAction::SetFunction { symbol, definition } => {
                self.note_macro_expansion_mutation();
                self.obarray.set_symbol_function_id(symbol, definition);
            }
            builtins::DefaliasAction::CallHook {
                hook,
                symbol_value,
                definition,
            } => {
                self.apply(hook, vec![symbol_value, definition])?;
            }
        }
        if let Some(symbol) = result.as_symbol_id() {
            let definition = self
                .obarray
                .symbol_function_id(symbol)
                .unwrap_or(Value::NIL);
            crate::emacs_core::interactive::sync_interactive_registry_for_symbol_definition(
                &mut self.interactive,
                symbol,
                definition,
            );
        }
        Ok(result)
    }

    /// GNU `defalias` records a `function-history' entry whenever a symbol
    /// that already has a function definition is redefined (`olddef' is
    /// non-nil; src/data.c:983-991).  This drives, among other things, the
    /// `Properties:' line of `apropos' output, so the property must exist on
    /// multiply-defined symbols (e.g. a C/preloaded builtin later redefined
    /// when its `.el' loads).  Must run *before* the new definition is
    /// installed, so the previous definition is still readable.
    pub(crate) fn record_defalias_function_history(&mut self, result: Value) {
        if let Some(symbol) = result.as_symbol_id() {
            let olddef = self
                .obarray
                .symbol_function_id(symbol)
                .unwrap_or(Value::NIL);
            if !olddef.is_nil() {
                self.add_to_function_history(symbol, olddef);
            }
        }
    }

    /// Port of GNU `add_to_function_history` (`src/data.c:933-968`): push
    /// `(FILE OLDDEF . PAST)` onto SYMBOL's `function-history' property, where
    /// FILE is the file currently being loaded (the trailing string element of
    /// `current-load-list', or nil).  If the property already has a record for
    /// FILE, the stale record is removed first so the history reflects only one
    /// entry per file (so an unload reverts cleanly).
    fn add_to_function_history(&mut self, symbol: SymId, olddef: Value) {
        let history_prop = intern("function-history");
        let past = self
            .obarray
            .get_property_id(symbol, history_prop)
            .unwrap_or(Value::NIL);

        // FILE = trailing string element of current-load-list (GNU walks the
        // list looking for an entry whose cdr is nil and car is a string).
        let mut file = Value::NIL;
        let mut tail = self.visible_variable_value_or_nil("current-load-list");
        while tail.is_cons() {
            if tail.cons_cdr().is_nil() && tail.cons_car().is_string() {
                file = tail.cons_car();
            }
            tail = tail.cons_cdr();
        }

        // `(plist-member PAST FILE 'equal)` — find the existing record for FILE.
        if let Some(tem) = Self::plist_member_equal(past, file) {
            if tem == past {
                // New def from the same file as the last change: nothing to do.
                return;
            }
            // Remove the previous info for this file by splicing it out:
            // prev = nthcdr(len(past) - len(tem) - 2, past); (setcdr prev (cdr tem)).
            let past_len = Self::list_length(past);
            let tem_len = Self::list_length(tem);
            let tempos = past_len - tem_len;
            if tempos >= 2 {
                let mut prev = past;
                for _ in 0..(tempos - 2) {
                    prev = prev.cons_cdr();
                }
                if prev.is_cons() {
                    prev.set_cdr(tem.cons_cdr());
                }
            }
        }

        let roots = self.save_specpdl_roots();
        self.push_specpdl_root(past);
        self.push_specpdl_root(olddef);
        let new_history = Value::cons(file, Value::cons(olddef, past));
        self.push_specpdl_root(new_history);
        let _ = self
            .obarray
            .put_property_id(symbol, history_prop, new_history);
        self.restore_specpdl_roots(roots);
    }

    /// `(plist-member PLIST KEY 'equal)` restricted to the key positions
    /// (even indices), returning the tail starting at the matching key.
    fn plist_member_equal(plist: Value, key: Value) -> Option<Value> {
        let mut tail = plist;
        while tail.is_cons() {
            if equal_value(&tail.cons_car(), &key, 0) {
                return Some(tail);
            }
            // Advance two cells (key, value).
            let cdr = tail.cons_cdr();
            if !cdr.is_cons() {
                break;
            }
            tail = cdr.cons_cdr();
        }
        None
    }

    fn list_length(list: Value) -> i64 {
        let mut n = 0;
        let mut tail = list;
        while tail.is_cons() {
            n += 1;
            tail = tail.cons_cdr();
        }
        n
    }

    fn current_load_list_is_file_context(current_load_list: Value) -> bool {
        let mut tail = current_load_list;
        while tail.is_cons() {
            if tail.cons_cdr().is_nil() && tail.cons_car().is_string() {
                return true;
            }
            tail = tail.cons_cdr();
        }
        false
    }

    pub(crate) fn record_load_history_entry(&mut self, entry: LoadHistoryEntry) {
        // GNU `defalias` omits autoload definitions while constructing a dump:
        // those entries are bootstrap implementation detail and otherwise add
        // substantial dead weight to the persisted load history.  Runtime
        // package autoload files have dump-mode=nil and remain observable.
        if entry.is_autoload_definition()
            && self.visible_variable_value_or_nil("dump-mode").is_truthy()
        {
            return;
        }
        let dedup = entry.should_deduplicate();
        let entry = entry.into_lisp_value();
        let current_load_list = self.visible_variable_value_or_nil("current-load-list");
        // GNU Frequire (fns.c) computes `from_file = load_in_progress` first
        // and only falls back to walking Vcurrent_load_list for the last
        // string element when no load is running (eval-buffer of a file).
        // During a load, `load-in-progress` is specbound t exactly while
        // `current-load-list` is bound to (filename), so the truthy check is
        // an O(1) equivalent of the walk. Without it, every recorded entry
        // re-walked the whole accumulated list — O(n^2) across a file with n
        // definitions (measured ~20% of the load of an 8000-form autoload
        // file, GNU LOADHIST_ATTACH is an unconditional O(1) prepend).
        let in_file_context = self
            .visible_variable_value_or_nil("load-in-progress")
            .is_truthy()
            || Self::current_load_list_is_file_context(current_load_list);
        if !in_file_context {
            return;
        }

        if dedup {
            let mut cursor = current_load_list;
            while cursor.is_cons() {
                if equal_value(&cursor.cons_car(), &entry, 0) {
                    return;
                }
                cursor = cursor.cons_cdr();
            }
        }

        let roots = self.save_specpdl_roots();
        self.push_specpdl_root(current_load_list);
        self.push_specpdl_root(entry);
        self.set_variable("current-load-list", Value::cons(entry, current_load_list));
        self.restore_specpdl_roots(roots);
    }

    #[tracing::instrument(level = "info", skip(self, subfeatures))]
    pub(crate) fn provide_value(
        &mut self,
        feature: Value,
        subfeatures: Option<Value>,
    ) -> EvalResult {
        self.note_macro_expansion_mutation();
        provide_value_in_state(&mut self.obarray, &mut self.features, feature, subfeatures)?;
        self.record_load_history_entry(LoadHistoryEntry::ProvidedFeature(feature));
        // GNU Emacs Fprovide (fns.c): after adding the feature, run any
        // load-hooks registered in `after-load-alist`.
        //   tem = Fassq(feature, Vafter_load_alist);
        //   if (CONSP(tem))  Fmapc(Qfuncall, XCDR(tem));
        //
        // GNU Emacs Fprovide: (mapc #'funcall (cdr (assq feature after-load-alist)))
        // Does NOT clear load-file-name — the delayed-func from eval-after-load
        // defers to after-load-functions when load-file-name is set, and
        // do-after-load-evaluation fires those hooks after the file finishes loading.
        self.run_after_load_hooks_for_feature(feature)?;
        Ok(feature)
    }

    /// Run `after-load-alist` callbacks for FEATURE, mirroring GNU's
    /// `Fprovide` behavior: `(mapc #'funcall (cdr (assq feature after-load-alist)))`.
    fn run_after_load_hooks_for_feature(&mut self, feature: Value) -> Result<(), Flow> {
        let after_load_alist = self
            .obarray
            .symbol_value("after-load-alist")
            .cloned()
            .unwrap_or(Value::NIL);
        if after_load_alist.is_nil() {
            return Ok(());
        }
        // Walk after-load-alist looking for an entry whose car `eq` FEATURE.
        let entry = {
            let mut cursor = after_load_alist;
            let mut found = Value::NIL;
            while cursor.is_cons() {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if pair_car.is_cons() {
                    let inner_pair_car = pair_car.cons_car();
                    if inner_pair_car == feature {
                        found = pair_car;
                        break;
                    }
                }
                cursor = pair_cdr;
            }
            found
        };
        if entry.is_nil() {
            return Ok(());
        }
        // entry is (FEATURE callback1 callback2 ...).
        // Call funcall on each callback in the cdr.
        // A callback can delete this entry from after-load-alist (its only
        // root) and trigger GC; root the entry for the walk, plus the moving
        // cursor in an updatable slot so even a mid-chain setcdr cannot free
        // the remainder we still read (marking is transitive from the
        // cursor, matching GNU's conservatively scanned tail local).
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(entry);
        let cursor_slot = self.push_specpdl_root_slot(Value::NIL);
        let callbacks = entry.cons_cdr();
        let mut cursor = callbacks;
        let result = loop {
            if !cursor.is_cons() {
                break Ok(());
            }
            self.set_specpdl_root_slot(&cursor_slot, cursor);
            let pair_car = cursor.cons_car();
            let pair_cdr = cursor.cons_cdr();
            let callback = pair_car;
            if let Err(err) = self.apply(callback, vec![]) {
                break Err(err);
            }
            cursor = pair_cdr;
        };
        self.restore_specpdl_roots(root_scope);
        result
    }

    #[tracing::instrument(level = "info", skip(self), err(Debug))]
    pub(crate) fn require_value(
        &mut self,
        feature: Value,
        filename: Option<Value>,
        noerror: Option<Value>,
    ) -> EvalResult {
        let feature_name =
            super::builtins::symbols::symbol_id(&feature).map(|sid| resolve_sym(sid).to_string());
        let filename_str = filename.as_ref().and_then(|v| {
            v.as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        });
        match plan_require_in_state(
            &self.obarray,
            self.buffers.current_buffer(),
            &mut self.features,
            &self.require_stack,
            feature,
            filename,
            noerror,
        ) {
            Err(e) => {
                let rendered = super::error::format_flow_with_eval(self, &e);
                tracing::error!(
                    feature = ?feature_name,
                    filename = ?filename_str,
                    "require plan failed: {}", rendered
                );
                return Err(e);
            }
            Ok(plan) => {
                self.record_load_history_entry(LoadHistoryEntry::RequiredFeature(feature));
                match plan {
                    RequirePlan::Return(value) => Ok(value),
                    RequirePlan::Load {
                        sym_id,
                        name,
                        path,
                        missing_file,
                    } => {
                        // The nesting entry rides the specpdl (GNU fns.c
                        // Frequire: record_unwind_protect (require_unwind,
                        // ...)), so a panic contained at a module/JIT
                        // boundary inside the load pops it via the boundary
                        // unwind instead of leaking a spurious "Recursive
                        // require" entry.
                        let spec_entry = self.specpdl.len();
                        self.specpdl.push(SpecBinding::RequireStack {
                            len: self.require_stack.len(),
                        });
                        self.require_stack.push(sym_id);
                        let result = super::autoload::with_implicit_load_state(self, |eval| {
                            eval.load_file_internal_with_options(
                                &path,
                                super::load::LoadOptions::implicit_dependency(missing_file),
                            )?;
                            eval.refresh_features_from_variable();
                            finish_require_in_state(&eval.features, sym_id, &name, Some(&path))
                        });
                        let result = self.unbind_to_with_result(spec_entry, result);
                        if let Err(ref e) = result
                            && !self.flow_has_active_handler(e)
                        {
                            let noerror_val =
                                noerror.as_ref().map(|v| !v.is_nil()).unwrap_or(false);
                            let path_str = path.display().to_string();
                            let rendered = super::error::format_flow_with_eval(self, e);
                            tracing::error!(
                                feature_name = ?feature_name,
                                path = %path_str,
                                noerror = noerror_val,
                                "require failed: {}", rendered
                            );
                        }
                        result
                    }
                }
            }
        }
    }

    fn flow_has_active_handler(&self, flow: &Flow) -> bool {
        match flow {
            Flow::Signal(sig) => self.has_active_condition_handler_for_signal(sig),
            Flow::Throw(thrown) => self.has_active_catch(&thrown.tag),
            // Nothing handles a shutdown; it unwinds to the process boundary.
            Flow::ThreadBlocked(_) | Flow::Shutdown(_) => false,
        }
    }

    // -----------------------------------------------------------------------
    // Lambda / Function application
    // -----------------------------------------------------------------------

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn make_interpreted_closure_with_expr_runtime_hook(
        &mut self,
        params_value: Value,
        body_value: Value,
        env_value: Value,
        docstring_value: Value,
        iform_value: Value,
    ) -> EvalResult {
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(params_value);
        self.push_specpdl_root(body_value);
        self.push_specpdl_root(env_value);
        self.push_specpdl_root(docstring_value);
        self.push_specpdl_root(iform_value);

        if !env_value.is_nil() {
            let closure_hook = self.visible_variable_value_or_nil_by_id(
                internal_make_interpreted_closure_function_symbol(),
            );
            if !closure_hook.is_nil() {
                self.push_specpdl_root(closure_hook);
                let result = self.apply(
                    closure_hook,
                    vec![
                        params_value,
                        body_value,
                        env_value,
                        docstring_value,
                        iform_value,
                    ],
                );
                self.restore_specpdl_roots(root_scope);
                return result;
            }
        }

        let result = builtins::symbols::make_interpreted_closure_from_parts(
            &params_value,
            &body_value,
            &env_value,
            Some(&docstring_value),
            Some(&iform_value),
        );
        self.restore_specpdl_roots(root_scope);
        result
    }

    fn make_interpreted_closure_with_value_runtime_hook(
        &mut self,
        source_function: Value,
        params_value: Value,
        body_value: Value,
        env_value: Value,
        docstring_value: Value,
        iform_value: Value,
    ) -> EvalResult {
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(source_function);
        self.push_specpdl_root(params_value);
        self.push_specpdl_root(body_value);
        self.push_specpdl_root(env_value);
        self.push_specpdl_root(docstring_value);
        self.push_specpdl_root(iform_value);

        if !env_value.is_nil() {
            let closure_hook = self.visible_variable_value_or_nil_by_id(
                internal_make_interpreted_closure_function_symbol(),
            );
            if !closure_hook.is_nil() {
                self.push_specpdl_root(closure_hook);
                let result = self.apply(
                    closure_hook,
                    vec![
                        params_value,
                        body_value,
                        env_value,
                        docstring_value,
                        iform_value,
                    ],
                );
                self.restore_specpdl_roots(root_scope);
                return result;
            }
        }

        let result = builtins::symbols::make_interpreted_closure_from_parts(
            &params_value,
            &body_value,
            &env_value,
            Some(&docstring_value),
            Some(&iform_value),
        );
        self.restore_specpdl_roots(root_scope);
        result
    }

    fn eval_dynamic_documentation_value(&mut self, value: Value) -> Result<Option<Value>, Flow> {
        if !value.is_cons() || value.cons_car().as_symbol_name() != Some(":documentation") {
            return Ok(None);
        }

        let tail = value.cons_cdr();
        if tail.is_nil() {
            return Ok(Some(Value::NIL));
        }
        if !tail.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), value],
            ));
        }

        self.eval_value(&tail.cons_car()).map(Some)
    }

    #[inline(always)]
    pub(crate) fn push_backtrace_frame(&mut self, function: Value, args: &[Value]) {
        match args {
            [arg] => {
                self.specpdl.push(SpecBinding::Backtrace1 {
                    function,
                    arg: *arg,
                    debug_on_exit: false,
                });
                return;
            }
            [arg0, arg1] => {
                self.specpdl.push(SpecBinding::Backtrace2 {
                    function,
                    arg0: *arg0,
                    arg1: *arg1,
                });
                return;
            }
            _ => {}
        }
        let args = self.backtrace_args_from_slice(args);
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args,
            debug_on_exit: false,
        });
    }

    /// Backtrace push for a native (JIT) caller: args live in the generated
    /// code's call-args slot. Reads them in place — the common 1-2 arity
    /// cases go straight into the compact specpdl forms with no intermediate
    /// collection (the SmallVec built merely to pass `&[Value]` was a
    /// measured ~30 Ir/call tax on native-to-native recursion).
    ///
    /// # Safety
    /// `args_ptr` must address `nargs` valid tagged words, alive for the
    /// duration of this call (the caller's call-args slot).
    pub(crate) unsafe fn push_backtrace_frame_from_native_args(
        &mut self,
        function: Value,
        args_ptr: *const i64,
        nargs: usize,
    ) {
        // SAFETY: caller contract.
        let read = |i: usize| Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
        match nargs {
            1 => {
                self.specpdl.push(SpecBinding::Backtrace1 {
                    function,
                    arg: read(0),
                    debug_on_exit: false,
                });
            }
            2 => {
                self.specpdl.push(SpecBinding::Backtrace2 {
                    function,
                    arg0: read(0),
                    arg1: read(1),
                });
            }
            _ => {
                // GNU stores exactly this: a pointer into the caller's
                // frame plus the count. The 3+-arity path previously
                // copied the args twice and parked them on the owned
                // side-stack — ~100 Ir per call on 3-arg native
                // recursion (tak).
                self.specpdl.push(SpecBinding::BacktraceNative {
                    function,
                    args_ptr,
                    nargs: nargs as u32,
                });
            }
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn push_backtrace_frame_owned(&mut self, function: Value, args: LispArgVec) {
        match args.as_slice() {
            [arg] => {
                self.specpdl.push(SpecBinding::Backtrace1 {
                    function,
                    arg: *arg,
                    debug_on_exit: false,
                });
                return;
            }
            [arg0, arg1] => {
                self.specpdl.push(SpecBinding::Backtrace2 {
                    function,
                    arg0: *arg0,
                    arg1: *arg1,
                });
                return;
            }
            _ => {}
        }
        let args = self.backtrace_args_from_owned(args);
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args,
            debug_on_exit: false,
        });
    }

    #[inline(always)]
    pub(crate) fn push_backtrace_frame_from_bc_stack(
        &mut self,
        function: Value,
        args_start: usize,
        nargs: usize,
    ) -> BytecodeBacktraceFrame {
        let base = self.specpdl.len();
        debug_assert!(
            args_start
                .checked_add(nargs)
                .is_some_and(|end| end <= self.bc_buf.len()),
            "bytecode backtrace arguments must be a live caller-stack span"
        );
        let (args, owns_args) = match BytecodeBacktraceSpan::try_new(args_start, nargs) {
            Some(span) => (BacktraceArgs::evaluated_bc_stack(span), false),
            None => (
                self.backtrace_args_from_oversized_bc_stack(args_start, nargs),
                true,
            ),
        };
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args,
            debug_on_exit: false,
        });
        BytecodeBacktraceFrame::new(base, owns_args)
    }

    /// Semantic fallback for a bytecode stack span too large for the compact
    /// descriptor. Keep it out of the ordinary Bcall instruction stream: a
    /// packed span covers every normally allocatable frame, while this path
    /// must retain behavior rather than impose a representation limit.
    #[cold]
    #[inline(never)]
    fn backtrace_args_from_oversized_bc_stack(
        &mut self,
        args_start: usize,
        nargs: usize,
    ) -> BacktraceArgs {
        let values = LispArgVec::from_slice(&self.bc_buf[args_start..args_start + nargs]);
        BacktraceArgs::evaluated(self.store_backtrace_args(values))
    }

    /// Push a backtrace frame for a special-form call (`nargs == UNEVALLED`
    /// in GNU eval.c:2585). `original_args` is the cons list of un-evaluated
    /// argument forms — XCDR of the original form. The walker emits
    /// `(nil FUNC FORMS FLAGS)` for these frames.
    pub(crate) fn push_unevalled_backtrace_frame(&mut self, function: Value, original_args: Value) {
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args: BacktraceArgs::unevalled(original_args),
            debug_on_exit: false,
        });
    }

    #[inline]
    fn store_backtrace_args(&mut self, args: LispArgVec) -> usize {
        let index = self.backtrace_args_stack.len();
        self.backtrace_args_stack.push(args);
        index
    }

    #[inline]
    fn backtrace_args_from_slice(&mut self, args: &[Value]) -> BacktraceArgs {
        match args {
            [] => BacktraceArgs::evaluated0(),
            _ => BacktraceArgs::evaluated(self.store_backtrace_args(LispArgVec::from_slice(args))),
        }
    }

    #[inline]
    fn backtrace_args_from_owned(&mut self, args: LispArgVec) -> BacktraceArgs {
        if args.is_empty() {
            BacktraceArgs::evaluated0()
        } else {
            BacktraceArgs::evaluated(self.store_backtrace_args(args))
        }
    }

    fn evaluated_backtrace_from_slice(
        &mut self,
        function: Value,
        debug_on_exit: bool,
        args: &[Value],
    ) -> SpecBinding {
        match args {
            [arg] => SpecBinding::Backtrace1 {
                function,
                arg: *arg,
                debug_on_exit,
            },
            [arg0, arg1] if !debug_on_exit => SpecBinding::Backtrace2 {
                function,
                arg0: *arg0,
                arg1: *arg1,
            },
            _ => SpecBinding::Backtrace {
                function,
                args: self.backtrace_args_from_slice(args),
                debug_on_exit,
            },
        }
    }

    fn evaluated_backtrace_from_owned(
        &mut self,
        function: Value,
        debug_on_exit: bool,
        args: LispArgVec,
    ) -> SpecBinding {
        match args.as_slice() {
            [arg] => SpecBinding::Backtrace1 {
                function,
                arg: *arg,
                debug_on_exit,
            },
            [arg0, arg1] if !debug_on_exit => SpecBinding::Backtrace2 {
                function,
                arg0: *arg0,
                arg1: *arg1,
            },
            _ => SpecBinding::Backtrace {
                function,
                args: self.backtrace_args_from_owned(args),
                debug_on_exit,
            },
        }
    }

    #[inline]
    fn release_backtrace_args(&mut self, args: &BacktraceArgs) {
        let Some(index) = args.owned_index() else {
            return;
        };
        self.release_owned_backtrace_args(index);
    }

    #[inline(never)]
    fn release_owned_backtrace_args(&mut self, index: usize) {
        if index >= self.backtrace_args_stack.len() {
            // Healed residue: a panic contained at a JIT-shim/module boundary
            // truncated `backtrace_args_stack` while the panicked extent's
            // Backtrace specpdl entries survive for the deferred depth-based
            // unwind (`restore_jit_shim_boundary` doc). Their slots are
            // already gone; releasing degrades to a no-op by design.
            return;
        }
        debug_assert_eq!(
            index + 1,
            self.backtrace_args_stack.len(),
            "backtrace args stack should unwind in LIFO order"
        );
        if index + 1 == self.backtrace_args_stack.len() {
            self.backtrace_args_stack.pop();
        } else {
            self.backtrace_args_stack[index].clear();
        }
    }

    /// Test-only: observed depth of the backtrace args stack (containment
    /// regression tests assert healed residue leaves it at base).
    #[cfg(test)]
    pub(crate) fn backtrace_args_stack_len_for_test(&self) -> usize {
        self.backtrace_args_stack.len()
    }

    fn release_backtrace_args_in_specpdl_suffix(&mut self, count: usize) {
        let mut truncate_to = self.backtrace_args_stack.len();
        for binding in self.specpdl[count..].iter().rev() {
            if let SpecBinding::Backtrace { args, .. } = binding
                && let BacktraceArgsView::Evaluated(index) = args.view()
            {
                if index >= truncate_to {
                    // Healed residue (see `release_backtrace_args`): the slot
                    // was already truncated by a containment boundary restore.
                    continue;
                }
                debug_assert_eq!(
                    index + 1,
                    truncate_to,
                    "backtrace args stack should match the specpdl unwind suffix"
                );
                truncate_to = index;
            }
        }
        self.backtrace_args_stack.truncate(truncate_to);
    }

    pub(crate) fn backtrace_args_values(&self, args: &BacktraceArgs) -> LispArgVec {
        match args.view() {
            BacktraceArgsView::Unevalled(value) => smallvec::smallvec![value],
            BacktraceArgsView::Evaluated0 => LispArgVec::new(),
            BacktraceArgsView::Evaluated(index) => self
                .backtrace_args_stack
                .get(index)
                .cloned()
                .unwrap_or_default(),
            BacktraceArgsView::EvaluatedBcStack(span) => {
                let start = span.start();
                let len = span.len();
                let end = start.saturating_add(len);
                if end <= self.bc_buf.len() {
                    LispArgVec::from_slice(&self.bc_buf[start..end])
                } else {
                    LispArgVec::new()
                }
            }
        }
    }

    /// Copy the logical GNU backtrace fields from any compact physical frame.
    /// Backtrace inspection is cold; centralizing the representation split
    /// keeps callers exhaustive without putting a larger enum in the hot
    /// specpdl entry itself.
    pub(crate) fn backtrace_entry_values(
        &self,
        entry: &SpecBinding,
    ) -> Option<(Value, LispArgVec, bool, bool)> {
        match entry {
            SpecBinding::Backtrace {
                function,
                args,
                debug_on_exit,
            } => Some((
                *function,
                self.backtrace_args_values(args),
                *debug_on_exit,
                args.is_unevalled(),
            )),
            SpecBinding::Backtrace1 {
                function,
                arg,
                debug_on_exit,
            } => Some((*function, smallvec::smallvec![*arg], *debug_on_exit, false)),
            SpecBinding::Backtrace2 {
                function,
                arg0,
                arg1,
            } => Some((*function, smallvec::smallvec![*arg0, *arg1], false, false)),
            SpecBinding::BacktraceNative {
                function,
                args_ptr,
                nargs,
            } => {
                // SAFETY: variant contract — the caller's call-args slot
                // outlives this entry.
                let args = (0..*nargs as usize)
                    .map(|i| Value::from_bits(unsafe { *args_ptr.add(i) } as usize))
                    .collect();
                Some((*function, args, false, false))
            }
            _ => None,
        }
    }

    /// True when the specpdl entry at `index` is a backtrace frame at all --
    /// GNU's `eassert (pdl->kind == SPECPDL_BACKTRACE)`
    /// (`src/eval.c:146, 154`, `src/lisp.h:3736`).
    pub(crate) fn specpdl_entry_is_backtrace(&self, index: usize) -> bool {
        matches!(
            self.specpdl.get(index),
            Some(
                SpecBinding::Backtrace { .. }
                    | SpecBinding::Backtrace1 { .. }
                    | SpecBinding::Backtrace2 { .. }
                    | SpecBinding::BacktraceNative { .. }
            )
        )
    }

    /// GNU `Fdefvaralias`'s specpdl scan (`src/eval.c:702-711`): is SYMBOL
    /// dynamically rebound anywhere on the current binding stack?
    ///
    /// GNU walks from `specpdl_ptr` down to `specpdl` -- the *whole* stack, not
    /// the current frame -- and compares with `EQ`, so no alias resolution
    /// happens: the question is about this exact symbol.  A binding that has
    /// already been unwound is gone from the stack and therefore not found,
    /// which is the difference between rows 1 and 2 of `tmp/l183-p6.el`.
    pub(crate) fn symbol_is_let_bound(&self, symbol: SymId) -> bool {
        self.specpdl
            .iter()
            .rev()
            .any(|entry| entry.let_bound_symbol() == Some(symbol))
    }

    /// GNU `backtrace_debug_on_exit` (`src/lisp.h:3733-3738`) for the frame at
    /// `index`, answering `false` for anything that is not a backtrace frame so
    /// that a caller unbinding a plain `let` region asks the question safely.
    pub(crate) fn backtrace_frame_wants_debug_on_exit(&self, index: usize) -> bool {
        match self.specpdl.get(index) {
            Some(SpecBinding::Backtrace { debug_on_exit, .. })
            | Some(SpecBinding::Backtrace1 { debug_on_exit, .. }) => *debug_on_exit,
            // Structurally false, by the variants' own contract.
            Some(SpecBinding::Backtrace2 { .. } | SpecBinding::BacktraceNative { .. }) => false,
            _ => false,
        }
    }

    /// GNU `set_backtrace_debug_on_exit` (`src/eval.c:151-156`).
    ///
    /// GNU's `bt` struct always has the bit (`src/lisp.h:3628`); this port's
    /// specpdl entry does not, because [`SpecBinding::Backtrace2`] and
    /// [`SpecBinding::BacktraceNative`] drop it to stay inside the hot entry's
    /// size budget and are documented as "structurally false".  Setting the
    /// flag on one of those therefore *promotes* the frame to the owned
    /// [`SpecBinding::Backtrace`] shape rather than silently losing the
    /// debugger entry -- the promotion is the payment for the size win, and it
    /// is on a cold path by construction (nothing but a debugger sets this).
    ///
    /// Returns whether a backtrace frame was found, mirroring GNU's
    /// `if (backtrace_p (pdl))` guard in `Fbacktrace_debug` (`src/eval.c:4025`).
    pub(crate) fn set_backtrace_debug_on_exit(&mut self, index: usize, flag: bool) -> bool {
        match self.specpdl.get_mut(index) {
            Some(SpecBinding::Backtrace { debug_on_exit, .. })
            | Some(SpecBinding::Backtrace1 { debug_on_exit, .. }) => {
                *debug_on_exit = flag;
                true
            }
            Some(SpecBinding::Backtrace2 { .. } | SpecBinding::BacktraceNative { .. }) => {
                if !flag {
                    // Already false by the variant's contract; nothing to do,
                    // and in particular nothing to promote.
                    return true;
                }
                self.promote_backtrace_frame_for_debug_on_exit(index);
                true
            }
            _ => false,
        }
    }

    /// Rewrite the compact frame at `index` into the owned
    /// [`SpecBinding::Backtrace`] shape with `debug_on_exit` set.
    ///
    /// The one subtlety is `backtrace_args_stack`: its slots are pushed in
    /// specpdl order and released LIFO (`release_backtrace_args_in_specpdl_suffix`
    /// asserts exactly that), so a promotion in the middle of the stack has to
    /// *insert* at the position this frame's slot would have occupied and shift
    /// the frames above it, not push on top.  For the entry-debugger path the
    /// frame is always the specpdl top and the insert degenerates to a push.
    #[cold]
    #[inline(never)]
    fn promote_backtrace_frame_for_debug_on_exit(&mut self, index: usize) {
        let (function, values) = match &self.specpdl[index] {
            SpecBinding::Backtrace2 {
                function,
                arg0,
                arg1,
            } => (*function, smallvec::smallvec![*arg0, *arg1]),
            SpecBinding::BacktraceNative {
                function,
                args_ptr,
                nargs,
            } => {
                let (function, args_ptr, nargs) = (*function, *args_ptr, *nargs as usize);
                // SAFETY: variant contract -- the native caller's call-args
                // slot outlives this entry, and the entry is live here.
                let values = (0..nargs)
                    .map(|i| Value::from_bits(unsafe { *args_ptr.add(i) } as usize))
                    .collect::<LispArgVec>();
                (function, values)
            }
            _ => return,
        };

        // The slot's position: the first owned slot belonging to a frame ABOVE
        // this one, or the top of the stack when there is none.
        let mut insert_at = self.backtrace_args_stack.len();
        for binding in &self.specpdl[index + 1..] {
            if let SpecBinding::Backtrace { args, .. } = binding
                && let Some(owned) = args.owned_index()
            {
                insert_at = insert_at.min(owned);
            }
        }
        self.backtrace_args_stack.insert(insert_at, values);
        for binding in self.specpdl[index + 1..].iter_mut() {
            if let SpecBinding::Backtrace { args, .. } = binding
                && let Some(owned) = args.owned_index()
                && owned >= insert_at
            {
                *args = BacktraceArgs::evaluated(owned + 1);
            }
        }
        self.specpdl[index] = SpecBinding::Backtrace {
            function,
            args: BacktraceArgs::evaluated(insert_at),
            debug_on_exit: true,
        };
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn backtrace_args_len(&self, args: &BacktraceArgs) -> usize {
        match args.view() {
            BacktraceArgsView::Unevalled(_) => 1,
            BacktraceArgsView::Evaluated0 => 0,
            BacktraceArgsView::Evaluated(index) => self
                .backtrace_args_stack
                .get(index)
                .map_or(0, |args| args.len()),
            BacktraceArgsView::EvaluatedBcStack(span) => span.len(),
        }
    }

    fn trace_backtrace_args(&self, args: &BacktraceArgs, visit: &mut dyn FnMut(Value)) {
        match args.view() {
            BacktraceArgsView::Unevalled(value) => visit(value),
            BacktraceArgsView::Evaluated0 => {}
            BacktraceArgsView::Evaluated(index) => {
                if let Some(args) = self.backtrace_args_stack.get(index) {
                    for arg in args.iter().copied() {
                        visit(arg);
                    }
                }
            }
            BacktraceArgsView::EvaluatedBcStack(span) => {
                let start = span.start();
                let end = start.saturating_add(span.len());
                if end <= self.bc_buf.len() {
                    for arg in self.bc_buf[start..end].iter().copied() {
                        visit(arg);
                    }
                }
            }
        }
    }

    /// Promote the UNEVALLED backtrace frame at `specpdl[count]` to the
    /// EVALD shape in place. Mirrors GNU `set_backtrace_args`
    /// (eval.c:144-156) called at eval.c:2638, 2660, 3299 after
    /// argument evaluation completes.
    ///
    /// `count` is the `specpdl.len()` observed *before* the outer
    /// `push_unevalled_backtrace_frame` — the same value a caller
    /// would pass to `unbind_to`.
    ///
    /// Panics if the slot is not an UNEVALLED backtrace frame. Callers
    /// must keep the invariant that every `set_backtrace_args_evalled`
    /// matches exactly one prior `push_unevalled_backtrace_frame`.
    pub(crate) fn set_backtrace_args_evalled(&mut self, count: usize, evaluated: &[Value]) {
        let (function, debug_on_exit) = match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace {
                function,
                args,
                debug_on_exit,
            }) if args.is_unevalled() => (*function, *debug_on_exit),
            other => panic!(
                "set_backtrace_args_evalled: expected UNEVALLED Backtrace at specpdl[{count}], got {other:?}"
            ),
        };
        let replacement = self.evaluated_backtrace_from_slice(function, debug_on_exit, evaluated);
        self.specpdl[count] = replacement;
    }

    pub(crate) fn set_backtrace_args_evalled_owned(&mut self, count: usize, evaluated: LispArgVec) {
        let (function, debug_on_exit) = match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace {
                function,
                args,
                debug_on_exit,
            }) if args.is_unevalled() => (*function, *debug_on_exit),
            other => panic!(
                "set_backtrace_args_evalled_owned: expected UNEVALLED Backtrace at specpdl[{count}], got {other:?}"
            ),
        };
        let replacement = self.evaluated_backtrace_from_owned(function, debug_on_exit, evaluated);
        self.specpdl[count] = replacement;
    }

    pub(crate) fn save_specpdl_roots(&self) -> SpecpdlRootScopeState {
        SpecpdlRootScopeState {
            saved_len: self.specpdl.len(),
        }
    }

    pub(crate) fn record_native_unwind(&mut self, action: NativeUnwindAction) -> NativeUnwindToken {
        let index = self.specpdl.len();
        self.specpdl.push(SpecBinding::NativeUnwind { action });
        NativeUnwindToken { index }
    }

    pub(crate) fn native_unwind_action_mut(
        &mut self,
        token: NativeUnwindToken,
    ) -> Option<&mut NativeUnwindAction> {
        match self.specpdl.get_mut(token.index) {
            Some(SpecBinding::NativeUnwind { action }) => Some(action),
            _ => None,
        }
    }

    pub(crate) fn push_specpdl_root(&mut self, value: Value) {
        self.specpdl.push(SpecBinding::GcRoot { value });
    }

    /// Push a GcRoot whose value can be UPDATED in place: one reusable root
    /// for traversals that must keep a moving cursor (list tail, hook chain
    /// cons) alive across per-element Lisp callbacks. A single slot per
    /// traversal — per-entry root pushes multiply root-seed work into every
    /// collection, which exact-GC stress mode turns into minutes.
    pub(crate) fn push_specpdl_root_slot(&mut self, value: Value) -> SpecpdlRootSlot {
        let index = self.specpdl.len();
        self.specpdl.push(SpecBinding::GcRoot { value });
        SpecpdlRootSlot { index }
    }

    /// Re-point an updatable root slot at a new value. The slot must still
    /// be live (not unwound); the debug assert catches misuse.
    pub(crate) fn set_specpdl_root_slot(&mut self, slot: &SpecpdlRootSlot, value: Value) {
        match self.specpdl.get_mut(slot.index) {
            Some(SpecBinding::GcRoot { value: slot_value }) => *slot_value = value,
            other => {
                debug_assert!(false, "specpdl root slot unwound or replaced: {other:?}");
            }
        }
    }

    fn save_eval_temp_roots(&self) -> EvalTempRootScopeState {
        EvalTempRootScopeState {
            saved_len: self.eval_temp_roots.len(),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn restore_eval_temp_roots(&mut self, scope: EvalTempRootScopeState) {
        self.eval_temp_roots.truncate(scope.saved_len);
    }

    fn restore_eval_temp_roots_to_sequence(&mut self, scope: EvalTempRootScopeState) {
        let current_len = self.eval_temp_roots.len();
        let let_temp_roots = if current_len > scope.saved_len {
            self.eval_temp_roots[scope.saved_len..].to_vec()
        } else {
            Vec::new()
        };
        self.eval_temp_roots
            .truncate(scope.saved_len.min(current_len));
        let Some(frame) = self.sequence_temp_root_frames.last_mut() else {
            return;
        };
        if scope.saved_len < frame.saved_len {
            return;
        }
        frame.let_temp_roots = let_temp_roots;
        self.refresh_current_sequence_temp_roots();
    }

    fn push_eval_temp_root(&mut self, value: Value) {
        self.eval_temp_roots.push(value);
    }

    fn push_eval_temp_root_slot(&mut self, value: Value) -> usize {
        let slot = self.eval_temp_roots.len();
        self.eval_temp_roots.push(value);
        slot
    }

    fn set_eval_temp_root_slot(&mut self, slot: usize, value: Value) {
        if let Some(root) = self.eval_temp_roots.get_mut(slot) {
            *root = value;
        }
    }

    fn save_sequence_temp_roots(&mut self) -> SequenceTempRootScopeState {
        let saved_len = self.eval_temp_roots.len();
        self.sequence_temp_root_frames.push(SequenceTempRootFrame {
            saved_len,
            call_roots: Vec::new(),
            let_temp_roots: Vec::new(),
        });
        SequenceTempRootScopeState { saved_len }
    }

    fn restore_sequence_temp_roots(&mut self, scope: SequenceTempRootScopeState) {
        let frame = self
            .sequence_temp_root_frames
            .pop()
            .expect("sequence temp root restore without matching save");
        let saved_len = frame.saved_len;
        debug_assert_eq!(saved_len, scope.saved_len);
        self.eval_temp_roots.truncate(scope.saved_len);
    }

    fn record_sequence_temp_roots_from_backtrace(&mut self, count: usize) {
        let Some(frame) = self.sequence_temp_root_frames.last() else {
            return;
        };
        let saved_len = frame.saved_len;
        let Some(entry) = self.specpdl.get(count) else {
            return;
        };
        let Some((_, values, _, unevalled)) = self.backtrace_entry_values(entry) else {
            return;
        };
        if unevalled {
            return;
        }
        let frame_index = self.sequence_temp_root_frames.len() - 1;
        self.sequence_temp_root_frames[frame_index].call_roots = values.to_vec();
        debug_assert!(self.eval_temp_roots.len() >= saved_len);
        self.refresh_current_sequence_temp_roots();
    }

    fn refresh_current_sequence_temp_roots(&mut self) {
        let Some(frame) = self.sequence_temp_root_frames.last() else {
            return;
        };
        self.eval_temp_roots.truncate(frame.saved_len);
        self.eval_temp_roots
            .extend(frame.call_roots.iter().copied());
        self.eval_temp_roots
            .extend(frame.let_temp_roots.iter().copied());
    }

    pub(crate) fn record_save_excursion(&mut self) -> Option<usize> {
        let (buffer_id, point) = self
            .buffers
            .current_buffer()
            .map(|buffer| (buffer.id, buffer.point_lisp_char_pos()))?;
        let marker = super::marker::make_registered_buffer_marker(
            &mut self.buffers,
            buffer_id,
            point,
            false,
        );
        let marker_id = super::marker::marker_id_value(&marker)
            .expect("registered save-excursion marker should carry an id");
        let count = self.specpdl.len();
        self.specpdl.push(SpecBinding::SaveExcursion {
            buffer_id,
            marker_id,
            marker,
        });
        Some(count)
    }

    pub(crate) fn restore_specpdl_roots(&mut self, scope: SpecpdlRootScopeState) {
        if self.specpdl.len() <= scope.saved_len {
            return;
        }
        if self.specpdl[scope.saved_len..]
            .iter()
            .all(|binding| matches!(binding, SpecBinding::GcRoot { .. }))
        {
            self.specpdl.truncate(scope.saved_len);
            return;
        }

        // GNU's specpdl is unwound in place by moving the stack pointer.
        // Keep Neomacs' extra GC-root entries just as cheap: remove root-only
        // sentinels from the active suffix without allocating a temporary tail.
        let mut index = 0usize;
        self.specpdl.retain(|binding| {
            let keep = index < scope.saved_len || !matches!(binding, SpecBinding::GcRoot { .. });
            index += 1;
            keep
        });
    }
    pub(crate) fn push_vm_root_frame(&mut self) {
        self.vm_root_frames.push(VmRootFrame::new());
    }

    pub(crate) fn pop_vm_root_frame(&mut self) {
        self.vm_root_frames.pop();
    }

    pub(crate) fn push_vm_frame_root(&mut self, value: Value) {
        self.vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots
            .push(value);
    }

    pub(crate) fn push_vm_frame_root_slot(&mut self, value: Value) -> usize {
        let roots = &mut self
            .vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots;
        let slot = roots.len();
        roots.push(value);
        slot
    }

    pub(crate) fn set_vm_frame_root_slot(&mut self, slot: usize, value: Value) {
        self.vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots[slot] = value;
    }

    pub(crate) fn push_eval_result_roots(&mut self, result: &EvalResult) {
        match result {
            Ok(value) => self.push_vm_frame_root(*value),
            Err(Flow::Signal(sig)) => {
                for value in sig.data.iter().copied() {
                    self.push_vm_frame_root(value);
                }
                if let Some(raw_data) = sig.raw_data {
                    self.push_vm_frame_root(raw_data);
                }
            }
            Err(Flow::Throw(thrown)) => {
                self.push_vm_frame_root(thrown.tag);
                self.push_vm_frame_root(thrown.value);
            }
            Err(Flow::ThreadBlocked(blocked)) => {
                self.push_vm_frame_root(blocked.blocker);
                self.push_vm_frame_root(blocked.remaining_forms);
            }
            // No Lisp values to root.
            Err(Flow::Shutdown(_)) => {}
        }
    }

    pub(crate) fn save_vm_roots(&mut self) -> VmRootScopeState {
        let pushed_vm_root_frame = self.vm_root_frames.is_empty();
        if pushed_vm_root_frame {
            self.push_vm_root_frame();
        }
        VmRootScopeState {
            pushed_vm_root_frame,
            saved_vm_root_frame_len: self.vm_root_frames.last().map(|frame| frame.roots.len()),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn save_vm_frame_roots(&self) -> usize {
        self.vm_root_frames
            .last()
            .expect("VM root frame missing")
            .roots
            .len()
    }

    pub(crate) fn restore_vm_frame_roots(&mut self, saved_len: usize) {
        self.vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots
            .truncate(saved_len);
    }

    pub(crate) fn restore_vm_roots(&mut self, scope: VmRootScopeState) {
        if let Some(saved_len) = scope.saved_vm_root_frame_len {
            self.restore_vm_frame_roots(saved_len);
        }
        if scope.pushed_vm_root_frame {
            self.pop_vm_root_frame();
        }
    }

    pub(crate) fn unbind_to_with_result(&mut self, count: usize, result: EvalResult) -> EvalResult {
        let specpdl_len = self.specpdl.len();
        if specpdl_len == count {
            return result;
        }
        if specpdl_len == count + 1 {
            let trivial_pop = self.specpdl.last().and_then(trivial_spec_binding_pop);
            if let Some(trivial_pop) = trivial_pop {
                if let TrivialSpecBindingPop::BacktraceArgs(args) = trivial_pop {
                    self.release_backtrace_args(&args);
                }
                // SAFETY: `trivial_spec_binding_pop` is the closed proof that
                // the top variant has no owned Rust payload. Its only copied
                // cleanup state is `BacktraceArgs`, which was released above.
                // This is GNU's common `specpdl_ptr--` without routing every
                // call through `SpecBinding`'s whole-enum drop glue.
                unsafe { self.specpdl.set_len(count) };
                return result;
            }
        }
        // GNU's six `if (backtrace_debug_on_exit (...)) val = call_debugger
        // (list2 (Qexit, val));` sites, as one.  Reaching here means the suffix
        // is not all-trivial, and `trivial_spec_binding_pop` treats exactly the
        // `debug_on_exit: true` frames as non-trivial -- so no fast path above
        // can have skipped a flagged frame, and this is the only place one can
        // be popped.  GNU runs the debugger BEFORE `specpdl_ptr--` so the frame
        // is still in the backtrace it shows.
        let result = self.run_debug_on_exit(count, result);
        if self.specpdl.len() <= count {
            // The debugger's own Lisp unwound past this region (a `throw` out
            // of `debug` reaches `top-level`); there is nothing left to pop.
            return result;
        }

        // GNU `unbind_to` over a suffix of plain, untrapped `let` bindings:
        // pop each entry and store its old value back.  No Lisp runs, so
        // `result` needs no root protection and only GNU's own quit-flag
        // bracket applies.
        if self.specpdl_suffix_is_plain_lets(count) {
            self.unbind_plain_lets(count);
            return result;
        }

        if self.specpdl[count..]
            .iter()
            .all(spec_binding_has_trivial_unbind)
        {
            // GNU's common eval path pops SPECPDL_BACKTRACE by moving
            // specpdl_ptr. Avoid result rooting and full unwind work when the
            // suffix has no cleanup or dynamic binding restoration.
            self.release_backtrace_args_in_specpdl_suffix(count);
            self.specpdl.truncate(count);
            return result;
        }

        self.drain_unwind_to(count, result)
    }

    /// Drain every specbinding down to COUNT while carrying RESULT through
    /// arbitrary Lisp cleanup.
    ///
    /// Each failed cleanup has already popped its own entry. Keep unwinding so
    /// lower bindings cannot leak; if another cleanup exits nonlocally, that
    /// later/lower flow supersedes the earlier one just as it does in GNU.
    /// Every specpdl entry above `count` is a `let` of a plain value cell with
    /// no variable watcher — the bindings GNU's `unbind_to` restores with a
    /// bare `SET_SYMBOL_VAL`.
    fn specpdl_suffix_is_plain_lets(&self, count: usize) -> bool {
        self.specpdl[count..].iter().all(|binding| match binding {
            SpecBinding::Let { sym_id, .. } => {
                self.obarray.get_by_id(*sym_id).is_some_and(|sym| {
                    sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
                }) && !self.watchers.has_watchers(*sym_id)
            }
            _ => false,
        })
    }

    /// Restore a suffix that [`Self::specpdl_suffix_is_plain_lets`] accepted.
    fn unbind_plain_lets(&mut self, count: usize) {
        let quitf = self.quit_flag_value();
        if !quitf.is_nil() {
            self.set_quit_flag_value(Value::NIL);
        }
        while self.specpdl.len() > count {
            let Some(SpecBinding::Let { sym_id, old_value }) = self.specpdl.pop() else {
                unreachable!("the suffix was checked to hold only plain let bindings");
            };
            self.obarray
                .store_plain_value_id(sym_id, old_value.as_plain());
            self.sync_cached_runtime_binding_by_id(sym_id, old_value.get().unwrap_or(Value::NIL));
        }
        if !quitf.is_nil() && self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(quitf);
        }
    }

    fn drain_unwind_to(&mut self, count: usize, result: EvalResult) -> EvalResult {
        // GNU eval.c `unbind_to(count, value)` carries VALUE through cleanup.
        // In Rust the value is not on the C stack/register root set, so keep
        // all heap payloads rooted while unwind-protect/watchers may allocate.
        let root_scope = self.save_vm_roots();
        self.push_eval_result_roots(&result);
        let mut cleanup_error = None;
        while self.specpdl.len() > count {
            match self.unbind_to_result(count) {
                Ok(()) => break,
                Err(flow) => {
                    let rooted_error: EvalResult = Err(flow);
                    self.push_eval_result_roots(&rooted_error);
                    cleanup_error = rooted_error.err();
                    // A cleanup nonlocal exit has already popped its own
                    // specbinding. Continue toward COUNT so lower dynamic
                    // bindings are not leaked. A lower cleanup flow replaces
                    // this one, matching GNU's nested nonlocal unwinding.
                }
            }
        }
        self.restore_vm_roots(root_scope);
        if let Some(flow) = cleanup_error {
            return Err(flow);
        }
        result
    }

    /// Execute BODY and unwind every typed/Lisp cleanup it registers, even
    /// when BODY returns early with `?`.
    ///
    /// This is the native-runtime equivalent of GNU's
    /// `record_unwind_protect` + `unbind_to`: callers put the fallible body in
    /// the closure, so Rust control flow cannot bypass the cleanup boundary.
    pub(crate) fn with_unwind_scope(
        &mut self,
        body: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        let count = self.specpdl.len();
        let result = body(self);
        self.drain_unwind_to(count, result)
    }

    #[inline]
    /// Grow the JIT residual-root window stack to hold at least `need` slots
    /// and republish the pointer/capacity mirrors generated code reads. Called
    /// from the cold grow shim only; new slots are NIL so every slot below
    /// `len` is always a valid traced Value.
    pub(crate) fn jit_root_stack_grow(&mut self, need: usize) {
        let new_len = need.max(64).next_power_of_two();
        self.jit_root_stack.resize(new_len, Value::NIL);
        self.jit_root_stack_ptr = self.jit_root_stack.as_mut_ptr();
        self.jit_root_stack_cap = new_len;
    }

    /// GNU `specpdl_ptr--` for the JIT native-call exit: pop the call's own
    /// `BacktraceNative` frame without touching the result value at all.
    /// Returns false when the stack is not in the balanced single-frame
    /// state (nested imbalance, debug residue) — the caller then takes the
    /// general [`Self::pop_bytecode_backtrace_frame_with_result`] path.
    #[inline]
    pub(crate) fn pop_native_backtrace_frame(&mut self, count: usize) -> bool {
        if self.specpdl.len() == count + 1
            && matches!(
                self.specpdl.last(),
                Some(SpecBinding::BacktraceNative { .. })
            )
        {
            // SAFETY: BacktraceNative owns no heap payload (a Value plus a
            // raw pointer and a length), so the length store alone is the
            // pointer-decrement pop; no drop glue needs to run.
            unsafe { self.specpdl.set_len(count) };
            true
        } else {
            false
        }
    }

    pub(crate) fn pop_bytecode_backtrace_frame_with_result(
        &mut self,
        count: usize,
        result: EvalResult,
    ) -> EvalResult {
        // GNU Ffuncall/Breturn exit protocol: a balanced call whose only
        // outstanding entry is its own non-debug Backtrace frame pops it with
        // a pointer decrement (eval.c "specpdl_ptr--"). debug_on_exit: false
        // is load-bearing — a future real backtrace-debug implementation
        // (GNU calls call_debugger(list2(Qexit, val)) first) must land in the
        // unbind_to_with_result fallback below.
        let can_pop = self.specpdl.len() == count + 1
            && matches!(
                self.specpdl.last(),
                Some(SpecBinding::Backtrace {
                    args,
                    debug_on_exit: false,
                    ..
                }) if args.is_evaluated()
            )
            || self.specpdl.len() == count + 1
                && matches!(
                    self.specpdl.last(),
                    // The inline evaluated variants (structurally
                    // debug_on_exit: false) own no side-stack payload —
                    // pointer-decrement pop, same as GNU specpdl_ptr--.
                    // BacktraceNative in particular is what every JIT
                    // native call's frame is; without this arm each pop
                    // took the full unbind_to fallback (~44 Ir measured).
                    Some(
                        SpecBinding::Backtrace1 {
                            debug_on_exit: false,
                            ..
                        } | SpecBinding::Backtrace2 { .. }
                            | SpecBinding::BacktraceNative { .. }
                    )
                );

        if can_pop {
            let binding = self.specpdl.pop().expect("can_pop checked len");
            if let SpecBinding::Backtrace { args, .. } = &binding {
                // Out-of-line args (Evaluated(_)) hold a backtrace_args_stack
                // slot that must unwind LIFO; release no-ops for the inline
                // variants.
                self.release_backtrace_args(args);
            }
            return result;
        }

        self.unbind_to_with_result(count, result)
    }

    #[inline]
    pub(crate) fn pop_bytecode_backtrace_token_with_result(
        &mut self,
        frame: BytecodeBacktraceFrame,
        result: EvalResult,
    ) -> EvalResult {
        self.pop_bytecode_backtrace_frame_with_result(frame.base(), result)
    }

    /// GNU `Breturn`: `if (backtrace_debug_on_exit (pdl)) val = call_debugger
    /// (list2 (Qexit, val));` and only then `specpdl_ptr--`
    /// (`src/bytecode.c:825-828`).
    ///
    /// The pop cannot run the debugger itself -- it is a bare length store with
    /// no result to replace -- so it REFUSES instead, handing the token back so
    /// the caller can take [`Self::pop_bytecode_backtrace_token_with_result`],
    /// which does. That makes "popped a frame that owed a debugger entry"
    /// unconstructible through this API rather than merely unreached.
    ///
    /// Ledger 172 §7 argued no flagged frame could reach the fast pops, and
    /// that was measured false: `backtrace-debug` flags an arbitrary live frame
    /// by index, including a byte-compiled caller already routed to the fast
    /// return.  Measured, `-Q --batch`, `tmp/l183-p10.el` -- a `byte-compile`d
    /// function whose callee runs `(backtrace-debug 1 t)` calls the debugger
    /// once in GNU and called it zero times here, while the interpreted twin
    /// agreed in both editors.
    #[inline(always)]
    pub(crate) fn pop_fast_bytecode_backtrace_frame(
        &mut self,
        frame: BytecodeBacktraceFrame,
    ) -> FastBytecodePop {
        if self.backtrace_frame_wants_debug_on_exit(frame.base()) {
            return FastBytecodePop::OwesDebugOnExit(frame);
        }
        self.pop_fast_bytecode_backtrace_frame_unchecked(frame);
        FastBytecodePop::Popped
    }

    /// [`Self::pop_fast_bytecode_backtrace_frame`] without its
    /// `backtrace_debug_on_exit` test.
    ///
    /// The single caller is the iterative driver's `Breturn`, whose eligibility
    /// gate has already asked the question about this exact specpdl index --
    /// `cleanup.specpdl_base - 1` -- three lines earlier and routed a flagged
    /// frame to the generic unwind.  Asking twice on the hottest return path in
    /// the interpreter buys nothing; asking NOWHERE is the bug this entry
    /// fixes, which is why the proof is named at the call site.
    #[inline(always)]
    pub(crate) fn pop_fast_bytecode_backtrace_frame_unchecked(
        &mut self,
        frame: BytecodeBacktraceFrame,
    ) {
        debug_assert!(
            !self.backtrace_frame_wants_debug_on_exit(frame.base()),
            "the unchecked bytecode pop was handed a frame owing a debugger entry"
        );
        let frame_word = frame.0;
        debug_assert_eq!(
            self.specpdl.len(),
            frame.base() + 1,
            "fast bytecode pop requires its frame to remain the specpdl top"
        );
        debug_assert!(matches!(
            self.specpdl.last(),
            Some(SpecBinding::Backtrace {
                args,
                debug_on_exit: false,
                ..
            }) if args.is_bytecode_storage()
        ));
        let count = if frame_word & BytecodeBacktraceFrame::OWNED_ARGS_FLAG == 0 {
            // The ordinary token is exactly its base: no mask or descriptor
            // decode on GNU's Breturn-shaped hot path.
            frame_word
        } else {
            self.release_oversized_bytecode_backtrace_frame(frame_word)
        };

        // SAFETY: `BytecodeBacktraceFrame` is private, non-Copy, and only
        // constructed immediately after pushing this Backtrace variant. The
        // interpreter driver consumes it only after the nested call restored
        // the exact specpdl depth; debug builds verify that protocol above.
        // Backtrace's fields (`Value`,
        // `BacktraceArgs`, bool) need no drop, so reducing the length is GNU's
        // `specpdl_ptr--` without leaking an owned Rust payload. Any path that
        // can leave another binding or debug-on-exit state uses the exhaustive
        // `pop_bytecode_backtrace_frame_with_result`/`unbind_to` path instead.
        unsafe { self.specpdl.set_len(count) };
    }

    /// Release the semantic fallback for a bytecode argument span that could
    /// not fit the compact descriptor. Keeping both the descriptor decode and
    /// side-stack maintenance here leaves the ordinary return as one predicted
    /// branch plus the `specpdl` pointer decrement.
    #[cold]
    #[inline(never)]
    fn release_oversized_bytecode_backtrace_frame(&mut self, frame_word: usize) -> usize {
        let count = frame_word & BytecodeBacktraceFrame::BASE_MASK;
        let args = match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace {
                args,
                debug_on_exit: false,
                ..
            }) => *args,
            _ => panic!("oversized bytecode pop requires its own non-debug backtrace frame"),
        };
        let index = args
            .owned_index()
            .expect("oversized bytecode backtrace must own an argument slot");
        self.release_owned_backtrace_args(index);
        count
    }

    fn apply_internal(
        &mut self,
        function: Value,
        args: LispArgVec,
        record_backtrace: bool,
    ) -> EvalResult {
        self.maybe_quit_before_gc()?;
        self.enter_interpreted_eval_depth()?;
        let bt_count = self.specpdl.len();
        if record_backtrace {
            self.push_backtrace_frame(function, &args);
        }
        let result = {
            if self.gc_safe_point_exact_should_collect() {
                self.gc_collect_from_current_roots();
            }
            // GNU Ffuncall eval.c:3185-3190 in order: record_in_backtrace,
            // maybe_gc, then `if (debug_on_next_call) do_debug_on_call
            // (Qlambda, count)`.  Only when a frame was recorded -- GNU's
            // Ffuncall always records one, and `record_backtrace: false` is
            // this port's marker for a caller that already did.
            let armed = if record_backtrace {
                self.take_debug_on_call_arm(DebugOnCallCode::Funcall)
            } else {
                None
            };
            let entered = match armed {
                Some(arm) => self.do_debug_on_call(arm),
                None => Ok(()),
            };
            // GNU does not probe stack space for every funcall. Keep growth
            // checks at the function-application boundary, but only on coarse
            // depth intervals so normal startup is not dominated by TLS lookups
            // in stacker::maybe_grow.
            match entered {
                Err(flow) => Err(flow),
                Ok(()) => {
                    self.maybe_grow_eval_stack(|ctx| ctx.funcall_general_untraced(function, args))
                }
            }
        };
        self.depth -= 1;
        let result = self.dispatch_signal_result_if_needed(result);
        self.unbind_to_with_result(bt_count, result)
    }

    /// Apply a function value to evaluated arguments.
    pub(crate) fn apply<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        self.apply_internal(function, args.into(), true)
    }

    /// Call Lisp while suppressing signaled conditions, matching GNU Emacs's
    /// `safe_funcall` (`src/eval.c`).  GNU uses this at diagnostic and display
    /// boundaries where a broken callback must not recursively replace the
    /// operation that is already handling an error.  `throw` and suspended
    /// thread flow remain nonlocal exits; `internal_condition_case_n` does not
    /// catch them in GNU either.
    ///
    /// `nil` is deliberately both the error sentinel and a valid callback
    /// result.  Callers that need a fallback use it in either case, exactly as
    /// GNU's `safe_calln` callers do.
    pub(crate) fn safe_funcall<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        let specpdl_count = self.specpdl.len();
        let result = (|| {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-redisplay"), Value::T)?;
            // GNU's catch-all internal condition handler prevents the debugger
            // from running. Neomacs dispatches signals on function return, so
            // an explicit binding provides the same boundary before we demote
            // the resulting Flow::Signal below.
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-debugger"), Value::T)?;
            self.apply(function, args)
        })();
        let result = self.unbind_to_with_result(specpdl_count, result);
        match result {
            Err(Flow::Signal(flow)) => {
                tracing::debug!(?flow, "error muted by safe_funcall");
                Ok(Value::NIL)
            }
            other => other,
        }
    }

    /// Apply from GNU's Lisp-visible `apply` / `funcall` subrs.
    ///
    /// GNU bytecode `Bcall` increments the same `lisp_eval_depth` counter
    /// before dispatching. If the callee is Lisp-visible `apply` or
    /// `funcall`, GNU then enters `Ffuncall`, whose depth guard observes the
    /// active `Bcall`. Neomacs mirrors that by using the single shared
    /// `Context::depth` counter for both interpreter and bytecode call sites.
    pub(crate) fn apply_from_lisp_funcall<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        self.apply_internal(function, args.into(), true)
    }

    #[inline]
    pub(crate) fn apply0(&mut self, function: Value) -> EvalResult {
        self.apply(function, LispArgVec::new())
    }

    #[inline]
    pub(crate) fn apply1(&mut self, function: Value, arg0: Value) -> EvalResult {
        let mut args = LispArgVec::new();
        args.push(arg0);
        self.apply(function, args)
    }

    #[inline]
    pub(crate) fn apply2(&mut self, function: Value, arg0: Value, arg1: Value) -> EvalResult {
        let mut args = LispArgVec::new();
        args.push(arg0);
        args.push(arg1);
        self.apply(function, args)
    }

    pub(crate) fn apply_untraced<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        self.apply_internal(function, args.into(), false)
    }

    /// Apply FUNC to ARGS, but record FRAME_FUNCTION (not FUNC) in the
    /// runtime backtrace frame. Used by `eval_sub_cons` when the form
    /// dispatches through a symbol: the symbol is what GNU stores in
    /// specpdl (and what `backtrace-frame` returns), while the
    /// resolved function cell is what actually runs.
    ///
    /// Mirrors GNU's `eval_sub` SYMBOLP arm at `eval.c:2600-2625`,
    /// where `original_fun` (the symbol) is the value written into the
    /// specpdl entry via `record_in_backtrace (original_fun, args, ...)`.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn apply_with_frame_function(
        &mut self,
        frame_function: Value,
        func: Value,
        args: impl Into<LispArgVec>,
    ) -> EvalResult {
        let args = args.into();
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(frame_function, &args);
        let result = self.maybe_gc_and_quit().and_then(|_| {
            self.maybe_grow_eval_stack(|ctx| ctx.funcall_general_untraced(func, args))
        });
        let result = self.dispatch_signal_result_if_needed(result);
        self.unbind_to_with_result(bt_count, result)
    }

    /// Unified function dispatch — matches GNU Emacs's funcall_general.
    /// Called by both the tree-walking interpreter (via apply) and the
    /// bytecode VM (via Vm::call_function).
    pub(crate) fn funcall_general<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        let args = args.into();
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(function, &args);
        // Same GNU Ffuncall arm site as `apply_internal` (eval.c:3189-3190):
        // this entry records its own backtrace frame, so it is a funcall in
        // GNU's sense and must be armable.
        let result = match self.take_debug_on_call_arm(DebugOnCallCode::Funcall) {
            Some(arm) => self
                .do_debug_on_call(arm)
                .and_then(|()| self.funcall_general_untraced(function, args)),
            None => self.funcall_general_untraced(function, args),
        };
        let result = self.dispatch_signal_result_if_needed(result);
        self.unbind_to_with_result(bt_count, result)
    }

    /// Execute a bytecode function through the JIT tier-up seam. This is THE
    /// dispatch point for every bytecode call: `funcall_general_untraced`
    /// (interpreter / funcall / apply) AND the VM's own bytecode→bytecode
    /// fast paths (`Vm::call_function_untraced_owned`) route here, so a
    /// function called only from compiled code accumulates heat and tiers up
    /// exactly like one called through funcall/eval. (Previously only the
    /// funcall path consulted the plan, so in fully byte-compiled code —
    /// where calls flow through the VM's Op::Call — the JIT never engaged
    /// at all.)
    ///
    /// The `match` over the dispatch plan is intentionally exhaustive: once
    /// a compiled tier exists it MUST be handled here, enforced by the
    /// compiler. Behind the `jit` feature; the default build is unchanged.
    pub(crate) fn execute_bytecode_call(
        &mut self,
        bc_data: &super::bytecode::ByteCodeFunction,
        args: LispArgVec,
        func_value: Value,
    ) -> EvalResult {
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit::Plan;
            match bc_data
                .jit_runtime()
                .dispatch_sized(bc_data.executable_ops().len())
            {
                Plan::Interpret => {
                    let mut vm = super::bytecode::Vm::from_context(self);
                    vm.execute_with_func_value(bc_data, args, func_value)
                }
                Plan::Compiled => {
                    // Run native code when the body is compilable and the
                    // call is valid (arity is checked inside
                    // try_run_compiled). Ok(None) — non-compilable body, a
                    // deopt (sound to rerun: guards never follow a call),
                    // or an arity mismatch — falls back to the Tier-0
                    // interpreter; Err propagates a Flow raised by a
                    // runtime call inside native code.
                    //
                    // Root the executing function for the duration: native
                    // code references its constants by raw bits, and a
                    // runtime call inside (Call/cons) may trigger GC.
                    let saved_roots = save_scratch_gc_roots();
                    push_scratch_gc_root(func_value);
                    let ctx_ptr = self as *mut Context;
                    let native = crate::emacs_core::jit::try_run_compiled(
                        ctx_ptr, bc_data, func_value, &args,
                    );
                    restore_scratch_gc_roots(saved_roots);
                    match native {
                        Ok(Some(bits)) => Ok(crate::emacs_core::value::Value::from_bits(bits)),
                        Ok(None) => {
                            crate::emacs_core::jit::note_seam_interp_fallback();
                            let mut vm = super::bytecode::Vm::from_context(self);
                            vm.execute_with_func_value(bc_data, args, func_value)
                        }
                        Err(flow) => Err(flow),
                    }
                }
            }
        }
        #[cfg(not(feature = "jit"))]
        {
            let mut vm = super::bytecode::Vm::from_context(self);
            vm.execute_with_func_value(bc_data, args, func_value)
        }
    }

    /// [`Context::execute_bytecode_call`] for arguments that already live on
    /// the GC-traced `bc_buf` at `[args_start, args_start + nargs)` — the
    /// VM's hot bytecode→bytecode path. The interpreter tier runs directly
    /// from the stack span (no `LispArgVec`); the compiled tier materializes
    /// one owned copy for `try_run_compiled` (native code may hold `args_ptr`
    /// across `bc_buf` reallocations, so it must not point into the Vec),
    /// which is amortized by the native run it precedes.
    pub(crate) fn execute_bytecode_call_from_stack(
        &mut self,
        bc_data: &super::bytecode::ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> EvalResult {
        match self.dispatch_bytecode_call_from_stack(bc_data, args_start, nargs, func_value) {
            BytecodeStackCallDispatch::Interpret => {
                let mut vm = super::bytecode::Vm::from_context(self);
                vm.execute_from_stack_args(bc_data, args_start, nargs, func_value)
            }
            BytecodeStackCallDispatch::Complete(result) => result,
        }
    }

    /// Consult the tier dispatcher once without recursively entering Tier 0.
    ///
    /// This is the typed seam used by the bytecode interpreter's iterative
    /// `Bcall` transition.  `Interpret` means the caller must install a Tier-0
    /// frame; `Complete` means native code either returned or raised a flow.
    pub(crate) fn dispatch_bytecode_call_from_stack(
        &mut self,
        bc_data: &super::bytecode::ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> BytecodeStackCallDispatch {
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit::Plan;
            match bc_data
                .jit_runtime()
                .dispatch_sized(bc_data.executable_ops().len())
            {
                Plan::Interpret => BytecodeStackCallDispatch::Interpret,
                Plan::Compiled => {
                    let args = LispArgVec::from_slice(&self.bc_buf[args_start..args_start + nargs]);
                    let saved_roots = save_scratch_gc_roots();
                    push_scratch_gc_root(func_value);
                    let ctx_ptr = self as *mut Context;
                    let native = crate::emacs_core::jit::try_run_compiled(
                        ctx_ptr, bc_data, func_value, &args,
                    );
                    restore_scratch_gc_roots(saved_roots);
                    match native {
                        Ok(Some(bits)) => BytecodeStackCallDispatch::Complete(Ok(
                            crate::emacs_core::value::Value::from_bits(bits),
                        )),
                        Ok(None) => {
                            crate::emacs_core::jit::note_seam_interp_fallback();
                            BytecodeStackCallDispatch::Interpret
                        }
                        Err(flow) => BytecodeStackCallDispatch::Complete(Err(flow)),
                    }
                }
            }
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (bc_data, args_start, nargs, func_value);
            BytecodeStackCallDispatch::Interpret
        }
    }

    pub(crate) fn funcall_general_untraced(
        &mut self,
        function: Value,
        args: impl Into<LispArgVec>,
    ) -> EvalResult {
        let args = args.into();
        match function.kind() {
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                // get_bytecode_data returns a reference into the GC-managed
                // ByteCodeObj.  GNU's bytecode interpreter executes from the
                // function struct in place, never copying.  Don't clone here
                // either — bytecode functions can have thousands of ops, and
                // cloning per call dominated debug-build batch-byte-compile
                // runtime.
                let bc_data = function.get_bytecode_data().unwrap();
                self.execute_bytecode_call(bc_data, args, function)
            }
            ValueKind::Veclike(VecLikeType::Lambda) => self.apply_lambda(function, args),
            ValueKind::Veclike(VecLikeType::Macro) => self.apply_lambda(function, args),
            ValueKind::Subr(_) => self.apply_subr_object(function, args, true),
            ValueKind::Veclike(VecLikeType::Subr) => self.apply_subr_object(function, args, true),
            ValueKind::Veclike(VecLikeType::ModuleFunction) => {
                self.apply_module_function(function, args)
            }
            ValueKind::Symbol(id) => self.apply_symbol_callable_untraced(id, args, true),
            ValueKind::T => self.apply_symbol_callable_untraced(intern("t"), args, true),
            ValueKind::Nil => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::symbol("nil")],
            )),
            _ if function.is_symbol_with_pos() => {
                // Transparently unwrap symbol-with-pos → bare symbol for funcall dispatch.
                let bare = function.as_symbol_with_pos_sym().unwrap();
                self.funcall_general_untraced(bare, args)
            }
            ValueKind::Cons => {
                if super::autoload::is_autoload_value(&function) {
                    Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("symbolp"), function],
                    ))
                } else if cons_head_symbol_id(&function) == Some(lambda_symbol()) {
                    self.apply_lambda(function, args)
                } else {
                    Err(signal(LispCondition::InvalidFunction, vec![function]))
                }
            }
            _ => Err(signal(LispCondition::InvalidFunction, vec![function])),
        }
    }

    /// Convert a `(lambda ...)` or `(closure ...)` cons cell into a
    /// `Value::Lambda`.  This mirrors GNU Emacs's `funcall_lambda` which
    /// handles both forms.  Used by both the interpreter and the bytecode VM.
    pub(crate) fn instantiate_callable_cons_form(&mut self, function: Value) -> EvalResult {
        if !function.is_cons() {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        }
        if list_length(&function).is_none() {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        }

        // Unwrap symbol-with-pos on the car so (lambda ...) / (closure ...)
        // forms with position-wrapped heads are recognized.
        let head_val = self.unwrap_symbol(function.cons_car());
        let Some(head_id) = head_val.as_symbol_id() else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };
        let mut tail = function.cons_cdr();

        let (env_value, params_value, is_lambda) = if head_id == lambda_symbol() {
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![function]));
            }
            let params_value = tail.cons_car();
            tail = tail.cons_cdr();
            // Mirrors GNU eval_sub lambda handling: a lambda gets
            // a lexical closure env only when
            // Vinternal_interpreter_environment is non-nil (i.e.
            // lexical mode is active). We use self.lexenv as the
            // single source of truth, matching GNU.
            let env_value = if !self.lexenv.is_nil() {
                self.lexenv
            } else {
                Value::NIL
            };
            (env_value, params_value, true)
        } else if head_id == closure_symbol() {
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![function]));
            }
            let env_value = tail.cons_car();
            tail = tail.cons_cdr();
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![function]));
            }
            let params_value = tail.cons_car();
            tail = tail.cons_cdr();
            (env_value, params_value, false)
        } else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };

        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(function);

        let docstring_value = if tail.is_cons() {
            let value = tail.cons_car();
            let rest = tail.cons_cdr();
            if value.is_string() && !rest.is_nil() {
                tail = rest;
                value
            } else {
                Value::NIL
            }
        } else {
            Value::NIL
        };

        let mut doc_form_value = Value::NIL;
        if tail.is_cons() {
            let item = tail.cons_car();
            if let Some(doc_form) = self.eval_dynamic_documentation_value(item)? {
                doc_form_value = doc_form;
                tail = tail.cons_cdr();
            }
        }

        while tail.is_cons() {
            let item = tail.cons_car();
            if !item.is_cons()
                || item.cons_car().as_symbol_id() != Some(declare_symbol())
                || list_length(&item).is_none()
            {
                break;
            }
            tail = tail.cons_cdr();
        }

        let iform_value = if tail.is_cons() {
            let item = tail.cons_car();
            if item.is_cons() && item.cons_car().as_symbol_id() == Some(interactive_symbol_id()) {
                tail = tail.cons_cdr();
                item
            } else {
                Value::NIL
            }
        } else {
            Value::NIL
        };

        let body_value = if tail.is_nil() {
            Value::list(vec![Value::NIL])
        } else {
            tail
        };
        let closure_doc_value = if !doc_form_value.is_nil() {
            doc_form_value
        } else {
            docstring_value
        };

        self.push_specpdl_root(params_value);
        self.push_specpdl_root(body_value);
        self.push_specpdl_root(env_value);
        self.push_specpdl_root(closure_doc_value);
        self.push_specpdl_root(iform_value);

        let result = if is_lambda {
            self.make_interpreted_closure_with_value_runtime_hook(
                function,
                params_value,
                body_value,
                env_value,
                closure_doc_value,
                iform_value,
            )
        } else {
            builtins::symbols::make_interpreted_closure_from_parts(
                &params_value,
                &body_value,
                &env_value,
                Some(&closure_doc_value),
                Some(&iform_value),
            )
        };
        self.restore_specpdl_roots(specpdl_root_scope);
        result
    }

    /// GNU funcall_subr (eval.c:3266-3280) pre-checks arity and
    /// signals `(wrong-number-of-arguments #<subr NAME> NUMARGS)`
    /// with the SUBR value. Call this before dispatching to a
    /// builtin so the check matches GNU's `funcall_subr` exactly
    /// and we never depend on the builtin's expect_args helper
    /// (which would emit `Value::symbol(name)` instead of the
    /// subr value).
    ///
    /// Returns `Some(Flow::Signal)` on arity mismatch, `None` when
    /// the arity is acceptable or the subr has no explicit arity
    /// registered (opt-out).
    #[inline]
    fn check_funcall_subr_arity_value(&self, function: Value, nargs: usize) -> Option<Flow> {
        let (_, entry) = subr_entry_from_value(function)?;
        let min = entry.min_args as usize;
        let max = entry.max_args.map(|m| m as usize);
        // Opt-out: a subr registered with (0, None) has declared
        // "I do my own arity check". Keep the legacy behaviour for
        // those until each one is migrated explicitly.
        if min == 0 && max.is_none() {
            return None;
        }
        let arity_bad = nargs < min || max.is_some_and(|m| nargs > m);
        if arity_bad {
            Some(signal(
                LispCondition::WrongNumberOfArguments,
                vec![function, Value::fixnum(nargs as i64)],
            ))
        } else {
            None
        }
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn check_funcall_subr_arity(&self, sym_id: SymId, nargs: usize) -> Option<Flow> {
        self.check_funcall_subr_arity_value(Value::subr_from_sym_id(sym_id), nargs)
    }

    fn dispatch_subr_value_internal(
        &mut self,
        function: Value,
        args: LispArgVec,
        wrong_arity_callee: Value,
    ) -> Option<EvalResult> {
        let (_, entry) = subr_entry_from_value(function)?;
        self.dispatch_subr_entry_internal(entry, args, wrong_arity_callee)
    }

    fn dispatch_subr_entry_internal(
        &mut self,
        entry: SubrEntry,
        args: LispArgVec,
        wrong_arity_callee: Value,
    ) -> Option<EvalResult> {
        let func = entry.function?;
        let nargs = args.len();
        if (nargs as u16) < entry.min_args {
            return Some(Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![wrong_arity_callee, Value::fixnum(nargs as i64)],
            )));
        }
        if let Some(max) = entry.max_args
            && nargs as u16 > max
        {
            return Some(Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![wrong_arity_callee, Value::fixnum(nargs as i64)],
            )));
        }
        Some(self.dispatch_subr_func_unchecked(func, args))
    }

    #[inline]
    fn dispatch_subr_entry_unchecked(
        &mut self,
        entry: SubrEntry,
        args: LispArgVec,
    ) -> Option<EvalResult> {
        let func = entry.function?;
        Some(self.dispatch_subr_func_unchecked(func, args))
    }

    #[inline]
    pub(crate) fn subr_entry_uses_fixed_value_call(entry: SubrEntry) -> bool {
        entry.dispatch_kind == SubrDispatchKind::Builtin
            && matches!(
                entry.function,
                Some(
                    SubrFn::A0(_)
                        | SubrFn::A1(_)
                        | SubrFn::A2(_)
                        | SubrFn::A3(_)
                        | SubrFn::A4(_)
                        | SubrFn::A5(_)
                        | SubrFn::A6(_)
                        | SubrFn::A7(_)
                        | SubrFn::A8(_),
                )
            )
    }

    #[inline]
    fn backtrace_arg_or_nil(&self, args: &BacktraceArgs, index: usize) -> Value {
        match args.view() {
            BacktraceArgsView::Unevalled(_) | BacktraceArgsView::Evaluated0 => Value::NIL,
            BacktraceArgsView::Evaluated(args_index) => self
                .backtrace_args_stack
                .get(args_index)
                .and_then(|args| args.get(index).copied())
                .unwrap_or(Value::NIL),
            BacktraceArgsView::EvaluatedBcStack(span) => {
                if index < span.len() {
                    self.bc_buf
                        .get(span.start().saturating_add(index))
                        .copied()
                        .unwrap_or(Value::NIL)
                } else {
                    Value::NIL
                }
            }
        }
    }

    #[inline]
    fn backtrace_evaluated_arg_or_nil(&self, count: usize, index: usize) -> Value {
        match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace { args, .. }) if args.is_evaluated() => {
                self.backtrace_arg_or_nil(args, index)
            }
            Some(SpecBinding::Backtrace1 { arg, .. }) => {
                if index == 0 {
                    *arg
                } else {
                    Value::NIL
                }
            }
            Some(SpecBinding::Backtrace2 { arg0, arg1, .. }) => match index {
                0 => *arg0,
                1 => *arg1,
                _ => Value::NIL,
            },
            Some(SpecBinding::BacktraceNative {
                args_ptr, nargs, ..
            }) => {
                if index < *nargs as usize {
                    // SAFETY: variant contract — the caller's call-args
                    // slot outlives this entry.
                    Value::from_bits(unsafe { *args_ptr.add(index) } as usize)
                } else {
                    Value::NIL
                }
            }
            Some(other) => panic!(
                "backtrace_evaluated_arg_or_nil: expected EVALD Backtrace at specpdl[{count}], got {other:?}"
            ),
            None => panic!("backtrace_evaluated_arg_or_nil: specpdl index out of range"),
        }
    }

    #[inline]
    pub(crate) fn dispatch_subr_entry_from_backtrace_unchecked(
        &mut self,
        entry: SubrEntry,
        backtrace_count: usize,
    ) -> Option<EvalResult> {
        match entry.function? {
            SubrFn::A0(func) => Some(func(self)),
            SubrFn::A1(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                Some(func(self, arg0))
            }
            SubrFn::A2(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                Some(func(self, arg0, arg1))
            }
            SubrFn::A3(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                Some(func(self, arg0, arg1, arg2))
            }
            SubrFn::A4(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                Some(func(self, arg0, arg1, arg2, arg3))
            }
            SubrFn::A5(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                Some(func(self, arg0, arg1, arg2, arg3, arg4))
            }
            SubrFn::A6(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                let arg5 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 5);
                Some(func(self, arg0, arg1, arg2, arg3, arg4, arg5))
            }
            SubrFn::A7(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                let arg5 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 5);
                let arg6 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 6);
                Some(func(self, arg0, arg1, arg2, arg3, arg4, arg5, arg6))
            }
            SubrFn::A8(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                let arg5 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 5);
                let arg6 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 6);
                let arg7 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 7);
                Some(func(self, arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7))
            }
            SubrFn::Many(_) | SubrFn::ManyNoContext(_) | SubrFn::ManySlice(_) => None,
        }
    }

    #[inline]
    fn dispatch_subr_func_unchecked(
        &mut self,
        func: crate::tagged::header::SubrFn,
        args: LispArgVec,
    ) -> EvalResult {
        match func {
            crate::tagged::header::SubrFn::Many(func) => func(self, args.into_vec()),
            crate::tagged::header::SubrFn::ManyNoContext(func) => func(args.into_vec()),
            crate::tagged::header::SubrFn::ManySlice(func) => func(self, &args),
            crate::tagged::header::SubrFn::A0(func) => func(self),
            crate::tagged::header::SubrFn::A1(func) => {
                func(self, args.first().copied().unwrap_or(Value::NIL))
            }
            crate::tagged::header::SubrFn::A2(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A3(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A4(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A5(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A6(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
                args.get(5).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A7(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
                args.get(5).copied().unwrap_or(Value::NIL),
                args.get(6).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A8(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
                args.get(5).copied().unwrap_or(Value::NIL),
                args.get(6).copied().unwrap_or(Value::NIL),
                args.get(7).copied().unwrap_or(Value::NIL),
            ),
        }
    }

    #[inline]
    fn apply_subr_object(
        &mut self,
        function: Value,
        args: LispArgVec,
        _rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        let Some((sym_id, entry)) = subr_entry_from_value(function) else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };
        self.apply_subr_object_with_entry(sym_id, function, args, entry)
    }

    #[inline]
    fn apply_subr_object_with_entry(
        &mut self,
        sym_id: SymId,
        function: Value,
        args: LispArgVec,
        entry: SubrEntry,
    ) -> EvalResult {
        if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        }
        if entry.dispatch_kind == SubrDispatchKind::ContextCallable {
            return self.apply_evaluator_callable_by_id(sym_id, args);
        }
        if let Some(result) = self.dispatch_subr_entry_internal(entry, args, function) {
            result.map_err(|flow| self.validate_throw(flow))
        } else {
            Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            ))
        }
    }

    /// Apply a dynamic module function.
    #[inline]
    fn apply_module_function(&mut self, function: Value, args: LispArgVec) -> EvalResult {
        super::dynamic_module::apply_module_function(self, function, args.to_vec())
    }

    #[inline]
    fn resolve_named_call_target_by_id(&mut self, sym_id: SymId) -> NamedCallTarget {
        let compiler_overrides_active = self.compiler_function_overrides_active();
        let function_epoch = self.obarray.function_epoch();
        if !compiler_overrides_active {
            // Fast path: a HashMap lookup that returns the cached target
            // when the function epoch hasn't moved.  An epoch mismatch
            // signals that some `defalias`/`fset`/autoload installation
            // happened since the cached entry was recorded; in that case
            // fall through and replace the entry below.
            if let Some(entry) = self.named_call_cache.get(&sym_id)
                && entry.function_epoch == function_epoch
            {
                return entry.target.clone();
            }
        }

        let target =
            if let Some(func) = compiler_function_override_in_obarray(&self.obarray, sym_id) {
                NamedCallTarget::Obarray(func)
            } else if let Some(func) = self.obarray.symbol_function_id(sym_id) {
                match func.kind() {
                    ValueKind::Nil => NamedCallTarget::Void,
                    // `(fset 'foo (symbol-function 'foo))` writes `#<subr foo>` into
                    // the function cell. Treat this as the canonical callable
                    // object, not an obarray indirection cycle.
                    ValueKind::Subr(sid) if sid == sym_id => {
                        NamedCallTarget::Subr(Value::subr_from_sym_id(sid))
                    }
                    ValueKind::Veclike(VecLikeType::Subr) if func.as_subr_id() == Some(sym_id) => {
                        NamedCallTarget::Subr(func)
                    }
                    _ => NamedCallTarget::Obarray(func),
                }
            } else if self.obarray.is_function_unbound_id(sym_id) {
                NamedCallTarget::Void
            } else if lookup_global_subr_entry(sym_id).is_some() {
                NamedCallTarget::Subr(Value::subr_from_sym_id(sym_id))
            } else {
                NamedCallTarget::Void
            };

        if !compiler_overrides_active {
            // Cap the cache to avoid unbounded growth on pathologic
            // workloads.  Past the cap we just stop caching new entries
            // — better to take an O(1) miss than to evict a hot entry.
            if self.named_call_cache.len() < NAMED_CALL_CACHE_CAPACITY {
                self.named_call_cache.insert(
                    sym_id,
                    NamedCallCacheEntry {
                        function_epoch,
                        target: target.clone(),
                    },
                );
            }
        }

        target
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn resolve_named_call_target(&mut self, name: &str) -> NamedCallTarget {
        self.resolve_named_call_target_by_id(intern(name))
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn store_named_call_cache(&mut self, symbol: SymId, target: NamedCallTarget) {
        let function_epoch = self.obarray.function_epoch();
        if self.named_call_cache.len() < NAMED_CALL_CACHE_CAPACITY {
            self.named_call_cache.insert(
                symbol,
                NamedCallCacheEntry {
                    function_epoch,
                    target,
                },
            );
        }
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn apply_named_callable_by_id(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        let frame_function = Value::from_sym_id(sym_id);
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(frame_function, &args);
        let result = self.apply_named_callable_by_id_core(
            sym_id,
            args,
            invalid_fn,
            rewrite_builtin_wrong_arity,
        );
        self.unbind_to_with_result(bt_count, result)
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn apply_named_callable(
        &mut self,
        name: &str,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        let frame_function = Value::symbol(name);
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(frame_function, &args);
        let result =
            self.apply_named_callable_core(name, args, invalid_fn, rewrite_builtin_wrong_arity);
        self.unbind_to_with_result(bt_count, result)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn apply_named_callable_by_id_core(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        invalid_fn: Value,
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

                match self.apply_untraced(func, args) {
                    Err(Flow::Signal(sig))
                        if !function_is_callable && sig.symbol == invalid_function_symbol() =>
                    {
                        Err(signal(
                            LispCondition::InvalidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    }
                    other => other,
                }
            }
            NamedCallTarget::Subr(func) => {
                let Some((sym_id, entry)) = subr_entry_from_value(func) else {
                    return Err(signal(LispCondition::InvalidFunction, vec![invalid_fn]));
                };
                if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
                    return Err(signal(LispCondition::InvalidFunction, vec![invalid_fn]));
                }
                self.apply_subr_object_with_entry(sym_id, func, args, entry)
            }
            NamedCallTarget::Void => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            )),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn apply_named_callable_core(
        &mut self,
        name: &str,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        match self.resolve_named_call_target(name) {
            NamedCallTarget::Obarray(func) => {
                if super::autoload::is_autoload_value(&func) {
                    return self.apply_named_autoload_callable(
                        name,
                        func,
                        args,
                        rewrite_builtin_wrong_arity,
                    );
                }
                let function_is_callable = self.function_value_is_callable(&func);

                match self.apply(func, args) {
                    Err(Flow::Signal(sig))
                        if !function_is_callable && sig.symbol == invalid_function_symbol() =>
                    {
                        Err(signal(
                            LispCondition::InvalidFunction,
                            vec![Value::symbol(name)],
                        ))
                    }
                    other => other,
                }
            }
            NamedCallTarget::Subr(func) => {
                let _sym_id = intern(name);
                let result = self.apply_subr_object(func, args, rewrite_builtin_wrong_arity);
                // Do NOT poison the cache with Void when the subr was found.
                if func
                    .as_subr_id()
                    .and_then(lookup_global_subr_entry)
                    .is_some_and(|e| e.dispatch_kind == SubrDispatchKind::SpecialForm)
                {
                    Err(signal(LispCondition::InvalidFunction, vec![invalid_fn]))
                } else {
                    result
                }
            }
            NamedCallTarget::Void => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::symbol(name)],
            )),
        }
    }

    fn apply_named_autoload_callable(
        &mut self,
        name: &str,
        autoload_form: Value,
        args: LispArgVec,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        self.apply_named_autoload_callable_by_id(
            intern(name),
            autoload_form,
            args,
            rewrite_builtin_wrong_arity,
        )
    }

    fn apply_named_autoload_callable_by_id(
        &mut self,
        sym_id: SymId,
        autoload_form: Value,
        args: LispArgVec,
        _rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        // Startup wrappers often expose autoload-shaped function cells for names
        // backed by builtins. Keep the autoload shape while preserving callability.
        if lookup_global_subr_entry(sym_id).is_some() {
            let subr = Value::subr_from_sym_id(sym_id);
            // GNU-faithful pre-check via check_funcall_subr_arity.
            if let Some(flow) = self.check_funcall_subr_arity_value(subr, args.len()) {
                return Err(flow);
            }
            if let Some(result) = self.dispatch_subr_value_internal(
                subr,
                args.clone(),
                Value::subr_from_sym_id(sym_id),
            ) {
                return result;
            }
        }

        let mut current_autoload = autoload_form;
        let function = loop {
            match self.load_named_autoload_call_step(sym_id, current_autoload)? {
                NamedAutoloadCallStep::RetrySymbol { autoload_form } => {
                    // GNU `funcall_general` sets `fun = original_fun` and
                    // jumps to `retry`, preserving the symbol identity across
                    // any number of chained autoload declarations.
                    current_autoload = autoload_form;
                }
                NamedAutoloadCallStep::DispatchFunction { function } => break function,
                NamedAutoloadCallStep::Void => {
                    return Err(signal(
                        LispCondition::VoidFunction,
                        vec![Value::from_sym_id(sym_id)],
                    ));
                }
            }
        };

        let function_is_callable = self.function_value_is_callable(&function);
        match self.apply_untraced(function, args) {
            Err(Flow::Signal(sig))
                if !function_is_callable && sig.symbol == invalid_function_symbol() =>
            {
                Err(signal(
                    LispCondition::InvalidFunction,
                    vec![Value::from_sym_id(sym_id)],
                ))
            }
            other => other,
        }
    }

    fn load_named_autoload_call_step(
        &mut self,
        sym_id: SymId,
        autoload_form: Value,
    ) -> Result<NamedAutoloadCallStep, Flow> {
        let loaded = super::autoload::builtin_autoload_do_load(
            self,
            vec![autoload_form, Value::from_sym_id(sym_id)],
        )?;

        Ok(if loaded.is_nil() {
            NamedAutoloadCallStep::Void
        } else if super::autoload::is_autoload_value(&loaded) {
            NamedAutoloadCallStep::RetrySymbol {
                autoload_form: loaded,
            }
        } else {
            NamedAutoloadCallStep::DispatchFunction { function: loaded }
        })
    }

    fn apply_evaluator_callable_by_id(&mut self, sym_id: SymId, args: LispArgVec) -> EvalResult {
        match evaluator_handler(sym_id) {
            Some(EvaluatorHandler::Callable(CallableHandler::Throw)) => {
                if args.len() != 2 {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![
                            Value::subr_from_sym_id(sym_id),
                            Value::fixnum(args.len() as i64),
                        ],
                    ));
                }
                let tag = args[0];
                let value = args[1];
                if self.has_active_catch(&tag) {
                    Err(Flow::throw(tag, value))
                } else {
                    Err(signal(LispCondition::NoCatch, vec![tag, value]))
                }
            }
            Some(EvaluatorHandler::SpecialForm(_)) | None => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            )),
        }
    }

    fn apply_lambda(&mut self, func_value: Value, args: LispArgVec) -> EvalResult {
        let raw_cons_lambda = func_value.is_cons();
        let (arglist, body, env) = if raw_cons_lambda {
            let tail = func_value.cons_cdr();
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![func_value]));
            }
            (tail.cons_car(), tail.cons_cdr(), None)
        } else {
            let Some(arglist) = func_value.closure_slot(CLOSURE_ARGLIST) else {
                return Err(signal(LispCondition::InvalidFunction, vec![func_value]));
            };
            let Some(body) = func_value.closure_body_value() else {
                return Err(signal(LispCondition::InvalidFunction, vec![func_value]));
            };
            (arglist, body, func_value.closure_env().unwrap_or(None))
        };

        // Root the function value on the specpdl so GC can trace it
        // (keeping body, env, and params alive through the call).
        let root_count = self.specpdl.len();
        self.specpdl.push(SpecBinding::GcRoot { value: func_value });
        if raw_cons_lambda {
            let old_lexenv = std::mem::replace(&mut self.lexenv, Value::NIL);
            self.specpdl.push(SpecBinding::LexicalEnv { old_lexenv });
        }

        let call_state = match self.begin_lambda_call(func_value, arglist, env, &args) {
            Ok(state) => state,
            Err(err) => {
                return self.unbind_to_with_result(root_count, Err(err));
            }
        };
        let result = match self.eval_lambda_body_value(body) {
            Err(Flow::ThreadBlocked(blocked))
                if !blocked.remaining_forms.is_nil()
                    && crate::emacs_core::threads::thread_condition_case_continuation_parts(
                        blocked.remaining_forms,
                    )
                    .is_none() =>
            {
                match builtins::symbols::make_interpreted_closure_from_parts(
                    &Value::NIL,
                    &blocked.remaining_forms,
                    &self.lexenv,
                    None,
                    None,
                ) {
                    Ok(resume_function) => {
                        Err(Flow::thread_blocked(blocked.blocker, resume_function))
                    }
                    Err(flow) => Err(flow),
                }
            }
            other => other,
        };
        let result = self.finish_lambda_call(call_state, result);
        self.unbind_to_with_result(root_count, result)
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn bind_lexical_value_rooted(&mut self, sym: SymId, value: Value) {
        bind_lexical_value_rooted_in_specpdl(&mut self.lexenv, &mut self.specpdl, sym, value);
    }

    // -----------------------------------------------------------------------
    // Macro expansion
    // -----------------------------------------------------------------------

    pub(crate) fn with_macro_expansion_scope(
        &mut self,
        f: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        self.macro_expansion_scope_depth += 1;
        let scope_enter_start = self.macro_perf_enabled.then(std::time::Instant::now);
        let state = match self.begin_macro_expansion_scope_frame() {
            Ok(state) => state,
            Err(flow) => {
                self.macro_expansion_scope_depth =
                    self.macro_expansion_scope_depth.saturating_sub(1);
                return Err(flow);
            }
        };
        if let Some(start) = scope_enter_start {
            self.macro_perf_stats
                .scope_enter
                .note_duration(start.elapsed());
        }
        let result = f(self);
        let scope_exit_start = self.macro_perf_enabled.then(std::time::Instant::now);
        let result = self.finish_macro_expansion_scope_frame(state, result);
        if let Some(start) = scope_exit_start {
            self.macro_perf_stats
                .scope_exit
                .note_duration(start.elapsed());
        }
        self.macro_expansion_scope_depth = self.macro_expansion_scope_depth.saturating_sub(1);
        result
    }

    fn begin_macro_expansion_scope_frame(
        &mut self,
    ) -> Result<ActiveMacroExpansionScopeState, Flow> {
        let saved_specpdl_len = self.specpdl.len();
        let old_dynvars = self
            .obarray
            .symbol_value_id(macroexp_dynvars_symbol())
            .copied()
            .unwrap_or(Value::NIL);

        let dynvars_root_index = self.specpdl.len();
        self.specpdl
            .push(SpecBinding::GcRoot { value: old_dynvars });
        let mut dynvars = old_dynvars;
        // GNU eval.c walks Vinternal_interpreter_environment directly and only
        // extends `macroexp--dynvars` for bare symbols. Dynamic specpdl
        // bindings are not part of this macro-expansion state.
        let mut cursor = self.lexenv;
        while cursor.is_cons() {
            let entry = cursor.cons_car();
            if let Some(sym) = entry.as_symbol_id() {
                dynvars = Value::cons(Value::from_sym_id(sym), dynvars);
                match self.specpdl.get_mut(dynvars_root_index) {
                    Some(SpecBinding::GcRoot { value }) => *value = dynvars,
                    other => panic!("expected macro-expansion dynvars gc root, got {other:?}"),
                }
            }
            cursor = cursor.cons_cdr();
        }

        // GNU eval.c specbinds `lexical-binding' during ordinary macro calls
        // so the macro can know whether its expansion will be interpreted
        // lexically.  This must be a real specpdl binding: `lexical-binding'
        // is LOCALIZED, so writing the raw symbol default can leak across
        // buffers and diverges from GNU's SPECPDL_LET_LOCAL/DEFAULT behavior.
        self.try_specbind_or_unwind_to(
            saved_specpdl_len,
            lexical_binding_symbol(),
            Value::bool_val(!self.lexenv.is_nil()),
        )?;
        if !crate::emacs_core::value::eq_value(&dynvars, &old_dynvars) {
            self.try_specbind_or_unwind_to(saved_specpdl_len, macroexp_dynvars_symbol(), dynvars)?;
        }

        Ok(ActiveMacroExpansionScopeState { saved_specpdl_len })
    }

    fn finish_macro_expansion_scope_frame(
        &mut self,
        state: ActiveMacroExpansionScopeState,
        result: EvalResult,
    ) -> EvalResult {
        self.unbind_to_with_result(state.saved_specpdl_len, result)
    }

    #[inline]
    pub(crate) fn macro_expansion_mutation_epoch(&self) -> u64 {
        self.macro_expansion_mutation_epoch
    }

    #[inline]
    pub(crate) fn note_macro_expansion_mutation(&mut self) {
        if self.macro_expansion_scope_depth > 0 {
            self.macro_expansion_mutation_epoch =
                self.macro_expansion_mutation_epoch.wrapping_add(1);
        }
    }

    pub(crate) fn note_runtime_macro_expansion(
        &mut self,
        form: Value,
        expand_elapsed: std::time::Duration,
    ) {
        self.macro_expand_calls = self.macro_expand_calls.saturating_add(1);
        self.macro_expand_total_us = self
            .macro_expand_total_us
            .saturating_add(expand_elapsed.as_micros() as u64);
        if self.macro_perf_enabled && expand_elapsed.as_millis() > 50 {
            let macro_head = if form.is_cons() {
                form.cons_car().as_symbol_name().unwrap_or("<non-symbol>")
            } else {
                "<atom>"
            };
            let form_str = crate::emacs_core::print::print_value(&form);
            let form_preview: String = form_str.chars().take(200).collect();
            tracing::warn!(
                "runtime macro expansion head={macro_head} took {expand_elapsed:.2?} form={form_preview}"
            );
        }
    }

    fn apply_macro_callable_for_macroexpand(
        &mut self,
        callable: Value,
        args: Vec<Value>,
    ) -> Result<Value, Flow> {
        let perf_start = self.macro_perf_enabled.then(std::time::Instant::now);
        // GNU Fmacroexpand applies the macro expander directly.  The
        // eval.c macro-call path specbinds `lexical-binding`, but the
        // Fmacroexpand path does not; bytecomp relies on the current
        // buffer's visible `lexical-binding` while macroexpanding source.
        let result = self.apply(callable, args);
        if let Some(start) = perf_start {
            self.macro_perf_stats
                .macro_apply
                .note_duration(start.elapsed());
        }
        result
    }

    pub(crate) fn expand_macro_for_macroexpand(
        &mut self,
        form: Value,
        definition: Value,
        args: Vec<Value>,
        environment: Option<Value>,
    ) -> Result<Value, Flow> {
        let perf_start = self.macro_perf_enabled.then(std::time::Instant::now);
        let expand_start = std::time::Instant::now();
        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(form);
        self.push_specpdl_root(definition);
        if let Some(environment) = environment {
            self.push_specpdl_root(environment);
        }

        let result = (|| {
            let expanded = if definition.is_macro() {
                self.apply_macro_callable_for_macroexpand(definition, args)?
            } else if cons_head_symbol_id(&definition) == Some(macro_symbol()) {
                self.apply_macro_callable_for_macroexpand(definition.cons_cdr(), args)?
            } else if self.function_value_is_callable(&definition) {
                // GNU `macroexpand` ENVIRONMENT entries store the macro
                // expander itself, not the full `(macro . fn)` function cell.
                self.apply_macro_callable_for_macroexpand(definition, args)?
            } else {
                return Err(signal(LispCondition::InvalidFunction, vec![definition]));
            };

            self.note_runtime_macro_expansion(form, expand_start.elapsed());
            Ok(expanded)
        })();
        self.restore_specpdl_roots(specpdl_root_scope);
        if let Some(start) = perf_start {
            self.macro_perf_stats
                .expand_macro
                .note_duration(start.elapsed());
        }
        result
    }

    pub(crate) fn note_eager_macro_perf_step1(&mut self, duration: std::time::Duration) {
        if self.macro_perf_enabled {
            self.macro_perf_stats.eager_step1.note_duration(duration);
        }
    }

    pub(crate) fn note_eager_macro_perf_step3(&mut self, duration: std::time::Duration) {
        if self.macro_perf_enabled {
            self.macro_perf_stats.eager_step3.note_duration(duration);
        }
    }

    pub(crate) fn note_eager_macro_perf_step4(&mut self, duration: std::time::Duration) {
        if self.macro_perf_enabled {
            self.macro_perf_stats.eager_step4.note_duration(duration);
        }
    }

    pub(crate) fn macro_perf_summary(&self) -> Option<String> {
        if !self.macro_perf_enabled {
            return None;
        }

        let mut parts = vec![format!(
            "expansions:{} expand-total:{:.2}ms",
            self.macro_expand_calls,
            self.macro_expand_total_us as f64 / 1000.0
        )];

        for counter in [
            self.macro_perf_stats.scope_enter.summary("scope-enter"),
            self.macro_perf_stats.scope_exit.summary("scope-exit"),
            self.macro_perf_stats.macro_apply.summary("macro-apply"),
            self.macro_perf_stats.expand_macro.summary("expand-macro"),
            self.macro_perf_stats.eager_step1.summary("eager-step1"),
            self.macro_perf_stats.eager_step3.summary("eager-step3"),
            self.macro_perf_stats.eager_step4.summary("eager-step4"),
        ]
        .into_iter()
        .flatten()
        {
            parts.push(counter);
        }

        Some(parts.join(" | "))
    }

    #[inline]
    pub(crate) fn macro_perf_enabled(&self) -> bool {
        self.macro_perf_enabled
    }

    // -----------------------------------------------------------------------
    // Variable assignment
    // -----------------------------------------------------------------------

    // Shared runtime write path for symbol-cell mutation. This mirrors GNU
    // `set_internal` after lexical handling has already been decided.

    // -----------------------------------------------------------------------
    // specbind / unbind_to — GNU Emacs specpdl-style dynamic variable binding
    // -----------------------------------------------------------------------

    fn run_specbind_watcher(
        &mut self,
        sym_id: SymId,
        value: Value,
        operation: &'static str,
    ) -> Result<(), Flow> {
        if !self.watchers.has_watchers(sym_id) {
            return Ok(());
        }
        let where_value = self.variable_watcher_where_for_set_by_id(sym_id);
        self.run_variable_watchers_by_id_with_where(
            sym_id,
            &value,
            &Value::NIL,
            operation,
            &where_value,
        )
    }

    /// Save the current value of a special variable and set a new value.
    /// Matches GNU Emacs's `specbind` in eval.c:
    /// - Follows SYMBOL_VARALIAS to the final target
    /// - For buffer-local variables with a local binding: SPECPDL_LET_LOCAL
    /// - For buffer-local variables without local binding: SPECPDL_LET_DEFAULT
    /// - For plain variables: SPECPDL_LET
    fn specbind_resolved(&mut self, sym_id: SymId, value: Value) -> Result<(), Flow> {
        // GNU `specbind` switches on the redirect first: a plain value cell is
        // a specpdl push and one store (`SET_SYMBOL_VAL`).  Every other shape
        // — alias, forwarded, buffer-local, the undo list — takes the full
        // path below.  The plain tail of that path is reproduced exactly: the
        // constant check already ran in `sf_let`, and watchers still fire.
        if sym_id != buffer_undo_list_symbol()
            && let Some(sym) = self.obarray.get_by_id(sym_id)
            && sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
        {
            let old_value = SavedBindingValue::from_plain(sym.plain());
            self.specpdl.push(SpecBinding::Let { sym_id, old_value });
            self.run_specbind_watcher(sym_id, value, "let")?;
            self.obarray.store_plain_value_id(sym_id, value);
            self.sync_cached_runtime_binding_by_id(sym_id, value);
            return Ok(());
        }
        let resolved =
            builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id).unwrap_or(sym_id);

        // `buffer-undo-list` is a per-buffer variable in GNU.  Neomacs stores
        // it in SharedUndoState instead of the generic buffer-local alist, so
        // dynamic binding must update that shared state directly.  This is
        // required for GNU's `with-silent-modifications`, which binds
        // `buffer-undo-list` to t so font-lock/jit-lock text-property changes
        // do not enter the user's undo history.
        if resolved == buffer_undo_list_symbol()
            && let Some(buf_id) = self.buffers.current_buffer_id()
        {
            let old_value = self
                .buffers
                .get(buf_id)
                .map(|buf| buf.get_undo_list())
                .unwrap_or(Value::NIL);
            self.specpdl.push(SpecBinding::LetLocal {
                sym_id: resolved,
                old_value,
                buffer_id: buf_id,
            });
            self.run_specbind_watcher(resolved, value, "let")?;
            let _ = self
                .buffers
                .set_buffer_local_property_by_sym_id(buf_id, resolved, value);
            self.sync_cached_runtime_binding_by_id(resolved, value);
            return Ok(());
        }

        // ONE symbol fetch decides the arm, like GNU `specbind`'s redirect
        // switch over an in-hand `Lisp_Symbol *` (eval.c:3642). The POD facts
        // are captured so the obarray borrow ends before any mutation; every
        // arm below reuses them instead of re-fetching the symbol.
        use crate::emacs_core::symbol::SymbolRedirect;
        let (redirect, forwarded) = match self.obarray.get_by_id(resolved) {
            Some(sym) => {
                let redirect = sym.redirect();
                let fwd = (redirect == SymbolRedirect::Forwarded).then(|| unsafe { sym.val.fwd });
                (redirect, fwd)
            }
            None => (SymbolRedirect::Plainval, None),
        };

        // FORWARDED BUFFER_OBJFWD specbind, separate from the legacy
        // LOCALIZED path. Mirrors GNU `specbind` SYMBOL_FORWARDED arm at
        // `eval.c:3641-3677`.
        {
            use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
            if let Some(fwd_ptr) = forwarded {
                let fwd = unsafe { &*fwd_ptr };
                if matches!(fwd.ty, LispFwdType::BufferObj) {
                    let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                    let Some(slot) = crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)
                    else {
                        return Ok(());
                    };
                    let off = slot.index();
                    let flags_idx = buf_fwd.local_flags_idx;
                    let buf_id_opt = self.buffers.current_buffer_id();
                    let has_local = match buf_id_opt {
                        Some(id) => self
                            .buffers
                            .get(id)
                            .map(|buf| flags_idx < 0 || buf.slot_local_flag(slot))
                            .unwrap_or(false),
                        None => false,
                    };
                    if has_local {
                        // SPECPDL_LET_LOCAL — save the current
                        // per-buffer slot value, then overwrite. On
                        // unbind we restore via set_buffer_local
                        // which writes back to the slot.
                        let buf_id = buf_id_opt.expect("has_local implies current buffer");
                        let old_val = self
                            .buffers
                            .get(buf_id)
                            .map(|b| b.slots[off])
                            .unwrap_or(Value::NIL);
                        self.specpdl.push(SpecBinding::LetLocal {
                            sym_id: resolved,
                            old_value: old_val,
                            buffer_id: buf_id,
                        });
                        self.run_specbind_watcher(resolved, value, "let")?;
                        // `check_forwarded_store_at` at site=Bind with a local
                        // binding reduces to the descriptor's own typed store
                        // (the predicate check GNU does in
                        // `store_symval_forwarding`); everything else it
                        // derives — fwd, slot, has_local — is already in hand.
                        let stored = match fwd.store(value) {
                            Ok(store) => store.canonical_value(),
                            Err(error) => return Err(forward_store_signal(error, value)),
                        };
                        if let Some(buf) = self.buffers.get_mut(buf_id) {
                            buf.slots[off] = stored;
                            // Always-local slots need no flag
                            // change; conditional slots already
                            // have the bit set (has_local check).
                        }
                        return Ok(());
                    } else {
                        // SPECPDL_LET_DEFAULT — save old default,
                        // propagate the new value via
                        // set_buffer_default_slot. On unbind we
                        // propagate the saved default back.
                        let old_default = if off < self.buffers.buffer_defaults.len() {
                            Some(self.buffers.buffer_defaults[off])
                        } else {
                            Some(buf_fwd.default)
                        };
                        self.specpdl.push(SpecBinding::LetDefault {
                            sym_id: resolved,
                            old_value: SavedBindingValue::from_option(old_default),
                            buffer_id: SavedBufferId::from_option(buf_id_opt),
                        });
                        // GNU routes a BUFFER_OBJFWD default binding through
                        // data.c's `set_default_internal`; its watcher
                        // operation is `set` even though the write was caused
                        // by a `let`.
                        super::data::set_default_internal_resolved(
                            self,
                            resolved,
                            value,
                            crate::emacs_core::symbol::SetInternalBind::Bind,
                        )?;
                        return Ok(());
                    }
                }
            }
        }

        // Phase 10E: SYMBOL_LOCALIZED specbind. Mirrors GNU `specbind`
        // SYMBOL_LOCALIZED arm at `eval.c:3641-3677`:
        //
        //   1. Read the current value (forces BLV swap-in to current
        //      buffer).
        //   2. Tentatively record SPECPDL_LET_LOCAL with the captured
        //      value and buffer.
        //   3. If !blv_found(blv) (the swap-in landed on defcell, not
        //      a per-buffer alist entry), demote to SPECPDL_LET_DEFAULT.
        //   4. Call set_internal_localized(BIND) to write the new
        //      value into wherever the BLV cache currently points.
        if redirect == SymbolRedirect::Localized
            && let Some(buf_id) = self.buffers.current_buffer_id()
        {
            let (cur_val, alist) = match self.buffers.get(buf_id) {
                Some(buf) => (Value::make_buffer(buf.id), buf.local_var_alist_value()),
                None => (Value::NIL, Value::NIL),
            };
            // Force a swap so blv.found / blv.valcell match the
            // current buffer state. After this, blv.where_buf =
            // cur_val.
            let old_val = self
                .obarray
                .find_symbol_value_in_buffer(
                    resolved,
                    Some(buf_id),
                    cur_val,
                    alist,
                    None,
                    0u64,
                    None,
                )
                .unwrap_or(Value::NIL);
            let has_local_binding = self
                .obarray
                .has_per_buffer_binding(resolved, cur_val, alist);
            if has_local_binding {
                self.specpdl.push(SpecBinding::LetLocal {
                    sym_id: resolved,
                    old_value: old_val,
                    buffer_id: buf_id,
                });
            } else {
                self.specpdl.push(SpecBinding::LetDefault {
                    sym_id: resolved,
                    old_value: SavedBindingValue::from_option(Some(old_val)),
                    buffer_id: SavedBufferId::from_option(Some(buf_id)),
                });
            }
            self.run_specbind_watcher(resolved, value, "let")?;
            let stored = check_forwarded_store_at(
                &self.obarray,
                &self.buffers,
                &self.specpdl,
                resolved,
                value,
                ForwardStoreSite::Bind,
            )?
            .value();
            // Write the new value via set_internal_localized
            // with bindflag=Bind. Bind never auto-creates a new
            // alist entry, so a let on a non-buffer-local
            // LOCALIZED symbol writes to defcell.cdr (the
            // global default), matching GNU.
            let new_alist = self.obarray.set_internal_localized(
                resolved,
                stored,
                cur_val,
                alist,
                crate::emacs_core::symbol::SetInternalBind::Bind,
                false,
            );
            if let Some(buf) = self.buffers.get_mut(buf_id) {
                buf.replace_local_var_alist(new_alist);
            }
            self.sync_cached_runtime_binding_by_id(resolved, stored);
            return Ok(());
        }

        // Plain value path (GNU: SYMBOL_PLAINVAL). A PLAINVAL symbol has no
        // forward descriptor (`assignment_forwarder` is None by redirect), so
        // the typed-store probe is pure overhead for it; non-buffer forwarded
        // symbols (Int/Bool/Obj/Kboard) still take it so `(let
        // ((gc-cons-threshold "x")) ...)` keeps signaling before the body.
        let old_value = self.obarray.symbol_value_id(resolved).copied();
        self.specpdl.push(SpecBinding::Let {
            sym_id: resolved,
            old_value: SavedBindingValue::from_option(old_value),
        });
        self.run_specbind_watcher(resolved, value, "let")?;
        let stored = if redirect == SymbolRedirect::Plainval {
            value
        } else {
            check_forwarded_store_at(
                &self.obarray,
                &self.buffers,
                &self.specpdl,
                resolved,
                value,
                ForwardStoreSite::Bind,
            )?
            .value()
        };
        self.obarray.set_symbol_value_id(resolved, stored);
        self.sync_cached_runtime_binding_by_id(resolved, stored);
        Ok(())
    }

    /// GNU-compatible checked entry point for dynamic binding.
    ///
    /// GNU's `specbind` reaches the same `store_symval_forwarding` an ordinary
    /// `setq` does -- it calls `set_internal (..., SET_INTERNAL_BIND)` for
    /// every forwarded symbol (`src/eval.c:3641-3677`) -- which is why
    /// `(let ((undo-limit "x")) ...)` signals before the body ever runs.
    pub(crate) fn try_specbind(&mut self, sym_id: SymId, value: Value) -> Result<(), Flow> {
        self.specbind_resolved(sym_id, value)
    }

    /// Enter one dynamic binding inside an already-established specpdl scope.
    /// If binding itself exits nonlocally (for example from a variable
    /// watcher), drain this binding and every earlier entry in the scope before
    /// returning that flow. This is the fallible counterpart of GNU callers'
    /// `specbind` + surrounding `unbind_to` pattern.
    pub(crate) fn try_specbind_or_unwind_to(
        &mut self,
        scope_count: usize,
        sym_id: SymId,
        value: Value,
    ) -> Result<(), Flow> {
        match self.try_specbind(sym_id, value) {
            Ok(()) => Ok(()),
            Err(flow) => match self.unbind_to_with_result(scope_count, Err(flow)) {
                Err(flow) => Err(flow),
                Ok(_) => unreachable!("unwinding an error cannot produce a value"),
            },
        }
    }

    /// Check if a `let` is currently shadowing a buffer-local
    /// variable's binding. Matches GNU
    /// `eval.c:3559-3577 (let_shadows_buffer_binding_p)`.
    ///
    /// When true, `setq` inside the let should modify the existing
    /// binding (whichever specpdl record is on top) rather than
    /// auto-creating a brand-new per-buffer binding.
    ///
    /// GNU walks the specpdl looking for SPECPDL_LET_DEFAULT records
    /// keyed to the symbol in the current buffer. SPECPDL_LET_LOCAL is
    /// explicitly excluded (GNU bug#62419), because a let over an
    /// existing buffer-local binding must keep writes in that local
    /// binding instead of treating the default as shadowed.
    pub(crate) fn let_shadows_buffer_binding_p(&self, sym_id: SymId) -> bool {
        let current = self.buffers.current_buffer_id();
        self.specpdl.iter().rev().any(|entry| match entry {
            SpecBinding::LetDefault {
                sym_id: s,
                buffer_id,
                ..
            } => *s == sym_id && buffer_id.get() == current,
            SpecBinding::LetLocal { .. } => false,
            SpecBinding::Let { .. }
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

    fn restore_default_binding_by_id(
        &mut self,
        sym_id: SymId,
        old_value: Option<Value>,
        bindflag: crate::emacs_core::symbol::SetInternalBind,
    ) -> Result<(), Flow> {
        // GNU's do_one_unbind and thread switching both call data.c's
        // `set_default_internal`, with the bind flag carrying the policy
        // difference. The shared storage seam also republishes the now-visible
        // runtime value and invalidates retained redisplay state.
        let value = old_value.unwrap_or(Value::UNBOUND);
        super::data::set_default_internal_resolved(self, sym_id, value, bindflag)?;
        // The evaluator caches a few of these cells in its own fields (the
        // quit flags, `throw-on-input`, the eval-depth limit) so its hot
        // paths do not consult the obarray. The plain-cell restore in
        // `unbind_to_result` republishes them; this one restores a
        // LOCALIZED/FORWARDED cell and must do the same, or a cache keeps a
        // value the binding it mirrors has already given up.
        self.sync_cached_runtime_binding_by_id(sym_id, old_value.unwrap_or(Value::NIL));
        Ok(())
    }

    /// Restore all specpdl bindings back to `count`.
    /// Matches GNU Emacs's unbind_to() in eval.c.
    pub(crate) fn unbind_to(&mut self, count: usize) {
        // Recovery/invariant-only callers have no Lisp result channel, but
        // they must still drain the whole suffix if cleanup signals. Normal
        // evaluator/VM paths use `unbind_to_with_result` and propagate it.
        let _ = self.drain_unwind_to(count, Ok(Value::NIL));
    }

    fn local_binding_value_for_thread_switch(
        &self,
        sym_id: SymId,
        buffer_id: crate::buffer::BufferId,
    ) -> Option<Value> {
        self.buffers
            .get(buffer_id)
            .and_then(|buf| buf.get_buffer_local_binding_by_sym_id(sym_id))
            .map(|binding| binding.as_value().unwrap_or(Value::UNBOUND))
    }

    fn set_local_binding_for_thread_switch(
        &mut self,
        sym_id: SymId,
        buffer_id: crate::buffer::BufferId,
        value: Value,
    ) {
        use crate::emacs_core::symbol::{SetInternalBind, SymbolRedirect};

        let is_localized = self
            .obarray
            .get_by_id(sym_id)
            .map(|s| s.redirect() == SymbolRedirect::Localized)
            .unwrap_or(false);
        if is_localized {
            let buf_val = Value::make_buffer(buffer_id);
            let alist = self
                .buffers
                .get(buffer_id)
                .map(|buf| buf.local_var_alist_value())
                .unwrap_or(Value::NIL);
            let new_alist = self.obarray.set_internal_localized(
                sym_id,
                value,
                buf_val,
                alist,
                SetInternalBind::ThreadSwitch,
                false,
            );
            if let Some(buf) = self.buffers.get_mut(buffer_id) {
                buf.replace_local_var_alist(new_alist);
            }
        } else if value.is_unbound() {
            let _ = self
                .buffers
                .set_buffer_local_void_property_by_sym_id(buffer_id, sym_id);
        } else {
            let _ = self
                .buffers
                .set_buffer_local_property_by_sym_id(buffer_id, sym_id, value);
        }
        self.sync_cached_runtime_binding_by_id(sym_id, value);
    }

    fn swap_let_binding_for_thread_switch(&mut self, index: usize) -> Result<(), Flow> {
        let (sym_id, old_value, originally_default_binding) = match self.specpdl.get(index) {
            Some(SpecBinding::Let { sym_id, old_value }) => (*sym_id, old_value.get(), false),
            Some(SpecBinding::LetDefault {
                sym_id, old_value, ..
            }) => (*sym_id, old_value.get(), true),
            _ => return Ok(()),
        };
        // GNU rechecks the redirect on every thread switch. A plain binding
        // can become LOCALIZED/FORWARDED inside its dynamic extent; from that
        // point it must fall through to the default-value path instead of
        // swapping the current buffer's local value into the saved default.
        let still_plain = self.obarray.get_by_id(sym_id).is_none_or(|symbol| {
            symbol.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
        });
        let use_default_storage = originally_default_binding || !still_plain;
        let current_value = if use_default_storage {
            super::data::default_value_by_id(self, sym_id)
        } else {
            self.obarray.symbol_value_id(sym_id).copied()
        };
        if use_default_storage {
            self.restore_default_binding_by_id(
                sym_id,
                old_value,
                crate::emacs_core::symbol::SetInternalBind::ThreadSwitch,
            )?;
        } else {
            match old_value {
                Some(value) => {
                    self.obarray.set_symbol_value_id(sym_id, value);
                    self.sync_cached_runtime_binding_by_id(sym_id, value);
                }
                None => {
                    self.obarray.makunbound_id(sym_id);
                    self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
                }
            }
        }
        // Commit the exchange only after the fallible forwarded/default store
        // succeeds.  GNU permits thread-switch unrewind to signal; retaining
        // the requested saved value lets the caller handle the error without
        // silently losing the binding it failed to install.
        match self.specpdl.get_mut(index) {
            Some(SpecBinding::Let {
                old_value: saved_value,
                ..
            })
            | Some(SpecBinding::LetDefault {
                old_value: saved_value,
                ..
            }) => {
                saved_value.set(current_value);
            }
            _ => {}
        }
        Ok(())
    }

    fn swap_local_let_binding_for_thread_switch(&mut self, index: usize) {
        let (sym_id, old_value, buffer_id) = match self.specpdl.get(index) {
            Some(SpecBinding::LetLocal {
                sym_id,
                old_value,
                buffer_id,
            }) => (*sym_id, *old_value, *buffer_id),
            _ => return,
        };
        let Some(current_value) = self.local_binding_value_for_thread_switch(sym_id, buffer_id)
        else {
            if let Some(binding) = self.specpdl.get_mut(index) {
                *binding = SpecBinding::Nop;
            }
            return;
        };
        if let Some(SpecBinding::LetLocal { old_value, .. }) = self.specpdl.get_mut(index) {
            *old_value = current_value;
        }
        self.set_local_binding_for_thread_switch(sym_id, buffer_id, old_value);
    }

    fn specpdl_unrewind_vars_for_thread_switch(&mut self, rewind: bool) -> Result<(), Flow> {
        let indices: Vec<usize> = if rewind {
            (0..self.specpdl.len()).collect()
        } else {
            (0..self.specpdl.len()).rev().collect()
        };
        let mut swapped = Vec::with_capacity(indices.len());
        for index in indices {
            if let Err(flow) = self.swap_let_binding_for_thread_switch(index) {
                // Thread switching is an exchange, so replaying every
                // completed exchange in reverse order restores both live
                // storage and the saved specpdl cells.  This matters when an
                // outer forwarded binding rejects its saved value after an
                // inner binding has already been exchanged: the old thread is
                // still current and must remain observably unchanged when the
                // signal is caught.
                for swapped_index in swapped.into_iter().rev() {
                    let rollback = self.swap_let_binding_for_thread_switch(swapped_index);
                    debug_assert!(
                        rollback.is_ok(),
                        "a successful thread-binding exchange must be reversible"
                    );
                    self.swap_local_let_binding_for_thread_switch(swapped_index);
                }
                self.lexenv_assq_cache.clear();
                self.lexenv_special_cache.clear();
                return Err(flow);
            }
            self.swap_local_let_binding_for_thread_switch(index);
            swapped.push(index);
        }
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        Ok(())
    }

    pub(crate) fn suspend_dynamic_bindings_for_thread_switch(
        &mut self,
    ) -> Result<ThreadDynamicBindingToken, Flow> {
        let lexenv = std::mem::replace(&mut self.lexenv, Value::NIL);
        if let Err(flow) = self.specpdl_unrewind_vars_for_thread_switch(false) {
            self.lexenv = lexenv;
            self.lexenv_assq_cache.clear();
            self.lexenv_special_cache.clear();
            return Err(flow);
        }
        let suspended_depth = self.suspended_thread_bindings.len();
        self.suspended_thread_bindings
            .push(ThreadDynamicBindingState {
                lexenv,
                specpdl: std::mem::take(&mut self.specpdl),
                condition_stack: std::mem::take(&mut self.condition_stack),
            });
        Ok(ThreadDynamicBindingToken { suspended_depth })
    }

    pub(crate) fn resume_dynamic_bindings_for_thread_switch(
        &mut self,
        token: ThreadDynamicBindingToken,
    ) -> Result<(), Flow> {
        assert!(
            self.specpdl.is_empty(),
            "a simulated thread must unwind its active specpdl before switching out"
        );
        assert!(
            self.condition_stack.is_empty(),
            "a simulated thread must unwind its active handlers before switching out"
        );
        assert_eq!(
            self.suspended_thread_bindings.len(),
            token.suspended_depth + 1,
            "simulated thread binding stacks must resume in LIFO order"
        );
        let state = self
            .suspended_thread_bindings
            .pop()
            .expect("validated suspended thread binding depth");
        self.specpdl = state.specpdl;
        self.condition_stack = state.condition_stack;
        let result = self.specpdl_unrewind_vars_for_thread_switch(true);
        // The thread is current by the time its bindings are resumed.  Its
        // lexical environment therefore belongs to the error handler too if
        // a forwarded dynamic value rejects the exchange.
        self.lexenv = state.lexenv;
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        result
    }

    pub(crate) fn unbind_to_result(&mut self, count: usize) -> Result<(), Flow> {
        // Mirrors GNU `unbind_to` in `eval.c:3907-3930`: suppress a
        // pending quit during cleanup so `unwind-protect` cleanup forms
        // run to completion, then restore the pending state on exit if
        // no inner form replaced it. Without this an interactive `C-g`
        // arriving during a long-running protected form would abort the
        // CLEANUP clause mid-way, leaving resources in a bad state.
        let quitf = self.quit_flag_value();
        if !quitf.is_nil() {
            self.set_quit_flag_value(Value::NIL);
        }
        let result = (|| -> Result<(), Flow> {
            while self.specpdl.len() > count {
                let binding = self.specpdl.pop().unwrap();
                match binding {
                    SpecBinding::Let { sym_id, old_value } => {
                        let old_value = old_value.get();
                        let still_plain = self.obarray.get_by_id(sym_id).is_none_or(|s| {
                            s.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
                        });
                        if still_plain {
                            if self.watchers.has_watchers(sym_id) {
                                let restore_val = old_value.unwrap_or(Value::NIL);
                                self.run_variable_watchers_by_id(
                                    sym_id,
                                    &restore_val,
                                    &Value::NIL,
                                    "unlet",
                                )?;
                            }
                            match old_value {
                                Some(val) => {
                                    self.obarray.set_symbol_value_id(sym_id, val);
                                    self.sync_cached_runtime_binding_by_id(sym_id, val);
                                }
                                None => {
                                    self.obarray.makunbound_id(sym_id);
                                    self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
                                }
                            }
                        } else {
                            self.restore_default_binding_by_id(
                                sym_id,
                                old_value,
                                crate::emacs_core::symbol::SetInternalBind::Unbind,
                            )?;
                        }
                    }
                    SpecBinding::LetLocal {
                        sym_id,
                        old_value,
                        buffer_id,
                    } => {
                        // Restore only if the buffer is still live AND the
                        // variable is *still* buffer-local in that buffer.
                        // Mirrors GNU `do_one_unbind` SPECPDL_LET_LOCAL
                        // arm at `eval.c:3852-3863`:
                        //     /* If this was a local binding, reset the value in
                        //        the appropriate buffer, but only if that buffer's
                        //        binding still exists.  */
                        //     if (!NILP (Flocal_variable_p (symbol, where)))
                        //       set_internal (symbol, old_value, where, UNBIND);
                        //
                        // The `Flocal_variable_p` guard is load-bearing: if the
                        // local binding was eliminated *inside* the `let` body
                        // (e.g. `kill-all-local-variables` killed a non-permanent
                        // local), GNU does NOT restore the old value — the kill
                        // wins. Without this guard neomacs resurrected the old
                        // local value, leaking stale buffer-local state across a
                        // major-mode switch (the org/derived-mode hook-loss path:
                        // `delay-mode-hooks`/`delayed-mode-hooks` machinery relies
                        // on KALV's reset surviving the surrounding `let`).
                        use crate::emacs_core::symbol::{SetInternalBind, SymbolRedirect};
                        let is_localized = self
                            .obarray
                            .get_by_id(sym_id)
                            .map(|s| s.redirect() == SymbolRedirect::Localized)
                            .unwrap_or(false);
                        let still_local = match self.buffers.get(buffer_id) {
                            None => false,
                            Some(buf) => {
                                if is_localized {
                                    let buf_val = Value::make_buffer(buffer_id);
                                    self.obarray.has_per_buffer_binding(
                                        sym_id,
                                        buf_val,
                                        buf.local_var_alist_value(),
                                    )
                                } else {
                                    // `is_localized` is false here, so a non-slot,
                                    // non-undo symbol is never in the alist: gate the
                                    // scan away (slot/undo still resolve).
                                    buf.has_buffer_local_by_sym_id_gated(sym_id, false)
                                }
                            }
                        };
                        if still_local {
                            if self.watchers.has_watchers(sym_id) {
                                self.run_variable_watchers_by_id_with_where(
                                    sym_id,
                                    &old_value,
                                    &Value::NIL,
                                    "unlet",
                                    &Value::make_buffer(buffer_id),
                                )?;
                            }
                            // Phase 10E: for LOCALIZED symbols, restore via
                            // set_internal_localized(UNBIND) targeting the
                            // saved buffer. This walks the buffer's alist
                            // and rewrites the cell's cdr in place,
                            // matching GNU's set_internal LOCALIZED arm
                            // and bypassing the legacy lisp_bindings path.
                            if is_localized {
                                let buf_val = Value::make_buffer(buffer_id);
                                let alist = self
                                    .buffers
                                    .get(buffer_id)
                                    .map(|buf| buf.local_var_alist_value())
                                    .unwrap_or(Value::NIL);
                                let new_alist = self.obarray.set_internal_localized(
                                    sym_id,
                                    old_value,
                                    buf_val,
                                    alist,
                                    SetInternalBind::Unbind,
                                    false,
                                );
                                if let Some(buf) = self.buffers.get_mut(buffer_id) {
                                    buf.replace_local_var_alist(new_alist);
                                }
                            } else {
                                let _ = self.buffers.set_buffer_local_property_by_sym_id(
                                    buffer_id, sym_id, old_value,
                                );
                            }
                            self.sync_cached_runtime_binding_by_id(sym_id, old_value);
                        }
                    }
                    SpecBinding::LetDefault {
                        sym_id, old_value, ..
                    } => {
                        let old_value = old_value.get();
                        self.restore_default_binding_by_id(
                            sym_id,
                            old_value,
                            crate::emacs_core::symbol::SetInternalBind::Unbind,
                        )?;
                    }
                    SpecBinding::LexicalEnv { old_lexenv } => {
                        // Mirrors GNU unbind_to for
                        // specbind(Qinternal_interpreter_environment, ...).
                        self.lexenv = old_lexenv;
                    }
                    SpecBinding::GcRoot { .. } => {}
                    SpecBinding::Backtrace { args, .. } => {
                        self.release_backtrace_args(&args);
                        // No-op, matches GNU SPECPDL_BACKTRACE
                    }
                    SpecBinding::Backtrace1 { .. }
                    | SpecBinding::Backtrace2 { .. }
                    | SpecBinding::BacktraceNative { .. } => {
                        // Inline evaluated backtraces own no side-stack payload.
                    }
                    SpecBinding::Nop => {
                        // No-op, matches GNU SPECPDL_NOP
                    }
                    SpecBinding::UnwindProtect {
                        forms: cleanup,
                        lexenv,
                    } => match self.lisp_execution() {
                        // GNU's `Fkill_emacs` is `attributes: noreturn`
                        // (src/emacs.c:2974) and ends in `exit (exit_code)` (:3088)
                        // without ever reaching `unbind_to`, so a cleanup form
                        // still on the specpdl when `kill-emacs` is called
                        // never runs.  This port has to drain the specpdl to
                        // walk back out to `main`; the drain must not evaluate
                        // what GNU has already exited past.  The binding
                        // restorations below/above still run -- see
                        // [`LispExecution`] for why that is invisible.
                        LispExecution::ExitedAlready => {}
                        LispExecution::Live => {
                            // Entry already popped — re-entrant errors won't re-unwind.
                            let saved_lexenv = self.lexenv;
                            self.lexenv = lexenv;
                            let cleanup_result = {
                                let mut guard = UnwindCleanupGuard::enter(self);
                                if cleanup.is_cons() || cleanup.is_nil() {
                                    // Interpreter path: list of forms
                                    guard.context().sf_progn_value(cleanup)
                                } else {
                                    // VM path: callable (bytecode function)
                                    guard.context().apply(cleanup, vec![])
                                }
                            };
                            self.lexenv = saved_lexenv;
                            cleanup_result?;
                        }
                    },
                    SpecBinding::SaveExcursion {
                        buffer_id,
                        marker_id,
                        marker: _,
                    } => {
                        self.restore_current_buffer_if_live(buffer_id);
                        if let Some(saved_pt) =
                            self.buffers.marker_emacs_byte_pos(buffer_id, marker_id)
                        {
                            let _ = self.buffers.goto_buffer_emacs_byte_pos(buffer_id, saved_pt);
                        }
                        self.buffers.remove_marker(marker_id);
                    }
                    SpecBinding::SaveCurrentBuffer { buffer_id } => {
                        self.restore_current_buffer_if_live(buffer_id);
                    }
                    SpecBinding::SaveRestriction { state } => {
                        self.buffers
                            .restore_saved_restriction_state(state.into_state());
                    }
                    SpecBinding::LoadsInProgress { len } => {
                        self.loads_in_progress.truncate(len);
                    }
                    SpecBinding::RequireStack { len } => {
                        self.require_stack.truncate(len);
                    }
                    SpecBinding::NativeUnwind { action } => {
                        action.run(self)?;
                    }
                }
            }
            Ok(())
        })();
        // If cleanup forms didn't set their own quit, reinstate the
        // pending state. Matches `eval.c:3927-3928`.
        if !quitf.is_nil() && self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(quitf);
        }
        result
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
    // Methods previously on VmSharedState, now on Context directly
    // -----------------------------------------------------------------------

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn begin_eval_with_lexical_arg(
        &mut self,
        lexical_arg: Option<Value>,
    ) -> Result<ActiveEvalLexicalArgState, Flow> {
        begin_eval_with_lexical_arg_in_state(
            &mut self.obarray,
            &mut self.lexenv,
            &mut self.specpdl,
            lexical_arg,
        )
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn finish_eval_with_lexical_arg(&mut self, state: ActiveEvalLexicalArgState) {
        finish_eval_with_lexical_arg_in_state(
            &mut self.obarray,
            &mut self.lexenv,
            &mut self.specpdl,
            state,
        );
    }

    pub(crate) fn begin_macro_expansion_scope(
        &mut self,
    ) -> Result<ActiveMacroExpansionScopeState, Flow> {
        self.macro_expansion_scope_depth += 1;
        match self.begin_macro_expansion_scope_frame() {
            Ok(state) => Ok(state),
            Err(flow) => {
                self.macro_expansion_scope_depth =
                    self.macro_expansion_scope_depth.saturating_sub(1);
                Err(flow)
            }
        }
    }

    pub(crate) fn finish_macro_expansion_scope(
        &mut self,
        state: ActiveMacroExpansionScopeState,
        result: EvalResult,
    ) -> EvalResult {
        let result = self.finish_macro_expansion_scope_frame(state, result);
        self.macro_expansion_scope_depth = self.macro_expansion_scope_depth.saturating_sub(1);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn kmacro_mut(&mut self) -> &mut KmacroManager {
        &mut self.kmacro
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn gui_frame_creation_state(
        &mut self,
    ) -> (
        &mut FrameManager,
        &mut BufferManager,
        &mut Option<Box<dyn DisplayHost>>,
    ) {
        (&mut self.frames, &mut self.buffers, &mut self.display_host)
    }

    pub(crate) fn recursive_command_loop_depth(&self) -> usize {
        // GNU's `command_loop_level` starts at -1 before entering the
        // top-level recursive edit, so ordinary interactive execution happens
        // at level 0. Neomacs stores the raw active-loop count instead
        // (0 outside the loop, 1 at top level), so translate here to the
        // GNU-visible level used by mode-line and minibuffer semantics.
        self.command_loop.recursive_depth.saturating_sub(1)
    }

    /// Lisp-visible recursive-edit depth, matching GNU's
    /// `command_loop_level + minibuf_level`.
    pub(crate) fn recursion_depth(&self) -> usize {
        self.recursive_command_loop_depth()
            .saturating_add(self.minibuffers.depth())
    }

    pub(crate) fn interactive_minibuffer_read_count(&self) -> u64 {
        self.interactive_minibuffer_read_count
    }

    // --- Post-command point adjustment (GNU `keyboard.c`) -------------------

    /// Current buffer point as a 1-based Lisp char position.
    fn apfp_point(&self, id: crate::buffer::BufferId) -> i64 {
        self.buffers
            .get(id)
            .map(|b| b.point_char_pos().to_lisp().as_i64())
            .unwrap_or(1)
    }

    /// Raw `SET_PT` equivalent: move point without running point-motion or
    /// intangibility hooks (GNU's `adjust_point_for_property` uses `SET_PT`).
    fn apfp_set_point(&mut self, id: crate::buffer::BufferId, lisp_pos: i64) {
        let byte = match self.buffers.get(id) {
            Some(b) => b.lisp_pos_to_accessible_emacs_byte_pos(
                crate::buffer::position::LispCharPos1::new(lisp_pos.max(1)),
            ),
            None => return,
        };
        let _ = self.buffers.goto_buffer_emacs_byte_pos(id, byte);
    }

    fn apfp_char_property(&mut self, pos: i64, prop: Value) -> Result<Value, Flow> {
        super::textprop::builtin_get_char_property(self, vec![Value::fixnum(pos), prop, Value::NIL])
    }

    fn apfp_pos_property(&mut self, pos: i64, prop: Value) -> Result<Value, Flow> {
        super::builtins::misc_eval::builtin_get_pos_property(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL],
        )
    }

    fn apfp_next_change(&mut self, pos: i64, prop: Value, zv: i64) -> Result<i64, Flow> {
        let v = super::builtins::misc_eval::builtin_next_single_char_property_change(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL, Value::NIL],
        )?;
        Ok(v.as_fixnum().unwrap_or(zv))
    }

    fn apfp_prev_change(&mut self, pos: i64, prop: Value, begv: i64) -> Result<i64, Flow> {
        let v = super::builtins::misc_eval::builtin_previous_single_char_property_change(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL, Value::NIL],
        )?;
        Ok(v.as_fixnum().unwrap_or(begv))
    }

    /// Port of GNU `keyboard.c:adjust_point_for_property`, invisible-text
    /// branch.  After a command moves point, GNU never leaves point resting
    /// inside an `invisible` region — it relocates point to a region boundary
    /// so the cursor is visible.  Without this, motion commands (e.g. evil
    /// `e`) that land inside org's invisible link-target text leave the cursor
    /// parked where the display collapses the hidden run to a single column,
    /// so it appears frozen.
    ///
    /// GNU also adjusts for `display`-intangible and composition here; those
    /// branches are not yet ported (they only add further adjustments that
    /// neomacs does not otherwise perform).  The invisible branch is iterated
    /// to a fixpoint, mirroring GNU's `check_*` re-entry loop.
    pub(crate) fn adjust_point_for_property(
        &mut self,
        last_pt: i64,
        modified: bool,
    ) -> Result<(), Flow> {
        let Some(id) = self.buffers.current_buffer_id() else {
            return Ok(());
        };
        let inv = Value::symbol("invisible");
        let spec = self
            .eval_symbol_by_id(intern("buffer-invisibility-spec"))
            .unwrap_or(Value::NIL);
        let display_sym = Value::symbol("display");
        // GNU's `FRAME_WINDOW_P (selected_frame)`: image/xwidget `display`
        // specs replace text (and so make it intangible) only on a GUI frame.
        let frame_window_p = self
            .frames
            .selected_frame()
            .map(|frame| frame.effective_window_system().is_some())
            .unwrap_or(false);

        // `orig_pt` mirrors GNU: point on entry, used to detect "we have not
        // moved yet" so the boundary-choice heuristic stays free.
        let mut orig_pt: i64 = self.apfp_point(id);

        for _ in 0..50 {
            let pt = self.apfp_point(id);
            let (begv, zv) = match self.buffers.get(id) {
                Some(b) => (
                    b.point_min_lisp_char_pos().as_i64(),
                    b.point_max_lisp_char_pos().as_i64(),
                ),
                None => return Ok(()),
            };
            if !(pt > begv && pt < zv) {
                break;
            }

            // GNU `adjust_point_for_property` display-intangible branch: never
            // leave point inside text that a `display` property replaces. Moving
            // forward relocates to the run end; moving backward to its start (an
            // empty replacing string relocates one char before the start). A
            // relocation re-enters the loop so the invisible branch re-checks
            // the new position, mirroring GNU's `check_display`/`check_invisible`
            // cycling.
            let disp = self.apfp_char_property(pt, display_sym)?;
            if !disp.is_nil() && super::xdisp::display_prop_replacing_p(disp, frame_window_p) {
                // Maximal run [dbeg, dend) around PT whose `display` value is
                // `eq` to the one at PT (GNU `get_property_and_range`). Stepped
                // boundary-by-boundary (checking each edge before advancing, as
                // the invisible branch does) so a change-scan is never issued
                // from a run edge, where it would jump past the adjacent run.
                let mut dend = pt;
                while dend < zv
                    && crate::emacs_core::value::eq_value(
                        &self.apfp_char_property(dend, display_sym)?,
                        &disp,
                    )
                {
                    dend = self.apfp_next_change(dend, display_sym, zv)?;
                }
                let mut dbeg = pt;
                while dbeg > begv
                    && crate::emacs_core::value::eq_value(
                        &self.apfp_char_property(dbeg - 1, display_sym)?,
                        &disp,
                    )
                {
                    dbeg = self.apfp_prev_change(dbeg, display_sym, begv)?;
                }
                let empty_string = disp
                    .as_lisp_string()
                    .map(|s| s.as_bytes().is_empty())
                    .unwrap_or(false);
                if dbeg < pt || (dbeg <= pt && empty_string) {
                    let target = if pt < last_pt {
                        if empty_string {
                            (dbeg - 1).max(begv)
                        } else {
                            dbeg
                        }
                    } else {
                        dend
                    };
                    self.apfp_set_point(id, target);
                    continue;
                }
            }

            let pt_before_invis = pt;
            let mut ellipsis = false;
            let mut beg = pt;
            let mut end = pt;

            // Find boundaries `beg`..`end` of the invisible run around PT.
            while end < zv {
                let prop = self.apfp_char_property(end, inv)?;
                let invisibility = super::xdisp::text_prop_means_invisible(prop, spec);
                if !invisibility.hides_source() {
                    break;
                }
                ellipsis = ellipsis || invisibility.shows_ellipsis();
                end = self.apfp_next_change(end, inv, zv)?;
            }
            while beg > begv {
                let prop = self.apfp_char_property(beg - 1, inv)?;
                let invisibility = super::xdisp::text_prop_means_invisible(prop, spec);
                if !invisibility.hides_source() {
                    break;
                }
                ellipsis = ellipsis || invisibility.shows_ellipsis();
                beg = self.apfp_prev_change(beg, inv, begv)?;
            }

            let mut moved = false;

            // Move away from the inside of the region.
            if beg < pt && end > pt {
                let target = if orig_pt == pt && (last_pt < beg || last_pt > end) {
                    orig_pt = -1;
                    if pt < last_pt { end } else { beg }
                } else if pt < last_pt {
                    beg
                } else {
                    end
                };
                self.apfp_set_point(id, target);
                moved = true;
            }

            // GNU keyboard.c: skip the boundary nudge when the invisible run's
            // start carries a replacing `display` property — the display engine
            // then positions the cursor, so point need not move (`shown`).
            let shown = {
                let dprop = self.apfp_char_property(beg, display_sym)?;
                !dprop.is_nil() && super::xdisp::display_prop_replacing_p(dprop, frame_window_p)
            };

            if !modified && !shown && !ellipsis && beg < end {
                let pt2 = self.apfp_point(id);
                if last_pt == beg && pt2 == end && end < zv {
                    self.apfp_set_point(id, end + 1);
                    moved = true;
                } else if last_pt == end && pt2 == beg && beg > begv {
                    self.apfp_set_point(id, beg - 1);
                    moved = true;
                } else if pt2 == (if pt2 < last_pt { beg } else { end }) {
                    // Already as far as we can go; avoid an infinite loop.
                } else {
                    let here = self.apfp_pos_property(pt2, inv)?;
                    if super::xdisp::text_prop_means_invisible(here, spec).hides_source() {
                        let other = if pt2 == beg { end } else { beg };
                        let other_val = self.apfp_pos_property(other, inv)?;
                        if !super::xdisp::text_prop_means_invisible(other_val, spec).hides_source()
                        {
                            self.apfp_set_point(id, other);
                            moved = true;
                        }
                    }
                }
            }

            let _ = pt_before_invis;
            if !moved {
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn note_interactive_minibuffer_read(&mut self) {
        self.interactive_minibuffer_read_count =
            self.interactive_minibuffer_read_count.saturating_add(1);
    }

    fn sync_current_buffer_to_selected_window(&mut self) {
        let Some(frame_id) = self.frames.selected_frame().map(|frame| frame.id) else {
            return;
        };
        super::window_cmds::remember_selected_window_point_in_state(
            &mut self.frames,
            &mut self.buffers,
            frame_id,
        );
        super::window_cmds::sync_selected_window_buffer_in_state(
            &self.frames,
            &mut self.buffers,
            frame_id,
        );
        let _ = self.sync_current_buffer_runtime_state();
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
