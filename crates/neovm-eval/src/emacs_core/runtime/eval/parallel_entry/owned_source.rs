//! Actual boxed source ownership while selected host slots are withdrawn.
//! In-place bindings are parked before host activations leave. Text-cache
//! retirement is the later `retire_text` step. No heap transfer, observation
//! export, source restoration, or Lisp entry is implemented here. Every armed
//! payload retains on Drop, forget, and unwind. Only a pre-effect refusal
//! returns an ordinary Context.

use super::super::Context;
use super::super::lifecycle::{
    detach_font_registry_for_source_shell, detach_terminal_registry_for_source_shell,
};
use super::super::world::Baton;
use super::source_entry::{SourceEntryRefusal, validate_serial_source_entry};
use crate::emacs_core::builtins::arithmetic::DetachedIntegerWidthActivation;
use crate::emacs_core::builtins::collections::HashTableTestRegistry;
use crate::emacs_core::builtins::file_notify::FileNotifyState;
use crate::emacs_core::builtins::hooks::WindowConfigurationSnapshot;
use crate::emacs_core::builtins::{
    collections::detach_hash_table_test_registry, file_notify::detach_file_notify_registry,
    hooks::detach_window_configuration_registry,
};
use crate::emacs_core::eval::data::dynamic_module_registry::{
    GlobalRefEntry, detach_dynamic_module_registry,
};
use crate::emacs_core::eval::data::face_registry::FaceAttrState;
use crate::emacs_core::eval::data::terminal_state::TerminalLispRegistry;
use neovm_text::emacs_core::ccl::{CclRegistry, detach_ccl_registry};
use neovm_text::emacs_core::charset::{CharsetRegistry, detach_charset_registry};
use neovm_value::error::{SourceShellInFlightReceipt, detach_source_shell_in_flight};
use neovm_value::heap_registry::detachment::DetachedRegistry;
use neovm_value::quit::quit_detach::{DetachedQuitActivation, detach_quit_activation};
use std::collections::HashMap;
use std::mem::ManuallyDrop;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceWithdrawalStep {
    Keeper,
    OwnerBindings,
    Ccl,
    Charset,
    Font,
    Terminal,
    HashTest,
    FileNotify,
    WindowConfiguration,
    DynamicModule,
    InFlight,
    IntegerWidth,
    Quit,
    TextEntry,
    CaseTable,
    Syntax,
    Category,
    Regex,
    StringPosition,
}

impl SourceWithdrawalStep {
    #[cfg(test)]
    fn trace_label(self) -> &'static str {
        match self {
            Self::Keeper => "keeper",
            Self::OwnerBindings => "owner-bindings",
            Self::Ccl => "ccl",
            Self::Charset => "charset",
            Self::Font => "font",
            Self::Terminal => "terminal",
            Self::HashTest => "hash-test",
            Self::FileNotify => "file-notify",
            Self::WindowConfiguration => "window-configuration",
            Self::DynamicModule => "dynamic-module",
            Self::InFlight => "in-flight",
            Self::IntegerWidth => "integer-width",
            Self::Quit => "quit",
            Self::TextEntry => "text-entry",
            Self::CaseTable => "case-table",
            Self::Syntax => "syntax",
            Self::Category => "category",
            Self::Regex => "regex",
            Self::StringPosition => "string-position",
        }
    }
}

/// This branch alone carries the still-ordinary source back to its caller.
/// No effect, drain, collector loan, or World exclusion has begun.
#[must_use]
pub(crate) struct SerialSourceRefusal {
    pub(crate) reason: SourceEntryRefusal,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "returned to the caller; only the pre-effect tests read it back"
        )
    )]
    pub(crate) source: Box<Context>,
}

#[must_use]
pub(crate) enum SourceWithdrawalFailure {
    BeforeEffects(SerialSourceRefusal),
    Retained(RetainedSourceFailure),
}

impl std::fmt::Debug for SourceWithdrawalFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeEffects(failure) => formatter
                .debug_tuple("BeforeEffects")
                .field(&failure.reason)
                .finish(),
            Self::Retained(failure) => formatter
                .debug_tuple("Retained")
                .field(&failure.step)
                .field(&failure._state)
                .finish(),
        }
    }
}

// No Any, downcast, Context accessor, or recovery method exists on this erased
// failure retention. Successful execution never erases its receipt types.
trait RetainedState: std::fmt::Debug {}

#[must_use]
pub(crate) struct RetainedSourceFailure {
    pub(crate) step: SourceWithdrawalStep,
    _state: Box<dyn RetainedState>,
}

struct FailedStep<R, E> {
    _source: SourceFrame<R>,
    _error: ManuallyDrop<E>,
}

impl<R, E: std::fmt::Debug> std::fmt::Debug for FailedStep<R, E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("FailedStep")
            .field(&*self._error)
            .finish()
    }
}

