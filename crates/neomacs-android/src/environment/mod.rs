//! Android's session paths, independent of JNI and process-global environment.

use std::io;
use std::path::Path;

use neovm_core::emacs_core::{Context, Value};

#[cfg(target_os = "android")]
mod android;

/// OS-provided private directories validated before the editor starts.
pub struct AndroidSessionEnvironment {
    files: String,
    cache: String,
}

impl AndroidSessionEnvironment {
    pub fn new(files: impl AsRef<Path>, cache: impl AsRef<Path>) -> io::Result<Self> {
        fn directory(path: &Path) -> io::Result<String> {
            let text = path.to_str().filter(|text| !text.contains('\0'));
            match text {
                Some(text) if path.is_absolute() => Ok(text.to_owned()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Android session directory must be an absolute Unicode path",
                )),
            }
        }
        Ok(Self {
            files: directory(files.as_ref())?,
            cache: directory(cache.as_ref())?,
        })
    }

    /// Install on the VM thread after image restoration, before startup Lisp.
    ///
    /// GNU android.c establishes these paths before entering Emacs. Here the
    /// Lisp environment, also used by filename expansion and child processes,
    /// is session-owned: Android's already-multithreaded process is untouched.
    pub fn install(&self, context: &mut Context) {
        let entries = [
            format!("HOME={}", self.files),
            format!("TMPDIR={}", self.cache),
            "SHELL=/system/bin/sh".to_owned(),
        ];
        for variable in ["process-environment", "initial-environment"] {
            let mut environment = context
                .obarray()
                .symbol_value(variable)
                .copied()
                .unwrap_or(Value::NIL);
            // Environment lookup uses the first entry, just as GNU does.
            // Separate list spines keep initial-environment immutable under
            // later destructive edits to process-environment's new entries.
            for entry in entries.iter().rev() {
                environment = Value::cons(Value::string(entry.clone()), environment);
            }
            context.set_variable(variable, environment);
        }
        // These Lisp variables were initialized in the dump-producing host.
        context.set_variable(
            "temporary-file-directory",
            Value::string(format!("{}/", self.cache.trim_end_matches('/'))),
        );
        context.set_variable("shell-file-name", Value::string("/system/bin/sh"));
        context.set_variable("abbreviated-home-dir", Value::NIL);
    }
}
