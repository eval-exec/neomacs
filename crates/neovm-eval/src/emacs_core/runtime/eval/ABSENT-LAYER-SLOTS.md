# Eval absent-layer slots: reachability and classification

This is the review table for the stand-ins that
`crates/neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs` expands into
eval's test-only unit runtime. It records, for every stand-in, the production
accessor it replaces, the code that reaches that accessor in eval's unit
runtime, and one of three classes. It is a documentation commit: no stand-in
changed.

## Method

- Source of the list: every `"ABSENT-LAYER <marker>"` in `absent_layer.rs`.
  The original survey covered 131 distinct markers (132 entries;
  `display::fontset.alias_alist_startup_value` is named by both the marker
  helper and the slot function). The eight stand-ins that no unit table
  installed were then deleted, and (a1) removed the four no-caller slots,
  leaving 116 distinct markers (116 rows; the original 132 entries contained
  one duplicate marker row for the fontset alias).
- Install status: cross-checked against the hook tables in
  `crates/neovm-eval/src/test_utils.rs` (`LIFECYCLE`, `INPUT`, `PROCESS`,
  `CASE_TABLES`, `KMACRO`, `DISPLAY_QUERY`, `DISPLAY_WINDOW`, `PERF`, `SET`,
  `WAIT`, `TIMER`, `FRONTEND`, `DISPLAY_SERVICE`, `FILE_NOTIFY`,
  `CALL_INTERACTIVELY`). A stand-in that the unit table does not install is
  marked **NOT-INSTALLED**: it is dead code in eval, not a live refusal.
- Reachability: for each slot we found the production accessor (the
  `pub(crate) fn` in a `runtime/eval` module that reads the hook table field),
  then opened every call site of that accessor in `crates/` outside
  `tests/`, `test_utils.rs` and `absent_layer.rs`. "Reachable" means reachable
  by any eval test, or by eval-side Lisp a test can evaluate, in the unit
  runtime.
- `kind`: `panic` (bare `absent_layer_panic`), `ASSERT-EMPTY`
  (`absent_layer_assert_empty`), `unlinked` (recorded no-op), or `value`
  (marker/handle returned).

## Classes

- **(a) Unreachable in eval's unit runtime.** No product caller, or every
  caller sits in a path the unit runtime never executes (bootstrap/pdump
  restore, redisplay, command loop, display service, GUI input). Per PEER
  5795, the bare panic IS the premise assertion here and the slot STAYS a
  panic: a fold would invent an answer and go silent the day a test reaches
  the path. Three follow-ups apply:
  - (a1) delete the hook field, the production accessor and the stand-in for
    slots whose production accessor has no caller at all;
  - (a2) delete stand-ins that no unit table installs (the completion trio,
    the five case-table seams, and the slots that already have exact unit
    impls);
  - (a3) every remaining panic message names the path that reaches it.
- **(b) Reachable, core/system-owned.** A Lisp builtin or service path can
  reach the slot, but the implementation belongs below eval (fileio, process,
  timer/wait, kmacro, command loop, display, minibuf). The slot stays a panic
  in eval and the witness test belongs in the lower layer (many already exist
  under `crates/neovm-core/src/emacs_core/tests/moved_eval/`). Before any eval
  test that hits a (b) slot is deleted, its core twin must show an executing
  nextest PASS.
- **(c) Reachable with an exact unit state.** The slot is reached on ordinary
  eval paths (Context construction, green carrier park/resume, pdump reset) and
  the unit runtime's state is provably empty/exact (no display registry, no
  active module env, empty process table). The slot must ASSERT its premise
  where the state is observable (the 6138 shape), not merely document it.
- **(p) Poison marker.** The slot returns an uninterned marker as a startup
  value (`fontset-alias-alist`, the upper bootstrap Lisp_Object variables).
  This is not an "exact" value: a consumer that reads it must fail loudly
  rather than treat the marker as data. Acceptable only while the printed name
  plainly contains `ABSENT-LAYER` and no eval test asserts on the value; both
  are verified below.

Counts after the PEER 5795 reclassification and the (a1) deletions:
(a) 43, (b) 42, (c) 29, (p) 2 (116 rows). The eight deleted stand-ins were classed (a) in the original
survey; they were the three completion seams and the five case-table seams,
and `neovm-display`'s unit runtime now installs the real eval functions in
their place. `display::terminal.frame_terminal` moved (c) -> (b) per F-1;
`core::wait.wait_for_duration_until_timer_deadline` moved (b) -> (c)
subject to the F-4 premise asserts; the two poison markers moved (c) -> (p)
per F-3.

