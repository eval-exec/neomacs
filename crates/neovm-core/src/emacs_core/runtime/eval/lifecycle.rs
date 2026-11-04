//! Lifecycle hook table (stage-4 W4/E11), extended with the cold core-service
//! upcalls that eval cannot own (stage-4 K2 oracleb batches).
//!
//! The modules whose lifecycle these hooks drive own their thread-local state
//! in the core/system layer: dynamic-module carrier bookkeeping and module
//! loading/dispatch, the sqlite/dbusbind/timefns carried World state, the
//! time-zone/SQLite/D-Bus resets at pdump reconstruct, and the bootstrap
//! variable registration of the core modules that stay above eval.  The same
//! descriptor carries the file-operation entry seams (the `expand-file-name`
//! and `load` Lisp entries plus the handler-aware file locks, the design's
//! SEAM row for `runtime/eval/edit_host.rs`) and the terminal-registry handle
//! operations (E6 registry handle).  The evaluator reaches all of them
//! through this one cold PC2 table, installed by core at `Context::new_inner`
//! and `Context::from_dump` before any GC-capable allocation.
//!
//! Frequency: bootstrap / pdump / carrier park / per module call / per input
//! event / per file operation / per terminal event — never in a Lisp loop or
//! a bytecode step.  The dynamic-module *registry's* root collection lives in
//! eval data (E6d) and is not reached through this table; the active
//! module-call frames, whose environments are core C state, are enumerated by
//! the `collect_gc_roots` slot.  The uninstalled read is a fail-closed
//! assertion: every call site runs only after construction/dump, so a missing
//! descriptor means the construction seam itself is broken.

use super::data::window_state::{FrameManager, WindowTree};
use super::*;
use crate::emacs_core::builtins::WindowConfigurationRestoreOptions;
use crate::emacs_core::reader::{MinibufferBufferUnwind, MinibufferSessionUnwind};
use crate::emacs_core::textprop::WindowBufferLookup;
use crate::heap_types::LispString;
use neovm_edit::emacs_core::display_metrics::{WindowBodyCellSize, WindowId};
use neovm_value::display_vocab::{FrameId, SplitDirection};
use neovm_value::error::{EvalResult, Flow};
use neovm_value::{HookInstallError, HookInstallStatus};
use std::any::Any;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Produce a placeholder for the World's carried thread-local state.
pub type WorldStatePlaceholder = fn() -> Box<dyn Any>;
/// Swap the World's carried thread-local state with a parked slot.
pub type WorldStateSwap = fn(&mut dyn Any);
/// Apply one loaded dynamic-module function.
pub type ApplyModuleFunction = fn(&mut Context, Value, Vec<Value>) -> EvalResult;
/// Whether a native module call is in progress on this thread.
pub type ModuleCallActive = fn() -> bool;
/// Enumerate the active module-call frames' Value roots.
pub type CollectModuleGcRoots = fn(&mut Vec<Value>, usize);

/// Dynamic-module lifecycle bodies (carrier hooks, panic text, module load,
/// module-call dispatch and the active-frame root enumeration).
#[derive(Clone, Copy)]
pub struct DynamicModuleHooks {
    reset: fn(),
    fingerprint: Option<fn(&mut u64)>,
    panic_message: fn(&(dyn Any + Send)) -> String,
    load_module: fn(&mut Context, PathBuf) -> EvalResult,
    apply_module_function: ApplyModuleFunction,
    module_call_active: ModuleCallActive,
    collect_gc_roots: CollectModuleGcRoots,
}

impl DynamicModuleHooks {
    /// Build the dynamic-module group.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one install site
    pub const fn new(
        reset: fn(),
        fingerprint: Option<fn(&mut u64)>,
        panic_message: fn(&(dyn Any + Send)) -> String,
        load_module: fn(&mut Context, PathBuf) -> EvalResult,
        apply_module_function: ApplyModuleFunction,
        module_call_active: ModuleCallActive,
        collect_gc_roots: CollectModuleGcRoots,
    ) -> Self {
        Self {
            reset,
            fingerprint,
            panic_message,
            load_module,
            apply_module_function,
            module_call_active,
            collect_gc_roots,
        }
    }
}

