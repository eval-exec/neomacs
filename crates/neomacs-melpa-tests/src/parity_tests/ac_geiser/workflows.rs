use expect_test::expect;

use super::ParityBatchCase;

/// The installation the README prescribes: hook `ac-geiser-setup' into the
/// geiser modes and add `geiser-repl-mode' to `ac-modes'.  The command's
/// docstring promises the source goes to the *front* of `ac-sources' and that
/// this "affects only the current buffer", so this pins the resulting source
/// list, that running setup twice does not install the source twice, that
/// `ac-sources' is buffer local and another buffer is untouched, and that
/// auto-complete knows `scheme-mode' out of the box but not
/// `geiser-repl-mode' -- which is exactly why the README asks the user to add
/// it.
fn setup_puts_the_geiser_source_first_for_the_current_buffer_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_puts_the_geiser_source_first_for_the_current_buffer_only",
        r##"(progn
  (acg-test-configure)
  (let ((other (generate-new-buffer "*acg-other*"))
        (observed nil))
    (acg-test-scheme-buffer "(define (demo x) x)\n")
    (push (list :default-sources ac-sources
                :repl-mode-known (and (memq 'geiser-repl-mode ac-modes) t)
                :scheme-mode-known (and (memq 'scheme-mode ac-modes) t))
          observed)
    (ac-geiser-setup)
    (ac-geiser-setup)
    (add-to-list 'ac-modes 'geiser-repl-mode)
    (push (list :sources ac-sources
                :buffer-local (local-variable-p 'ac-sources)
                :repl-mode-known (and (memq 'geiser-repl-mode ac-modes) t))
          observed)
    (with-current-buffer other
      (push (list :other-sources ac-sources) observed))
    (nreverse observed)))"##,
        expect![
            "OK ((:default-sources #1=(ac-source-words-in-same-mode-buffers) :repl-mode-known nil :scheme-mode-known t) (:sources (ac-source-geiser . #1#) :buffer-local t :repl-mode-known t) (:other-sources #1#))"
        ],
    )
}

fn completing_at_the_repl_asks_the_live_scheme_and_inserts_the_choice() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_at_the_repl_asks_the_live_scheme_and_inserts_the_choice",
        r##"(progn
  (acg-test-start-repl)
  (add-to-list 'ac-modes 'geiser-repl-mode)
  (ac-geiser-setup)
  (goto-char (point-max))
  (insert "ca")
  (acg-test-complete)
  (let ((first (nth 0 ac-candidates))
        (result nil))
    (setq result (list :prefix ac-prefix
                       :candidates (acg-test-candidates)
                       :annotation (popup-item-symbol first)
                       :properties (text-properties-at 0 first)
                       :requests (acg-test-requests)))
    (ac-complete)
    (append result
            (list :line (acg-test-line)
                  :column (current-column)
                  :buffer (buffer-name)
                  :mode major-mode))))"##,
        expect![[
            r#"OK (:prefix "ca" :candidates ("car" "case" "cadr" "call-with-values") :annotation "g" :properties (symbol "g" document ac-geiser-documentation) :requests ("(geiser:eval #f (geiser:completions \"ca\"))") :line "car" :column 21 :buffer "*Geiser Fake REPL*" :mode geiser-repl-mode)"#
        ]],
    )
}

fn completing_in_a_scheme_buffer_merges_local_bindings_with_repl_symbols() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_in_a_scheme_buffer_merges_local_bindings_with_repl_symbols",
        r##"(progn
  (acg-test-configure)
  (let ((scheme (acg-test-scheme-buffer
                 "(define (demo cadence)\n  (let ((carriage 1))\n    ca")))
    (geiser-fake)
    (switch-to-buffer scheme)
    (goto-char (point-max))
    (ac-geiser-setup)
    (acg-test-complete)
    (list :live (and (geiser-repl--live-p) t)
          :prefix ac-prefix
          :from-geiser (ac-source-geiser-candidates)
          :candidates (acg-test-candidates)
          :requests (acg-test-requests))))"##,
        expect![[
            r#"OK (:live t :prefix "ca" :from-geiser ("carriage" "cadence" "call-with-values" "car" "case" "cadr") :candidates ("car" "case" "cadr" "cadence" "cadence" "carriage" "carriage" "call-with-values") :requests ("(geiser:eval #f (geiser:completions \"ca\"))" "(geiser:eval #f (geiser:completions \"ca\"))"))"#
        ]],
    )
    .fresh_process()
}

