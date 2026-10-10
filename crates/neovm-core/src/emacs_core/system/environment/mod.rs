//! Lisp-visible and child-process environment policy.
//!
//! GNU Emacs deliberately separates the editor's mutable
//! `process-environment` from the native process environment. Missing
//! variables normally stay missing; `DISPLAY` is the exception because it is
//! associated with the selected GUI frame and falls back to the immutable
//! startup snapshot.

use super::error::EvalResult;
use super::eval::Context;
use super::value::Value;
use crate::heap_types::LispString;
use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

pub(crate) enum EnvironmentLookup {
    Value(Value),
    Negative,
    Missing,
}

fn host_environment_entries() -> Vec<(String, String)> {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut entries: Vec<(String, String)> = std::env::vars().collect();
    // Mirror GNU `w32.c init_environment`: guarantee HOME is set on Windows,
    // where the OS environment provides APPDATA/USERPROFILE but typically not
    // HOME. Without it `getenv "HOME"` is nil and `~` never expands, so e.g.
    // `directory-files "~"` fails fatally during startup. GNU defaults HOME to
    // the roaming AppData folder (CSIDL_APPDATA == %APPDATA%), else "C:/".
    #[cfg(windows)]
    if !entries.iter().any(|(k, _)| k.eq_ignore_ascii_case("HOME")) {
        let home = std::env::var("APPDATA")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "C:/".to_string());
        entries.push(("HOME".to_string(), home));
    }
    // GNU w32.c init_environment also guarantees SHELL before Lisp snapshots
    // the process environment. Keep `(getenv "SHELL")` and
    // `shell-file-name` on the same private cmdproxy path.
    #[cfg(windows)]
    if !entries
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("SHELL") && !value.is_empty())
    {
        let shell = super::shell_file_name::resolve_current();
        entries.retain(|(name, _)| !name.eq_ignore_ascii_case("SHELL"));
        entries.push(("SHELL".to_owned(), shell.lisp_name().to_owned()));
    }
    entries
}

fn environment_list(entries: Vec<(String, String)>) -> Value {
    Value::list(
        entries
            .into_iter()
            .map(|(name, value)| Value::string(format!("{name}={value}")))
            .collect::<Vec<_>>(),
    )
}

/// Names of the variables a launcher wrapper changed, `:`-separated.
const LAUNCHER_CHANGED_ENV: &str = "NEOMACS_WRAPPER_ENV";
/// Prefix of the variables holding the values those names had before the
/// wrapper ran; a name without one was unset.
const LAUNCHER_ORIGINAL_ENV_PREFIX: &str = "NEOMACS_WRAPPER_ORIGINAL_";

/// Undo a launcher wrapper's changes to the inherited environment.
///
/// A packaged Neomacs can start through a wrapper that points the editor at
/// its private libraries, runtime files, drivers and plugins (the Nix
/// package prefixes `LD_LIBRARY_PATH`, for one).  Those settings belong to
/// the editor process; a program started from Neomacs must not run with
/// them.  GNU's `initial-environment' is the environment inherited from the
/// parent process, which is the one the user launched with, so rebuild it
/// from the wrapper's record (see `nix/package.nix').  Unlike GNU, the Lisp
/// startup environment then differs from this process's own `environ' by
/// exactly the wrapper's changes.  That native environment is untouched:
/// the dynamic loader has already fixed the editor's library search path,
/// and libraries that read their settings later (Vulkan, GStreamer) keep
/// seeing them.
fn environment_before_launcher(inherited: Vec<(String, String)>) -> Vec<(String, String)> {
    let Some(changed) = inherited
        .iter()
        .find(|(name, _)| environment_name_eq(name.as_bytes(), LAUNCHER_CHANGED_ENV.as_bytes()))
        .map(|(_, names)| names.clone())
    else {
        return inherited;
    };
    let changed: Vec<&str> = changed.split(':').filter(|name| !name.is_empty()).collect();
    let is_changed = |name: &str| {
        changed
            .iter()
            .any(|changed| environment_name_eq(changed.as_bytes(), name.as_bytes()))
    };
    let launcher_record = |name: &str| {
        environment_name_eq(name.as_bytes(), LAUNCHER_CHANGED_ENV.as_bytes())
            || name
                .split_at_checked(LAUNCHER_ORIGINAL_ENV_PREFIX.len())
                .is_some_and(|(prefix, _)| {
                    environment_name_eq(prefix.as_bytes(), LAUNCHER_ORIGINAL_ENV_PREFIX.as_bytes())
                })
    };
    let original = |name: &str| {
        inherited.iter().find_map(|(entry, value)| {
            let (prefix, suffix) = entry.split_at_checked(LAUNCHER_ORIGINAL_ENV_PREFIX.len())?;
            (environment_name_eq(prefix.as_bytes(), LAUNCHER_ORIGINAL_ENV_PREFIX.as_bytes())
                && environment_name_eq(suffix.as_bytes(), name.as_bytes()))
            .then(|| value.clone())
        })
    };

    let mut restored = Vec::with_capacity(inherited.len());
    for (name, value) in &inherited {
        if launcher_record(name) {
            continue;
        }
        if is_changed(name) {
            if let Some(original) = original(name) {
                restored.push((name.clone(), original));
            }
        } else {
            restored.push((name.clone(), value.clone()));
        }
    }
    // A variable the launcher removed outright comes back too.
    for name in changed {
        if !restored
            .iter()
            .any(|(entry, _)| environment_name_eq(entry.as_bytes(), name.as_bytes()))
            && let Some(original) = original(name)
        {
            restored.push((name.to_owned(), original));
        }
    }
    restored
}