impl<R, E: std::fmt::Debug> RetainedState for FailedStep<R, E> {}

struct NoEffects;

/// A typed receipt chain. Every payload is retained independently. No partial
/// chain destructor can restore a slot or destroy the source allocation.
struct Effect<R, T> {
    _previous: R,
    _receipt: ManuallyDrop<T>,
}

struct SourceFrame<R> {
    source: ManuallyDrop<Box<Context>>,
    effects: R,
}

impl<R: 'static> SourceFrame<R> {
    /// A private static adapter, never an arbitrary Lisp or caller callback.
    /// Its result cannot borrow the Context. Each completed receipt becomes a
    /// new type in the chain before another operation can run.
    fn advance<T: 'static, E: std::fmt::Debug + 'static>(
        mut self,
        step: SourceWithdrawalStep,
        apply: for<'source> fn(&'source mut Context) -> Result<T, E>,
    ) -> Result<SourceFrame<Effect<R, T>>, SourceWithdrawalFailure> {
        #[cfg(test)]
        super::super::lifecycle::record_source_shell_trace(step.trace_label());
        match apply(&mut self.source) {
            Ok(receipt) => Ok(SourceFrame {
                source: self.source,
                effects: Effect {
                    _previous: self.effects,
                    _receipt: ManuallyDrop::new(receipt),
                },
            }),
            Err(error) => Err(SourceWithdrawalFailure::Retained(RetainedSourceFailure {
                step,
                _state: Box::new(FailedStep {
                    _source: self,
                    _error: ManuallyDrop::new(error),
                }),
            })),
        }
    }
}

type Keeper = Effect<NoEffects, Baton>;
type Parked = Effect<Keeper, super::super::owner_mirrors::ParkedOwner>;
type Ccl = Effect<Parked, DetachedRegistry<CclRegistry>>;
type Charset = Effect<Ccl, DetachedRegistry<CharsetRegistry>>;
type Font = Effect<Charset, DetachedRegistry<FaceAttrState>>;
type Terminal = Effect<Font, DetachedRegistry<TerminalLispRegistry>>;
type HashTest = Effect<Terminal, DetachedRegistry<HashTableTestRegistry>>;
type FileNotify = Effect<HashTest, DetachedRegistry<FileNotifyState>>;
type WindowConfiguration =
    Effect<FileNotify, DetachedRegistry<HashMap<i64, WindowConfigurationSnapshot>>>;
type DynamicModule = Effect<WindowConfiguration, DetachedRegistry<Vec<Option<GlobalRefEntry>>>>;
type InFlight = Effect<DynamicModule, SourceShellInFlightReceipt>;
type IntegerWidth = Effect<InFlight, DetachedIntegerWidthActivation>;
type Quit = Effect<IntegerWidth, DetachedQuitActivation>;

/// Owns the actual Context, World exclusion, the parked owner, and all eleven
/// selected receipts. In-place bindings are parked before their host
/// activations are withdrawn. The exact chain type prevents missing,
/// duplicate, or reordered receipt families from constructing this wrapper.
/// It grants no World or heap access. Text caches, heap and store TLS, and
/// JIT still need their own closed source transaction.
#[must_use]
pub(crate) struct OwnedSourceRegistries {
    _frame: SourceFrame<Quit>,
}

static_assertions::assert_not_impl_any!(OwnedSourceRegistries: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RetainedSourceFailure: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SourceWithdrawalFailure: Send, Sync, Clone, Copy);