/// One module's carried World-state pair.
#[derive(Clone, Copy)]
pub struct WorldStateHooks {
    placeholder: WorldStatePlaceholder,
    swap: WorldStateSwap,
}

impl WorldStateHooks {
    /// Build one carried World-state pair.
    pub const fn new(placeholder: WorldStatePlaceholder, swap: WorldStateSwap) -> Self {
        Self { placeholder, swap }
    }
}

/// D-Bus carried World state plus the cold connection behaviour the input
/// reader needs (event drain, watch liveness, thread-local reset).
#[derive(Clone, Copy)]
pub struct DbusbindHooks {
    world: WorldStateHooks,
    drain_events: fn(&mut Context) -> Result<usize, Flow>,
    has_active_watches: fn() -> bool,
    reset_thread_locals: fn(),
}

impl DbusbindHooks {
    /// Build the D-Bus group.
    pub const fn new(
        world: WorldStateHooks,
        drain_events: fn(&mut Context) -> Result<usize, Flow>,
        has_active_watches: fn() -> bool,
        reset_thread_locals: fn(),
    ) -> Self {
        Self {
            world,
            drain_events,
            has_active_watches,
            reset_thread_locals,
        }
    }
}

/// The load startup-restore boot entries (W4 boot closure).
#[derive(Clone, Copy)]
pub struct BootHooks {
    ensure_startup_compat_variables: fn(&mut Context),
    clear_created_lisp_face: fn(&str),
    remove_face_new_frame_defaults_entry: fn(&Context, &str),
    restore_created_faces_from_table: fn(&[String]),
    gui_window_system_symbol: fn() -> &'static str,
    apply_post_image_init: fn(&mut Context),
    apply_lread_constants: fn(&mut Context),
    install_host_environment_snapshot: fn(&mut Context),
    shell_file_name_install: fn(&mut Context),
    shell_file_name_install_dump_default: fn(&mut Context),
    seed_batch_startup_frame: fn(&mut Context) -> FrameId,
    register_core_bootstrap_vars: fn(&mut Obarray),
}

impl BootHooks {
    /// Build the load startup-restore group.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one call site
    pub const fn new(
        ensure_startup_compat_variables: fn(&mut Context),
        clear_created_lisp_face: fn(&str),
        remove_face_new_frame_defaults_entry: fn(&Context, &str),
        restore_created_faces_from_table: fn(&[String]),
        gui_window_system_symbol: fn() -> &'static str,
        apply_post_image_init: fn(&mut Context),
        apply_lread_constants: fn(&mut Context),
        install_host_environment_snapshot: fn(&mut Context),
        shell_file_name_install: fn(&mut Context),
        shell_file_name_install_dump_default: fn(&mut Context),
        seed_batch_startup_frame: fn(&mut Context) -> FrameId,
        register_core_bootstrap_vars: fn(&mut Obarray),
    ) -> Self {
        Self {
            ensure_startup_compat_variables,
            clear_created_lisp_face,
            remove_face_new_frame_defaults_entry,
            restore_created_faces_from_table,
            gui_window_system_symbol,
            apply_post_image_init,
            apply_lread_constants,
            install_host_environment_snapshot,
            shell_file_name_install,
            shell_file_name_install_dump_default,
            seed_batch_startup_frame,
            register_core_bootstrap_vars,
        }
    }
}

/// Cold file-operation upcalls whose bodies stay in the core system layer
/// (the `expand-file-name`/`load` Lisp entries and the handler-aware file
/// lock operations).  This is the design's SEAM row for
/// `runtime/eval/edit_host.rs`, extended to the two Lisp-entry callers that
/// cannot move into the reader body.
#[derive(Clone, Copy)]
pub struct FileIoHooks {
    expand_file_name_entry: fn(&mut Context, Vec<Value>) -> EvalResult,
    dispatch_file_handler: fn(&mut Context, &str, &[Value]) -> Result<Option<Value>, Flow>,
    resolve_filename: fn(&Context, &LispString) -> LispString,
    lock_file: fn(&mut Context, &LispString) -> Result<Value, Flow>,
    unlock_file: fn(&mut Context, &LispString) -> Result<Value, Flow>,
}

