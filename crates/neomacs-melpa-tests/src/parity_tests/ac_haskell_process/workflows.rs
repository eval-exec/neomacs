use expect_test::expect;

use super::ParityBatchCase;

/// What auto-complete asks the source before it asks for anything else.  The
/// four cells of `ac-source-haskell-process' are the whole contract, and the
/// `available' cell is consulted in each of the three buffer states a user is
/// ever in: an ordinary buffer, a `haskell-mode' buffer, and the REPL's
/// `haskell-interactive-mode' buffer.
///
/// The pairing is the point.  In a Haskell buffer with no session running the
/// source answers that it *is* available - its own docstring says "are (or might
/// later be) available" - and then, asked for candidates, returns nil, because
/// `ac-haskell-process-candidates' does nothing without a session.  So the
/// source is offered to the user in exactly the state where it has nothing to
/// offer, which is the behaviour a completion front end has to tolerate.
fn the_source_is_offered_in_haskell_buffers_and_stays_empty_without_a_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_source_is_offered_in_haskell_buffers_and_stays_empty_without_a_session",
        r##"(let ((buffer (ac-haskell-test-open)))
  (list
   (list :cells (mapcar #'car ac-source-haskell-process)
         :available (cdr (assq 'available ac-source-haskell-process))
         :candidates (cdr (assq 'candidates ac-source-haskell-process))
         :document (cdr (assq 'document ac-source-haskell-process))
         :symbol (cdr (assq 'symbol ac-source-haskell-process)))
   (with-temp-buffer
     (list :ordinary-buffer (copy-sequence (ac-haskell-process-available-p))))
   (ac-haskell-test-in buffer
     (goto-char (point-max))
     (list :haskell-mode-buffer (copy-sequence (ac-haskell-process-available-p))
           :session (haskell-session-maybe)
           :candidates (let ((ac-prefix "ma")) (ac-haskell-process-candidates))
           :candidates-for-import (let ((ac-prefix "Data.L"))
                                    (ac-haskell-process-candidates))))
   (with-temp-buffer
     (haskell-interactive-mode)
     (list :repl-buffer (copy-sequence (ac-haskell-process-available-p))
           :candidates (let ((ac-prefix "ma")) (ac-haskell-process-candidates))))))"##,
        expect![[
            r#"OK ((:cells (available candidates document symbol) :available ac-haskell-process-available-p :candidates ac-haskell-process-candidates :document ac-haskell-process-doc :symbol "h") (:ordinary-buffer nil) (:haskell-mode-buffer (haskell-mode haskell-interactive-mode) :session nil :candidates nil :candidates-for-import nil) (:repl-buffer (haskell-interactive-mode) :candidates nil))"#
        ]],
    )
}

fn setting_up_adds_the_source_to_this_buffer_and_leaves_the_default_alone() -> ParityBatchCase {
    ParityBatchCase::value(
        "setting_up_adds_the_source_to_this_buffer_and_leaves_the_default_alone",
        r##"(let ((global-before (copy-sequence (default-value 'ac-sources)))
      (buffer (ac-haskell-test-open)))
  (ac-haskell-test-in buffer
    (let ((local-before (local-variable-p 'ac-sources)))
      (ac-haskell-process-setup)
      (let ((once (list :local (local-variable-p 'ac-sources)
                        :sources (copy-sequence ac-sources))))
        (ac-haskell-process-setup)
        (let ((twice (copy-sequence ac-sources)))
          (auto-complete-mode 1)
          (ac-haskell-process-setup)
          (let ((with-mode (list :local (local-variable-p 'ac-sources)
                                 :sources (copy-sequence ac-sources))))
            (auto-complete-mode -1)
            (list :local-before local-before
                  :after-one-call once
                  :after-two-calls twice
                  :idempotent (equal (plist-get once :sources) twice)
                  :with-auto-complete-mode with-mode
                  :global-before global-before
                  :global-after (copy-sequence (default-value 'ac-sources))
                  :global-untouched (equal global-before
                                           (default-value 'ac-sources)))))))))"##,
        expect![
            "OK (:local-before nil :after-one-call (:local t :sources (ac-source-haskell-process ac-source-words-in-same-mode-buffers)) :after-two-calls (ac-source-haskell-process ac-source-words-in-same-mode-buffers) :idempotent t :with-auto-complete-mode (:local t :sources (ac-source-haskell-process ac-source-words-in-same-mode-buffers)) :global-before (ac-source-words-in-same-mode-buffers) :global-after (ac-source-words-in-same-mode-buffers) :global-untouched t)"
        ],
    )
}

fn documentation_is_fetched_from_hoogle_with_the_symbol_quoted_for_the_shell() -> ParityBatchCase {
    ParityBatchCase::value(
        "documentation_is_fetched_from_hoogle_with_the_symbol_quoted_for_the_shell",
        r##"(let ((log (ac-haskell-test-install-hoogle
            "Prelude map :: (a -> b) -> [a] -> [b]\n\nmap f xs applies f to each element of xs.\n")))
  (let* ((plain (ac-haskell-process-doc "map"))
         (dangerous (ac-haskell-process-doc "Data.List.(++)$;rm -rf"))
         (missing (let ((exec-path '("/nonexistent")))
                    (list :found (executable-find "hoogle")
                          :doc (ac-haskell-process-doc "map")))))
    (list :arguments (ac-haskell-test-hoogle-arguments log)
          :plain plain
          :dangerous-symbol-returned (equal plain dangerous)
          :without-hoogle missing)))"##,
        expect![[
            r#"OK (:arguments ("--info" "map" "--info" "Data.List.(++)$;rm -rf") :plain "Prelude map :: (a -> b) -> [a] -> [b]\n\nmap f xs applies f to each element of xs.\n" :dangerous-symbol-returned t :without-hoogle (:found nil :doc nil))"#
        ]],
    )
}

fn the_documentation_popup_anchors_on_the_symbol_and_is_silent_without_hoogle() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_documentation_popup_anchors_on_the_symbol_and_is_silent_without_hoogle",
        r##"(let ((buffer (ac-haskell-test-open)))
  (ac-haskell-test-in buffer
    (let ((probe (lambda (search)
                   (goto-char (point-min))
                   (search-forward search)
                   (backward-char 1)
                   (list :at (buffer-substring-no-properties
                              (line-beginning-position) (line-end-position))
                         :point (point)
                         :symbol (let ((symbol (symbol-at-point)))
                                   (and symbol (symbol-name symbol)))
                         :in-string (and (in-string-p) t)
                         :start (ac-haskell-process-symbol-start-pos)))))
      (list (funcall probe "putStrLn")
            (funcall probe "hello")
            (progn (goto-char (point-max))
                   (list :at-end-of-buffer (symbol-at-point)
                         :start (ac-haskell-process-symbol-start-pos)))
            (let ((exec-path '("/nonexistent")))
              (goto-char (point-min))
              (search-forward "putStrLn")
              (backward-char 1)
              (let ((before (list (point) (buffer-string))))
                (ac-haskell-process-popup-doc)
                (list :returned t
                      :point-unmoved (equal (car before) (point))
                      :buffer-unchanged (equal (cadr before) (buffer-string)))))))))"##,
        expect![[
            r#"OK ((:at "main = putStrLn (map id \"hello\")" :point 66 :symbol "putStrLn" :in-string nil :start 59) (:at "main = putStrLn (map id \"hello\")" :point 81 :symbol "hello" :in-string t :start nil) (:at-end-of-buffer nil :start nil) (:returned t :point-unmoved t :buffer-unchanged t))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_source_is_offered_in_haskell_buffers_and_stays_empty_without_a_session(),
        setting_up_adds_the_source_to_this_buffer_and_leaves_the_default_alone(),
        documentation_is_fetched_from_hoogle_with_the_symbol_quoted_for_the_shell(),
        the_documentation_popup_anchors_on_the_symbol_and_is_silent_without_hoogle(),
    ]
}
