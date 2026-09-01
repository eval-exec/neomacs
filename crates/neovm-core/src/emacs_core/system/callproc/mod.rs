//! GNU-style synchronous subprocess owner, corresponding to `callproc.c`.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_min_args;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::error::{EvalResult, Flow, signal};
use super::intern::resolve_sym;
use super::process::ProcessOutputDecoding;
use super::value::{Value, ValueKind, VecLikeType};
use crate::buffer::BufferManager;
use crate::heap_types::LispString;

#[cfg(test)]
thread_local! {
    static NEW_CHILD_COMMAND_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_new_child_command_calls_for_test() {
    NEW_CHILD_COMMAND_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn new_child_command_calls_for_test() -> usize {
    NEW_CHILD_COMMAND_CALLS.with(std::cell::Cell::get)
}

/// GNU's `command-line-max-length` initializer (`src/callproc.c:2246-2252`).
///
/// `sysconf (_SC_ARG_MAX) / 4` -- "divide it by 4 as a crude way to go
/// bytes->characters" -- with GNU's own 4096 fallback for platforms that do not
/// publish `_SC_ARG_MAX`.  `sysconf` answers -1 without setting `errno` when a
/// limit is indeterminate, which is the same "no answer" case, so it takes the
/// fallback too.
pub fn command_line_max_length() -> i64 {
    #[cfg(unix)]
    {
        // SAFETY: `sysconf` reads a static system limit and touches no memory
        // the caller owns.
        let arg_max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        if arg_max > 0 {
            return (arg_max as i64) / 4;
        }
    }
    4096
}

/// Build a child `Command` already isolated into its own OS session.
///
/// Every pipe-stdio subprocess neomacs launches MUST go through this (instead
/// of bare `Command::new`) so that an interactive child (e.g. `bash -i` via
/// `shell-command-switch "-ic"`) cannot disrupt the editor. Such a child does
/// terminal job-control setup; without isolation that breaks neomacs two ways,
/// both reported under issue #132:
///   * suspend — the child's SIGTSTP/SIGTTOU reach neomacs's process group and
///     stop the whole editor;
///   * hang — left as a *background* process group on neomacs's controlling
///     terminal, the child is SIGTTOU/SIGTTIN-stopped during its own job-control
///     init and never exits, wedging a synchronous `call-process` wait forever.
///
/// On Unix we therefore `setsid` the child (new session: own process group AND
/// no controlling terminal), which fixes both. On Windows we give it its own
/// process group (`CREATE_NEW_PROCESS_GROUP`). Children that genuinely need a
/// controlling terminal (M-x shell/term) are spawned via portable_pty, which
/// sets up the pty as their controlling terminal — they do not use this path.
pub(crate) fn new_child_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    #[cfg(test)]
    NEW_CHILD_COMMAND_CALLS.with(|calls| calls.set(calls.get() + 1));
    let mut command = Command::new(program);
    isolate_child_command(&mut command);
    command
}

/// Apply the platform's "own process group" isolation to an already-built
/// command. Split out so callers that need portable_pty (which already
/// `setsid`s the child) or a pre-configured command can opt in explicitly.
pub(crate) fn isolate_child_command(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // setsid() in the child before exec puts it in a brand-new session:
        // its own process group AND no controlling terminal. Two #132 reasons:
        //   * isolation — the child's SIGTSTP/SIGTTOU stay in its own group and
        //     can never stop neomacs (the original suspend);
        //   * no controlling tty — an interactive child (`bash -i` via
        //     `shell-command-switch "-ic"`) is otherwise a *background* process
        //     group on neomacs's controlling terminal, gets SIGTTOU/SIGTTIN-
        //     stopped during its job-control init, and wedges a synchronous
        //     `call-process` wait forever (the hang). With no controlling
        //     terminal bash degrades to "no job control" and runs to completion.
        // `setsid` subsumes `setpgid(0, 0)`. PTY children that *need* a
        // controlling terminal go through portable_pty instead, not this path.
        //
        // SAFETY: the closure runs in the forked child before exec and calls
        // only the async-signal-safe `setsid`. A freshly forked process is
        // never a process-group leader, so `setsid` cannot fail with EPERM.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP: the child does not receive console
        // Ctrl-C/Ctrl-Break aimed at neomacs.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = command;
    }
}

fn maybe_redisplay_sync_output(
    eval: &mut super::eval::Context,
    destination: &Value,
    display: bool,
) -> Result<(), Flow> {
    if display && destination_writes_to_buffer_in_state(&eval.buffers, destination)? {
        eval.redisplay();
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum OutputTarget {
    Discard,
    Buffer(BufferOutputTarget),
    File(LispString),
}

#[derive(Clone, Debug)]
enum BufferOutputTarget {
    Current,
    Named(LispString),
    Existing(crate::buffer::BufferId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StderrTarget {
    Discard,
    ToStdoutTarget,
    File,
}

#[derive(Clone, Debug)]
struct DestinationSpec {
    stdout: OutputTarget,
    stderr: StderrTarget,
    stderr_file: Option<LispString>,
    no_wait: bool,
}

fn signal_wrong_type_string(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), value],
    )
}

fn lisp_string_to_os_string(string: &LispString) -> OsString {
    #[cfg(unix)]
    {
        // Byte-faithful: a multibyte arg drops to unibyte bytes (eight-bit chars
        // become their raw byte), like Emacs `string-as-unibyte`; the unibyte
        // branch already passes raw bytes through.
        if string.is_multibyte() {
            OsString::from_vec(crate::emacs_core::emacs_char::str_as_unibyte(
                string.as_bytes(),
            ))
        } else {
            OsString::from_vec(string.as_bytes().to_vec())
        }
    }

    #[cfg(not(unix))]
    {
        OsString::from(crate::emacs_core::emacs_char::to_utf8_lossy(
            string.as_bytes(),
        ))
    }
}

fn lisp_string_to_output_path(string: &LispString) -> std::path::PathBuf {
    super::fileio::lisp_file_name_to_path_buf(string)
}

fn resolve_call_process_program(
    eval: &super::eval::Context,
    program: &LispString,
) -> Result<OsString, Flow> {
    let search = super::process::ExecutableSearch::capture(eval);
    let resolved = search.resolve(program, super::process::ExecutableLookupMode::CallProcess)?;
    Ok(lisp_string_to_os_string(&resolved))
}

fn fallback_subprocess_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
}

