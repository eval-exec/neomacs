//! File locking primitives.
//!
//! GNU Emacs owns these in `filelock.c`, and `buffer.c` drives them from
//! `restore-buffer-modified-p` when a file-visiting buffer changes between
//! modified and unmodified states.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::error::{EvalResult, Flow, signal};
use super::fileio::{
    find_file_name_handler_lisp_for_eval, lisp_file_name_to_path_buf,
    resolve_filename_lisp_for_eval,
};
use super::value::{Value, ValueKind};
use crate::buffer::BufferId;
use crate::heap_types::LispString;

/// GNU reports a lock failure with `report_file_errno` (filelock.c:648, 776),
/// so the DATA must be exactly what `get_file_errno_data` (fileio.c) builds:
/// `(ACTION STRERROR FILENAME)`, with the errno choosing the condition symbol
/// and STRERROR being the bare libc text.  Reuse that port rather than
/// re-deriving the shape here — an ad-hoc triple got the order wrong and
/// leaked Rust's "(os error N)" suffix.
fn file_lock_error(context: &str, filename: &LispString, err: io::Error) -> Flow {
    super::fileio::signal_file_action_error_value(
        err,
        context,
        Value::heap_string(filename.clone()),
    )
}

/// GNU `lock_file_1` takes the user from `Fuser_login_name (Qnil)` — the
/// Lisp value, not the OS environment — substituting "" for a non-string.
fn lock_user_name(eval: &super::eval::Context) -> String {
    eval.visible_variable_value_or_nil("user-login-name")
        .as_utf8_str()
        .unwrap_or("")
        .to_string()
}

/// GNU `lock_file_1` and `current_lock_owner` take the host from
/// `Fsystem_name ()` — the Lisp variable, which a sandbox or
/// --no-build-details can rebind — mapping '@' to '-' and substituting ""
/// for a non-string.  Reading the OS hostname here instead makes a
/// same-(system-name) lock look like a foreign host whose staleness can
/// never be verified.
fn lock_host_name(eval: &super::eval::Context) -> String {
    eval.visible_variable_value_or_nil("system-name")
        .as_utf8_str()
        .unwrap_or("")
        .replace('@', "-")
}

