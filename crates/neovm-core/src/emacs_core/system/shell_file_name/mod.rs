//! Platform policy for Lisp's `shell-file-name`.
//!
//! GNU's `init_callproc` uses `$SHELL` when the startup environment supplies
//! it. Its Windows startup layer supplies a private `cmdproxy.exe` when the
//! variable is absent, while POSIX hosts fall back to `/bin/sh`. Keep that
//! target decision here so bootstrap and post-image startup cannot drift.

use std::ffi::OsString;
use std::path::Path;

use super::eval::Context;
use super::value::Value;

const POSIX_SHELL: &str = "/bin/sh";
const WINDOWS_SHELL_PROXY: &str = "cmdproxy.exe";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellPlatform {
    Posix,
    Windows,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedShellFileName {
    Environment(String),
    PosixSh,
    WindowsCmdProxy(String),
}

impl ResolvedShellFileName {
    pub(crate) fn lisp_name(&self) -> &str {
        match self {
            Self::Environment(name) | Self::WindowsCmdProxy(name) => name,
            Self::PosixSh => POSIX_SHELL,
        }
    }
}

/// Resolve the startup shell for an explicit target family.
///
/// Taking the platform as a value makes the Windows contract testable on
/// every CI host. `path_exec` is the same private architecture-dependent
/// directory GNU searches for `cmdproxy.exe`; when the OS cannot identify the
/// running executable, the bare name preserves GNU's final PATH-search
/// fallback instead of inventing a POSIX shell.
pub(crate) fn resolve_for(
    platform: ShellPlatform,
    inherited_shell: Option<OsString>,
    path_exec: Option<&Path>,
) -> ResolvedShellFileName {
    // GNU w32.c treats an empty SHELL like a missing one while the generic
    // callproc.c path preserves an explicitly empty environment value.
    let inherited_shell = match platform {
        ShellPlatform::Windows => inherited_shell.filter(|shell| !shell.is_empty()),
        ShellPlatform::Posix => inherited_shell,
    };
    if let Some(shell) = inherited_shell {
        let mut name = shell.to_string_lossy().into_owned();
        if platform == ShellPlatform::Windows {
            name = name.replace('\\', "/");
        }
        return ResolvedShellFileName::Environment(name);
    }

    match platform {
        ShellPlatform::Posix => ResolvedShellFileName::PosixSh,
        ShellPlatform::Windows => {
            let name = path_exec
                .map(|directory| directory.join(WINDOWS_SHELL_PROXY))
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| WINDOWS_SHELL_PROXY.to_owned());
            ResolvedShellFileName::WindowsCmdProxy(name)
        }
    }
}

const CURRENT_PLATFORM: ShellPlatform = std::cfg_select! {
    windows => { ShellPlatform::Windows }
    _ => { ShellPlatform::Posix }
};

pub(crate) fn resolve_current() -> ResolvedShellFileName {
    let path_exec = super::path_exec::resolve();
    resolve_for(
        CURRENT_PLATFORM,
        std::env::var_os("SHELL"),
        path_exec.as_ref().map(|resolved| resolved.dir()),
    )
}

/// Install the one authoritative startup answer into Lisp.
pub(crate) fn install(eval: &mut Context) {
    let shell = resolve_current();
    eval.set_variable("shell-file-name", Value::unibyte_string(shell.lisp_name()));
    eval.obarray.make_special("shell-file-name");
}

#[cfg(test)]
#[path = "tests/shell_file_name_test.rs"]
mod tests;