impl OwnedSourceRegistries {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant source shell; no production caller enters it"
        )
    )]
    /// # Safety
    ///
    /// This is the actual sole serial source, with no live collector, mutator,
    /// or borrowed or raw heap or store view, and no enclosing with_store
    /// frame. Those frames must have returned before the first keeper drain
    /// here. Context ownership and the entry observations cannot prove that
    /// residual fact. After success or retained failure, ordinary source
    /// execution and GC must remain closed until the remaining withdrawal and
    /// return protocol completes. This intermediate object may never start
    /// another mutator. Only the future closed eval boundary may call this
    /// in production.
    pub(crate) unsafe fn capture(source: Box<Context>) -> Result<Self, SourceWithdrawalFailure> {
        if let Err(reason) = validate_serial_source_entry(&source) {
            return Err(SourceWithdrawalFailure::BeforeEffects(
                SerialSourceRefusal { reason, source },
            ));
        }
        // Armed before the first drain. Moving the Box preserves its original
        // allocation. No failure or unwind below invokes Context::drop.
        let frame = SourceFrame {
            source: ManuallyDrop::new(source),
            effects: NoEffects,
        };
        let frame = frame.advance(SourceWithdrawalStep::Keeper, Context::issue_green_baton)?;
        let frame = frame.advance(SourceWithdrawalStep::OwnerBindings, |source| {
            Ok::<_, std::convert::Infallible>(source.park_owner())
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Ccl, |source| {
            detach_ccl_registry(&source.ccl_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Charset, |source| {
            detach_charset_registry(&source.charset_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Font, |source| {
            detach_font_registry_for_source_shell(&source.font_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Terminal, |source| {
            detach_terminal_registry_for_source_shell(&source.terminal_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::HashTest, |source| {
            detach_hash_table_test_registry(&source.hash_table_test_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::FileNotify, |source| {
            detach_file_notify_registry(&source.file_notify_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::WindowConfiguration, |source| {
            detach_window_configuration_registry(&source.window_configuration_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::DynamicModule, |source| {
            detach_dynamic_module_registry(&source.dynamic_module_registry)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::InFlight, |source| {
            // SAFETY: capture's contract excluded every other use of this
            // thread's in-flight slot. The sealed wrapper keeps the private
            // receipt unnameable outside neovm-value.
            unsafe { detach_source_shell_in_flight(&source.in_flight_registry) }
        })?;
        let frame = frame.advance(SourceWithdrawalStep::IntegerWidth, |source| {
            source.integer_width_context.detach_activation()
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Quit, |source| {
            // SAFETY: capture's contract excluded every other writer of this
            // thread's quit slot. The receipt names no World.
            unsafe { detach_quit_activation(&source.quit_requested) }
        })?;
        Ok(Self { _frame: frame })
    }
}

// These records carry an observed effect only. Their distinct concrete types
// require every family in the final chain. They do not certify that later
// code has left a slot empty. No saved cache word is restored.
#[derive(Debug)]
struct TextEntryChecked;
#[derive(Debug)]
struct CaseRetired(neovm_value::heap_registry::text_retirement::TextCacheRetirement);
#[derive(Debug)]
struct SyntaxRetired(neovm_value::heap_registry::text_retirement::TextCacheRetirement);
#[derive(Debug)]
struct CategoryRetired(neovm_value::heap_registry::text_retirement::TextCacheRetirement);
#[derive(Debug)]
struct RegexRetired(neovm_value::heap_registry::text_retirement::TextCacheRetirement);
#[derive(Debug)]
struct StringPositionRetired(neovm_value::heap_registry::text_retirement::TextCacheRetirement);
type TextEntry = Effect<Quit, TextEntryChecked>;
type Case = Effect<TextEntry, CaseRetired>;
type Syntax = Effect<Case, SyntaxRetired>;
type Category = Effect<Syntax, CategoryRetired>;
type Regex = Effect<Category, RegexRetired>;
type StringPosition = Effect<Regex, StringPositionRetired>;

/// Actual source and prior receipts after all five disposable text families
/// have been retired. Canonical tables remain rooted in the boxed Context.
/// No heap loan, final census, TLS export, or parallel authority exists yet.
#[must_use]
pub(crate) struct OwnedSourceTextRetired {
    _frame: SourceFrame<StringPosition>,
}

static_assertions::assert_not_impl_any!(OwnedSourceTextRetired: Send, Sync, Clone, Copy);

impl OwnedSourceRegistries {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant text retirement; no production caller enters it"
        )
    )]
    pub(crate) fn retire_text(self) -> Result<OwnedSourceTextRetired, SourceWithdrawalFailure> {
        // No collector, callback, or user code runs between these steps.
        // Recheck the actual source: thread-local activation can change
        // without moving this Box.
        let frame = self
            ._frame
            .advance(SourceWithdrawalStep::TextEntry, |source| {
                validate_serial_source_entry(source).map(|()| TextEntryChecked)
            })?;
        let frame = frame.advance(SourceWithdrawalStep::CaseTable, |source| {
            neovm_text::emacs_core::casetab::retirement::retire_casetab_for_heap(
                source.tagged_heap.heap_identity(),
            )
            .map(CaseRetired)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Syntax, |source| {
            neovm_text::emacs_core::syntax::retirement::retire_syntax_for_heap(
                source.tagged_heap.heap_identity(),
            )
            .map(SyntaxRetired)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Category, |source| {
            neovm_text::emacs_core::category::retirement::retire_category_for_heap(
                source.tagged_heap.heap_identity(),
            )
            .map(CategoryRetired)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::Regex, |source| {
            neovm_text::emacs_core::regex::retirement::retire_regex_for_heap(
                source.tagged_heap.heap_identity(),
            )
            .map(RegexRetired)
        })?;
        let frame = frame.advance(SourceWithdrawalStep::StringPosition, |source| {
            Ok::<_, std::convert::Infallible>(StringPositionRetired(
                neovm_value::string_pos_cache::retire_string_pos_cache_for_heap(
                    source.tagged_heap.heap_identity(),
                ),
            ))
        })?;
        Ok(OwnedSourceTextRetired { _frame: frame })
    }
}

#[cfg(test)]
#[path = "tests/owned_source_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/owned_source_retirement_test.rs"]
mod retirement_tests;
