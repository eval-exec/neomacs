//! What a further Green owner is built from (design-r3.md §11).
//!
//! Captured once, from the first owner, when the Green runtime starts. It
//! holds copied values and canonical registry handles; nothing in it is a
//! borrow of the first owner's Context, which may be parked on another OS
//! thread while a further owner is built.

use super::super::construct::{ExecutionSeed, RegistryHandles};
use super::super::{Context, InlineSubr};
use super::super::{CoreEvalSymbols, CustomManager, SubrEntry, Value};
use crate::emacs_core::subr::leaf::LeafSpec;
use neovm_edit::buffer::BufferId;
use neovm_value::intern::SymId;
use neovm_value::tagged::value::TaggedValue;

/// The startup subr tables, which live in thread-locals (§11, class C).
/// Written only by `init_builtins` at startup; every further owner installs
/// this copy on its OS thread. The canonical subr objects are minted for
/// every registered subr before the copy, so no thread mints its own.
struct NativeTables {
    subrs: Vec<Option<SubrEntry>>,
    inline: Vec<InlineSubr>,
    leaves: Vec<Option<&'static LeafSpec>>,
    subr_objects: Vec<Option<TaggedValue>>,
}

/// Active Green snapshots: registration while any snapshot lives would reach
/// only the registering thread. Retirement must reopen startup for later Worlds.
#[cfg(debug_assertions)]
static NATIVE_TABLES_CAPTURED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(debug_assertions)]
impl Drop for NativeTables {
    fn drop(&mut self) {
        NATIVE_TABLES_CAPTURED.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A subr is being registered: no Green runtime may have captured the
/// tables already, or its further owners would never see it.
#[cfg(debug_assertions)]
pub(crate) fn debug_assert_native_tables_open(name: &str) {
    debug_assert!(
        NATIVE_TABLES_CAPTURED.load(std::sync::atomic::Ordering::Relaxed) == 0,
        "subr {name} registered after a Green runtime captured the subr tables"
    );
}

impl NativeTables {
    #[doc(hidden)]
    pub fn capture() -> Self {
        let subrs = super::super::GLOBAL_SUBR_TABLE.with(|table| table.borrow().clone());
        crate::emacs_core::eval::subr_abi::mint_static_subr_objects(
            subrs
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.is_some())
                // SAFETY: round-trip of an interned bootstrap id (peer seal N8).
                .map(|(index, _)| unsafe { SymId::from_raw(index as u32) }),
        );
        let native = Self {
            subrs,
            inline: super::super::INLINE_SUBR_TABLE.with(|table| table.borrow().clone()),
            leaves: crate::emacs_core::subr::leaf::snapshot_subr_leaves(),
            subr_objects: neovm_value::tagged::value::snapshot_static_subr_objects(),
        };
        // Count only a fully built snapshot. Every such snapshot has exactly
        // one Drop; simultaneously live Worlds keep registration closed.
        #[cfg(debug_assertions)]
        NATIVE_TABLES_CAPTURED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        native
    }

    #[doc(hidden)]
    pub fn install(&self) {
        super::super::GLOBAL_SUBR_TABLE.with(|table| *table.borrow_mut() = self.subrs.clone());
        super::super::INLINE_SUBR_TABLE.with(|table| *table.borrow_mut() = self.inline.clone());
        crate::emacs_core::subr::leaf::install_subr_leaves(self.leaves.clone());
        neovm_value::tagged::value::install_static_subr_objects(self.subr_objects.clone());
    }
}

/// The first owner's execution state a further owner starts from.
pub(crate) struct GreenBootstrap {
    core_eval_symbols: CoreEvalSymbols,
    standard_syntax_table: Value,
    syntax_code_objects: Value,
    standard_category_table: Value,
    /// The buffer a new Lisp thread starts in.
    pub(super) current_buffer: Option<BufferId>,
    noninteractive: bool,
    symbols_with_pos_enabled: bool,
    print_symbols_bare: bool,
    compiler_function_overrides_active: bool,
    quit_flag: Value,
    inhibit_quit: Value,
    throw_on_input: Value,
    registries: RegistryHandles,
    native: NativeTables,
}

impl GreenBootstrap {
    /// Capture from the first owner, which is running.
    #[doc(hidden)]
    pub fn capture(owner: &Context) -> Self {
        Self {
            core_eval_symbols: CoreEvalSymbols {
                internal_interpreter_environment_symbol: owner
                    .internal_interpreter_environment_symbol,
                load_read_stream_token: owner.load_read_stream_token,
                compiler_function_overrides_symbol: owner.compiler_function_overrides_symbol,
                quit_flag_symbol: owner.quit_flag_symbol,
                inhibit_quit_symbol: owner.inhibit_quit_symbol,
                throw_on_input_symbol: owner.throw_on_input_symbol,
                kill_emacs_symbol: owner.kill_emacs_symbol,
                noninteractive_symbol: owner.noninteractive_symbol,
                symbols_with_pos_enabled_symbol: owner.symbols_with_pos_enabled_symbol,
                print_symbols_bare_symbol: owner.print_symbols_bare_symbol,
                runtime_projection_mask: owner.runtime_projection_mask.clone(),
            },
            standard_syntax_table: owner.standard_syntax_table,
            syntax_code_objects: owner.syntax_code_objects,
            standard_category_table: owner.standard_category_table,
            current_buffer: owner.edit.buffers.current_buffer_id(),
            noninteractive: owner.noninteractive,
            symbols_with_pos_enabled: owner.symbols_with_pos_enabled,
            print_symbols_bare: owner.print_symbols_bare,
            compiler_function_overrides_active: owner.compiler_function_overrides_active,
            quit_flag: owner.quit_flag,
            inhibit_quit: owner.inhibit_quit,
            throw_on_input: owner.throw_on_input,
            registries: RegistryHandles::of(owner),
            native: NativeTables::capture(),
        }
    }