fn current_lock_info_string(user: &str, host: &str) -> String {
    let prefix = format!("{}@{}.{}", user, host, std::process::id());
    let boot_time = system_boot_time_sec();
    if boot_time == 0 {
        prefix
    } else {
        format!("{prefix}:{boot_time}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedLockInfo {
    user: String,
    host: String,
    pid: u32,
    boot_time: i64,
}

/// Parse USER@HOST.PID:BOOT.  GNU (current_lock_owner) takes the LAST `@`
/// (the user may contain one) and the last `.` after it, and rejects any
/// trailing bytes after the pid or boot integer as EINVAL.
fn parse_lock_info(contents: &str) -> Option<ParsedLockInfo> {
    let trimmed = contents.trim();
    let (user, rest) = trimmed.rsplit_once('@')?;
    let (host, pid_and_boot) = rest.rsplit_once('.')?;
    let mut parts = pid_and_boot.split(':');
    let pid = parts.next()?.parse().ok()?;
    let boot_time = match parts.next() {
        None => 0,
        Some(boot) => boot.parse().ok()?,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(ParsedLockInfo {
        user: user.to_string(),
        host: host.to_string(),
        pid,
        boot_time,
    })
}

/// Identity of another process holding a lock, parsed from the
/// USER@HOST.PID:BOOT lockfile contents.  GNU keeps the raw bytes plus
/// parse offsets (`lock_info_type`); the two consumers need two different
/// projections of it, so carry the parsed fields and derive each string.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LockClasher {
    user: String,
    host: String,
    pid: u32,
}

impl LockClasher {
    /// GNU `lock_file` rewrites ".PID" into " (pid PID)" and drops the boot
    /// time before handing the clasher to `ask-user-about-lock`.
    fn opponent(&self) -> String {
        format!("{}@{} (pid {})", self.user, self.host, self.pid)
    }
}

/// GNU's literal answer enum from filelock.c: 0 (free) / I_OWN_IT /
/// ANOTHER_OWNS_IT, with errno values carried separately as `io::Error`.
enum LockOwner {
    None,
    Current,
    Other(LockClasher),
}

/// GNU returns EINVAL for lock contents that do not parse as
/// USER@HOST.PID:BOOT.  `lock_file` deliberately ignores that errno (no
/// prompt), while file-locked-p and unlock-file report it as a file-error.
fn invalid_lock_contents_error() -> io::Error {
    // Carry the real EINVAL so report_file_errno's strerror text matches GNU.
    super::fileio::file_error_class::invalid_argument_error()
}

/// GNU's `make-lock-file-name` (files.el) prepends ".#" to the non-directory
/// part of FILENAME.  Compute it byte-faithfully on the encoded path so raw
/// unibyte file names survive intact.
fn fallback_make_lock_file_name(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?;
    let mut lock_name = std::ffi::OsString::from(".#");
    lock_name.push(name);
    let mut out = PathBuf::new();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        out.push(parent);
    }
    out.push(lock_name);
    Some(out)
}

/// GNU `make_lock_file_name` (filelock.c:535-560) is the ONLY place in the
/// lock path that expands FN: `fn = Fexpand_file_name (fn, Qnil)` at :543.
/// Everything else — Fget_truename_buffer, Fverify_visited_file_modtime,
/// Ffile_exists_p, the supersession calln, and the file name reported by
/// report_file_errno — sees the caller's string verbatim.  That matters
/// because `buffer-file-truename` is stored abbreviated ("~/..."), so
/// expanding before the truename lookup makes it miss its own buffer.
fn make_lock_file_name(
    eval: &mut super::eval::Context,
    filename: &LispString,
) -> Result<Option<PathBuf>, Flow> {
    let filename = resolve_filename_lisp_for_eval(eval, filename);
    if !eval.obarray.fboundp("make-lock-file-name") {
        // GNU cannot reach lock_file before loadup defines this function —
        // filelock.c:589 bails out under will_dump_p — so it has no analogue
        // for this state.  Use files.el's own ".#NAME" rule rather than
        // signalling void-function out of an ordinary edit.  This is the ONLY
        // reason to bypass the Lisp function; an error raised BY the function
        // is the Lisp layer's answer and must be respected.
        return Ok(fallback_make_lock_file_name(&lisp_file_name_to_path_buf(
            &filename,
        )));
    }
    // GNU uses calln here (filelock.c:558): whatever make-lock-file-name
    // signals propagates out of Flock_file.  `?` is the whole point — a
    // swallowed error used to leave us inventing a ".#NAME" lock the Lisp
    // layer had just refused to name.
    let file = Value::heap_string(filename.clone());
    let lock_file_name = eval.apply(Value::symbol("make-lock-file-name"), vec![file])?;
    match lock_file_name.kind() {
        ValueKind::Nil => Ok(None),
        ValueKind::String => Ok(Some(lisp_file_name_to_path_buf(
            lock_file_name
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload"),
        ))),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), lock_file_name],
        )),
    }
}

fn read_lock_contents(lock_path: &Path) -> io::Result<String> {
    match fs::read_link(lock_path) {
        Ok(target) => Ok(target.to_string_lossy().into_owned()),
        Err(link_err) => match fs::read_to_string(lock_path) {
            Ok(contents) => Ok(contents),
            Err(_) => Err(link_err),
        },
    }
}

