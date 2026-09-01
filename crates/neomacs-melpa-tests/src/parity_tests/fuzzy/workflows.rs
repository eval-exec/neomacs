use expect_test::expect;

use super::ParityBatchCase;

fn jaro_winkler_and_match_score_rank_similar_strings() -> ParityBatchCase {
    ParityBatchCase::value(
        "jaro_winkler_and_match_score_rank_similar_strings",
        r####"
(let* ((exact (fuzzy-jaro-winkler-distance "release" "release"))
       (close (fuzzy-jaro-winkler-distance "release" "relese"))
       (far (fuzzy-jaro-winkler-distance "release" "deploy"))
       (score-exact (fuzzy-match-score "release" "release" #'fuzzy-jaro-winkler-distance))
       (score-close (fuzzy-match-score "release" "relese" #'fuzzy-jaro-winkler-distance)))
  (list :exact exact
        :close close
        :far far
        :ordered (and (> exact close) (> close far) t)
        :score-ordered (and (>= score-exact score-close) t)
        :score-exact score-exact
        :score-close score-close))
"####,
        expect![
            "OK (:exact 1.0 :close 0.9714285714285714 :far 0.5396825396825397 :ordered t :score-ordered t :score-exact 1.0 :score-close 0.9714285714285714)"
        ],
    )
}

fn all_completions_returns_scored_fuzzy_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "all_completions_returns_scored_fuzzy_matches",
        r####"
(let* ((collection '("release-train" "deploy" "reload" "train" "other"))
       (matches (fuzzy-all-completions "rel" collection)))
  (list :matches matches
        :count (length matches)
        :contains-release (and (member "release-train" matches) t)
        :contains-reload (and (member "reload" matches) t)
        :omits-deploy (not (member "deploy" matches))))
"####,
        expect![[
            r#"OK (:matches ("release-train" "reload") :count 2 :contains-release t :contains-reload t :omits-deploy t)"#
        ]],
    )
}

fn search_regexp_compile_and_forward_backward_find_fuzzy_spans() -> ParityBatchCase {
    ParityBatchCase::value(
        "search_regexp_compile_and_forward_backward_find_fuzzy_spans",
        r####"
(with-temp-buffer
  (insert "alpha release-train omega reload\n")
  (goto-char (point-min))
  (let* ((re (fuzzy-search-regexp-compile "rel"))
         (fwd (fuzzy-search-forward "rel"))
         (fwd-pos (and fwd (list (match-beginning 0) (match-end 0)
                                 (match-string-no-properties 0))))
         (bwd (progn (goto-char (point-max))
                     (fuzzy-search-backward "rel")))
         (bwd-pos (and bwd (list (match-beginning 0) (match-end 0)
                                 (match-string-no-properties 0)))))
    (list :regexp re
          :forward fwd-pos
          :backward bwd-pos)))
"####,
        expect![[
            r#"OK (:regexp "\\([er].\\{0,2\\}[el]\\|.\\{0,2\\}[elr].\\{0,2\\}\\)" :forward (7 11 "rele") :backward (27 30 "rel"))"#
        ]],
    )
}

fn isearch_activation_installs_and_removes_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "isearch_activation_installs_and_removes_hooks",
        r####"
(let ((before (memq 'fuzzy-isearch-end-hook isearch-mode-end-hook)))
  (turn-on-fuzzy-isearch)
  (let ((on (and (memq 'fuzzy-isearch-end-hook isearch-mode-end-hook) t)))
    (turn-off-fuzzy-isearch)
    (list :before (and before t)
          :on on
          :after (and (memq 'fuzzy-isearch-end-hook isearch-mode-end-hook) t)
          :active (and (boundp 'fuzzy-isearch) fuzzy-isearch))))
"####,
        expect!["OK (:before nil :on t :after nil :active nil)"],
    )
}

fn quicksilver_abbrev_scoring_prefers_prefix_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "quicksilver_abbrev_scoring_prefers_prefix_matches",
        r####"
(let ((prefix (fuzzy-quicksilver-abbrev-score "release-train" "rt"))
      (scattered (fuzzy-quicksilver-abbrev-score "release-train" "rl"))
      (none (fuzzy-quicksilver-abbrev-score "release-train" "zz"))
      (re (fuzzy-quicksilver-make-abbrev-regexp "rt")))
  (list :prefix prefix
        :scattered scattered
        :none none
        :prefix-better (and (> prefix scattered) (> scattered none) t)
        :regexp re))
"####,
        expect![[
            r#"OK (:prefix 0.823076923076923 :scattered 0.8461538461538461 :none 0.0 :prefix-better nil :regexp "^.*?\\(r\\).*?\\(t\\)")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        jaro_winkler_and_match_score_rank_similar_strings(),
        all_completions_returns_scored_fuzzy_matches(),
        search_regexp_compile_and_forward_backward_find_fuzzy_spans(),
        isearch_activation_installs_and_removes_hooks(),
        quicksilver_abbrev_scoring_prefers_prefix_matches(),
    ]
}