impl FileIoHooks {
    /// Build the file-operation group.
    pub const fn new(
        expand_file_name_entry: fn(&mut Context, Vec<Value>) -> EvalResult,
        dispatch_file_handler: fn(&mut Context, &str, &[Value]) -> Result<Option<Value>, Flow>,
        resolve_filename: fn(&Context, &LispString) -> LispString,
        lock_file: fn(&mut Context, &LispString) -> Result<Value, Flow>,
        unlock_file: fn(&mut Context, &LispString) -> Result<Value, Flow>,
    ) -> Self {
        Self {
            expand_file_name_entry,
            dispatch_file_handler,
            resolve_filename,
            lock_file,
            unlock_file,
        }
    }
}

/// Terminal-registry operations the input reader publishes through the same
/// cold descriptor (E6 registry handle).  The Lisp registry state itself
/// stays in the display layer until its E5 payload moves.
#[derive(Clone, Copy)]
pub struct TerminalHooks {
    handle_value: fn() -> Value,
    delete_noelisp: fn(&mut Context, u64) -> EvalResult,
}

impl TerminalHooks {
    /// Build the terminal-registry group.
    pub const fn new(
        handle_value: fn() -> Value,
        delete_noelisp: fn(&mut Context, u64) -> EvalResult,
    ) -> Self {
        Self {
            handle_value,
            delete_noelisp,
        }
    }
}

/// Command-layer Lisp entries whose bodies stay in the core command modules
/// (design R0).  Eval's unwind, redisplay, minibuffer and completion paths
/// call them at a command boundary, never per bytecode step.
#[derive(Clone, Copy)]
pub struct CommandSeamHooks {
    window_configuration_capture: fn(&mut Context, Vec<Value>) -> EvalResult,
    window_configuration_restore:
        fn(&mut Context, Value, WindowConfigurationRestoreOptions) -> EvalResult,
    run_redisplay_window_change_hooks: fn(&mut Context) -> EvalResult,
    unwind_minibuffer_session: fn(&mut Context, MinibufferSessionUnwind) -> EvalResult,
    unwind_minibuffer_buffer: fn(&mut Context, MinibufferBufferUnwind) -> EvalResult,
    get_char_property: fn(&mut Context, Vec<Value>) -> EvalResult,
    try_completion: fn(&mut Context, Vec<Value>) -> EvalResult,
    all_completions: fn(&mut Context, Vec<Value>) -> EvalResult,
    test_completion: fn(&mut Context, Vec<Value>) -> EvalResult,
}

impl CommandSeamHooks {
    /// Build the command-entry group.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one install site
    pub const fn new(
        window_configuration_capture: fn(&mut Context, Vec<Value>) -> EvalResult,
        window_configuration_restore: fn(
            &mut Context,
            Value,
            WindowConfigurationRestoreOptions,
        ) -> EvalResult,
        run_redisplay_window_change_hooks: fn(&mut Context) -> EvalResult,
        unwind_minibuffer_session: fn(&mut Context, MinibufferSessionUnwind) -> EvalResult,
        unwind_minibuffer_buffer: fn(&mut Context, MinibufferBufferUnwind) -> EvalResult,
        get_char_property: fn(&mut Context, Vec<Value>) -> EvalResult,
        try_completion: fn(&mut Context, Vec<Value>) -> EvalResult,
        all_completions: fn(&mut Context, Vec<Value>) -> EvalResult,
        test_completion: fn(&mut Context, Vec<Value>) -> EvalResult,
    ) -> Self {
        Self {
            window_configuration_capture,
            window_configuration_restore,
            run_redisplay_window_change_hooks,
            unwind_minibuffer_session,
            unwind_minibuffer_buffer,
            get_char_property,
            try_completion,
            all_completions,
            test_completion,
        }
    }
}