/// HOST is the Lisp `(system-name)` with '@' mapped to '-', exactly as
/// lock files are written; staleness is decidable only for locks on it.
fn current_lock_owner(lock_path: &Path, host: &str) -> Result<LockOwner, io::Error> {
    match fs::symlink_metadata(lock_path) {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(LockOwner::None),
        Err(err) => return Err(err),
    }

    let contents = read_lock_contents(lock_path)?;
    if contents.is_empty() {
        // GNU zaps an empty lock file (a buggy-filesystem leftover,
        // <https://bugs.gnu.org/72641>) and reports the file free.
        return match fs::remove_file(lock_path) {
            Ok(()) => Ok(LockOwner::None),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(LockOwner::None),
            Err(err) => Err(err),
        };
    }
    let Some(info) = parse_lock_info(&contents) else {
        return Err(invalid_lock_contents_error());
    };

    // GNU filelock.c current_lock_owner: staleness is decidable only for
    // locks on THIS host. Same host + our pid = ours (GNU compares host and
    // pid, not the user). Same host + a LIVE pid whose boot time matches (or
    // is absent) = another live Emacs. Anything else — dead pid, unparseable
    // pid, or a boot time from a previous boot — is a stale lock left by a
    // crashed or killed session: zap the lockfile and report the file free,
    // NEVER prompt (an interactive session would otherwise hang every user
    // who reopens a file their crashed session had locked).
    let clasher = LockClasher {
        user: info.user.clone(),
        host: info.host.clone(),
        pid: info.pid,
    };
    if info.host != host {
        return Ok(LockOwner::Other(clasher));
    }
    if info.pid == std::process::id() {
        return Ok(LockOwner::Current);
    }
    let pid_alive = process_is_alive(info.pid);
    let boot_matches = info.boot_time == 0 || (info.boot_time - system_boot_time_sec()).abs() <= 1;
    if pid_alive && boot_matches {
        return Ok(LockOwner::Other(clasher));
    }
    match fs::remove_file(lock_path) {
        Ok(()) => Ok(LockOwner::None),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(LockOwner::None),
        Err(err) => Err(err),
    }
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    (unsafe { libc::kill(pid, 0) } == 0)
        || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, GetLastError,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};

    if pid == 0 {
        return false;
    }
    // GNU's sys_kill reports EPERM for these reserved system PIDs, which
    // current_lock_owner interprets as proof that the process exists.
    if pid <= 4 {
        return true;
    }

    let process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid) };
    if !process.is_null() {
        unsafe {
            CloseHandle(process);
        }
        return true;
    }

    match unsafe { GetLastError() } {
        ERROR_INVALID_PARAMETER => false,
        ERROR_ACCESS_DENIED => true,
        // GNU's sys_kill falls through to success for other OpenProcess
        // failures, conservatively retaining a lock it cannot prove stale.
        _ => true,
    }
}

#[cfg(not(any(unix, windows)))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

/// Seconds since the epoch at which this system booted, or 0 when unknown.
/// GNU appends this value to new lock files and uses it to reject a live PID
/// recycled after a reboot.
fn system_boot_time_sec() -> i64 {
    // GNU's lock format uses zero to mean that no comparable boot timestamp
    // was available. Keep that sentinel at this compatibility boundary.
    crate::emacs_core::host_info::boot_time()
        .map(crate::emacs_core::host_info::BootTime::unix_seconds)
        .unwrap_or(0)
}

fn create_lock_file(lock_path: &Path, contents: &str, force: bool) -> io::Result<()> {
    if force {
        match fs::remove_file(lock_path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }

    #[cfg(unix)]
    {
        match std::os::unix::fs::symlink(contents, lock_path) {
            Ok(()) => return Ok(()),
            Err(err)
                if matches!(
                    err.raw_os_error(),
                    Some(libc::ENOSYS) | Some(libc::EOPNOTSUPP) | Some(libc::ENAMETOOLONG)
                ) => {}
            Err(err) => return Err(err),
        }
    }

    // GNU falls back to a regular file only when symbolic links are not
    // supported.  Preserve `lock_file_1`'s atomic EEXIST result: a plain
    // `fs::write` would follow or overwrite an existing lock after the
    // ownership check, reintroducing a check/create race.
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(lock_path)?;
    io::Write::write_all(&mut file, contents.as_bytes())
}

enum LockAttempt {
    Acquired,
    OtherOwner(LockClasher),
    Unavailable,
}

/// Atomically acquire LOCK_PATH, mirroring GNU `lock_if_free`.
///
/// A stale lock is removed by `current_lock_owner`, after which acquisition is
/// retried.  Native filesystem errors are represented as `Unavailable` because
/// GNU `lock_file` deliberately ignores the errno returned by `lock_if_free`:
/// inability to publish an advisory lock must not replace the file operation's
/// own result.
fn lock_if_free(lock_path: &Path, contents: &str, host: &str) -> LockAttempt {
    loop {
        match create_lock_file(lock_path, contents, false) {
            Ok(()) => return LockAttempt::Acquired,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                match current_lock_owner(lock_path, host) {
                    Ok(LockOwner::None) => continue,
                    Ok(LockOwner::Current) => return LockAttempt::Acquired,
                    Ok(LockOwner::Other(clasher)) => return LockAttempt::OtherOwner(clasher),
                    Err(_) => return LockAttempt::Unavailable,
                }
            }
            Err(_) => return LockAttempt::Unavailable,
        }
    }
}

