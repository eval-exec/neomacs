use crate::common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct BufferCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_buffer_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping buffer semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        BufferCase {
            name: "modified_and_restore_transitions",
            form: r#"(let ((buf (get-buffer-create " *compat-buffer-state*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (list :initial
              (buffer-modified-p)
              (buffer-modified-tick)
              (buffer-chars-modified-tick)
              (recent-auto-save-p)
              :after-set-t
              (progn
                (set-buffer-modified-p t)
                (list (buffer-modified-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)
                      (recent-auto-save-p)))
              :after-restore-nil
              (progn
                (restore-buffer-modified-p nil)
                (list (buffer-modified-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)
                      (recent-auto-save-p)))
              :after-restore-autosaved
              (progn
                (restore-buffer-modified-p 'autosaved)
                (list (buffer-modified-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)
                      (recent-auto-save-p)))))
    (kill-buffer buf)))"#,
        },
        BufferCase {
            name: "autosave_state_transitions",
            form: r#"(let ((buf (get-buffer-create " *compat-buffer-auto*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (insert "x")
        (list :before-auto
              (buffer-modified-p)
              (recent-auto-save-p)
              (buffer-modified-tick)
              (buffer-chars-modified-tick)
              :after-auto
              (progn
                (set-buffer-auto-saved)
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))
              :after-set-t
              (progn
                (set-buffer-modified-p t)
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))
              :after-insert
              (progn
                (insert "y")
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))
              :after-clear
              (progn
                (set-buffer-modified-p nil)
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))))
    (kill-buffer buf)))"#,
        },
        BufferCase {
            name: "indirect_buffer_text_properties_follow_shared_text_edits",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-text-props-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "abcdef")
          (put-text-property 2 5 'face 'bold))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-text-props-indirect*" nil)))
          (unwind-protect
              (progn
                (with-current-buffer indirect
                  (delete-region 3 4))
                (list
                 (with-current-buffer base
                   (list
                    (buffer-string)
                    (get-text-property 2 'face)
                    (get-text-property 3 'face)
                    (get-text-property 4 'face)
                    (get-text-property 5 'face)))
                 (with-current-buffer indirect
                   (list
                    (buffer-string)
                    (get-text-property 2 'face)
                    (get-text-property 3 'face)
                    (get-text-property 4 'face)
                    (get-text-property 5 'face)))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "indirect_buffer_undo_list_follows_shared_text_history",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-undo-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (setq buffer-undo-list nil)
          (insert "abc"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-undo-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer base
                 (prin1-to-string buffer-undo-list))
               (with-current-buffer indirect
                 (prin1-to-string buffer-undo-list))
               (with-current-buffer indirect
                 (let ((buffer-undo-list buffer-undo-list))
                   (primitive-undo 1 buffer-undo-list)
                   (buffer-string)))
               (with-current-buffer base
                 (buffer-string)))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "set_buffer_modified_p_returns_nil_and_updates_indirect_base_state",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-modified-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "x"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-modified-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer indirect
                 (set-buffer-modified-p nil))
               (with-current-buffer base
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p)))
               (with-current-buffer indirect
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "indirect_buffer_autosave_state_is_buffer_local",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-autosave-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "xy"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-autosave-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer indirect
                 (set-buffer-auto-saved))
               (with-current-buffer base
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p)))
               (with-current-buffer indirect
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "restore_buffer_modified_p_autosaved_targets_indirect_base",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-restore-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "xy"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-restore-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer indirect
                 (restore-buffer-modified-p 'autosaved))
               (with-current-buffer base
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p)))
               (with-current-buffer indirect
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "internal_set_buffer_modified_tick_shares_modiff_not_autosave",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-modiff-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "xy"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-modiff-indirect*" nil)))
          (unwind-protect
              (progn
                (with-current-buffer indirect
                  (set-buffer-auto-saved)
                  (internal--set-buffer-modified-tick 77))
                (list
                 (with-current-buffer base
                   (list (buffer-modified-p)
                         (buffer-modified-tick)
                         (recent-auto-save-p)))
                 (with-current-buffer indirect
                   (list (buffer-modified-p)
                         (buffer-modified-tick)
                         (recent-auto-save-p)))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "killing_base_buffer_kills_indirect_buffers",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-kill-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "abc"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-kill-indirect*" nil)))
          (list (buffer-live-p base)
                (buffer-live-p indirect)
                (kill-buffer base)
                (buffer-live-p base)
                (buffer-live-p indirect)
                (get-buffer " *compat-buffer-kill-base*")
                (get-buffer " *compat-buffer-kill-indirect*"))))
    (when (get-buffer " *compat-buffer-kill-base*")
      (kill-buffer " *compat-buffer-kill-base*"))
    (when (get-buffer " *compat-buffer-kill-indirect*")
      (kill-buffer " *compat-buffer-kill-indirect*"))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "buffer semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}

#[test]
fn edit_preparation_remeasures_after_before_change_hooks() {
    if !oracle_enabled() {
        return;
    }
    for command in [
        "(delete-region 3 5)",
        "(goto-char 3) (delete-char 2)",
        "(goto-char 5) (delete-char -2)",
        "(delete-and-extract-region 3 5)",
    ] {
        for hook in [
            "(erase-buffer)",
            "(goto-char 1) (delete-char 1)",
            "(goto-char 1) (insert \"xx\")",
        ] {
            let form = format!(
                "(with-temp-buffer (insert \"aé€𝄞def\")
                   (add-hook 'before-change-functions
                     (lambda (&rest _) {hook}) nil t)
                   {command} (list (buffer-string) (point)))"
            );
            let expected = run_oracle_eval(&form).expect("GNU edit oracle");
            let actual = run_neovm_eval(&form).expect("NeoVM edit probe");
            assert_eq!(actual, expected, "{form}");
        }
    }
}
/// Edit commands whose before-change (or interval) callbacks edit the buffer.
/// Each command measures its range before the callbacks run; GNU re-measures
/// afterwards (`del_range_1`, `replace_range`), and so must neomacs.
const P41_HOOKS: [(&str, &str); 3] = [
    ("erase", "(erase-buffer)"),
    (
        "drop-first",
        "(when (> (point-max) 3) (save-excursion (goto-char 1) (delete-char 1)))",
    ),
    (
        "insert-front",
        "(save-excursion (goto-char 1) (insert \"xé\"))",
    ),
];

/// (name, initial text, command, whether the command itself may leave raw
/// eight-bit bytes in the buffer).
const P41_COMMANDS: [(&str, &str, &str, bool); 21] = [
    ("delete-region", "aé€𝄞def", "(delete-region 3 5)", false),
    (
        "delete-char",
        "aé€𝄞def",
        "(goto-char 3) (delete-char 2)",
        false,
    ),
    (
        "delete-char-backward",
        "aé€𝄞def",
        "(goto-char 5) (delete-char -2)",
        false,
    ),
    (
        "delete-horizontal-space",
        "aé  \t €𝄞",
        "(goto-char 5) (delete-horizontal-space)",
        false,
    ),
    (
        "zap-to-char",
        "aé€𝄞def",
        "(goto-char 2) (zap-to-char 1 ?d)",
        false,
    ),
    (
        "kill-line",
        "aé€𝄞def\nxyz",
        "(goto-char 3) (kill-line)",
        false,
    ),
    (
        "transpose-chars",
        "aé€𝄞def",
        "(goto-char 3) (transpose-chars 1)",
        false,
    ),
    (
        "delete-and-extract-region",
        "aé€𝄞def",
        "(delete-and-extract-region 3 5)",
        false,
    ),
    (
        "translate-region",
        "aé€𝄞def",
        "(let ((tt (make-char-table 'translation-table))) (aset tt ?€ ?z) (aset tt ?d ?q) (translate-region 2 7 tt))",
        false,
    ),
    ("upcase-region", "aé€𝄞def", "(upcase-region 2 7)", false),
    (
        "capitalize-region",
        "aé€𝄞def",
        "(capitalize-region 2 7)",
        false,
    ),
    (
        "upcase-word",
        "aé€𝄞def ghi",
        "(goto-char 2) (upcase-word 1)",
        false,
    ),
    (
        "transpose-regions",
        "aé€𝄞def",
        "(transpose-regions 2 3 5 6)",
        false,
    ),
    (
        "replace-region-contents",
        "aé€𝄞def",
        "(replace-region-contents 3 5 \"AB\")",
        false,
    ),
    (
        "replace-region-contents-fallback",
        "aé€𝄞def",
        "(replace-region-contents 3 5 \"AB\" 0)",
        false,
    ),
    (
        "base64-encode-region",
        "aé€𝄞def",
        "(set-buffer-multibyte nil) (base64-encode-region 2 5)",
        true,
    ),
    (
        "base64-decode-region",
        "xYWJjZA==y",
        "(base64-decode-region 2 10)",
        false,
    ),
    (
        "encode-coding-region",
        "aé€𝄞def",
        "(encode-coding-region 2 5 'utf-8)",
        true,
    ),
    (
        "decode-coding-region",
        "abcdéf",
        "(decode-coding-region 2 5 'utf-8)",
        false,
    ),
    (
        "replace-match",
        "aé€𝄞def",
        "(goto-char 1) (re-search-forward \"€𝄞\") (replace-match \"XY\")",
        false,
    ),
    (
        "move-to-column",
        "aé\tbcdef",
        "(goto-char 4) (move-to-column 3 t)",
        false,
    ),
];

/// A text-property `modification-hooks` function runs at the same seam.
const P41_INTERVAL_HOOK_COMMANDS: [&str; 2] =
    ["(delete-region 3 5)", "(delete-and-extract-region 3 5)"];

fn p41_valid_text_probe(text: &str, install: &str, command: &str, raw_bytes_ok: bool) -> String {
    let raw_check = if raw_bytes_ok {
        "t"
    } else {
        "(let ((ok t)) (dolist (c (append s nil) ok) (when (>= c #x3fff80) (setq ok nil))))"
    };
    format!(
        r#"(with-temp-buffer
             (insert "{text}")
             {install}
             (condition-case nil (progn {command}) (error nil))
             (let ((s (buffer-string)))
               (list {raw_check}
                     (= (length s) (- (point-max) (point-min)))
                     (= (string-bytes s) (- (position-bytes (point-max)) (position-bytes (point-min))))
                     (<= (point-min) (point) (point-max)))))"#
    )
}

/// Evaluate FORM, reporting a Rust panic as a failed case so one run lists
/// every command that still panics.
fn p41_eval_catching_panics(form: &str) -> String {
    match std::panic::catch_unwind(|| run_neovm_eval(form)) {
        Ok(Ok(printed)) => printed,
        Ok(Err(error)) => format!("harness error: {error}"),
        Err(_) => "panic".to_string(),
    }
}

/// The crash class of idiom-audit P4.1: every command that measured its range
/// before its modification callbacks ran either panicked (gap_buffer
/// "end > len") or split a multibyte character when a callback edited the
/// buffer. GNU itself reads stale positions in several of these combinations
/// (and aborts in `encode-coding-region`), so the invariant here is the one
/// GNU cannot give: no panic, whole characters, coherent positions.
#[test]
fn edit_commands_keep_text_valid_when_change_callbacks_rewrite_the_buffer() {
    let mut failures = Vec::new();
    for (name, text, command, raw_bytes_ok) in P41_COMMANDS {
        for (hook_name, hook) in P41_HOOKS {
            for (seam, install) in [
                (
                    "before-change",
                    format!("(add-hook 'before-change-functions (lambda (&rest _) {hook}) nil t)"),
                ),
                (
                    "after-change",
                    format!(
                        "(let ((once nil)) (add-hook 'after-change-functions (lambda (&rest _) (unless once (setq once t) {hook})) nil t))"
                    ),
                ),
            ] {
                let form = p41_valid_text_probe(text, &install, command, raw_bytes_ok);
                let actual = p41_eval_catching_panics(&form);
                if actual != "OK (t t t t)" {
                    failures.push(format!("{name} {seam} {hook_name}: {actual}"));
                }
            }
        }
    }
    for command in P41_INTERVAL_HOOK_COMMANDS {
        for (hook_name, hook) in P41_HOOKS {
            let install = format!(
                "(put-text-property 2 6 'modification-hooks (list (lambda (_b _e) {hook})))"
            );
            let form = p41_valid_text_probe("aé€𝄞def", &install, command, false);
            let actual = p41_eval_catching_panics(&form);
            if actual != "OK (t t t t)" {
                failures.push(format!(
                    "{command} modification-hooks {hook_name}: {actual}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Where GNU's own re-measurement is well defined, neomacs must produce the
/// same text, point and return value.
#[test]
fn edit_commands_remeasure_after_change_callbacks_like_gnu() {
    if !oracle_enabled() {
        return;
    }
    let mut cases = Vec::new();
    for (name, text, command, _) in P41_COMMANDS {
        for (hook_name, hook) in P41_HOOKS {
            let gnu_defined = match name {
                "delete-region"
                | "delete-char"
                | "delete-char-backward"
                | "delete-horizontal-space"
                | "zap-to-char"
                | "kill-line"
                | "delete-and-extract-region" => true,
                "upcase-region"
                | "capitalize-region"
                | "upcase-word"
                | "transpose-chars"
                | "replace-region-contents"
                | "replace-match" => hook_name != "erase",
                "replace-region-contents-fallback" => true,
                // GNU reads stale positions (translate-region, transpose-regions,
                // base64, coding regions) or aborts (encode-coding-region).
                _ => false,
            };
            if gnu_defined {
                cases.push(format!(
                    r#"(with-temp-buffer
                         (insert "{text}")
                         (add-hook 'before-change-functions (lambda (&rest _) {hook}) nil t)
                         (list (condition-case err (progn {command}) (error (list 'error err)))
                               (append (buffer-string) nil) (point)))"#
                ));
            }
        }
    }
    for hook in [
        "(when (> (point-max) 4) (save-excursion (goto-char 1) (delete-char 1)))",
        "(save-excursion (goto-char 1) (insert \"é\"))",
    ] {
        cases.push(format!(
            r#"(with-temp-buffer
                 (insert "aé\tbcdef") (goto-char 4)
                 (let ((once nil))
                   (add-hook 'after-change-functions
                     (lambda (&rest _) (unless once (setq once t) {hook})) nil t))
                 (list (move-to-column 3 t) (append (buffer-string) nil) (point)))"#
        ));
    }
    for command in P41_INTERVAL_HOOK_COMMANDS {
        for hook in [
            "(save-excursion (goto-char 1) (delete-char 1))",
            "(save-excursion (goto-char 1) (insert \"xé\"))",
        ] {
            cases.push(format!(
                r#"(with-temp-buffer
                     (insert "aé€𝄞def")
                     (put-text-property 2 6 'modification-hooks (list (lambda (_b _e) {hook})))
                     (list (condition-case err {command} (error (list 'error err)))
                           (append (buffer-string) nil) (point)))"#
            ));
        }
    }
    for form in cases {
        let expected = run_oracle_eval(&form).expect("GNU edit oracle");
        let actual = run_neovm_eval(&form).expect("NeoVM edit probe");
        assert_eq!(actual, expected, "{form}");
    }
}

#[test]
fn treesit_parse_string_restores_caller_before_parser_creation() {
    if !oracle_enabled() {
        return;
    }
    // tsA owns condition-name parity; this probe observes caller state on
    // language-loader failure without pinning the independent error symbol.
    let form = r#"(with-temp-buffer
      (insert "aé€𝄞")
      (let ((orig (current-buffer)))
        (list (condition-case nil
                (treesit-parse-string "x" 'no-such-lang-tse-scope)
                (error (list 'caught (eq (current-buffer) orig))))
              (eq (current-buffer) orig) (buffer-string) (point))))"#;
    let expected = run_oracle_eval(form).expect("GNU treesit oracle");
    let actual = run_neovm_eval(form).expect("NeoVM treesit probe");
    assert_eq!(actual, expected);
}

#[test]
fn buffer_property_hook_errors_restore_current_buffer() {
    if !oracle_enabled() {
        return;
    }
    for command in [
        "(put-text-property 1 2 'face 'bold other)",
        "(add-text-properties 1 2 '(face bold) other)",
        "(set-text-properties 1 2 '(face bold) other)",
        "(remove-text-properties 1 2 '(face nil) other)",
        "(remove-list-of-text-properties 1 2 '(face) other)",
        "(add-face-text-property 1 2 'italic nil other)",
    ] {
        for setup in [
            "(setq buffer-read-only t)",
            "(add-hook 'before-change-functions (lambda (&rest _) (error \"scope-hook\")) nil t)",
        ] {
            let form = format!(
                "(let* ((orig (current-buffer)) (other (generate-new-buffer \"tse-property\")))
                   (with-current-buffer other (insert \"abc\") (put-text-property 1 2 'face 'bold) {setup})
                   (unwind-protect
                     (list (condition-case e {command} (error (list (car e) (eq orig (current-buffer)))))
                           (eq orig (current-buffer)))
                     (with-current-buffer other (setq buffer-read-only nil before-change-functions nil)) (kill-buffer other)))"
            );
            let expected = run_oracle_eval(&form).expect("GNU property oracle");
            let actual = run_neovm_eval(&form).expect("NeoVM property probe");
            assert_eq!(actual, expected, "{form}");
        }
    }
}

#[test]
fn replace_region_contents_remeasures_each_live_diff_run() {
    if !oracle_enabled() {
        return;
    }
    for hook in [
        "(goto-char 1) (insert \"xx\")",
        "(goto-char 1) (delete-char 1)",
    ] {
        for fallback in ["nil", "0"] {
            let form = format!(
                r#"(with-temp-buffer
              (insert "aé€𝄞def")
              (let* ((log nil)
                    (before-change-functions (list (lambda (&rest _) {hook})))
                    (after-change-functions (list (lambda (&rest args) (push args log)))))
                (list (replace-region-contents 3 5 "AB" {fallback})
                      (buffer-string) (point) (nreverse log))))"#
            );
            let expected = run_oracle_eval(&form).expect("GNU replacement oracle");
            let actual = run_neovm_eval(&form).expect("NeoVM replacement probe");
            assert_eq!(actual, expected, "{form}");
        }
    }
}

#[test]
fn replace_match_preparation_updates_registers_before_after_hooks() {
    if !oracle_enabled() {
        return;
    }
    for hook in [
        "(goto-char 1) (insert \"xx\")",
        "(goto-char 1) (delete-char 1)",
        "(erase-buffer)",
    ] {
        let form = format!(
            r#"(with-temp-buffer
          (insert "aé€𝄞DEF") (goto-char 1) (re-search-forward "\\(€𝄞\\)")
          (let ((calls nil))
            (add-hook 'before-change-functions
              (lambda (s e) (push (list 'before s e) calls) {hook}) nil t)
            (add-hook 'after-change-functions
              (lambda (s e l) (push (list 'after s e l
                (match-beginning 0) (match-end 0)
                (match-beginning 1) (match-end 1)) calls)) nil t)
            (replace-match "ab" t t)
            (list (buffer-string) (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1) (nreverse calls))))"#
        );
        // GNU retains a stale, out-of-bounds point after the erase hook;
        // compare text and register publication separately from that corner.
        assert_eq!(
            run_neovm_eval(&form).expect("NeoVM prepared replacement"),
            run_oracle_eval(&form).expect("GNU prepared replacement"),
            "{form}"
        );
    }
}

#[test]
fn replace_match_retains_original_official_point_after_hook_relocation() {
    if !oracle_enabled() {
        return;
    }
    for hook in [
        "(goto-char 1) (insert \"xx\")",
        "(goto-char 1) (delete-char 1)",
    ] {
        let form = format!(
            r#"(with-temp-buffer
          (insert "aé€𝄞DEF") (goto-char 1) (re-search-forward "€𝄞")
          (add-hook 'before-change-functions (lambda (&rest _) {hook}) nil t)
          (replace-match "ab" t t) (list (buffer-string) (point)))"#
        );
        assert_eq!(
            run_neovm_eval(&form).expect("NeoVM replacement point"),
            run_oracle_eval(&form).expect("GNU replacement point"),
            "{form}"
        );
    }
}

#[test]
fn move_to_column_force_remeasures_each_stage_after_hooks() {
    if !oracle_enabled() {
        return;
    }
    for stage in [1, 2] {
        for action in [
            "(erase-buffer)",
            "(goto-char (point-max))",
            "(goto-char 1)",
            "(goto-char 1) (insert \"é\")",
        ] {
            let form = format!(
                r#"(with-temp-buffer
              (insert "a\tbcdef") (goto-char 3)
              (let ((calls nil) (count 0))
                (add-hook 'after-change-functions
                  (lambda (s e l) (setq count (1+ count))
                    (push (list count s e l (point)) calls)
                    (when (= count {stage}) {action})) nil t)
                (let ((ret (move-to-column 3 t)))
                  (list ret (buffer-string) (point) (nreverse calls)))))"#
            );
            assert_eq!(
                run_neovm_eval(&form).expect("NeoVM column stages"),
                run_oracle_eval(&form).expect("GNU column stages"),
                "{form}"
            );
        }
    }
}

#[test]
fn edit_preparation_before_hook_selected_buffer_persists_and_is_edited() {
    if !oracle_enabled() {
        return;
    }
    let form = r#"(let ((source (generate-new-buffer " *owned-prepare-source*"))
                       (target (generate-new-buffer " *owned-prepare-target*")))
      (unwind-protect
          (progn
            (with-current-buffer target (insert "uvwxyz"))
            (set-buffer source)
            (insert "abcdef")
            (setq-local before-change-functions
                        (list (lambda (_start _end) (set-buffer target))))
            (list (delete-region 3 5)
                  (buffer-name) (buffer-string)
                  (with-current-buffer source (buffer-string))))
        (when (buffer-live-p source) (kill-buffer source))
        (when (buffer-live-p target) (kill-buffer target))))"#;
    let expected = run_oracle_eval(form).expect("GNU selected-buffer oracle");
    assert_eq!(
        run_neovm_eval(form).expect("NeoVM selected-buffer probe"),
        expected
    );
}

#[test]
fn edit_preparation_killed_marker_signals_without_editing_selected_buffer() {
    if !oracle_enabled() {
        return;
    }
    let form = r#"(let ((source (generate-new-buffer " *owned-prepare-source*"))
                       (target (generate-new-buffer " *owned-prepare-target*")))
      (unwind-protect
          (progn
            (with-current-buffer target (insert "uvwxyz"))
            (set-buffer source)
            (insert "abcdef")
            (setq-local before-change-functions
                        (list (lambda (_start _end)
                                (set-buffer target) (kill-buffer source))))
            (list (condition-case err (delete-region 3 5)
                    (error (list (car err) (cadr err))))
                  (buffer-name) (buffer-string)))
        (when (buffer-live-p source) (kill-buffer source))
        (when (buffer-live-p target) (kill-buffer target))))"#;
    let expected = run_oracle_eval(form).expect("GNU killed-marker oracle");
    assert_eq!(
        run_neovm_eval(form).expect("NeoVM killed-marker probe"),
        expected
    );
}

#[test]
fn edit_preparation_deactivate_watcher_signal_aborts_mutation() {
    if !oracle_enabled() {
        return;
    }
    let form = r#"(with-temp-buffer
      (insert "abcdef")
      (let ((watcher (lambda (_symbol value operation _where)
                       (when (and (eq operation 'set) (eq value t))
                         (error "deactivate-watcher")))))
        (unwind-protect
            (progn
              (add-variable-watcher 'deactivate-mark watcher)
              (list (condition-case err (delete-region 3 5)
                      (error (cadr err)))
                    (buffer-string)))
          (remove-variable-watcher 'deactivate-mark watcher))))"#;
    let expected = run_oracle_eval(form).expect("GNU deactivate watcher oracle");
    assert_eq!(
        run_neovm_eval(form).expect("NeoVM deactivate watcher probe"),
        expected
    );
}

#[test]
fn compat_subst_region_shared_edit_geometry_matches_gnu() {
    if !oracle_enabled() {
        tracing::debug!("skipping shared edit helper audit: GNU Emacs oracle unavailable");
        return;
    }

    let form = r###"(let (rows)
  (dolist (multiby '(nil t))
    (dolist (properties '(nil t))
      (dolist (pair '((97 122) (233 232)))
        (dolist (noundo '(nil t))
          (with-temp-buffer
            (set-buffer-multibyte multiby)
            (insert (if multiby "aéaé" (unibyte-string 97 233 97 233)))
            (when properties (put-text-property 1 (point-max) 'tse-prop 'kept))
            (goto-char 3)
            (let ((result (subst-char-in-region 1 (point-max) (car pair) (cadr pair) noundo)))
              (push (list multiby properties pair noundo result (point)
                          (string-to-list (buffer-string))
                          (get-text-property 1 'tse-prop)) rows)))))))
  (with-temp-buffer
    (insert "aé")
    (let ((result (condition-case err (subst-char-in-region 1 3 ?a ?é) (error (car err)))))
      (push (list 'different-byte-length result (string-to-list (buffer-string))) rows)))
  (nreverse rows))"###;
    let expected = r###"OK ((nil nil (97 122) nil nil 3 (122 233 122 233) nil) (nil nil (97 122) t nil 3 (122 233 122 233) nil) (nil nil (233 232) nil nil 3 (97 232 97 232) nil) (nil nil (233 232) t nil 3 (97 232 97 232) nil) (nil t (97 122) nil nil 3 (122 233 122 233) kept) (nil t (97 122) t nil 3 (122 233 122 233) kept) (nil t (233 232) nil nil 3 (97 232 97 232) kept) (nil t (233 232) t nil 3 (97 232 97 232) kept) (t nil (97 122) nil nil 3 (122 233 122 233) nil) (t nil (97 122) t nil 3 (122 233 122 233) nil) (t nil (233 232) nil nil 3 (97 232 97 232) nil) (t nil (233 232) t nil 3 (97 232 97 232) nil) (t t (97 122) nil nil 3 (122 233 122 233) kept) (t t (97 122) t nil 3 (122 233 122 233) kept) (t t (233 232) nil nil 3 (97 232 97 232) kept) (t t (233 232) t nil 3 (97 232 97 232) kept) (different-byte-length error (97 233)))"###;
    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    assert_eq!(gnu, expected, "unexpected GNU shared edit helper contract");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "shared edit helper semantics mismatch:\nGNU: {gnu}\nNeoVM: {neovm}"
    );
}

#[test]
fn compat_word_casing_shared_replace_expansion_matches_gnu() {
    if !oracle_enabled() {
        tracing::debug!("skipping shared edit helper audit: GNU Emacs oracle unavailable");
        return;
    }

    let form = r###"(let (rows)
  (dolist (properties '(nil t))
    (dolist (command '(upcase-word capitalize-word downcase-word))
      (with-temp-buffer
        (insert "ß STRAẞE")
        (when properties (put-text-property 1 2 'tse-prop 'kept))
        (goto-char 1)
        (let ((result (funcall command 1)))
          (push (list properties command result (point)
                      (string-to-list (buffer-string))
                      (get-text-property 1 'tse-prop)) rows)))))
  (nreverse rows))"###;
    let expected = r###"OK ((nil upcase-word nil 3 (83 83 32 83 84 82 65 7838 69) nil) (nil capitalize-word nil 3 (83 115 32 83 84 82 65 7838 69) nil) (nil downcase-word nil 2 (223 32 83 84 82 65 7838 69) nil) (t upcase-word nil 3 (83 83 32 83 84 82 65 7838 69) nil) (t capitalize-word nil 3 (83 115 32 83 84 82 65 7838 69) nil) (t downcase-word nil 2 (223 32 83 84 82 65 7838 69) kept))"###;
    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    assert_eq!(gnu, expected, "unexpected GNU shared edit helper contract");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "shared edit helper semantics mismatch:\nGNU: {gnu}\nNeoVM: {neovm}"
    );
}

fn tse_excursion_window_oracle_case(form: &str, expected_state: &str) {
    if !oracle_enabled() {
        return;
    }
    let expected = run_oracle_eval(form).expect("GNU saved excursion window oracle");
    assert_eq!(
        expected, expected_state,
        "GNU must reach the full expected state: {form}"
    );
    let actual = run_neovm_eval(form).expect("Neo saved excursion window result");
    assert_eq!(actual, expected, "{form}");
}

const TSE_EXCURSION_WINDOW_CASE: &str = r#"(defun tse-excursion-window-case (action)
  (let ((caller (generate-new-buffer " *tse-excursion-caller*"))
        (other (generate-new-buffer " *tse-excursion-other*"))
        (victim (generate-new-buffer " *tse-excursion-victim*")))
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (switch-to-buffer caller)
          (insert "abcdef") (goto-char 2)
          (let ((original-window (selected-window))
                (other-window (split-window)))
            (with-current-buffer other (insert "uvwxyz"))
            (set-window-buffer other-window other)
            (with-current-buffer victim
              (setq kill-buffer-query-functions
                    (list (lambda ()
                            (funcall action original-window other-window caller other)
                            nil))))
            (let ((result (kill-buffer victim)))
              (list result
                    (eq (current-buffer) caller) (point)
                    (eq (selected-window) other-window)
                    (window-live-p original-window)
                    (and (window-live-p original-window) (window-point original-window))
                    (buffer-live-p victim)))))
      (with-current-buffer victim (setq kill-buffer-query-functions nil))
      (mapc (lambda (buffer) (when (buffer-live-p buffer) (kill-buffer buffer)))
            (list caller other victim)))))"#;

#[test]
fn saved_excursion_skips_deleted_original_window() {
    let form = format!(
        "(progn {} (tse-excursion-window-case {}))",
        TSE_EXCURSION_WINDOW_CASE,
        r#"(lambda (original alternative _caller _other)
 (select-window original) (goto-char 4) (select-window alternative) (delete-window original))"#
    );
    tse_excursion_window_oracle_case(&form, "OK (nil t 2 t nil nil t)");
}

#[test]
fn saved_excursion_skips_original_window_with_changed_buffer() {
    let form = format!(
        "(progn {} (tse-excursion-window-case {}))",
        TSE_EXCURSION_WINDOW_CASE,
        r#"(lambda (original alternative _caller other)
 (select-window original) (goto-char 4) (select-window alternative) (set-window-buffer original other))"#
    );
    tse_excursion_window_oracle_case(&form, "OK (nil t 2 t t 7 t)");
}

#[test]
fn saved_excursion_skips_killed_saved_buffer() {
    let form = format!(
        "(progn {} (tse-excursion-window-case {}))",
        TSE_EXCURSION_WINDOW_CASE,
        r#"(lambda (original alternative caller _other)
 (select-window original) (goto-char 4) (select-window alternative)
 (let ((kill-buffer-query-functions nil)) (kill-buffer caller)))"#
    );
    tse_excursion_window_oracle_case(&form, "OK (nil nil 7 t t 1 t)");
}

#[test]
fn compat_casing_hook_mutations_keep_gnu_live_source_and_full_state() {
    if !oracle_enabled() {
        return;
    }
    let cases = [
        (
            "upcase-region-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (upcase-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 66 201 8364 119070 100 101 102) t 14 t 2 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 5) ((t (98 233 8364) (2 3 4)) . 2)) ((after 2 4 2)))"###,
        ),
        (
            "upcase-region-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (upcase-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 88 65 66 233 8364 119070 100 101 102) t 16 t 3 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 5) ((t (120 97 98) (nil 1 2)) . 2) (1 . 3)) ((before 2 5) (after 2 5 3)))"###,
        ),
        (
            "downcase-region-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (downcase-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 98 233 8364 119070 100 101 102) t 14 t 2 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 5) ((t (98 233 8364) (2 3 4)) . 2)) nil)"###,
        ),
        (
            "downcase-region-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (downcase-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 120 97 98 233 8364 119070 100 101 102) t 16 t 3 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 5) ((t (120 97 98) (nil 1 2)) . 2) (1 . 3)) ((before 2 5)))"###,
        ),
        (
            "capitalize-region-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (capitalize-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 66 233 8364 119070 100 101 102) t 14 t 2 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 5) ((t (98 233 8364) (2 3 4)) . 2)) ((after 2 3 1)))"###,
        ),
        (
            "capitalize-region-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (capitalize-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 88 97 98 233 8364 119070 100 101 102) t 16 t 3 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 5) ((t (120 97 98) (nil 1 2)) . 2) (1 . 3)) ((before 2 5) (after 2 3 1)))"###,
        ),
        (
            "upcase-initials-region-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (upcase-initials-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 66 233 8364 119070 100 101 102) t 14 t 2 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 5) ((t (98 233 8364) (2 3 4)) . 2)) ((after 2 3 1)))"###,
        ),
        (
            "upcase-initials-region-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (upcase-initials-region 2 5) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 88 97 98 233 8364 119070 100 101 102) t 16 t 3 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 5) ((t (120 97 98) (nil 1 2)) . 2) (1 . 3)) ((before 2 5) (after 2 3 1)))"###,
        ),
        (
            "upcase-word-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (upcase-word 1) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 66 201 8364 119070 100 101 102) t 14 t 4 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 4) ((t (98 233) (2 3)) . 2)) ((after 2 4 2)))"###,
        ),
        (
            "upcase-word-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (upcase-word 1) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 88 65 98 233 8364 119070 100 101 102) t 16 t 4 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 4) ((t (120 97) (nil 1)) . 2) (1 . 3)) ((before 2 4) (after 2 4 2)))"###,
        ),
        (
            "downcase-word-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (downcase-word 1) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 98 233 8364 119070 100 101 102) t 14 t 4 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 4) ((t (98 233) (2 3)) . 2)) nil)"###,
        ),
        (
            "downcase-word-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (downcase-word 1) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 120 97 98 233 8364 119070 100 101 102) t 16 t 4 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 4) ((t (120 97) (nil 1)) . 2) (1 . 3)) ((before 2 4)))"###,
        ),
        (
            "capitalize-word-none",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  nil
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (capitalize-word 1) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (97 66 233 8364 119070 100 101 102) t 14 t 4 1 9 (1 2 3 4 5 6 7 8) (1 2 5 9) (2 5) ((2 . 4) ((t (98 233) (2 3)) . 2)) ((after 2 3 1)))"###,
        ),
        (
            "capitalize-word-prefix",
            r###"(with-temp-buffer
 (insert "abé€𝄞def")
 (dotimes (i 8) (put-text-property (1+ i) (+ i 2) 'tse-index (1+ i)))
 (goto-char 2) (buffer-enable-undo) (setq buffer-undo-list nil)
 (let ((calls nil) (ran nil)
       (markers (list (copy-marker 1) (copy-marker 2) (copy-marker 5 t) (copy-marker 9)))
       (overlay (make-overlay 2 5)))
  (add-hook 'before-change-functions (lambda (beg end) (push (list 'before beg end) calls) (unless ran (setq ran t) (goto-char 1) (insert "xx"))) nil t)
  (add-hook 'after-change-functions
   (lambda (beg end old) (push (list 'after beg end old) calls)) nil t)
  (let* ((result (condition-case err (capitalize-word 1) (error err)))
         (text (buffer-string)) (chars (string-to-list text)))
   (list result chars (multibyte-string-p text) (string-bytes text)
         (equal text (apply #'string chars)) (point) (point-min) (point-max)
         (let ((i 0) props) (while (< i (length text))
          (push (get-text-property i 'tse-index text) props) (setq i (1+ i))) (nreverse props))
         (mapcar #'marker-position markers) (list (overlay-start overlay) (overlay-end overlay))
         (mapcar (lambda (entry)
          (if (and (consp entry) (stringp (car entry)))
           (cons (list (multibyte-string-p (car entry)) (string-to-list (car entry))
             (let ((i 0) props) (while (< i (length (car entry)))
              (push (get-text-property i 'tse-index (car entry)) props) (setq i (1+ i))) (nreverse props))) (cdr entry))
           (if (and (consp entry) (markerp (car entry)))
             (cons (list 'undo-marker (marker-position (car entry))
                         (marker-insertion-type (car entry))) (cdr entry))
             entry))) buffer-undo-list)
         (nreverse calls)))))"###,
            r###"OK (nil (120 88 97 98 233 8364 119070 100 101 102) t 16 t 4 1 11 (nil nil 1 2 3 4 5 6 7 8) (1 4 7 11) (4 7) ((2 . 4) ((t (120 97) (nil 1)) . 2) (1 . 3)) ((before 2 4) (after 2 3 1)))"###,
        ),
    ];
    for (name, form, expected) in cases {
        let gnu = run_oracle_eval(form).expect("GNU casing transaction oracle");
        assert_eq!(
            gnu, expected,
            "unexpected GNU casing transaction state for {name}"
        );
        let actual = run_neovm_eval(form).expect("NeoVM casing transaction result");
        assert_eq!(actual, gnu, "casing transaction mismatch for {name}");
    }
}

// GNU casefiddle.c:443-525 narrows the notification to the first and last
// changed source characters, including expansions; :570 adjusts old length.
#[test]
fn compat_expanded_casing_after_change_extents_match_gnu() {
    if !oracle_enabled() {
        return;
    }

    let form = r#"(mapcar
 (lambda (spec)
  (with-temp-buffer
   (set-buffer-multibyte (nth 3 spec))
   (insert (nth 2 spec))
   (when (nth 4 spec)
    (let ((table (copy-case-table (standard-case-table))))
     (set-case-table table)
     (if (eq (nth 4 spec) 'width)
         (let ((up (char-table-extra-slot table 0)))
          (aset up ?a ?é)
          (aset up ?é ?A))
       (set-case-syntax-pair ?İ ?i table)
       (set-case-syntax-pair ?ẞ ?ß table))))
   (let (calls)
    (add-hook 'after-change-functions
              (lambda (beg end old) (push (list beg end old) calls)) nil t)
    (let ((result (funcall (nth 1 spec) (point-min) (point-max))))
     (list (car spec) result (string-to-list (buffer-string))
           (string-bytes (buffer-string))
           (multibyte-string-p (buffer-string))
           (point) (point-max) (nreverse calls))))))
 (list
  (list 'prefix-suffix 'upcase-region "--ß--" t nil)
  (list 'separated-expansions 'upcase-region "--ß--ﬃ--" t nil)
  (list 'mixed-upcase 'upcase-region "--éaß--" t nil)
  (list 'mixed-downcase 'downcase-region "--AİZ--" t nil)
  (list 'capitalize-expansions 'capitalize-region "--ßa--ﬃb--" t nil)
  (list 'initial-expansions 'upcase-initials-region "--ßa--ﬃb--" t nil)
  (list 'multibyte-byte8 'upcase-region
        (concat "--" (string-to-multibyte (unibyte-string 128 255))
                "ßa" (string-to-multibyte (unibyte-string 255)) "--") t nil)
  (list 'unibyte 'upcase-region
        (unibyte-string 45 45 128 97 233 255 45 45) nil nil)
  (list 'custom-width 'upcase-region "--aéß--" t 'width)
  (list 'custom-specials 'upcase-region "--iß--ﬃ--" t 'specials)))"#;

    let gnu = run_oracle_eval(form).expect("GNU expanded casing extent oracle");
    assert!(gnu.starts_with("OK "), "GNU casing fixture failed: {gnu}");
    let actual = run_neovm_eval(form).expect("NeoVM expanded casing extent result");
    assert_eq!(actual, gnu, "expanded casing after-change extent mismatch");
}
