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

use super::data::face_registry::FontRegistryHandle;
use super::data::fontset_state::FontsetRegistrySnapshot;
use super::data::terminal_state::TerminalRegistryHandle;
use super::data::window_state::{Frame, FrameManager, MiniWindowResize, WindowTree};
use super::*;
#[doc(hidden)]
pub use crate::emacs_core::builtins::WindowConfigurationRestoreOptions;
use crate::emacs_core::reader::{MinibufferBufferUnwind, MinibufferSessionUnwind};
use neovm_edit::emacs_core::display_metrics::{WindowBodyCellSize, WindowId};
use neovm_edit::emacs_core::textprop::WindowBufferLookup;
use neovm_value::display_vocab::{FrameId, SplitDirection};
use neovm_value::error::{EvalResult, Flow};
use neovm_value::heap_types::LispString;
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
    register_platform_bootstrap_vars: fn(&mut Obarray),
    init_frame_lisp_faces: fn(&mut Frame),
    prepare_runtime_faces: fn(&mut Context, FrameId) -> bool,
    sync_runtime_face_table: fn(&mut Context, FrameId),
    ensure_lisp_face_id_property: fn(&mut Context, &str) -> Result<(), Flow>,
    clear_font_cache_state: fn(),
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
        register_platform_bootstrap_vars: fn(&mut Obarray),
        init_frame_lisp_faces: fn(&mut Frame),
        prepare_runtime_faces: fn(&mut Context, FrameId) -> bool,
        sync_runtime_face_table: fn(&mut Context, FrameId),
        ensure_lisp_face_id_property: fn(&mut Context, &str) -> Result<(), Flow>,
        clear_font_cache_state: fn(),
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
            register_platform_bootstrap_vars,
            init_frame_lisp_faces,
            prepare_runtime_faces,
            sync_runtime_face_table,
            ensure_lisp_face_id_property,
            clear_font_cache_state,
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
    frame_terminal: fn(&mut Context, Vec<Value>) -> EvalResult,
    print_handle: fn(&Value) -> Option<String>,
    configure_window_system_runtime: fn(neomacs_display_protocol::GraphicalDisplayIdentity),
    park_carrier_tls: fn(),
    carrier_tls_fingerprint: Option<fn(&mut u64)>,
    reset_thread_locals: fn(),
    reset_handle: fn(),
    world_state_placeholder: fn() -> Box<dyn Any>,
    swap_world_state: fn(&mut dyn Any),
    live_terminal_ids: fn() -> Vec<u64>,
    current_registry_handle: fn() -> TerminalRegistryHandle,
    install_registry_handle: fn(&TerminalRegistryHandle),
    install_owner_registry_handle: fn(&TerminalRegistryHandle),
}

