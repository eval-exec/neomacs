//! Browser product policy installed on the evaluator Worker before its loop.

use neovm_core::emacs_core::eval::Context;

/// Install shipped Lisp defaults before GNU startup loads personal init files.
/// The shared VM and filesystem modules do not select Lisp product policy.
pub(crate) fn configure_lisp(evaluator: &mut Context) -> Result<(), String> {
    evaluator
        .eval_str(
            r##"(progn
          (load (expand-file-name
                 "../lisp/neomacs-wasm/neomacs-wasm-startup.el"
                 invocation-directory) nil t t)
          (neomacs-wasm-startup-initialize))"##,
        )
        .map(|_| ())
        .map_err(|error| format!("failed to initialize browser Lisp policy: {error:?}"))
}
