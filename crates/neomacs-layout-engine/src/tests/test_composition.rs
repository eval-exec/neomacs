//! Explicit composition fixtures for tests that use an unbootstrapped VM.
//!
//! GNU `composite.c` selects through `composition-function-table`, not Unicode
//! grapheme heuristics. These narrow rules select the samples under test; they
//! are not a substitute for the full table installed by loadup. GNU Emacs 31.1
//! selects these same sample spans, including the complete woman-technologist
//! emoji (4 terminal columns), but not the incomplete man/ZWJ/woman sequence.

use neovm_core::emacs_core::Context;

pub(crate) fn install_rules(eval: &mut Context) {
    eval.eval_str(
        r##"(progn
      (setq auto-composition-mode t
            auto-composition-function 'auto-compose-chars
            composition-function-table (make-char-table nil))
      (let ((rule (list (vector "[^\n][̀-ͯ️⃣]+" 1 'compose-gstring-for-graphic))))
        (aset composition-function-table #x300 rule)
        (aset composition-function-table #x301 rule)
        (aset composition-function-table #xfe0f rule)
        (aset composition-function-table #x20e3 rule))
      (set-char-table-range composition-function-table '(#x600 . #x74f)
                            (list (vector "[؀-ݏ‌‍]+" 0 'arabic-shape-gstring)))
      (aset composition-function-table #x1f469
            (list (vector "👩‍💻" 0 'font-shape-gstring))))"##,
    )
    .expect("explicit GNU-shaped composition rules");
}