impl TerminalHooks {
    /// Build the terminal-registry group.
    pub const fn new(
        handle_value: fn() -> Value,
        delete_noelisp: fn(&mut Context, u64) -> EvalResult,
        frame_terminal: fn(&mut Context, Vec<Value>) -> EvalResult,
        print_handle: fn(&Value) -> Option<String>,
        configure_window_system_runtime: fn(neomacs_display_protocol::GraphicalDisplayIdentity),
        park_carrier_tls: fn(),
        carrier_tls_fingerprint: Option<fn(&mut u64)>,
        reset_thread_locals: fn(),
        reset_handle: fn(),
        world_state_placeholder: fn() -> Box<dyn Any>,
        swap_world_state: fn(&mut dyn Any),
        live_terminal_ids: fn() -> Vec<u64>,
        current_registry_handle: fn() -> TerminalRegistryHandle,
        install_registry_handle: fn(&TerminalRegistryHandle),
        install_owner_registry_handle: fn(&TerminalRegistryHandle),
    ) -> Self {
        Self {
            handle_value,
            delete_noelisp,
            frame_terminal,
            print_handle,
            configure_window_system_runtime,
            park_carrier_tls,
            carrier_tls_fingerprint,
            reset_thread_locals,
            reset_handle,
            world_state_placeholder,
            swap_world_state,
            live_terminal_ids,
            current_registry_handle,
            install_registry_handle,
            install_owner_registry_handle,
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
    menu_bar_top_level_items: fn(&Context) -> Vec<(Value, String)>,
    key_binding: fn(&mut Context, Vec<Value>) -> EvalResult,
    where_is_internal: fn(&mut Context, Vec<Value>) -> EvalResult,
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
        menu_bar_top_level_items: fn(&Context) -> Vec<(Value, String)>,
        key_binding: fn(&mut Context, Vec<Value>) -> EvalResult,
        where_is_internal: fn(&mut Context, Vec<Value>) -> EvalResult,
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
            menu_bar_top_level_items,
            key_binding,
            where_is_internal,
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
    apply_mini_window_resize: fn(&mut Frame, MiniWindowResize),
    frame_resize_pixelwise: fn(&mut Frame, &BufferManager, u32, u32),
    frame_parameter_entry: fn(&mut Context, Value, Value) -> EvalResult,
}

impl WindowTreeHooks {
    /// Build the window-tree operation group.
    pub const fn new(
        char_property_window_lookup: fn(Option<&FrameManager>, WindowId) -> WindowBufferLookup,
        window_resize_check: fn(&WindowTree, WindowId, SplitDirection, WindowBodyCellSize) -> bool,
        window_resize_apply: fn(&mut WindowTree, WindowId, bool, f32, f32),
        apply_mini_window_resize: fn(&mut Frame, MiniWindowResize),
        frame_resize_pixelwise: fn(&mut Frame, &BufferManager, u32, u32),
        frame_parameter_entry: fn(&mut Context, Value, Value) -> EvalResult,
    ) -> Self {
        Self {
            char_property_window_lookup,
            window_resize_check,
            window_resize_apply,
            apply_mini_window_resize,
            frame_resize_pixelwise,
            frame_parameter_entry,
        }
    }
}

/// Fontset-registry state and snapshot operations (registry stays display-side).
#[derive(Clone, Copy)]
pub struct FontsetHooks {
    alias_alist_startup_value: fn() -> Value,
    list_value: fn() -> Value,
    reset: fn(),
    snapshot: fn() -> FontsetRegistrySnapshot,
    restore: fn(FontsetRegistrySnapshot),
}

impl FontsetHooks {
    /// Build the fontset-registry group.
    pub const fn new(
        alias_alist_startup_value: fn() -> Value,
        list_value: fn() -> Value,
        reset: fn(),
        snapshot: fn() -> FontsetRegistrySnapshot,
        restore: fn(FontsetRegistrySnapshot),
    ) -> Self {
        Self {
            alias_alist_startup_value,
            list_value,
            reset,
            snapshot,
            restore,
        }
    }

    #[doc(hidden)]
    pub fn alias_alist_startup_value(&self) -> fn() -> Value {
        self.alias_alist_startup_value
    }

    #[doc(hidden)]
    pub fn list_value(&self) -> fn() -> Value {
        self.list_value
    }

    #[doc(hidden)]
    pub fn reset(&self) -> fn() {
        self.reset
    }

    #[doc(hidden)]
    pub fn snapshot(&self) -> fn() -> FontsetRegistrySnapshot {
        self.snapshot
    }

    #[doc(hidden)]
    pub fn restore(&self) -> fn(FontsetRegistrySnapshot) {
        self.restore
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
    fontset: FontsetHooks,
    command: CommandSeamHooks,
    window_tree: WindowTreeHooks,
    xfaces: XfacesHooks,
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
        fontset: FontsetHooks,
        command: CommandSeamHooks,
        window_tree: WindowTreeHooks,
        xfaces: XfacesHooks,
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
            fontset,
            command,
            window_tree,
            xfaces,
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

    /// Fontset-registry state and snapshot operations.
    pub fn fontset(&self) -> FontsetHooks {
        self.fontset
    }

    /// Command-layer entry seams.
    pub fn command(&self) -> CommandSeamHooks {
        self.command
    }

    /// Window-tree operations.
    pub fn window_tree(&self) -> WindowTreeHooks {
        self.window_tree
    }

    /// Face-attribute registry carrier state.
    pub fn xfaces(&self) -> XfacesHooks {
        self.xfaces
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
    #[doc(hidden)]
    pub fn placeholder(&self) -> WorldStatePlaceholder {
        self.placeholder
    }

    #[doc(hidden)]
    pub fn swap(&self) -> WorldStateSwap {
        self.swap
    }
}

impl DynamicModuleHooks {
    #[doc(hidden)]
    pub fn reset(&self) -> fn() {
        self.reset
    }

    #[doc(hidden)]
    pub fn fingerprint(&self) -> Option<fn(&mut u64)> {
        self.fingerprint
    }

    #[doc(hidden)]
    pub fn panic_message(&self) -> fn(&(dyn Any + Send)) -> String {
        self.panic_message
    }

    #[doc(hidden)]
    pub fn load_module(&self) -> fn(&mut Context, PathBuf) -> EvalResult {
        self.load_module
    }

    #[doc(hidden)]
    pub fn apply_module_function(&self) -> ApplyModuleFunction {
        self.apply_module_function
    }

    #[doc(hidden)]
    pub fn module_call_active(&self) -> ModuleCallActive {
        self.module_call_active
    }

    #[doc(hidden)]
    pub fn collect_gc_roots(&self) -> CollectModuleGcRoots {
        self.collect_gc_roots
    }
}

impl DbusbindHooks {
    #[doc(hidden)]
    pub fn drain_events(&self) -> fn(&mut Context) -> Result<usize, Flow> {
        self.drain_events
    }

    #[doc(hidden)]
    pub fn has_active_watches(&self) -> fn() -> bool {
        self.has_active_watches
    }

    #[doc(hidden)]
    pub fn reset_thread_locals(&self) -> fn() {
        self.reset_thread_locals
    }
}

impl FileIoHooks {
    #[doc(hidden)]
    pub fn expand_file_name_entry(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.expand_file_name_entry
    }

    #[doc(hidden)]
    pub fn dispatch_file_handler(
        &self,
    ) -> fn(&mut Context, &str, &[Value]) -> Result<Option<Value>, Flow> {
        self.dispatch_file_handler
    }

    #[doc(hidden)]
    pub fn resolve_filename(&self) -> fn(&Context, &LispString) -> LispString {
        self.resolve_filename
    }

    #[doc(hidden)]
    pub fn lock_file(&self) -> fn(&mut Context, &LispString) -> Result<Value, Flow> {
        self.lock_file
    }

    #[doc(hidden)]
    pub fn unlock_file(&self) -> fn(&mut Context, &LispString) -> Result<Value, Flow> {
        self.unlock_file
    }
}

impl TerminalHooks {
    #[doc(hidden)]
    pub fn handle_value(&self) -> fn() -> Value {
        self.handle_value
    }

    #[doc(hidden)]
    pub fn delete_noelisp(&self) -> fn(&mut Context, u64) -> EvalResult {
        self.delete_noelisp
    }

    #[doc(hidden)]
    pub fn frame_terminal(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.frame_terminal
    }

    #[doc(hidden)]
    pub fn print_handle(&self) -> fn(&Value) -> Option<String> {
        self.print_handle
    }

    #[doc(hidden)]
    pub fn configure_window_system_runtime(
        &self,
    ) -> fn(neomacs_display_protocol::GraphicalDisplayIdentity) {
        self.configure_window_system_runtime
    }

    #[doc(hidden)]
    pub fn park_carrier_tls(&self) -> fn() {
        self.park_carrier_tls
    }

    #[doc(hidden)]
    pub fn carrier_tls_fingerprint(&self) -> Option<fn(&mut u64)> {
        self.carrier_tls_fingerprint
    }

    #[doc(hidden)]
    pub fn reset_thread_locals(&self) -> fn() {
        self.reset_thread_locals
    }

    #[doc(hidden)]
    pub fn reset_handle(&self) -> fn() {
        self.reset_handle
    }

    #[doc(hidden)]
    pub fn world_state_placeholder(&self) -> fn() -> Box<dyn Any> {
        self.world_state_placeholder
    }

    #[doc(hidden)]
    pub fn swap_world_state(&self) -> fn(&mut dyn Any) {
        self.swap_world_state
    }

    #[doc(hidden)]
    pub fn live_terminal_ids(&self) -> fn() -> Vec<u64> {
        self.live_terminal_ids
    }

    #[doc(hidden)]
    pub fn current_registry_handle(&self) -> fn() -> TerminalRegistryHandle {
        self.current_registry_handle
    }

    #[doc(hidden)]
    pub fn install_registry_handle(&self) -> fn(&TerminalRegistryHandle) {
        self.install_registry_handle
    }

    #[doc(hidden)]
    pub fn install_owner_registry_handle(&self) -> fn(&TerminalRegistryHandle) {
        self.install_owner_registry_handle
    }
}

/// Face-attribute registry carrier state (E6 registry handle).  The registry
/// payload and its replace-on-activation rule stay in the display layer; eval
/// only parks the handle and fingerprints it for the carrier check.
#[derive(Clone, Copy)]
pub struct XfacesHooks {
    park_carrier_tls: fn(),
    carrier_tls_fingerprint: Option<fn(&mut u64)>,
    current_registry_handle: fn() -> FontRegistryHandle,
    install_registry_handle: fn(&FontRegistryHandle),
}

impl XfacesHooks {
    /// Build the face-registry carrier group.
    pub const fn new(
        park_carrier_tls: fn(),
        carrier_tls_fingerprint: Option<fn(&mut u64)>,
        current_registry_handle: fn() -> FontRegistryHandle,
        install_registry_handle: fn(&FontRegistryHandle),
    ) -> Self {
        Self {
            park_carrier_tls,
            carrier_tls_fingerprint,
            current_registry_handle,
            install_registry_handle,
        }
    }

    #[doc(hidden)]
    pub fn park_carrier_tls(&self) -> fn() {
        self.park_carrier_tls
    }

    #[doc(hidden)]
    pub fn carrier_tls_fingerprint(&self) -> Option<fn(&mut u64)> {
        self.carrier_tls_fingerprint
    }

    #[doc(hidden)]
    pub fn current_registry_handle(&self) -> fn() -> FontRegistryHandle {
        self.current_registry_handle
    }

    #[doc(hidden)]
    pub fn install_registry_handle(&self) -> fn(&FontRegistryHandle) {
        self.install_registry_handle
    }
}

impl CommandSeamHooks {
    #[doc(hidden)]
    pub fn window_configuration_capture(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.window_configuration_capture
    }

    #[doc(hidden)]
    pub fn window_configuration_restore(
        &self,
    ) -> fn(&mut Context, Value, WindowConfigurationRestoreOptions) -> EvalResult {
        self.window_configuration_restore
    }

    #[doc(hidden)]
    pub fn run_redisplay_window_change_hooks(&self) -> fn(&mut Context) -> EvalResult {
        self.run_redisplay_window_change_hooks
    }

    #[doc(hidden)]
    pub fn unwind_minibuffer_session(
        &self,
    ) -> fn(&mut Context, MinibufferSessionUnwind) -> EvalResult {
        self.unwind_minibuffer_session
    }

    #[doc(hidden)]
    pub fn unwind_minibuffer_buffer(
        &self,
    ) -> fn(&mut Context, MinibufferBufferUnwind) -> EvalResult {
        self.unwind_minibuffer_buffer
    }

    #[doc(hidden)]
    pub fn get_char_property(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.get_char_property
    }

    #[doc(hidden)]
    pub fn try_completion(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.try_completion
    }

    #[doc(hidden)]
    pub fn all_completions(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.all_completions
    }

    #[doc(hidden)]
    pub fn test_completion(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.test_completion
    }

    #[doc(hidden)]
    pub fn menu_bar_top_level_items(&self) -> fn(&Context) -> Vec<(Value, String)> {
        self.menu_bar_top_level_items
    }

    #[doc(hidden)]
    pub fn key_binding(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.key_binding
    }

    #[doc(hidden)]
    pub fn where_is_internal(&self) -> fn(&mut Context, Vec<Value>) -> EvalResult {
        self.where_is_internal
    }
}

impl WindowTreeHooks {
    #[doc(hidden)]
    pub fn char_property_window_lookup(
        &self,
    ) -> fn(Option<&FrameManager>, WindowId) -> WindowBufferLookup {
        self.char_property_window_lookup
    }

    #[doc(hidden)]
    pub fn window_resize_check(
        &self,
    ) -> fn(&WindowTree, WindowId, SplitDirection, WindowBodyCellSize) -> bool {
        self.window_resize_check
    }

    #[doc(hidden)]
    pub fn window_resize_apply(&self) -> fn(&mut WindowTree, WindowId, bool, f32, f32) {
        self.window_resize_apply
    }

    #[doc(hidden)]
    pub fn apply_mini_window_resize(&self) -> fn(&mut Frame, MiniWindowResize) {
        self.apply_mini_window_resize
    }

    #[doc(hidden)]
    pub fn frame_resize_pixelwise(&self) -> fn(&mut Frame, &BufferManager, u32, u32) {
        self.frame_resize_pixelwise
    }

    #[doc(hidden)]
    pub fn frame_parameter_entry(&self) -> fn(&mut Context, Value, Value) -> EvalResult {
        self.frame_parameter_entry
    }
}

static LIFECYCLE_HOOKS: OnceLock<LifecycleHooks> = OnceLock::new();
static LIFECYCLE_HOOKS_IDENTITY: OnceLock<&'static LifecycleHooks> = OnceLock::new();

/// Publish the lifecycle descriptor.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a), first in the one-time hook
/// block so it precedes any GC-capable allocation; core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting lifecycle publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_lifecycle_hooks(
    hooks: &'static LifecycleHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&LIFECYCLE_HOOKS, &LIFECYCLE_HOOKS_IDENTITY, hooks)
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

/// Register the platform bootstrap variables installed by the core crate at
/// the same construction point the pre-split platform call used.
pub(crate) fn register_platform_bootstrap_vars(obarray: &mut Obarray) {
    (lifecycle_hooks().boot.register_platform_bootstrap_vars)(obarray)
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

impl super::keyboard::FatalSignalShutdown for ShutdownRequest {
    fn fatal_signal(signal: super::keyboard::FatalSignal) -> Self {
        Self {
            exit_code: signal as i32,
            restart: false,
        }
    }
}

/// GNU's last-terminal test is one live terminal. Zero also takes the fatal
/// path. The eval unit runtime's query panics, and that runtime has no
/// other terminal, so the absent slot is treated as last.
fn last_live_display_terminal() -> bool {
    let query = lifecycle_hooks().terminal().live_terminal_ids();
    #[cfg(test)]
    if crate::test_utils::terminal_live_ids_slot_is_absent(query) {
        return true;
    }
    query().len() <= 1
}

/// Delete a terminal without running Lisp hooks (input-reader disconnect).
///
/// When this is the last live terminal, publish `shutdown_if_last` first,
/// while the frame and terminal are still live. Dying then keeps the queued
/// delete walks from running. A non-last terminal only queues the delete.
/// The eval unit runtime's delete slot refuses the call, and that runtime
/// has no terminal record.
pub fn delete_terminal_noelisp_owned(
    ctx: &mut Context,
    terminal_id: u64,
    shutdown_if_last: ShutdownRequest,
) -> EvalResult {
    if last_live_display_terminal() {
        let _ = ctx.shutdown_with_hooks(shutdown_if_last);
    }
    let delete = lifecycle_hooks().terminal().delete_noelisp();
    #[cfg(test)]
    if crate::test_utils::terminal_delete_noelisp_slot_is_absent(delete) {
        return Ok(Value::NIL);
    }
    delete(ctx, terminal_id)
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

/// Collect the top-level menu-bar items (command-layer collector).
pub fn menu_bar_top_level_items(ctx: &Context) -> Vec<(Value, String)> {
    (lifecycle_hooks().command().menu_bar_top_level_items())(ctx)
}

/// `(key-binding KEYS)` through the command-layer entry.
pub fn key_binding(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().key_binding())(ctx, args)
}

/// `(where-is-internal ...)` through the command-layer entry.
pub fn where_is_internal(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().command().where_is_internal())(ctx, args)
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

/// Resize a frame from a native size observation under the core window
/// body-unit rules (PC2 service; per native resize event, never per Lisp
/// step).
pub fn resize_frame_pixelwise_with_buffer_constraints(
    frame: &mut Frame,
    buffers: &BufferManager,
    width: u32,
    height: u32,
) {
    (lifecycle_hooks().window_tree().frame_resize_pixelwise())(frame, buffers, width, height)
}

/// Read one entry of the `frame-parameter` Lisp entry for a frame designator.
///
/// The body (frame resolution including the synthetic batch-frame arm and the
/// realised scroll-bar/font parameter projection) is display behaviour; the
/// system environment fallback reaches it through this cold slot instead of
/// naming the display crate upward.  Called only when no frame value resolved.
pub fn frame_parameter_entry(eval: &mut Context, frame: Value, key: Value) -> EvalResult {
    (lifecycle_hooks().window_tree().frame_parameter_entry())(eval, frame, key)
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

/// The frame-init face hook used by the runtime FrameManager.
pub fn frame_init_hook() -> fn(&mut Frame) {
    lifecycle_hooks().boot.init_frame_lisp_faces
}

/// Realize runtime faces for one frame, if the lifecycle is enabled.
pub fn prepare_runtime_faces(ctx: &mut Context, frame_id: FrameId) -> bool {
    (lifecycle_hooks().boot.prepare_runtime_faces)(ctx, frame_id)
}

/// Synchronize the runtime face table from the frame's Lisp faces.
pub fn sync_runtime_face_table(ctx: &mut Context, frame_id: FrameId) {
    (lifecycle_hooks().boot.sync_runtime_face_table)(ctx, frame_id)
}

/// Ensure a face's numeric id is stamped on its symbol.
pub fn ensure_lisp_face_id_property(ctx: &mut Context, face_name: &str) -> Result<(), Flow> {
    (lifecycle_hooks().boot.ensure_lisp_face_id_property)(ctx, face_name)
}

/// Clear the display font cache at pdump reconstruct.
pub fn clear_font_cache_state() {
    (lifecycle_hooks().boot.clear_font_cache_state)()
}

/// GNU `Fframe_terminal (selected_frame)` for the KBOARD forwarding arm.
pub fn frame_terminal_value(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    (lifecycle_hooks().terminal().frame_terminal())(ctx, args)
}

/// Format a terminal handle for the printer.
pub fn print_terminal_handle(value: &Value) -> Option<String> {
    (lifecycle_hooks().terminal().print_handle())(value)
}

/// Configure the terminal runtime for a graphical display identity.
pub fn configure_window_system_runtime(
    identity: neomacs_display_protocol::GraphicalDisplayIdentity,
) {
    (lifecycle_hooks()
        .terminal()
        .configure_window_system_runtime())(identity)
}

/// The startup `fontset-alias-alist` seed value.
pub fn fontset_alias_alist_startup_value() -> Value {
    (lifecycle_hooks().fontset().alias_alist_startup_value())()
}

/// The current `fontset-list` value.
pub fn fontset_list_value() -> Value {
    (lifecycle_hooks().fontset().list_value())()
}

/// Reset the fontset registry at carrier park.
pub fn reset_fontset_registry() {
    (lifecycle_hooks().fontset().reset())()
}

/// Snapshot the display fontset registry for pdump.
pub fn fontset_registry_snapshot() -> FontsetRegistrySnapshot {
    (lifecycle_hooks().fontset().snapshot())()
}

/// Restore the display fontset registry from a pdump snapshot.
pub fn restore_fontset_registry(snapshot: FontsetRegistrySnapshot) {
    (lifecycle_hooks().fontset().restore())(snapshot)
}

/// Apply a planned minibuffer resize to one frame.
pub fn apply_mini_window_resize(frame: &mut Frame, resize: MiniWindowResize) {
    (lifecycle_hooks().window_tree().apply_mini_window_resize())(frame, resize)
}

/// Park the terminal registry handle on a green carrier park.
pub fn terminal_park_carrier_tls() {
    (lifecycle_hooks().terminal().park_carrier_tls())()
}

/// Debug-only fingerprint of the terminal registry carrier state.
#[cfg(debug_assertions)]
pub fn terminal_carrier_tls_fingerprint(h: &mut u64) {
    if let Some(fingerprint) = lifecycle_hooks().terminal().carrier_tls_fingerprint() {
        fingerprint(h);
    }
}

/// Reset the terminal thread-local state at pdump reconstruct.
pub fn reset_terminal_thread_locals() {
    (lifecycle_hooks().terminal().reset_thread_locals())()
}

/// Reset the terminal handle for a fresh Context.
pub fn reset_terminal_handle() {
    (lifecycle_hooks().terminal().reset_handle())()
}

/// Placeholder for the terminal manager's carried World state.
pub fn terminal_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().terminal().world_state_placeholder())()
}

/// Swap the terminal manager's carried World state with a parked slot.
pub fn terminal_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().terminal().swap_world_state())(slot)
}

/// Live terminal ids in the keyboard poll order.
pub fn live_terminal_ids_in_keyboard_poll_order() -> Vec<u64> {
    (lifecycle_hooks().terminal().live_terminal_ids())()
}

/// The collecting Context's terminal Lisp registry handle.
pub fn current_terminal_registry_handle() -> TerminalRegistryHandle {
    (lifecycle_hooks().terminal().current_registry_handle())()
}

/// Install HANDLE as the current terminal Lisp registry (id snapshot mode).
pub fn install_terminal_registry_handle(handle: &TerminalRegistryHandle) {
    (lifecycle_hooks().terminal().install_registry_handle())(handle)
}

/// Install HANDLE as the owner terminal Lisp registry (no id snapshot).
pub fn install_owner_terminal_registry_handle(handle: &TerminalRegistryHandle) {
    (lifecycle_hooks().terminal().install_owner_registry_handle())(handle)
}

/// Park the face-attribute registry handle on a green carrier park.
pub fn xfaces_park_carrier_tls() {
    (lifecycle_hooks().xfaces().park_carrier_tls())()
}

/// Debug-only fingerprint of the face-attribute registry carrier state.
#[cfg(debug_assertions)]
pub fn xfaces_carrier_tls_fingerprint(h: &mut u64) {
    if let Some(fingerprint) = lifecycle_hooks().xfaces().carrier_tls_fingerprint() {
        fingerprint(h);
    }
}

/// The collecting Context's face-attribute registry handle.
pub fn current_font_registry_handle() -> FontRegistryHandle {
    (lifecycle_hooks().xfaces().current_registry_handle())()
}

/// Install HANDLE as the current face-attribute registry.
pub fn install_font_registry_handle(handle: &FontRegistryHandle) {
    (lifecycle_hooks().xfaces().install_registry_handle())(handle)
}
