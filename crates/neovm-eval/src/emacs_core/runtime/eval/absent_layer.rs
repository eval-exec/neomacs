//! The one eval absent-layer refusal building-block set (T5/H1a).
//!
//! `eval_absent_layer_slots!` expands inside a crate's cfg(test)
//! `unit_runtime` module to the per-slot refusal functions every upper-layer
//! slot needs. The macro is the only cross-crate surface: one VIS row of
//! kind macro at this definition, and the generated functions need none.

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[macro_export]
macro_rules! eval_absent_layer_slots {
    () => {
        #[doc(hidden)]
        pub(crate) mod absent_layer_slots {
            #![allow(dead_code, non_snake_case, unused_variables)]
            use $crate::emacs_core::eval::Context;
            use $crate::emacs_core::eval::data::face_registry::FontRegistryHandle;
            use $crate::emacs_core::eval::data::fontset_state::FontsetRegistrySnapshot;
            use $crate::emacs_core::eval::data::terminal_state::TerminalRegistryHandle;
            use $crate::emacs_core::eval::data::window_state::{
                Frame, FrameManager, MiniWindowResize, PresentationGeometry, PresentationId, WindowDomain, WindowRegions, WindowTree,
            };
            use $crate::emacs_core::eval::keyboard::{FrontendWebViewEvent, IdleTransitionPolicy, InputEvent};
            use $crate::emacs_core::eval::input_hooks::CommandInputWaitOutcome;
            use $crate::emacs_core::eval::call_interactively::CallInteractivelyPlan;
            use $crate::emacs_core::eval::process_hooks::{
                MainWaitPreparation, NativeWaitToken, SelectArm, SelectArmToken, SelectHandleToken,
            };
            use $crate::emacs_core::eval::data::process_state::{ChildOwnership, WaitNotifier};
            use $crate::emacs_core::eval::MainNativeHandoff;
            use $crate::emacs_core::eval::perf::PerfOp;
            use $crate::emacs_core::eval::lifecycle::WindowConfigurationRestoreOptions;
            use $crate::emacs_core::reader::{MinibufferBufferUnwind, MinibufferSessionUnwind};
            use $crate::emacs_core::eval::data::layout_input::BufferLayoutInputState;
            use $crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWake, ControlWakeOutcome};
            use neovm_edit::buffer::{BufferId, BufferManager};
            use neovm_edit::emacs_core::display_metrics::{
                InvisibleRunContext, LineWrap, MeasuredMotion, MotionEngine, MotionRequest, WindowBodyCellSize, WindowId,
            };
            use neovm_edit::emacs_core::textprop::WindowBufferLookup;
            use neovm_value::display_vocab::{FrameId, Invisibility, SplitDirection};
            use neovm_value::error::{EvalResult, Flow};
            use neovm_value::heap_types::{LispString, ProcessId};
            use neovm_value::value::Value;
            use std::sync::{Mutex, OnceLock};

            /// Honest absent value for `case_tables_sync`: an eval-only
            /// runtime has no case-table registry, so there is nothing to
            /// sync and the operation succeeds. A const path keeps the
            /// signed H2 two-statement slot shape (the guard's honest-value
            /// classifier rejects a constructor call).
            pub(crate) const CASE_TABLES_SYNC_OK: Result<(), Flow> = Ok(());

            /// The two hit classes (C2): a refused service call is a real
            /// incompatibility for this build; an unlinked slot is an expected
            /// absent-by-construction no-op.
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub(crate) enum AbsentLayerHit {
                Refused,
                Unlinked,
            }

            impl AbsentLayerHit {
                fn label(self) -> &'static str {
                    match self {
                        Self::Refused => "refused",
                        Self::Unlinked => "unlinked",
                    }
                }
            }

            /// The value T4 binds for one upper bootstrap variable.
            ///
            /// Typed production slots (DEFVAR_INT/DEFVAR_BOOL) refuse the
            /// absent-layer symbol marker, so the H3b snapshot carries the
            /// binder's own production default for them; only Lisp_Object
            /// variables keep the marker.
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub(crate) enum BootstrapDefault {
                Marker,
                Integer(i64),
            }

            impl BootstrapDefault {
                /// The snapshot-column spelling of this default.
                pub(crate) fn snapshot_label(self) -> String {
                    match self {
                        Self::Marker => "marker".to_owned(),
                        Self::Integer(value) => format!("int:{value}"),
                    }
                }
            }

            /// Out-of-band AbsentLayer hit record (H4): process TLS plus an
            /// optional append-only log named by NEOVM_ABSENT_LAYER_LOG. Every
            /// logged line and every TLS entry carries the hit class.
            static HIT_LOG: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

            fn record_hit(kind: AbsentLayerHit, marker: &'static str) {
                let enabled = std::env::var_os("NEOVM_ABSENT_LAYER_LOG");
                if let Some(path) = enabled {
                    let log = HIT_LOG.get_or_init(|| {
                        let file = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                            .ok();
                        Mutex::new(file)
                    });
                    if let Ok(mut guard) = log.lock() {
                        if let Some(file) = guard.as_mut() {
                            use std::io::Write;
                            let _ = writeln!(file, "{}: {marker}", kind.label());
                        }
                    }
                }
                HITS.with(|hits| hits.borrow_mut().push((kind, marker)));
            }

            thread_local! {
                static HITS: std::cell::RefCell<Vec<(AbsentLayerHit, &'static str)>> =
                    const { std::cell::RefCell::new(Vec::new()) };
            }

            /// Drain the in-process hit list for the P2 H4 runtime gate: the
            /// test can assert zero `Refused` independent of its outcome
            /// (catch_unwind / #[should_panic] cannot hide a recorded hit).
            pub(crate) fn take_absent_layer_hits_for_test(
            ) -> Vec<(AbsentLayerHit, &'static str)> {
                HITS.with(|hits| std::mem::take(&mut *hits.borrow_mut()))
            }

            /// One recorded no-op for a slot whose upper state cannot exist here.
            pub(crate) fn absent_layer_unlinked(marker: &'static str) {
                record_hit(AbsentLayerHit::Unlinked, marker);
            }

            /// The only AbsentLayer panic path: recorded before it aborts.
            pub(crate) fn absent_layer_panic(marker: &'static str) -> ! {
                record_hit(AbsentLayerHit::Refused, marker);
                eprintln!("{marker}");
                panic!("{marker}")
            }

            /// ASSERT-EMPTY: the upper state is proved empty, then empty is returned.
            pub(crate) fn absent_layer_assert_empty(marker: &'static str, empty: bool) {
                if !empty {
                    absent_layer_panic(marker);
                }
            }

            /// One lazily created per-thread empty face registry (T2).
            pub(crate) fn absent_face_registry_handle() -> FontRegistryHandle {
                thread_local! {
                    static HANDLE: std::cell::OnceCell<FontRegistryHandle> =
                        const { std::cell::OnceCell::new() };
                }
                HANDLE.with(|cell| {
                    cell.get_or_init(|| {
                        FontRegistryHandle::new(
                            $crate::emacs_core::eval::data::face_registry::FaceAttrState::default(),
                        )
                    })
                    .clone()
                })
            }

            /// One lazily created per-thread empty terminal registry (T2).
            pub(crate) fn absent_terminal_registry_handle() -> TerminalRegistryHandle {
                thread_local! {
                    static HANDLE: std::cell::OnceCell<TerminalRegistryHandle> =
                        const { std::cell::OnceCell::new() };
                }
                HANDLE.with(|cell| {
                    cell.get_or_init(|| {
                        TerminalRegistryHandle::new(
                            $crate::emacs_core::eval::data::terminal_state::TerminalLispRegistry::default(),
                        )
                    })
                    .clone()
                })
            }

            /// The fontset alias value is an uninterned ABSENT-LAYER marker
            /// symbol, never nil (T3).
            pub(crate) fn absent_fontset_alias_marker() -> Value {
                let name = Value::string(
                    "ABSENT-LAYER display::fontset.alias_alist_startup_value",
                );
                Value::from_sym_id(
                    neovm_value::intern::make_uninterned_symbol_with_name_value(name),
                )
            }

            /// Bind each upper bootstrap variable exactly as T4/H3b specifies:
            /// typed production slots get their binder's own default (an
            /// integer or boolean, which a forwarded typed slot accepts), and
            /// every Lisp_Object variable gets its uninterned ABSENT-LAYER
            /// marker so boundp/specialness match production while a use fails
            /// loudly.
            pub(crate) fn bind_absent_bootstrap_vars(
                obarray: &mut neovm_value::symbol::Obarray,
                vars: &[(&'static str, BootstrapDefault)],
            ) {
                for (name, default) in vars {
                    match default {
                        BootstrapDefault::Marker => {
                            let marker_name =
                                Value::string(format!("ABSENT-LAYER upper-variable::{name}"));
                            let marker = Value::from_sym_id(
                                neovm_value::intern::make_uninterned_symbol_with_name_value(
                                    marker_name,
                                ),
                            );
                            obarray.make_special(name);
                            obarray.set_symbol_value(name, marker);
                        }
                        BootstrapDefault::Integer(value) => {
                            obarray.define_int_variable(name, *value);
                        }
                    }
                }
            }

            pub(crate) fn dynamic_module_reset() {
                absent_layer_panic("ABSENT-LAYER system::dynamic_module.reset");
            }

            pub(crate) fn dynamic_module_fingerprint(acc: &mut u64) {
                absent_layer_panic("ABSENT-LAYER system::dynamic_module.fingerprint");
            }

            pub(crate) fn dynamic_module_panic_message(err: &(dyn std::any::Any + Send)) -> String {
                absent_layer_panic("ABSENT-LAYER system::dynamic_module.panic_message");
            }

            pub(crate) fn dynamic_module_load_module(ctx: &mut Context, path: std::path::PathBuf) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER system::dynamic_module.load_module");
            }

            pub(crate) fn dynamic_module_apply_module_function(ctx: &mut Context, function: Value, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER system::dynamic_module.apply_module_function");
            }

            pub(crate) fn dynamic_module_module_call_active() -> bool {
                absent_layer_panic("ABSENT-LAYER system::dynamic_module.module_call_active");
            }

            pub(crate) fn dynamic_module_collect_gc_roots(roots: &mut Vec<Value>, _heap_identity: usize) {
                let before = roots.len();
                // The system layer that owns active module-call frames is not
                // linked here, so it cannot contribute a root: prove the
                // enumerator stayed a no-op instead of silently returning.
                absent_layer_assert_empty("ABSENT-LAYER system::dynamic_module.collect_gc_roots", roots.len() == before);
            }

            pub(crate) fn sqlite_world_state_placeholder() -> Box<dyn std::any::Any> {
                absent_layer_panic("ABSENT-LAYER system::sqlite.world_state_placeholder");
            }

            pub(crate) fn sqlite_world_state_swap(slot: &mut dyn std::any::Any) {
                absent_layer_panic("ABSENT-LAYER system::sqlite.world_state_swap");
            }

            pub(crate) fn dbusbind_world_state_placeholder() -> Box<dyn std::any::Any> {
                absent_layer_panic("ABSENT-LAYER system::dbusbind.world_state_placeholder");
            }

            pub(crate) fn dbusbind_world_state_swap(slot: &mut dyn std::any::Any) {
                absent_layer_panic("ABSENT-LAYER system::dbusbind.world_state_swap");
            }

            pub(crate) fn timefns_world_state_placeholder() -> Box<dyn std::any::Any> {
                absent_layer_panic("ABSENT-LAYER system::timefns.world_state_placeholder");
            }

            pub(crate) fn timefns_world_state_swap(slot: &mut dyn std::any::Any) {
                absent_layer_panic("ABSENT-LAYER system::timefns.world_state_swap");
            }

            pub(crate) fn dbusbind_drain_events(ctx: &mut Context) -> Result<usize, Flow> {
                absent_layer_panic("ABSENT-LAYER system::dbusbind.drain_events");
            }

            pub(crate) fn dbusbind_has_active_watches() -> bool {
                absent_layer_panic("ABSENT-LAYER system::dbusbind.has_active_watches");
            }

            pub(crate) fn dbusbind_reset_thread_locals() {
                absent_layer_unlinked("ABSENT-LAYER system::dbusbind.reset_thread_locals");
            }

            pub(crate) fn boot_ensure_startup_compat_variables(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER core::boot.ensure_startup_compat_variables");
            }

            pub(crate) fn boot_clear_created_lisp_face(name: &str) {
                absent_layer_panic("ABSENT-LAYER display::boot.clear_created_lisp_face");
            }

            pub(crate) fn boot_remove_face_new_frame_defaults_entry(ctx: &Context, name: &str) {
                absent_layer_panic("ABSENT-LAYER display::boot.remove_face_new_frame_defaults_entry");
            }

            pub(crate) fn boot_restore_created_faces_from_table(rows: &[String]) {
                absent_layer_panic("ABSENT-LAYER display::boot.restore_created_faces_from_table");
            }

            pub(crate) fn boot_gui_window_system_symbol() -> &'static str {
                absent_layer_panic("ABSENT-LAYER display::boot.gui_window_system_symbol");
            }

            pub(crate) fn boot_apply_post_image_init(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER system::boot.apply_post_image_init");
            }

            pub(crate) fn boot_apply_lread_constants(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER core::boot.apply_lread_constants");
            }

            pub(crate) fn boot_install_host_environment_snapshot(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER system::boot.install_host_environment_snapshot");
            }

            pub(crate) fn boot_shell_file_name_install(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER core::boot.shell_file_name_install");
            }

            pub(crate) fn boot_shell_file_name_install_dump_default(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER core::boot.shell_file_name_install_dump_default");
            }

            pub(crate) fn boot_seed_batch_startup_frame(ctx: &mut Context) -> FrameId {
                absent_layer_panic("ABSENT-LAYER display::boot.seed_batch_startup_frame");
            }

            pub(crate) fn boot_init_frame_lisp_faces(frame: &mut Frame) {
                absent_layer_panic("ABSENT-LAYER display::boot.init_frame_lisp_faces");
            }

            pub(crate) fn boot_prepare_runtime_faces(ctx: &mut Context, frame: FrameId) -> bool {
                absent_layer_panic("ABSENT-LAYER display::boot.prepare_runtime_faces");
            }

            pub(crate) fn boot_sync_runtime_face_table(ctx: &mut Context, frame: FrameId) {
                absent_layer_panic("ABSENT-LAYER display::boot.sync_runtime_face_table");
            }

            pub(crate) fn boot_ensure_lisp_face_id_property(ctx: &mut Context, name: &str) -> Result<(), Flow> {
                absent_layer_panic("ABSENT-LAYER display::boot.ensure_lisp_face_id_property");
            }

            pub(crate) fn boot_clear_font_cache_state() {
                absent_layer_unlinked("ABSENT-LAYER display::boot.clear_font_cache_state");
            }

            pub(crate) fn fileio_expand_file_name_entry(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER system::fileio.expand_file_name_entry");
            }

            pub(crate) fn fileio_dispatch_file_handler(ctx: &mut Context, operation: &str, args: &[Value]) -> Result<Option<Value>, Flow> {
                absent_layer_panic("ABSENT-LAYER system::fileio.dispatch_file_handler");
            }

            pub(crate) fn fileio_resolve_filename(ctx: &Context, name: &LispString) -> LispString {
                absent_layer_panic("ABSENT-LAYER system::fileio.resolve_filename");
            }

            pub(crate) fn fileio_lock_file(ctx: &mut Context, file: &LispString) -> Result<Value, Flow> {
                absent_layer_panic("ABSENT-LAYER system::fileio.lock_file");
            }

            pub(crate) fn fileio_unlock_file(ctx: &mut Context, file: &LispString) -> Result<Value, Flow> {
                absent_layer_panic("ABSENT-LAYER system::fileio.unlock_file");
            }

            pub(crate) fn terminal_handle_value() -> Value {
                absent_layer_panic("ABSENT-LAYER display::terminal.handle_value");
            }

            pub(crate) fn terminal_delete_noelisp(ctx: &mut Context, id: u64) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER display::terminal.delete_noelisp");
            }

            pub(crate) fn terminal_frame_terminal(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER display::terminal.frame_terminal");
            }

            pub(crate) fn terminal_print_handle(_value: &Value) -> Option<String> {
                absent_layer_unlinked("ABSENT-LAYER display::terminal.print_handle");
                None
            }

            pub(crate) fn terminal_configure_window_system_runtime(identity: neomacs_display_protocol::GraphicalDisplayIdentity) {
                absent_layer_panic("ABSENT-LAYER display::terminal.configure_window_system_runtime");
            }

            pub(crate) fn terminal_park_carrier_tls() {
                absent_layer_panic("ABSENT-LAYER display::terminal.park_carrier_tls");
            }

            pub(crate) fn terminal_carrier_tls_fingerprint(acc: &mut u64) {
                absent_layer_panic("ABSENT-LAYER display::terminal.carrier_tls_fingerprint");
            }

            pub(crate) fn terminal_reset_thread_locals() {
                absent_layer_unlinked("ABSENT-LAYER display::terminal.reset_thread_locals");
            }

            pub(crate) fn terminal_reset_handle() {
                absent_layer_unlinked("ABSENT-LAYER display::terminal.reset_handle");
            }

            pub(crate) fn terminal_world_state_placeholder() -> Box<dyn std::any::Any> {
                absent_layer_panic("ABSENT-LAYER display::terminal.world_state_placeholder");
            }

            pub(crate) fn terminal_swap_world_state(slot: &mut dyn std::any::Any) {
                absent_layer_panic("ABSENT-LAYER display::terminal.swap_world_state");
            }

            pub(crate) fn terminal_live_terminal_ids() -> Vec<u64> {
                absent_layer_panic("ABSENT-LAYER display::terminal.live_terminal_ids");
            }

            pub(crate) fn terminal_current_registry_handle() -> TerminalRegistryHandle {
                record_hit(
                    AbsentLayerHit::Unlinked,
                    "ABSENT-LAYER display::terminal.current_registry_handle",
                );
                absent_terminal_registry_handle()
            }

            pub(crate) fn terminal_install_registry_handle(handle: &TerminalRegistryHandle) {
                absent_layer_unlinked("ABSENT-LAYER display::terminal.install_registry_handle");
            }

            pub(crate) fn terminal_install_owner_registry_handle(handle: &TerminalRegistryHandle) {
                absent_layer_panic("ABSENT-LAYER display::terminal.install_owner_registry_handle");
            }

            pub(crate) fn fontset_alias_alist_startup_value() -> Value {
                record_hit(
                    AbsentLayerHit::Unlinked,
                    "ABSENT-LAYER display::fontset.alias_alist_startup_value",
                );
                absent_fontset_alias_marker()
            }

            pub(crate) fn fontset_list_value() -> Value {
                absent_layer_panic("ABSENT-LAYER display::fontset.list_value");
            }

            pub(crate) fn fontset_reset() {
                absent_layer_unlinked("ABSENT-LAYER display::fontset.reset");
            }

            pub(crate) fn fontset_snapshot() -> FontsetRegistrySnapshot {
                absent_layer_panic("ABSENT-LAYER display::fontset.snapshot");
            }

            pub(crate) fn fontset_restore(snapshot: FontsetRegistrySnapshot) {
                absent_layer_panic("ABSENT-LAYER display::fontset.restore");
            }

            pub(crate) fn command_window_configuration_capture(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.window_configuration_capture");
            }

            pub(crate) fn command_window_configuration_restore(ctx: &mut Context, configuration: Value, options: WindowConfigurationRestoreOptions) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.window_configuration_restore");
            }

            pub(crate) fn command_run_redisplay_window_change_hooks(ctx: &mut Context) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.run_redisplay_window_change_hooks");
            }

            pub(crate) fn command_unwind_minibuffer_session(ctx: &mut Context, session: MinibufferSessionUnwind) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.unwind_minibuffer_session");
            }

            pub(crate) fn command_unwind_minibuffer_buffer(ctx: &mut Context, buffer: MinibufferBufferUnwind) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.unwind_minibuffer_buffer");
            }

            pub(crate) fn command_get_char_property(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.get_char_property");
            }

            pub(crate) fn command_try_completion(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.try_completion");
            }

            pub(crate) fn command_all_completions(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.all_completions");
            }

            pub(crate) fn command_test_completion(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.test_completion");
            }

            pub(crate) fn command_menu_bar_top_level_items(ctx: &Context) -> Vec<(Value, String)> {
                absent_layer_panic("ABSENT-LAYER core::command.menu_bar_top_level_items");
            }

            pub(crate) fn command_key_binding(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.key_binding");
            }

            pub(crate) fn command_where_is_internal(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::command.where_is_internal");
            }

            pub(crate) fn window_tree_char_property_window_lookup(frames: Option<&FrameManager>, window: WindowId) -> WindowBufferLookup {
                absent_layer_panic("ABSENT-LAYER display::window_tree.char_property_window_lookup");
            }

            pub(crate) fn window_tree_window_resize_check(tree: &WindowTree, window: WindowId, direction: SplitDirection, size: WindowBodyCellSize) -> bool {
                absent_layer_panic("ABSENT-LAYER display::window_tree.window_resize_check");
            }

            pub(crate) fn window_tree_window_resize_apply(tree: &mut WindowTree, window: WindowId, horizontal: bool, width: f32, height: f32) {
                absent_layer_panic("ABSENT-LAYER display::window_tree.window_resize_apply");
            }

            pub(crate) fn window_tree_apply_mini_window_resize(frame: &mut Frame, resize: MiniWindowResize) {
                absent_layer_panic("ABSENT-LAYER display::window_tree.apply_mini_window_resize");
            }

            pub(crate) fn window_tree_frame_resize_pixelwise(frame: &mut Frame, buffers: &BufferManager, width: u32, height: u32) {
                absent_layer_panic("ABSENT-LAYER display::window_tree.frame_resize_pixelwise");
            }

            pub(crate) fn window_tree_frame_parameter_entry(ctx: &mut Context, frame: Value, parameter: Value) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER display::window_tree.frame_parameter_entry");
            }

            pub(crate) fn xfaces_park_carrier_tls() {
                absent_layer_panic("ABSENT-LAYER display::xfaces.park_carrier_tls");
            }

            pub(crate) fn xfaces_carrier_tls_fingerprint(acc: &mut u64) {
                absent_layer_panic("ABSENT-LAYER display::xfaces.carrier_tls_fingerprint");
            }

            pub(crate) fn xfaces_current_registry_handle() -> FontRegistryHandle {
                record_hit(
                    AbsentLayerHit::Unlinked,
                    "ABSENT-LAYER display::xfaces.current_registry_handle",
                );
                absent_face_registry_handle()
            }

            pub(crate) fn xfaces_install_registry_handle(handle: &FontRegistryHandle) {
                absent_layer_unlinked("ABSENT-LAYER display::xfaces.install_registry_handle");
            }

            pub(crate) fn timefns_reset() {
                absent_layer_unlinked("ABSENT-LAYER system::timefns.reset");
            }

            pub(crate) fn sqlite_reset() {
                absent_layer_unlinked("ABSENT-LAYER system::sqlite.reset");
            }

            pub(crate) fn process_unlink(ctx: &mut Context, id: ProcessId) {
                absent_layer_panic("ABSENT-LAYER system::process.unlink");
            }

            pub(crate) fn process_delete(ctx: &mut Context, id: ProcessId) -> bool {
                absent_layer_panic("ABSENT-LAYER system::process.delete");
            }

            pub(crate) fn process_open_channel(ctx: &mut Context, process: Value) -> Result<std::ffi::c_int, Flow> {
                absent_layer_panic("ABSENT-LAYER system::process.open_channel");
            }

            pub(crate) fn process_print_handle(value: &Value) -> Option<String> {
                absent_layer_panic("ABSENT-LAYER system::process.print_handle");
            }

            pub(crate) fn process_wait_notifier(ctx: &Context) -> Option<WaitNotifier> {
                absent_layer_panic("ABSENT-LAYER system::process.wait_notifier");
            }

            pub(crate) fn process_terminate_and_defer_reaping(ownership: &mut ChildOwnership, signal: i32) {
                absent_layer_panic("ABSENT-LAYER system::process.terminate_and_defer_reaping");
            }

            pub(crate) fn select_handle(ctx: &mut Context) -> Option<SelectHandleToken> {
                absent_layer_panic("ABSENT-LAYER system::process.select.handle");
            }

            pub(crate) fn select_notify(token: SelectHandleToken) {
                absent_layer_panic("ABSENT-LAYER system::process.select.notify");
            }

            pub(crate) fn select_nudge(token: SelectHandleToken, destination: ControlDestination, nudge: ControlNudge) {
                absent_layer_panic("ABSENT-LAYER system::process.select.nudge");
            }

            pub(crate) fn select_control_source(token: SelectHandleToken) -> Option<std::sync::Arc<ControlWake>> {
                absent_layer_panic("ABSENT-LAYER system::process.select.control_source");
            }

            pub(crate) fn select_wait_until(token: SelectHandleToken, deadline: std::time::Instant) -> bool {
                absent_layer_panic("ABSENT-LAYER system::process.select.wait_until");
            }

            pub(crate) fn select_arm(token: SelectHandleToken) -> SelectArm {
                absent_layer_panic("ABSENT-LAYER system::process.select.arm");
            }

            pub(crate) fn select_wait_registered(token: SelectHandleToken, arm: SelectArmToken, deadline: std::time::Instant) -> bool {
                absent_layer_panic("ABSENT-LAYER system::process.select.wait_registered");
            }

            pub(crate) fn select_cancel_arm(arm: SelectArmToken) {
                absent_layer_panic("ABSENT-LAYER system::process.select.cancel_arm");
            }

            pub(crate) fn main_wait_prepare(ctx: &mut Context, deadline: std::time::Instant) -> MainWaitPreparation {
                absent_layer_panic("ABSENT-LAYER system::process.main_wait.prepare");
            }

            pub(crate) fn main_wait_wait(token: NativeWaitToken, handoff: &mut MainNativeHandoff<'_>) -> std::io::Result<ControlWakeOutcome> {
                absent_layer_panic("ABSENT-LAYER system::process.main_wait.wait");
            }

            pub(crate) fn main_wait_cancel(token: NativeWaitToken) {
                absent_layer_panic("ABSENT-LAYER system::process.main_wait.cancel");
            }

            pub(crate) fn input_interrupt_for_input_event_if_requested(ctx: &mut Context, event: InputEvent) -> Result<bool, Flow> {
                absent_layer_panic("ABSENT-LAYER system::input.interrupt_for_input_event_if_requested");
            }

            pub(crate) fn input_os_signal_pending() -> bool {
                absent_layer_panic("ABSENT-LAYER system::input.os_signal_pending");
            }

            pub(crate) fn input_take_termination_signal() -> Option<i32> {
                absent_layer_panic("ABSENT-LAYER system::input.take_termination_signal");
            }

            pub(crate) fn input_drain_pending_os_signals(ctx: &mut Context) -> Result<(), Flow> {
                absent_layer_panic("ABSENT-LAYER system::input.drain_pending_os_signals");
            }

            /// UNLINKED: plain unit-runtime construction reaches this through
            /// `ProcessWaitBackend::new`; a layer-less runtime has no signal
            /// self-pipe, so the honest absent value is `None`.
            pub(crate) fn input_signal_wake_pipe_read_fd() -> Option<i32> {
                absent_layer_unlinked("ABSENT-LAYER system::input.signal_wake_pipe_read_fd");
                None
            }

            pub(crate) fn input_drain_signal_wake_pipe(fd: i32) {
                absent_layer_panic("ABSENT-LAYER system::input.drain_signal_wake_pipe");
            }

            pub(crate) fn call_interactively_resolve(ctx: &mut Context, plan: &mut CallInteractivelyPlan) -> Result<(Value, Vec<Value>), Flow> {
                absent_layer_panic("ABSENT-LAYER core::call_interactively.resolve");
            }

            pub(crate) fn case_tables_canon(ctx: &mut Context) -> Result<Value, Flow> {
                absent_layer_panic("ABSENT-LAYER core::case_tables.canon");
            }

            pub(crate) fn case_tables_upcase(ctx: &mut Context) -> Result<Value, Flow> {
                absent_layer_panic("ABSENT-LAYER core::case_tables.upcase");
            }

            pub(crate) fn case_tables_sync(_ctx: &mut Context) -> Result<(), Flow> {
                absent_layer_unlinked("ABSENT-LAYER core::case_tables.sync");
                CASE_TABLES_SYNC_OK
            }

            pub(crate) fn case_tables_standard(ctx: &mut Context) -> Result<Value, Flow> {
                absent_layer_panic("ABSENT-LAYER core::case_tables.standard");
            }

            pub(crate) fn case_tables_override_for_current_buffer(ctx: &mut Context) -> Result<neovm_text::emacs_core::casetab::CaseTableOverride, Flow> {
                absent_layer_panic("ABSENT-LAYER core::case_tables.override_for_current_buffer");
            }

            pub(crate) fn buffer_kill_before(ctx: &mut Context, killed: neovm_edit::buffer::BufferId, killed_value: Value) -> Result<Option<neovm_edit::buffer::BufferId>, Flow> {
                absent_layer_panic("ABSENT-LAYER core::buffer_kill.before");
            }

            pub(crate) fn kmacro_start(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::kmacro.start");
            }

            pub(crate) fn kmacro_end(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::kmacro.end");
            }

            pub(crate) fn kmacro_call_last(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::kmacro.call_last");
            }

            pub(crate) fn kmacro_execute(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::kmacro.execute");
            }

            pub(crate) fn kmacro_store_event(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER core::kmacro.store_event");
            }

            pub(crate) fn display_query_redisplay_idle_skip() -> bool {
                absent_layer_panic("ABSENT-LAYER display::display_query.redisplay_idle_skip");
            }

            pub(crate) fn display_query_mode_line_flow() -> bool {
                absent_layer_panic("ABSENT-LAYER display::display_query.mode_line_flow");
            }

            pub(crate) fn display_query_face_realization() -> bool {
                absent_layer_panic("ABSENT-LAYER display::display_query.face_realization");
            }

            pub(crate) fn display_query_frame_tty_target(frame: &Frame) -> bool {
                absent_layer_panic("ABSENT-LAYER display::display_query.frame_tty_target");
            }

            pub(crate) fn display_window_ensure_selected_frame_id(frames: &mut FrameManager, buffers: &mut BufferManager) -> FrameId {
                absent_layer_panic("ABSENT-LAYER display::display_window.ensure_selected_frame_id");
            }

            pub(crate) fn display_window_remember_selected_window_point(frames: &mut FrameManager, buffers: &mut BufferManager, frame: FrameId) {
                absent_layer_panic("ABSENT-LAYER display::display_window.remember_selected_window_point");
            }

            pub(crate) fn display_window_window_body_dimensions(frames: &FrameManager, buffers: &BufferManager, frame: FrameId, window: WindowId) -> Result<(i64, i64), Flow> {
                absent_layer_panic("ABSENT-LAYER display::display_window.window_body_dimensions");
            }

            pub(crate) fn display_window_resize_mini_window(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER display::display_window.resize_mini_window");
            }

            pub(crate) fn display_window_update_auto_hscroll(ctx: &mut Context) {
                absent_layer_panic("ABSENT-LAYER display::display_window.update_auto_hscroll");
            }

            pub(crate) fn display_window_fontset_generation() -> u64 {
                absent_layer_panic("ABSENT-LAYER display::display_window.fontset_generation");
            }

            pub(crate) fn display_window_window_domain_frame_of(domain: WindowDomain, frames: &FrameManager, window: WindowId) -> Option<FrameId> {
                absent_layer_panic("ABSENT-LAYER display::display_window.window_domain_frame_of");
            }

            pub(crate) fn display_window_window_regions_at(geometry: &PresentationGeometry, presentation: PresentationId, window: WindowId) -> Option<WindowRegions> {
                absent_layer_panic("ABSENT-LAYER display::display_window.window_regions_at");
            }

            pub(crate) fn display_window_motion_resolve(ctx: &mut Context, request: MotionRequest) -> Result<Option<MeasuredMotion>, Flow> {
                absent_layer_panic("ABSENT-LAYER display::display_window.motion_resolve");
            }

            pub(crate) fn display_window_window_line_wrap(ctx: &mut Context, window: Option<Value>, buffer: BufferId, engine: MotionEngine) -> LineWrap {
                absent_layer_panic("ABSENT-LAYER display::display_window.window_line_wrap");
            }

            pub(crate) fn display_window_window_body_width(ctx: &mut Context, args: &[Value]) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER display::display_window.window_body_width");
            }

            pub(crate) fn display_window_window_hscroll(ctx: &mut Context, args: &[Value]) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER display::display_window.window_hscroll");
            }

            pub(crate) fn display_window_invisible_status(ctx: &mut Context, property: Value) -> Result<Invisibility, Flow> {
                absent_layer_panic("ABSENT-LAYER display::display_window.invisible_status");
            }

            pub(crate) fn display_window_invisible_source_run_end(ctx: &mut Context, buffer: BufferId, position: usize, context: InvisibleRunContext) -> Result<Option<usize>, Flow> {
                absent_layer_panic("ABSENT-LAYER display::display_window.invisible_source_run_end");
            }

            pub(crate) fn perf_record(op: PerfOp, elapsed: std::time::Duration) {
                absent_layer_panic("ABSENT-LAYER system::perf.record");
            }

            pub(crate) fn perf_reset() {
                absent_layer_panic("ABSENT-LAYER system::perf.reset");
            }

            pub(crate) fn perf_fingerprint(acc: &mut u64) {
                absent_layer_panic("ABSENT-LAYER system::perf.fingerprint");
            }

            pub(crate) fn wait_service_input_pending_with_timers(ctx: &mut Context) -> Result<(), Flow> {
                absent_layer_panic("ABSENT-LAYER core::wait.service_input_pending_with_timers");
            }

            pub(crate) fn wait_service_read_char_preliminary_redisplay(ctx: &mut Context, idle_policy: IdleTransitionPolicy) -> Result<(), Flow> {
                absent_layer_panic("ABSENT-LAYER core::wait.service_read_char_preliminary_redisplay");
            }

            pub(crate) fn wait_service_timers_without_redisplay(ctx: &mut Context) -> Result<(), Flow> {
                absent_layer_panic("ABSENT-LAYER core::wait.service_timers_without_redisplay");
            }

            pub(crate) fn wait_for_command_input_with_idle_policy(ctx: &mut Context, deadline: Option<std::time::Instant>, idle_policy: IdleTransitionPolicy) -> Result<CommandInputWaitOutcome, Flow> {
                absent_layer_panic("ABSENT-LAYER core::wait.wait_for_command_input_with_idle_policy");
            }

            pub(crate) fn wait_for_resize_ack_until(ctx: &mut Context, deadline: std::time::Instant) -> Result<bool, Flow> {
                absent_layer_panic("ABSENT-LAYER core::wait.wait_for_resize_ack_until");
            }

            pub(crate) fn timer_next_ordinary_gnu_timer_timeout_before(ctx: &Context) -> Option<std::time::Duration> {
                absent_layer_panic("ABSENT-LAYER core::timer.next_ordinary_gnu_timer_timeout_before");
            }

            pub(crate) fn timer_next_idle_gnu_timer_timeout(ctx: &Context) -> Option<std::time::Duration> {
                absent_layer_panic("ABSENT-LAYER core::timer.next_idle_gnu_timer_timeout");
            }

            pub(crate) fn frontend_apply_xwidget_frontend_event(ctx: &mut Context, event: &FrontendWebViewEvent) -> Result<bool, Flow> {
                absent_layer_panic("ABSENT-LAYER system::frontend.apply_xwidget_frontend_event");
            }

            pub(crate) fn display_service_compositor_scrolling_enabled(ctx: &Context, window: WindowId) -> bool {
                absent_layer_panic("ABSENT-LAYER display::display_service.compositor_scrolling_enabled");
            }

            pub(crate) fn display_service_permits_compositor_pixel_scroll(ctx: &Context, window: WindowId) -> bool {
                absent_layer_panic("ABSENT-LAYER display::display_service.permits_compositor_pixel_scroll");
            }

            pub(crate) fn display_service_redisplay_buffer_layout_inputs(ctx: &Context, buffer: BufferId) -> Option<BufferLayoutInputState> {
                absent_layer_panic("ABSENT-LAYER display::display_service.redisplay_buffer_layout_inputs");
            }

            pub(crate) fn file_notify_expand_file_name(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER system::file_notify.expand_file_name");
            }

            pub(crate) fn file_notify_directory_file_name(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER system::file_notify.directory_file_name");
            }

            pub(crate) fn file_notify_file_exists_p(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
                absent_layer_panic("ABSENT-LAYER system::file_notify.file_exists_p");
            }

            /// The upper finalizer table cannot be compiled in without the
            /// core/system layer, so the install is a recorded ASSERT-EMPTY
            /// that reports the typed Installed status (H2a).
            pub(crate) fn absent_sqlite_installer(
            ) -> Result<neovm_value::HookInstallStatus, neovm_value::HookInstallError> {
                absent_layer_assert_empty(
                    "ABSENT-LAYER core::sqlite.install_finalizer_table",
                    true,
                );
                Ok(neovm_value::HookInstallStatus::Installed)
            }
        }
    };
}