## system:: (37)

| marker | kind | production accessor | reaching path in eval's unit runtime | class |
|---|---|---|---|---|
| system::dynamic_module.reset | unlinked | lifecycle.rs:911 | green carrier reset_step `dynamic_module` (carrier_tls.rs:227); exact state: no active module envs | c |
| system::dynamic_module.load_module | panic | lifecycle.rs:964 | Lisp `load` of a module-suffixed file (lisp/load/mod.rs:2901) hands to the system dynamic-module loader; module loading is system-owned | b |
| system::dynamic_module.apply_module_function | panic | lifecycle.rs:1071 | apply.rs:4070, reached only from apply.rs:3454 `ValueKind::ModuleFunction`; no such value can exist without the system loader | a |
| system::dynamic_module.collect_gc_roots | ASSERT-EMPTY | lifecycle.rs:1085 | thread_local_roots.rs:76 GC sink enumerates module-call frames; exact premise: none | c |
| system::sqlite.world_state_swap (non-empty slot) | ASSERT-EMPTY | lifecycle.rs:974 | green carrier swap (green/carried.rs:33,144); exact placeholder `AbsentLayerWorldState` | c |
| system::dbusbind.world_state_swap (non-empty slot) | ASSERT-EMPTY | lifecycle.rs:984 | green carrier swap (green/carried.rs:38,144); exact placeholder | c |
| system::timefns.world_state_swap (non-empty slot) | ASSERT-EMPTY | lifecycle.rs:994 | green carrier swap (green/carried.rs:83,144); exact placeholder | c |
| system::dbusbind.drain_events | panic | lifecycle.rs:1090 | `read-event`/`read-char` service loops (keyboard.rs:3068,4856); D-Bus is system-owned | b |
| system::dbusbind.has_active_watches | panic | lifecycle.rs:1095 | `read-char` blocking condition (keyboard.rs:4931) asks the unlinked D-Bus watch set | b |
| system::dbusbind.reset_thread_locals | unlinked | lifecycle.rs:1100 | reset_builtins_thread_locals pdump hook (stubs.rs:227, pdump/runtime.rs:88); exact state: no D-Bus TLS | c |
| system::boot.apply_post_image_init | panic | lifecycle.rs:1029 | only finalize_cached_bootstrap_eval (lisp/load/mod.rs:5797); unit tests never build/restore a dump | a |
| system::boot.install_host_environment_snapshot | panic | lifecycle.rs:1039 | only post-pdump finalize / loadup bootstrap (lisp/load/mod.rs:5723,6383) | a |
| system::fileio.expand_file_name_entry | panic | lifecycle.rs:1110 | `load` file search (lread/mod.rs:1233) and kill-emacs auto-save unlink (command_loop.rs:367); fileio is system-owned | b |
| system::fileio.dispatch_file_handler | panic | lifecycle.rs:1115 | `load` builtin dispatches the system file-name-handler table (misc_eval.rs:822) | b |
| system::fileio.lock_file | panic | lifecycle.rs:1127 | EditHost modification lock (edit_host.rs:404) -> system filelock | b |
| system::fileio.unlock_file | panic | lifecycle.rs:1132 | set-buffer-modified-p / kill-buffer / kill-emacs (edit_host.rs:409, command_loop.rs:339, core kill_buffer/mod.rs:150) | b |
| system::fileio.unlink_file_path | panic | lifecycle.rs:1138 | kill-emacs auto-save-list unlink (command_loop.rs:394), raw system fileio | b |
| system::timefns.reset | unlinked | lifecycle.rs:999 | pdump load hook on every Context::new heap reset (pdump/runtime.rs:91); exact state: no time-zone TLS | c |
| system::sqlite.reset | unlinked | lifecycle.rs:1105 | reset_builtins_thread_locals (stubs.rs:222, pdump/runtime.rs:88); exact state: no SQLite TLS | c |
| system::process.open_channel | panic | process_hooks.rs:266 | only the unlinked system dynamic-module loader (system/dynamic_module/mod.rs:1868) | a |
| system::process.print_handle | panic | process_hooks.rs:271 | Lisp printer for a `Process` value (lisp/print/mod.rs:2354,3834); handle text is system-owned | b |
| system::process.wait_notifier | panic | process_hooks.rs:277 | inotify/kqueue/w32notify add-watch and the async wait path (gc_pacing.rs:1081, file_notify platform lisp.rs); process-owned | b |
| system::process.terminate_and_defer_reaping | panic | process_hooks.rs:282 | LiveProcessIo::drop only for `ChildOwnership::Unreaped` (process_state.rs:1583); only a system-spawned child creates that | a |
| system::input.interrupt_for_input_event_if_requested | panic | input_hooks.rs:118 | `read-char`/wait special-input policy (keyboard.rs:3078,4216); policy body is core's CommandLoopHost | b |
| system::input.signal_wake_pipe_read_fd | unlinked (None) | input_hooks.rs:142 | every ProcessManager/ProcessWaitBackend construction (process_state.rs:1671); exact state: no POSIX self-pipe | c |
| system::input.drain_signal_wake_pipe | panic | input_hooks.rs:150 | only for a registered self-pipe fd (process_state.rs:1761); read_fd is None, so the key cannot occur | a |
| system::perf.record | panic | perf.rs:139 | perf_enabled=false in the unit SET (test_utils.rs:996) gates record_op/time_op; never dispatched | a |
| system::perf.reset | unlinked | perf.rs:146 | green carrier reset_step `perf_trace` (carrier_tls.rs:251); exact state: no system tracing state | c |
| system::frontend.apply_xwidget_frontend_event | panic | hooks_proof.rs:376 | `InputEvent::WebView` handling (keyboard.rs:3147,4367); only a display/webview frontend produces it | a |
| system::file_notify.expand_file_name | panic | hooks_proof.rs:312 | kqueue/w32notify add-watch (platform/macos, windows lisp.rs); not reachable on Linux eval unit tests; fileio-owned | b |
| system::file_notify.directory_file_name | panic | hooks_proof.rs:312 | same kqueue/w32notify watch-add path; fileio-owned | b |
| system::file_notify.file_exists_p | panic | hooks_proof.rs:312 | macos kqueue add-watch existence probe; fileio-owned | b |

