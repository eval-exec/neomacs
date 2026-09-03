//! Constructing a Context: the constructors and VM harnesses, bootstrap variable seeding, and the C-level DEFVAR registrations that mirror GNU's syms_of_*() functions.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    pub fn new() -> Self {
        let mut ctx = Self::new_inner(true);
        // Register builtins AFTER new_inner returns — the function is too
        // large (1500+ lines) for reliable codegen in debug mode when
        // combined with the full native subr manifest in the same frame.
        builtins::init_builtins(&mut ctx);
        // Seed GNU's 24 standard built-in fringe bitmaps (right-arrow, left-arrow,
        // continuation/truncation markers, …) and their `'fringe` indices into
        // the registry, AFTER the obarray is populated by init_builtins.
        ctx.pre_register_standard_fringe_bitmaps();
        ctx
    }

    pub(crate) fn ensure_startup_messages_buffer(&mut self) {
        // GNU's initialized batch/runtime state has a live `*Messages*`
        // buffer before user Lisp runs: `emacs.c` clears pre-dump messages via
        // `message_dolog`, whose xdisp.c path creates `messages-buffer-name`.
        // Keep it after the initial minibuffer in buffer-list order and do not
        // select it.
        if self.buffers.find_buffer_by_name("*Messages*").is_none() {
            self.buffers.create_buffer("*Messages*");
        }
    }

    #[cfg(test)]
    pub(crate) fn new_vm_runtime_harness() -> Self {
        // GNU bytecode executes inside the same callable runtime surface as the
        // ordinary evaluator. Keep the default VM harness on that full surface.
        Self::new()
    }

    #[cfg(test)]
    pub(crate) fn new_minimal_vm_harness() -> Self {
        // Keep this reduced constructor only for low-level VM/opcode tests
        // that intentionally do not depend on the full builtin surface.
        let mut ev = Self::new_inner(true);
        ev.obarray = Obarray::new();
        super::super::errors::init_standard_errors(&mut ev.obarray);
        ev.obarray
            .set_symbol_value("most-positive-fixnum", Value::fixnum(i64::MAX >> 2));
        ev.obarray.make_special("most-positive-fixnum");
        ev.obarray.set_constant("most-positive-fixnum");
        ev.obarray
            .set_symbol_value("most-negative-fixnum", Value::fixnum(-(i64::MAX >> 2) - 1));
        ev.obarray.make_special("most-negative-fixnum");
        ev.obarray.set_constant("most-negative-fixnum");
        ev.specpdl.clear();
        ev.backtrace_args_stack.clear();
        ev.lexenv = Value::NIL;
        ev.features.clear();
        ev.require_stack.clear();
        ev.loads_in_progress.clear();
        ev.load_read_cursors.clear();
        ev.last_uncaught_signal_backtrace = None;
        ev.buffers = BufferManager::new();
        ev.xwidgets = super::super::xwidget::XwidgetState::new();
        ev.last_overlay_modification_hooks.clear();
        ev.interval_insert_behind_hooks = Value::NIL;
        ev.interval_insert_in_front_hooks = Value::NIL;
        ev.match_data = None;
        ev.processes = ProcessManager::new();
        ev.watchers = VariableWatcherList::new();
        ev.current_local_map = Value::NIL;
        ev.selected_global_map = super::super::keymap::SelectedGlobalMap::default();
        ev.registers = RegisterManager::new();
        ev.bookmarks = BookmarkManager::new();
        ev.abbrevs = AbbrevManager::new();
        ev.autoloads = AutoloadManager::new();
        ev.custom = CustomManager::new();
        ev.rectangle = RectangleState::new();
        ev.interactive = InteractiveRegistry::new();
        ev.input_mode_interrupt = false;
        ev.frames = lisp_frame_manager();
        ev.modes = ModeRegistry::new();
        ev.threads = ThreadManager::new();
        ev.kmacro = KmacroManager::new();
        ev.command_loop = crate::keyboard::CommandLoop::default();
        ev.input_rx = None;
        ev.host_input_wait_backend = None;
        ev.eval_task_rx = None;
        ev.redisplay_fn = None;
        ev.frame_snapshot_fn = None;
        ev.window_layout_query_adapter = WindowLayoutQueryAdapter::Unavailable;
        ev.display_host = None;
        ev.coding_systems = CodingSystemManager::new();
        ev.face_table = FaceTable::new();
        ev.face_change_count = 0;
        ev.display_var_change_count = 0;
        ev.redisplay_generation = 0;
        ev.menu_bar_rebuild_generation = 0;
        ev.media_generation = 0;
        ev.last_redisplay_signature = None;
        ev.depth = 0;
        ev.max_depth = 1600;
        ev.gc_pending = false;
        ev.gc_count = 0;
        ev.gc_stress = gc_stress_from_env();
        ev.condition_stack.clear();
        ev.next_resume_id = 1;
        ev.named_call_cache.clear();

        ev.macro_expand_calls = 0;
        ev.macro_expand_total_us = 0;
        ev.macro_perf_enabled = std::env::var_os("NEOVM_TRACE_MACRO_PERF").is_some();
        ev.macro_perf_stats = MacroPerfStats::default();
        ev.interpreted_closure_filter_fn = None;
        register_subrs(&mut ev);
        ev.finish_runtime_activation(false);
        ev
    }

    /// GNU emacs.c / data.c / fns.c-level startup globals: version and
    /// platform identity, invocation paths, subprocess program names, the
    /// load/exec path environment, and process/terminal defaults. Pulled
    /// out of new_inner so the constructor reads as a sequence of phases
    /// (and stays small enough for reliable debug codegen; see the
    /// init_builtins note in Context::new).
    pub(super) fn seed_startup_platform_variables(
        obarray: &mut Obarray,
        default_directory: String,
    ) {
        // Set up standard global variables
        // Match GNU data.c: DEFVAR_LISP marks these symbols declared-special,
        // then make_symbol_constant installs the SYMBOL_NOWRITE trap.
        obarray.set_symbol_value("most-positive-fixnum", Value::fixnum(i64::MAX >> 2));
        obarray.make_special("most-positive-fixnum");
        obarray.set_constant("most-positive-fixnum");
        obarray.set_symbol_value("most-negative-fixnum", Value::fixnum(-(i64::MAX >> 2) - 1));
        obarray.make_special("most-negative-fixnum");
        obarray.set_constant("most-negative-fixnum");
        // Mathematical constants (defconst in float-sup.el)
        obarray.set_symbol_value("float-e", Value::make_float(std::f64::consts::E));
        obarray.set_symbol_value("float-pi", Value::make_float(std::f64::consts::PI));
        obarray.set_symbol_value("pi", Value::make_float(std::f64::consts::PI));
        obarray.set_symbol_value("emacs-version", Value::string(crate::GNU_EMACS_VERSION));
        obarray.make_special("emacs-version");
        obarray.set_symbol_value(
            "emacs-copyright",
            Value::string("Copyright (C) 2026 Free Software Foundation, Inc."),
        );
        obarray.make_special("emacs-copyright");
        obarray.set_symbol_value("emacs-major-version", Value::fixnum(31));
        obarray.set_symbol_value("emacs-minor-version", Value::fixnum(0));
        obarray.set_symbol_value("emacs-build-number", Value::fixnum(1));
        obarray.set_symbol_value("system-type", Value::symbol(gnu_system_type()));
        obarray.make_special("system-type");
        // GNU Emacs uses unibyte for default-directory during dump because
        // the locale isn't set up yet (see init_buffer in buffer.c).
        obarray.set_symbol_value(
            "default-directory",
            Value::unibyte_string(default_directory.clone()),
        );
        obarray.set_symbol_value(
            "command-line-default-directory",
            Value::unibyte_string(default_directory),
        );
        let obarray_object = Value::vector(vec![Value::NIL]);
        obarray.set_symbol_value("obarray", obarray_object);
        obarray.set_symbol_value("neovm--obarray-object", obarray_object);
        obarray.make_special("obarray");
        obarray.set_symbol_value("standard-input", Value::T);
        obarray.make_special("standard-input");
        obarray.set_symbol_value(
            "command-line-args",
            Value::list(vec![
                Value::string("neovm-worker"),
                Value::string("--batch"),
            ]),
        );
        obarray.make_special("command-line-args");
        obarray.set_symbol_value("command-line-args-left", Value::NIL);
        obarray.set_symbol_value("command-line-functions", Value::NIL);
        obarray.set_symbol_value("command-line-processed", Value::T);
        obarray.set_symbol_value("command-switch-alist", Value::NIL);
        obarray.set_symbol_value(
            "pdumper-fingerprint",
            Value::string(crate::emacs_core::pdump::fingerprint_hex()),
        );
        obarray.make_special("pdumper-fingerprint");
        // GNU emacs.c: set from argv[0]. NeoVM uses current exe path.
        let exe_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok());
        let invocation_name = exe_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "neomacs".to_string());
        let invocation_directory = exe_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| format!("{}/", d.to_string_lossy()))
            .unwrap_or_else(|| "./".to_string());
        obarray.set_symbol_value("invocation-name", Value::string(invocation_name));
        obarray.make_special("invocation-name");
        obarray.set_symbol_value("invocation-directory", Value::string(invocation_directory));
        obarray.make_special("invocation-directory");
        obarray.set_symbol_value("installation-directory", Value::NIL);
        obarray.make_special("installation-directory");
        // GNU `callproc.c` initializes this from the build-time `PATH_INFO`
        // (`epaths.h`, default "/usr/local/share/info"), never nil.  Lisp
        // assumes it is a string: `Info--default-directory-list` runs
        // `(file-name-as-directory configure-info-directory)`, which errors
        // with `(wrong-type-argument stringp nil)` when nil and breaks
        // `doom sync` (GitHub issue #127).  Mirror GNU's default constant.
        obarray.set_symbol_value(
            "configure-info-directory",
            Value::string("/usr/local/share/info"),
        );
        // GNU keyboard.c: internal--top-level-message for command loop entry
        obarray.set_symbol_value(
            "internal--top-level-message",
            Value::string("Back to top level"),
        );
        // charset.c:2426 DEFVAR_LISP, init nil.
        obarray.define_special_variable("charset-map-path", Value::NIL);
        obarray.set_symbol_value("doc-directory", Value::NIL);
        // warnings.el defcustom — needed before warnings.el loads
        obarray.set_symbol_value("warning-minimum-log-level", Value::keyword(":warning"));
        obarray.set_symbol_value("warning-minimum-level", Value::keyword(":warning"));
        // GNU callproc.c defines these with DEFVAR_LISP, so lexical-binding
        // Lisp must treat them as dynamically scoped special variables.
        obarray.set_symbol_value("process-environment", Value::NIL);
        obarray.make_special("process-environment");
        obarray.set_symbol_value("initial-environment", Value::NIL);
        obarray.make_special("initial-environment");
        // GNU uses "emacsclient" here because the matching client is part of
        // its installation.  Neomacs must advertise the client it owns, so
        // package probes do not accidentally select a host GNU emacsclient.
        for (name, program) in [
            ("ctags-program-name", "ctags"),
            ("etags-program-name", "etags"),
            ("hexl-program-name", "hexl"),
            ("emacsclient-program-name", "neomacsclient"),
            ("movemail-program-name", "movemail"),
            ("ebrowse-program-name", "ebrowse"),
            ("rcs2log-program-name", "rcs2log"),
        ] {
            obarray.set_symbol_value(name, Value::unibyte_string(program));
            obarray.make_special(name);
        }
        obarray.set_symbol_value("path-separator", Value::string(":"));
        obarray.make_special("path-separator");
        obarray.set_symbol_value("shared-game-score-directory", Value::NIL);
        obarray.set_symbol_value("system-messages-locale", Value::NIL);
        obarray.make_special("system-messages-locale");
        obarray.set_symbol_value("system-time-locale", Value::NIL);
        obarray.make_special("system-time-locale");
        obarray.set_symbol_value("before-init-time", Value::NIL);
        obarray.make_special("before-init-time");
        obarray.set_symbol_value("after-init-time", Value::NIL);
        obarray.make_special("after-init-time");
        obarray.set_symbol_value(
            "system-configuration",
            super::super::builtins_extra::system_configuration_value(),
        );
        obarray.make_special("system-configuration");
        obarray.set_symbol_value(
            "system-configuration-options",
            super::super::builtins_extra::system_configuration_options_value(),
        );
        obarray.make_special("system-configuration-options");
        obarray.set_symbol_value(
            "system-configuration-features",
            super::super::builtins_extra::system_configuration_features_value(),
        );
        obarray.make_special("system-configuration-features");
        // GNU `keyboard.c` defines this with DEFVAR_LISP, so lexical-binding
        // Lisp must treat it as dynamically scoped.
        obarray.set_symbol_value("delayed-warnings-list", Value::NIL);
        obarray.make_special("delayed-warnings-list");
        // GNU `subr.el` defines this with `defvar`; seed it for early warning
        // paths while preserving the same special-variable semantics.
        obarray.set_symbol_value("delayed-warnings-hook", Value::NIL);
        obarray.make_special("delayed-warnings-hook");
        obarray.set_symbol_value(
            "command-line-ns-option-alist",
            Value::list(vec![Value::list(vec![
                Value::string("-NSOpen"),
                Value::fixnum(1),
                Value::symbol("ns-handle-nxopen"),
            ])]),
        );
        obarray.set_symbol_value(
            "command-line-x-option-alist",
            Value::list(vec![Value::list(vec![
                Value::string("-display"),
                Value::fixnum(1),
                Value::symbol("x-handle-display"),
            ])]),
        );
        obarray.set_symbol_value("load-path", Value::NIL);
        obarray.make_special("load-path");
        obarray.set_symbol_value("load-history", Value::NIL);
        obarray.set_symbol_value(
            "fontset-alias-alist",
            super::super::builtins::symbols::fontset_alias_alist_startup_value(),
        );
        // GNU Emacs with module support includes the module suffixes before
        // compiled and source Lisp suffixes, secondary suffix first -- on darwin
        // `(".so" ".dylib" ".elc" ".el")`, see `load_suffixes_startup_values_for_os`.
        obarray.set_symbol_value(
            "load-suffixes",
            Value::list(
                super::super::lread::load_suffixes_startup_values_for_os(std::env::consts::OS)
                    .into_iter()
                    .map(Value::string)
                    .collect(),
            ),
        );
        obarray.make_special("load-suffixes");
        obarray.set_symbol_value(
            "module-file-suffix",
            Value::make_string(super::super::lread::module_file_suffix()),
        );
        obarray.make_special("module-file-suffix");
        obarray.set_symbol_value(
            "dynamic-library-suffixes",
            Value::list(
                super::super::lread::dynamic_library_suffixes_for_os(std::env::consts::OS)
                    .into_iter()
                    .map(Value::string)
                    .collect(),
            ),
        );
        obarray.make_special("dynamic-library-suffixes");
        obarray.set_symbol_value("dynamic-library-alist", Value::NIL);
        obarray.make_special("dynamic-library-alist");
        let dynamic_library_alist = intern("dynamic-library-alist");
        obarray
            .put_property_id(
                dynamic_library_alist,
                intern("risky-local-variable"),
                Value::T,
            )
            .expect("setting dynamic-library-alist property should not fail");
        // load-file-rep-suffixes: suffixes for alternate representations of
        // the same file (e.g., compressed ".gz").  Default is just ("").
        obarray.set_symbol_value(
            "load-file-rep-suffixes",
            Value::list(vec![Value::string("")]),
        );
        obarray.make_special("load-file-rep-suffixes");
        // file-coding-system-alist: needed by jka-cmpr-hook.el and others.
        obarray.set_symbol_value("file-coding-system-alist", Value::NIL);
        // GNU fns.c initializes `features' to include `emacs', and
        // thread.c:syms_of_threads provides `threads' when thread builtins
        // are installed.
        obarray.set_symbol_value("features", initial_features_value());
        super::super::xwidget::init_xwidget_variables(obarray);
        obarray.set_symbol_value_id(lexical_binding_symbol(), Value::NIL);
        obarray.set_symbol_value("load-file-name", Value::NIL);
        obarray.make_special("load-file-name");
        obarray.set_symbol_value("inhibit-quit", Value::NIL);
        obarray.set_symbol_value("float-output-format", Value::NIL);
        obarray.make_special("float-output-format");
        // GNU Emacs print.c: all print-* variables are DEFVAR_BOOL or
        // DEFVAR_LISP, making them dynamically scoped (special).
        // This is essential so `(let ((print-escape-newlines t)) ...)`
        // affects the C print code via dynamic binding.
        for name in [
            "print-length",
            "print-level",
            "print-circle",
            "print-gensym",
            "print-continuous-numbering",
            "print-number-table",
            "print-charset-text-property",
            "print-unreadable-function",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        obarray.set_symbol_value("text-quoting-style", Value::NIL);
        obarray.make_special("text-quoting-style");
        // GNU DEFVAR_LISP variables needed by loadup.el and early .el files.
        // chartab.c:1375 DEFVAR_LISP, init nil.
        obarray.define_special_variable("char-code-property-alist", Value::NIL);
        // redisplay--inhibit-bidi and resize-mini-windows are registered (with
        // GNU xdisp.c inits) by xdisp::register_bootstrap_vars.

        // GNU C variables checked by cus-start.el during bootstrap.
        // 178 DEFVAR_LISP/DEFVAR_INT/DEFVAR_BOOL variables extracted from
        // GNU Emacs -Q. Default values match GNU's init_*() functions.
        for name in [
            "alter-fullscreen-frames",
            "auto-save-visited-file-name",
            "blink-cursor-alist",
            "default-frame-alist",
            "display-fill-column-indicator-character",
            "display-line-numbers",
            "display-line-numbers-width",
            "enable-character-translation",
            "focus-follows-mouse",
            "line-number-display-limit",
            "make-pointer-invisible",
            "menu-bar-mode",
            "mode-line-compact",
            "mouse-autoselect-window",
            "resize-mini-frames",
            "ring-bell-function",
            "scalable-fonts-allowed",
            "scroll-preserve-screen-position",
            "show-trailing-whitespace",
            "tab-bar-mode",
            "tab-bar-position",
            "temp-buffer-show-function",
            "tool-bar-mode",
            "tool-bar-style",
            "treesit-extra-load-path",
            "treesit-auto-install-grammar",
            "treesit-enabled-modes",
            "treesit-language-remap-alist",
            "treesit-load-name-override-list",
            "treesit-languages-require-line-column-tracking",
            "treesit-major-mode-remap-alist",
            "treesit-thing-settings",
            // undo-outer-limit is registered (with its GNU src/undo.c init and
            // the src/emacs.c batch override) by undo::register_bootstrap_vars.
            "window-combination-resize",
            // Mouse pointer shapes — GNU defines these in
            // src/xfns.c (and parallel files w32fns.c, pgtkfns.c,
            // haikufns.c, androidfns.c) as integer Lisp_Object
            // variables that hold X cursor font codes. neomacs has
            // no native window-system bindings for these yet, so
            // they default to nil. Cursor audit Finding 9 in
            // drafts/cursor-audit.md flagged the symbols as
            // missing entirely; Lisp code that tried
            // (setq x-pointer-shape ...) hit void-variable.
            //
            // `x-nontext-pointer-shape' and `x-mode-pointer-shape' are NOT in
            // this list, and the omission is the point.  Every `DEFVAR_LISP'
            // GNU has for either is inside
            // `#if false /* This doesn't really do anything.  */' --
            // `src/xfns.c:10333-10338' and `10347-10352', and the same pair in
            // `src/androidfns.c'; `w32fns.c' and `haikufns.c' do not declare
            // them at all.  A declaration in a dead preprocessor branch is not
            // a declaration, so no GNU build binds the symbol and
            // `(boundp 'x-mode-pointer-shape)' is nil under GNU 31.0.90.
            // Seeding one here is entry 138's invented existence, reached
            // through a case that is not about a platform: the C global
            // `Vx_mode_pointer_shape' still exists and is still assigned
            // `Qnil' on the line after the `#endif', which is what makes the
            // seed look justified from the C side.  Nothing in GNU's `lisp/',
            // this tree's `lisp/', or either editor's own sources reads
            // either name.  (Ledger 168.)
            "x-pointer-shape",
            "x-sensitive-text-pointer-shape",
            "x-hourglass-pointer-shape",
            "x-window-horizontal-drag-cursor",
            "x-window-vertical-drag-cursor",
            "x-window-left-edge-cursor",
            "x-window-top-left-corner-cursor",
            "x-window-top-edge-cursor",
            "x-window-top-right-corner-cursor",
            "x-window-right-edge-cursor",
            "x-window-bottom-right-corner-cursor",
            "x-window-bottom-edge-cursor",
            "x-window-bottom-left-corner-cursor",
            "x-cursor-fore-pixel",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        // GNU `frame.c` initializes these global minor-mode variables in C:
        //   Vmenu_bar_mode = Qt
        //   Vtool_bar_mode = Qt   (when built with window-system support)
        // neomacs is a window-system-capable build, so match GNU's defaults
        // instead of starting graphical sessions with both modes forced off.
        obarray.set_symbol_value("menu-bar-mode", Value::T);
        obarray.set_symbol_value("tool-bar-mode", Value::T);
        for name in [
            "auto-hscroll-mode",
            "display-fill-column-indicator-column",
            "display-line-numbers-current-absolute",
            "make-cursor-line-fully-visible",
            "mouse-highlight",
            "overflow-newline-into-fringe",
            "select-active-regions",
            "x-select-enable-clipboard-manager",
        ] {
            obarray.set_symbol_value(name, Value::T);
            obarray.make_special(name);
        }
        // auto-save-interval/timeout, double-click-fuzz/time, meta-prefix-char
        // and polling-period are registered (with GNU keyboard.c values and
        // DEFVAR specialness) by keyboard::pure::register_bootstrap_vars.
        obarray.define_int_variable("display-line-numbers-major-tick", 0);
        obarray.define_int_variable("display-line-numbers-minor-tick", 0);
        obarray.define_special_variable("echo-keystrokes", Value::fixnum(1));
        obarray.define_int_variable("gc-cons-threshold", 800_000);
        obarray.set_symbol_value("help-char", Value::fixnum(8));
        // hourglass-delay, hscroll-margin/step, line-number-display-limit-width,
        // maximum-scroll-margin, messages-buffer-name, scroll-* and the
        // tool-bar label size are registered (with GNU xdisp.c values and
        // DEFVAR specialness) by xdisp::register_bootstrap_vars.
        obarray.set_symbol_value("message-log-max", Value::fixnum(1000));
        // next-screen-context-lines is registered by
        // window_cmds::register_bootstrap_vars; overline-margin by
        // xdisp::register_bootstrap_vars.
        obarray.define_int_variable("process-error-pause-time", 1);
        obarray.set_symbol_value("eol-mnemonic-dos", Value::string("\\"));
        obarray.set_symbol_value("eol-mnemonic-mac", Value::string("/"));
        obarray.set_symbol_value("eol-mnemonic-undecided", Value::string(":"));
        obarray.set_symbol_value("eol-mnemonic-unix", Value::string(":"));
        obarray.set_symbol_value(
            "report-emacs-bug-address",
            Value::string("bug-gnu-emacs@gnu.org"),
        );
        obarray.make_special("report-emacs-bug-address");
        // fns.c:6867 DEFVAR_LISP, build_unibyte_string ("(yes or no) ").
        obarray.define_special_variable("yes-or-no-prompt", Value::string("(yes or no) "));
        // Float-valued C variables
        obarray.set_symbol_value("gc-cons-percentage", Value::make_float(0.1));
        // max-mini-window-height is registered by xdisp::register_bootstrap_vars.
        // `max-image-size', `image-scaling-factor', `image-cache-eviction-delay',
        // `image-types' and `x-bitmap-file-path' are registered by
        // image::register_bootstrap_vars, GNU's `syms_of_image'.
        // Display engine C variables (xdisp.c)
        obarray.define_special_variable("global-mode-string", Value::NIL);
        // Fringe C variable (fringe.c `syms_of_fringe`: `Vfringe_bitmaps = Qnil`).
        // GNU binds this to nil; `lisp/fringe.el` then guards its standard-bitmap
        // seeding and `fringe-indicator-alist`/`fringe-cursor-alist` defaults on
        // `(boundp 'fringe-bitmaps)`, and `push`es each bitmap symbol onto it.
        // Binding it here lets fringe.el install those defaults in Lisp (GNU's
        // own path) instead of hardcoding the alists in Rust.
        obarray.set_symbol_value("fringe-bitmaps", Value::NIL);
        obarray.make_special("fringe-bitmaps");
        // File loading C variables (lread.c)
        // Process/daemon C variables (process.c)
        obarray.set_symbol_value("internal--daemon-sockname", Value::NIL);
        // Other missing C variables cus-start.el checks
        obarray.set_symbol_value("history-length", Value::fixnum(100));
        obarray.make_special("history-length");
        // minibuf.c:2538 DEFVAR_LISP, init Qt.
        obarray.define_special_variable("minibuffer-follows-selected-frame", Value::T);
        obarray.set_symbol_value("recenter-redisplay", Value::symbol("tty"));
        // frame.c:7733 DEFVAR_LISP, init Qiconify_top_level.
        obarray.define_special_variable("iconify-child-frame", Value::symbol("iconify-top-level"));
        // frame-inhibit-implied-resize is registered by
        // frame_vars::register_bootstrap_vars with GNU's GUI default.
        obarray.set_symbol_value("mark-even-if-inactive", Value::T);
        // minibuf.c:2533 DEFVAR_LISP, init nil.
        obarray.define_special_variable("read-buffer-function", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-prompt-properties",
            Value::list(vec![Value::symbol("read-only"), Value::T]),
        );
        obarray.set_symbol_value("help-event-list", Value::NIL);
        // GNU `keyboard.c:14127`:
        //   DEFVAR_LISP ("prefix-help-command", Vprefix_help_command, ...);
        //   Vprefix_help_command = intern_c_string ("describe-prefix-bindings");
        // The default is consulted by `read_key_sequence` when the
        // help-char fires after a prefix. Keyboard audit Finding 5
        // in `drafts/keyboard-command-loop-audit.md`.
        obarray.define_special_variable(
            "prefix-help-command",
            Value::symbol("describe-prefix-bindings"),
        );
        obarray.set_symbol_value("debug-ignored-errors", Value::NIL);
        // debug-on-event is registered (init sigusr2, keyboard.c:14358) by
        // keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("debug-on-signal", Value::NIL);
        // Remaining cus-start.el variables (general + platform names).
        // `temporary-file-directory' is not one of the platform names -- GNU
        // declares it in `filelock.c:814' for every build, with the same nil
        // init -- so it keeps its own seed here.
        obarray.set_symbol_value("temporary-file-directory", Value::NIL);
        // The 32 names whose C declaration belongs to a platform are a table
        // in `cus_start_platform_vars', and that table seeds NOTHING.  25 of
        // them are ones GNU leaves UNBOUND in a build like this one, so
        // seeding those made `boundp' disagree with GNU (entry 138); the other
        // 7 GNU declares with a `DEFVAR_LISP' that supplies a value AND the
        // `declared_special' bit, so a nil seed disagreed with GNU on both
        // (entry 141).  Each of the 7 is declared at the Neomacs counterpart
        // of its `syms_of_*', named in its table row.

        // GNU DEFVAR_LISP variables from lread.c that must be bound to nil
        // before any Elisp runs (code may test `boundp` or read them directly).
        //
        // Keep GNU's exception for `values`: `lread.c` defines it via
        // `DEFVAR_LISP` and then explicitly clears the declared-special bit,
        // so it remains an ordinary variable even under lexical binding.
        obarray.set_symbol_value("values", Value::NIL);
        obarray.set_symbol_value("eval-buffer-list", Value::NIL);
        obarray.make_special("eval-buffer-list");
        obarray.set_symbol_value("lread--unescaped-character-literals", Value::NIL);
        obarray.make_special("lread--unescaped-character-literals");
        obarray.set_symbol_value("load-read-function", Value::symbol("read"));
        obarray.make_special("load-read-function");
        obarray.set_symbol_value("load-source-file-function", Value::NIL);
        obarray.make_special("load-source-file-function");
        obarray.set_symbol_value("load-true-file-name", Value::NIL);
        obarray.make_special("load-true-file-name");
        obarray.set_symbol_value("user-init-file", Value::NIL);
        obarray.make_special("user-init-file");
        obarray.set_symbol_value("source-directory", Value::NIL);
        obarray.make_special("source-directory");
        obarray.set_symbol_value("after-load-alist", Value::NIL);
        obarray.make_special("after-load-alist");
        obarray.set_symbol_value("load-history", Value::NIL);
        obarray.make_special("load-history");
        obarray.set_symbol_value("current-load-list", Value::NIL);
        obarray.make_special("current-load-list");
        obarray.set_symbol_value("preloaded-file-list", Value::NIL);
        obarray.make_special("preloaded-file-list");
        // `Obarray::define_bool_variable` conses onto this list, the way GNU's
        // `defvar_bool` does (`src/lread.c:5261`), so only seed the empty list
        // when nothing has registered yet -- otherwise bootstrap ordering would
        // decide whether the registrations survive.
        if obarray
            .find_symbol_value(intern("byte-boolean-vars"))
            .is_none()
        {
            obarray.set_symbol_value("byte-boolean-vars", Value::NIL);
        }
        obarray.make_special("byte-boolean-vars");
        obarray.set_symbol_value(
            "bytecomp-version-regexp",
            Value::string(r#"^;;;.\(in Emacs version\|bytecomp version FSF\)"#),
        );
        obarray.make_special("bytecomp-version-regexp");
        obarray.set_symbol_value("load-path-filter-function", Value::NIL);
        obarray.make_special("load-path-filter-function");
        obarray.set_symbol_value("internal--get-default-lexical-binding-function", Value::NIL);
        obarray.make_special("internal--get-default-lexical-binding-function");
        obarray.set_symbol_value("read-symbol-shorthands", Value::NIL);
        obarray.make_special("read-symbol-shorthands");
        obarray.set_symbol_value("macroexp--dynvars", Value::NIL);
        obarray.make_special("macroexp--dynvars");
    }

    /// Reader, printer, keyboard, minibuffer, and display DEFVAR globals
    /// (GNU lread.c / print.c / keyboard.c / minibuf.c / xdisp.c
    /// syms_of_* territory).
    pub(super) fn seed_reader_keyboard_variables(
        obarray: &mut Obarray,
        standard_syntax_table: Value,
        minibuffer_local_map: Value,
    ) {
        obarray.set_symbol_value("inhibit-debugger", Value::NIL);
        obarray.make_special("inhibit-debugger");
        obarray.set_symbol_value("debug-on-error", Value::NIL);
        obarray.make_special("debug-on-error");
        obarray.set_symbol_value("debug-on-signal", Value::NIL);
        obarray.make_special("debug-on-signal");
        obarray.set_symbol_value("debug-ignored-errors", Value::NIL);
        obarray.make_special("debug-ignored-errors");
        obarray.define_int_variable("internal-when-entered-debugger", -1);
        obarray.set_symbol_value("signal-hook-function", Value::NIL);
        obarray.make_special("signal-hook-function");
        // GNU `eval.c` defines `internal-interpreter-environment` and then
        // immediately `Funintern`s that symbol, so Lisp-visible lookup sees a
        // separate ordinary symbol while the evaluator keeps a hidden special
        // variable for its own lexical-environment bookkeeping.
        obarray.set_symbol_value("internal-make-interpreted-closure-function", Value::NIL);
        obarray.make_special("internal-make-interpreted-closure-function");
        // GNU seeds `debugger` from eval.c before Lisp startup.
        // `eval-expression` relies on it.
        obarray.set_symbol_value("debugger", Value::symbol("debug-early"));
        obarray.make_special("debugger");
        obarray.set_symbol_value("standard-output", Value::T);
        // GNU DEFVAR_INT from dispnew.c — used by bytecomp.el
        // `src/dispnew.c:7488' DEFVAR_INT -- declared with NO initializer
        // beside it and no `init_*' that supplies one either, which is the
        // whole point: the C global lives in `globals' and starts at 0, and the
        // only things that
        // ever write it are `init_baud_rate' from `init_tty'
        // (`src/term.c:4755', `4923') and the `baud_rate = 19200' a window
        // system's terminal init does (`src/xterm.c:32279',
        // `src/pgtkterm.c:7034').  `--batch' creates no terminal, so GNU
        // reports 0 there.  `neomacs-bin' does those two assignments at the
        // same two places; this seed is the zero underneath them.
        obarray.define_int_variable("baud-rate", 0);
        obarray.set_symbol_value("search-slow-speed", Value::fixnum(1200));
        // GNU startup.el sets these based on --debug-init
        obarray.set_symbol_value("init-file-debug", Value::NIL);
        // `src/callproc.c:2240-2252' DEFVAR_INT: `sysconf (_SC_ARG_MAX) / 4'
        // where that is available, else 4096.  GNU divides by four "as a crude
        // way to go bytes->characters"; `multiple-command-partition-arguments'
        // is the caller.  Computed here rather than pinned to a constant for
        // the same reason GNU asks the C library: it is a property of the
        // machine, not of the editor.
        obarray.define_int_variable(
            "command-line-max-length",
            super::super::callproc::command_line_max_length(),
        );
        // GNU callproc.c: exec-path is built from PATH env var.
        // exec-directory is the directory containing helper programs.
        let exec_path: Vec<Value> = super::super::load::exec_path_dirs_from_env()
            .into_iter()
            .map(Value::unibyte_string)
            .collect();
        obarray.set_symbol_value("exec-path", Value::list(exec_path));
        obarray.make_special("exec-path");
        obarray.set_symbol_value(
            "exec-directory",
            Value::unibyte_string(
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "/usr/bin/".to_string()),
            ),
        );
        obarray.set_symbol_value(
            "exec-suffixes",
            Value::list(vec![Value::unibyte_string("")]),
        );
        obarray.make_special("exec-suffixes");
        obarray.set_symbol_value("buffer-read-only", Value::NIL);
        obarray.set_symbol_value("left-margin-width", Value::NIL);
        obarray.set_symbol_value("right-margin-width", Value::NIL);
        obarray.set_symbol_value("left-fringe-width", Value::NIL);
        obarray.set_symbol_value("right-fringe-width", Value::NIL);
        obarray.set_symbol_value("fringes-outside-margins", Value::NIL);
        obarray.set_symbol_value("scroll-bar-width", Value::NIL);
        obarray.set_symbol_value("scroll-bar-height", Value::NIL);
        obarray.set_symbol_value("vertical-scroll-bar", Value::T);
        obarray.set_symbol_value("horizontal-scroll-bar", Value::T);
        obarray.set_symbol_value("kill-ring", Value::NIL);
        obarray.set_symbol_value("kill-ring-yank-pointer", Value::NIL);
        obarray.set_symbol_value("last-command", Value::NIL);
        obarray.set_symbol_value("current-fill-column--has-warned", Value::NIL);
        obarray.set_symbol_value("current-input-method", Value::NIL);
        obarray.set_symbol_value("current-input-method-title", Value::NIL);
        // charset.c:2438 DEFVAR_LISP, init nil.
        obarray.define_special_variable("current-iso639-language", Value::NIL);
        // current-key-remap-sequence is registered by
        // keyboard::pure::register_bootstrap_vars.
        // GNU's `current-language-environment` defcustom defaults to "English"
        // (mule-cmds.el:1812), and the dumped image / `-Q` keeps it there.  This
        // value matters during loadup: `set-language-info` (mule-cmds.el:1181)
        // re-applies `set-charset-priority` whenever a language-info KEY is set
        // for the *current* language environment.  Seeding "UTF-8" here made
        // utf-8-lang.el's `(set-language-info-alist "UTF-8" ...)` reorder the
        // charset priority list at dump time (unicode-bmp/unicode to the front),
        // diverging from GNU's raw definition order.  Match GNU's default.
        obarray.set_symbol_value("current-language-environment", Value::string("English"));
        obarray.set_symbol_value(
            "current-load-list",
            Value::list(vec![
                Value::symbol("comp--no-native-compile"),
                Value::cons(
                    Value::symbol("defun"),
                    Value::symbol("load--fixup-all-elns"),
                ),
                Value::symbol("load--eln-dest-dir"),
                Value::symbol("load--bin-dest-dir"),
            ]),
        );
        obarray.set_symbol_value("current-locale-environment", Value::string("C.UTF-8"));
        obarray.set_symbol_value("current-minibuffer-command", Value::NIL);
        obarray.make_special("current-minibuffer-command");
        obarray.set_symbol_value("current-transient-input-method", Value::NIL);
        obarray.set_symbol_value("real-last-command", Value::NIL);
        // last-repeatable-command, this-original-command and defining-kbd-macro
        // are registered by keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("prefix-arg", Value::NIL);
        obarray.set_symbol_value("executing-kbd-macro", Value::NIL);
        obarray.make_special("executing-kbd-macro");
        obarray.define_int_variable("executing-kbd-macro-index", 0);
        obarray.define_c_hook_variable("kbd-macro-termination-hook");
        obarray.set_symbol_value("command-history", Value::NIL);
        obarray.make_special("command-history");
        obarray.set_symbol_value("extended-command-history", Value::NIL);
        obarray.set_symbol_value("read-file-name-completion-ignore-case", Value::NIL);
        obarray.make_special("read-file-name-completion-ignore-case");
        obarray.set_symbol_value("completion-regexp-list", Value::NIL);
        obarray.make_special("completion-regexp-list");
        obarray.set_symbol_value("completion--all-sorted-completions-location", Value::NIL);
        obarray.set_symbol_value("completion--capf-misbehave-funs", Value::NIL);
        obarray.set_symbol_value("completion--capf-safe-funs", Value::NIL);
        obarray.set_symbol_value(
            "completion--embedded-envvar-re",
            Value::string(
                "\\(?:^\\|[^$]\\(?:\\$\\$\\)*\\)\\$\\([[:alnum:]_]*\\|{\\([^}]*\\)\\)\\'",
            ),
        );
        obarray.set_symbol_value("completion--flex-score-last-md", Value::NIL);
        obarray.set_symbol_value("completion-all-sorted-completions", Value::NIL);
        obarray.set_symbol_value(
            "completion--cycling-threshold-type",
            Value::list(vec![Value::symbol("choice")]),
        );
        obarray.set_symbol_value(
            "completion--styles-type",
            Value::list(vec![Value::symbol("repeat")]),
        );
        obarray.set_symbol_value(
            "completion-at-point-functions",
            Value::list(vec![Value::symbol("tags-completion-at-point-function")]),
        );
        obarray.set_symbol_value(
            "completion-setup-hook",
            Value::list(vec![Value::symbol("completion-setup-function")]),
        );
        obarray.set_symbol_value("completion-list-mode-hook", Value::NIL);
        // completion-ignored-extensions is a dired.c DEFVAR_LISP; see
        // `dired::register_bootstrap_vars' below.
        obarray.set_symbol_value(
            "completion-styles",
            Value::list(vec![
                Value::symbol("basic"),
                Value::symbol("partial-completion"),
                Value::symbol("emacs22"),
            ]),
        );
        obarray.set_symbol_value(
            "completion-category-defaults",
            Value::list(vec![
                Value::list(vec![
                    Value::symbol("buffer"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("substring"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("unicode-name"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("substring"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("project-file"),
                    Value::list(vec![Value::symbol("styles"), Value::symbol("substring")]),
                ]),
                Value::list(vec![
                    Value::symbol("xref-location"),
                    Value::list(vec![Value::symbol("styles"), Value::symbol("substring")]),
                ]),
                Value::list(vec![
                    Value::symbol("info-menu"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("substring"),
                    ]),
                ]),
                Value::list(vec![
                    Value::symbol("symbol-help"),
                    Value::list(vec![
                        Value::symbol("styles"),
                        Value::symbol("basic"),
                        Value::symbol("shorthand"),
                        Value::symbol("substring"),
                    ]),
                ]),
                // NB: GNU's `completion-category-defaults' defvar
                // (lisp/minibuffer.el) ends at `symbol-help'.  The
                // `calendar-month' entry is added at runtime by calendar.el's
                // `add-to-list', which is not loaded under `emacs -Q'; don't
                // hardcode it here.
            ]),
        );
        // Do NOT hardcode completion-styles-alist here.
        // GNU defines it via (defvar completion-styles-alist ...)
        // in lisp/minibuffer.el:1158 with all 8 styles including
        // flex, substring, initials, shorthand. defvar only sets
        // the value when the symbol is void, so pre-setting it
        // here would shadow the Lisp definition and lose styles
        // like flex — breaking fido-vertical-mode which requires
        // the flex completion style.
        obarray.set_symbol_value("completion-category-overrides", Value::NIL);
        obarray.set_symbol_value("completion-cycle-threshold", Value::NIL);
        obarray.set_symbol_value("completions-detailed", Value::NIL);
        obarray.set_symbol_value("completions-format", Value::symbol("horizontal"));
        obarray.set_symbol_value("completions-group", Value::NIL);
        obarray.set_symbol_value("completions-group-format", Value::string("     %s  "));
        obarray.set_symbol_value("completions-group-sort", Value::NIL);
        obarray.set_symbol_value(
            "completions-header-format",
            Value::string("%s possible completions:\n"),
        );
        obarray.set_symbol_value(
            "completions-highlight-face",
            Value::symbol("completions-highlight"),
        );
        obarray.set_symbol_value("completions-max-height", Value::NIL);
        obarray.set_symbol_value("completions-sort", Value::symbol("alphabetical"));
        obarray.set_symbol_value("completion-auto-help", Value::T);
        obarray.set_symbol_value("completion-auto-deselect", Value::T);
        obarray.set_symbol_value("completion-auto-select", Value::NIL);
        obarray.set_symbol_value("completion-auto-wrap", Value::T);
        obarray.set_symbol_value("completion-base-position", Value::NIL);
        obarray.set_symbol_value("completion-cycling", Value::NIL);
        obarray.set_symbol_value("completion-extra-properties", Value::NIL);
        obarray.set_symbol_value("completion-fail-discreetly", Value::NIL);
        obarray.set_symbol_value("completion-flex-nospace", Value::NIL);
        obarray.set_symbol_value("completion-in-region--data", Value::NIL);
        obarray.set_symbol_value(
            "completion-in-region-function",
            Value::symbol("completion--in-region"),
        );
        obarray.set_symbol_value("completion-in-region-functions", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode--predicate", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode-hook", Value::NIL);
        obarray.set_symbol_value("completion-in-region-mode-predicate", Value::NIL);
        obarray.set_symbol_value("completion-show-help", Value::T);
        obarray.set_symbol_value("completion-show-inline-help", Value::T);
        obarray.set_symbol_value("completion-lazy-hilit", Value::NIL);
        obarray.set_symbol_value("completion-lazy-hilit-fn", Value::NIL);
        obarray.set_symbol_value(
            "completion-list-insert-choice-function",
            Value::symbol("completion--replace"),
        );
        obarray.set_symbol_value("completion-no-auto-exit", Value::NIL);
        obarray.set_symbol_value(
            "completion-pcm--delim-wild-regex",
            Value::string("[-_./:| *]"),
        );
        obarray.set_symbol_value("completion-pcm--regexp", Value::NIL);
        obarray.set_symbol_value(
            "completion-pcm-complete-word-inserts-delimiters",
            Value::NIL,
        );
        obarray.set_symbol_value("completion-pcm-word-delimiters", Value::string("-_./:| "));
        obarray.set_symbol_value("completion-reference-buffer", Value::NIL);
        obarray.set_symbol_value("completion-tab-width", Value::NIL);
        obarray.set_symbol_value("history-length", Value::fixnum(100));
        obarray.make_special("history-length");
        obarray.set_symbol_value("history-add-new-input", Value::T);
        obarray.make_special("history-add-new-input");
        // read-buffer-function is registered above (minibuf.c:2533).
        obarray.set_symbol_value(
            "read-file-name-function",
            Value::symbol("read-file-name-default"),
        );
        // minibuf.c:2528 DEFVAR_LISP, init nil.
        obarray.define_special_variable("read-expression-history", Value::NIL);
        obarray.set_symbol_value("read-number-history", Value::NIL);
        obarray.set_symbol_value("read-char-history", Value::NIL);
        obarray.set_symbol_value("read-answer-short", Value::symbol("auto"));
        obarray.set_symbol_value("read-char-by-name-sort", Value::NIL);
        obarray.set_symbol_value("read-char-choice-use-read-key", Value::NIL);
        obarray.set_symbol_value("read-circle", Value::T);
        obarray.make_special("read-circle");
        obarray.set_symbol_value("read-envvar-name-history", Value::NIL);
        obarray.set_symbol_value("read-face-name-sample-text", Value::string("SAMPLE"));
        obarray.set_symbol_value("read-key-delay", Value::make_float(0.01));
        obarray.set_symbol_value(
            "read-answer-map--memoize",
            Value::hash_table(HashTableTest::Equal),
        );
        obarray.set_symbol_value("read-extended-command-mode", Value::NIL);
        obarray.set_symbol_value("read-extended-command-mode-hook", Value::NIL);
        obarray.set_symbol_value("read-extended-command-predicate", Value::NIL);
        obarray.set_symbol_value("read-hide-char", Value::NIL);
        obarray.set_symbol_value("read-mail-command", Value::symbol("rmail"));
        obarray.set_symbol_value("read-only-mode-hook", Value::NIL);
        obarray.define_int_variable("read-process-output-max", 65536);
        obarray.set_symbol_value("read-quoted-char-radix", Value::fixnum(8));
        obarray.set_symbol_value("read-regexp--case-fold", Value::NIL);
        obarray.set_symbol_value("read-regexp-defaults-function", Value::NIL);
        obarray.set_symbol_value("read-symbol-shorthands", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-frame-alist",
            Value::list(vec![
                Value::cons(Value::symbol("width"), Value::fixnum(80)),
                Value::cons(Value::symbol("height"), Value::fixnum(2)),
            ]),
        );
        obarray.set_symbol_value("minibuffer-inactive-mode-hook", Value::NIL);
        obarray.set_symbol_value("minibuffer-mode-hook", Value::NIL);
        obarray.set_symbol_value("minibuffer-local-map", minibuffer_local_map);
        obarray.set_symbol_value("minibuffer-local-filename-syntax", standard_syntax_table);
        obarray.set_symbol_value("minibuffer-history", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-history-variable",
            Value::symbol("minibuffer-history"),
        );
        obarray.set_symbol_value("minibuffer-history-position", Value::NIL);
        obarray.set_symbol_value("minibuffer-history-isearch-message-overlay", Value::NIL);
        obarray.set_symbol_value("minibuffer-history-search-history", Value::NIL);
        obarray.set_symbol_value("minibuffer-history-sexp-flag", Value::NIL);
        obarray.set_symbol_value("minibuffer-default", Value::NIL);
        obarray.set_symbol_value("minibuffer-default-add-done", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-default-add-function",
            Value::symbol("minibuffer-default-add-completions"),
        );
        obarray.set_symbol_value("minibuffer--original-buffer", Value::NIL);
        obarray.set_symbol_value("minibuffer--regexp-primed", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer--regexp-prompt-regexp",
            Value::string(
                "\\(?:Posix search\\|RE search\\|Search for regexp\\|Query replace regexp\\)",
            ),
        );
        obarray.set_symbol_value("minibuffer--require-match", Value::NIL);
        // minibuffer-follows-selected-frame is registered earlier in bootstrap.
        // GNU src/minibuf.c:2557-2559 DEFVARs this hook and sets it to Qnil.
        // minibuffer.el's `minibuffer--regexp-exit', `minibuffer--nonselected-exit'
        // and `minibuffer-exit-on-screen-keyboard', plus `minibuffer-restore-windows',
        // are all put here by `add-hook' while loadup runs.
        obarray.define_c_hook_variable("minibuffer-exit-hook");
        obarray.set_symbol_value("minibuffer-completion-table", Value::NIL);
        obarray.set_symbol_value("minibuffer-completion-predicate", Value::NIL);
        obarray.set_symbol_value("minibuffer-completion-confirm", Value::NIL);
        // `minibuffer-completion-auto-choose` belongs to minibuffer.el.  Do
        // not pre-bind it here: `defcustom` preserves an existing value, so a
        // Rust seed would override GNU Emacs's Lisp default.
        obarray.set_symbol_value("minibuffer-completion-base", Value::NIL);
        obarray.set_symbol_value("minibuffer-help-form", Value::NIL);
        obarray.set_symbol_value("minibuffer-completing-file-name", Value::NIL);
        // `minibuffer-regexp-mode` belongs to lisp/minibuffer.el:5641, a global
        // `define-minor-mode` whose `defcustom` is initialized by
        // `custom-initialize-after-file-load`.  That initializer ends in
        // `custom-initialize-set` (lisp/custom.el:68-82), which returns without
        // doing anything when the symbol already has a default top-level value.
        // A Rust seed here does not merely duplicate the Lisp default: it
        // suppresses the `:set` function, so the mode body never runs and the
        // mode never installs `minibuffer--regexp-setup` /
        // `minibuffer--regexp-exit`, while the variable still reads t.
        obarray.set_symbol_value("minibuffer-regexp-mode-hook", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-regexp-prompts",
            Value::list(vec![
                Value::string("Posix search"),
                Value::string("RE search"),
                Value::string("Search for regexp"),
                Value::string("Query replace regexp"),
            ]),
        );
        obarray.set_symbol_value("minibuffer-message-clear-timeout", Value::NIL);
        obarray.set_symbol_value("minibuffer-message-overlay", Value::NIL);
        obarray.set_symbol_value("minibuffer-message-properties", Value::NIL);
        // minibuffer-message-timeout is registered by
        // keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("minibuffer-message-timer", Value::NIL);
        obarray.set_symbol_value("minibuffer-lazy-count-format", Value::string("%s "));
        obarray.set_symbol_value("minibuffer-text-before-history", Value::NIL);
        // GNU src/minibuf.c declares these with DEFVAR_LISP/DEFVAR_BOOL.
        // They must be special so lexical-binding Lisp sees dynamic
        // minibuffer/completion bindings inside byte-compiled functions.
        for name in [
            "minibuffer-auto-raise",
            "minibuffer-completion-table",
            "minibuffer-completion-predicate",
            "minibuffer-completion-confirm",
            "minibuffer-completing-file-name",
            "minibuffer-help-form",
            "minibuffer-history-variable",
            "minibuffer-history-position",
            "minibuffer-allow-text-properties",
            "minibuffer-prompt-properties",
            "read-hide-char",
            "inhibit-interaction",
            "read-minibuffer-restore-windows",
        ] {
            obarray.make_special(name);
        }
        obarray.set_symbol_value(
            "minibuffer-prompt-properties",
            Value::list(vec![Value::symbol("read-only"), Value::T]),
        );
        obarray.set_symbol_value("minibuffer-scroll-window", Value::NIL);
        obarray.make_special("minibuffer-scroll-window");
        obarray.set_symbol_value("other-window-scroll-buffer", Value::NIL);
        obarray.make_special("other-window-scroll-buffer");
        obarray.set_symbol_value("other-window-scroll-default", Value::NIL);
        obarray.make_special("other-window-scroll-default");
        obarray.set_symbol_value("minibuffer-visible-completions", Value::NIL);
        obarray.set_symbol_value("minibuffer-visible-completions--always-bind", Value::NIL);
        obarray.set_symbol_value("minibuffer-depth-indicate-mode", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-default-prompt-format",
            Value::string(" (default %s)"),
        );
        obarray.set_symbol_value("minibuffer-beginning-of-buffer-movement", Value::NIL);
        obarray.set_symbol_value("minibuffer-electric-default-mode", Value::NIL);
        obarray.set_symbol_value("minibuffer-temporary-goal-position", Value::NIL);
        obarray.set_symbol_value(
            "minibuffer-confirm-exit-commands",
            Value::list(vec![
                Value::symbol("completion-at-point"),
                Value::symbol("minibuffer-complete"),
                Value::symbol("minibuffer-complete-word"),
            ]),
        );
        obarray.set_symbol_value("minibuffer-history-case-insensitive-variables", Value::NIL);
        obarray.set_symbol_value("minibuffer-on-screen-keyboard-displayed", Value::NIL);
        obarray.set_symbol_value("minibuffer-on-screen-keyboard-timer", Value::NIL);
        // GNU src/minibuf.c:2553-2555 DEFVARs this hook and sets it to Qnil.
        // rfn-eshadow.el, minibuffer.el and simple.el `add-hook' their entries
        // onto it while loadup runs, and `add-hook' conses onto the front, so
        // the resulting order is a record of that preload order.
        obarray.define_c_hook_variable("minibuffer-setup-hook");
        obarray.set_symbol_value("regexp-search-ring", Value::NIL);
        obarray.set_symbol_value("regexp-search-ring-max", Value::fixnum(16));
        obarray.set_symbol_value("regexp-search-ring-yank-pointer", Value::NIL);
        obarray.set_symbol_value("search-ring", Value::NIL);
        obarray.set_symbol_value("search-ring-max", Value::fixnum(16));
        obarray.set_symbol_value("search-ring-update", Value::NIL);
        obarray.set_symbol_value("search-ring-yank-pointer", Value::NIL);
        obarray.set_symbol_value("last-abbrev", Value::NIL);
        obarray.set_symbol_value("last-abbrev-location", Value::fixnum(0));
        obarray.set_symbol_value("last-abbrev-text", Value::NIL);
        obarray.set_symbol_value("last-command-event", Value::NIL);
        // last-event-frame, last-event-device, last-nonmenu-event and
        // last-kbd-macro are registered by keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("last-input-event", Value::NIL);
        obarray.set_symbol_value("last-prefix-arg", Value::NIL);
        obarray.set_symbol_value("last-code-conversion-error", Value::NIL);
        obarray.set_symbol_value("last-coding-system-specified", Value::NIL);
        obarray.set_symbol_value("last-coding-system-used", Value::symbol("undecided-unix"));
        obarray.set_symbol_value("last-next-selection-coding-system", Value::NIL);
        obarray.set_symbol_value("command-debug-status", Value::NIL);
        obarray.make_special("command-debug-status");
        obarray.set_symbol_value(
            "command-error-function",
            Value::symbol("help-command-error-confusable-suggestions"),
        );
        obarray.set_symbol_value("key-substitution-in-progress", Value::NIL);
        obarray.set_symbol_value("this-command", Value::NIL);
        obarray.set_symbol_value("real-this-command", Value::NIL);
        obarray.set_symbol_value("this-command-keys-shift-translated", Value::NIL);
        obarray.set_symbol_value("current-prefix-arg", Value::NIL);
        obarray.set_symbol_value("track-mouse", Value::NIL);
        obarray.make_special("track-mouse");
        obarray.set_symbol_value(
            "while-no-input-ignore-events",
            // GNU's `init_while_no_input_ignore_events'
            // (src/keyboard.c:13315-13336) builds an eleven-name base list
            // UNCONDITIONALLY, then conses `dbus-event' on under
            // `#ifdef HAVE_DBUS', `file-notify' under `USE_FILE_NOTIFY',
            // `thread-event' under `THREADS_ENABLED', and `sleep-event'
            // unconditionally last.  This build has the file-notify and
            // threads options and not HAVE_DBUS (ledger 192), so it is GNU's
            // list minus exactly `dbus-event'.  `sleep-event' and
            // `toolkit-theme-changed' were missing from the base list here and
            // are guarded by nothing in GNU; both restored, ledger 192.
            Value::list(vec![
                Value::symbol("sleep-event"),
                Value::symbol("thread-event"),
                Value::symbol("file-notify"),
                Value::symbol("select-window"),
                Value::symbol("help-echo"),
                Value::symbol("move-frame"),
                Value::symbol("iconify-frame"),
                Value::symbol("make-frame-visible"),
                Value::symbol("focus-in"),
                Value::symbol("focus-out"),
                Value::symbol("config-changed-event"),
                Value::symbol("selection-request"),
                Value::symbol("monitors-changed"),
                Value::symbol("toolkit-theme-changed"),
            ]),
        );
        obarray.make_special("while-no-input-ignore-events");
        // GNU's nine `dbusbind.c' DEFVARs -- `dbus-message-type-*',
        // `dbus-debug', `dbus-compiled-version', `dbus-runtime-version',
        // `dbus-registered-objects-table' (src/dbusbind.c:2069-2159) -- are
        // inside `#ifdef HAVE_DBUS' with the rest of the file, and this build
        // has no D-Bus transport, so it declares none of them (ledger 192).
        // `lisp/net/dbus.el:40-46' declares them bare for exactly this build,
        // and `:53' supplies `dbus-debug'.
        obarray.set_symbol_value("deactivate-mark", Value::NIL);
        obarray.make_special("deactivate-mark");
        obarray.make_buffer_local("deactivate-mark", true);
        // GNU `keyboard.c` DEFVARs.  `command_loop_1` resets
        // `disable-point-adjustment` to nil before each command; commands may
        // set it non-nil to suppress the post-command `adjust_point_for_property`
        // step.  `global-disable-point-adjustment` is the permanent override.
        obarray.set_symbol_value("disable-point-adjustment", Value::NIL);
        obarray.make_special("disable-point-adjustment");
        obarray.set_symbol_value("global-disable-point-adjustment", Value::NIL);
        obarray.make_special("global-disable-point-adjustment");
        obarray.set_symbol_value("mark-active", Value::NIL);
        obarray.set_symbol_value("mark-even-if-inactive", Value::T);
        obarray.make_special("mark-even-if-inactive");
        obarray.set_symbol_value("mark-ring", Value::NIL);
        obarray.set_symbol_value("mark-ring-max", Value::fixnum(16));
        // saved-region-selection is set by keyboard::pure::register_bootstrap_vars
        obarray.set_symbol_value("transient-mark-mode", Value::NIL);
        obarray.set_symbol_value("transient-mark-mode-hook", Value::NIL);
        // post-select-region-hook and display-monitors-changed-functions are
        // registered by keyboard::pure::register_bootstrap_vars.
        obarray.set_symbol_value("echo-area-clear-hook", Value::NIL);
        // terminal.c:700 / term.c:5233 / term.c:5240 DEFVAR_LISP, init nil.
        obarray.define_c_hook_variable("delete-terminal-functions");
        obarray.define_c_hook_variable("suspend-tty-functions");
        obarray.define_c_hook_variable("resume-tty-functions");
        obarray.set_symbol_value("overriding-local-map", Value::NIL);
        obarray.make_special("overriding-local-map");
        obarray.set_symbol_value("overriding-local-map-menu-flag", Value::NIL);
        obarray.make_special("overriding-local-map-menu-flag");
        obarray.set_symbol_value("overriding-plist-environment", Value::NIL);
        obarray.make_special("overriding-plist-environment");
        obarray.set_symbol_value("overriding-terminal-local-map", Value::NIL);
        // GNU uses DEFVAR_KBOARD here. NeoVM does not yet split keyboard state
        // per terminal, so model it as a dynamically scoped runtime variable.
        obarray.make_special("overriding-terminal-local-map");
        // textconv.c:2621 DEFVAR_LISP, init Qlambda.
        obarray
            .define_special_variable("overriding-text-conversion-style", Value::symbol("lambda"));
    }

    /// Core eval.c / keyboard.c DEFVAR globals plus the standard error
    /// hierarchy and indentation/font variable seeding.
    pub(super) fn seed_core_eval_variables(obarray: &mut Obarray) {
        // Core eval variables (stay in eval.rs)
        obarray.set_symbol_value("purify-flag", Value::NIL);
        obarray.make_special("purify-flag");
        obarray.define_int_variable("max-lisp-eval-depth", 1600);
        obarray.define_int_variable("lisp-eval-depth-reserve", 200);

        // Terminal/display variables (C-level DEFVAR in official Emacs)
        // `standard-display-table' is a DEFVAR_LISP in dispnew.c (default nil),
        // hence special: `(let ((standard-display-table ...)) ...)' must bind it
        // dynamically so the `standard-display-*' functions (disp-table.el) see
        // and mutate the binding instead of the global default.
        obarray.set_symbol_value("standard-display-table", Value::NIL);
        obarray.make_special("standard-display-table");
        // `glyph-table' is a DEFVAR_LISP in dispnew.c, default nil. It must be
        // bound (and special) so `boundp'/`special-variable-p' agree with GNU.
        obarray.set_symbol_value("glyph-table", Value::NIL);
        obarray.make_special("glyph-table");
        obarray.set_symbol_value(
            "image-load-path",
            Value::list(vec![
                Value::string("/usr/share/emacs/30.1/etc/images/"),
                Value::symbol("data-directory"),
            ]),
        );
        // `image-types' and `image-scaling-factor' are registered by
        // image::register_bootstrap_vars, GNU's `syms_of_image'.

        // User init / startup (C DEFVAR in official Emacs)
        obarray.set_symbol_value("user-init-file", Value::NIL);
        obarray.set_symbol_value("user-emacs-directory", Value::string("~/.emacs.d/"));

        // Frame parameters (C DEFVAR in official Emacs)
        obarray.set_symbol_value("frame--special-parameters", Value::NIL);

        // Initialize distributed bootstrap variables.
        //
        // GNU's `DEFVAR_BOOL' table comes first, for the reason `main' runs
        // every `syms_of_*' before Lisp: `Fmake_variable_buffer_local' copies
        // the symbol's forwarder into the BLV (`src/data.c:2112-2140'), so a
        // variable that is going to be localized below -- `indent-tabs-mode',
        // `display-fill-column-indicator', `display-line-numbers-widen' --
        // has to be forwarded before that happens or the coercion is dropped.
        super::super::defvar_bool::register_bootstrap_vars(obarray);
        super::super::alloc::register_bootstrap_vars(obarray);
        super::super::load::register_bootstrap_vars(obarray);
        super::super::fileio::register_bootstrap_vars(obarray);
        super::super::process::register_bootstrap_vars(obarray);
        super::super::undo::register_bootstrap_vars(obarray);
        super::super::category::register_bootstrap_vars(obarray);
        super::super::window_cmds::register_bootstrap_vars(obarray);
        super::super::keyboard::pure::register_bootstrap_vars(obarray);
        super::super::composite::register_bootstrap_vars(obarray);
        super::super::coding::register_bootstrap_vars(obarray);
        super::super::dired::register_bootstrap_vars(obarray);
        super::super::xdisp::register_bootstrap_vars(obarray);
        super::super::textprop::register_bootstrap_vars(obarray);
        super::super::xfaces::register_bootstrap_vars(obarray);
        super::super::frame_vars::register_bootstrap_vars(obarray);
        super::super::buffer_vars::register_bootstrap_vars(obarray);
        super::super::image::register_bootstrap_vars(obarray);
        super::super::fontset::register_bootstrap_vars(obarray);

        // ---- end C-level bootstrap variables ----

        obarray.set_symbol_value("unread-input-method-events", Value::NIL);
        obarray.set_symbol_value("unread-post-input-method-events", Value::NIL);
        obarray.set_symbol_value("input-method-alist", Value::NIL);
        obarray.set_symbol_value("input-method-activate-hook", Value::NIL);
        obarray.set_symbol_value("input-method-after-insert-chunk-hook", Value::NIL);
        obarray.set_symbol_value("input-method-deactivate-hook", Value::NIL);
        obarray.set_symbol_value("input-method-exit-on-first-char", Value::NIL);
        obarray.set_symbol_value("input-method-exit-on-invalid-key", Value::NIL);
        // GNU `src/keyboard.c` initializes this DEFVAR_LISP to Qlist.
        obarray.set_symbol_value("input-method-function", Value::symbol("list"));
        obarray.make_special("input-method-function");
        obarray.set_symbol_value("input-method-highlight-flag", Value::T);
        obarray.set_symbol_value("input-method-history", Value::NIL);
        // input-method-previous-message is set by keyboard::pure::register_bootstrap_vars
        obarray.set_symbol_value("input-method-use-echo-area", Value::NIL);
        obarray.set_symbol_value("input-method-verbose-flag", Value::symbol("default"));
        obarray.set_symbol_value("unread-command-events", Value::NIL);
        // No `variable-documentation` is seeded here, and ledger 178 is why.
        //
        // This used to write one for all 1972 names of two hand-typed tables
        // in `doc.rs`, under the comment "GNU Emacs seeds core startup vars
        // with integer `variable-documentation` offsets in the DOC table".
        // GNU does no such thing.  Every `variable-documentation` GNU installs
        // is downstream of the variable existing: `Fsnarf_documentation` puts
        // one on a name this build BINDS (`src/doc.c:606-613`, where the
        // `Fput` is the entire branch), Lisp `defvar` puts one on a name it is
        // defining and only when the docstring is non-nil (`src/eval.c:911`),
        // and `Fdefvaralias` copies one across an alias edge
        // (`src/eval.c:723`).  There is no fourth writer and nothing runs
        // before the variable is there.
        //
        // The 70 offset rows made the point a second time by seeding
        // `(fixnum 0)`, which is precisely the value GNU reserves to mean
        // "there is no doc" -- `if (BASE_EQ (tem, make_fixnum (0))) tem =
        // Qnil;` (`src/doc.c:433-434`) -- and which `make-docfile` can never
        // emit, the smallest real offset being `end + 1 - buf`.
        //
        // A seeded row landed on the symbol's plist, which is the FIRST arm
        // `documentation_property_plan` consults, so it answered ahead of
        // `Fsnarf_documentation`'s `Fboundp` gate: 35 unbound names carried a
        // doc where GNU carries none.  Measured GNU 31.0.90 `-Q --batch`:
        // 18815 symbols, zero unbound-yet-documented, zero holding the
        // reserved `0`.  `no_unbound_symbol_carries_a_variable_documentation`
        // is the guard.
        // Bootstrap primitive function cells that GNU `simple.el` references
        // before its own Elisp defs overwrite them. Without these placeholders,
        // loaded GNU bytecode can capture `nil` for forward/runtime calls into
        // Builtin function cells are set by SubrSpec registration during init_builtins().
        for name in ["mark-marker", "region-beginning", "region-end"] {
            obarray.set_symbol_function(name, Value::subr_from_sym_id(intern(name)));
        }

        // `word-at-point` is defined in GNU Emacs Lisp by `thingatpt.el`,
        // not as a startup builtin.
        obarray.clear_function_silent("word-at-point");

        // Mark standard variables as special (dynamically bound)
        for name in &[
            "debug-on-error",
            "debugger",
            // "lexical-binding" is registered below like GNU lread.c:
            // DEFVAR_LISP plus make-variable-buffer-local.
            "load-prefer-newer",
            "load-path",
            "load-history",
            "default-directory",
            "load-file-name",
            "set-auto-coding-for-load",
            "noninteractive",
            "inhibit-quit",
            "inhibit-read-only",
            "inhibit-modification-hooks",
            "internal-make-interpreted-closure-function",
            "print-length",
            "print-level",
            "standard-output",
            "case-fold-search",
            "buffer-read-only",
            "current-prefix-arg",
            "prefix-arg",
            "last-prefix-arg",
            "last-command-event",
            "last-input-event",
            "last-command",
            "real-last-command",
            "this-command",
            "real-this-command",
            "this-command-keys-shift-translated",
            "unread-command-events",
            "unread-input-method-events",
            "unread-post-input-method-events",
            // transient-mark-mode is a C-level variable in GNU (buffer.c),
            // always dynamically scoped. Must be special so (let ((transient-mark-mode t)) ...)
            // creates a dynamic binding visible to called functions like region-active-p.
            "transient-mark-mode",
        ] {
            obarray.make_special(name);
        }

        // Initialize the standard error hierarchy (error, user-error, etc.)
        super::super::errors::init_standard_errors(obarray);

        // Initialize indentation variables (tab-width, indent-tabs-mode, etc.)
        super::super::indent::init_indent_vars(obarray);
        super::super::font::init_font_vars(obarray);
    }

    /// C-level DEFVAR registrations mirroring GNU's per-file syms_of_*()
    /// functions, plus buffer-local bootstrap variables. If a variable is
    /// declared via DEFVAR in GNU C, it must be registered here or elisp
    /// reading or let-binding it gets void-variable.
    pub(super) fn seed_c_level_defvars(obarray: &mut Obarray, custom: &mut CustomManager) {
        // `case-fold-search` is DEFVAR_LISP + Fmake_variable_buffer_local
        // in GNU `buffer.c:5971-5975`. Install it as a LOCALIZED symbol
        // with `local_if_set = 1` at init time so reads/writes route
        // through the BLV + local_var_alist path instead of the legacy
        // `BufferLocals::lisp_bindings` fallback. Default is `t`.
        {
            let id = crate::emacs_core::intern::intern("case-fold-search");
            obarray.set_symbol_value("case-fold-search", Value::T);
            obarray.make_symbol_localized(id, Value::T);
            obarray.set_blv_local_if_set(id, true);
        }

        // `indent-tabs-mode` is DEFVAR_BOOL + make-variable-buffer-local
        // (bindings.el:1032). GNU's DEFVAR_BOOL installs a C-backed
        // forwarder; NeoMacs stores it as a plain Lisp value and
        // then hoists it to LOCALIZED at init. Default is `t`
        // (matches `init_indent_vars`).
        {
            let id = crate::emacs_core::intern::intern("indent-tabs-mode");
            obarray.make_symbol_localized(id, Value::T);
            obarray.set_blv_local_if_set(id, true);
        }

        super::super::textprop::init_textprop_vars(obarray, custom);
        super::super::syntax::init_syntax_vars(obarray, custom);
        // Register all DEFVAR_PER_BUFFER variables from GNU Emacs buffer.c.
        // These are C-level buffer-local variables that must exist before
        // any .el file loads.  Default values match init_buffer_once().
        macro_rules! defvar_per_buffer {
            ($name:expr, $val:expr) => {
                obarray.make_special($name);
                obarray.set_symbol_value($name, $val);
            };
        }
        {
            // Core buffer identity
            defvar_per_buffer!("buffer-file-name", Value::NIL);
            defvar_per_buffer!("buffer-file-truename", Value::NIL);
            // GNU buffer.c:5381 — default-directory defaults to cwd.
            // This sets the GLOBAL default; new buffers inherit it.
            {
                let cwd = std::env::current_dir()
                    .map(|p| {
                        let mut s = p.to_string_lossy().into_owned();
                        if !s.ends_with('/') {
                            s.push('/');
                        }
                        s
                    })
                    .unwrap_or_else(|_| "/".to_string());
                // GNU Emacs uses make_unibyte_string for default-directory
                // because the locale isn't set up yet during dump.  loadup.el
                // checks (multibyte-string-p default-directory) and errors
                // if it's multibyte.
                defvar_per_buffer!("default-directory", Value::unibyte_string(cwd));
            }
            defvar_per_buffer!("buffer-read-only", Value::NIL);
            defvar_per_buffer!("buffer-undo-list", Value::NIL);
            defvar_per_buffer!("buffer-saved-size", Value::fixnum(0));
            defvar_per_buffer!("buffer-backed-up", Value::NIL);
            defvar_per_buffer!("buffer-file-format", Value::NIL);
            defvar_per_buffer!("buffer-auto-save-file-name", Value::NIL);
            defvar_per_buffer!("buffer-auto-save-file-format", Value::T);
            defvar_per_buffer!("buffer-file-coding-system", Value::NIL);
            defvar_per_buffer!("buffer-display-count", Value::fixnum(0));
            defvar_per_buffer!("buffer-display-time", Value::NIL);

            // Modes
            defvar_per_buffer!("major-mode", Value::symbol("fundamental-mode"));
            defvar_per_buffer!("mode-name", Value::NIL);
            defvar_per_buffer!("mode-line-format", Value::string("%-"));
            defvar_per_buffer!("header-line-format", Value::NIL);
            defvar_per_buffer!("tab-line-format", Value::NIL);
            defvar_per_buffer!("local-abbrev-table", Value::NIL);
            defvar_per_buffer!("local-minor-modes", Value::NIL);
            defvar_per_buffer!("abbrev-mode", Value::NIL);
            defvar_per_buffer!("overwrite-mode", Value::NIL);
            defvar_per_buffer!("auto-fill-function", Value::NIL);

            // Search (GNU buffer.c DEFVAR_PER_BUFFER)
            defvar_per_buffer!("case-fold-search", Value::T);
            defvar_per_buffer!("indent-tabs-mode", Value::T);

            // Display
            defvar_per_buffer!("tab-width", Value::fixnum(8));
            defvar_per_buffer!("fill-column", Value::fixnum(70));
            defvar_per_buffer!("left-margin", Value::fixnum(0));
            defvar_per_buffer!("truncate-lines", Value::NIL);
            defvar_per_buffer!("word-wrap", Value::NIL);
            defvar_per_buffer!("ctl-arrow", Value::T);
            defvar_per_buffer!("selective-display", Value::NIL);
            defvar_per_buffer!("selective-display-ellipses", Value::T);
            defvar_per_buffer!("enable-multibyte-characters", Value::T);
            defvar_per_buffer!("buffer-display-table", Value::NIL);
            defvar_per_buffer!("buffer-invisibility-spec", Value::NIL);
            defvar_per_buffer!("line-spacing", Value::NIL);
            defvar_per_buffer!("cache-long-scans", Value::T);
            defvar_per_buffer!("point-before-scroll", Value::NIL);

            // Cursor
            defvar_per_buffer!("cursor-type", Value::T);
            defvar_per_buffer!("neomacs-cursor-effect", Value::NIL);
            defvar_per_buffer!("cursor-in-non-selected-windows", Value::T);

            // Marks
            defvar_per_buffer!("mark-active", Value::NIL);

            // Bidi
            defvar_per_buffer!("bidi-display-reordering", Value::T);
            defvar_per_buffer!("bidi-paragraph-direction", Value::NIL);
            defvar_per_buffer!("bidi-paragraph-start-re", Value::NIL);
            defvar_per_buffer!("bidi-paragraph-separate-re", Value::NIL);

            // Fringes and margins
            defvar_per_buffer!("left-fringe-width", Value::NIL);
            defvar_per_buffer!("right-fringe-width", Value::NIL);
            defvar_per_buffer!("left-margin-width", Value::fixnum(0));
            defvar_per_buffer!("right-margin-width", Value::fixnum(0));
            defvar_per_buffer!("fringes-outside-margins", Value::NIL);
            defvar_per_buffer!("fringe-indicator-alist", Value::NIL);
            defvar_per_buffer!("fringe-cursor-alist", Value::NIL);
            defvar_per_buffer!("indicate-empty-lines", Value::NIL);
            defvar_per_buffer!("indicate-buffer-boundaries", Value::NIL);

            // Scroll bars
            defvar_per_buffer!("scroll-bar-width", Value::NIL);
            defvar_per_buffer!("scroll-bar-height", Value::NIL);
            defvar_per_buffer!("vertical-scroll-bar", Value::T);
            defvar_per_buffer!("horizontal-scroll-bar", Value::T);
            defvar_per_buffer!("scroll-up-aggressively", Value::NIL);
            defvar_per_buffer!("scroll-down-aggressively", Value::NIL);

            // Other
            defvar_per_buffer!("text-conversion-style", Value::NIL);

            // Phase 10B/C: install BUFFER_OBJFWD descriptors for
            // every entry in BUFFER_SLOT_INFO. After this point
            // each of these symbols has redirect=Forwarded with a
            // descriptor that resolves reads/writes to
            // `Buffer::slots[offset]`. The earlier
            // `defvar_per_buffer!` left them as LOCALIZED; we
            // overwrite that with the FORWARDED tag here so the
            // VM lookup/assign hot path takes the slot fast path.
            //
            // Mirrors GNU's `defvar_per_buffer` in `buffer.c`,
            // which always uses BUFFER_OBJFWD for these C-side
            // BVAR slots (`buffer.h:319-329`).
            {
                use crate::buffer::buffer::BUFFER_SLOT_INFO;
                use crate::emacs_core::forward::alloc_buffer_objfwd;
                use crate::emacs_core::intern::intern;

                for info in BUFFER_SLOT_INFO {
                    if !info.install_as_forwarder {
                        // Internal BVAR-only slot (syntax-table /
                        // category-table / case-table). Mirrors GNU's
                        // handling of `syntax_table_` etc. which
                        // occupy BVAR slot positions but are not
                        // DEFVAR_PER_BUFFER'd. Reads/writes happen
                        // exclusively through dedicated builtins.
                        continue;
                    }
                    let id = intern(info.name);
                    let fwd = alloc_buffer_objfwd(
                        info.offset.as_u16(),
                        info.local_flags_idx,
                        info.predicate,
                        info.default.to_value(),
                    );
                    obarray.install_buffer_objfwd(id, fwd);
                }
            }
        }

        // GNU lread.c registers `lexical-binding` with DEFVAR_LISP and
        // then calls Fmake_variable_buffer_local. It is not a BVAR
        // BUFFER_OBJFWD slot, but ordinary `set` in a buffer must
        // auto-create a buffer-local binding.
        {
            let id = crate::emacs_core::intern::intern("lexical-binding");
            obarray.set_symbol_value("lexical-binding", Value::NIL);
            obarray.make_special("lexical-binding");
            obarray.make_symbol_localized(id, Value::NIL);
            obarray.set_blv_local_if_set(id, true);
        }

        // -----------------------------------------------------------------
        // C-level DEFVAR registrations: mirrors GNU's syms_of_*() functions.
        //
        // GNU Emacs declares hundreds of C-backed Lisp variables via
        // DEFVAR_LISP / DEFVAR_BOOL / DEFVAR_INT in its src/*.c files.
        // Each becomes a globally-visible symbol with a default value.
        // Elisp code reads/writes them freely; many are let-bound in
        // standard .el files during bootstrap and normal operation.
        //
        // If a variable is declared via DEFVAR in GNU's C code, it
        // MUST be registered here. Otherwise any elisp code that
        // reads or let-binds it will get void-variable.
        // -----------------------------------------------------------------

        // --- src/search.c: syms_of_search ---
        // DEFVAR_LISP, default nil. Let-bound extensively in subr.el,
        // custom.el, widget.el, mule.el, etc. to freeze match data
        // during internal string-match calls.
        obarray.set_symbol_value("inhibit-changing-match-data", Value::NIL);
        obarray.make_special("inhibit-changing-match-data");

        // --- src/search.c: syms_of_search ---
        // DEFVAR_LISP, default nil. When non-nil, a regexp substituted for
        // bunches of spaces in a regexp search. Has no elisp defvar (sibling
        // `search-whitespace-regexp` is an isearch.el defcustom), so it must be
        // seeded here; hi-lock.el (highlight-regexp) let-binds it.
        obarray.set_symbol_value("search-spaces-regexp", Value::NIL);
        obarray.make_special("search-spaces-regexp");

        // --- src/xdisp.c: syms_of_xdisp ---
        // DEFVAR_LISP, default nil. Abnormal hook run before redisplaying a
        // window with scrolling; neomacs drives it from the explicit
        // run-window-scroll-functions callsites in window_cmds, so seeding the
        // symbol only makes `boundp` true before any setq/let.
        obarray.set_symbol_value("window-scroll-functions", Value::NIL);
        obarray.make_special("window-scroll-functions");

        // --- src/alloc.c: syms_of_alloc ---
        // GC accounting DEFVAR_INTs (monotonic allocation counters). neomacs
        // does not track them yet, so seed 0 so `boundp' agrees with GNU.
        for name in [
            "cons-cells-consed",
            "floats-consed",
            "vector-cells-consed",
            "symbols-consed",
            "string-chars-consed",
            "intervals-consed",
        ] {
            obarray.define_int_variable(name, 0);
        }
        // --- src/profiler.c: syms_of_profiler ---
        obarray.define_int_variable("profiler-max-stack-depth", 16);
        obarray.define_int_variable("profiler-log-size", 10_000);
        // DEFVAR_INT, default 65536 (bignum digit-width limit).
        obarray.define_int_variable("integer-width", 65536);

        // --- src/frame.c: syms_of_frame ---
        // DEFVAR_LISP, default 20 (minimum frame alpha/opacity).
        obarray.set_symbol_value("frame-alpha-lower-limit", Value::fixnum(20));
        obarray.make_special("frame-alpha-lower-limit");
        // DEFVAR_LISP, default nil (function to adjust reported mouse position).
        obarray.set_symbol_value("mouse-position-function", Value::NIL);
        obarray.make_special("mouse-position-function");
        //
        // TWO entries found this independently, from two different consumers,
        // and both accounts are kept because each names a different invariant
        // that depends on this one fix:

        // `frame.c:7555' DEFVAR_KBOARD, and the kboard slot starts nil
        // (`keyboard.c:13129', `kset_default_minibuffer_frame (kb, Qnil)').
        // This port models kboard variables as globals, as it does for
        // `last-kbd-macro' and `defining-kbd-macro' in `keyboard::pure'.
        //
        // It was assigned only by `post_image_init' and by the frame setup in
        // `neomacs-bin', both of which run AFTER `loadup', so the name was
        // unbound for the whole of loadup where GNU has it bound from
        // `syms_of_frame' on.  Ledger 182 found it because
        // `Fsnarf_documentation' asks `Fboundp' once, at the end of loadup:
        // the variable was the only one of the DOC table's 766 bound names
        // that the snarf could not see, so it was the only one left with no
        // documentation.  The lazy lookup the snarf replaced asked the same
        // question at query time and so could not see the gap.
        //
        // DEFVAR_KBOARD, default nil (`src/frame.c:7555`).  It has to be bound
        // HERE and not from `post_image_init`'s reset table, because
        // `defvar_object::adopt` runs at the end of this bootstrap and can
        // only tag names that already exist: bound later, the symbol stays
        // `SYMBOL_PLAINVAL` and answers `special-variable-p` nil and
        // `makunbound` yes, where GNU answers t and refuses.  Measured,
        // `-Q --batch`: GNU `(t nil t)`, this port `(t nil nil)` (ledger 183).
        obarray.set_symbol_value("default-minibuffer-frame", Value::NIL);
        obarray.make_special("default-minibuffer-frame");

        // --- src/keymap.c: syms_of_keymap ---
        // DEFVAR_LISP, default nil (preferred modifier for `where-is').
        obarray.set_symbol_value("where-is-preferred-modifier", Value::NIL);
        obarray.make_special("where-is-preferred-modifier");

        // --- src/coding.c: syms_of_coding ---
        // `coding-category-utf-8' holds the coding system for the UTF-8 detection
        // category; its default is the `utf-8' coding system symbol.
        obarray.set_symbol_value("coding-category-utf-8", Value::symbol("utf-8"));
        obarray.make_special("coding-category-utf-8");

        // --- src/charset.c: syms_of_charset ---
        // `charset-list' is a DEFVAR_LISP (the list of defined charsets), NOT a
        // function -- GNU signals void-function for `(charset-list)'. Seed the
        // variable so `boundp' agrees; the neomacs registry populates the
        // ordered list separately.
        obarray.set_symbol_value("charset-list", Value::NIL);
        obarray.make_special("charset-list");

        // --- src/minibuf.c: read-buffer history ---
        // `buffer-name-history' is the minibuffer history list for buffer names,
        // default nil.
        obarray.set_symbol_value("buffer-name-history", Value::NIL);
        obarray.make_special("buffer-name-history");

        // --- src/casefiddle.c: syms_of_casefiddle ---
        // DEFVAR_BOOL + Fmake_variable_buffer_local, default 0 (nil).
        // Checked by case-conversion functions. Buffer-local via
        // make-variable-buffer-local (NOT defvar_per_buffer).
        {
            let id = crate::emacs_core::intern::intern("case-symbols-as-words");
            // DEFVAR_BOOL marks the symbol special like every C DEFVAR.
            obarray.make_symbol_localized(id, Value::NIL);
            obarray.set_blv_local_if_set(id, true);
        }

        // --- src/emacs.c: syms_of_emacs ---
        // DEFVAR_LISP, default nil. Run by kill-emacs.
        obarray.define_c_hook_variable("kill-emacs-hook");

        // --- src/cmds.c: syms_of_cmds ---
        // DEFVAR_LISP, default nil. `newline' dynamically binds this in
        // simple.el so noninteractive newline insertion runs only its local
        // postprocessor, matching GNU Emacs.
        obarray.set_symbol_value("post-self-insert-hook", Value::NIL);
        obarray.make_special("post-self-insert-hook");

        // --- src/buffer.c: syms_of_buffer ---
        // The three long-line DEFVAR_INTs, in GNU's declaration order
        // (`src/buffer.c:6007', `6025', `6043').  `long-line-optimizations-p'
        // consults the first two through `narrow-to-region' around the command
        // hooks and the third through the hscroll shortcut.
        obarray.define_int_variable("long-line-optimizations-region-size", 500_000);
        obarray.define_int_variable("long-line-optimizations-bol-search-limit", 128);
        obarray.define_int_variable("large-hscroll-threshold", 10_000);
        // GNU registers overlay hook property names with DEFSYM.  They are
        // globally interned symbols, not variables.
        for name in ["insert-in-front-hooks", "insert-behind-hooks"] {
            let id = crate::emacs_core::intern::intern(name);
            obarray.ensure_interned_global_id(id);
        }

        // --- src/keyboard.c: syms_of_keyboard ---
        // These are all DEFVAR_LISP variables in GNU.  They must exist and be
        // special before Lisp loadup: package functions compiled with lexical
        // binding rely on surrounding `let` forms remaining dynamically
        // visible while add-hook/remove-hook update the active value cell.
        obarray.define_c_hook_variable("pre-command-hook");
        obarray.define_c_hook_variable("post-command-hook");

        // GNU registers this command-loop restriction label with DEFSYM.
        {
            let id = crate::emacs_core::intern::intern("long-line-optimizations-in-command-hooks");
            obarray.ensure_interned_global_id(id);
        }

        // --- src/lread.c: syms_of_lread ---
        // GNU registers these names with DEFSYM while initializing the reader.
        // They are globally interned symbols even when they have no value or
        // function binding.
        for name in ["hash-table", "data", "test", "size", "purecopy", "weakness"] {
            let id = crate::emacs_core::intern::intern(name);
            obarray.ensure_interned_global_id(id);
        }

        // --- src/callint.c: syms_of_callint ---
        // DEFVAR_LISP, default nil.
        obarray.define_c_hook_variable("mouse-leave-buffer-hook");

        // --- src/xterm.c: syms_of_xterm / src/pgtkterm.c: syms_of_pgtkterm ---
        // GNU defines these from the compiled window-system backend before
        // Lisp loadup.  `lisp/loadup.el' deliberately checks only `boundp' for
        // some of them, and `term/x-win.el' mutates `x-keysym-table' while
        // installing the X keysym map.
        obarray.set_symbol_value("x-keysym-table", Value::hash_table(HashTableTest::Eql));
        obarray.make_special("x-keysym-table");
        obarray.set_symbol_value(
            "x-toolkit-scroll-bars",
            if cfg!(target_os = "windows") {
                Value::T
            } else {
                Value::symbol("gtk")
            },
        );
        obarray.make_special("x-toolkit-scroll-bars");
        // `gtk-version-string' and `cairo-version-string' used to be declared
        // here, holding the literals "3.24.51" and "1.18.4".  Ledger 199
        // removed them: GNU declares each one INSIDE the same conditional
        // block as the `Fprovide' that advertises its toolkit --
        // `src/xfns.c:10539-10549' pairs `Fprovide ("gtk")' with
        // `DEFVAR_LISP ("gtk-version-string", ...)' under one `#ifdef USE_GTK',
        // and `10552-10558' pairs `Fprovide ("cairo")' with
        // `cairo-version-string' under `#ifdef USE_CAIRO'
        // (`src/pgtkfns.c:3786-3802' repeats both for PGTK,
        // `src/haikuterm.c:4886' / `src/haikufns.c:3312' for Haiku).  One
        // `configure' switch compiles both statements, so no GNU build can
        // bind the variable while `featurep' answers nil -- and this port's
        // display stack is winit + wgpu + WPE, with `(featurep 'gtk)' and
        // `(featurep 'cairo)' both nil.  `lisp/version.el:113-127' and
        // `lisp/erc/erc.el:5466-5468' read them only behind those `featurep'
        // guards, which is why the invented values were inert AND undetected.
        // `emacs_core::provide_coupled_vars' holds the rule and the scan.
        obarray.define_int_variable("x-selection-timeout", 0);
        // `src/xterm.c:32704' / `32922' DEFVAR_INT, inits 200 and 128.
        obarray.define_int_variable("x-mouse-click-focus-ignore-time", 200);
        obarray.define_int_variable("x-color-cache-bucket-size", 128);
        // `src/xterm.c:32833' DEFVAR_LISP, `make_float (1.0)' -- a float, not
        // the fixnum 1: `handle_one_xevent' scales the XInput 2 scroll unit by
        // it with `XFLOATINT' after a `NUMBERP' test
        // (`src/xterm.c:22802-22803').
        obarray.define_special_variable("x-scroll-event-delta-factor", Value::make_float(1.0));
        // `src/xterm.c:32976' DEFVAR_LISP, `list2 (QCLIPBOARD, QPRIMARY)'.  The
        // list is not decoration: `x_should_preserve_selection' preserves only
        // the selections named in it when the value is a cons, and nothing at
        // all when the value is nil (`src/xselect.c:1385-1401'), so a nil
        // default is the opposite of GNU's behaviour rather than a milder
        // version of it.
        obarray.define_special_variable(
            "x-auto-preserve-selections",
            Value::list(vec![Value::symbol("CLIPBOARD"), Value::symbol("PRIMARY")]),
        );
        obarray.set_symbol_value("x-session-id", Value::NIL);
        obarray.make_special("x-session-id");
        obarray.set_symbol_value("x-session-previous-id", Value::NIL);
        obarray.make_special("x-session-previous-id");
        for name in [
            "x-ctrl-keysym",
            "x-alt-keysym",
            "x-hyper-keysym",
            "x-meta-keysym",
            "x-super-keysym",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        // The rest of `syms_of_xterm', entry 173.  This port already declared
        // 24 of `xterm.c''s 39 names before this block grew; the fifteen below
        // are the remainder, each with GNU's own initializer.
        //
        // `xterm.c:33013' DEFVAR_LISP, `Vx_allow_focus_stealing = Qnewer_time'
        // at `33037' -- a SYMBOL naming one of four policies, dispatched by
        // `EQ' against `Qimitate_pager', `Qnewer_time' and `Qraise_and_focus'
        // (`xterm.c:28876-28894', again at `29097').  nil is a fourth policy,
        // not the absence of one, so a nil seed would have chosen differently
        // rather than more weakly.
        obarray.define_special_variable("x-allow-focus-stealing", Value::symbol("newer-time"));
        // `xterm.c:33000' DEFVAR_LISP, `Vx_fast_selection_list = list1 (QCLIPBOARD)',
        // with GNU's own comment saying the default is chosen so tool-bar
        // updates need no `_XReply'.
        obarray.define_special_variable(
            "x-fast-selection-list",
            Value::list(vec![Value::symbol("CLIPBOARD")]),
        );
        // `xterm.c:32797' DEFVAR_LISP, `make_float (0.1)' -- a float, like
        // `polling-period' and `x-scroll-event-delta-factor'.
        obarray.define_special_variable("x-wait-for-event-timeout", Value::make_float(0.1));
        for name in [
            // `xterm.c:33054', `33064', `33076', `32845', `33039' -- five
            // policy flags, all `Qnil' in `syms_of_xterm'.
            "x-detect-server-trust",
            "x-lax-frame-positioning",
            "x-quit-keysym",
            "x-set-frame-visibility-more-laxly",
            "x-use-fast-mouse-position",
            // `xterm.c:32885', `32892', `32901', `32927', `32934' -- the
            // drag-and-drop callbacks, all `Qnil' in C.  GNU reports function
            // symbols for three of them only because `lisp/x-dnd.el' assigns
            // them at load time, not because the declaration does.
            "x-dnd-movement-function",
            "x-dnd-wheel-function",
            "x-dnd-unsupported-drop-function",
            "x-dnd-targets-list",
            "x-dnd-native-test-function",
            // `xterm.c:32986', `32993' -- the X input-method coding pair.
            "x-input-coding-system",
            "x-input-coding-function",
        ] {
            obarray.define_special_variable(name, Value::NIL);
        }
        // --- src/xfns.c: syms_of_xfns ---
        // The three `syms_of_xfns' names this port was short of; of the 23
        // names GNU binds from that file it already declares 20.  All three
        // are `Qnil' in C:
        // `xfns.c:10479' (`x_gtk_resize_child_frames'), `10436'
        // (`Vx_max_tooltip_size') and `10441' (`Vx_no_window_manager', whose
        // own comment reads "We don't have any way to find this out, so set it
        // to nil and maybe the user would like to set it to t").
        for name in [
            "x-gtk-resize-child-frames",
            "x-max-tooltip-size",
            "x-no-window-manager",
        ] {
            obarray.define_special_variable(name, Value::NIL);
        }
        // --- src/xselect.c: syms_of_xselect ---
        // GNU exposes these X selection notification hooks as DEFVAR_LISP
        // globals with nil defaults.
        for name in [
            "x-lost-selection-functions",
            "x-sent-selection-functions",
            // `xselect.c:3434' / `3442' DEFVAR_LISP, both `Qnil'.
            "x-treat-local-requests-remotely",
            "x-selection-alias-alist",
        ] {
            obarray.set_symbol_value(name, Value::NIL);
            obarray.make_special(name);
        }
        // --- src/xsettings.c: syms_of_xsettings ---
        // `xsettings.c:1402' DEFVAR_LISP, `Vxft_settings = empty_unibyte_string'
        // -- the empty STRING, not nil: `Fx_get_font_settings' concatenates it
        // and `xsettings.el' passes it to `read'.  The other name in this file,
        // `font-use-system-font', is already declared here.
        obarray.define_special_variable("xft-settings", Value::unibyte_string(""));
        // --- src/ccl.c: syms_of_ccl ---
        // `ccl.c:2378' DEFVAR_LISP, `make_nil_vector (16)'.  A 16-slot vector,
        // not nil: `Fregister_code_conversion_map' and `ccl.el' index into it
        // and grow it, and `aset' on nil signals.
        obarray.define_special_variable(
            "code-conversion-map-vector",
            Value::vector(vec![Value::NIL; 16]),
        );
        // --- src/doc.c: syms_of_doc ---
        // `doc.c:691' / `695' DEFVAR_LISP, both `Qnil' at declaration time.
        // `Fsnarf_documentation' is what gives either one a value, and it does
        // so only when there is a DOC file: `Vbuild_files' is filled from
        // `buildobj.h' (`doc.c:542-553') and `Vdoc_file_name = filename' is
        // assigned *after* `doc_open' succeeds (`doc.c:555-566'), so a failed
        // open signals and leaves the name alone.  This port has no
        // `make-docfile', no `buildobj.h' and no `etc/DOC' -- `doc.rs''s
        // `Snarf-documentation' is a shim that opens nothing -- so nil is what
        // is true here as well as what GNU's declaration ships.  Writing "DOC"
        // would name a file that does not exist.
        obarray.define_special_variable("internal-doc-file-name", Value::NIL);
        obarray.define_special_variable("build-files", Value::NIL);
        // --- src/syntax.c: syms_of_syntax ---
        // `syntax.c:3747' DEFVAR_LISP, `Vcomment_use_syntax_ppss = Qt' at
        // `3749'.  t, not nil, and the two readers take opposite branches on
        // it: `find_defun_start' calls out to `syntax-ppss' only while it is
        // non-nil (`syntax.c:600'), and `back_comment' honours
        // `open-paren-in-column-0-is-defun-start' only while it is nil
        // (`syntax.c:889').  So nil is a different parser, not a disabled one.
        // Neomacs's `forward-comment' does not read it yet.
        obarray.define_special_variable("comment-use-syntax-ppss", Value::T);
        // --- src/keymap.c: syms_of_keymap ---
        // `keymap.c:3400' DEFVAR_LISP, `Qnil'.
        obarray.define_special_variable("describe-bindings-check-shadowing-in-ranges", Value::NIL);
        // --- src/textconv.c: syms_of_textconv ---
        // `textconv.c:2593' DEFVAR_LISP `Qnil', and `2631' DEFVAR_LISP
        // `Qunderline' -- a face NAME, so nil would mean "no face" rather than
        // GNU's underline.  `overriding-text-conversion-style', the third name
        // in this file, is declared above.
        obarray.define_special_variable("text-conversion-edits", Value::NIL);
        obarray.define_special_variable("text-conversion-face", Value::symbol("underline"));
        // --- src/menu.c: syms_of_menu ---
        // `menu.c:1629' DEFVAR_LISP, `Qnil'.  `x-pre-popup-menu-hook', the only
        // other name in `menu.c', is already declared here.
        obarray.define_special_variable("x-popup-menu-function", Value::NIL);
        // --- src/dispnew.c: syms_of_display ---
        // `dispnew.c:7567' DEFVAR_LISP, `make_fixnum (5)'.  Spelled `x-' but
        // declared in `dispnew.o', which is in GNU's unconditional `base_obj'.
        obarray.define_special_variable("x-show-tooltip-timeout", Value::fixnum(5));
    }

    pub(super) fn new_inner(reset_thread_locals: bool) -> Self {
        // Create the heap and set thread-locals so tagged constructors work
        // during evaluator initialization.
        let mut tagged_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
        crate::tagged::gc::set_tagged_heap(&mut tagged_heap);

        // Clear any caches that hold heap-allocated Values (tagged pointers) from a
        // previous heap. Critical for test isolation when multiple Contexts
        // are created sequentially on the same thread.
        if reset_thread_locals {
            super::super::pdump::runtime::reset_runtime_for_new_heap(
                super::super::pdump::runtime::HeapResetMode::FreshContext,
            );
        }

        let mut obarray = Obarray::new();
        // Builtin names are interned by SubrSpec registration during init_builtins(),
        // which runs after Context construction.
        let default_directory = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .map(|mut s| {
                if !s.ends_with('/') {
                    s.push('/');
                }
                s
            })
            .unwrap_or_else(|| "./".to_string());
        // Create all keymaps as Emacs-compatible cons-list values
        let minibuffer_local_map = make_sparse_list_keymap();
        // Keep only the base minibuffer map here. GNU Lisp defines
        // `read-expression-map` / `read--expression-map` itself in simple.el via
        // `defvar-keymap`; prebinding them here causes those definitions to be
        // skipped, which leaves RET/C-j handling diverged from GNU Emacs.
        // Standard keymaps required by loadup.el files (normally created by C code)
        // `global-map`, `esc-map`, `ctl-x-map`, and `help-map` are defined in GNU Lisp,
        // so keep them unbound here and let the Lisp `defvar` / `defvar-keymap`
        // initializers run.  Prebinding them here causes GNU definitions like
        // help.el's `defvar-keymap help-map ...` to skip installing their real
        // bindings.
        let special_event_map = make_sparse_list_keymap();
        let mode_line_window_dedicated_keymap = make_sparse_list_keymap();
        let indent_rigidly_map = make_sparse_list_keymap();
        let text_mode_map = make_sparse_list_keymap();
        let image_slice_map = make_sparse_list_keymap();
        let tool_bar_map = make_sparse_list_keymap();
        let key_translation_map = make_sparse_list_keymap();
        let function_key_map = make_sparse_list_keymap();
        let input_decode_map = make_sparse_list_keymap();
        let local_function_key_map = make_sparse_list_keymap();
        // GNU Emacs: local-function-key-map inherits from function-key-map
        // (keyboard.c:13097). Without this, bindings in function-key-map
        // (like [backspace] → [?\C-?]) are not found during key translation.
        list_keymap_set_parent(local_function_key_map, function_key_map);
        // GNU keyboard.c seeds special-event-map with delete-frame and focus
        // handlers at C bootstrap time and leaves hook semantics to frame.el.
        list_keymap_define(
            special_event_map,
            Value::symbol("delete-frame"),
            Value::symbol("handle-delete-frame"),
        );
        list_keymap_define(
            special_event_map,
            Value::symbol("focus-in"),
            Value::symbol("handle-focus-in"),
        );
        list_keymap_define(
            special_event_map,
            Value::symbol("focus-out"),
            Value::symbol("handle-focus-out"),
        );
        // GNU's `dbus-event' entry (src/keyboard.c:14572-14576) is inside
        // `#ifdef HAVE_DBUS', as are its three neighbours -- the DEFSYM
        // (`:13477'), the `while-no-input-ignore-events' cons (`:13325') and
        // the `DBUS_EVENT' ignore-event case (`:13370').  This build has no
        // D-Bus transport, so it installs none of them (ledger 192).
        // GNU keyboard.c installs file notification events in
        // `special-event-map` when file notification support is present.
        list_keymap_define(
            special_event_map,
            Value::symbol("file-notify"),
            Value::symbol("file-notify-handle-event"),
        );

        let standard_syntax_table = super::super::syntax::builtin_standard_syntax_table(Vec::new())
            .expect("startup seeding requires standard syntax table");
        let syntax_code_objects = super::super::syntax::snapshot_syntax_code_objects()
            .unwrap_or_else(super::super::syntax::ensure_syntax_code_objects);
        let standard_category_table =
            super::super::category::ensure_standard_category_table_object()
                .expect("startup seeding requires standard category table");

        Self::seed_startup_platform_variables(&mut obarray, default_directory);
        // GNU DEFVAR_LISP variables from eval.c / keyboard.c.
        let core_eval_symbols = install_core_eval_symbols(&mut obarray, true);
        Self::seed_reader_keyboard_variables(
            &mut obarray,
            standard_syntax_table,
            minibuffer_local_map,
        );
        // ---- C-level bootstrap variables required by loadup.el files ----

        // Standard keymaps (C creates these in keyboard.c:init_kboard)
        // keyboard.c:14130 DEFVAR_LISP -- special like every C DEFVAR.
        obarray.define_special_variable("special-event-map", special_event_map);
        obarray.set_symbol_value(
            "mode-line-window-dedicated-keymap",
            mode_line_window_dedicated_keymap,
        );
        obarray.set_symbol_value("indent-rigidly-map", indent_rigidly_map);
        obarray.set_symbol_value("text-mode-map", text_mode_map);
        obarray.set_symbol_value("image-slice-map", image_slice_map);
        obarray.set_symbol_value("tool-bar-map", tool_bar_map);
        // keyboard.c:14210 / 14202 DEFVAR_LISP -- special like every C DEFVAR.
        obarray.define_special_variable("key-translation-map", key_translation_map);
        obarray.define_special_variable("function-key-map", function_key_map);
        obarray.set_symbol_value("input-decode-map", input_decode_map);
        obarray.make_special("input-decode-map");
        obarray.set_symbol_value("local-function-key-map", local_function_key_map);
        obarray.make_special("local-function-key-map");
        obarray.set_symbol_value("keyboard-translate-table", Value::NIL);
        // GNU uses DEFVAR_KBOARD here. NeoVM does not yet split keyboard state
        // per terminal, so model it as a dynamically scoped runtime variable.
        obarray.make_special("keyboard-translate-table");

        Self::seed_core_eval_variables(&mut obarray);
        let mut custom = CustomManager::new();
        Self::seed_c_level_defvars(&mut obarray, &mut custom);

        #[cfg(target_os = "windows")]
        super::super::w32::register_bootstrap_symbols(&mut obarray);

        let mut command_loop = crate::keyboard::CommandLoop::new();
        command_loop
            .keyboard
            .set_terminal_translation_maps(input_decode_map, local_function_key_map);
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
            specpdl: Vec::new(),
            suspended_thread_bindings: Vec::new(),
            profiler: super::super::profiler::ProfilerState::default(),
            lexenv: Value::NIL,
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
            features: initial_feature_ids(),
            require_stack: Vec::new(),
            loads_in_progress: Vec::new(),
            load_read_cursors: Vec::new(),
            last_uncaught_signal_backtrace: None,
            buffers: BufferManager::new(),
            xwidgets: super::super::xwidget::XwidgetState::new(),
            last_overlay_modification_hooks: Vec::new(),
            interval_insert_behind_hooks: Value::NIL,
            interval_insert_in_front_hooks: Value::NIL,
            match_data: None,
            combine_after_change_list: Vec::new(),
            combine_after_change_buffer: None,
            processes: ProcessManager::new(),
            watchers: VariableWatcherList::new(),
            active_variable_watchers: HashSet::new(),
            standard_syntax_table,
            syntax_code_objects,
            standard_category_table,
            current_local_map: Value::NIL,
            selected_global_map: super::super::keymap::SelectedGlobalMap::default(),
            registers: RegisterManager::new(),
            bookmarks: BookmarkManager::new(),
            abbrevs: AbbrevManager::new(),
            autoloads: AutoloadManager::new(),
            custom,
            rectangle: RectangleState::new(),
            interactive: InteractiveRegistry::new(),
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
            modes: ModeRegistry::new(),
            threads: ThreadManager::new(),
            kmacro: KmacroManager::new(),
            command_loop,
            input_rx: None,
            host_input_wait_backend: None,
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
            coding_systems: CodingSystemManager::new(),
            code_conversion_workspace:
                crate::code_conversion_workspace::CodeConversionWorkspace::default(),
            face_table: FaceTable::new(),
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
        super::super::runtime_identity::install(&mut ev);
        if let Some(subfeatures) = super::super::process::make_network_process_subfeatures() {
            ev.provide_value(Value::symbol("make-network-process"), Some(subfeatures))
                .expect("startup make-network-process provide should succeed");
        }
        ev.finish_runtime_activation(false);
        ev
    }
}