pub(super) fn subprocess_default_directory(eval: &super::eval::Context) -> Option<PathBuf> {
    let default_dir =
        super::fileio::default_directory_lisp_in_state(&eval.obarray, &[], &eval.buffers)?;
    // GNU's encode_current_directory (callproc.c) runs default-directory through
    // Fexpand_file_name before the subprocess chdir. A buffer visiting a file under
    // $HOME has an abbreviated default-directory like "~/foo/" (GNU abbreviates it
    // identically); the OS chdir cannot resolve a literal "~", so without expanding
    // it here every subprocess (git/grep/lsp/compile) silently runs in the wrong
    // directory -- is_dir() fails on the literal "~/foo", falling back to $HOME, and
    // e.g. `git ls-files` then fails with "not a git repository".
    let expanded = super::fileio::expand_file_name_lisp(&default_dir, None);
    let path = super::fileio::lisp_file_name_to_path_buf(&expanded);
    if path.is_dir() {
        Some(path)
    } else {
        fallback_subprocess_directory()
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn configure_subprocess_current_dir(eval: &super::eval::Context, command: &mut Command) {
    if let Some(dir) = subprocess_default_directory(eval) {
        command.current_dir(dir);
    }
}

/// The `default-directory` value GNU's `get_current_directory` reads from
/// `BVAR (current_buffer, directory)`.  A `(let ((default-directory ...)) ...)`
/// dynamically rebinds the buffer-local slot, so prefer the dynamically-visible
/// value (let-aware) and fall back to the buffer-local/global value.
fn subprocess_curdir_lisp(eval: &super::eval::Context) -> Option<LispString> {
    let visible = eval.visible_variable_value_or_nil("default-directory");
    if let Some(string) = visible.as_lisp_string() {
        return Some(string.clone());
    }
    super::fileio::default_directory_lisp_in_state(&eval.obarray, &[], &eval.buffers)
}

/// GNU `get_current_directory (true)` (callproc.c): make sure the child will be
/// able to `chdir` into `default-directory` *before* spawning, and signal
/// `(file-missing "Setting current directory" "No such file or directory" DIR)`
/// (via `report_file_error ("Setting current directory", curdir)`) when it is an
/// inaccessible local directory.  Returns the validated, expanded path to use as
/// the child's working directory (or None to inherit the editor's cwd).
///
/// Mirroring GNU exactly: the directory is first run through
/// `unhandled-file-name-directory`; if that is nil the directory is a remote /
/// handled location that cannot be made the OS cwd, so GNU substitutes `~` and
/// performs no accessibility check (we keep neomacs's `$HOME` fallback).  Only a
/// *local* directory is expanded and validated, and only that case can signal.
fn validate_subprocess_current_directory(
    eval: &mut super::eval::Context,
) -> Result<Option<PathBuf>, Flow> {
    let Some(curdir) = subprocess_curdir_lisp(eval) else {
        return Ok(None);
    };

    // `dir = Funhandled_file_name_directory (curdir)`; nil => remote/handled.
    let unhandled = super::fileio::builtin_unhandled_file_name_directory_eval(
        eval,
        vec![Value::heap_string(curdir.clone())],
    )?;
    if unhandled.is_nil() {
        // Remote/handled directory: GNU uses "~" and does not validate.
        return Ok(fallback_subprocess_directory());
    }
    let unhandled_lisp = unhandled
        .as_lisp_string()
        .cloned()
        .unwrap_or_else(|| curdir.clone());

    // `dir = expand_and_dir_to_file (dir)` then
    // `if (! file_accessible_directory_p (encoded_dir)) report_file_error (...)`.
    let expanded = super::fileio::expand_file_name_lisp(&unhandled_lisp, None);
    let path = super::fileio::lisp_file_name_to_path_buf(&expanded);
    match accessible_directory_errno(&path) {
        None => Ok(Some(path)),
        Some(errno) => {
            // report_file_error ("Setting current directory", curdir): the
            // un-encoded `default-directory` value the user supplied, with the
            // errno-derived condition + strerror (ENOENT -> file-missing /
            // "No such file or directory", EACCES -> permission-denied, ...).
            Err(super::process::signal_file_errno(
                "Setting current directory",
                Value::heap_string(curdir),
                errno,
            ))
        }
    }
}

/// GNU `file_accessible_directory_p` (fileio.c): a directory DIR is accessible
/// iff `access ("DIR/.", F_OK)` succeeds, which requires every component of DIR
/// to be a searchable directory.  Return `None` on success, otherwise the errno
/// GNU's `report_file_error` would turn into the signalled condition (ENOENT,
/// EACCES, ENOTDIR, ...).
#[cfg(unix)]
fn accessible_directory_errno(path: &std::path::Path) -> Option<libc::c_int> {
    use std::os::unix::ffi::OsStrExt;
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    // Append "/." (GNU appends "/./" to dodge a macOS bug; "/." is enough on
    // Linux and keeps the errno identical).
    if bytes.last() != Some(&b'/') {
        bytes.push(b'/');
    }
    bytes.push(b'.');
    let Ok(c_path) = std::ffi::CString::new(bytes) else {
        return Some(libc::ENOENT);
    };
    if unsafe { libc::access(c_path.as_ptr(), libc::F_OK) } == 0 {
        None
    } else {
        Some(
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::ENOENT),
        )
    }
}

#[cfg(not(unix))]
fn accessible_directory_errno(path: &std::path::Path) -> Option<libc::c_int> {
    if super::fileio::file_accessible_directory_path(path) {
        None
    } else if path.exists() {
        Some(libc::EACCES)
    } else {
        Some(libc::ENOENT)
    }
}

/// Expand a relative INFILE against `default-directory` like GNU
/// `Fcall_process`'s `Fexpand_file_name (args[1], get_current_directory (false))`
/// (callproc.c:327).  Without this a bare relative INFILE ("foo.txt") is resolved
/// against the editor's process cwd instead of the buffer's `default-directory`.
fn expand_subprocess_infile(
    eval: &mut super::eval::Context,
    infile: Option<LispString>,
) -> Result<Option<LispString>, Flow> {
    let Some(infile) = infile else {
        return Ok(None);
    };
    let base = subprocess_curdir_lisp(eval)
        .map(Value::heap_string)
        .unwrap_or(Value::NIL);
    let expanded =
        super::fileio::builtin_expand_file_name(eval, vec![Value::heap_string(infile), base])?;
    Ok(Some(
        super::builtins::expect_lisp_string(&expanded)?.clone(),
    ))
}

fn configure_subprocess_environment(
    eval: &super::eval::Context,
    command: &mut Command,
    current_dir: Option<&Path>,
) {
    super::environment::ChildEnvironment::materialize(eval, current_dir).apply_to_command(command);
}

fn is_file_keyword(value: &Value) -> bool {
    value.as_keyword_id().is_some_and(|k| {
        let n = resolve_sym(k);
        n == ":file" || n == "file"
    })
}

fn parse_file_target(value: &Value) -> Result<OutputTarget, Flow> {
    let tail = value.cons_cdr();
    let file_value = if tail.is_cons() {
        tail.cons_car()
    } else {
        tail
    };
    let file = super::builtins::expect_lisp_string(&file_value)?.clone();
    Ok(OutputTarget::File(file))
}

fn parse_real_buffer_destination_in_state(
    buffers: &BufferManager,
    value: &Value,
) -> Result<(OutputTarget, bool), Flow> {
    match value.kind() {
        ValueKind::Fixnum(_) => Ok((OutputTarget::Discard, true)),
        ValueKind::Nil => Ok((OutputTarget::Discard, false)),
        ValueKind::T => Ok((OutputTarget::Buffer(BufferOutputTarget::Current), false)),
        ValueKind::String => Ok((
            OutputTarget::Buffer(BufferOutputTarget::Named(
                value
                    .as_lisp_string()
                    .expect("ValueKind::String must carry LispString payload")
                    .clone(),
            )),
            false,
        )),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buffer_id = value.as_buffer_id().unwrap();
            if buffers.get(buffer_id).is_none() {
                Err(signal(
                    "error",
                    vec![Value::string("Selecting deleted buffer")],
                ))
            } else {
                Ok((
                    OutputTarget::Buffer(BufferOutputTarget::Existing(buffer_id)),
                    false,
                ))
            }
        }
        ValueKind::Cons => {
            let first = value.cons_car();
            if is_file_keyword(&first) {
                Ok((parse_file_target(value)?, false))
            } else {
                Err(signal_wrong_type_string(first))
            }
        }
        _other => Err(signal_wrong_type_string(*value)),
    }
}

