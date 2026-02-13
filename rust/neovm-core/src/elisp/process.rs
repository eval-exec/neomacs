//! Process/subprocess management for the Elisp VM.
//!
//! Provides process abstractions: creating, killing, querying, and
//! communicating with subprocesses.  `start-process` creates a tracked
//! record; `call-process` and `shell-command-to-string` run real OS
//! commands via `std::process::Command`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::process::{Command, Stdio};

use super::error::{signal, EvalResult, Flow};
use super::value::{list_to_vec, Value};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for a process.
pub type ProcessId = u64;

/// Status of a managed process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    Run,
    Stop,
    Exit(i32),
    Signal(i32),
}

/// A tracked process record.
#[derive(Clone, Debug)]
pub struct Process {
    pub id: ProcessId,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: ProcessStatus,
    pub buffer_name: Option<String>,
    /// Queued input (sent via `process-send-string`).
    pub stdin_queue: String,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
}

/// Manages the set of live processes.
#[derive(Clone, Debug)]
pub struct ProcessManager {
    processes: HashMap<ProcessId, Process>,
    next_id: ProcessId,
    /// Environment variable overrides (for `setenv`/`getenv`).
    env_overrides: HashMap<String, Option<String>>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            next_id: 1,
            env_overrides: HashMap::new(),
        }
    }

    /// Create a new process record.  Returns the process id.
    pub fn create_process(
        &mut self,
        name: String,
        buffer_name: Option<String>,
        command: String,
        args: Vec<String>,
    ) -> ProcessId {
        let id = self.next_id;
        self.next_id += 1;
        let proc = Process {
            id,
            name,
            command,
            args,
            status: ProcessStatus::Run,
            buffer_name,
            stdin_queue: String::new(),
            stdout: String::new(),
            stderr: String::new(),
        };
        self.processes.insert(id, proc);
        id
    }

    /// Kill (remove) a process by id.  Returns true if found.
    pub fn kill_process(&mut self, id: ProcessId) -> bool {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.status = ProcessStatus::Signal(9);
            true
        } else {
            false
        }
    }

    /// Delete a process entirely.
    pub fn delete_process(&mut self, id: ProcessId) -> bool {
        self.processes.remove(&id).is_some()
    }

    /// Get process status.
    pub fn process_status(&self, id: ProcessId) -> Option<&ProcessStatus> {
        self.processes.get(&id).map(|p| &p.status)
    }

    /// Get a process by id.
    pub fn get(&self, id: ProcessId) -> Option<&Process> {
        self.processes.get(&id)
    }

    /// List all process ids.
    pub fn list_processes(&self) -> Vec<ProcessId> {
        self.processes.keys().copied().collect()
    }

    /// Find a process by name.
    pub fn find_by_name(&self, name: &str) -> Option<ProcessId> {
        self.processes
            .values()
            .find(|p| p.name == name)
            .map(|p| p.id)
    }

    /// Queue input for a process.
    pub fn send_input(&mut self, id: ProcessId, input: &str) -> bool {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.stdin_queue.push_str(input);
            true
        } else {
            false
        }
    }

    /// Get stdout output from a process.
    pub fn get_output(&self, id: ProcessId) -> Option<&str> {
        self.processes.get(&id).map(|p| p.stdout.as_str())
    }

    /// Get an environment variable (checking overrides first, then OS).
    pub fn getenv(&self, name: &str) -> Option<String> {
        if let Some(override_val) = self.env_overrides.get(name) {
            return override_val.clone();
        }
        std::env::var(name).ok()
    }

    /// Set an environment variable override.  If value is None, unset it.
    pub fn setenv(&mut self, name: String, value: Option<String>) {
        self.env_overrides.insert(name, value);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn expect_int_or_marker(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        v if super::marker::is_marker(v) => super::marker::marker_position_as_int(v),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integer-or-marker-p"), other.clone()],
        )),
    }
}

fn file_error_symbol(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "file-missing",
        std::io::ErrorKind::AlreadyExists => "file-already-exists",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        _ => "file-error",
    }
}

fn signal_process_io(action: &str, target: Option<&str>, err: std::io::Error) -> Flow {
    let mut data = vec![Value::string(action), Value::string(err.to_string())];
    if let Some(target) = target {
        data.push(Value::string(target));
    }
    signal(file_error_symbol(err.kind()), data)
}

#[derive(Clone, Debug)]
enum OutputTarget {
    Discard,
    Buffer(Value),
    File(String),
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
    stderr_file: Option<String>,
    no_wait: bool,
}

fn signal_wrong_type_string(value: Value) -> Flow {
    signal(
        "wrong-type-argument",
        vec![Value::symbol("stringp"), value],
    )
}

fn expect_string_strict(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal_wrong_type_string(other.clone())),
    }
}

fn is_file_keyword(value: &Value) -> bool {
    matches!(value, Value::Keyword(k) if k == ":file" || k == "file")
}