fn each_candidate_documents_itself_from_the_running_scheme() -> ParityBatchCase {
    ParityBatchCase::value(
        "each_candidate_documents_itself_from_the_running_scheme",
        r##"(progn
  (acg-test-start-repl)
  (ac-geiser-setup)
  (goto-char (point-max))
  (insert "ca")
  (acg-test-complete)
  (list :car (popup-item-documentation (acg-test-candidate "car"))
        :case (popup-item-documentation (acg-test-candidate "case"))
        :cadr (popup-item-documentation (acg-test-candidate "cadr"))
        :requests (acg-test-requests)))"##,
        expect![[
            r#"OK (:car #("(car pair)\n----\nReturn the contents of the car of PAIR." 1 4 (face geiser-font-lock-autodoc-identifier)) :case #("(case key clauses)\n----\nEvaluate the clause whose datum matches KEY." 1 5 (face geiser-font-lock-autodoc-identifier)) :cadr #("(cadr pair)\n----\n" 1 5 (face geiser-font-lock-autodoc-identifier)) :requests ("(geiser:eval #f (geiser:completions \"ca\"))" "(geiser:eval #f (geiser:symbol-documentation (quote car)))" "(geiser:eval #f (geiser:symbol-documentation (quote case)))" "(geiser:eval #f (geiser:symbol-documentation (quote cadr)))"))"#
        ]],
    )
    .fresh_process()
}

fn a_prefix_the_scheme_does_not_know_produces_no_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_prefix_the_scheme_does_not_know_produces_no_candidates",
        r##"(progn
  (acg-test-start-repl)
  (ac-geiser-setup)
  (goto-char (point-max))
  (insert "zzz")
  (acg-test-complete)
  (list :prefix ac-prefix
        :from-geiser (ac-source-geiser-candidates)
        :candidates (acg-test-candidates)
        :line (acg-test-line)
        :requests (acg-test-requests)))"##,
        expect![[
            r#"OK (:prefix "zzz" :from-geiser nil :candidates nil :line "zzz" :requests ("(geiser:eval #f (geiser:completions \"zzz\"))" "(geiser:eval #f (geiser:completions \"zzz\"))"))"#
        ]],
    )
    .fresh_process()
}

fn without_a_running_repl_the_source_stays_silent_and_contacts_no_scheme() -> ParityBatchCase {
    ParityBatchCase::value(
        "without_a_running_repl_the_source_stays_silent_and_contacts_no_scheme",
        r##"(progn
  (acg-test-configure)
  (acg-test-scheme-buffer "(define (cabbage x) x)\n(carriage)\nca")
  (goto-char (point-max))
  (ac-geiser-setup)
  (acg-test-complete)
  (list :live (geiser-repl--live-p)
        :from-geiser (ac-source-geiser-candidates)
        :prefix ac-prefix
        :candidates (acg-test-candidates)
        :sources ac-sources
        :requests (acg-test-requests)
        :line (acg-test-line)))"##,
        expect![[
            r#"OK (:live nil :from-geiser nil :prefix "ca" :candidates ("cabbage" "carriage") :sources (ac-source-geiser ac-source-words-in-same-mode-buffers) :requests no-request :line "ca")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_puts_the_geiser_source_first_for_the_current_buffer_only(),
        completing_at_the_repl_asks_the_live_scheme_and_inserts_the_choice(),
        completing_in_a_scheme_buffer_merges_local_bindings_with_repl_symbols(),
        each_candidate_documents_itself_from_the_running_scheme(),
        a_prefix_the_scheme_does_not_know_produces_no_candidates(),
        without_a_running_repl_the_source_stays_silent_and_contacts_no_scheme(),
    ]
}
