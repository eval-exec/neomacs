//! Actual boxed source ownership while selected host slots are withdrawn.
//! In-place bindings are parked before host activations leave. Text-cache
//! retirement and the closed census share that same shell: the census borrows
//! the keeper the shell already holds. Observation withdrawal uses that same
//! collector frame and returns only the local receipt. The closed heap split
//! lives in `bootstrap` and has no production caller. Observation export,
//! source restoration, and Lisp entry are not implemented here. Every armed payload
//! retains on Drop, forget, and unwind. Only a pre-effect refusal returns an
//! ordinary Context.

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
    HeapPreparation,
    ObservationEntry,
    Observations,
    WorldCarriers,
    JitCache,
    JitBackend,
    JitCompiler,
    HeapTransfer,
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
            Self::HeapPreparation => "heap-preparation",
            Self::ObservationEntry => "observation-entry",
            Self::Observations => "observations",
            Self::WorldCarriers => "world-carriers",
            Self::JitCache => "jit-cache",
            Self::JitBackend => "jit-backend",
            Self::JitCompiler => "jit-compiler",
            Self::HeapTransfer => "heap-transfer",
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
        let result = apply(&mut self.source);
        self.retain_step_result(step, result)
    }

    fn advance_after_effect<T: 'static, E: std::fmt::Debug + 'static>(
        mut self,
        step: SourceWithdrawalStep,
        apply: for<'source, 'effect> fn(&'source mut Context, &'effect R) -> Result<T, E>,
    ) -> Result<SourceFrame<Effect<R, T>>, SourceWithdrawalFailure> {
        #[cfg(test)]
        super::super::lifecycle::record_source_shell_trace(step.trace_label());
        let result = apply(&mut self.source, &self.effects);
        self.retain_step_result(step, result)
    }

    fn retain_step_result<T: 'static, E: std::fmt::Debug + 'static>(
        self,
        step: SourceWithdrawalStep,
        result: Result<T, E>,
    ) -> Result<SourceFrame<Effect<R, T>>, SourceWithdrawalFailure> {
        match result {
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

struct Keeper(ManuallyDrop<Baton>);

trait HasKeeper {
    fn keeper(&self) -> &Baton;
}

impl HasKeeper for Keeper {
    fn keeper(&self) -> &Baton {
        &self.0
    }
}

impl<R: HasKeeper, T> HasKeeper for Effect<R, T> {
    fn keeper(&self) -> &Baton {
        self._previous.keeper()
    }
}

impl<R: HasKeeper + 'static> SourceFrame<R> {
    fn advance_with_keeper<T: 'static, E: std::fmt::Debug + 'static>(
        mut self,
        step: SourceWithdrawalStep,
        apply: for<'source, 'keeper> fn(&'source mut Context, &'keeper Baton) -> Result<T, E>,
    ) -> Result<SourceFrame<Effect<R, T>>, SourceWithdrawalFailure> {
        #[cfg(test)]
        super::super::lifecycle::record_source_shell_trace(step.trace_label());
        let result = apply(&mut self.source, self.effects.keeper());
        self.retain_step_result(step, result)
    }
}
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
/// It grants no World or heap access. JIT still needs its own closed source
/// transaction. Observation withdrawal is the next method on this shell.
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
    /// This is the actual sole serial source, with no active Lisp, native, VM,
    /// or compile frame, no live collector or mutator, and no borrowed or raw
    /// heap or store view or enclosing with_store frame. Those frames must
    /// have returned before the first keeper drain
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
        // Later steps borrow this retained keeper. The wrap moves the receipt
        // and does not drop the baton or issue another one.
        let SourceFrame { source, effects } = frame;
        let frame = SourceFrame {
            source,
            effects: Keeper(effects._receipt),
        };
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

#[derive(Debug)]
enum SourceCensusRefusal {
    Entry(SourceEntryRefusal),
    Heap(super::super::world::OwnedParallelError),
}

type HeapDrained = Effect<StringPosition, neovm_value::tagged::gc::FrozenLegacyRoots>;

/// Source-local result of the one closed census. It owns the original root
/// words and grants no heap loan. Observation ownership, the remaining World
/// carriers, and JIT state still need their own withdrawal before any transport.
#[must_use]
pub(crate) struct OwnedSourceHeapDrained {
    _frame: SourceFrame<HeapDrained>,
}

static_assertions::assert_not_impl_any!(OwnedSourceHeapDrained: Send, Sync, Clone, Copy);

impl OwnedSourceRegistries {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant source census; no production caller enters it"
        )
    )]
    pub(crate) fn prepare_census(self) -> Result<OwnedSourceHeapDrained, SourceWithdrawalFailure> {
        // Retirement and census stay contiguous. A caller cannot hand an old
        // retirement record back after a cache-only root was repopulated.
        self.retire_text()?.close_heap_tls()
    }
}

