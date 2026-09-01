use expect_test::expect;

use super::ParityBatchCase;

/// The Commentary's documented usage, run verbatim against a real scoring
/// script.
///
/// asilea's own example is a compiler-flag search:
///
///     (asilea-run "timing-script"
///                 [["-O2" "-O3" "-Ofast" "-Os"]
///                  [nil "-ffast-math"]
///                  [nil "-ffoo" ("-ffoo" "-fbar")]
///                  [nil "-fexample-optimization"]])
///
/// That exact matrix appears nowhere in the suite beside this one, and it is
/// the shape that exercises every part of the option encoding at once: a group
/// with four mutually exclusive choices, groups whose first choice is `nil' and
/// so contribute no flag at all, and -- the interesting one -- a single choice
/// `("-ffoo" "-fbar")' that expands to *two* arguments on the command line.
///
/// The script is real, and it scores its own argv rather than returning a
/// constant: each flag it recognises lowers the energy by a different amount,
/// so the run has a genuine optimum and a stand-in returning one number could
/// not produce this result.  It also records every argument vector it was
/// given, which is what proves the flattening: `("-ffoo" "-fbar")' has to reach
/// the process as two separate arguments, and a `nil' choice has to reach it as
/// none.
///
/// What the recorded walk shows, precisely: `-ffoo -fbar' reaches the process
/// as two arguments while the neighbouring choice `-ffoo' reaches it as one, so
/// the nested group really is flattened; no invocation carries an empty
/// argument, so a `nil' choice contributes nothing rather than an empty string;
/// and the search moves from `-O2' to `-Ofast', ending at energy 72, which is
/// 100 - 25 - 3 for `-Ofast' plus `-ffoo'.  The fixed random sequence keeps the
/// walk inside two of the four groups, so this pins the encoding and the
/// descent, not coverage of every choice in the matrix.
///
/// Randomness is pinned through the package's own
/// `asilea-random-generator-function', which is the one double the standards
/// permit here -- without it the walk is different every run and nothing about
/// the result could be asserted.  Everything else is the real annealing loop,
/// the real subprocesses and the real energy parser.
fn the_documented_compiler_flag_search_flattens_groups_and_finds_the_best_score() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_documented_compiler_flag_search_flattens_groups_and_finds_the_best_score",
        r##"(let* ((root (file-name-as-directory
               (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (log (expand-file-name "argv.log" root))
       (script (expand-file-name "timing-script" root))
       (draws 0)
       accepted
       finished)
  (make-directory root t)
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "printf '%s\\n' \"$*\" >> \"" log "\"\n"
            "score=100\n"
            "for a in \"$@\"; do\n"
            "  case \"$a\" in\n"
            "    -O3)   score=$((score - 20)) ;;\n"
            "    -Ofast) score=$((score - 25)) ;;\n"
            "    -Os)   score=$((score - 5))  ;;\n"
            "    -O2)   score=$((score - 10)) ;;\n"
            "    -ffast-math) score=$((score - 7)) ;;\n"
            "    -ffoo) score=$((score - 3)) ;;\n"
            "    -fbar) score=$((score - 4)) ;;\n"
            "    -fexample-optimization) score=$((score - 1)) ;;\n"
            "  esac\n"
            "done\n"
            "printf '%s\\n' \"$score\"\n"))
  (set-file-modes script #o755)
  (let* ((asilea-max-steps 12)
         (asilea-concurrent-jobs 1)
         (asilea-initial-temperature 10.0)
         ;; Deterministic walk: the package's own randomness hook, cycling a
         ;; fixed sequence so the same states are visited in both editors.
         (sequence '(0.10 0.90 0.35 0.72 0.05 0.55 0.28 0.83 0.41 0.66 0.19 0.94))
         (asilea-random-generator-function
          (lambda (limit)
            (let ((draw (nth (mod draws (length sequence)) sequence)))
              (setq draws (1+ draws))
              (if (integerp limit)
                  (mod (truncate (* draw 1000)) limit)
                (* draw limit)))))
         (asilea-solution-accepted-function
          (lambda (state energy) (setq accepted (cons (copy-tree state) energy))))
         (asilea-finished-function (lambda () (setq finished t))))
    (asilea-run-synchronously
     script
     [["-O2" "-O3" "-Ofast" "-Os"]
      [nil "-ffast-math"]
      [nil "-ffoo" ("-ffoo" "-fbar")]
      [nil "-fexample-optimization"]]))
  (list :finished finished
        :accepted-energy (cdr accepted)
        :invocations
        (with-temp-buffer
          (insert-file-contents log)
          (split-string (buffer-string) "\n" t))))"##,
        expect![[
            r#"OK (:finished t :accepted-energy 72 :invocations ("-O2 -ffoo -fbar" "-O2 -ffoo" "-O2 -ffoo -fbar" "-O2 -ffoo" "-Ofast -ffoo" "-Ofast -ffoo" "-Ofast -ffoo -fbar" "-Ofast -ffoo" "-Ofast -ffoo" "-Ofast -ffoo" "-Ofast -ffoo -fbar" "-Ofast -ffoo"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![the_documented_compiler_flag_search_flattens_groups_and_finds_the_best_score()]
}