fn parse_file_target(items: &[Value]) -> Result<OutputTarget, Flow> {
    let file_value = items.get(1).cloned().unwrap_or(Value::Nil);
    let file = expect_string_strict(&file_value)?;
    Ok(OutputTarget::File(file))
}

fn parse_real_buffer_destination(
    eval: &super::eval::Evaluator,
    value: &Value,
) -> Result<(OutputTarget, bool), Flow> {
    match value {
        Value::Int(_) => Ok((OutputTarget::Discard, true)),
        Value::Nil => Ok((OutputTarget::Discard, false)),
        Value::True | Value::Str(_) => Ok((OutputTarget::Buffer(value.clone()), false)),
        Value::Buffer(id) => {
            if eval.buffers.get(*id).is_none() {
                Err(signal("error", vec![Value::string("Selecting deleted buffer")]))
            } else {
                Ok((OutputTarget::Buffer(value.clone()), false))
            }
        }
        Value::Cons(_) => {
            let items = list_to_vec(value).ok_or_else(|| signal_wrong_type_string(value.clone()))?;
            let first = items.first().cloned().unwrap_or(Value::Nil);
            if is_file_keyword(&first) {
                Ok((parse_file_target(&items)?, false))
            } else {
                Err(signal_wrong_type_string(first))
            }
        }
        other => Err(signal_wrong_type_string(other.clone())),
    }
}

fn parse_stderr_destination(value: &Value) -> Result<(StderrTarget, Option<String>), Flow> {
    match value {
        Value::Nil => Ok((StderrTarget::Discard, None)),
        Value::True => Ok((StderrTarget::ToStdoutTarget, None)),
        Value::Str(s) => Ok((StderrTarget::File, Some((**s).clone()))),
        other => Err(signal_wrong_type_string(other.clone())),
    }
}

fn parse_call_process_destination(
    eval: &super::eval::Evaluator,
    destination: &Value,
) -> Result<DestinationSpec, Flow> {
    if let Value::Cons(_) = destination {
        let items =
            list_to_vec(destination).ok_or_else(|| signal_wrong_type_string(destination.clone()))?;
        let first = items.first().cloned().unwrap_or(Value::Nil);
        if is_file_keyword(&first) {
            let stdout = parse_file_target(&items)?;
            return Ok(DestinationSpec {
                stdout,
                stderr: StderrTarget::ToStdoutTarget,
                stderr_file: None,
                no_wait: false,
            });
        }
        let second = items.get(1).cloned().unwrap_or(Value::Nil);
        let (stdout, no_wait) = parse_real_buffer_destination(eval, &first)?;
        let (stderr, stderr_file) = parse_stderr_destination(&second)?;
        return Ok(DestinationSpec {
            stdout,
            stderr,
            stderr_file,
            no_wait,
        });
    }

    let (stdout, no_wait) = parse_real_buffer_destination(eval, destination)?;
    let stderr = match destination {
        Value::Nil | Value::Int(_) => StderrTarget::Discard,
        _ => StderrTarget::ToStdoutTarget,
    };
    Ok(DestinationSpec {
        stdout,
        stderr,
        stderr_file: None,
        no_wait,
    })
}

fn insert_process_output(
    eval: &mut super::eval::Evaluator,
    destination: &Value,
    output: &str,
) -> Result<(), Flow> {
    match destination {
        Value::Str(name) => {
            let id = eval
                .buffers
                .find_buffer_by_name(name)
                .unwrap_or_else(|| eval.buffers.create_buffer(name));
            let buf = eval.buffers.get_mut(id).ok_or_else(|| {
                signal("error", vec![Value::string("No such live buffer for process output")])
            })?;
            buf.insert(output);
            Ok(())
        }
        Value::Buffer(id) => {
            let buf = eval.buffers.get_mut(*id).ok_or_else(|| {
                signal("error", vec![Value::string("Selecting deleted buffer")])
            })?;
            buf.insert(output);
            Ok(())
        }
        _ => {
            if let Some(buf) = eval.buffers.current_buffer_mut() {
                buf.insert(output);
            }
            Ok(())
        }
    }
}

fn write_output_target(
    eval: &mut super::eval::Evaluator,
    target: &OutputTarget,
    output: &[u8],
    append: bool,
) -> Result<(), Flow> {
    match target {
        OutputTarget::Discard => Ok(()),
        OutputTarget::Buffer(destination) => {
            let text = String::from_utf8_lossy(output).into_owned();
            insert_process_output(eval, destination, &text)
        }
        OutputTarget::File(path) => {
            if append {
                use std::io::Write;
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| signal_process_io("Writing process output", Some(path), e))?;
                file.write_all(output)
                    .map_err(|e| signal_process_io("Writing process output", Some(path), e))
            } else {
                std::fs::write(path, output)
                    .map_err(|e| signal_process_io("Writing process output", Some(path), e))
            }
        }
    }
}