/// Window-tree operations that stay in the core display layer (they drive the
/// core window body-unit rules) but operate on eval-owned window state.
#[derive(Clone, Copy)]
pub struct WindowTreeHooks {
    char_property_window_lookup: fn(Option<&FrameManager>, WindowId) -> WindowBufferLookup,
    window_resize_check: fn(&WindowTree, WindowId, SplitDirection, WindowBodyCellSize) -> bool,
    window_resize_apply: fn(&mut WindowTree, WindowId, bool, f32, f32),
}

impl WindowTreeHooks {
    /// Build the window-tree operation group.
    pub const fn new(
        char_property_window_lookup: fn(Option<&FrameManager>, WindowId) -> WindowBufferLookup,
        window_resize_check: fn(&WindowTree, WindowId, SplitDirection, WindowBodyCellSize) -> bool,
        window_resize_apply: fn(&mut WindowTree, WindowId, bool, f32, f32),
    ) -> Self {
        Self {
            char_property_window_lookup,
            window_resize_check,
            window_resize_apply,
        }
    }
}

/// The cold lifecycle descriptor core publishes at the construction seams.
///
/// Besides the per-module lifecycle groups this descriptor also carries the
/// cold core-service upcalls that eval cannot own: the file-operation entry
/// seams (design SEAM row for `edit_host`) and the terminal-registry handle
/// operations.  Publishing them from the one already-installed descriptor
/// keeps the eval->core construction surface identical (the install-seam
/// rows are an E13 item); every slot is cold and fail-closed on the same
/// missing-descriptor assertion.
#[derive(Clone, Copy)]
pub struct LifecycleHooks {
    dynamic_module: DynamicModuleHooks,
    sqlite: WorldStateHooks,
    dbusbind: DbusbindHooks,
    timefns: WorldStateHooks,
    boot: BootHooks,
    fileio: FileIoHooks,
    terminal: TerminalHooks,
    command: CommandSeamHooks,
    window_tree: WindowTreeHooks,
    timefns_reset: fn(),
    sqlite_reset: fn(),
}

impl LifecycleHooks {
    /// Build the one canonical descriptor core publishes.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one install site
    pub const fn new(
        dynamic_module: DynamicModuleHooks,
        sqlite: WorldStateHooks,
        dbusbind: DbusbindHooks,
        timefns: WorldStateHooks,
        boot: BootHooks,
        fileio: FileIoHooks,
        terminal: TerminalHooks,
        command: CommandSeamHooks,
        window_tree: WindowTreeHooks,
        timefns_reset: fn(),
        sqlite_reset: fn(),
    ) -> Self {
        Self {
            dynamic_module,
            sqlite,
            dbusbind,
            timefns,
            boot,
            fileio,
            terminal,
            command,
            window_tree,
            timefns_reset,
            sqlite_reset,
        }
    }

    /// Dynamic-module lifecycle group.
    pub fn dynamic_module(&self) -> DynamicModuleHooks {
        self.dynamic_module
    }

    /// SQLite carried World state.
    pub fn sqlite(&self) -> WorldStateHooks {
        self.sqlite
    }

    /// D-Bus carried World state and cold connection behaviour.
    pub fn dbusbind(&self) -> DbusbindHooks {
        self.dbusbind
    }

    /// Time functions carried World state.
    pub fn timefns(&self) -> WorldStateHooks {
        self.timefns
    }

    /// The load startup-restore boot group.
    pub fn boot(&self) -> BootHooks {
        self.boot
    }

    /// Cold file-operation upcalls.
    pub fn fileio(&self) -> FileIoHooks {
        self.fileio
    }

    /// Terminal-registry handle operations.
    pub fn terminal(&self) -> TerminalHooks {
        self.terminal
    }

    /// Command-layer entry seams.
    pub fn command(&self) -> CommandSeamHooks {
        self.command
    }

    /// Window-tree operations.
    pub fn window_tree(&self) -> WindowTreeHooks {
        self.window_tree
    }