## display:: (56 distinct markers)

| marker | kind | production accessor | reaching path in eval's unit runtime | class |
|---|---|---|---|---|
| display::fontset.alias_alist_startup_value | value (poison) | lifecycle.rs:1361 | Context construction seeds fontset-alias-alist (construct.rs:527) with the uninterned marker; printed name is `ABSENT-LAYER display::fontset.alias_alist_startup_value`, no eval test asserts on the value (PEER 5795 F-3) | p |
| display::boot.clear_created_lisp_face | panic | lifecycle.rs:1009 | bootstrap/pdump surface normalization only (lisp/load/mod.rs:5255,5482) | a |
| display::boot.remove_face_new_frame_defaults_entry | panic | lifecycle.rs:1014 | same bootstrap-only pass (lisp/load/mod.rs:5259,5486) | a |
| display::boot.restore_created_faces_from_table | panic | lifecycle.rs:1019 | clear_runtime_loader_state dump restore (lisp/load/mod.rs:5718) | a |
| display::boot.gui_window_system_symbol | panic | lifecycle.rs:1024 | bootstrap restore (lisp/load/mod.rs:5565,5568) and GUI keyboard forwarding (keyboard_forwarding.rs:39) | a |
| display::boot.seed_batch_startup_frame | ASSERT-EMPTY override | lifecycle.rs:1054 | NOT-INSTALLED as a panic: unit table installs `unit_boot_seed_batch_startup_frame` (test_utils.rs:479,644); `force-mode-line-update` witnesses it (vm_test.rs:4024) | c |
| display::boot.init_frame_lisp_faces | panic | lifecycle.rs:1317 | display Frame constructor (neovm-display/window/display_ext.rs:2260) | a |
| display::boot.prepare_runtime_faces | panic | lifecycle.rs:1322 | Context::sync_runtime_faces_for_frame (eval/mod.rs:5413) from display tooltip/layout paths | a |
| display::boot.sync_runtime_face_table | panic | lifecycle.rs:1327 | same display/layout face-realization path (eval/mod.rs:5419) | a |
| display::boot.ensure_lisp_face_id_property | panic | lifecycle.rs:1332 | Context::set_face_attribute (eval/mod.rs:5435) called only from `neomacs/src/main.rs` startup | a |
| display::boot.clear_font_cache_state | unlinked | lifecycle.rs:1337 | every Context::new via register_core_load_hooks (pdump/runtime.rs:86); no display font cache | c |
| display::terminal.handle_value | panic | lifecycle.rs:1143 | GUI MonitorsChanged input events (keyboard.rs:3109,4435); unit tests inject no GUI events | a |
| display::terminal.delete_noelisp | ASSERT-EMPTY override | lifecycle.rs:1175 | NOT-INSTALLED as a panic: `unit_terminal_delete_noelisp` (test_utils.rs:438) asserts the empty terminal record | c |
| display::terminal.frame_terminal | panic | lifecycle.rs:1342 | `variable-binding-locus` on a Kboard-forwarded variable (symbols.rs:3869) delegates to `frame-terminal`; GNU's `Fframe_terminal` (data.c:2523-2524) returns the terminal, so this is NOT exact -- witnesses belong in core (PEER 5795 F-1) | b |
| display::terminal.print_handle | unlinked (None, premise-asserted) | lifecycle.rs:1347 | `prin1` and error formatting query it (lisp/print/mod.rs:3444, error_format.rs:93); asserts the empty terminal registry before answering None (a record can exist; PEER 5795 F-2, done) | c |
| display::terminal.configure_window_system_runtime | panic | lifecycle.rs:1352 | GUI keyboard-forwarding config only (keyboard_forwarding.rs:34) | a |
| display::terminal.park_carrier_tls | unlinked | lifecycle.rs:1391 | green carrier park reset_step (carrier_tls.rs:158,486); no display terminal TLS | c |
| display::terminal.reset_thread_locals | unlinked | lifecycle.rs:1404 | pdump restore_active_runtime (pdump/runtime.rs:146, pdump/mod.rs:764); no display terminal TLS | c |
| display::terminal.reset_handle | unlinked | lifecycle.rs:1409 | every Context::new reset_runtime_for_new_heap (pdump/runtime.rs:135); no terminal handle | c |
| display::terminal.world_state_swap (non-empty slot) | ASSERT-EMPTY | lifecycle.rs:1419 | green carrier park/resume (green/carried.rs:58,252); exact empty placeholder | c |
| display::terminal.live_terminal_ids | ASSERT-EMPTY override | lifecycle.rs:1424 | NOT-INSTALLED as a panic: `unit_terminal_live_ids` answers the exact empty registry (test_utils.rs) | c |
| display::terminal.current_registry_handle | value | lifecycle.rs:1429 | every Context::new captures RegistryHandles::current (construct.rs:149, pdump_reconstruct.rs:347); empty registry | c |
| display::terminal.install_registry_handle | unlinked | lifecycle.rs:1434 | eval setup_thread_locals_inner (gc_pacing.rs:866); no display snapshot TLS | c |
| display::terminal.install_owner_registry_handle | unlinked | lifecycle.rs:1439 | eval owner activation (gc_pacing.rs:870, carrier_tls.rs:477); lower registry untouched | c |
| display::fontset.list_value | panic | lifecycle.rs:1366 | Lisp `fontset-list` (eval stub, stubs.rs:1093) delegates to the display-owned fontset registry | b |
| display::fontset.reset | unlinked | lifecycle.rs:1371 | every Context::new reset_builtins_thread_locals (builtins/mod.rs:114, symbols.rs:4391); no registry | c |
| display::fontset.restore (non-empty registry) | ASSERT-EMPTY | lifecycle.rs:1381 | eval pdump restore_active_runtime (pdump/convert.rs:7061, pdump/mod.rs:766) with the empty snapshot | c |
| display::window_tree.window_resize_check | panic | lifecycle.rs:1268 | redisplay mini-window resize staging (redisplay_hooks.rs:551) only | a |
| display::window_tree.apply_mini_window_resize | panic | lifecycle.rs:1386 | redisplay mini-window preparation (redisplay_hooks.rs:517) only | a |
| display::window_tree.frame_resize_pixelwise | panic | lifecycle.rs:1280 | native GUI resize events / opening-frame size sync (keyboard.rs:1609,1831) | a |
| display::window_tree.frame_parameter_entry | panic | lifecycle.rs:1295 | system selected-frame-display/`display` fallback (system/environment/mod.rs:355); core `frame-parameter` implementation | b |
| display::xfaces.park_carrier_tls | unlinked | lifecycle.rs:1444 | green carrier park reset_step (carrier_tls.rs:162,554); no display face TLS | c |
| display::xfaces.current_registry_handle | value | lifecycle.rs:1457 | every Context::new captures the absent face handle (construct.rs:148, pdump_reconstruct.rs:346) | c |
| display::xfaces.install_registry_handle | unlinked | lifecycle.rs:1462 | eval setup_thread_locals (gc_pacing.rs:864); display face TLS absent | c |
| display::display_query.redisplay_idle_skip | panic | display_query_hooks.rs:281 | command-loop redisplay (command_loop.rs:998, redisplay_hooks.rs:1240) | a |
| display::display_query.mode_line_flow | panic | display_query_hooks.rs:286 | command-loop mode-line policy (command_loop.rs:904) | a |
| display::display_query.face_realization | panic | display_query_hooks.rs:291 | redisplay/command-loop face policy and RowsObservation (rows_observation.rs:143,188; command_loop.rs:905,1048) | a |
| display::display_query.frame_tty_target | panic | display_query_hooks.rs:296 | RowsObservation TTY admission (rows_observation.rs:197,313) | a |
| display::display_window.ensure_selected_frame_id | panic | display_query_hooks.rs:308 | minibuffer activation (minibuffer_window.rs:111); core minibuf tests own the witness | b |
| display::display_window.remember_selected_window_point | panic | display_query_hooks.rs:313 | minibuffer activation and redisplay (vm_shared.rs:352, minibuffer_window.rs:134, redisplay_hooks.rs:1169, command_loop.rs:929) | b |
| display::display_window.window_body_dimensions | panic | display_query_hooks.rs:322 | redisplay glyph-matrix adjustment (redisplay_hooks.rs:51,976; display/window/display.rs:50) | a |
| display::display_window.resize_mini_window | panic | display_query_hooks.rs:332 | core's apply_prepared_minibuffer_resize facade (redisplay_hooks.rs:571); display-owned `resize-mini-window-internal` | b |
| display::display_window.update_auto_hscroll | panic | display_query_hooks.rs:337 | redisplay and command-loop auto-hscroll (redisplay_hooks.rs:1252, command_loop.rs:1029) | a |
| display::display_window.window_domain_frame_of | panic | display_query_hooks.rs:347 | `minibuffer-selected-window` via Context::minibuffer_selected_window_id (minibuffer_window.rs:85); display-owned | b |
| display::display_window.window_regions_at | panic | display_query_hooks.rs:399 | GUI mouse-position descriptor (keyboard.rs:6511) | a |
| display::display_window.motion_resolve | panic | display_query_hooks.rs:356 | `vertical-motion`/`compute-motion` via EditHost (edit_host.rs:459); display-owned | b |
| display::display_window.window_line_wrap | panic | display_query_hooks.rs:364 | `vertical-motion`/`line-number-display-width` line-wrap via EditHost (edit_host.rs:479); display-owned | b |
| display::display_window.window_body_width | panic | display_query_hooks.rs:374 | EditHost window geometry (edit_host.rs:484); display-owned `window-body-width` | b |
| display::display_window.window_hscroll | panic | display_query_hooks.rs:379 | EditHost window geometry (edit_host.rs:488); display-owned `window-hscroll` | b |
| display::display_window.invisible_status | panic | display_query_hooks.rs:384 | `current-indentation`/`vertical-motion` over invisible text via EditHost (edit_host.rs:439); display-owned | b |
| display::display_window.invisible_source_run_end | panic | display_query_hooks.rs:389 | `current-indentation` over invisible runs via EditHost (edit_host.rs:435); display-owned | b |
| display::display_service.compositor_scrolling_enabled | panic | hooks_proof.rs:388 | command-loop redisplay_signature compositor policy (command_loop.rs:1231) | a |
| display::display_service.permits_compositor_pixel_scroll | panic | hooks_proof.rs:395 | command-loop redisplay_signature compositor policy (command_loop.rs:1239) | a |
| display::display_service.redisplay_buffer_layout_inputs | panic | hooks_proof.rs:402 | redisplay and command-loop redisplay signature (redisplay_hooks.rs:1066, command_loop.rs:1266) | a |

