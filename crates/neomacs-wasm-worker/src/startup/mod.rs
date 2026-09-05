//! Browser product policy installed on the evaluator Worker before its loop.

use neovm_core::emacs_core::eval::Context;

/// Select GNU's in-process directory listing and the browser HTTP transport.
/// The shared VM and filesystem modules do not select Lisp product policy.
pub(crate) fn configure_lisp(evaluator: &mut Context) -> Result<(), String> {
    evaluator
        .eval_str(
            r##"(progn
          (setq ls-lisp-use-insert-directory-program nil)
          (require 'ls-lisp)
          (require 'url-neomacs-http)
          (url-neomacs-http-enable))"##,
        )
        .map(|_| ())
        .map_err(|error| format!("failed to initialize browser Lisp policy: {error:?}"))
}