    /// The pdump-reconstruct time-zone reset.
    pub fn timefns_reset(&self) -> fn() {
        self.timefns_reset
    }

    /// The pdump-reconstruct SQLite thread-local reset.
    pub fn sqlite_reset(&self) -> fn() {
        self.sqlite_reset
    }
}

impl WorldStateHooks {
    fn placeholder(&self) -> WorldStatePlaceholder {
        self.placeholder
    }

    fn swap(&self) -> WorldStateSwap {
        self.swap
    }
}

impl DynamicModuleHooks {
    fn reset(&self) -> fn() {
        self.reset
    }

    fn fingerprint(&self) -> Option<fn(&mut u64)> {
        self.fingerprint
    }

    fn panic_message(&self) -> fn(&(dyn Any + Send)) -> String {
        self.panic_message
    }

    fn load_module(&self) -> fn(&mut Context, PathBuf) -> EvalResult {
        self.load_module
    }

    fn apply_module_function(&self) -> ApplyModuleFunction {
        self.apply_module_function
    }

    fn module_call_active(&self) -> ModuleCallActive {
        self.module_call_active
    }

    fn collect_gc_roots(&self) -> CollectModuleGcRoots {
        self.collect_gc_roots
    }
}

impl DbusbindHooks {
    fn drain_events(&self) -> fn(&mut Context) -> Result<usize, Flow> {
        self.drain_events
    }

    fn has_active_watches(&self) -> fn() -> bool {
        self.has_active_watches
    }

    fn reset_thread_locals(&self) -> fn() {
        self.reset_thread_locals
    }
}

impl FileIoHooks {
    fn expand_file_name_entry(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.expand_file_name_entry
    }

    fn dispatch_file_handler(
        &self,
    ) -> fn(&mut Context, &str, &[Value]) -> Result<Option<Value>, Flow> {
        self.dispatch_file_handler
    }

    fn resolve_filename(&self) -> fn(&Context, &LispString) -> LispString {
        self.resolve_filename
    }

    fn lock_file(&self) -> fn(&mut Context, &LispString) -> Result<Value, Flow> {
        self.lock_file
    }

    fn unlock_file(&self) -> fn(&mut Context, &LispString) -> Result<Value, Flow> {
        self.unlock_file
    }
}

impl TerminalHooks {
    fn handle_value(&self) -> fn() -> Value {
        self.handle_value
    }

    fn delete_noelisp(&self) -> fn(&mut Context, u64) -> EvalResult {
        self.delete_noelisp
    }
}

impl CommandSeamHooks {
    fn window_configuration_capture(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.window_configuration_capture
    }

    fn window_configuration_restore(
        &self,
    ) -> fn(&mut Context, Value, WindowConfigurationRestoreOptions) -> EvalResult {
        self.window_configuration_restore
    }

    fn run_redisplay_window_change_hooks(&self) -> fn(&mut Context) -> EvalResult {
        self.run_redisplay_window_change_hooks
    }

    fn unwind_minibuffer_session(&self) -> fn(&mut Context, MinibufferSessionUnwind) -> EvalResult {
        self.unwind_minibuffer_session
    }

    fn unwind_minibuffer_buffer(&self) -> fn(&mut Context, MinibufferBufferUnwind) -> EvalResult {
        self.unwind_minibuffer_buffer
    }

    fn get_char_property(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.get_char_property
    }

    fn try_completion(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.try_completion
    }

    fn all_completions(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.all_completions
    }

    fn test_completion(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.test_completion
    }
}

impl WindowTreeHooks {
    fn char_property_window_lookup(
        &self,
    ) -> fn(Option<&FrameManager>, WindowId) -> WindowBufferLookup {
        self.char_property_window_lookup
    }

    fn window_resize_check(
        &self,
    ) -> fn(&WindowTree, WindowId, SplitDirection, WindowBodyCellSize) -> bool {
        self.window_resize_check
    }

    fn window_resize_apply(&self) -> fn(&mut WindowTree, WindowId, bool, f32, f32) {
        self.window_resize_apply
    }
}

