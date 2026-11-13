//! Cold keyboard-macro entry table (stage-4 K2 batch K7).
//!
//! The keyboard-macro command bodies stay in the core command layer
//! (`commands/kmacro`): they drive the command loop's macro iteration through
//! `CommandLoopHost`, publish status through the core builtin dispatcher and
//! read the command loop's keyboard state. The evaluator's VM dispatch already
//! owns the control flow that enters them (the `start-kbd-macro`,
//! `end-kbd-macro`, `call-last-kbd-macro`, `execute-kbd-macro` and
//! `store-kbd-macro-event` byte-code operations), so it reaches the five
//! bodies through this one cold PC2 table, installed by core at
//! `Context::new_inner` and `Context::from_dump`.
//!
//! Frequency: per keyboard-macro command, never per bytecode step. The table
//! does not enumerate GC roots; every body roots its own arguments and macro
//! values exactly as before.

use super::*;
use neovm_value::error::EvalResult;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// One GNU keyboard-macro entry point.
pub type KmacroCommand = fn(&mut Context, Vec<Value>) -> EvalResult;

/// The five cold keyboard-macro bodies the VM dispatch calls.
#[derive(Clone, Copy)]
pub struct KmacroHooks {
    start: KmacroCommand,
    end: KmacroCommand,
    call_last: KmacroCommand,
    execute: KmacroCommand,
    store_event: KmacroCommand,
}

impl KmacroHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        start: KmacroCommand,
        end: KmacroCommand,
        call_last: KmacroCommand,
        execute: KmacroCommand,
        store_event: KmacroCommand,
    ) -> Self {
        Self {
            start,
            end,
            call_last,
            execute,
            store_event,
        }
    }

    /// `(start-kbd-macro &optional APPEND NO-EXEC)`.
    pub fn start(&self) -> KmacroCommand {
        self.start
    }

    /// `(end-kbd-macro &optional REPEAT LOOPFUNC)`.
    pub fn end(&self) -> KmacroCommand {
        self.end
    }

    /// `(call-last-kbd-macro &optional REPEAT LOOPFUNC)`.
    pub fn call_last(&self) -> KmacroCommand {
        self.call_last
    }

    /// `(execute-kbd-macro MACRO &optional COUNT LOOPFUNC)`.
    pub fn execute(&self) -> KmacroCommand {
        self.execute
    }

    /// `(store-kbd-macro-event EVENT)`.
    pub fn store_event(&self) -> KmacroCommand {
        self.store_event
    }
}

static KMACRO_HOOKS: OnceLock<KmacroHooks> = OnceLock::new();
static KMACRO_HOOKS_IDENTITY: OnceLock<&'static KmacroHooks> = OnceLock::new();

/// Publish the core command layer's keyboard-macro bodies.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a); core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting keyboard-macro publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_kmacro_hooks(
    hooks: &'static KmacroHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&KMACRO_HOOKS, &KMACRO_HOOKS_IDENTITY, hooks)
}

/// The installed table, if core has run one of the two pinned install seams.
pub fn kmacro_hooks() -> Option<&'static KmacroHooks> {
    KMACRO_HOOKS.get()
}

/// Typed refusal for the unreachable uninstalled path: every call site is a
/// VM byte-code operation that runs after construction/dump, so a missing
/// table means the construction seam itself is broken.
fn hooks_or_refusal() -> Result<&'static KmacroHooks, Flow> {
    kmacro_hooks().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("keyboard-macro hooks are not installed")],
        )
    })
}

/// Enter `start-kbd-macro` through the cold table.
pub fn start_kbd_macro(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (hooks_or_refusal()?.start())(ctx, args)
}

/// Enter `end-kbd-macro` through the cold table.
pub fn end_kbd_macro(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (hooks_or_refusal()?.end())(ctx, args)
}

/// Enter `call-last-kbd-macro` through the cold table.
pub fn call_last_kbd_macro(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (hooks_or_refusal()?.call_last())(ctx, args)
}

/// Enter `execute-kbd-macro` through the cold table.
pub fn execute_kbd_macro(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (hooks_or_refusal()?.execute())(ctx, args)
}

/// Enter `store-kbd-macro-event` through the cold table.
pub fn store_kbd_macro_event(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (hooks_or_refusal()?.store_event())(ctx, args)
}
