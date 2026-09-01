//! GNU-compatible executable lookup shared by asynchronous and synchronous
//! subprocess creation.
//!
//! GNU owns this behavior in one deep module, `openp` (`src/lread.c`), used by
//! both `make-process` and `call-process`.  Keeping the lookup here prevents the
//! two Rust callers from growing subtly different path expansion, suffix, and
//! errno rules.

use super::{lisp_string_to_os_string, os_str_to_lisp_string, signal_file_errno, sys};
use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{Value, ValueKind, list_to_vec};
use crate::heap_types::LispString;
#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// The two GNU callers share search semantics but intentionally differ for a
/// leading system-absolute program name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutableLookupMode {
    /// `Fmake_process` accepts an absolute non-directory path directly; the
    /// eventual spawn owns existence/access errors (`process.c:2028-2046`).
    MakeProcess,
    /// `Fcall_process` always asks `openp` to verify X_OK first
    /// (`callproc.c:519-526`).
    CallProcess,
}

/// Frozen Lisp-visible inputs consumed by GNU `openp`.
///
/// Capturing the values before process creation gives callers a small, stable
/// interface and makes HOME from the dynamic `process-environment` explicit.
/// The implementation owns candidate expansion, suffix probing, and errno
/// precedence behind that interface.
#[derive(Clone, Debug)]
pub(crate) struct ExecutableSearch {
    exec_path: Value,
    exec_suffixes: Value,
    default_directory: Option<LispString>,
    home_directory: Option<Vec<u8>>,
}

enum ProgramLocation<'a> {
    SystemAbsolute(PathBuf),
    SearchPath(&'a LispString),
}

impl<'a> ProgramLocation<'a> {
    fn classify(program: &'a LispString) -> Self {
        let path = PathBuf::from(lisp_string_to_os_string(program));
        if path.is_absolute() {
            Self::SystemAbsolute(path)
        } else {
            // In GNU process.c, `~` is not system-absolute and therefore goes
            // through `openp`; `expand-file-name` resolves it against HOME.
            Self::SearchPath(program)
        }
    }
}

enum SearchBase {
    DefaultDirectory,
    ExecPath(LispString),
}

impl ExecutableSearch {
    pub(crate) fn capture(eval: &Context) -> Self {
        Self {
            exec_path: eval.visible_variable_value_or_nil("exec-path"),
            exec_suffixes: eval.visible_variable_value_or_nil("exec-suffixes"),
            default_directory: super::visible_default_directory_lisp(eval),
            home_directory: super::super::fileio::home_directory_for_expand_file_name(eval),
        }
    }

    pub(crate) fn resolve(
        &self,
        program: &LispString,
        mode: ExecutableLookupMode,
    ) -> Result<LispString, Flow> {
        let searchable_program = match (mode, ProgramLocation::classify(program)) {
            (ExecutableLookupMode::MakeProcess, ProgramLocation::SystemAbsolute(path)) => {
                if path.is_dir() {
                    return Err(signal(
                        "error",
                        vec![Value::string(
                            "Specified program for new process is a directory",
                        )],
                    ));
                }
                return Ok(program.clone());
            }
            (ExecutableLookupMode::CallProcess, ProgramLocation::SystemAbsolute(path)) => {
                return self.resolve_absolute_call_process(program, path);
            }
            (_, ProgramLocation::SearchPath(program)) => program,
        };

        let suffixes = self.exec_suffixes()?;
        let bases = self.search_bases(program)?;
        let mut last_errno = libc::ENOENT;

        for base in bases {
            let default_directory = match &base {
                SearchBase::DefaultDirectory => self.default_directory.as_ref(),
                SearchBase::ExecPath(directory) => Some(directory),
            };
            let Some(default_directory) = default_directory else {
                continue;
            };

            let expanded = super::super::fileio::expand_file_name_lisp_with_home(
                searchable_program,
                Some(default_directory),
                self.home_directory.as_deref(),
            );
            for suffix in &suffixes {
                let candidate = append_suffix(
                    super::super::fileio::lisp_file_name_to_path_buf(&expanded),
                    suffix,
                );
                match sys::executable_path_access(&candidate) {
                    Ok(()) => return Ok(os_str_to_lisp_string(candidate.as_os_str())),
                    failure => record_lookup_errno(&mut last_errno, failure),
                }
            }
        }

        Err(lookup_error(program, last_errno))
    }

    fn resolve_absolute_call_process(
        &self,
        program: &LispString,
        path: PathBuf,
    ) -> Result<LispString, Flow> {
        let mut last_errno = libc::ENOENT;
        match sys::executable_path_access(&path) {
            Ok(()) => Ok(os_str_to_lisp_string(path.as_os_str())),
            failure => {
                record_lookup_errno(&mut last_errno, failure);
                Err(lookup_error(program, last_errno))
            }
        }
    }

    fn exec_suffixes(&self) -> Result<Vec<LispString>, Flow> {
        if self.exec_suffixes.is_nil() {
            return Ok(vec![LispString::from_unibyte(Vec::new())]);
        }

        let suffix_values = list_to_vec(&self.exec_suffixes)
            .ok_or_else(|| signal_wrong_type_string(self.exec_suffixes))?;
        suffix_values
            .iter()
            .map(|value| super::super::builtins::expect_lisp_string(value).cloned())
            .collect()
    }

    fn search_bases(&self, program: &LispString) -> Result<Vec<SearchBase>, Flow> {
        if self.exec_path.is_nil() {
            return Ok(vec![SearchBase::DefaultDirectory]);
        }

        let entries =
            list_to_vec(&self.exec_path).ok_or_else(|| lookup_error(program, libc::ENOENT))?;
        Ok(entries
            .iter()
            .filter_map(|entry| match entry.kind() {
                ValueKind::Nil => Some(SearchBase::DefaultDirectory),
                ValueKind::String => entry.as_lisp_string().cloned().map(SearchBase::ExecPath),
                _ => None,
            })
            .collect())
    }
}

fn signal_wrong_type_string(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), value],
    )
}

fn lookup_error(program: &LispString, errno: libc::c_int) -> Flow {
    signal_file_errno(
        "Searching for program",
        Value::heap_string(program.clone()),
        errno,
    )
}

fn record_lookup_errno(last_errno: &mut libc::c_int, result: Result<(), libc::c_int>) {
    if let Err(errno) = result
        && errno != libc::ENOENT
        && errno != libc::ENOTDIR
    {
        *last_errno = errno;
    }
}

fn append_suffix(mut candidate: PathBuf, suffix: &LispString) -> PathBuf {
    if suffix.as_bytes().is_empty() {
        return candidate;
    }

    let mut os = candidate.into_os_string();
    #[cfg(unix)]
    os.push(OsStr::from_bytes(suffix.as_bytes()));
    #[cfg(not(unix))]
    os.push(crate::emacs_core::emacs_char::to_utf8_lossy(
        suffix.as_bytes(),
    ));
    candidate = PathBuf::from(os);
    candidate
}