static LIFECYCLE_HOOKS: OnceLock<LifecycleHooks> = OnceLock::new();
static LIFECYCLE_HOOKS_IDENTITY: OnceLock<&'static LifecycleHooks> = OnceLock::new();

/// Publish the lifecycle descriptor.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Core calls
/// this at `Context::new_inner` and `Context::from_dump`, first in the
/// one-time hook block so it precedes any GC-capable allocation.
#[must_use = "handle a conflicting lifecycle publication"]
#[cold]
#[inline(never)]
pub fn install_lifecycle_hooks(
    hooks: &'static LifecycleHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    let selected = LIFECYCLE_HOOKS_IDENTITY.get_or_init(|| hooks);
    if !std::ptr::eq(*selected, hooks) {
        return Err(HookInstallError::DifferentTable);
    }
    match LIFECYCLE_HOOKS.set(**selected) {
        Ok(()) => Ok(HookInstallStatus::Installed),
        Err(_same_descriptor) => Ok(HookInstallStatus::AlreadyInstalled),
    }
}

/// The installed descriptor.
///
/// Fail-closed: every call site (carrier park, JIT panic path, module load,
/// pdump reconstruct) runs after `new_inner`/`from_dump`, so a missing table
/// means the construction seam is broken and the state swap must not be
/// silently skipped.
fn lifecycle_hooks() -> &'static LifecycleHooks {
    LIFECYCLE_HOOKS
        .get()
        .expect("lifecycle hooks install at Context construction")
}

/// Clear the dynamic-module carrier thread-local state on a carrier park.
pub fn reset_dynamic_module_carrier_tls() {
    (lifecycle_hooks().dynamic_module().reset())();
}

/// Debug-only fingerprint of the dynamic-module carrier state.
#[cfg(debug_assertions)]
pub fn dynamic_module_carrier_tls_fingerprint(h: &mut u64) {
    if let Some(fingerprint) = lifecycle_hooks().dynamic_module().fingerprint() {
        fingerprint(h);
    }
}

/// Text of a caught dynamic-module panic payload.
pub fn dynamic_module_panic_message(payload: &(dyn Any + Send)) -> String {
    (lifecycle_hooks().dynamic_module().panic_message())(payload)
}

/// Load a dynamic module from PATH through the core loader.
pub fn load_module(ctx: &mut Context, path: PathBuf) -> EvalResult {
    (lifecycle_hooks().dynamic_module().load_module())(ctx, path)
}

/// Placeholder for SQLite's carried World state.
pub fn sqlite_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().sqlite().placeholder())()
}

/// Swap SQLite's carried World state with a parked slot.
pub fn sqlite_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().sqlite().swap())(slot)
}

/// Placeholder for D-Bus's carried World state.
pub fn dbusbind_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().dbusbind().world.placeholder())()
}

/// Swap D-Bus's carried World state with a parked slot.
pub fn dbusbind_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().dbusbind().world.swap())(slot)
}

/// Placeholder for the time-zone rule's carried World state.
pub fn timefns_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().timefns().placeholder())()
}

/// Swap the time-zone rule's carried World state with a parked slot.
pub fn timefns_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().timefns().swap())(slot)
}

/// Reset the time-zone thread-local rule at pdump reconstruct.
pub fn reset_timefns_thread_locals() {
    (lifecycle_hooks().timefns_reset())()
}

/// Re-apply the startup compatibility variables (xfaces).
pub fn ensure_startup_compat_variables(ctx: &mut Context) {
    (lifecycle_hooks().boot.ensure_startup_compat_variables)(ctx)
}

/// Clear one created Lisp face from the xfaces created-set.
pub fn clear_created_lisp_face(name: &str) {
    (lifecycle_hooks().boot.clear_created_lisp_face)(name)
}

/// Remove one face from the canonical new-frame-defaults store.
pub fn remove_face_new_frame_defaults_entry(ctx: &Context, face_name: &str) {
    (lifecycle_hooks().boot.remove_face_new_frame_defaults_entry)(ctx, face_name)
}

