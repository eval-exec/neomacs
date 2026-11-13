//! System Lisp entries that bind file, module and dump services.
//!
//! `make-temp-file-internal`, `open-dribble-file`,
//! `redirect-debugging-output`, `module-load` and `dump-emacs-portable`
//! reach file I/O, dynamic-module and pdump behaviour that lives above the
//! evaluator, so their bodies stay in the core command layer (STAGE4 design
//! §5, R0) and call those services downward.  The subr declarations stay in
//! `subrs/mod.rs` (C14): names, arity, order and module_path are unchanged.

use crate::emacs_core::error::{EvalResult, LispCondition, expect_args, expect_args_range, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::symbol::CanonicalAccess;
use crate::emacs_core::value::Value;
use neovm_edit::emacs_core::edit_host::EditHost;

pub(crate) fn builtin_make_temp_file_internal(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    crate::emacs_core::fileio::builtin_make_temp_file_internal(eval, args)
}

/// `(open-dribble-file FILE)` -> nil
///
/// Mirrors GNU `src/keyboard.c:12327-12367`. Opens FILE for
/// writing as the dribble file, where every input event will be
/// logged for debugging. Passing nil closes the current dribble
/// file. Keyboard audit Finding 11 in
/// `drafts/keyboard-command-loop-audit.md`: the previous body
/// validated the argument and silently dropped it.
///
/// The actual writes happen in the keyboard event-ingest path
/// (`KBoard::record_input_event`), which calls
/// `dribble_write_event` whenever the dribble file handle is
/// open.
pub(crate) fn builtin_open_dribble_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("open-dribble-file", &args, 1)?;
    if args[0].is_nil() {
        eval.command_loop_mut().keyboard.kboard.close_dribble_file();
        return Ok(Value::NIL);
    }
    let path = neovm_edit::emacs_core::file_name::lisp_file_name_to_path_buf(
        eval.expect_lisp_string(args[0])?,
    );
    if let Err(err) = eval
        .command_loop_mut()
        .keyboard
        .kboard
        .open_dribble_file(&path)
    {
        return Err(signal(
            LispCondition::FileError,
            vec![
                Value::string("Cannot open dribble file"),
                Value::string(err.to_string()),
            ],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_redirect_debugging_output(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("redirect-debugging-output", &args, 1, 2)?;
    if args[0].is_nil() {
        *eval.debugging_output_file_mut() = None;
        return Ok(Value::NIL);
    }
    let expanded =
        crate::emacs_core::fileio::builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let path = neovm_edit::emacs_core::file_name::lisp_file_name_to_path_buf(
        eval.expect_lisp_string(expanded)?,
    );
    let append = args.get(1).is_some_and(|value| value.is_truthy());
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append)
        .open(&path)
        .map_err(|err| {
            signal(
                LispCondition::FileError,
                vec![Value::string(err.to_string())],
            )
        })?;
    *eval.debugging_output_file_mut() = Some(file);
    Ok(Value::NIL)
}

pub(crate) fn builtin_module_load(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("module-load", &args, 1)?;
    let path = neovm_edit::emacs_core::file_name::lisp_file_name_to_path_buf(
        ctx.expect_lisp_string(args[0])?,
    );
    crate::emacs_core::dynamic_module::load_module(ctx, path)
}

pub(crate) fn builtin_dump_emacs_portable(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("dump-emacs-portable", &args, 1, 2)?;

    if !ctx.noninteractive() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Dumping Emacs currently works only in batch mode.  If you'd like it to work interactively, please consider contributing a patch to Emacs.",
            )],
        ));
    }
    if ctx.threads().current_thread_id() != 0 {
        return Err(signal(
            "error",
            vec![Value::string(
                "This function can be called only in the main thread",
            )],
        ));
    }
    if ctx.threads().all_thread_ids().into_iter().any(|id| id != 0) {
        return Err(signal(
            "error",
            vec![Value::string(
                "No other Lisp threads can be running when this function is called",
            )],
        ));
    }

    let expanded_lisp = crate::emacs_core::fileio::expand_file_name_lisp(
        ctx.expect_lisp_string(args[0])?,
        crate::emacs_core::fileio::default_directory_lisp_in_state(
            ctx.variables(),
            &[],
            &ctx.edit.buffers,
        )
        .as_ref(),
    );
    let dump_path_buf =
        neovm_edit::emacs_core::file_name::lisp_file_name_to_path_buf(&expanded_lisp);
    let dump_path = dump_path_buf.as_path();
    let expanded_path = dump_path.display().to_string();
    let is_final_dump = dump_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "emacs.pdmp" | "neomacs.pdump"));
    if is_final_dump {
        crate::emacs_core::load::normalize_final_dump_runtime_surface(ctx)
            .map_err(crate::emacs_core::error::flow_from_eval_error)?;
    }
    let saved_post_gc_hook = ctx
        .obarray()
        .canonical_value_by_name(CanonicalAccess::DUMP, "post-gc-hook")
        .unwrap_or(Value::NIL);
    let saved_command_line_processed = ctx
        .obarray()
        .canonical_value_by_name(CanonicalAccess::DUMP, "command-line-processed")
        .unwrap_or(Value::NIL);
    // `process-environment` is a `DEFVAR_LISP` (`src/callproc.c:2144`) that
    // `lisp/eshell/esh-var.el` and `lisp/progmodes/compile.el` localise, so it
    // is in ledger 191's class -- but this read is deliberately buffer-less and
    // the restore below is its matching global write, so the localized arm is
    // the right answer here and is accepted by name rather than by silence.
    // GNU's own `Fdump_emacs_portable` saves nothing of the sort. Ledger 196.
    let saved_process_environment = ctx
        .obarray()
        .canonical_value_without_buffer(CanonicalAccess::DUMP, "process-environment")
        .any_arm()
        .unwrap_or(Value::NIL);
    ctx.set_variable("post-gc-hook", Value::NIL);
    ctx.gc_collect_exact();
    // A portable dump cannot represent finalizer objects (the pdump writer
    // arms refuse them; see `pdump/convert.rs`). Pending finalizers are no
    // obstacle: the collection above doomed them and ran their functions
    // (GNU likewise runs pending finalizers before dumping), and each run can
    // drop the last reference to further finalizers, so keep collecting until
    // the live-finalizer registry stops shrinking.
    let mut live_finalizers = ctx.tagged_heap().live_finalizer_count();
    while live_finalizers > 0 {
        ctx.gc_collect_exact();
        let remaining = ctx.tagged_heap().live_finalizer_count();
        if remaining >= live_finalizers {
            break; // fixpoint: the survivors are genuinely reachable
        }
        live_finalizers = remaining;
    }
    debug_assert!(
        !ctx.tagged_heap().has_pending_doomed_finalizers(),
        "gc_collect_exact drains and runs the doomed-finalizer queue"
    );
    if live_finalizers > 0 {
        // Refuse up front with an ordinary elisp error instead of reaching the
        // writer's panic backstop. (GNU pdumper instead dumps a reachable
        // finalizer as an inert object whose function never runs in the child;
        // neomacs deliberately refuses rather than silently reviving a
        // finalizer that will never fire.)
        ctx.set_variable("post-gc-hook", saved_post_gc_hook);
        return Err(signal(
            "error",
            vec![Value::string("Cannot dump Emacs with a finalizer object")],
        ));
    }
    ctx.set_variable("command-line-processed", Value::NIL);
    ctx.set_variable("process-environment", Value::NIL);
    // This global is needed by loadup's build tools, but runtime startup
    // reconstructs it. Save it only in native locals, after the last GC:
    // Lisp bindings retaining the originals would also enter the image.
    // Ordinary runtime redumps keep GNU's behavior, regardless of filename.
    let saved_build_exec_path = ctx.loadup_dump_mode.map(|_| {
        let exec_path = ctx
            .obarray()
            .canonical_value_without_buffer(CanonicalAccess::DUMP, "exec-path")
            .any_arm()
            .unwrap_or(Value::NIL);
        ctx.set_variable("exec-path", Value::NIL);
        exec_path
    });
    let dump_result = crate::emacs_core::pdump::dump_to_file(ctx, dump_path);
    if let Some(exec_path) = saved_build_exec_path {
        ctx.set_variable("exec-path", exec_path);
    }
    ctx.set_variable("post-gc-hook", saved_post_gc_hook);
    ctx.set_variable("command-line-processed", saved_command_line_processed);
    ctx.set_variable("process-environment", saved_process_environment);

    dump_result.map_err(|err| match err {
        // GNU reports the two dump-output I/O phases through
        // `report_file_error` (src/pdumper.c:4204 for the target open,
        // :4348 for the image write), and `get_file_errno_data`
        // (src/fileio.c:265-283) maps the errno to the condition and DATA
        // (ACTION STRERROR FILE...): EACCES signals `permission-denied`,
        // ENOENT `file-missing`, and anything else `file-error`.  The typed
        // phase supplies the ACTION string ("Opening dump output" vs
        // "Could not write to dump file"), so a write failure can never be
        // reported as an open failure.  `signal_file_action_error_value` is
        // the neovm-system port of that GNU helper, shared with the file
        // primitives.
        crate::emacs_core::pdump::DumpError::Io { phase, source } => {
            crate::emacs_core::fileio::signal_file_action_error_value(
                source,
                phase.action(),
                Value::string(expanded_path),
            )
        }
        // Neomacs-specific writer failures (section layout, fingerprint,
        // ...) have no GNU counterpart; keep the historical shape.
        other => signal(
            LispCondition::FileError,
            vec![
                Value::string("dump-emacs-portable"),
                Value::string(expanded_path),
                Value::string(other.to_string()),
            ],
        ),
    })?;

    // R2-B1 (resolution B): dump-time AOT preload. Only on the FINAL production
    // dump and only when the producer is explicitly enabled by env (so ordinary
    // dump-emacs-portable calls — tests, plain dumps — pay nothing). Runs IN this
    // process, which owns the patched pdump fingerprint slot + the live obarray,
    // so the emitted `.so` matches the runtime by construction. Failures are
    // logged + swallowed inside the hook (an additive miss → runtime JITs).
    #[cfg(feature = "jit")]
    if is_final_dump {
        let dump_dir = dump_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        crate::emacs_core::jit::aot::run_dump_time_preload(ctx, dump_dir);
    }

    Ok(Value::NIL)
}