/// Install the environment inherited by this Neomacs process as the Lisp
/// startup environment.
///
/// GNU initializes `process-environment` before `loadup.el`, so every startup
/// consumer observes the same HOME, PATH, and other host variables as file
/// expansion and subprocess creation. A cached evaluator needs the same
/// operation when it is activated, because its dumped environment belongs to
/// the process that created the cache rather than the current process.
pub(crate) fn install_host_environment_snapshot(eval: &mut Context) {
    install_environment_snapshot(eval, host_environment_entries());
}

fn install_environment_snapshot(eval: &mut Context, inherited: Vec<(String, String)>) {
    let process_environment = environment_list(environment_before_launcher(inherited));
    {
        let obarray = eval.obarray_mut();
        obarray.make_special("initial-environment");
        obarray.make_special("process-environment");
    }
    let initial_environment = super::builtins::builtin_copy_sequence(vec![process_environment])
        .expect("copy the startup environment snapshot");
    eval.set_variable("initial-environment", initial_environment);
    eval.set_variable("process-environment", process_environment);
}

#[derive(Clone, Debug)]
pub(crate) struct ChildEnvironment {
    /// Shared so that handing the list to a command (possibly more than
    /// once, on a retry) copies no variable.
    entries: std::sync::Arc<[(OsString, OsString)]>,
}

fn environment_name_eq(left: &[u8], right: &[u8]) -> bool {
    std::cfg_select! {
        windows => {
            left.eq_ignore_ascii_case(right)
        }
        _ => {
            left == right
        }
    }
}