/// Restore the created-face set from a dumped face table.
pub fn restore_created_faces_from_table(face_names: &[String]) {
    (lifecycle_hooks().boot.restore_created_faces_from_table)(face_names)
}

/// The GUI window-system symbol used during startup restore.
pub fn gui_window_system_symbol() -> &'static str {
    (lifecycle_hooks().boot.gui_window_system_symbol)()
}

/// Apply the post-image-init table (startup reset rows).
pub fn apply_post_image_init(ctx: &mut Context) {
    (lifecycle_hooks().boot.apply_post_image_init)(ctx)
}

/// Re-apply the `Lread` constant rows after image load.
pub fn apply_lread_constants(ctx: &mut Context) {
    (lifecycle_hooks().boot.apply_lread_constants)(ctx)
}

/// Install the host environment snapshot into the evaluator.
pub fn install_host_environment_snapshot(ctx: &mut Context) {
    (lifecycle_hooks().boot.install_host_environment_snapshot)(ctx)
}

/// Install `shell-file-name` from the host.
pub fn shell_file_name_install(ctx: &mut Context) {
    (lifecycle_hooks().boot.shell_file_name_install)(ctx)
}

/// Install the dump-time `shell-file-name` default.
pub fn shell_file_name_install_dump_default(ctx: &mut Context) {
    (lifecycle_hooks().boot.shell_file_name_install_dump_default)(ctx)
}

/// Seed the initial startup frame before the display is initialized.
pub fn seed_batch_startup_frame(ctx: &mut Context) -> FrameId {
    (lifecycle_hooks().boot.seed_batch_startup_frame)(ctx)
}

/// Register the core bootstrap variables owned by modules that stay above
/// eval (fileio, process, dbusbind), at the same bootstrap seam.
pub fn register_core_bootstrap_vars(obarray: &mut Obarray) {
    (lifecycle_hooks().boot.register_core_bootstrap_vars)(obarray)
}

/// Apply one loaded dynamic-module function.
pub fn dynamic_module_apply_module_function(
    ctx: &mut Context,
    function: Value,
    args: Vec<Value>,
) -> EvalResult {
    (lifecycle_hooks().dynamic_module().apply_module_function())(ctx, function, args)
}

/// Whether a native module call is in progress on this thread.
pub fn dynamic_module_call_active() -> bool {
    (lifecycle_hooks().dynamic_module().module_call_active())()
}

/// Enumerate the active module-call frames' Value roots.
pub fn collect_dynamic_module_active_gc_roots(roots: &mut Vec<Value>, heap_identity: usize) {
    (lifecycle_hooks().dynamic_module().collect_gc_roots())(roots, heap_identity)
}

/// Drain pending D-Bus events into the evaluator.
pub fn dbusbind_drain_events(ctx: &mut Context) -> Result<usize, Flow> {
    (lifecycle_hooks().dbusbind().drain_events())(ctx)
}

/// Whether any D-Bus watch is active.
pub fn dbusbind_has_active_watches() -> bool {
    (lifecycle_hooks().dbusbind().has_active_watches())()
}

/// Reset D-Bus's thread-local connection state at pdump reconstruct.
pub fn reset_dbusbind_thread_locals() {
    (lifecycle_hooks().dbusbind().reset_thread_locals())()
}

/// Reset SQLite's thread-local state at pdump reconstruct.
pub fn reset_sqlite_thread_locals() {
    (lifecycle_hooks().sqlite_reset())()
}

/// The `expand-file-name` Lisp entry (handler-aware body stays in core).
pub fn expand_file_name_entry(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().fileio().expand_file_name_entry())(ctx, args)
}

/// The handler-aware file-operation dispatch used by `load`.
pub fn dispatch_file_handler(
    ctx: &mut Context,
    operation: &str,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    (lifecycle_hooks().fileio().dispatch_file_handler())(ctx, operation, args)
}