## core:: (38 markers)

### command, case tables, buffer kill, kmacro, call-interactively, sqlite, boot (29)

| marker | kind | production accessor | reaching path in eval's unit runtime | class |
|---|---|---|---|---|
| core::boot.ensure_startup_compat_variables | panic | lifecycle.rs:1004 | eval load/pdump bootstrap wrapper only (lisp/load/mod.rs:4157,5724,6973,6996) | a |
| core::boot.apply_lread_constants | panic | lifecycle.rs:1034 | clear_runtime_loader_state on bootstrap/pdump finalize (lisp/load/mod.rs:5668,5721,6592) | a |
| core::boot.shell_file_name_install | panic | lifecycle.rs:1044 | loadup PreloadOnly bootstrap (lisp/load/mod.rs:6384) | a |
| core::boot.shell_file_name_install_dump_default | panic | lifecycle.rs:1049 | loadup Dump bootstrap (lisp/load/mod.rs:6389) | a |
| core::command.window_configuration_capture | panic | lifecycle.rs:1188 | save-window-excursion VM/JIT (vm.rs:6192, jit/compile/dispatch.rs:1433, hooks.rs:94); capture body is core window_hooks `current-window-configuration` | b |
| core::command.window_configuration_restore | panic | lifecycle.rs:1194 | `set-window-configuration`/save-window-excursion restore (hooks.rs:110, eval/mod.rs:1537); body is core | b |
| core::command.run_redisplay_window_change_hooks | panic | lifecycle.rs:1203 | command-loop redisplay and RedisplayHookOwnership::run (command_loop.rs:1022, redisplay_hooks.rs:1236) | a |
| core::command.unwind_minibuffer_session | panic | lifecycle.rs:1210 | NativeUnwindAction::run (eval/mod.rs:1539); action recorded only by core read_minibuf | a |
| core::command.unwind_minibuffer_buffer | panic | lifecycle.rs:1215 | NativeUnwindAction::run (eval/mod.rs:1542); recorded only by core read_minibuf | a |
| core::command.get_char_property | panic | lifecycle.rs:1220 | Context::adjust_point_for_property (vm_shared.rs:121) from core command_loop_1 (core/keyboard/command_loop.rs:956) | a |
| core::command.menu_bar_top_level_items | panic | lifecycle.rs:1240 | display menu-bar navigation (neovm-display/display/mod.rs:1679) | a |
| core::command.key_binding | panic | lifecycle.rs:1245 | display menu shortcut resolver (display/menu/shortcuts.rs:30); Lisp `key-binding` registers core builtin directly | a |
| core::command.where_is_internal | panic | lifecycle.rs:1250 | display menu shortcut resolver (shortcuts.rs:39); Lisp `where-is-internal` registers core builtin directly | a |
| core::call_interactively.resolve | panic | call_interactively.rs:656 | eval VM `call-interactively` dispatch (vm.rs:10548); resolver body is core callint | b |
| core::buffer_kill.before | panic | buffer_kill.rs:74 | green_before_buffer_killed (green/api.rs:225) from core `kill-buffer`; requires a parked green thread; unit runtime registers kill-buffer as a panicking subr | a |
| core::kmacro.start | panic | kmacro_hooks.rs:115 | eval VM dispatch of `start-kbd-macro` (vm.rs:10437); body is core kmacro | b |
| core::kmacro.end | panic | kmacro_hooks.rs:120 | eval VM dispatch of `end-kbd-macro` (vm.rs:10443); core-owned | b |
| core::kmacro.call_last | panic | kmacro_hooks.rs:125 | eval VM `call-last-kbd-macro` (vm.rs:7961); core-owned | b |
| core::kmacro.execute | panic | kmacro_hooks.rs:130 | eval VM `execute-kbd-macro` (vm.rs:7966); core-owned | b |
| core::kmacro.store_event | panic | kmacro_hooks.rs:135 | eval VM `store-kbd-macro-event` (vm.rs:10454); core-owned | b |
| core::sqlite.install_finalizer_table | ASSERT-EMPTY | hooks_proof.rs:257 | eval unit_runtime install_eval_hooks publishes no finalizer table; empty premise holds | c |

