//! Actual boxed source ownership while selected host slots are withdrawn.
//! No heap transfer, observation export, owner parking, source restoration,
//! or Lisp entry is implemented here. Every armed payload retains on
//! Drop, forget, and unwind. Only a pre-effect refusal returns an ordinary
//! Context.

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
type Ccl = Effect<Keeper, DetachedRegistry<CclRegistry>>;
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

/// Owns the actual Context, World exclusion, and all eleven selected receipts.
/// The exact chain type prevents missing, duplicate, or reordered receipt
/// families from constructing this wrapper. It grants no World or heap access.
/// Text caches, source bindings, heap and store TLS, and JIT still need their
/// own closed source transaction.
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

#[cfg(test)]
#[path = "tests/owned_source_test.rs"]
mod tests;
