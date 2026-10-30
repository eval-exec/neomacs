//! Rebuilding a Context from a portable dump image.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;
use crate::emacs_core::symbol::CanonicalAccess;

impl Context {
    /// Reconstruct an Context from pdump data.
    ///
    /// Thread-local heap pointers and caches must already be set by the caller
    /// before calling this.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_dump(
        tagged_heap: crate::tagged::gc::AliasableBox<crate::tagged::gc::TaggedHeap>,
        obarray: Obarray,
        lexenv: Value,
        features: Vec<SymId>,
        require_stack: Vec<SymId>,
        loads_in_progress: Vec<crate::heap_types::LispString>,
        buffers: crate::buffer::buffer::OwnedBufferStorageParts,
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
        selected_global_map: super::super::keymap::SelectedGlobalMap,
        kmacro: KmacroManager,
        registers: RegisterManager,
        bookmarks: BookmarkManager,
        watchers: VariableWatcherList,
    ) -> Self {
        // A restored image can contain byte-code values before any Context
        // constructor ran; install the structural semantics before the first
        // one can be compared or hashed.
        crate::emacs_core::bytecode::semantics::install();
        let dumped_function_surface = obarray.clone();
        let mut obarray = obarray;
        let core_eval_symbols = install_core_eval_symbols(&mut obarray, false);
        let tagged_heap = tagged_heap;
        crate::tagged::gc::set_tagged_heap_root(tagged_heap.root());
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
            treesit: super::super::treesit::TreeSitterManager::new(),
            frames: lisp_frame_manager(),
            coding_systems: coding_systems,
            macro_perf_stats: MacroPerfStats::default(),
            buffers: buffers,
            global: super::global_state::GlobalState {
                modes,
                face_table,
                abbrevs,
                interactive,
                rectangle,
                selected_global_map,
                kmacro,
                registers,
                bookmarks,
                ..super::global_state::GlobalState::fresh(crate::keyboard::CommandLoop::new())
            },
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
            window_edge_drag: crate::emacs_core::window_edge_drag::WindowEdgeDrag::default(),
            specpdl: Vec::new(),
            suspended_thread_bindings: Vec::new(),
            profiler: super::super::profiler::ProfilerState::default(),
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
            buffers: world.buffers,
            xwidgets: world.global.xwidgets,
            last_overlay_modification_hooks: Vec::new(),
            interval_insert_behind_hooks: Value::NIL,
            interval_insert_in_front_hooks: Value::NIL,
            match_data: None,
            combine_after_change_list: Vec::new(),
            combine_after_change_buffer: None,
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
            shutdown_in_progress: false,
            daemon: world.global.daemon,
            _reserved_input_mode_interrupt: true,
            quit_char: world.global.quit_char,
            waiting_for_user_input: false,
            frames: world.frames,
            modes: world.global.modes,
            threads: world.global.threads,
            kmacro: world.global.kmacro,
            command_loop: world.global.command_loop,
            input_rx: crate::host_input::HostInputReceiver::default(),
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
                crate::code_conversion_workspace::CodeConversionWorkspace::default(),
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
            ccl_registry: super::super::ccl::current_ccl_registry_handle(),
            charset_registry: super::super::charset::current_charset_registry_handle(),
            font_registry: super::super::xfaces::current_font_registry_handle(),
            terminal_registry: super::super::terminal::pure::current_terminal_registry_handle(),
            hash_table_test_registry:
                super::super::builtins::current_hash_table_test_registry_handle(),
            file_notify_registry: super::super::builtins::current_file_notify_registry_handle(),
            window_configuration_registry:
                super::super::builtins::current_window_configuration_registry_handle(),
            dynamic_module_registry:
                super::super::dynamic_module::current_dynamic_module_registry_handle(),
            in_flight_registry: super::super::error::current_in_flight_registry_handle(),
            cached_standard_case_table: None,
        };
        ev.setup_thread_locals();

        // Rebuild the builtin subr registry after pdump restore. The dumped
        // obarray already carries the authoritative runtime function-cell
        // surface, so restore that surface immediately afterward.
        builtins::init_builtins(&mut ev);
        for (sym_id, symbol) in dumped_function_surface.iter_symbols() {
            if !symbol.function.get().is_nil() {
                ev.obarray.set_symbol_function_id_for(
                    sym_id,
                    symbol.function.get(),
                    crate::emacs_core::symbol::FunctionEpochBump::PdumpRestore,
                );
            } else if dumped_function_surface.is_function_unbound_id(sym_id) {
                ev.obarray.fmakunbound_id(sym_id);
            } else {
                ev.obarray.clear_function_silent_id(sym_id);
            }
        }

        ev.provide_value(
            Value::symbol("make-network-process"),
            Some(super::super::process::make_network_process_subfeatures()),
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
                super::super::window_cmds::restore_gnu_configuration_hook_default,
            );
        }

        ev.finish_runtime_activation(true);

        ev
    }

    pub(crate) fn install_pdump_image(
        &mut self,
        image: super::super::pdump::mmap_image::LoadedMmapImage,
    ) {
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
}