fn route_captured_output(
    eval: &mut super::eval::Evaluator,
    destination: &DestinationSpec,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), Flow> {
    write_output_target(eval, &destination.stdout, stdout, false)?;
    match destination.stderr {
        StderrTarget::Discard => Ok(()),
        StderrTarget::ToStdoutTarget => {
            write_output_target(eval, &destination.stdout, stderr, true)
        }
        StderrTarget::File => {
            let path = destination
                .stderr_file
                .as_ref()
                .ok_or_else(|| signal("error", vec![Value::string("Missing stderr file target")]))?
                .clone();
            write_output_target(eval, &OutputTarget::File(path), stderr, false)
        }
    }
}

fn configure_call_process_stdin(command: &mut Command, infile: Option<&str>) -> Result<(), Flow> {
    match infile {
        None => {
            command.stdin(Stdio::null());
            Ok(())
        }
        Some(path) => {
            let file = std::fs::File::open(path)
                .map_err(|e| signal_process_io("Opening process input file", Some(path), e))?;
            command.stdin(Stdio::from(file));
            Ok(())
        }
    }
}

/// Resolve a process argument: either a ProcessId integer or a name string.
fn resolve_process(eval: &super::eval::Evaluator, value: &Value) -> Result<ProcessId, Flow> {
    match value {
        Value::Int(n) => {
            let id = *n as ProcessId;
            if eval.processes.get(id).is_some() {
                Ok(id)
            } else {
                Err(signal(
                    "error",
                    vec![Value::string(format!("No process {}", n))],
                ))
            }
        }
        Value::Str(s) => eval.processes.find_by_name(s).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(format!("No process named {}", s))],
            )
        }),
        Value::Symbol(s) => eval.processes.find_by_name(s).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(format!("No process named {}", s))],
            )
        }),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("processp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins (eval-dependent)
// ---------------------------------------------------------------------------

/// (start-process NAME BUFFER PROGRAM &rest ARGS) -> process-id
pub(crate) fn builtin_start_process(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("start-process", &args, 3)?;
    let name = expect_string(&args[0])?;
    let buffer = if args[1].is_nil() {
        None
    } else {
        Some(expect_string(&args[1])?)
    };
    let program = expect_string(&args[2])?;
    let proc_args: Vec<String> = args[3..]
        .iter()
        .map(expect_string)
        .collect::<Result<Vec<_>, _>>()?;

    let id = eval
        .processes
        .create_process(name, buffer, program, proc_args);
    Ok(Value::Int(id as i64))
}

/// (call-process PROGRAM &optional INFILE DESTINATION DISPLAY &rest ARGS)
///
/// Runs the command synchronously using `std::process::Command`, captures
/// output.  Returns the exit code as an integer.
pub(crate) fn builtin_call_process(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("call-process", &args, 1)?;
    let program = expect_string(&args[0])?;

    let infile = if args.len() > 1 && !args[1].is_nil() {
        Some(expect_string_strict(&args[1])?)
    } else {
        None
    };

    let destination = if args.len() > 2 {
        &args[2]
    } else {
        &Value::Nil
    };
    let destination_spec = parse_call_process_destination(eval, destination)?;

    // DISPLAY (arg index 3): ignored in this implementation.

    let cmd_args: Vec<String> = if args.len() > 4 {
        args[4..]
            .iter()
            .map(expect_string)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    if destination_spec.no_wait {
        let mut command = Command::new(&program);
        command.args(&cmd_args).stdout(Stdio::null());
        configure_call_process_stdin(&mut command, infile.as_deref())?;
        match destination_spec.stderr {
            StderrTarget::Discard | StderrTarget::ToStdoutTarget => {
                command.stderr(Stdio::null());
            }
            StderrTarget::File => {
                let path = destination_spec
                    .stderr_file
                    .as_ref()
                    .ok_or_else(|| signal("error", vec![Value::string("Missing stderr file target")]))?;
                let file = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(path)
                    .map_err(|e| signal_process_io("Writing process output", Some(path), e))?;
                command.stderr(Stdio::from(file));
            }
        };

        let mut child = command
            .spawn()
            .map_err(|e| signal_process_io("Searching for program", Some(&program), e))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(Value::Nil);
    }

    let mut command = Command::new(&program);
    command
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_call_process_stdin(&mut command, infile.as_deref())?;
    let output = command
        .output()
        .map_err(|e| signal_process_io("Searching for program", Some(&program), e))?;

    let exit_code = output.status.code().unwrap_or(-1);
    route_captured_output(eval, &destination_spec, &output.stdout, &output.stderr)?;
    Ok(Value::Int(exit_code as i64))
}

/// (call-process-region START END PROGRAM &optional DELETE DESTINATION DISPLAY &rest ARGS)
///
/// Pipes buffer region from START to END through PROGRAM.
pub(crate) fn builtin_call_process_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("call-process-region", &args, 3)?;
    let program = expect_string(&args[2])?;

    let delete = args.len() > 3 && args[3].is_truthy();
    let destination = if args.len() > 4 {
        &args[4]
    } else {
        &Value::Nil
    };
    let destination_spec = parse_call_process_destination(eval, destination)?;
    // DISPLAY (arg index 5): ignored.

    let cmd_args: Vec<String> = if args.len() > 6 {
        args[6..]
            .iter()
            .map(expect_string)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    // START semantics:
    // - nil => use whole buffer contents, ignore END
    // - string => use that string as stdin, ignore END
    // - integer/marker => use region START..END
    let region_text = match &args[0] {
        Value::Nil => {
            let (text, maybe_delete_range) = {
                let buf = eval
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let len = buf.text.len();
                (buf.text.text_range(0, len), (0usize, len))
            };
            if delete {
                let buf = eval
                    .buffers
                    .current_buffer_mut()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                buf.delete_region(maybe_delete_range.0, maybe_delete_range.1);
            }
            text
        }
        Value::Str(s) => {
            if delete {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integer-or-marker-p"), args[0].clone()],
                ));
            }
            (**s).clone()
        }
        _ => {
            let start = expect_int_or_marker(&args[0])? as usize;
            let end = expect_int_or_marker(&args[1])? as usize;
            let (text, region_beg, region_end) = {
                let buf = eval
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                // Convert 1-based Emacs positions to 0-based byte positions.
                let beg = (start.saturating_sub(1)).min(buf.text.len());
                let e = (end.saturating_sub(1)).min(buf.text.len());
                let (region_beg, region_end) = if beg <= e { (beg, e) } else { (e, beg) };
                (
                    buf.text.text_range(region_beg, region_end),
                    region_beg,
                    region_end,
                )
            };

            if delete {
                let buf = eval
                    .buffers
                    .current_buffer_mut()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                buf.delete_region(region_beg, region_end);
            }

            text
        }
    };

    use std::io::Write;
    if destination_spec.no_wait {
        let mut command = Command::new(&program);
        command.args(&cmd_args).stdin(Stdio::piped()).stdout(Stdio::null());
        match destination_spec.stderr {
            StderrTarget::Discard | StderrTarget::ToStdoutTarget => {
                command.stderr(Stdio::null());
            }
            StderrTarget::File => {
                let path = destination_spec
                    .stderr_file
                    .as_ref()
                    .ok_or_else(|| signal("error", vec![Value::string("Missing stderr file target")]))?;
                let file = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(path)
                    .map_err(|e| signal_process_io("Writing process output", Some(path), e))?;
                command.stderr(Stdio::from(file));
            }
        };

        let mut child = command
            .spawn()
            .map_err(|e| signal_process_io("Searching for program", Some(&program), e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(region_text.as_bytes());
        }

        std::thread::spawn(move || {
            let _ = child.wait();
        });

        return Ok(Value::Nil);
    }

    let mut child = Command::new(&program)
        .args(&cmd_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| signal_process_io("Searching for program", Some(&program), e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(region_text.as_bytes());
    }

    let output = child
        .wait_with_output()
        .map_err(|e| signal_process_io("Process error", None, e))?;

    let exit_code = output.status.code().unwrap_or(-1);
    route_captured_output(eval, &destination_spec, &output.stdout, &output.stderr)?;
    Ok(Value::Int(exit_code as i64))
}

/// (delete-process PROCESS) -> nil
pub(crate) fn builtin_delete_process(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-process", &args, 1)?;
    let id = resolve_process(eval, &args[0])?;
    eval.processes.delete_process(id);
    Ok(Value::Nil)
}

/// (process-send-string PROCESS STRING) -> nil
pub(crate) fn builtin_process_send_string(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-string", &args, 2)?;
    let id = resolve_process(eval, &args[0])?;
    let input = expect_string(&args[1])?;
    if !eval.processes.send_input(id, &input) {
        return Err(signal("error", vec![Value::string("Process not found")]));
    }
    Ok(Value::Nil)
}

/// (process-status PROCESS) -> symbol
pub(crate) fn builtin_process_status(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-status", &args, 1)?;
    let id = resolve_process(eval, &args[0])?;
    match eval.processes.process_status(id) {
        Some(ProcessStatus::Run) => Ok(Value::symbol("run")),
        Some(ProcessStatus::Stop) => Ok(Value::symbol("stop")),
        Some(ProcessStatus::Exit(_)) => Ok(Value::symbol("exit")),
        Some(ProcessStatus::Signal(_)) => Ok(Value::symbol("signal")),
        None => Err(signal("error", vec![Value::string("Process not found")])),
    }
}

/// (process-exit-status PROCESS) -> integer
pub(crate) fn builtin_process_exit_status(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-exit-status", &args, 1)?;
    let id = resolve_process(eval, &args[0])?;
    match eval.processes.process_status(id) {
        Some(ProcessStatus::Exit(code)) => Ok(Value::Int(*code as i64)),
        Some(ProcessStatus::Signal(sig)) => Ok(Value::Int(*sig as i64)),
        Some(_) => Ok(Value::Int(0)),
        None => Err(signal("error", vec![Value::string("Process not found")])),
    }
}

/// (process-list) -> list of process ids
pub(crate) fn builtin_process_list(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-list", &args, 0)?;
    let ids = eval.processes.list_processes();
    let values: Vec<Value> = ids.iter().map(|id| Value::Int(*id as i64)).collect();
    Ok(Value::list(values))
}

/// (process-name PROCESS) -> string
pub(crate) fn builtin_process_name(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-name", &args, 1)?;
    let id = resolve_process(eval, &args[0])?;
    match eval.processes.get(id) {
        Some(proc) => Ok(Value::string(proc.name.clone())),
        None => Err(signal("error", vec![Value::string("Process not found")])),
    }
}

/// (process-buffer PROCESS) -> string or nil
pub(crate) fn builtin_process_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-buffer", &args, 1)?;
    let id = resolve_process(eval, &args[0])?;
    match eval.processes.get(id) {
        Some(proc) => match &proc.buffer_name {
            Some(name) => Ok(Value::string(name.clone())),
            None => Ok(Value::Nil),
        },
        None => Err(signal("error", vec![Value::string("Process not found")])),
    }
}

// ---------------------------------------------------------------------------
// Builtins (pure — no evaluator needed)
// ---------------------------------------------------------------------------

/// (shell-command-to-string COMMAND) -> string
///
/// Runs COMMAND via the system shell and returns captured stdout.
pub(crate) fn builtin_shell_command_to_string(args: Vec<Value>) -> EvalResult {
    expect_args("shell-command-to-string", &args, 1)?;
    let command = expect_string(&args[0])?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let output = Command::new(&shell)
        .arg("-c")
        .arg(&command)
        .output()
        .map_err(|e| signal_process_io("Shell command failed", Some(&shell), e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(Value::string(stdout))
}

/// (getenv VARIABLE) -> string or nil
pub(crate) fn builtin_getenv(args: Vec<Value>) -> EvalResult {
    expect_args("getenv", &args, 1)?;
    let name = expect_string(&args[0])?;
    match std::env::var(&name) {
        Ok(val) => Ok(Value::string(val)),
        Err(_) => Ok(Value::Nil),
    }
}

/// (setenv VARIABLE &optional VALUE) -> string or nil
///
/// Sets the environment variable VARIABLE to VALUE.  If VALUE is nil
/// or omitted, removes the variable.
pub(crate) fn builtin_setenv(args: Vec<Value>) -> EvalResult {
    expect_min_args("setenv", &args, 1)?;
    let name = expect_string(&args[0])?;

    if args.len() > 1 && !args[1].is_nil() {
        let value = expect_string(&args[1])?;
        // Safety: this is single-threaded for the Elisp VM, so setting env
        // vars is acceptable.
        unsafe {
            std::env::set_var(&name, &value);
        }
        Ok(Value::string(value))
    } else {
        unsafe {
            std::env::remove_var(&name);
        }
        Ok(Value::Nil)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{format_eval_result, parse_forms, Evaluator};

    fn eval_one(src: &str) -> String {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        let result = ev.eval_expr(&forms[0]);
        format_eval_result(&result)
    }

    fn eval_all(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    /// Find the path of a binary, trying /bin, /usr/bin, and PATH lookup.
    fn find_bin(name: &str) -> String {
        for dir in &["/bin", "/usr/bin", "/run/current-system/sw/bin"] {
            let path = format!("{}/{}", dir, name);
            if std::path::Path::new(&path).exists() {
                return path;
            }
        }
        // Fallback: try to find via `which`
        if let Ok(output) = std::process::Command::new("which").arg(name).output() {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
        // Last resort: return the bare name and let Command search PATH
        name.to_string()
    }

    fn tmp_file(label: &str) -> String {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        format!("/tmp/neovm-{label}-{}-{nonce}.txt", std::process::id())
    }

    // -- ProcessManager unit tests ------------------------------------------

    #[test]
    fn process_manager_create_and_query() {
        let mut pm = ProcessManager::new();
        let id = pm.create_process(
            "test".into(),
            Some("*test*".into()),
            "/bin/echo".into(),
            vec!["hello".into()],
        );
        assert!(id > 0);
        assert!(pm.get(id).is_some());
        assert_eq!(pm.get(id).unwrap().name, "test");
        assert_eq!(pm.get(id).unwrap().command, "/bin/echo");
        assert_eq!(pm.process_status(id), Some(&ProcessStatus::Run));
    }

    #[test]
    fn process_manager_kill() {
        let mut pm = ProcessManager::new();
        let id = pm.create_process("p".into(), None, "prog".into(), vec![]);
        assert!(pm.kill_process(id));
        assert_eq!(pm.process_status(id), Some(&ProcessStatus::Signal(9)));
    }

    #[test]
    fn process_manager_delete() {
        let mut pm = ProcessManager::new();
        let id = pm.create_process("p".into(), None, "prog".into(), vec![]);
        assert!(pm.delete_process(id));
        assert!(pm.get(id).is_none());
    }

    #[test]
    fn process_manager_send_input() {
        let mut pm = ProcessManager::new();
        let id = pm.create_process("p".into(), None, "prog".into(), vec![]);
        assert!(pm.send_input(id, "hello "));
        assert!(pm.send_input(id, "world"));
        assert_eq!(pm.get(id).unwrap().stdin_queue, "hello world");
    }

    #[test]
    fn process_manager_find_by_name() {
        let mut pm = ProcessManager::new();
        let id = pm.create_process("my-proc".into(), None, "prog".into(), vec![]);
        assert_eq!(pm.find_by_name("my-proc"), Some(id));
        assert_eq!(pm.find_by_name("nonexistent"), None);
    }

    #[test]
    fn process_manager_list() {
        let mut pm = ProcessManager::new();
        let id1 = pm.create_process("a".into(), None, "p".into(), vec![]);
        let id2 = pm.create_process("b".into(), None, "q".into(), vec![]);
        let ids = pm.list_processes();
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn process_manager_env() {
        let mut pm = ProcessManager::new();
        pm.setenv("NEOVM_TEST_VAR".into(), Some("hello".into()));
        assert_eq!(pm.getenv("NEOVM_TEST_VAR"), Some("hello".into()));
        pm.setenv("NEOVM_TEST_VAR".into(), None);
        assert_eq!(pm.getenv("NEOVM_TEST_VAR"), None);
    }

    // -- Elisp-level tests --------------------------------------------------

    #[test]
    fn start_process_and_query() {
        let echo = find_bin("echo");
        let results = eval_all(&format!(
            r#"(start-process "my-proc" nil "{echo}" "hello")
               (process-status 1)
               (process-name 1)
               (process-buffer 1)"#,
        ));
        assert_eq!(results[0], "OK 1");
        assert_eq!(results[1], "OK run");
        assert_eq!(results[2], r#"OK "my-proc""#);
        assert_eq!(results[3], "OK nil");
    }

    #[test]
    fn start_process_with_buffer() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(start-process "p" "*output*" "{cat}")
               (process-buffer 1)"#,
        ));
        assert_eq!(results[1], r#"OK "*output*""#);
    }

    #[test]
    fn delete_process_removes() {
        let echo = find_bin("echo");
        let results = eval_all(&format!(
            r#"(start-process "p" nil "{echo}")
               (delete-process 1)
               (process-list)"#,
        ));
        assert_eq!(results[2], "OK nil");
    }

    #[test]
    fn process_send_string_test() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(start-process "p" nil "{cat}")
               (process-send-string 1 "hello")"#,
        ));
        assert_eq!(results[1], "OK nil");
    }

    #[test]
    fn process_exit_status_initial() {
        let echo = find_bin("echo");
        let results = eval_all(&format!(
            r#"(start-process "p" nil "{echo}")
               (process-exit-status 1)"#,
        ));
        assert_eq!(results[1], "OK 0");
    }

    #[test]
    fn process_list_test() {
        let echo = find_bin("echo");
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(start-process "a" nil "{echo}")
               (start-process "b" nil "{cat}")
               (process-list)"#,
        ));
        // Process list contains two entries.  Order may vary.
        let list_str = &results[2];
        assert!(list_str.contains("1"));
        assert!(list_str.contains("2"));
    }

    #[test]
    fn call_process_echo() {
        let echo = find_bin("echo");
        // call-process with echo, inserting into current buffer
        let results = eval_all(&format!(
            r#"(get-buffer-create "cp-test")
               (set-buffer "cp-test")
               (call-process "{echo}" nil t nil "hello" "world")
               (buffer-string)"#,
        ));
        // Exit code should be 0.
        assert_eq!(results[2], "OK 0");
        // Buffer should contain "hello world\n".
        assert_eq!(results[3], r#"OK "hello world\n""#);
    }

    #[test]
    fn call_process_no_destination() {
        let echo = find_bin("echo");
        // call-process with nil destination discards output
        let results = eval_all(&format!(
            r#"(get-buffer-create "cp-nil")
               (set-buffer "cp-nil")
               (call-process "{echo}" nil nil nil "hello")
               (buffer-string)"#,
        ));
        assert_eq!(results[2], "OK 0");
        assert_eq!(results[3], r#"OK """#);
    }

    #[test]
    fn call_process_infile_feeds_stdin() {
        let cat = find_bin("cat");
        let infile = tmp_file("cp-infile");
        std::fs::write(&infile, "infile-data").expect("write infile");
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (list
                   (call-process "{cat}" "{infile}" t nil)
                   (buffer-string)))"#
        ));
        assert_eq!(results[0], r#"OK (0 "infile-data")"#);
        let _ = std::fs::remove_file(&infile);
    }

    #[test]
    fn call_process_destination_buffer_name_inserts_there() {
        let echo = find_bin("echo");
        let results = eval_all(&format!(
            r#"(get-buffer-create "cp-src")
               (get-buffer-create "cp-dst")
               (set-buffer "cp-src")
               (erase-buffer)
               (set-buffer "cp-dst")
               (erase-buffer)
               (set-buffer "cp-src")
               (call-process "{echo}" nil "cp-dst" nil "hello")
               (list
                 (with-current-buffer "cp-src" (buffer-string))
                 (with-current-buffer "cp-dst" (buffer-string)))"#,
        ));
        assert_eq!(results[7], "OK 0");
        assert_eq!(results[8], r#"OK ("" "hello\n")"#);
    }

    #[test]
    fn call_process_file_destination_collects_stdout_and_stderr() {
        let sh = find_bin("sh");
        let out = tmp_file("cp-file");
        let _ = std::fs::remove_file(&out);
        let results = eval_all(&format!(
            r#"(call-process "{sh}" nil '(:file "{out}") nil "-c" "echo out; echo err >&2")
               (with-temp-buffer (insert-file-contents "{out}") (buffer-string))"#
        ));
        assert_eq!(results[0], "OK 0");
        assert!(results[1].contains("out"));
        assert!(results[1].contains("err"));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn call_process_pair_destination_splits_stderr_to_file() {
        let sh = find_bin("sh");
        let out = tmp_file("cp-pair-out");
        let err = tmp_file("cp-pair-err");
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&err);
        let results = eval_all(&format!(
            r#"(call-process "{sh}" nil '((:file "{out}") "{err}") nil "-c" "echo out; echo err >&2")
               (with-temp-buffer (insert-file-contents "{out}") (buffer-string))
               (with-temp-buffer (insert-file-contents "{err}") (buffer-string))"#
        ));
        assert_eq!(results[0], "OK 0");
        assert!(results[1].contains("out"));
        assert!(!results[1].contains("err"));
        assert!(results[2].contains("err"));
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&err);
    }

    #[test]
    fn call_process_integer_destination_returns_nil() {
        let echo = find_bin("echo");
        // Any integer destination behaves like 0: discard and return nil.
        let results = eval_all(&format!(
            r#"(get-buffer-create "cp-int")
               (set-buffer "cp-int")
               (call-process "{echo}" nil 2 nil "hello")
               (buffer-string)"#,
        ));
        assert_eq!(results[2], "OK nil");
        assert_eq!(results[3], r#"OK """#);
    }

    #[test]
    fn call_process_false() {
        let false_bin = find_bin("false");
        // false exits with code 1
        let result = eval_one(&format!(r#"(call-process "{false_bin}")"#));
        assert_eq!(result, "OK 1");
    }

    #[test]
    fn call_process_region_test() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(get-buffer-create "cpr-test")
               (set-buffer "cpr-test")
               (insert "hello world")
               (call-process-region 1 12 "{cat}" nil t)
               (buffer-string)"#,
        ));
        // exit code 0
        assert_eq!(results[3], "OK 0");
        // Buffer should contain original text plus piped output
        assert!(results[4].contains("hello world"));
    }

    #[test]
    fn call_process_region_destination_buffer_name_inserts_there() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(get-buffer-create "cpr-src")
               (get-buffer-create "cpr-dst")
               (with-current-buffer "cpr-src" (erase-buffer) (insert "abc"))
               (with-current-buffer "cpr-dst" (erase-buffer))
               (with-current-buffer "cpr-src"
                 (call-process-region (point-min) (point-max) "{cat}" nil "cpr-dst" nil))
               (list
                 (with-current-buffer "cpr-src" (buffer-string))
                 (with-current-buffer "cpr-dst" (buffer-string)))"#,
        ));
        assert_eq!(results[4], "OK 0");
        assert_eq!(results[5], r#"OK ("abc" "abc")"#);
    }

    #[test]
    fn call_process_region_file_destination_writes_file() {
        let cat = find_bin("cat");
        let out = tmp_file("cpr-file");
        let _ = std::fs::remove_file(&out);
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (insert "abc")
                 (call-process-region (point-min) (point-max) "{cat}" nil '(:file "{out}") nil))
               (with-temp-buffer (insert-file-contents "{out}") (buffer-string))"#
        ));
        assert_eq!(results[0], "OK 0");
        assert_eq!(results[1], r#"OK "abc""#);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn call_process_region_start_nil_uses_whole_buffer() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (insert "abc")
                 (list (call-process-region nil nil "{cat}" nil t nil)
                       (buffer-string)))"#
        ));
        assert_eq!(results[0], r#"OK (0 "abcabc")"#);
    }

    #[test]
    fn call_process_region_start_string_uses_string_input() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (insert "abc")
                 (list (call-process-region "xyz" nil "{cat}" nil t nil)
                       (buffer-string)))"#
        ));
        assert_eq!(results[0], r#"OK (0 "abcxyz")"#);
    }

    #[test]
    fn call_process_region_start_string_with_delete_signals_wrong_type() {
        let cat = find_bin("cat");
        let result = eval_one(&format!(
            r#"(condition-case err
                   (call-process-region "xyz" nil "{cat}" t t nil)
                 (error (car err)))"#
        ));
        assert_eq!(result, "OK wrong-type-argument");
    }

    #[test]
    fn call_process_region_accepts_marker_positions() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (insert "abcdef")
                 (goto-char 3)
                 (let ((m (copy-marker (point))))
                   (list (call-process-region m (point-max) "{cat}" nil t nil)
                         (buffer-string))))"#
        ));
        assert_eq!(results[0], r#"OK (0 "abcdefcdef")"#);
    }

    #[test]
    fn call_process_region_reversed_bounds_are_accepted() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (insert "abc")
                 (list (call-process-region (point-max) (point-min) "{cat}" nil t nil)
                       (buffer-string)))"#
        ));
        assert_eq!(results[0], r#"OK (0 "abcabc")"#);
    }

    #[test]
    fn call_process_region_reversed_bounds_with_delete_delete_region() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(with-temp-buffer
                 (insert "abc")
                 (list (call-process-region (point-max) (point-min) "{cat}" t t nil)
                       (buffer-string)))"#
        ));
        assert_eq!(results[0], r#"OK (0 "abc")"#);
    }

    #[test]
    fn call_process_region_integer_destination_returns_nil() {
        let cat = find_bin("cat");
        let results = eval_all(&format!(
            r#"(get-buffer-create "cpr-int")
               (set-buffer "cpr-int")
               (erase-buffer)
               (insert "abc")
               (call-process-region 1 4 "{cat}" nil 3 nil)
               (buffer-string)"#,
        ));
        assert_eq!(results[4], "OK nil");
        assert_eq!(results[5], r#"OK "abc""#);
    }

    #[test]
    fn shell_command_to_string_test() {
        let result = eval_one(r#"(shell-command-to-string "echo -n hello")"#);
        assert_eq!(result, r#"OK "hello""#);
    }

    #[test]
    fn shell_command_to_string_with_pipe() {
        let result = eval_one(r#"(shell-command-to-string "echo hello | tr a-z A-Z")"#);
        assert_eq!(result, r#"OK "HELLO\n""#);
    }

    #[test]
    fn getenv_path() {
        // PATH should always be set
        let result = eval_one(r#"(getenv "PATH")"#);
        assert!(result.starts_with("OK \""));
    }

    #[test]
    fn getenv_nonexistent() {
        let result = eval_one(r#"(getenv "NEOVM_DEFINITELY_NOT_SET_12345")"#);
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn setenv_and_getenv() {
        let results = eval_all(
            r#"(setenv "NEOVM_TEST_SETENV" "myvalue")
               (getenv "NEOVM_TEST_SETENV")"#,
        );
        assert_eq!(results[0], r#"OK "myvalue""#);
        assert_eq!(results[1], r#"OK "myvalue""#);
    }

    #[test]
    fn setenv_unset() {
        let results = eval_all(
            r#"(setenv "NEOVM_TEST_UNSET" "val")
               (setenv "NEOVM_TEST_UNSET")
               (getenv "NEOVM_TEST_UNSET")"#,
        );
        assert_eq!(results[2], "OK nil");
    }

    #[test]
    fn call_process_bad_program() {
        let result = eval_one(r#"(call-process "/nonexistent/program_xyz")"#);
        assert!(result.contains("ERR"));
    }

    #[test]
    fn call_process_bad_program_signals_file_missing() {
        let result = eval_one(
            r#"(condition-case err (call-process "/nonexistent/program_xyz") (error (car err)))"#,
        );
        assert_eq!(result, "OK file-missing");
    }

    #[test]
    fn call_process_missing_infile_signals_file_missing() {
        let cat = find_bin("cat");
        let result = eval_one(&format!(
            r#"(condition-case err (call-process "{cat}" "/nonexistent/neovm-process-infile") (error (car err)))"#
        ));
        assert_eq!(result, "OK file-missing");
    }

    #[test]
    fn call_process_region_bad_program_signals_file_missing() {
        let result = eval_one(
            r#"(condition-case err (call-process-region 1 1 "/nonexistent/program_xyz") (error (car err)))"#,
        );
        assert_eq!(result, "OK file-missing");
    }

    #[test]
    fn call_process_symbol_destination_signals_wrong_type_argument() {
        let echo = find_bin("echo");
        let result = eval_one(&format!(
            r#"(condition-case err (call-process "{echo}" nil 'foo nil "x") (error err))"#
        ));
        assert_eq!(result, "OK (wrong-type-argument stringp foo)");
    }

    #[test]
    fn call_process_bad_stderr_target_signals_wrong_type_argument() {
        let echo = find_bin("echo");
        let result = eval_one(&format!(
            r#"(condition-case err (call-process "{echo}" nil '(t 99) nil "x") (error err))"#
        ));
        assert_eq!(result, "OK (wrong-type-argument stringp 99)");
    }

    #[test]
    fn process_status_wrong_arg_type() {
        let result = eval_one(r#"(process-status 999)"#);
        assert!(result.contains("ERR"));
    }

    #[test]
    fn start_process_multiple_args() {
        let echo = find_bin("echo");
        let results = eval_all(&format!(
            r#"(start-process "echo" nil "{echo}" "a" "b" "c")
               (process-name 1)"#,
        ));
        assert_eq!(results[0], "OK 1");
        assert_eq!(results[1], r#"OK "echo""#);
    }
}
