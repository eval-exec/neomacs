use expect_test::expect;

use super::ParityBatchCase;

fn streamed_release_report_renders_and_copies_back_as_practical_markdown() -> ParityBatchCase {
    ParityBatchCase::value(
        "streamed_release_report_renders_and_copies_back_as_practical_markdown",
        r##"
(let ((first-chunk
       (concat
        "# Release review\n\n"
        "> Verify **GNU Emacs _and_ Neomacs** before shipping.\n\n"
        "- inspect `Cargo.toml`\n"
        "- compare [CI logs](https://example.test/runs/42)\n\n"
        "| Runtime | Result | Detail |\n"
        "|:---|:---:|---:|\n"
        "| GNU Emacs | pass | oracle |\n"
        "| Neomacs | pending | `map-put!` |\n\n"
        "```elisp\n"
        "(let ((result (mapcar #'1+ '(1 2 3))))\n"
        "  (message \"**literal markdown**: %S\" result))\n"))
      (second-chunk
       (concat
        "```\n\n"
        "See ~~the old workaround~~ **the minimized divergence**.\n")))
  (with-temp-buffer
    (insert first-chunk)
    (agent-shell-markdown-replace-markup :render-images nil)
    (let ((streaming-visible
           (substring-no-properties (buffer-string))))
      (goto-char (point-max))
      (insert second-chunk)
      (agent-shell-markdown-replace-markup
       :force t
       :render-images nil)
      (let ((rendered
             (substring-no-properties (buffer-string)))
            copied-code
            copied-link
            copied-document)
        (goto-char (point-min))
        (search-forward "(let ((result")
        (agent-shell-copy-source-block-at-point (1- (point)))
        (setq copied-code (current-kill 0 t))
        (goto-char (point-min))
        (search-forward "CI logs")
        (agent-shell-copy-link-url-at-point (1- (point)))
        (setq copied-link (current-kill 0 t))
        (agent-shell-copy-as-markdown (point-min) (point-max))
        (setq copied-document
              (substring-no-properties (current-kill 0 t)))
        (list
         streaming-visible
         rendered
         copied-code
         copied-link
         copied-document
         (equal copied-document
                (concat first-chunk second-chunk)))))))
"##,
        expect![[
            r##"OK ("Release review\n\n> Verify GNU Emacs and Neomacs before shipping.\n\n- inspect Cargo.toml\n- compare CI logs\n\n│ Runtime   │ Result  │ Detail   │\n├───────────┼─────────┼──────────┤\n│ GNU Emacs │ pass    │ oracle   │\n│ Neomacs   │ pending │ map-put! │\n\n```elisp\n(let ((result (mapcar #'1+ '(1 2 3))))\n  (message \"**literal markdown**: %S\" result))\n" "Release review\n\n> Verify GNU Emacs and Neomacs before shipping.\n\n- inspect Cargo.toml\n- compare CI logs\n\n│ Runtime   │ Result  │ Detail   │\n├───────────┼─────────┼──────────┤\n│ GNU Emacs │ pass    │ oracle   │\n│ Neomacs   │ pending │ map-put! │\n\n\nelisp ⧉\n\n(let ((result (mapcar #'1+ '(1 2 3))))\n  (message \"**literal markdown**: %S\" result))\n\n\nSee the old workaround the minimized divergence.\n" "(let ((result (mapcar #'1+ '(1 2 3))))\n  (message \"**literal markdown**: %S\" result))" "https://example.test/runs/42" "# Release review\n\n> Verify **GNU Emacs _and_ Neomacs** before shipping.\n\n- inspect `Cargo.toml`\n- compare [CI logs](https://example.test/runs/42)\n\n| Runtime | Result | Detail |\n|:---|:---:|---:|\n| GNU Emacs | pass | oracle |\n| Neomacs | pending | map-put! |\n\n```elisp\n(let ((result (mapcar #'1+ '(1 2 3))))\n  (message \"**literal markdown**: %S\" result))\n```\n\nSee ~~the old workaround~~ **the minimized divergence**.\n" nil)"##
        ]],
    )
}

pub(super) fn markdown_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![streamed_release_report_renders_and_copies_back_as_practical_markdown()]
}