/// Where GNU's `lfname` local ends up in `lock_file` (filelock.c:592-599).
///
/// GNU represents all three states as one `Lisp_Object` that is nil in two
/// unrelated cases, and the two nils behave differently: `create-lockfiles`
/// nil skips only the lock file, while a nil from `make-lock-file-name`
/// returns from `lock_file` immediately — before the supersession check.
/// Naming the three states keeps that distinction impossible to collapse.
enum LockFileTarget {
    /// `create-lockfiles` is nil (filelock.c:593): no lock file, but the
    /// supersession check still runs — `lock-file`'s docstring says so.
    LockingDisabled,
    /// `make-lock-file-name` returned nil for this file (filelock.c:597-598):
    /// GNU returns at once, so not even the threat check happens.
    FileExemptFromLocking,
    /// The lock file to acquire.
    At(PathBuf),
}

fn lock_file_target(
    eval: &mut super::eval::Context,
    filename: &LispString,
) -> Result<LockFileTarget, Flow> {
    if !eval
        .visible_variable_value_or_nil("create-lockfiles")
        .is_truthy()
    {
        return Ok(LockFileTarget::LockingDisabled);
    }
    Ok(match make_lock_file_name(eval, filename)? {
        None => LockFileTarget::FileExemptFromLocking,
        Some(path) => LockFileTarget::At(path),
    })
}

/// GNU `lock_file` (filelock.c:601-608): if some live buffer visits FN and
/// its file has changed on disk since it was visited, ask the user — unless
/// this Emacs already owns the lock, in which case we made the change.
///
/// `calln` propagates whatever `userlock--ask-user-about-supersession-threat`
/// signals (file-supersession, or the batch-mode "Cannot resolve conflict"
/// error), which is what aborts the modification.  Never swallow it.
fn check_supersession_threat(
    eval: &mut super::eval::Context,
    filename: &LispString,
    target: &LockFileTarget,
    host: &str,
) -> Result<(), Flow> {
    let file = Value::heap_string(filename.clone());
    let subject_buf = eval.apply(Value::symbol("get-truename-buffer"), vec![file])?;
    if subject_buf.is_nil() {
        return Ok(());
    }
    if eval
        .apply(
            Value::symbol("verify-visited-file-modtime"),
            vec![subject_buf],
        )?
        .is_truthy()
    {
        return Ok(());
    }
    if eval
        .apply(Value::symbol("file-exists-p"), vec![file])?
        .is_nil()
    {
        return Ok(());
    }
    if let LockFileTarget::At(lock_path) = target
        && matches!(current_lock_owner(lock_path, host), Ok(LockOwner::Current))
    {
        return Ok(());
    }
    eval.apply(
        Value::symbol("userlock--ask-user-about-supersession-threat"),
        vec![file],
    )?;
    Ok(())
}

fn lock_file_resolved(
    eval: &mut super::eval::Context,
    filename: &LispString,
) -> Result<Value, Flow> {
    let target = lock_file_target(eval, filename)?;
    if matches!(target, LockFileTarget::FileExemptFromLocking) {
        return Ok(Value::NIL);
    }

    let host = lock_host_name(eval);
    check_supersession_threat(eval, filename, &target, &host)?;

    let LockFileTarget::At(lock_path) = target else {
        return Ok(Value::NIL);
    };
    // Re-read (system-name): the threat check ran Lisp, which may have
    // rebound it, and GNU reads it afresh inside lock_file_1.
    let host = lock_host_name(eval);
    let lock_info = current_lock_info_string(&lock_user_name(eval), &host);
    match lock_if_free(&lock_path, &lock_info, &host) {
        LockAttempt::Acquired | LockAttempt::Unavailable => Ok(Value::NIL),
        LockAttempt::OtherOwner(clasher) => {
            // GNU calls ask-user-about-lock with calln: any signal it raises
            // — the batch-mode file-locked signal from userlock.el above all
            // — propagates and aborts the modification.  Never swallow it.
            let attack = eval.apply(
                Value::symbol("ask-user-about-lock"),
                vec![
                    Value::heap_string(filename.clone()),
                    Value::string(clasher.opponent()),
                ],
            )?;
            if attack.is_truthy() {
                // GNU ignores the result of the forced `lock_file_1` too.  The
                // advisory lock must never mask the operation that requested it.
                let _ = create_lock_file(&lock_path, &lock_info, true);
            }
            Ok(Value::NIL)
        }
    }
}