impl OwnedSourceTextRetired {
    fn close_heap_tls(self) -> Result<OwnedSourceHeapDrained, SourceWithdrawalFailure> {
        let frame = self._frame.advance_with_keeper(
            SourceWithdrawalStep::HeapPreparation,
            |source, keeper| {
                validate_serial_source_entry(source).map_err(SourceCensusRefusal::Entry)?;
                // SAFETY: `source` is owned inside the fail-closed `SourceFrame`
                // (`ManuallyDrop<Box<Context>>`); no caller can re-enter it
                // without explicit full recovery.
                unsafe { source.close_owned_source_heap_tls(keeper) }
                    .map_err(SourceCensusRefusal::Heap)
            },
        )?;
        Ok(OwnedSourceHeapDrained { _frame: frame })
    }
}

#[derive(Debug)]
struct ObservationEntryChecked;
type ObservationEntry = Effect<StringPosition, ObservationEntryChecked>;
type ObservationClosed = Effect<
    ObservationEntry,
    (
        neovm_value::tagged::gc::FrozenLegacyRoots,
        neovm_value::tagged::collection_reads::SourceObservationsDetached,
    ),
>;

/// Owns the original source, the final census, and the observation TLS receipt.
/// The collector and the original observation `Rc` do not leave the finish frame.
/// World carriers and JIT withdrawal come later. No production caller enters it.
#[must_use]
pub(crate) struct OwnedSourceObservationClosed {
    _frame: SourceFrame<ObservationClosed>,
}

static_assertions::assert_not_impl_any!(OwnedSourceObservationClosed: Send, Sync, Clone, Copy);

impl OwnedSourceRegistries {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant observation withdrawal; no production caller enters it"
        )
    )]
    pub(crate) fn withdraw_observations(
        self,
    ) -> Result<OwnedSourceObservationClosed, SourceWithdrawalFailure> {
        // Retirement, the entry recheck, the census, and observation withdrawal
        // stay one transaction. No caller callback runs between them.
        let retired = self.retire_text()?;
        let frame = retired
            ._frame
            .advance(SourceWithdrawalStep::ObservationEntry, |source| {
                validate_serial_source_entry(source).map(|()| ObservationEntryChecked)
            })?;
        let frame =
            frame.advance_with_keeper(SourceWithdrawalStep::Observations, |source, keeper| {
                // SAFETY: `source` is owned inside the fail-closed `SourceFrame`
                // (`ManuallyDrop<Box<Context>>`); no caller can re-enter it
                // without explicit full recovery.
                unsafe { source.withdraw_owned_source_observations(keeper) }
            })?;
        Ok(OwnedSourceObservationClosed { _frame: frame })
    }
}

type WorldCarriers = Effect<ObservationClosed, super::super::green::SourceWorldCarriers>;

/// Local source plus the twelve native World payloads after their TLS withdrawal.
/// Observations stay on the collector receipt. No payload is Send. JIT withdrawal
/// still comes later. No production caller enters it.
#[must_use]
pub(crate) struct OwnedSourceCarriersClosed {
    _frame: SourceFrame<WorldCarriers>,
}

static_assertions::assert_not_impl_any!(OwnedSourceCarriersClosed: Send, Sync, Clone, Copy);

impl OwnedSourceObservationClosed {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant carrier withdrawal; no production caller enters it"
        )
    )]
    pub(crate) fn withdraw_carriers(
        self,
    ) -> Result<OwnedSourceCarriersClosed, SourceWithdrawalFailure> {
        let frame = self._frame.advance_after_effect(
            SourceWithdrawalStep::WorldCarriers,
            |_source, effects| {
                super::super::green::SourceWorldCarriers::withdraw(&effects._receipt.1)
            },
        )?;
        Ok(OwnedSourceCarriersClosed { _frame: frame })
    }
}

#[cfg(test)]
#[path = "tests/owned_source_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/owned_source_retirement_test.rs"]
mod retirement_tests;

#[cfg(test)]
#[path = "tests/owned_source_census_test.rs"]
mod census_tests;

#[cfg(test)]
#[path = "tests/owned_source_observations_test.rs"]
mod observation_tests;

#[cfg(test)]
#[path = "tests/owned_source_carriers_test.rs"]
mod carrier_tests;

type JitCache = Effect<WorldCarriers, super::source_jit_cache::SourceJitCache>;

