//! Formatting driven by recorded output from a real Artistic Style.
//!
//! The corpus already puts a program called `astyle` on `PATH`, so the
//! subprocess seam is crossed and the argument vectors are pinned. What that
//! program does is a `sed` script with two hard-coded substitutions --
//! `int main(){` -> `int main() {` and an indent on `return 0;` -- so it is
//! not a formatter, and it is **input-independent in the way that matters**:
//! it reads its arguments only to log them. `--style=`, `--indent=spaces=`,
//! `--options=` and the six default flags have no effect on anything it
//! writes.
//!
//! That leaves the whole configuration surface asserted at the wrong end.
//! `arguments.rs` proves the package *composes* `--style=google` correctly and
//! `commands.rs` proves it *sends* it; nothing shows that sending it changes a
//! single character of the result, and a test that formatted with
//! `--style=allman` would produce byte-identical output today.
//!
//! Artistic Style is obtainable (`nix shell nixpkgs#astyle`), so these
//! workflows replay **recorded output from the real formatter**, version
//! 3.6.13, keyed on the argument vector. The stand-in fails loudly -- it
//! writes `UNRECORDED` to a miss log and exits 99 -- on an argument vector or
//! an input it has no recording for, and every workflow asserts that the miss
//! log is empty, because a formatter that answers "unchanged" to an unknown
//! request is indistinguishable from one that had nothing to change.
//!
//! Tool version recorded beside the package pin: astyle 3.6.13, against
//! astyle.el 20200328.616.

use expect_test::expect;

use super::ParityBatchCase;