fn unlock_file_resolved(
    eval: &mut super::eval::Context,
    filename: &LispString,
) -> Result<Value, Flow> {
    let Some(lock_path) = make_lock_file_name(eval, filename)? else {
        return Ok(Value::NIL);
    };

    match current_lock_owner(&lock_path, &lock_host_name(eval))
        .map_err(|err| file_lock_error("Unlocking file", filename, err))?
    {
        LockOwner::None | LockOwner::Other(_) => Ok(Value::NIL),
        LockOwner::Current => match fs::remove_file(&lock_path) {
            Ok(()) => Ok(Value::NIL),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Value::NIL),
            Err(err) => Err(file_lock_error("Unlocking file", filename, err)),
        },
    }
}

/// Handler-aware `lock-file` operation, corresponding to GNU `Flock_file`.
/// Keep this boundary separate from `lock_file_resolved`: internal native
/// filesystem work must never receive a remote or otherwise magic filename.
pub(crate) fn lock_file(
    eval: &mut super::eval::Context,
    filename: &LispString,
) -> Result<Value, Flow> {
    let operation = Value::symbol("lock-file");
    let handler = find_file_name_handler_lisp_for_eval(eval, filename, operation);
    if !handler.is_nil() {
        return eval.funcall_general(
            handler,
            vec![operation, Value::heap_string(filename.clone())],
        );
    }

    lock_file_resolved(eval, filename)
}

/// Handler-aware `unlock-file` operation, corresponding to GNU
/// `Funlock_file`.  GNU discards a file-name handler's return value.
///
/// GNU wraps the native path in `internal_condition_case_1` for `file-error`
/// and routes any such error to `userlock--handle-unlock-error`
/// (filelock.c:717-720, userlock.el:217), which warns and returns nil — so
/// `unlock-file` itself never signals a file-error.  The handler path is
/// deliberately outside that condition case, exactly as in GNU.
pub(crate) fn unlock_file(
    eval: &mut super::eval::Context,
    filename: &LispString,
) -> Result<Value, Flow> {
    let operation = Value::symbol("unlock-file");
    let handler = find_file_name_handler_lisp_for_eval(eval, filename, operation);
    if !handler.is_nil() {
        eval.funcall_general(
            handler,
            vec![operation, Value::heap_string(filename.clone())],
        )?;
        return Ok(Value::NIL);
    }

    match unlock_file_resolved(eval, filename) {
        Err(Flow::Signal(sig))
            if super::errors::signal_matches_condition_value_sym(
                &eval.obarray,
                sig.symbol,
                &Value::symbol("file-error"),
            ) =>
        {
            let error_object = Value::cons(Value::symbol(sig.symbol), signal_payload_value(&sig));
            eval.apply(
                Value::symbol("userlock--handle-unlock-error"),
                vec![error_object],
            )?;
            Ok(Value::NIL)
        }
        other => other,
    }
}

/// The `(SYMBOL . DATA)` object a `condition-case` variable would be bound to.
fn signal_payload_value(sig: &super::error::SignalData) -> Value {
    match &sig.raw_data {
        Some(raw) => *raw,
        None if sig.data.is_empty() => Value::NIL,
        None => Value::list(sig.data.clone()),
    }
}

/// Handler-aware `file-locked-p` operation, corresponding to GNU
/// `Ffile_locked_p`.  Preserve the handler's tri-state result: nil means
/// unlocked, t means owned by this Emacs, and a string names another owner.
fn file_locked_p(eval: &mut super::eval::Context, filename: &LispString) -> Result<Value, Flow> {
    let operation = Value::symbol("file-locked-p");
    let handler = find_file_name_handler_lisp_for_eval(eval, filename, operation);
    if !handler.is_nil() {
        return eval.funcall_general(
            handler,
            vec![operation, Value::heap_string(filename.clone())],
        );
    }

    let Some(lock_path) = make_lock_file_name(eval, filename)? else {
        return Ok(Value::NIL);
    };

    match current_lock_owner(&lock_path, &lock_host_name(eval))
        .map_err(|err| file_lock_error("Testing file lock", filename, err))?
    {
        LockOwner::None => Ok(Value::NIL),
        LockOwner::Current => Ok(Value::T),
        // GNU Ffile_locked_p reports only the USER part of the clasher.
        LockOwner::Other(clasher) => Ok(Value::string(clasher.user)),
    }
}