### wait and timer (8) and upper-variable binder (1)

| marker | kind | production accessor | reaching path in eval's unit runtime | class |
|---|---|---|---|---|
| core::wait.service_input_pending_with_timers | panic | hooks_proof.rs:320 | GUI command-boundary idle service (keyboard/display_service.rs:39) invoked by core command_loop.rs:573; earlier eval test was re-homed as core display_service_class_b_test.rs | b |
| core::wait.service_read_char_preliminary_redisplay | panic | hooks_proof.rs:324 | `read-event`/`read-char`/`read-key-sequence` via read_char (keyboard.rs:4880); core moved keyboard tests | b |
| core::wait.service_timers_without_redisplay | panic | hooks_proof.rs:331 | read_char timer service (keyboard.rs:4971) and the core-used test fence (test_support.rs:1099); system-owned | b |
| core::wait.wait_for_command_input_with_idle_policy | panic | hooks_proof.rs:335 | timed read-event blocking wait (keyboard.rs:5007); system-owned | b |
| core::wait.wait_for_resize_ack_until | panic | hooks_proof.rs:343 | wait_for_pending_resize_events (keyboard.rs:2867) from display and core eval_test.rs:3469 | b |
| core::wait.wait_for_duration_until_timer_deadline | panic (not installed) | hooks_proof.rs:350 | NOT-INSTALLED: unit table installs a working Green/select `unit_wait_for_duration_until_timer_deadline` (test_utils.rs:973), which now ASSERTS the empty live process table before sleeping (PEER 5795 F-4); the self-pipe half was dropped as vacuous per PEER 5796 (that slot is constant None and the unit poller handle is unreachable) | c |
| core::timer.next_ordinary_gnu_timer_timeout_before | panic | hooks_proof.rs:363 | unbounded read-event/read-key-sequence timer query (keyboard.rs:4955); system-owned | b |
| core::timer.next_idle_gnu_timer_timeout | panic | hooks_proof.rs:367 | idle-timer query (keyboard.rs:4956); system-owned | b |
| upper-variable::{name} | value (poison) | lifecycle.rs:1060 | every eval unit Context construction binds upper bootstrap Lisp_Object vars to the uninterned marker (construct.rs:1473); printed name is `ABSENT-LAYER upper-variable::<name>`, no eval test asserts on the values (PEER 5795 F-3) | p |