const SETUP: &str = r##"        (progn
          (setq astyle-test-input "#include <stdio.h>\nint main(){\nint *p=NULL;\nint  x = 1+2;\nif(x>0){\nprintf(\"%d\\n\",x);\n}\n\n\nreturn 0;\n}\n")
          (setq astyle-test-rc-contents "--style=kr\n--indent=spaces=8\n--pad-oper\n")
          (setq astyle-test-recordings
                (list
                 (list (list "--style=google" "--indent=spaces=4" "--pad-oper" "--pad-header" "--break-blocks" "--delete-empty-lines" "--align-pointer=type" "--align-reference=name")
                       "#include <stdio.h>\nint main() {\n    int* p = NULL;\n    int  x = 1 + 2;\n\n    if (x > 0) {\n        printf(\"%d\\n\", x);\n    }\n\n    return 0;\n}\n")
                 (list (list "--style=allman" "--indent=spaces=2" "--pad-oper" "--pad-header" "--break-blocks" "--delete-empty-lines" "--align-pointer=type" "--align-reference=name")
                       "#include <stdio.h>\nint main()\n{\n  int* p = NULL;\n  int  x = 1 + 2;\n\n  if (x > 0)\n  {\n    printf(\"%d\\n\", x);\n  }\n\n  return 0;\n}\n")
                 (list (list "--options=@@RC@@")
                       "#include <stdio.h>\nint main()\n{\n        int *p = NULL;\n        int  x = 1 + 2;\n        if(x > 0) {\n                printf(\"%d\\n\", x);\n        }\n\n\n        return 0;\n}\n")))
          (defun astyle-test-key (arguments)
            ;; Terminate every argument with NUL rather than separating with it, so
            ;; this agrees byte for byte with the stand-in's `printf '%s\\0'` loop.
            ;; Separator semantics differ from terminator semantics by exactly one
            ;; trailing byte, and every lookup misses.
            (secure-hash 'sha256
                         (mapconcat (lambda (argument) (concat argument "\0"))
                                    arguments "")))
          (defun astyle-test-write (path content)
            (make-directory (file-name-directory path) t)
            (let ((coding-system-for-write 'utf-8-unix))
              (with-temp-buffer
                (insert content)
                (write-region (point-min) (point-max) path nil 'silent))))
          (defun astyle-test-install (rc-directory)
            "Install the recorded astyle stand-in on `exec-path'.

        Recordings hold the recording machine's rc path as `@@RC@@'; it is replaced
        by RC-DIRECTORY here, so the key computed from a replayed argument vector
        matches the key computed when the recording was made."
            (let* ((root (file-name-as-directory
                          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                   (bin (expand-file-name "astyle-bin" root))
                   (records (expand-file-name "astyle-records" root))
                   (misses (expand-file-name "astyle-misses.log" root))
                   (substitute
                    (lambda (text)
                      (replace-regexp-in-string
                       "@@RC@@"
                       (concat (file-name-as-directory rc-directory) ".astylerc")
                       text t t))))
              (dolist (recording astyle-test-recordings)
                (let* ((arguments (mapcar substitute (nth 0 recording)))
                       (directory (expand-file-name
                                   (astyle-test-key arguments) records)))
                  (astyle-test-write (expand-file-name "in" directory)
                                     astyle-test-input)
                  (astyle-test-write (expand-file-name "out" directory)
                                     (nth 1 recording))))
              (astyle-test-write misses "")
              (setenv "ASTYLE_TEST_RECORDS" (directory-file-name records))
              (setenv "ASTYLE_TEST_MISSES" misses)
              (make-directory bin t)
              (let ((program (expand-file-name "astyle" bin)))
                (astyle-test-write program "#!/bin/sh\n# Replay recorded Artistic Style 3.6.13 output, keyed on the argument\n# vector.  Fail loudly on anything unrecorded: a formatter that answers\n# \"unchanged\" to an unknown request is indistinguishable from a formatter\n# that had nothing to change, which is a legitimate result.\nkey=$(for a in \"$@\"; do printf '%s\\0' \"$a\"; done | sha256sum | cut -d' ' -f1)\ndir=\"$ASTYLE_TEST_RECORDS/$key\"\ninput=$(cat)\nif [ ! -d \"$dir\" ]; then\n  printf 'UNRECORDED argv:' >> \"$ASTYLE_TEST_MISSES\"\n  for a in \"$@\"; do printf ' %s' \"$a\" >> \"$ASTYLE_TEST_MISSES\"; done\n  printf '\\n' >> \"$ASTYLE_TEST_MISSES\"\n  exit 99\nfi\nif [ \"$input\" != \"$(cat \"$dir/in\")\" ]; then\n  printf 'UNRECORDED stdin for argv:' >> \"$ASTYLE_TEST_MISSES\"\n  for a in \"$@\"; do printf ' %s' \"$a\" >> \"$ASTYLE_TEST_MISSES\"; done\n  printf '\\n' >> \"$ASTYLE_TEST_MISSES\"\n  exit 99\nfi\ncat \"$dir/out\"\n")
                (set-file-modes program #o755))
              (setq exec-path (cons bin exec-path))
              (setenv "PATH" (concat bin path-separator (or (getenv "PATH") "")))
              misses))
          (defun astyle-test-misses ()
            (let ((file (getenv "ASTYLE_TEST_MISSES")))
              (if (file-readable-p file)
                  (with-temp-buffer
                    (insert-file-contents-literally file)
                    (buffer-string))
                :no-miss-log))))"##;

/// The style and indent settings change the formatted text, not just the argv.
///
/// The same buffer is formatted twice through the package's real
/// `astyle-buffer` command -- reformatter, real subprocess, real stdin and
/// stdout -- once with `google`/4 and once with `allman`/2. Real astyle puts
/// the brace on the declaration line for google and on its own line for
/// allman, so the two products differ in brace placement, indent width and
/// line count. Under the `sed` stand-in both runs return the same bytes.
///
/// The other flags are visible in the same product and are worth naming,
/// because each is a default the package sends on every invocation and none
/// of them did anything before: `--pad-oper` turns `1+2` into `1 + 2`,
/// `--align-pointer=type` turns `int *p` into `int* p`, `--pad-header` turns
/// `if(x>0)` into `if (x > 0)`, `--break-blocks` inserts the blank line before
/// `if`, and `--delete-empty-lines` collapses the two blank lines before
/// `return` into one.
fn google_and_allman_produce_different_text_from_the_same_buffer() -> ParityBatchCase {
    let elisp_form = format!(
        r##"(progn
          {SETUP}
          (let* ((root (file-name-as-directory
                        (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                 (project (expand-file-name "plain/" root))
                 (rc-project (expand-file-name "rc/" root)))
            (make-directory project t)
            (make-directory rc-project t)
            (astyle-test-install rc-project)
            (cl-flet ((format-with
                       (style indent)
                       (let ((file (expand-file-name "main.c" project)))
                         (astyle-test-write file astyle-test-input)
                         (with-temp-buffer
                           (setq buffer-file-name file)
                           (insert astyle-test-input)
                           (setq-local astyle-style style)
                           (setq-local astyle-indent indent)
                           (setq-local c-basic-offset indent)
                           (astyle-buffer)
                           (buffer-substring-no-properties
                            (point-min) (point-max))))))
              (let ((google (format-with "google" 4))
                    (allman (format-with "allman" 2)))
                (list
                 :google google
                 :allman allman
                 :differ (not (equal google allman))
                 :both-changed-the-input
                 (list (not (equal google astyle-test-input))
                       (not (equal allman astyle-test-input)))
                 :misses (astyle-test-misses))))))"##
    );
    let expect = expect![[
        r##"OK (:google "#include <stdio.h>\nint main() {\n    int* p = NULL;\n    int  x = 1 + 2;\n\n    if (x > 0) {\n        printf(\"%d\\n\", x);\n    }\n\n    return 0;\n}\n" :allman "#include <stdio.h>\nint main()\n{\n  int* p = NULL;\n  int  x = 1 + 2;\n\n  if (x > 0)\n  {\n    printf(\"%d\\n\", x);\n  }\n\n  return 0;\n}\n" :differ t :both-changed-the-input (t t) :misses "")"##
    ]];

    ParityBatchCase::value(
        "google_and_allman_produce_different_text_from_the_same_buffer",
        elisp_form,
        expect,
    )
    .fresh_process()
}

/// A project `.astylerc` replaces every default flag, and the output shows it.
///
/// When `locate-dominating-file` finds the rc file the package sends exactly
/// one argument, `--options=<dir>.astylerc`, and drops the six defaults. The
/// corpus pins that argument vector; what it cannot show is that the defaults
/// are really gone. This rc file asks for `kr`, 8-space indent and
/// `--pad-oper` alone, and the recorded output is the proof: `int *p` keeps
/// its pointer spacing because `--align-pointer=type` is no longer sent,
/// `if(x > 0)` is padded around the operator but not after the keyword
/// because `--pad-oper` is sent and `--pad-header` is not, and the two blank
/// lines before `return` both survive because `--delete-empty-lines` is gone.
///
/// Each of those is a flag *absent* from the command line being visible in the
/// text, which is the half an argv assertion structurally cannot reach.
fn a_project_rc_file_drops_the_default_flags_and_the_output_proves_it() -> ParityBatchCase {
    let elisp_form = format!(
        r##"(progn
          {SETUP}
          (let* ((root (file-name-as-directory
                        (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                 (rc-project (expand-file-name "rc/" root)))
            (make-directory rc-project t)
            (astyle-test-install rc-project)
            (astyle-test-write
             (expand-file-name ".astylerc" rc-project)
             astyle-test-rc-contents)
            (let ((file (expand-file-name "main.c" rc-project)))
              (astyle-test-write file astyle-test-input)
              (with-temp-buffer
                (setq buffer-file-name file)
                (insert astyle-test-input)
                (setq-local astyle-style "google")
                (setq-local astyle-indent 4)
                (setq-local c-basic-offset 4)
                (let ((arguments (astyle--format-args)))
                  (astyle-buffer)
                  (list
                   :arguments arguments
                   :formatted (buffer-substring-no-properties
                               (point-min) (point-max))
                   :pointer-spacing-kept
                   (and (string-match-p "int \\*p" (buffer-string)) t)
                   :blank-lines-kept
                   (and (string-match-p "\n\n\n" (buffer-string)) t)
                   :misses (astyle-test-misses))))))) "##
    );
    let expect = expect![[
        r##"OK (:arguments ("--options=[ORACLE-SANDBOX]/rc/.astylerc") :formatted "#include <stdio.h>\nint main()\n{\n        int *p = NULL;\n        int  x = 1 + 2;\n        if(x > 0) {\n                printf(\"%d\\n\", x);\n        }\n\n\n        return 0;\n}\n" :pointer-spacing-kept t :blank-lines-kept t :misses "")"##
    ]];

    ParityBatchCase::value(
        "a_project_rc_file_drops_the_default_flags_and_the_output_proves_it",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn workflows_practical_formatting_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        google_and_allman_produce_different_text_from_the_same_buffer(),
        a_project_rc_file_drops_the_default_flags_and_the_output_proves_it(),
    ]
}