fn environment_value_string(string: &LispString, start: usize) -> LispString {
    let bytes = string.as_bytes()[start..].to_vec();
    if string.is_multibyte() {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

fn lisp_bytes_to_os_string(bytes: &[u8]) -> OsString {
    std::cfg_select! {
        unix => {
            OsString::from_vec(bytes.to_vec())
        }
        _ => {
            OsString::from(super::emacs_char::to_utf8_lossy(bytes))
        }
    }
}

fn split_environment_entry(entry: &LispString) -> (OsString, Option<OsString>) {
    let bytes = entry.as_bytes();
    if let Some(separator) = bytes.iter().position(|byte| *byte == b'=') {
        (
            lisp_bytes_to_os_string(&bytes[..separator]),
            Some(lisp_bytes_to_os_string(&bytes[separator + 1..])),
        )
    } else {
        (lisp_bytes_to_os_string(bytes), None)
    }
}

fn os_environment_name_eq(left: &OsStr, right: &OsStr) -> bool {
    std::cfg_select! {
        windows => {
            left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy())
        }
        _ => {
            left == right
        }
    }
}

/// The key an environment variable is de-duplicated by: its name, compared
/// as `os_environment_name_eq` compares (case-insensitively on Windows).
fn environment_name_key(name: &OsStr) -> Vec<u8> {
    std::cfg_select! {
        windows => {
            name.to_string_lossy().to_ascii_uppercase().into_bytes()
        }
        _ => {
            std::os::unix::ffi::OsStrExt::as_bytes(name).to_vec()
        }
    }
}

fn push_unique_environment_entry(
    entries: &mut Vec<(OsString, OsString)>,
    seen: &mut rustc_hash::FxHashSet<Vec<u8>>,
    name: OsString,
    value: Option<OsString>,
) {
    // A set, not a scan of the names so far: a child's environment is a
    // hundred-odd variables, and every spawn built it.
    if !seen.insert(environment_name_key(&name)) {
        return;
    }
    if let Some(value) = value {
        entries.push((name, value));
    }
}

fn process_environment_prefix(environment: Value) -> Vec<(OsString, Option<OsString>)> {
    let mut entries = Vec::new();
    let mut tail = environment;
    while tail.is_cons() {
        let car = tail.cons_car();
        let Some(string) = car.as_lisp_string() else {
            break;
        };
        entries.push(split_environment_entry(string));
        tail = tail.cons_cdr();
    }
    entries
}

fn frame_x_display_value(frame: &crate::window::Frame) -> Option<Value> {
    match frame.display_identity() {
        crate::window::FrameDisplayIdentity::Graphical(identity) => {
            identity.x_display().map(Value::string)
        }
        crate::window::FrameDisplayIdentity::None => frame
            .parameter("display")
            .filter(|value| value.as_lisp_string().is_some()),
    }
}

fn selected_frame_display_value(eval: &Context) -> Option<Value> {
    eval.frames.selected_frame().and_then(frame_x_display_value)
}

fn corrected_pwd(current_dir: Option<&Path>) -> Option<OsString> {
    let directory = current_dir
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())?;
    let mut value = directory.into_os_string();

    // GNU removes trailing directory separators while preserving root.
    std::cfg_select! {
        unix => {
            use std::os::unix::ffi::OsStrExt;
            let bytes = value.as_os_str().as_bytes();
            let keep = bytes
                .iter()
                .rposition(|byte| *byte != b'/')
                .map_or(bytes.len(), |last| (last + 1).max(1));
            value = OsString::from_vec(bytes[..keep].to_vec());
        }
        _ => {}
    }

    Some(value)
}

impl ChildEnvironment {
    /// Materialize the exact environment passed to a child process.
    ///
    /// This is the sole equivalent of GNU `make_environment_block`: it
    /// corrects `PWD`, injects the selected frame's `DISPLAY` when policy does
    /// not mention it, preserves first-definition precedence, and removes bare
    /// negative entries.
    pub(crate) fn materialize(eval: &Context, current_dir: Option<&Path>) -> Self {
        let process_environment = eval.visible_variable_value_or_nil("process-environment");
        let process_entries = process_environment_prefix(process_environment);
        let mut entries = Vec::with_capacity(process_entries.len() + 2);
        let mut seen = rustc_hash::FxHashSet::default();
        seen.reserve(process_entries.len() + 2);

        if matches!(
            lookup_environment_list(&LispString::from_utf8("PWD"), process_environment),
            EnvironmentLookup::Value(_)
        ) {
            push_unique_environment_entry(
                &mut entries,
                &mut seen,
                OsString::from("PWD"),
                corrected_pwd(current_dir),
            );
        }

        let display_name = OsString::from("DISPLAY");
        let display_is_explicit = process_entries
            .iter()
            .any(|(name, _)| os_environment_name_eq(name, &display_name));
        if !display_is_explicit {
            let display = selected_frame_display_value(eval).or_else(|| {
                let initial_environment = eval.visible_variable_value_or_nil("initial-environment");
                match lookup_environment_list(
                    &LispString::from_utf8("DISPLAY"),
                    initial_environment,
                ) {
                    EnvironmentLookup::Value(value) => Some(value),
                    EnvironmentLookup::Negative | EnvironmentLookup::Missing => None,
                }
            });
            if let Some(display) = display.and_then(|value| value.as_lisp_string().cloned()) {
                push_unique_environment_entry(
                    &mut entries,
                    &mut seen,
                    display_name,
                    Some(lisp_bytes_to_os_string(display.as_bytes())),
                );
            }
        }

        for (name, value) in process_entries {
            push_unique_environment_entry(&mut entries, &mut seen, name, value);
        }

        Self {
            entries: entries.into(),
        }
    }