fn parse_stderr_destination(value: &Value) -> Result<(StderrTarget, Option<LispString>), Flow> {
    match value.kind() {
        ValueKind::Nil => Ok((StderrTarget::Discard, None)),
        ValueKind::T => Ok((StderrTarget::ToStdoutTarget, None)),
        ValueKind::String => Ok((
            StderrTarget::File,
            Some(
                value
                    .as_lisp_string()
                    .expect("ValueKind::String must carry LispString payload")
                    .clone(),
            ),
        )),
        _other => Err(signal_wrong_type_string(*value)),
    }
}

fn parse_call_process_destination(
    buffers: &BufferManager,
    destination: &Value,
) -> Result<DestinationSpec, Flow> {
    if destination.is_cons() {
        let first = destination.cons_car();
        if is_file_keyword(&first) {
            let stdout = parse_file_target(destination)?;
            return Ok(DestinationSpec {
                stdout,
                stderr: StderrTarget::ToStdoutTarget,
                stderr_file: None,
                no_wait: false,
            });
        }
        let (stdout, no_wait) = parse_real_buffer_destination_in_state(buffers, &first)?;
        // GNU `call_process` accepts any cons here.  Its cdr contributes an
        // explicit stderr target only when that cdr is itself a cons; dotted
        // tails are ignored and stderr retains the default of sharing stdout.
        let tail = destination.cons_cdr();
        let (stderr, stderr_file) = if tail.is_cons() {
            parse_stderr_destination(&tail.cons_car())?
        } else {
            (StderrTarget::ToStdoutTarget, None)
        };
        return Ok(DestinationSpec {
            stdout,
            stderr,
            stderr_file,
            no_wait,
        });
    }

    let (stdout, no_wait) = parse_real_buffer_destination_in_state(buffers, destination)?;
    let stderr = match destination.kind() {
        ValueKind::Nil | ValueKind::Fixnum(_) => StderrTarget::Discard,
        _ => StderrTarget::ToStdoutTarget,
    };
    Ok(DestinationSpec {
        stdout,
        stderr,
        stderr_file: None,
        no_wait,
    })
}

fn destination_writes_to_buffer_in_state(
    buffers: &BufferManager,
    destination: &Value,
) -> Result<bool, Flow> {
    let spec = parse_call_process_destination(buffers, destination)?;
    Ok(matches!(spec.stdout, OutputTarget::Buffer(_)))
}

fn insert_process_output_in_state(
    eval: &mut super::eval::Context,
    destination: &BufferOutputTarget,
    output: &crate::heap_types::LispString,
) -> Result<(), Flow> {
    let buffer_id = match destination {
        BufferOutputTarget::Current => eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
        BufferOutputTarget::Named(name) => {
            let name_str = crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes());
            eval.buffers
                .find_buffer_by_name(&name_str)
                .unwrap_or_else(|| eval.buffers.create_buffer(&name_str))
        }
        BufferOutputTarget::Existing(buffer_id) => *buffer_id,
    };

    super::editfns::insert_lisp_string_with_change_hooks_in_buffer(eval, buffer_id, output)
}

fn write_output_target_in_state(
    eval: &mut super::eval::Context,
    target: &OutputTarget,
    decoding: &mut ProcessOutputDecoding,
    output: &[u8],
    append: bool,
) -> Result<(), Flow> {
    match target {
        OutputTarget::Discard => Ok(()),
        OutputTarget::Buffer(destination) => {
            // Only the BUFFER destination decodes.  GNU hands a `(:file NAME)`
            // destination straight to the child as its stdout fd
            // (src/callproc.c:570), so those bytes are never converted.
            // `decode_coding_c_string` detects before it decodes
            // (src/coding.c:8129-8130), so `Fcall_process` sees the same
            // re-basing an asynchronous process does: measured under GNU
            // 31.0.90, `(let ((coding-system-for-read 'undecided))
            // (call-process "sh" nil t nil "-c" "printf 'caf\\303\\251\\r\\n'"))`
            // leaves `last-coding-system-used' at `utf-8-dos'.
            let resolved = decoding.detected(
                &eval.coding_systems,
                output,
                // The whole of this route's output is in hand, which is
                // GNU's state when it sets `CODING_MODE_LAST_BLOCK` at the
                // end of `Fcall_process`'s read loop (src/callproc.c:796).
                crate::emacs_core::coding::SourceBlock::Last,
            );
            // And then decodes through `decode_coding_c_string`
            // (src/callproc.c:856) -- the same macro, and so the same
            // `decode_coding_object`, that `make-process` and
            // `decode-coding-string` reach.  `Fcall_process` binds
            // `inhibit-modification-hooks` around it (:850) because its decode
            // writes straight into the destination buffer; this one produces a
            // string and the insertion below runs the change hooks GNU's
            // `signal_after_change` (:884) runs.
            //
            // An EMPTY run is not decoded at all: `if (!nread) ;`
            // (src/callproc.c:835) is the first arm of the three-way branch, so
            // no decoder and no `:post-read-conversion` runs for it.  That
            // matters here in a way it does not in GNU, because GNU reads the
            // child's stdout and stderr through ONE descriptor when DESTINATION
            // is `t` (`fd_error = fd_output`) while this route captures them
            // separately and would otherwise offer the second, usually empty,
            // one to the decoder as well -- measured, a `call-process` under a
            // coding system with a `:post-read-conversion` runs the hook once
            // in GNU and would have run it twice here.
            // `Vlast_coding_system_used = CODING_ID_NAME (process_coding.id)`
            // (src/callproc.c:913).  It sits inside the branch that READ the
            // child's output into a buffer, so a `(:file ...)` or discarded
            // destination -- where GNU never opens fd0 -- leaves the variable
            // alone; measured under GNU 31.0.90, `(call-process "sh" nil nil
            // nil "-c" "printf 'a\\r\\n'")` does not move it.  The name is the
            // one `detect_coding` and `decode_eol` left in `coding->id`, so an
            // undecided coding system reports the character code detection
            // chose and the subsidiary the end-of-line scan chose.
            // It is assigned once, AFTER the read loop, from whatever the
            // last decode left in `coding->id` -- so an EMPTY run, which never
            // reached a decoder, reports the name the previous one resolved to
            // rather than reverting to the unresolved one.
            let decoded = if output.is_empty() {
                None
            } else {
                // `Fcall_process` reads the child's output to the END before
                // it decodes (src/callproc.c:786-795, which raises
                // `CODING_MODE_LAST_BLOCK` on the read that returns zero), and
                // it decodes through a `struct coding_system` of its own that
                // no other run continues -- so the block is `Last` and there
                // is no decoder state to carry in or out.
                Some(resolved.decode_in_context(
                    eval,
                    output,
                    &mut crate::encoding::CodingDecoderState::default(),
                    crate::emacs_core::coding::SourceBlock::Last,
                )?)
            };
            let used = decoded
                .as_ref()
                .map_or_else(|| resolved.name(), |run| run.coding_used());
            eval.set_variable("last-coding-system-used", Value::symbol(used));
            // Both rewrites are IN PLACE on `coding->id`, and `Fcall_process`
            // reads the child's stdout and stderr through that one
            // `struct coding_system` -- when DESTINATION is `t` they are
            // literally the same file descriptor (`fd_error = fd_output`,
            // src/callproc.c).  So a character code and an end-of-line type
            // resolved by the first run decode the rest of the child's output
            // too, and the reported name does not fall back to the unresolved
            // one for a second, possibly EMPTY, run.
            //
            // Re-classifying rather than assuming `Coding` is what keeps the
            // faithful case: a first run of pure ASCII leaves `undecided`,
            // which STILL requires detection, so the second run detects on its
            // own bytes exactly as GNU's does.
            *decoding = ProcessOutputDecoding::for_name(used);
            match decoded {
                Some(run) => insert_process_output_in_state(eval, destination, &run.text),
                None => Ok(()),
            }
        }
        OutputTarget::File(path) => {
            // GNU opens the `(:file DEST)` output file at spawn time with
            // O_CREAT|O_TRUNC and reports a failure as
            // `report_file_errno ("Opening process output file", output_file, ...)`
            // (callproc.c:570/591) — the DATA list carries the filename and a bare
            // strerror.  neomacs writes the captured output here instead, but mirrors
            // GNU's operation string + filename so the signalled error data matches.
            let file_error = |e: std::io::Error| {
                super::process::signal_process_file_error(
                    "Opening process output file",
                    Value::heap_string(path.clone()),
                    e,
                )
            };
            let path_buf = lisp_string_to_output_path(path);
            if append {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path_buf)
                    .map_err(file_error)?;
                file.write_all(output).map_err(file_error)
            } else {
                std::fs::write(&path_buf, output).map_err(file_error)
            }
        }
    }
}