/// Retains the original source-local legacy caches. Persistent backend and
/// pending-flow TLS still need their own withdrawal. This is not a complete
/// JIT or transport right.
#[must_use]
pub(crate) struct OwnedSourceCacheClosed {
    _frame: SourceFrame<JitCache>,
}

static_assertions::assert_not_impl_any!(OwnedSourceCacheClosed: Send, Sync, Clone, Copy);

impl OwnedSourceCarriersClosed {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant JIT cache withdrawal; no production caller enters it"
        )
    )]
    pub(crate) fn withdraw_jit_cache(
        self,
    ) -> Result<OwnedSourceCacheClosed, SourceWithdrawalFailure> {
        let frame = self
            ._frame
            .advance(SourceWithdrawalStep::JitCache, |source| {
                // SAFETY: the original unsafe capture required the actual sole
                // source with all active native, VM, and compile frames ended.
                // No source execution, callback, or collecting drain follows
                // its final census. The original Box, census, and prior
                // receipts remain owned on every exit.
                unsafe { super::source_jit_cache::withdraw(source) }
            })?;
        Ok(OwnedSourceCacheClosed { _frame: frame })
    }
}

#[cfg(test)]
#[path = "tests/owned_source_jit_cache_test.rs"]
mod jit_cache_tests;

type JitBackend = Effect<JitCache, super::source_jit_backend::SourceJitBackend>;

/// Retains the actual local persistent backend after the cache. Background
/// and frontend scope state still precede a complete JIT or TLS suspension.
#[must_use]
pub(crate) struct OwnedSourceBackendClosed {
    _frame: SourceFrame<JitBackend>,
}

static_assertions::assert_not_impl_any!(OwnedSourceBackendClosed: Send, Sync, Clone, Copy);

impl OwnedSourceCacheClosed {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant JIT backend withdrawal; no production caller enters it"
        )
    )]
    pub(crate) fn withdraw_jit_backend(
        self,
    ) -> Result<OwnedSourceBackendClosed, SourceWithdrawalFailure> {
        let frame = self._frame.advance_after_effect(
            SourceWithdrawalStep::JitBackend,
            |_source, effects| {
                // SAFETY: the original unsafe capture excludes all execution
                // and heap or store frames. The exact cache and the root and
                // source backing stay in this chain, and no execution or
                // collection has followed.
                unsafe { super::source_jit_backend::withdraw(&effects._receipt) }
            },
        )?;
        Ok(OwnedSourceBackendClosed { _frame: frame })
    }
}

#[cfg(test)]
#[path = "tests/owned_source_jit_backend_test.rs"]
mod jit_backend_tests;

type JitCompiler = Effect<JitBackend, super::source_jit_compiler::SourceJitCompiler>;

/// Actual boxed source after the local cache and backend are retained and
/// the compile scopes are closed. Root publication and strict GEN1 admission
/// are still required. This type cannot send the Context.
#[must_use]
pub(crate) struct OwnedSourceCompilerClosed {
    _frame: SourceFrame<JitCompiler>,
}

static_assertions::assert_not_impl_any!(OwnedSourceCompilerClosed: Send, Sync, Clone, Copy);

impl OwnedSourceBackendClosed {
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L4 dormant JIT compiler closure; no production caller enters it"
        )
    )]
    pub(crate) fn close_jit_compiler(
        self,
    ) -> Result<OwnedSourceCompilerClosed, SourceWithdrawalFailure> {
        let frame = self._frame.advance_after_effect(
            SourceWithdrawalStep::JitCompiler,
            |_source, effects| {
                // SAFETY: the original unsafe capture excluded every complete
                // execution and collector or store frame. No execution or
                // collecting drain has followed, and the original receipts
                // and backing stay owned by this chain.
                unsafe { super::source_jit_compiler::close(&effects._receipt) }
            },
        )?;
        Ok(OwnedSourceCompilerClosed { _frame: frame })
    }
}

mod bootstrap;
mod keeper_service;

/// The keeper era and service entry points shared by the L6.7 driver and E6-P.4.
pub(in crate::emacs_core::eval::parallel_entry) use bootstrap::{PreparedEraStart, ReturnedEraEnd};
pub(in crate::emacs_core::eval::parallel_entry) use keeper_service::{
    KeeperEra, KeeperStartNotice, keeper_serve,
};

/// The live-era fixture the bootstrap rows and the E5.2 rows share.
#[cfg(test)]
pub(in crate::emacs_core::eval::parallel_entry) use bootstrap::era_fixture;

#[cfg(test)]
#[path = "tests/owned_source_jit_compiler_test.rs"]
mod jit_compiler_tests;