    pub(crate) fn apply_to_child_command(
        &self,
        command: &mut crate::emacs_core::callproc::ChildCommand,
    ) {
        command.set_exact_env(std::sync::Arc::clone(&self.entries));
    }

    #[cfg(unix)]
    pub(crate) fn apply_to_pty_command(&self, command: &mut portable_pty::CommandBuilder) {
        command.env_clear();
        for (name, value) in self.entries.iter() {
            command.env(name, value);
        }
    }
}

/// Search an Emacs environment list using GNU's first-match semantics.
///
/// String entries have the form `NAME=VALUE`; a bare `NAME` is an explicit
/// negative entry that suppresses all fallback.
pub(crate) fn lookup_environment_list(
    varname: &LispString,
    environment: Value,
) -> EnvironmentLookup {
    let var_bytes = varname.as_bytes();
    let mut tail = environment;
    while tail.is_cons() {
        let entry = tail.cons_car();
        if let Some(string) = entry.as_lisp_string() {
            let bytes = string.as_bytes();
            if bytes.len() >= var_bytes.len()
                && environment_name_eq(&bytes[..var_bytes.len()], var_bytes)
            {
                if bytes.len() > var_bytes.len() && bytes[var_bytes.len()] == b'=' {
                    return EnvironmentLookup::Value(Value::heap_string(environment_value_string(
                        string,
                        var_bytes.len() + 1,
                    )));
                }
                if bytes.len() == var_bytes.len() {
                    return EnvironmentLookup::Negative;
                }
            }
        }
        tail = tail.cons_cdr();
    }
    EnvironmentLookup::Missing
}

fn selected_frame_display(eval: &mut Context, frame: Value) -> EvalResult {
    let selected = if frame.is_nil() {
        eval.frames.selected_frame()
    } else {
        frame
            .as_frame_id()
            .and_then(|frame_id| eval.frames.get(crate::window::FrameId(frame_id)))
    };
    if let Some(frame) = selected {
        // A native Wayland display is not an X DISPLAY. GNU's PGTK path
        // deliberately ignores it and falls through to the startup
        // environment.
        return Ok(frame_x_display_value(frame).unwrap_or(Value::NIL));
    }

    super::frame::builtin_frame_parameter(eval, vec![frame, Value::symbol("display")])
}

/// Resolve `getenv-internal` through GNU's environment policy.
pub(crate) fn getenv_internal(
    eval: &mut Context,
    varname: &LispString,
    environment_or_frame: Value,
) -> EvalResult {
    if environment_or_frame.is_cons() {
        return Ok(
            match lookup_environment_list(varname, environment_or_frame) {
                EnvironmentLookup::Value(value) => value,
                EnvironmentLookup::Negative => Value::T,
                EnvironmentLookup::Missing => Value::NIL,
            },
        );
    }

    let process_environment = eval.visible_variable_value_or_nil("process-environment");
    match lookup_environment_list(varname, process_environment) {
        EnvironmentLookup::Value(value) => return Ok(value),
        EnvironmentLookup::Negative => return Ok(Value::NIL),
        EnvironmentLookup::Missing => {}
    }

    std::cfg_select! {
        windows => {
            // GNU's Windows port repairs a few native variables without
            // recording those changes in `process-environment`.
            let name = String::from_utf8_lossy(varname.as_bytes());
            if let Some(value) = std::env::var_os(name.as_ref()) {
                return Ok(Value::string(value.to_string_lossy()));
            }
        }
        _ => {}
    }

    if varname.as_bytes() == b"DISPLAY" {
        let display = selected_frame_display(eval, environment_or_frame)?;
        if display.as_lisp_string().is_some() {
            return Ok(display);
        }

        let initial_environment = eval.visible_variable_value_or_nil("initial-environment");
        return Ok(
            match lookup_environment_list(varname, initial_environment) {
                EnvironmentLookup::Value(value) => value,
                EnvironmentLookup::Negative | EnvironmentLookup::Missing => Value::NIL,
            },
        );
    }

    Ok(Value::NIL)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
