//! Lisp-visible and child-process environment policy.
//!
//! GNU Emacs deliberately separates the editor's mutable
//! `process-environment` from the native process environment. Missing
//! variables normally stay missing; `DISPLAY` is the exception because it is
//! associated with the selected GUI frame and falls back to the immutable
//! startup snapshot.

use super::error::{EvalResult, LispCondition, expect_min_args, signal};
use super::eval::Context;
use super::value::Value;
use crate::heap_types::LispString;
use neovm_host_abi::{HostKind, ProcessEnvironmentModel};
use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

pub(crate) enum EnvironmentLookup {
    Value(Value),
    Negative,
    Missing,
}

fn inherited_native_process_environment() -> Vec<(String, String)> {
    std::cfg_select! {
        target_family = "wasm" => {
            panic!("a WebAssembly target cannot inherit a native process environment")
        }
        _ => {
            std::env::vars().collect()
        }
    }
}

fn repair_native_process_environment(entries: &mut Vec<(String, String)>) {
    std::cfg_select! {
        windows => {
            // Mirror GNU `w32.c init_environment`: guarantee HOME is set on
            // Windows, where the OS environment typically omits it.
            if !entries.iter().any(|(k, _)| k.eq_ignore_ascii_case("HOME")) {
                let home = std::env::var("APPDATA")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "C:/".to_string());
                entries.push(("HOME".to_string(), home));
            }

            // GNU also guarantees SHELL before Lisp snapshots the process
            // environment. Keep it on the same private cmdproxy path as
            // `shell-file-name`.
            if !entries
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("SHELL") && !value.is_empty())
            {
                let shell = super::shell_file_name::resolve_current();
                entries.retain(|(name, _)| !name.eq_ignore_ascii_case("SHELL"));
                entries.push(("SHELL".to_owned(), shell.lisp_name().to_owned()));
            }
        }
        _ => {
            let _ = entries;
        }
    }
}

fn host_process_environment(host: HostKind) -> Value {
    let mut entries = match host.process_environment() {
        ProcessEnvironmentModel::InheritedNative => inherited_native_process_environment(),
        ProcessEnvironmentModel::Empty => Vec::new(),
    };
    repair_native_process_environment(&mut entries);

    Value::list(
        entries
            .into_iter()
            .map(|(name, value)| Value::string(format!("{name}={value}")))
            .collect::<Vec<_>>(),
    )
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
    let process_environment = host_process_environment(eval.host_kind);
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
    entries: Vec<(OsString, OsString)>,
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

fn push_unique_environment_entry(
    entries: &mut Vec<(OsString, OsString)>,
    seen: &mut Vec<OsString>,
    name: OsString,
    value: Option<OsString>,
) {
    if seen
        .iter()
        .any(|existing| os_environment_name_eq(existing, &name))
    {
        return;
    }
    seen.push(name.clone());
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
        crate::window::FrameDisplayIdentity::X11(display) => Some(Value::string(display)),
        crate::window::FrameDisplayIdentity::Wayland(_) => None,
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
        let mut seen = Vec::with_capacity(process_entries.len() + 2);

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

        Self { entries }
    }

    pub(crate) fn apply_to_child_command(
        &self,
        command: &mut crate::emacs_core::callproc::ChildCommand,
    ) {
        command.env_clear();
        command.envs(self.entries.iter().map(|(name, value)| (name, value)));
    }

    #[cfg(unix)]
    pub(crate) fn apply_to_pty_command(&self, command: &mut portable_pty::CommandBuilder) {
        command.env_clear();
        for (name, value) in &self.entries {
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

/// (getenv-internal VARIABLE &optional ENV) -> string-or-nil
///
/// GNU `callproc.c` `Fgetenv_internal`. Defined for every host: the process
/// environment is host state, not process control, so both process backends
/// re-export this one implementation.
pub(crate) fn builtin_getenv_internal(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("getenv-internal", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("getenv-internal"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // `getenv_internal` takes `&mut Context`, so a borrow of VARNAME's payload
    // would span it. The name is short and this is not a hot path, so copy the
    // bytes out rather than reason about whether the callee can reach a
    // safepoint (DIVERGENCES.md 163).
    let varname = eval.expect_lisp_string(args[0])?.clone();
    getenv_internal(eval, &varname, args.get(1).copied().unwrap_or(Value::NIL))
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