fn route_captured_output_in_state(
    eval: &mut super::eval::Context,
    destination: &DestinationSpec,
    decoding: ProcessOutputDecoding,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), Flow> {
    // ONE decoding for the whole call, carried by value from here down, because
    // GNU has one `struct coding_system` for the whole call.
    let mut decoding = decoding;
    write_output_target_in_state(eval, &destination.stdout, &mut decoding, stdout, false)?;
    match destination.stderr {
        StderrTarget::Discard => Ok(()),
        StderrTarget::ToStdoutTarget => {
            write_output_target_in_state(eval, &destination.stdout, &mut decoding, stderr, true)
        }
        StderrTarget::File => {
            let path = destination
                .stderr_file
                .as_ref()
                .ok_or_else(|| signal("error", vec![Value::string("Missing stderr file target")]))?
                .clone();
            write_output_target_in_state(
                eval,
                &OutputTarget::File(path),
                &mut decoding,
                stderr,
                false,
            )
        }
    }
}

#[cfg(unix)]
fn signal_description(signal: i32) -> String {
    let ptr = unsafe { libc::strsignal(signal) };
    if ptr.is_null() {
        "unknown".to_string()
    } else {
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

fn call_process_status_value(status: std::process::ExitStatus) -> Value {
    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        return Value::string(signal_description(signal));
    }

    Value::fixnum(status.code().unwrap_or(-1) as i64)
}

fn configure_call_process_stdin(
    command: &mut Command,
    infile: Option<&LispString>,
) -> Result<(), Flow> {
    match infile {
        None => {
            command.stdin(Stdio::null());
            Ok(())
        }
        Some(path) => {
            let file = std::fs::File::open(lisp_string_to_output_path(path)).map_err(|e| {
                // GNU `report_file_error ("Opening process input file", infile)`
                // (callproc.c:340): the DATA list ends with the (expanded)
                // filename, and the strerror text carries no Rust "(os error N)".
                super::process::signal_process_file_error(
                    "Opening process input file",
                    Value::heap_string(path.clone()),
                    e,
                )
            })?;
            command.stdin(Stdio::from(file));
            Ok(())
        }
    }
}

fn encode_call_process_region_string_input(
    input: &LispString,
    coding: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    crate::encoding::encode_lisp_string(input, coding, eol_conversion)
}

fn encode_call_process_region_buffer_text(
    text: &LispString,
    coding: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    crate::encoding::encode_lisp_string(text, coding, eol_conversion)
}

/// Resolve the coding system that encodes the region text sent to a
/// `call-process-region` subprocess.  GNU writes the region to a temp file with
/// `coding-system-for-write` (or, when that is nil, falls back to a byte-faithful
/// `raw-text` for unibyte/ASCII text), so honor `coding-system-for-write` here —
/// this is what makes the EOL conversion of `dos`/`mac`/`unix`/`utf-8-dos`
/// take effect on the region bytes.  When no write coding is requested the data
/// is left untouched (`raw-text`, a byte pass-through), matching the previous
/// behavior.
fn resolve_call_process_region_write_coding(eval: &super::eval::Context) -> String {
    let for_write = eval.visible_variable_value_or_nil("coding-system-for-write");
    for_write
        .is_truthy()
        .then(|| resolve_sym_value_name(&for_write))
        .flatten()
        .unwrap_or_else(|| "raw-text".to_string())
}

fn run_process_command_in_state(
    eval: &mut super::eval::Context,
    program: &LispString,
    infile: Option<LispString>,
    destination: &Value,
    cmd_args: &[LispString],
    operation_args: &[Value],
) -> EvalResult {
    let destination_spec = parse_call_process_destination(&eval.buffers, destination)?;
    // GNU `Fcall_process`: validate the cwd via `get_current_directory` (signals
    // "Setting current directory" for an inaccessible local dir) and expand a
    // relative INFILE against `default-directory`, both *before* spawning. The cwd
    // check happens first because GNU expands INFILE against
    // `get_current_directory (false)`, which itself validates the directory.
    let subprocess_dir = validate_subprocess_current_directory(eval)?;
    let infile = expand_subprocess_infile(eval, infile)?;
    let program_os = resolve_call_process_program(eval, program)?;
    let cmd_args_os = cmd_args
        .iter()
        .map(lisp_string_to_os_string)
        .collect::<Vec<OsString>>();

    if destination_spec.no_wait {
        let mut command = new_child_command(&program_os);
        command.args(&cmd_args_os).stdout(Stdio::null());
        if let Some(dir) = &subprocess_dir {
            command.current_dir(dir);
        }
        configure_subprocess_environment(eval, &mut command, subprocess_dir.as_deref());
        configure_call_process_stdin(&mut command, infile.as_ref())?;
        match destination_spec.stderr {
            StderrTarget::Discard | StderrTarget::ToStdoutTarget => {
                command.stderr(Stdio::null());
            }
            StderrTarget::File => {
                let path = destination_spec.stderr_file.as_ref().ok_or_else(|| {
                    signal("error", vec![Value::string("Missing stderr file target")])
                })?;
                let path_buf = lisp_string_to_output_path(path);
                let file = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&path_buf)
                    .map_err(|e| {
                        super::process::signal_process_io("Writing process output", None, e)
                    })?;
                command.stderr(Stdio::from(file));
            }
        };

        let mut child = command
            .spawn()
            .map_err(|e| super::process::signal_process_io("Searching for program", None, e))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(Value::NIL);
    }

    let mut command = new_child_command(&program_os);
    command
        .args(&cmd_args_os)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = &subprocess_dir {
        command.current_dir(dir);
    }
    configure_subprocess_environment(eval, &mut command, subprocess_dir.as_deref());
    configure_call_process_stdin(&mut command, infile.as_ref())?;
    let output = command
        .output()
        .map_err(|e| super::process::signal_process_io("Searching for program", None, e))?;

    let decoding = resolve_call_process_output_decoding(eval, operation_args, &destination_spec)?;
    route_captured_output_in_state(
        eval,
        &destination_spec,
        decoding,
        &output.stdout,
        &output.stderr,
    )?;
    Ok(call_process_status_value(output.status))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn run_process_capture_output(
    eval: &super::eval::Context,
    program: &LispString,
    cmd_args: &[LispString],
) -> Result<(i32, Vec<u8>), Flow> {
    let mut command = new_child_command(resolve_call_process_program(eval, program)?);
    command
        .args(cmd_args.iter().map(lisp_string_to_os_string))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let subprocess_dir = subprocess_default_directory(eval);
    if let Some(dir) = &subprocess_dir {
        command.current_dir(dir);
    }
    configure_subprocess_environment(eval, &mut command, subprocess_dir.as_deref());
    let output = command
        .output()
        .map_err(|e| super::process::signal_process_io("Searching for program", None, e))?;
    Ok((output.status.code().unwrap_or(-1), output.stdout))
}

fn parse_optional_infile(args: &[Value], index: usize) -> Result<Option<LispString>, Flow> {
    if args.len() > index && !args[index].is_nil() {
        Ok(Some(
            super::builtins::expect_lisp_string(&args[index])?.clone(),
        ))
    } else {
        Ok(None)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn obarray_lisp_string_variable(
    obarray: &super::symbol::Obarray,
    name: &str,
    fallback: &str,
) -> Result<LispString, Flow> {
    let value = obarray.symbol_value(name).copied().unwrap_or(Value::NIL);
    if value.is_nil() {
        Ok(LispString::from_utf8(fallback))
    } else {
        Ok(super::builtins::expect_lisp_string(&value)?.clone())
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn signal_process_lines_status_error(program: &LispString, status: i32) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "{} exited with status {status}",
            crate::emacs_core::emacs_char::to_utf8_lossy(program.as_bytes())
        ))],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn shell_command_fragment(value: &Value) -> Result<LispString, Flow> {
    super::process::char_sequence_to_lisp_string(value)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn mapconcat_identity_lisp_strings(strings: &[LispString], separator: &[u8]) -> LispString {
    if strings.is_empty() {
        return LispString::from_unibyte(Vec::new());
    }

    let multibyte = strings.iter().any(LispString::is_multibyte);
    let separator_bytes = separator
        .len()
        .saturating_mul(strings.len().saturating_sub(1));
    let total_len = strings.iter().map(LispString::sbytes).sum::<usize>() + separator_bytes;
    let mut bytes = Vec::with_capacity(total_len);

    for (index, string) in strings.iter().enumerate() {
        if index != 0 {
            bytes.extend_from_slice(separator);
        }
        bytes.extend_from_slice(string.as_bytes());
    }

    if multibyte {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn shell_command_with_legacy_args(command: &Value, args: &[Value]) -> Result<LispString, Flow> {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(shell_command_fragment(command)?);
    for arg in args {
        parts.push(shell_command_fragment(arg)?);
    }
    Ok(mapconcat_identity_lisp_strings(&parts, b" "))
}

/// Resolve the coding system used to encode the string arguments of a
/// synchronous subprocess, mirroring GNU `callproc.c` `Fcall_process`
/// (~lines 405-440):
///   * if `coding-system-for-write` is bound and non-nil, use it;
///   * else if no argument is multibyte, use `raw-text` (byte-faithful);
///   * else fall back to the cdr of `default-process-coding-system`
///     (GNU's `complement_process_encoding_system`, since
///     `find-operation-coding-system` returns nil for `call-process`);
///   * finally, if the resolved coding is NOT ASCII-compatible, downgrade to
///     `raw-text` so we never feed a multibyte-shifting codec into argv.
fn resolve_call_process_arg_coding(eval: &super::eval::Context, cmd_args: &[LispString]) -> String {
    let raw_text = || "raw-text".to_string();

    let for_write = eval.visible_variable_value_or_nil("coding-system-for-write");
    let resolved = if let Some(sym) = for_write
        .is_truthy()
        .then(|| resolve_sym_value_name(&for_write))
        .flatten()
    {
        sym
    } else if !cmd_args.iter().any(|arg| arg.is_multibyte()) {
        // No multibyte argument: GNU uses `raw-text`, leaving bytes untouched.
        return raw_text();
    } else {
        // `complement_process_encoding_system (nil)` falls back to the cdr of
        // `default-process-coding-system` (default `utf-8-unix`).
        let default_cs = eval.visible_variable_value_or_nil("default-process-coding-system");
        default_cs
            .is_cons()
            .then(|| resolve_sym_value_name(&default_cs.cons_cdr()))
            .flatten()
            .unwrap_or_else(|| "utf-8-unix".to_string())
    };

    if eval.coding_systems.is_ascii_compatible(&resolved) {
        resolved
    } else {
        raw_text()
    }
}

/// Resolve the coding system that DECODES a synchronous subprocess's output
/// into the destination buffer, mirroring GNU `Fcall_process`
/// (src/callproc.c:729-763) exactly:
///
///   * `coding-system-for-read`, when non-nil, wins outright;
///   * else the car of `(find-operation-coding-system 'call-process ARGS...)`,
///     i.e. the `process-coding-system-alist` entry matching PROGRAM;
///   * else the car of `default-process-coding-system`;
///   * else nil, which GNU turns into a byte-faithful raw copy.
///
/// GNU then runs `Fcheck_coding_system (val)` on the winner, so an unknown
/// coding system signals `coding-system-error` here rather than being silently
/// replaced by a default.
///
/// This runs AFTER the child has been reaped, which is where GNU decides it
/// too — `coding_systems` is only computed lazily at src/callproc.c:736, once
/// the pipe is ready to be read.  Anything that reads the coding variables
/// while the child runs therefore observes the same values GNU would.
///
/// `operation_args` is the GNU-shaped `call-process` argument vector
/// (PROGRAM INFILE BUFFER DISPLAY &rest ARGS); `find-operation-coding-system`
/// matches its regexps against PROGRAM and hands the whole vector to a
/// function-valued alist entry.
fn resolve_call_process_output_decoding(
    eval: &mut super::eval::Context,
    operation_args: &[Value],
    destination: &DestinationSpec,
) -> Result<ProcessOutputDecoding, Flow> {
    let for_read = eval.visible_variable_value_or_nil("coding-system-for-read");
    let resolved = if for_read.is_truthy() {
        for_read
    } else {
        let operation = find_call_process_operation_coding_system(eval, operation_args)?;
        if operation.is_cons() {
            operation.cons_car()
        } else {
            let default_cs = eval.visible_variable_value_or_nil("default-process-coding-system");
            if default_cs.is_cons() {
                default_cs.cons_car()
            } else {
                Value::NIL
            }
        }
    };

    // GNU `Fcheck_coding_system (val)` (src/callproc.c:753).  nil is a valid
    // coding system there (`Fcoding_system_p` accepts it), so this only
    // rejects a name that no coding system defines.
    super::coding::builtin_check_coding_system(&eval.coding_systems, vec![resolved])?;

    let decoding = ProcessOutputDecoding::for_coding(resolved);
    if destination_buffer_is_multibyte(eval, &destination.stdout) {
        return Ok(decoding);
    }
    // GNU src/callproc.c:754-759, verbatim: "In unibyte mode, character code
    // conversion should not take place but EOL conversion should.  So, setup
    // raw-text or one of the subsidiary according to the information just
    // setup."  `Fset_buffer (buffer)` at :722-723 means the buffer this asks
    // about is the DESTINATION buffer, not the caller's.
    Ok(decoding.without_character_conversion())
}

/// Whether the buffer a synchronous subprocess's stdout lands in is multibyte.
///
/// A destination that is not a buffer at all answers `true`, because the
/// unibyte rule above then has nothing to weaken: the bytes are written to a
/// file or discarded without ever being decoded.  A named buffer that does not
/// exist yet also answers `true` — it will be created multibyte.
fn destination_buffer_is_multibyte(eval: &super::eval::Context, target: &OutputTarget) -> bool {
    let OutputTarget::Buffer(destination) = target else {
        return true;
    };
    let buffer_id = match destination {
        BufferOutputTarget::Current => eval.buffers.current_buffer_id(),
        BufferOutputTarget::Named(name) => {
            let name = crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes());
            eval.buffers.find_buffer_by_name(&name)
        }
        BufferOutputTarget::Existing(buffer_id) => Some(*buffer_id),
    };
    buffer_id
        .and_then(|id| eval.buffers.get(id))
        .map_or(true, |buffer| buffer.get_multibyte())
}

/// `(find-operation-coding-system 'call-process PROGRAM ...)`, guarded on a
/// non-nil `process-coding-system-alist` the way the network path guards on
/// `network-coding-system-alist`: with an empty chain GNU's lookup can only
/// return nil, and skipping it keeps a user callback from being invited to run
/// arbitrary Lisp on every subprocess.
fn find_call_process_operation_coding_system(
    eval: &mut super::eval::Context,
    operation_args: &[Value],
) -> EvalResult {
    if operation_args.is_empty()
        || eval
            .visible_variable_value_or_nil("process-coding-system-alist")
            .is_nil()
    {
        return Ok(Value::NIL);
    }

    let mut args = Vec::with_capacity(operation_args.len() + 1);
    args.push(Value::symbol("call-process"));
    args.extend_from_slice(operation_args);

    // Root every heap value: a function-valued alist entry runs arbitrary Lisp
    // and can trigger GC.
    let roots = eval.save_specpdl_roots();
    for value in &args {
        eval.push_specpdl_root(*value);
    }
    let result = super::builtins::builtin_find_operation_coding_system(eval, args);
    eval.restore_specpdl_roots(roots);
    result
}

/// Extract the symbol name from a coding-system Lisp value (a symbol), or None
/// if it is not a symbol we can name.
fn resolve_sym_value_name(value: &Value) -> Option<String> {
    match value.kind() {
        ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
        _ => None,
    }
}

/// Encode each subprocess argument through the resolved write coding system,
/// matching GNU `Fcall_process`'s per-argument `encode_coding_string` loop.
/// The result is a unibyte `LispString` whose bytes are passed verbatim to
/// `execvp`, so EOL conversion (e.g. `utf-8-dos` turning `\n` into `\r\n`) and
/// charset encoding are honored.
fn encode_call_process_args(
    eval: &super::eval::Context,
    cmd_args: &[LispString],
) -> Vec<LispString> {
    if cmd_args.is_empty() {
        return Vec::new();
    }
    let coding = resolve_call_process_arg_coding(eval, cmd_args);
    let eol_conversion = eval.eol_conversion();
    cmd_args
        .iter()
        .map(|arg| {
            let bytes = crate::encoding::encode_lisp_string(arg, &coding, eol_conversion);
            LispString::from_unibyte(bytes)
        })
        .collect()
}

fn builtin_call_process_impl(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("call-process", &args, 1)?;
    let program = super::builtins::expect_lisp_string(&args[0])?.clone();
    let infile = parse_optional_infile(&args, 1)?;
    let destination = args.get(2).copied().unwrap_or(Value::NIL);
    let cmd_args = if args.len() > 4 {
        let parsed = super::process::parse_lisp_string_args_strict(&args[4..])?;
        encode_call_process_args(eval, &parsed)
    } else {
        Vec::new()
    };
    // GNU passes `Fcall_process`'s own argument vector to
    // `find-operation-coding-system` (src/callproc.c:740-744).
    let operation_args = args.clone();
    run_process_command_in_state(
        eval,
        &program,
        infile,
        &destination,
        &cmd_args,
        &operation_args,
    )
}

fn builtin_call_process_region_impl(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("call-process-region", &args, 3)?;
    // GNU `Fcall_process_region` (src/callproc.c:1099-1147) runs in this order:
    // validate the region, write it to the temp file, perform the DELETE, and
    // only then call `call_process` — which is where PROGRAM is type-checked
    // (src/callproc.c:390) and searched for on `exec-path` (src/callproc.c:447-476).
    // Everything that can signal about the program, the working directory, the
    // destination, or the argument vector therefore happens AFTER the region is
    // gone, and this function keeps that order.
    //
    // Bug 10 (GNU `Fcall_process_region`): when DELETE is set GNU removes the
    // region with `Fdelete_region`/`del_range`, whose `prepare_to_modify_buffer`
    // runs `barf_if_buffer_read_only` first — so a read-only buffer signals
    // `(buffer-read-only BUFFER)` *before* the region is touched. Mirror that by
    // checking now (only the integer/marker + whole-buffer delete cases reach
    // `Fdelete_region`; a string START with DELETE errors with `wrong-type` like
    // GNU below and never deletes).
    let delete = args.len() > 3 && args[3].is_truthy();
    if delete && !args[0].is_string() {
        super::editfns::ensure_current_buffer_writable_in_state(&eval.obarray, &[], &eval.buffers)?;
    }

    // Resolve the region's write coding (`coding-system-for-write`) before taking
    // the mutable buffers borrow below.  GNU encodes the region while writing the
    // temp file, which also precedes the DELETE.
    let write_coding = resolve_call_process_region_write_coding(eval);

    let eol_conversion = eval.eol_conversion();
    let region_text = match args[0].kind() {
        ValueKind::Nil => {
            let (text, maybe_delete_range) = {
                let buf = eval
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let range = buf.full_emacs_byte_range();
                (
                    encode_call_process_region_buffer_text(
                        &buf.buffer_substring_lisp_string_range(range),
                        &write_coding,
                        eol_conversion,
                    ),
                    range,
                )
            };
            if delete {
                let current_id = eval
                    .buffers
                    .current_buffer_id()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let _ = eval
                    .buffers
                    .delete_buffer_emacs_byte_range(current_id, maybe_delete_range);
            }
            text
        }
        ValueKind::String => {
            if delete {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integer-or-marker-p"), args[0]],
                ));
            }
            encode_call_process_region_string_input(
                args[0]
                    .as_lisp_string()
                    .expect("ValueKind::String must carry LispString payload"),
                &write_coding,
                eol_conversion,
            )
        }
        _ => {
            let region_args =
                super::position::LispRegionArgs::from_values(&eval.buffers, args[0], args[1])?;
            let (text, region) = {
                let buf = eval
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let region = super::process::checked_region_bytes(buf, region_args)?;
                (
                    encode_call_process_region_buffer_text(
                        &buf.buffer_substring_lisp_string_range(region),
                        &write_coding,
                        eol_conversion,
                    ),
                    region,
                )
            };

            if delete {
                let current_id = eval
                    .buffers
                    .current_buffer_id()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let _ = eval
                    .buffers
                    .delete_buffer_emacs_byte_range(current_id, region);
            }

            text
        }
    };

    // From here on we are inside GNU's `call_process`: PROGRAM is type-checked
    // and looked up, the working directory is validated, the child environment
    // and argument vector are built, and the destination is resolved.  A signal
    // from any of these leaves the DELETE above already applied, exactly as in
    // GNU.
    let program = super::builtins::expect_lisp_string(&args[2])?.clone();
    let program_os = resolve_call_process_program(eval, &program)?;
    // GNU `Fcall_process_region` delegates the chdir to `Fcall_process`, so the
    // same `get_current_directory` validation applies: signal "Setting current
    // directory" for an inaccessible local `default-directory`.
    let subprocess_dir = validate_subprocess_current_directory(eval)?;
    // Materialize the frame-aware child environment before taking the mutable
    // buffer borrow below.
    let subprocess_env =
        super::environment::ChildEnvironment::materialize(eval, subprocess_dir.as_deref());

    // Encode the trailing string ARGUMENTS through the write coding system,
    // exactly like the synchronous `call-process` path (GNU
    // `Fcall_process_region` delegates argv encoding to `Fcall_process`).
    let cmd_args = if args.len() > 6 {
        let parsed = super::process::parse_lisp_string_args_strict(&args[6..])?;
        encode_call_process_args(eval, &parsed)
    } else {
        Vec::new()
    };

    let destination = if args.len() > 4 {
        &args[4]
    } else {
        &Value::NIL
    };
    let destination_spec = parse_call_process_destination(&eval.buffers, destination)?;

    if destination_spec.no_wait {
        let mut command = new_child_command(&program_os);
        if let Some(dir) = &subprocess_dir {
            command.current_dir(dir);
        }
        subprocess_env.apply_to_command(&mut command);
        command
            .args(cmd_args.iter().map(lisp_string_to_os_string))
            .stdin(Stdio::piped())
            .stdout(Stdio::null());
        match destination_spec.stderr {
            StderrTarget::Discard | StderrTarget::ToStdoutTarget => {
                command.stderr(Stdio::null());
            }
            StderrTarget::File => {
                let path = destination_spec.stderr_file.as_ref().ok_or_else(|| {
                    signal("error", vec![Value::string("Missing stderr file target")])
                })?;
                let file = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(lisp_string_to_output_path(path))
                    .map_err(|e| {
                        super::process::signal_process_io("Writing process output", None, e)
                    })?;
                command.stderr(Stdio::from(file));
            }
        };

        let mut child = command
            .spawn()
            .map_err(|e| super::process::signal_process_io("Searching for program", None, e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&region_text);
        }

        std::thread::spawn(move || {
            let _ = child.wait();
        });

        return Ok(Value::NIL);
    }

    let mut command = new_child_command(&program_os);
    if let Some(dir) = &subprocess_dir {
        command.current_dir(dir);
    }
    subprocess_env.apply_to_command(&mut command);
    let mut child = command
        .args(cmd_args.iter().map(lisp_string_to_os_string))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| super::process::signal_process_io("Searching for program", None, e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&region_text);
    }

    let output = child
        .wait_with_output()
        .map_err(|e| super::process::signal_process_io("Process error", None, e))?;

    // GNU `Fcall_process_region` reshapes its arguments into `call_process`'s
    // own vector — PROGRAM, the temp INFILE, BUFFER, DISPLAY, then ARGS
    // (src/callproc.c:1149-1163) — and it is that vector which reaches
    // `find-operation-coding-system`.  The temp file's name is an
    // implementation detail neither GNU nor a coding callback can rely on, so
    // this passes nil in its place.
    let mut operation_args = vec![
        Value::heap_string(program.clone()),
        Value::NIL,
        *destination,
        args.get(5).copied().unwrap_or(Value::NIL),
    ];
    operation_args.extend_from_slice(args.get(6..).unwrap_or(&[]));
    let decoding = resolve_call_process_output_decoding(eval, &operation_args, &destination_spec)?;
    route_captured_output_in_state(
        eval,
        &destination_spec,
        decoding,
        &output.stdout,
        &output.stderr,
    )?;
    Ok(call_process_status_value(output.status))
}

pub(crate) fn builtin_call_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let destination = args.get(2).copied().unwrap_or(Value::NIL);
    let display = args.get(3).is_some_and(|v| v.is_truthy());
    let result = builtin_call_process_impl(eval, args)?;
    maybe_redisplay_sync_output(eval, &destination, display)?;
    Ok(result)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_call_process_shell_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("call-process-shell-command", &args, 1)?;
    let infile = parse_optional_infile(&args, 1)?;
    let destination = args.get(2).copied().unwrap_or(Value::NIL);
    let display = args.get(3).is_some_and(|v| v.is_truthy());
    let shell_command = shell_command_with_legacy_args(&args[0], args.get(4..).unwrap_or(&[]))?;
    let shell_program = obarray_lisp_string_variable(eval.obarray(), "shell-file-name", "sh")?;
    let shell_switch = obarray_lisp_string_variable(eval.obarray(), "shell-command-switch", "-c")?;
    let shell_args = vec![shell_switch, shell_command];
    let operation_args =
        shell_call_process_operation_args(&shell_program, &shell_args, destination);
    let result = run_process_command_in_state(
        eval,
        &shell_program,
        infile,
        &destination,
        &shell_args,
        &operation_args,
    )?;
    maybe_redisplay_sync_output(eval, &destination, display)?;
    Ok(result)
}

