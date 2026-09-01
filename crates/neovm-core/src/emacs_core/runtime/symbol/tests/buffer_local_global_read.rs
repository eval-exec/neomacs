//! Ledger 196 -- the class ledger 191 named, pinned site by site.
//!
//! In GNU a special variable that some buffer has made local is **swapped in
//! for `current_buffer`** before any C read: `swap_in_symval_forwarding`
//! (`src/data.c:1573-1603`) ends with
//! `store_symval_forwarding (blv->fwd, blv_value (blv), NULL)`, which writes
//! that buffer's value into the very cell the C code dereferences.  So GNU has
//! **no** buffer-less read of a special variable -- `Vfoo`, `foo` and
//! `BVAR (current_buffer, foo)` are all "the current buffer's value".
//!
//! In this port the global obarray and the buffer-local binding are two
//! different places: [`Obarray::symbol_value`] answers the BLV *defcell* for a
//! `Localized` symbol (`symbol.rs`, the `SymbolRedirect::Localized` arm) and
//! answers `None` for a `DEFVAR_PER_BUFFER` name, whose forwarder is
//! `LispFwdType::BufferObj` and whose `load()` is `None` by construction
//! (`forward.rs`).  Any Rust site that reads such a name without naming a
//! buffer therefore disagrees with GNU the moment a buffer localises it.
//!
//! Every expectation below was produced by running the same form under
//! GNU Emacs 31.0.90 (`emacs -Q --batch`), never derived from the C source.

use crate::emacs_core::error::format_eval_result;
use crate::emacs_core::eval::Context;

fn ev() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new()
}

