//! Rebuilding a Context from a portable dump image.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::hooks_proof::EvalHooksInstalled;
use super::*;
use neovm_value::symbol::CanonicalAccess;

impl Context {
    /// Reconstruct an Context from pdump data.
    ///
    /// Thread-local heap pointers and caches must already be set by the caller
    /// before calling this.
    #[allow(clippy::too_many_arguments)]
    #[doc(hidden)]
    pub fn from_dump(
        tagged_heap: neovm_value::tagged::gc::AliasableBox<neovm_value::tagged::gc::TaggedHeap>,
        obarray: Obarray,
        lexenv: Value,
        features: Vec<SymId>,
        require_stack: Vec<SymId>,
        loads_in_progress: Vec<neovm_value::heap_types::LispString>,
        buffers: neovm_edit::buffer::buffer::OwnedBufferStorageParts,
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
        selected_global_map: crate::emacs_core::eval::keymap_lookup::SelectedGlobalMap,
        kmacro: KmacroManager,
        registers: RegisterManager,
        bookmarks: BookmarkManager,
        watchers: VariableWatcherList,
        _hooks: EvalHooksInstalled,
    ) -> Self {
        // A restored image can contain byte-code values before any Context
        // constructor ran; install the structural semantics before the first
        // one can be compared or hashed.
        // Every core-owned cold table (lifecycle, process, input, command
        // layer, display query, perf) was published by the E3a boot entry
        // before this constructor was allowed to run (the proof parameter).
        crate::emacs_core::bytecode::semantics::install_eval_bytecode_semantics_seam();
        // The function-cell observer is process state, not image state, and a
        // restored image can hold byte-code functions before any Context
        // constructor ran. Install it exactly where the semantics hook is
        // installed; skipping this silently loses redefinition pinning and
        // inline eviction in every image-restored process.
        #[cfg(feature = "jit")]
        crate::emacs_core::jit::install_function_cell_observer_seam();
        // The text-property storage in the value layer roots its intermediate
        // conses through the evaluator's scratch-root stack; hand it the
        // stack operations here, at the same one-time hooks.
        super::construct::install_scratch_root_hooks_seam();
        // The string position cache lives above heap_types; publish its
        // byte-mutation stamp at the same one-time seam. A restored image can
        // reach conversions before any Context constructor ran.
        // SAFETY: seam install (peer seal N9). The hook invalidates the
        // position cache before returning, and this is image restore, one of
        // the two pinned seams.
        let installed = unsafe {
            neovm_value::heap_types::install_string_bytes_changed_hook(
                &neovm_value::string_pos_cache::STRING_BYTES_STAMP,
            )
        };
        debug_assert!(
            installed.is_ok(),
            "a different byte-mutation stamp is already installed"
        );
        // The value layer owns EvalError's Display table; the printer installs
        // its one formatting algorithm here, at the same one-time hooks.
        crate::emacs_core::print::install_eval_error_format_hook();
        // The value layer also owns TaggedValue's Display table; the printer
        // installs its one formatting algorithm at the same seam, so a
        // restored image formats values before any Context constructor ran.
        crate::emacs_core::print::install_value_display_format_hook();
        // The sqlite object-finalizer table is process-global; the boot
        // install entry publishes it with the rest of the hook set.
        // The command-layer and display-query tables are part of the E3a
        // boot set, published above this constructor.
        // The evaluator's observed-run log of function-epoch moves is plain
        // process state too; install it unconditionally so non-JIT builds
        // keep the verify-run mismatch report.
        super::cconv_memo::install_function_epoch_move_sink_seam();
        // The buffer substrate asks the text layer for composition and
        // syntax queries through host-free fn-pointer tables; a restored
        // image builds buffers before any Context constructor runs, so
        // publish them at this one-time seam too.  Both installs are
        // idempotent.
        neovm_text::emacs_core::composite::install_substrate_seams();
        neovm_text::emacs_core::syntax::install_substrate_seams();
        // The text layer's host-free engines need the same hot-path timing
        // hooks from a restored image (stage-3b C7); the install is
        // idempotent and delegates to perf_trace's existing counters.
        super::construct::install_text_substrate_hooks();
        let dumped_function_surface = obarray.clone();
        let mut obarray = obarray;
        let core_eval_symbols = install_core_eval_symbols(&mut obarray, false);
        let tagged_heap = tagged_heap;
        neovm_value::tagged::gc::set_tagged_heap_root(tagged_heap.root());
        let noninteractive = obarray
            .canonical_value_or_nil(
                CanonicalAccess::DUMP,
                core_eval_symbols.noninteractive_symbol,
            )
            .is_truthy();
        let symbols_with_pos_enabled = obarray
            .canonical_value_or_nil(
                CanonicalAccess::DUMP,
                core_eval_symbols.symbols_with_pos_enabled_symbol,
            )
            .is_truthy();
        let print_symbols_bare = obarray
            .canonical_value_or_nil(
                CanonicalAccess::DUMP,
                core_eval_symbols.print_symbols_bare_symbol,
            )
            .is_truthy();
        let compiler_function_overrides_active = obarray
            .canonical_value_or_nil(
                CanonicalAccess::DUMP,
                core_eval_symbols.compiler_function_overrides_symbol,
            )
            .is_cons();
        let quit_flag = obarray
            .canonical_value_or_nil(CanonicalAccess::DUMP, core_eval_symbols.quit_flag_symbol);
        let inhibit_quit = obarray
            .canonical_value_or_nil(CanonicalAccess::DUMP, core_eval_symbols.inhibit_quit_symbol);
        let throw_on_input = obarray.canonical_value_or_nil(
            CanonicalAccess::DUMP,
            core_eval_symbols.throw_on_input_symbol,
        );

        let world = world::WorldStorageBuilder {
            owned_roots: Default::default(),
            obarray: obarray,
            features: features,
            watchers: watchers,
            active_variable_watchers: HashSet::new(),
            autoloads: autoloads,
            treesit: neovm_text::emacs_core::treesit::TreeSitterManager::new(),
            frames: lisp_frame_manager(),
            coding_systems: coding_systems,
            macro_perf_stats: MacroPerfStats::default(),
            buffers: buffers,
            global: super::global_state::GlobalState::fresh_then(
                crate::emacs_core::eval::data::command_loop::CommandLoop::new(),
                |global| {
                    global.modes = modes;
                    global.face_table = face_table;
                    global.abbrevs = abbrevs;
                    global.interactive = interactive;
                    global.rectangle = rectangle;
                    global.selected_global_map = selected_global_map;
                    global.kmacro = kmacro;
                    global.registers = registers;
                    global.bookmarks = bookmarks;
                },
            ),
            // Prepare allocation-capable managers before retiring the old TLS
            // installation. The closed factory reinstalls the final view.
            tagged_heap,
        }
        .build();
        let mut ev = Self {
            integer_width_context: Default::default(),
            owned_roots: world.owned_roots,
            tagged_heap: world.tagged_heap,
            pdump_image: None,
            after_pdump_load_hook_pending: false,
            cached_system_name: world.global.cached_system_name,
            obarray: world.obarray,
            window_edge_drag: super::data::window_edge_drag::WindowEdgeDrag::default(),
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
            attention: attention::attention_of(
                quit_flag,
                throw_on_input,
                compiler_function_overrides_active,
            ),
            unwind_cleanup_depth: 0,
            noninteractive_symbol: core_eval_symbols.noninteractive_symbol,
            noninteractive,
            symbols_with_pos_enabled_symbol: core_eval_symbols.symbols_with_pos_enabled_symbol,
            symbols_with_pos_enabled,
            print_symbols_bare_symbol: core_eval_symbols.print_symbols_bare_symbol,
            runtime_projection_mask: core_eval_symbols.runtime_projection_mask,
            print_symbols_bare,
            features: world.features,
            require_stack,
            loads_in_progress,
            load_read_cursors: Vec::new(),
            last_uncaught_signal_backtrace: None,
            edit: neovm_edit::buffer::EditRuntime::new(world.buffers),
            xwidgets: world.global.xwidgets,
            match_data: None,
            _reserved_message_log_need_newline: false,
            processes: world.global.processes,
            watchers: world.watchers,
            active_variable_watchers: world.active_variable_watchers,
            standard_syntax_table,
            syntax_code_objects,
            standard_category_table,
            current_local_map,
            selected_global_map: world.global.selected_global_map,
            registers: world.global.registers,
            bookmarks: world.global.bookmarks,
            abbrevs: world.global.abbrevs,
            autoloads: world.autoloads,
            custom,
            rectangle: world.global.rectangle,
            interactive: world.global.interactive,
            treesit: world.treesit,
            minibuffers: world.global.minibuffers,
            interactive_minibuffer_read_count: world.global.interactive_minibuffer_read_count,
            current_message: world.global.current_message,
            echo_area_buffers: world.global.echo_area_buffers,
            echo_area_resize_exact_pending: false,
            _reserved_debugging_output_file: None,
            _reserved_message_buf_print: false,
            minibuffer_selected_window: world.global.minibuffer_selected_window,
            active_minibuffer_window: world.global.active_minibuffer_window,
            shutdown_request: None,
            loadup_dump_mode: None,
            shutdown_phase: super::ShutdownPhase::Running,
            daemon: world.global.daemon,
            _reserved_input_mode_interrupt: true,
            quit_char: world.global.quit_char,
            waiting_for_user_input: false,
            frames: world.frames,
            modes: world.global.modes,
            threads: world.global.threads,
            kmacro: world.global.kmacro,
            command_loop: world.global.command_loop,
            input_rx: crate::emacs_core::eval::host_input::HostInputReceiver::default(),
            eval_task_rx: None,
            quit_requested: QuitRequest::new(),
            redisplay_fn: None,
            mode_line_display_flow: None,
            redisplay_prepare_fn: None,
            font_shape_fn: None,
            gstring_shape_cache: world.global.gstring_shape_cache,
            display_idle_maintenance_fn: None,
            scroll_preview_fn: None,
            frame_snapshot_fn: None,
            window_layout_query_adapter: WindowLayoutQueryAdapter::Unavailable,
            scroll_goal: None,
            display_host: None,
            tty_frame_host_factory: None,
            visual_config: neomacs_display_protocol::VisualConfig::default(),
            pending_menu_bar_popup_anchor: None,
            coding_systems: world.coding_systems,
            code_conversion_workspace:
                neovm_text::code_conversion_workspace::CodeConversionWorkspace::default(),
            face_table: world.global.face_table,
            face_change_count: 0,
            materialized_face_table_source: world.global.materialized_face_table_source,
            display_var_change_count: 0,
            input_progress: Default::default(),
            redisplay_generation: 0,
            body_redisplay_all: 0,
            body_redisplay_by_window: FxHashMap::default(),
            body_redisplay_by_buffer: FxHashMap::default(),
            menu_bar_rebuild_generation: world.global.menu_bar_rebuild_generation,
            chrome_dirty: Default::default(),
            gnu_redisplay_hooks: redisplay_hooks::RedisplayHookOwnership::initial(),
            context_instance_id: next_context_instance_id(),
            media_generation: 0,
            last_redisplay_signature: world.global.last_redisplay_signature,
            depth: 0,
            eval_counter: 0,
            max_depth: 1600,
            gc_pending: false,
            gc_count: 0,
            last_activation_thread: None,
            gc_inhibit_depth: 0,
            gc_driver_active: false,
            gc_stress: gc_stress_from_env(),
            gc_runtime_settings_cache: GcRuntimeSettingsCache::default(),
            vm_root_frames: Vec::new(),
            backtrace_args_stack: Vec::new(),
            eval_temp_roots: Vec::new(),
            eval_call_roots: Vec::new(),
            sequence_temp_root_frames: Vec::new(),
            bc_buf: Vec::with_capacity(4096),
            jit_root_stack: Vec::new(),
            jit_root_stack_ptr: std::ptr::null_mut(),
            jit_root_stack_top: 0,
            jit_root_stack_cap: 0,
            jit_stack_limit: 0,
            native_stack_probe: Default::default(),
            bytecode_stack_position: crate::emacs_core::bytecode::vm::BC_STACK_POSITION_INITIAL,
            jit_stack_scratch: [0; super::native_stack::JIT_STACK_SCRATCH_WORDS],
            bc_frames: Vec::new(),
            condition_stack: Vec::new(),
            next_resume_id: 1,
            pending_safe_funcalls: world.global.pending_safe_funcalls,
            compiler_function_overrides_symbol: core_eval_symbols
                .compiler_function_overrides_symbol,
            compiler_function_overrides_active,
            symbol_bytecode_call_cache:
                crate::emacs_core::bytecode::vm::SymbolByteCodeCallCache::new(),
            interpreter_stacks: crate::emacs_core::bytecode::vm::InterpreterStackPool::new(),
            jit_bind_stack: Vec::new(),
            owner: owner_bindings::OwnerSlot::DORMANT,
            apply_fast_path_epoch: std::cell::Cell::new(u64::MAX),
            named_call_cache: FxHashMap::with_capacity_and_hasher(
                NAMED_CALL_CACHE_CAPACITY,
                Default::default(),
            ),
            form_head_cache: FormHeadCache::default(),
            lexenv_assq_cache: LexenvAssqCache::default(),
            lexenv_special_cache: LexenvSpecialCache::default(),

            macro_expansion_scope_depth: 0,
            macro_expansion_mutation_epoch: 0,
            macro_expand_calls: 0,
            macro_expand_total_us: 0,
            macro_perf_enabled: std::env::var_os("NEOVM_TRACE_MACRO_PERF").is_some(),
            macro_perf_stats: world.macro_perf_stats,
            interpreted_closure_filter_fn: None,
            cconv_memo: cconv_memo::CconvMemo::from_env(),
            tier_i: tier_i::TierI::from_env(),
            fringe_bitmaps: world.global.fringe_bitmaps,
            ccl_registry: neovm_text::emacs_core::ccl::current_ccl_registry_handle(),
            charset_registry: neovm_text::emacs_core::charset::current_charset_registry_handle(),
            font_registry: crate::emacs_core::eval::lifecycle::current_font_registry_handle(),
            terminal_registry: crate::emacs_core::eval::lifecycle::current_terminal_registry_handle(
            ),
            hash_table_test_registry:
                super::super::builtins::current_hash_table_test_registry_handle(),
            file_notify_registry: super::super::builtins::current_file_notify_registry_handle(),
            window_configuration_registry:
                super::super::builtins::current_window_configuration_registry_handle(),
            dynamic_module_registry:
                super::data::dynamic_module_registry::current_dynamic_module_registry_handle(),
            in_flight_registry: neovm_value::error::current_in_flight_registry_handle(),
            cached_standard_case_table: None,
            frame_and_buffer_state: None,
        };
        ev.setup_thread_locals();

        // Rebuild the builtin subr registry after pdump restore. The dumped
        // obarray already carries the authoritative runtime function-cell
        // surface, so restore that surface immediately afterward.
        super::hooks_proof::register_core_subrs(&mut ev);
        for (sym_id, symbol) in dumped_function_surface.iter_symbols() {
            if !symbol.function.get().is_nil() {
                ev.obarray.set_symbol_function_id_for(
                    sym_id,
                    symbol.function.get(),
                    neovm_value::symbol::FunctionEpochBump::PdumpRestore,
                );
            } else if dumped_function_surface.is_function_unbound_id(sym_id) {
                ev.obarray.fmakunbound_id(sym_id);
            } else {
                ev.obarray.clear_function_silent_id(sym_id);
            }
        }

        ev.provide_value(
            Value::symbol("make-network-process"),
            Some(crate::emacs_core::eval::data::process_state::make_network_process_subfeatures()),
        )
        .expect("startup make-network-process provide should succeed");

        // The fringe-bitmap registry is reconstructed empty by `from_dump` (it
        // is not part of the dump image), so re-seed GNU's standard built-in
        // bitmaps. The `'fringe` indices may already be set on the dumped
        // symbols; re-`put`ting the same value is idempotent.
        ev.pre_register_standard_fringe_bitmaps();

        // An image stamped under the legacy policy can omit this C-owned
        // hook entirely. Preserve every saved cell, then let the existing
        // final activation adopt GNU forwarding/special metadata.
        // SAFETY: this cold hook-default repair only defines a missing special
        // value cell. It cannot call Lisp or retarget canonical dispatch metadata.
        unsafe {
            ev.obarray.with_legacy_value_write(
                super::redisplay_hooks::restore_gnu_configuration_hook_default,
            );
        }

        ev.finish_runtime_activation(true);

        ev
    }

    #[doc(hidden)]
    pub fn install_pdump_image(&mut self, image: super::super::pdump::mmap_image::LoadedMmapImage) {
        // Leak: see the field doc — global interner aliases and mapped heap
        // objects reference the image for the remainder of the process.
        self.pdump_image = Some(&*Box::leak(Box::new(image)));
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn pdump_image_contains_ptr(&self, ptr: *const u8) -> bool {
        self.pdump_image
            .as_ref()
            .is_some_and(|image| image.contains_ptr(ptr))
    }
}