/// The GNU-shaped `call-process` argument vector a `*-shell-command' wrapper
/// hands down: these wrappers reach `call-process` with the SHELL as PROGRAM
/// and the assembled command line as its arguments (see `call-process-shell-command'
/// in lisp/subr.el), so that is the vector `find-operation-coding-system' sees.
fn shell_call_process_operation_args(
    shell_program: &LispString,
    shell_args: &[LispString],
    destination: Value,
) -> Vec<Value> {
    let mut operation_args = vec![
        Value::heap_string(shell_program.clone()),
        Value::NIL,
        destination,
        Value::NIL,
    ];
    operation_args.extend(shell_args.iter().map(|arg| Value::heap_string(arg.clone())));
    operation_args
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_file(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-file", &args, 1)?;
    let program = super::builtins::expect_lisp_string(&args[0])?.clone();
    let infile = parse_optional_infile(&args, 1)?;
    let destination = args.get(2).copied().unwrap_or(Value::NIL);
    let display = args.get(3).is_some_and(|v| v.is_truthy());
    let cmd_args = if args.len() > 4 {
        super::process::parse_lisp_string_args_strict(&args[4..])?
    } else {
        Vec::new()
    };
    let operation_args = args.clone();
    let result = run_process_command_in_state(
        eval,
        &program,
        infile,
        &destination,
        &cmd_args,
        &operation_args,
    )?;
    maybe_redisplay_sync_output(eval, &destination, display)?;
    Ok(result)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_file_shell_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-file-shell-command", &args, 1)?;
    let infile = parse_optional_infile(&args, 1)?;
    let destination = args.get(2).copied().unwrap_or(Value::NIL);
    let display = args.get(3).is_some_and(|v| v.is_truthy());
    let shell_command = shell_command_with_legacy_args(&args[0], args.get(4..).unwrap_or(&[]))?;
    let shell_program = obarray_lisp_string_variable(eval.obarray(), "shell-file-name", "sh")?;
    let shell_switch = obarray_lisp_string_variable(eval.obarray(), "shell-command-switch", "-c")?;
    let shell_args = vec![shell_switch, shell_command];
    let operation_args =
        shell_call_process_operation_args(&shell_program, &shell_args, destination);
    let result = run_process_command_in_state(
        eval,
        &shell_program,
        infile,
        &destination,
        &shell_args,
        &operation_args,
    )?;
    maybe_redisplay_sync_output(eval, &destination, display)?;
    Ok(result)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-lines", &args, 1)?;
    let program = super::builtins::expect_lisp_string(&args[0])?.clone();
    let cmd_args = super::process::parse_lisp_string_args_strict(&args[1..])?;
    let (status, stdout) = run_process_capture_output(eval, &program, &cmd_args)?;
    if status != 0 {
        return Err(signal_process_lines_status_error(&program, status));
    }
    Ok(parse_output_lines(&stdout))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines_ignore_status(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-lines-ignore-status", &args, 1)?;
    let program = super::builtins::expect_lisp_string(&args[0])?.clone();
    let cmd_args = super::process::parse_lisp_string_args_strict(&args[1..])?;
    let (_, stdout) = run_process_capture_output(eval, &program, &cmd_args)?;
    Ok(parse_output_lines(&stdout))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines_handling_status(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-lines-handling-status", &args, 2)?;
    let program = super::builtins::expect_lisp_string(&args[0])?.clone();
    let status_handler = args[1];
    let cmd_args = super::process::parse_lisp_string_args_strict(&args[2..])?;
    let (status, stdout) = run_process_capture_output(eval, &program, &cmd_args)?;
    let lines = parse_output_lines(&stdout);

    if !status_handler.is_nil() {
        let _ = eval.apply(status_handler, vec![Value::fixnum(status as i64)])?;
    } else if status != 0 {
        return Err(signal_process_lines_status_error(&program, status));
    }

    Ok(lines)
}

pub(crate) fn builtin_call_process_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("call-process-region", &args, 3)?;
    let destination = args.get(4).copied().unwrap_or(Value::NIL);
    let display = args.get(5).is_some_and(|v| v.is_truthy());
    let result = builtin_call_process_region_impl(eval, args)?;
    maybe_redisplay_sync_output(eval, &destination, display)?;
    Ok(result)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn parse_output_lines(stdout: &[u8]) -> Value {
    let mut text = String::from_utf8_lossy(stdout).into_owned();
    if text.ends_with('\n') {
        text.pop();
    }
    if text.is_empty() {
        Value::NIL
    } else {
        Value::list(text.split('\n').map(Value::string).collect())
    }
}

#[cfg(test)]
#[path = "tests/raw_bytes.rs"]
mod raw_bytes_tests;

#[cfg(test)]
#[path = "tests/read_coding.rs"]
mod read_coding_tests;

#[cfg(test)]
#[path = "tests/working_dir_infile.rs"]
mod working_dir_infile_tests;

#[cfg(all(test, unix))]
mod child_isolation_tests {
    use super::new_child_command;
    use std::process::Stdio;

    /// Regression test for issue #132: every spawned pipe-stdio child must live
    /// in its own *session* (`setsid`) — its own process group AND no
    /// controlling terminal. The process group stops a child's SIGTSTP/SIGTTOU
    /// from suspending the editor (the suspend); the lack of a controlling
    /// terminal stops an interactive `bash -i` from being SIGTTOU/SIGTTIN-
    /// stopped as a background process group, which would wedge a synchronous
    /// `call-process` forever (the hang).
    #[test]
    fn child_runs_in_its_own_session() {
        let parent_pgid = unsafe { libc::getpgrp() };
        let parent_sid = unsafe { libc::getsid(0) };
        let mut child = new_child_command("sh")
            .arg("-c")
            .arg("sleep 1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn child");
        let pid = child.id() as libc::pid_t;
        // Read the child's process group + session while it is still alive.
        let child_pgid = unsafe { libc::getpgid(pid) };
        let child_sid = unsafe { libc::getsid(pid) };
        let _ = child.kill();
        let _ = child.wait();

        assert!(child_pgid > 0, "getpgid failed for live child");
        assert_ne!(
            child_pgid, parent_pgid,
            "child shares the editor's process group; its SIGTSTP/SIGTTOU could suspend neomacs (#132 suspend)"
        );
        assert_eq!(
            child_pgid, pid,
            "isolated child should lead its own process group"
        );
        // setsid makes the child a session leader (sid == pid) in a session
        // distinct from the editor's, so it has no controlling terminal and an
        // interactive shell cannot get SIGTTOU/SIGTTIN-stopped (#132 hang).
        assert!(child_sid > 0, "getsid failed for live child");
        assert_eq!(
            child_sid, pid,
            "isolated child should lead its own session (setsid)"
        );
        assert_ne!(
            child_sid, parent_sid,
            "child shares the editor's session/controlling terminal (#132 hang)"
        );
    }
}