    /// Install the startup subr tables on this OS thread.
    pub(crate) fn install_native_tables(&self) {
        self.native.install();
    }

    /// A further owner's execution state: these values and handles, with
    /// fresh owner-private managers.
    #[doc(hidden)]
    pub fn seed(&self) -> ExecutionSeed {
        ExecutionSeed {
            core_eval_symbols: CoreEvalSymbols {
                internal_interpreter_environment_symbol: self
                    .core_eval_symbols
                    .internal_interpreter_environment_symbol,
                load_read_stream_token: self.core_eval_symbols.load_read_stream_token,
                compiler_function_overrides_symbol: self
                    .core_eval_symbols
                    .compiler_function_overrides_symbol,
                quit_flag_symbol: self.core_eval_symbols.quit_flag_symbol,
                inhibit_quit_symbol: self.core_eval_symbols.inhibit_quit_symbol,
                throw_on_input_symbol: self.core_eval_symbols.throw_on_input_symbol,
                kill_emacs_symbol: self.core_eval_symbols.kill_emacs_symbol,
                noninteractive_symbol: self.core_eval_symbols.noninteractive_symbol,
                symbols_with_pos_enabled_symbol: self
                    .core_eval_symbols
                    .symbols_with_pos_enabled_symbol,
                print_symbols_bare_symbol: self.core_eval_symbols.print_symbols_bare_symbol,
                runtime_projection_mask: self.core_eval_symbols.runtime_projection_mask.clone(),
            },
            custom: CustomManager::new(),
            standard_syntax_table: self.standard_syntax_table,
            syntax_code_objects: self.syntax_code_objects,
            standard_category_table: self.standard_category_table,
            noninteractive: self.noninteractive,
            symbols_with_pos_enabled: self.symbols_with_pos_enabled,
            print_symbols_bare: self.print_symbols_bare,
            compiler_function_overrides_active: self.compiler_function_overrides_active,
            quit_flag: self.quit_flag,
            inhibit_quit: self.inhibit_quit,
            throw_on_input: self.throw_on_input,
            registries: self.registries.clone(),
        }
    }

    /// Visit every value the bootstrap holds (the subr objects are leaked
    /// static objects, outside the heap).
    #[doc(hidden)]
    pub fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        for value in [
            self.standard_syntax_table,
            self.syntax_code_objects,
            self.standard_category_table,
            self.quit_flag,
            self.inhibit_quit,
            self.throw_on_input,
        ] {
            visit(value);
        }
        visit(
            self.core_eval_symbols
                .load_read_stream_token
                .as_lisp_value(),
        );
    }
}
