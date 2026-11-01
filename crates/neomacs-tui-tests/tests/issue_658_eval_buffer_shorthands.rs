//! Issue #658: `eval-buffer` must apply a visited file's
//! `read-symbol-shorthands`, defining the longhand symbols.
//!
//! GNU's reader consults the special variable while reading
//! (src/lread.c:4956-5010). The fixture file maps `oi-` to
//! `gptel-openrouter-` in its Local Variables block, like the reported
//! gptel-openrouter package. The form runs from the `M-:` prompt
//! (`load-in-progress` nil), the path where Neomacs previously ignored the
//! buffer's binding.

use neomacs_tui_tests::pair::{boot_pair, eval_expression_one};

const FIXTURE: &str = r#";;; tui-shorthand-fixture.el --- fixture  -*- lexical-binding: t; -*-
;; Local Variables:
;; read-symbol-shorthands: (("oi-" . "gptel-openrouter-"))
;; End:
(setq oi-tui-shorthand 41)
"#;

#[test]
fn eval_buffer_uses_the_visited_files_symbol_shorthands() {
    let (mut gnu, mut neo) = boot_pair("");
    let dir = tempfile::tempdir().expect("temporary directory");
    let fixture = dir.path().join("tui-shorthand-fixture.el");
    std::fs::write(&fixture, FIXTURE).expect("write fixture");
    let gnu_out = dir.path().join("issue-658-gnu.out");
    let neo_out = dir.path().join("issue-658-neomacs.out");
    let form = |path: &std::path::Path, out: &std::path::Path| {
        format!(
            r##"(let ((buffer (find-file-noselect {path:?})))
  (unwind-protect
      (with-current-buffer buffer
        (eval-buffer)
        (let ((result (list (boundp 'gptel-openrouter-tui-shorthand)
                            (boundp 'oi-tui-shorthand)
                            (and (boundp 'gptel-openrouter-tui-shorthand)
                                 (symbol-value 'gptel-openrouter-tui-shorthand)))))
          (with-temp-file {out:?}
            (prin1 result (current-buffer)))))
    (kill-buffer buffer)))"##,
            path = path,
            out = out
        )
    };
    eval_expression_one(&mut gnu, &form(&fixture, &gnu_out));
    eval_expression_one(&mut neo, &form(&fixture, &neo_out));

    let gnu_result = std::fs::read_to_string(&gnu_out).expect("GNU eval-buffer result");
    let neo_result = std::fs::read_to_string(&neo_out).expect("Neomacs eval-buffer result");
    assert_eq!(
        gnu_result, "(t nil 41)",
        "GNU must define the shorthand-expanded symbol"
    );
    assert_eq!(
        neo_result,
        gnu_result,
        "Neomacs eval-buffer must apply the visited file's shorthands; \
         GNU grid:\n{}\nNeomacs grid:\n{}",
        gnu.text_grid().join("\n"),
        neo.text_grid().join("\n")
    );
}