## Notes for the follow-up commits

- The eight never-installed stand-ins (three completion seams, five case-table
  seams) have been deleted. `neovm-display`'s unit runtime installed those
  stand-ins, so it now installs the real eval functions instead; the eval
  unit table already did.
- Six slots are already substituted by exact unit implementations in eval
  (`unit_process_shutdown_noop`, the exact OS-signal pending/drain,
  `unit_wait_for_duration_until_timer_deadline`, `unit_boot_seed_batch_startup_frame`,
  `unit_terminal_delete_noelisp`, `unit_terminal_live_ids`). Their stand-ins
  are the (a2) deletions; the unit substitutions remain. The
  wait-for-duration substitution now asserts its only assertable premise
  (empty live process table; PEER 5796 dropped the vacuous self-pipe half),
  and the terminal print-handle asserts the empty registry before its None
  (F-2).
- (a) stays panics (PEER 5795): (a1) deleted the four no-caller hook
  fields/accessors/stand-ins (`process.unlink`, `process.delete`,
  `window_tree.char_property_window_lookup`, `window_tree.window_resize_apply`)
  together with their PLAIN-PUB and PROMOTION-LEDGER rows;
  (a2) delete the not-installed stand-ins; (a3) make every remaining (a)
  panic message name the reaching path -- done: each marker now carries
  " (no unit path: ...)" with the reaching function/path summarized from the
  table's "reaching path" column.
- The (b) group should not gain eval tests; the witnesses live, or should
  live, under `crates/neovm-core/src/emacs_core/tests/moved_eval/` with the real
  hooks installed. Before an eval test that hits a (b) slot is deleted, its
  core twin must show an executing nextest PASS.
- `test_utils.rs`'s `unit_runtime_runtime_paths_reach_only_unlinked_slots`
  doc comment still names `ABSENT-LAYER core::case_tables.sync`; that slot was
  deleted in b978fd3132 and the comment needs the same cleanup when the next
  test-support commit touches that area.