/// `with-temp-buffer` / `setq-local` are Lisp macros a bare [`Context`] has not
/// loaded, so the probes spell them with the special forms they expand to.
fn in_fresh_buffer(body: &str) -> String {
    format!(
        "(save-current-buffer
           (set-buffer (get-buffer-create \"l196\"))
           (prog1 (progn {body}) (kill-buffer \"l196\")))"
    )
}

// ---------------------------------------------------------------------------
// `max-lisp-eval-depth` -- GNU `DEFVAR_INT` (`src/eval.c:4405`), read as the
// bare `intmax_t` global at `src/eval.c:2585`:
//     if (++lisp_eval_depth > max_lisp_eval_depth)
// Measured under GNU:
//   (with-temp-buffer (setq-local max-lisp-eval-depth 90) <recurse>)
//     => errors at ~90, i.e. the BUFFER-LOCAL bound.
// ---------------------------------------------------------------------------

/// The recursion bound the evaluator enforces is GNU's *current buffer's*
/// `max-lisp-eval-depth`, not the default.
#[test]
fn max_lisp_eval_depth_honours_the_buffer_local_bound_like_gnu() {
    let mut eval = ev();

    // Depth reached before `excessive-lisp-nesting`, bucketed so the pin does
    // not chase an exact frame count: under 300 means the buffer-local 90 was
    // in force, above means the 1600 default was.
    let form = in_fresh_buffer(
        "(progn
           (set (make-local-variable 'max-lisp-eval-depth) 90)
           (setq l196-n 0)
           (fset 'l196-rec (function (lambda () (setq l196-n (1+ l196-n)) (l196-rec))))
           (condition-case nil (l196-rec) (error nil))
           (if (< l196-n 300) 'buffer-local-90 'global-1600))",
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(&form)),
        "OK buffer-local-90"
    );
}

// ---------------------------------------------------------------------------
// `completion-ignore-case` -- GNU `DEFVAR_BOOL` (`src/minibuf.c:2585`), read as
// the bare C `bool` throughout `src/dired.c` (`:592`, `:599`, `:886`).
// Measured under GNU, with a directory holding only `Foo.txt`:
//   (file-name-completion "foo" D)                                => nil
//   (with-temp-buffer (setq-local completion-ignore-case t)
//                     (file-name-completion "foo" D))             => "Foo.txt"
//   ... (file-name-all-completions "foo" D)                       => ("Foo.txt")
// ---------------------------------------------------------------------------

/// File-name completion folds case when the *current buffer* says to.
#[test]
fn file_name_completion_reads_the_buffer_local_completion_ignore_case_like_gnu() {
    let mut eval = ev();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("Foo.txt"), b"x").expect("write");
    let d = dir.path().to_str().expect("utf-8 tempdir");

    // Control: the global default is nil, so no folding.
    assert_eq!(
        format_eval_result(&eval.eval_str(&format!(r#"(file-name-completion "foo" "{d}/")"#))),
        "OK nil"
    );

    let form = in_fresh_buffer(&format!(
        r#"(progn (set (make-local-variable 'completion-ignore-case) t)
                  (list (file-name-completion "foo" "{d}/")
                        (file-name-all-completions "foo" "{d}/")))"#
    ));
    assert_eq!(
        format_eval_result(&eval.eval_str(&form)),
        r#"OK ("Foo.txt" ("Foo.txt"))"#
    );
}

// ---------------------------------------------------------------------------
// `print-level` / `print-length` -- GNU `DEFVAR_LISP` (`src/print.c:2923`,
// `:2918`), read bare at `src/print.c:2531` and `:2256`.
//
// GNU truncates only when the buffer-local binding is still swapped in.  With a
// BUFFER print stream `PRINTPREPARE` does `set_buffer_internal` on the stream's
// buffer, which swaps the binding OUT -- which is why `prin1-to-string` and
// `error-message-string` (`src/print.c:1058`, printing into
// `Vprin1_to_string_buffer`) do NOT truncate.  With a FUNCTION stream no buffer
// switch happens and the buffer-local value is what `Vprint_level` holds.
//
// Measured under GNU, current buffer holding local print-level 2 / length 2:
//   (prin1 '(1 (2 (3 (4 (5))))) #'external-debugging-output) => (1 (2 ...))
//   (prin1 '(1 2 3 4 5 6 7 8)   #'external-debugging-output) => (1 2 ...)
//   (prin1-to-string '(1 (2 (3 (4 (5))))))                   => "(1 (2 (3 (4 (5)))))"
// ---------------------------------------------------------------------------

/// Printing to a non-buffer stream honours the current buffer's print options.
#[test]
fn print_level_and_length_read_the_buffer_local_binding_like_gnu() {
    let mut eval = ev();

    let form = in_fresh_buffer(
        "(progn
           (set (make-local-variable 'print-level) 2)
           (set (make-local-variable 'print-length) 2)
           (setq l196-out \"\")
           (prin1 '(1 (2 (3 (4 (5)))))
                  (function (lambda (c) (setq l196-out (concat l196-out (char-to-string c))))))
           (setq l196-deep l196-out)
           (setq l196-out \"\")
           (prin1 '(1 2 3 4 5 6 7 8)
                  (function (lambda (c) (setq l196-out (concat l196-out (char-to-string c))))))
           (list l196-deep l196-out))",
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(&form)),
        r#"OK ("(1 (2 ...))" "(1 2 ...)")"#
    );
}

/// The complement, and the reason the row above is narrow: a BUFFER stream
/// swaps the binding out, so GNU does NOT truncate there.  This is the control
/// that keeps a fix from over-reaching.
#[test]
fn print_level_is_swapped_out_by_a_buffer_stream_like_gnu() {
    let mut eval = ev();

    let form = in_fresh_buffer(
        "(progn
           (set (make-local-variable 'print-level) 2)
           (set (make-local-variable 'print-length) 2)
           (list (prin1-to-string '(1 (2 (3 (4 (5))))))
                 (prin1-to-string '(1 2 3 4 5 6 7 8))))",
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(&form)),
        r#"OK ("(1 (2 (3 (4 (5)))))" "(1 2 3 4 5 6 7 8)")"#
    );
}

// ---------------------------------------------------------------------------
// `track-mouse` -- GNU `DEFVAR_LISP` (`src/keyboard.c:14134`), read as the bare
// global `track_mouse` in every terminal back-end (`src/term.c:3465`,
// `src/androidterm.c:558`, `src/haikuterm.c:425`, `src/w32fns.c:5118`).
// ---------------------------------------------------------------------------

/// The motion-event gate reads the current buffer's `track-mouse`.
#[test]
fn track_mouse_enabled_reads_the_buffer_local_binding_like_gnu() {
    let mut eval = ev();
    assert!(
        !eval.track_mouse_enabled(),
        "control: the default is nil, so the gate is closed"
    );

    eval.eval_str(
        "(progn (set-buffer (get-buffer-create \"l196tm\"))
                (set (make-local-variable 'track-mouse) 'dragging))",
    )
    .expect("localise track-mouse");
    assert!(
        eval.track_mouse_enabled(),
        "GNU's `track_mouse` is the swapped-in current-buffer binding"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(default-value 'track-mouse)")),
        "OK nil",
        "and the default is untouched"
    );
}

// ---------------------------------------------------------------------------
// `read-minibuffer-restore-windows` -- GNU `DEFVAR_BOOL`
// (`src/minibuf.c:2706`), read as the bare C bool inside `read_minibuf` at
// `src/minibuf.c:695` and `:702`.
// ---------------------------------------------------------------------------

/// `read_minibuf`'s window-restoration decision reads the current buffer's
/// binding, so a buffer that turns it off keeps its window configuration.
#[test]
fn read_minibuffer_restore_windows_reads_the_buffer_local_binding_like_gnu() {
    let mut eval = ev();
    assert_eq!(
        format_eval_result(&eval.eval_str("(default-value 'read-minibuffer-restore-windows)")),
        "OK t",
        "control: GNU's default is true (src/minibuf.c:2717)"
    );

    // The buffer stays current: the Rust assertion below is the reader's own
    // question, and GNU's swapped-in bool is a property of `current_buffer`.
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(progn (set-buffer (get-buffer-create \"l196rm\"))
                    (set (make-local-variable 'read-minibuffer-restore-windows) nil)
                    (list read-minibuffer-restore-windows
                          (default-value 'read-minibuffer-restore-windows)))",
        )),
        "OK (nil t)"
    );
    assert!(
        !crate::emacs_core::reader::minibuffer_restore_windows_requested(&eval),
        "the reader must ask the current buffer, as GNU's swapped-in bool does"
    );
}

// ---------------------------------------------------------------------------
// `command-error-function` -- GNU `DEFVAR_LISP` (`src/keyboard.c:14299`), read
// bare in `cmd_error_internal` at `src/keyboard.c:1041-1042`.
// ---------------------------------------------------------------------------

/// The command-error report dispatches through the current buffer's handler.
#[test]
fn command_error_function_reads_the_buffer_local_binding_like_gnu() {
    let mut eval = ev();

    eval.eval_str(
        "(progn (set-buffer (get-buffer-create \"l196ce\"))
                (setq l196-ce-ran nil)
                (set (make-local-variable 'command-error-function)
                     (lambda (_data _context _caller) (setq l196-ce-ran t))))",
    )
    .expect("localise command-error-function");

    let data = crate::emacs_core::value::Value::list(vec![
        crate::emacs_core::value::Value::symbol("error"),
        crate::emacs_core::value::Value::string("boom"),
    ]);
    let _ = eval.report_command_error(data, "");
    assert_eq!(
        format_eval_result(&eval.eval_str("l196-ce-ran")),
        "OK t",
        "GNU calls Vcommand_error_function, which the swap-in made the local one"
    );
}

// ---------------------------------------------------------------------------
// `default-directory` -- GNU has **no global at all**: it is
// `DEFVAR_PER_BUFFER ("default-directory", directory, ...)`
// (`src/buffer.c:5392`), and `openp` expands a non-absolute candidate against
// `BVAR (current_buffer, directory)` (`src/lread.c:1815`).
//
// Measured under GNU, with `D/l196probe.el` on disk:
//   (with-temp-buffer (setq-local default-directory D)
//     (let ((load-path (list nil))) (load "l196probe" nil t)))  => loads
// ---------------------------------------------------------------------------

/// `load` through a relative `load-path` entry resolves against the *current
/// buffer's* `default-directory`.
#[test]
fn load_path_resolves_against_the_buffer_local_default_directory_like_gnu() {
    let mut eval = ev();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("l196probe.el"),
        b";;; -*- lexical-binding: t -*-\n(setq l196-probe-loaded 'yes)\n",
    )
    .expect("write");
    let d = dir.path().to_str().expect("utf-8 tempdir");

    let form = in_fresh_buffer(&format!(
        r#"(progn (set (make-local-variable 'default-directory) "{d}/")
                  (setq l196-probe-loaded nil)
                  (let ((load-path (list nil)))
                    (load "l196probe" nil t))
                  l196-probe-loaded)"#
    ));
    assert_eq!(format_eval_result(&eval.eval_str(&form)), "OK yes");
}

// ---------------------------------------------------------------------------
// The standing guard, derived from the port's own declaration table
// ---------------------------------------------------------------------------

/// No production Rust may read a `DEFVAR_PER_BUFFER` name out of the obarray
/// without a buffer.
///
/// This is the half of ledger 191's class with **no correct case at all**. GNU
/// declares these names `DEFVAR_PER_BUFFER` and spells every read
/// `BVAR (current_buffer, ...)`; there is no `Vfoo` to read. This port installs
/// them as `LispFwdType::BufferObj` forwarders whose buffer-less `load()` is
/// `None` by construction (`forward.rs`), so such a read does not even reach a
/// default -- it gets nothing, and whatever fallback the site wrote takes over
/// silently. That is exactly how `get_load_path` came to resolve `load-path`
/// against the process cwd instead of against the buffer.
///
/// The denied set is not a hand-kept list: it is `BUFFER_SLOT_INFO` itself, the
/// table that decides which names are `DEFVAR_PER_BUFFER` in the first place,
/// so a slot added tomorrow is guarded the day it is added.
///
/// It covers every crate that can reach an `Obarray`, not just this one: the
/// ban is about the name, and the layout engine and the app binary both hold an
/// evaluator.
///
/// Two sites survive as documented dead fallbacks and are named rather than
/// silently allowed: `editing/indent/mod.rs`'s `tab-width` and
/// `lisp/native/builtins/misc_eval.rs`'s
/// `buffer-read-only` both read the buffer first and reach the obarray only
/// when the buffer already answered `None`, which for a `BufferObj` forwarder
/// cannot happen. `indent.rs` belongs to ledger 195's motion work, so deleting
/// its line is owed, not done here (ledger 196).
#[test]
fn no_production_rust_reads_a_per_buffer_name_from_the_bare_obarray() {
    use crate::buffer::buffer::BUFFER_SLOT_INFO;

    let denied: Vec<&'static str> = BUFFER_SLOT_INFO
        .iter()
        .filter(|info| info.install_as_forwarder)
        .map(|info| info.name)
        .collect();
    assert!(
        denied.len() > 40,
        "the guard derives its denied set from BUFFER_SLOT_INFO; \
         {} forwarded slots is too few to be the real table",
        denied.len()
    );

    // Grandfathered dead fallbacks, each behind a buffer read that already
    // answered. Named, so removing one is a deliberate act.
    let allowed: &[(&str, &str)] = &[
        ("emacs_core/editing/indent/mod.rs", "tab-width"),
        (
            "emacs_core/lisp/native/builtins/misc_eval.rs",
            "buffer-read-only",
        ),
    ];

    // Every crate that can reach an `Obarray`. Sibling crates are included
    // because the ban is about the *name*, not about which crate spells it: the
    // layout engine and the app binary both hold an evaluator, and a per-buffer
    // read there would be exactly as wrong and exactly as silent.
    let workspace = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).to_path_buf();
    let crates = workspace.join("crates");
    let roots = [
        "neovm-core",
        "neomacs",
        "neomacs-layout-engine",
        "neomacs-display-runtime",
        "neomacs-display-protocol",
        "neovm-worker",
    ];

    let mut files = Vec::new();
    for root in roots {
        let root_src = crates.join(root).join("src");
        let before = files.len();
        let mut stack = vec![root_src.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name == "tests") {
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.ends_with(".rs") || name.ends_with("_test.rs") || name == "test_utils.rs" {
                    continue;
                }
                files.push(path);
            }
        }
        // Per root, not just in total: a total-only assertion would let a
        // renamed or missing crate drop out silently while neovm-core alone
        // kept the number healthy. That is the brief's false-green shape.
        assert!(
            files.len() > before,
            "the walk of {} found no production files; a crate that dropped out \
             of this guard would make it silently narrower",
            root_src.display()
        );
    }
    assert!(
        files.len() > 100,
        "the walk of {} found only {} production files across {} crates; an \
         empty or truncated walk would make this guard a false green",
        workspace.display(),
        files.len(),
        roots.len()
    );

    let mut offenders = Vec::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(&workspace)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
            .replace("crates/neovm-core/src/", "");
        for name in &denied {
            for call in [
                format!(".symbol_value(\"{name}\")"),
                format!(".find_symbol_value(\"{name}\")"),
            ] {
                if source.contains(&call) && !allowed.iter().any(|(f, n)| rel == *f && n == name) {
                    offenders.push(format!("{rel}: {call}"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "GNU has no global for a DEFVAR_PER_BUFFER name -- read it through \
         Obarray::value_in_buffer with the current buffer instead (ledger 196):\n  {}",
        offenders.join("\n  ")
    );
}