/// Resolve FILE against the visible default directory (EditHost seam).
pub fn resolve_filename(ctx: &Context, filename: &LispString) -> LispString {
    (lifecycle_hooks().fileio().resolve_filename())(ctx, filename)
}

/// Handler-aware `lock-file` (EditHost seam).
pub fn lock_file(ctx: &mut Context, filename: &LispString) -> Result<Value, Flow> {
    (lifecycle_hooks().fileio().lock_file())(ctx, filename)
}

/// Handler-aware `unlock-file` (EditHost seam).
pub fn unlock_file(ctx: &mut Context, filename: &LispString) -> Result<Value, Flow> {
    (lifecycle_hooks().fileio().unlock_file())(ctx, filename)
}

/// The current terminal's Lisp handle value.
pub fn terminal_handle_value() -> Value {
    (lifecycle_hooks().terminal().handle_value())()
}

/// Delete a terminal without running Lisp hooks (input-reader disconnect).
pub fn delete_terminal_noelisp_owned(ctx: &mut Context, terminal_id: u64) -> EvalResult {
    (lifecycle_hooks().terminal().delete_noelisp())(ctx, terminal_id)
}

/// Capture a window configuration through the core `current-window-configuration`
/// entry.
pub fn window_configuration_capture(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().window_configuration_capture())(ctx, args)
}

/// Restore a window configuration through the core `set-window-configuration`
/// entry.
pub fn window_configuration_restore(
    ctx: &mut Context,
    configuration: Value,
    options: WindowConfigurationRestoreOptions,
) -> EvalResult {
    (lifecycle_hooks().command().window_configuration_restore())(ctx, configuration, options)
}

/// Run GNU's window-change functions at a redisplay boundary.
pub fn run_redisplay_window_change_hooks(ctx: &mut Context) -> EvalResult {
    (lifecycle_hooks()
        .command()
        .run_redisplay_window_change_hooks())(ctx)
}

/// Tear down one minibuffer level (core `read_minibuf_unwind` equivalent).
pub fn unwind_minibuffer_session(ctx: &mut Context, state: MinibufferSessionUnwind) -> EvalResult {
    (lifecycle_hooks().command().unwind_minibuffer_session())(ctx, state)
}

/// Restore the pre-minibuffer window/buffer selection.
pub fn unwind_minibuffer_buffer(ctx: &mut Context, state: MinibufferBufferUnwind) -> EvalResult {
    (lifecycle_hooks().command().unwind_minibuffer_buffer())(ctx, state)
}

/// The `get-char-property` entry used by `adjust_point_for_property`.
pub fn get_char_property(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().get_char_property())(ctx, args)
}

/// The `try-completion` entry.
pub fn try_completion(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().try_completion())(ctx, args)
}

/// The `all-completions` entry.
pub fn all_completions(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().all_completions())(ctx, args)
}

/// The `test-completion` entry.
pub fn test_completion(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().test_completion())(ctx, args)
}

/// The edit layer's char-property WINDOW lookup for a caller that owns frames.
///
/// The slot resolves once; the returned closure keeps the original factory
/// shape so the text-property entry call sites are unchanged.
pub fn char_property_window_lookup(
    frames: Option<&FrameManager>,
) -> impl Fn(WindowId) -> WindowBufferLookup + '_ {
    let lookup = lifecycle_hooks()
        .window_tree()
        .char_property_window_lookup();
    move |window| lookup(frames, window)
}

/// Validate a staged pixel resize plan (core window body-unit rules).
pub fn window_resize_check(
    tree: &WindowTree,
    id: WindowId,
    axis: SplitDirection,
    cells: WindowBodyCellSize,
) -> bool {
    (lifecycle_hooks().window_tree().window_resize_check())(tree, id, axis, cells)
}

/// Apply a staged pixel resize plan.
pub fn window_resize_apply(
    tree: &mut WindowTree,
    id: WindowId,
    horflag: bool,
    char_width: f32,
    char_height: f32,
) {
    (lifecycle_hooks().window_tree().window_resize_apply())(
        tree,
        id,
        horflag,
        char_width,
        char_height,
    )
}