fn current_buffer_file_lock_target(
    eval: &super::eval::Context,
    buffer_id: BufferId,
) -> Option<LispString> {
    let root_id = eval.buffers.modified_state_root_id(buffer_id)?;
    let buffer = eval.buffers.get(root_id)?;
    let file_name = buffer.buffer_local_value("buffer-file-name")?;
    let file_truename = buffer.buffer_local_value("buffer-file-truename")?;
    match (file_name.kind(), file_truename.kind()) {
        (ValueKind::String, ValueKind::String) => file_truename.as_lisp_string().cloned(),
        _ => None,
    }
}

/// Lock the current file-visiting buffer before its first text change.
///
/// This is the Rust-side counterpart of GNU `prepare_to_modify_buffer_1`
/// (`src/insdel.c`): every real text edit crosses the central before-change
/// boundary, and a clean base buffer acquires its file lock there before any
/// first/before-change hook runs.  Keeping the transition here avoids teaching
/// every insertion, deletion, replacement, process-filter, and text-property
/// producer about file locking separately.
pub(crate) fn lock_current_buffer_before_change(
    eval: &mut super::eval::Context,
) -> Result<(), Flow> {
    let Some(buffer_id) = eval.buffers.current_buffer_id() else {
        return Ok(());
    };
    let clean = eval
        .buffers
        .modified_state_root_id(buffer_id)
        .and_then(|root_id| eval.buffers.get(root_id))
        .is_some_and(|buffer| buffer.modified_state_value().is_nil());
    if !clean {
        return Ok(());
    }
    let Some(filename) = current_buffer_file_lock_target(eval, buffer_id) else {
        return Ok(());
    };
    let _ = lock_file(eval, &filename)?;
    Ok(())
}

pub(crate) fn sync_modified_buffer_file_lock(
    eval: &mut super::eval::Context,
    buffer_id: BufferId,
    was_modified: bool,
    flag: Value,
) -> Result<(), Flow> {
    let Some(filename) = current_buffer_file_lock_target(eval, buffer_id) else {
        return Ok(());
    };

    // No expansion here: GNU's restore_buffer_modified_p hands
    // BVAR (b, file_truename) to Flock_file / Funlock_file untouched.
    if !was_modified && !flag.is_nil() {
        let _ = lock_file(eval, &filename)?;
    } else if was_modified && flag.is_nil() {
        let _ = unlock_file(eval, &filename)?;
    }
    Ok(())
}

pub(crate) fn builtin_lock_file(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("lock-file", &args, 1)?;
    let filename = eval.expect_lisp_string(args[0])?;
    let filename = filename.clone();
    lock_file(eval, &filename)
}

pub(crate) fn builtin_unlock_file(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("unlock-file", &args, 1)?;
    let filename = eval.expect_lisp_string(args[0])?;
    let filename = filename.clone();
    unlock_file(eval, &filename)
}

pub(crate) fn builtin_file_locked_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("file-locked-p", &args, 1)?;
    let filename = eval.expect_lisp_string(args[0])?;
    let filename = filename.clone();
    file_locked_p(eval, &filename)
}

pub(crate) fn builtin_lock_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("lock-buffer", &args, 0, 1)?;
    let filename = if let Some(filename) = args.first() {
        if filename.is_nil() {
            None
        } else {
            Some(super::builtins::expect_lisp_string(filename)?.clone())
        }
    } else {
        let current = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        current
            .buffer_local_value("buffer-file-truename")
            .and_then(|value| match value.kind() {
                ValueKind::String => value.as_lisp_string().cloned(),
                _ => None,
            })
    };

    let modified = eval
        .buffers
        .current_buffer()
        .is_some_and(|buffer| buffer.modified_state_value().is_truthy());
    if modified && let Some(filename) = filename {
        let _ = lock_file(eval, &filename)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_unlock_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("unlock-buffer", &args, 0)?;
    let Some(current) = eval.buffers.current_buffer() else {
        return Ok(Value::NIL);
    };
    if current.modified_state_value().is_truthy()
        && let Some(truename) = current.buffer_local_value("buffer-file-truename")
        && truename.is_string()
    {
        let filename = truename
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone();
        let _ = unlock_file(eval, &filename)?;
    }
    Ok(Value::NIL)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
