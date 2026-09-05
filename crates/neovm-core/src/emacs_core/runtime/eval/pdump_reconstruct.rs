//! Rebuilding a Context from a portable dump image.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    /// Reconstruct an Context from pdump data.
    ///
    /// Thread-local heap pointers and caches must already be set by the caller
    /// before calling this.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_dump(
        tagged_heap: Box<crate::tagged::gc::TaggedHeap>,
        obarray: Obarray,
        lexenv: Value,
        features: Vec<SymId>,
        require_stack: Vec<SymId>,
        loads_in_progress: Vec<crate::heap_types::LispString>,
        buffers: BufferManager,
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
        let dumped_function_surface = obarray.clone();
        let mut obarray = obarray;
        let core_eval_symbols = install_core_eval_symbols(&mut obarray, false);
        let mut tagged_heap = tagged_heap;
        crate::tagged::gc::set_tagged_heap(&mut tagged_heap);
        let noninteractive = obarray
            .symbol_value_id_or_nil(core_eval_symbols.noninteractive_symbol)
            .is_truthy();
        let symbols_with_pos_enabled = obarray
            .symbol_value_id_or_nil(core_eval_symbols.symbols_with_pos_enabled_symbol)
            .is_truthy();
        let print_symbols_bare = obarray
            .symbol_value_id_or_nil(core_eval_symbols.print_symbols_bare_symbol)
            .is_truthy();
        let compiler_function_overrides_active = obarray
            .symbol_value_id_or_nil(core_eval_symbols.compiler_function_overrides_symbol)
            .is_cons();
        let quit_flag = obarray.symbol_value_id_or_nil(core_eval_symbols.quit_flag_symbol);
        let inhibit_quit = obarray.symbol_value_id_or_nil(core_eval_symbols.inhibit_quit_symbol);
        let throw_on_input =
            obarray.symbol_value_id_or_nil(core_eval_symbols.throw_on_input_symbol);

        let mut ev = Self {
            tagged_heap,
            pdump_image: None,
            after_pdump_load_hook_pending: false,
            cached_system_name: Value::NIL,
            obarray,
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
            unwind_cleanup_depth: 0,
            noninteractive_symbol: core_eval_symbols.noninteractive_symbol,
            noninteractive,
            symbols_with_pos_enabled_symbol: core_eval_symbols.symbols_with_pos_enabled_symbol,
            symbols_with_pos_enabled,
            print_symbols_bare_symbol: core_eval_symbols.print_symbols_bare_symbol,
            print_symbols_bare,
            features,
            require_stack,
            loads_in_progress,
            load_read_cursors: Vec::new(),
            last_uncaught_signal_backtrace: None,
            buffers,
            xwidgets: super::super::xwidget::XwidgetState::new(),
            last_overlay_modification_hooks: Vec::new(),
            interval_insert_behind_hooks: Value::NIL,
            interval_insert_in_front_hooks: Value::NIL,
            match_data: None,
            combine_after_change_list: Vec::new(),
            combine_after_change_buffer: None,
            message_log_need_newline: false,
            processes: ProcessManager::new(),
            watchers,
            active_variable_watchers: HashSet::new(),
            standard_syntax_table,
            syntax_code_objects,
            standard_category_table,
            current_local_map,
            selected_global_map,
            registers,
            bookmarks,
            abbrevs,
            autoloads,
            custom,
            rectangle,
            interactive,
            treesit: super::super::treesit::TreeSitterManager::new(),
            minibuffers: MinibufferManager::new(),
            interactive_minibuffer_read_count: 0,
            current_message: None,
            echo_area_buffers: EchoAreaBuffers::default(),
            echo_area_resize_exact_pending: false,
            debugging_output_file: None,
            message_buf_print: false,
            minibuffer_selected_window: None,
            active_minibuffer_window: None,
            shutdown_request: None,
            input_mode_interrupt: true,
            quit_char: 7,
            waiting_for_user_input: false,
            frames: lisp_frame_manager(),
            modes,
            threads: ThreadManager::new(),
            kmacro,
            command_loop: crate::keyboard::CommandLoop::new(),
            input_rx: None,
            eval_task_rx: None,
            quit_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            redisplay_fn: None,
            frame_snapshot_fn: None,
            window_layout_query_adapter: WindowLayoutQueryAdapter::Unavailable,
            pending_pixel_scroll: None,
            display_host: None,
            tty_frame_host_factory: None,
            visual_config: neomacs_display_protocol::VisualConfig::default(),
            pending_menu_bar_popup_anchor: None,
            coding_systems,
            code_conversion_workspace:
                crate::code_conversion_workspace::CodeConversionWorkspace::default(),
            face_table,
            face_change_count: 0,
            materialized_face_table_source: None,
            display_var_change_count: 0,
            redisplay_generation: 0,
            menu_bar_rebuild_generation: 0,
            chrome_dirty: Default::default(),
            context_instance_id: next_context_instance_id(),
            media_generation: 0,
            last_redisplay_signature: None,
            depth: 0,
            eval_counter: 0,
            max_depth: 1600,
            gc_pending: false,
            gc_count: 0,
            gc_inhibit_depth: 0,
            gc_driver_active: false,
            gc_stress: gc_stress_from_env(),
            gc_runtime_settings_cache: GcRuntimeSettingsCache::default(),
            vm_root_frames: Vec::new(),
            backtrace_args_stack: Vec::new(),
            eval_temp_roots: Vec::new(),
            sequence_temp_root_frames: Vec::new(),
            bc_buf: Vec::with_capacity(4096),
            jit_root_stack: Vec::new(),
            jit_root_stack_ptr: std::ptr::null_mut(),
            jit_root_stack_top: 0,
            jit_root_stack_cap: 0,
            bc_frames: Vec::new(),
            condition_stack: Vec::new(),
            next_resume_id: 1,
            pending_safe_funcalls: Vec::new(),
            compiler_function_overrides_symbol: core_eval_symbols
                .compiler_function_overrides_symbol,
            compiler_function_overrides_active,
            named_call_cache: FxHashMap::with_capacity_and_hasher(
                NAMED_CALL_CACHE_CAPACITY,
                Default::default(),
            ),
            lexenv_assq_cache: LexenvAssqCache::default(),
            lexenv_special_cache: LexenvSpecialCache::default(),

            macro_expansion_scope_depth: 0,
            macro_expansion_mutation_epoch: 0,
            macro_expand_calls: 0,
            macro_expand_total_us: 0,
            macro_perf_enabled: std::env::var_os("NEOVM_TRACE_MACRO_PERF").is_some(),
            macro_perf_stats: MacroPerfStats::default(),
            interpreted_closure_filter_fn: None,
            fringe_bitmaps: super::super::builtins::fringe_bitmap::FringeBitmapRegistry::new(),
        };
        ev.setup_thread_locals();

        // Rebuild the builtin subr registry after pdump restore. The dumped
        // obarray already carries the authoritative runtime function-cell
        // surface, so restore that surface immediately afterward.
        builtins::init_builtins(&mut ev);
        for (sym_id, symbol) in dumped_function_surface.iter_symbols() {
            if !symbol.function.is_nil() {
                ev.obarray.set_symbol_function_id(sym_id, symbol.function);
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
