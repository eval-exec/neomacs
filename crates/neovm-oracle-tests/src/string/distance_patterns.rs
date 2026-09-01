//! Advanced oracle parity tests for `string-distance` (Levenshtein) patterns:
//! identity distance, single-operation distances, symmetry verification,
//! triangle inequality across many strings, sorting by distance, fuzzy
//! matching with scoring, and spell-checker simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Identity, single operation (insert/delete/substitute), and boundary cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_single_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that identical strings have distance 0, single insert/delete/sub
    // always yields distance 1, and boundary cases (empty, single char) work.
    let form = r#"
(let ((results nil))
  ;; Identical strings => distance 0
  (dolist (s '("" "a" "hello" "abracadabra" "the quick brown fox"))
    (setq results (cons (list 'ident s (string-distance s s)) results)))

  ;; Single insertion => distance 1
  (let ((pairs '(("" "x")
                 ("abc" "abcd")
                 ("abc" "xabc")
                 ("abc" "abxc")
                 ("hello" "helloo"))))
    (dolist (p pairs)
      (let ((a (car p)) (b (cadr p)))
        (setq results (cons (list 'insert a b (string-distance a b)) results)))))

  ;; Single deletion => distance 1 (reverse of insertion)
  (let ((pairs '(("x" "")
                 ("abcd" "abc")
                 ("xabc" "abc")
                 ("abxc" "abc"))))
    (dolist (p pairs)
      (let ((a (car p)) (b (cadr p)))
        (setq results (cons (list 'delete a b (string-distance a b)) results)))))

  ;; Single substitution => distance 1
  (let ((pairs '(("a" "b")
                 ("cat" "bat")
                 ("cat" "cot")
                 ("cat" "cas")
                 ("hello" "hallo"))))
    (dolist (p pairs)
      (let ((a (car p)) (b (cadr p)))
        (setq results (cons (list 'subst a b (string-distance a b)) results)))))

  (nreverse results))
"#;
    let expect = expect_test::expect![[
        r#""OK ((ident \"\" 0) (ident \"a\" 0) (ident \"hello\" 0) (ident \"abracadabra\" 0) (ident \"the quick brown fox\" 0) (insert \"\" \"x\" 1) (insert \"abc\" \"abcd\" 1) (insert \"abc\" \"xabc\" 1) (insert \"abc\" \"abxc\" 1) (insert \"hello\" \"helloo\" 1) (delete \"x\" \"\" 1) (delete \"abcd\" \"abc\" 1) (delete \"xabc\" \"abc\" 1) (delete \"abxc\" \"abc\" 1) (subst \"a\" \"b\" 1) (subst \"cat\" \"bat\" 1) (subst \"cat\" \"cot\" 1) (subst \"cat\" \"cas\" 1) (subst \"hello\" \"hallo\" 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symmetry: d(a,b) = d(b,a) for many string pairs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_symmetry_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify symmetry across many pairs including mixed-length, repeated chars,
    // common prefixes/suffixes, and completely disjoint strings.
    let form = r#"
(let ((strings '("" "a" "ab" "abc" "abcd" "dcba" "aaaa" "bbbb"
                 "kitten" "sitting" "sunday" "saturday"
                 "algorithm" "altruistic" "pneumonoultramicroscopicsilicovolcanoconiosis"
                 "supercalifragilisticexpialidocious"))
      (all-symmetric t)
      (checked 0)
      (counterexamples nil))
  (let ((i 0))
    (dolist (a strings)
      (let ((j 0))
        (dolist (b strings)
          (when (>= j i)
            (let ((d-ab (string-distance a b))
                  (d-ba (string-distance b a)))
              (setq checked (1+ checked))
              (unless (= d-ab d-ba)
                (setq all-symmetric nil)
                (setq counterexamples
                      (cons (list a b d-ab d-ba) counterexamples)))))
          (setq j (1+ j))))
      (setq i (1+ i))))
  (list 'symmetric all-symmetric
        'pairs-checked checked
        'counterexamples counterexamples))
"#;
    let expect =
        expect_test::expect![[r#""OK (symmetric t pairs-checked 136 counterexamples nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Triangle inequality: d(a,c) <= d(a,b) + d(b,c)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_triangle_inequality_broad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exhaustively verify the metric triangle inequality across many triples,
    // including edge cases with empty strings and repeated characters.
    let form = r#"
(let ((strings '("" "a" "ab" "ba" "abc" "xyz" "aaa" "bbb"
                 "kitten" "sitting" "hello" "world" "test"))
      (violations 0)
      (total 0)
      (max-slack 0))
  (dolist (a strings)
    (dolist (b strings)
      (dolist (c strings)
        (let* ((d-ab (string-distance a b))
               (d-bc (string-distance b c))
               (d-ac (string-distance a c))
               (slack (- (+ d-ab d-bc) d-ac)))
          (setq total (1+ total))
          (when (< slack 0)
            (setq violations (1+ violations)))
          (when (> slack max-slack)
            (setq max-slack slack))))))
  (list 'violations violations
        'total total
        'all-valid (= violations 0)
        'max-slack max-slack))
"#;
    let expect =
        expect_test::expect![[r#""OK (violations 0 total 2197 all-valid t max-slack 14)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sorting candidates by distance from a target
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_sort_by_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort a list of words by their Levenshtein distance to a target word,
    // breaking ties alphabetically. Test with multiple target words.
    let form = r#"
(progn
  (fset 'neovm--sdp-sort-by-distance
    (lambda (target candidates)
      (let ((scored (mapcar (lambda (c)
                              (cons (string-distance target c) c))
                            candidates)))
        (setq scored (sort scored
                          (lambda (a b)
                            (or (< (car a) (car b))
                                (and (= (car a) (car b))
                                     (string< (cdr a) (cdr b)))))))
        scored)))

  (unwind-protect
      (let ((words '("apple" "apply" "ape" "maple" "ample"
                     "application" "appeal" "applet" "apricot"
                     "banana" "mango" "grape" "pineapple")))
        (list
          ;; Sort by distance to "apple"
          (funcall 'neovm--sdp-sort-by-distance "apple" words)
          ;; Sort by distance to "banana"
          (funcall 'neovm--sdp-sort-by-distance "banana" words)
          ;; Sort by distance to "app" (short query)
          (funcall 'neovm--sdp-sort-by-distance "app" words)
          ;; Sort by distance to "" (empty - should rank by length)
          (funcall 'neovm--sdp-sort-by-distance "" words)))
    (fmakunbound 'neovm--sdp-sort-by-distance)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . \"apple\") (1 . \"ample\") (1 . \"applet\") (1 . \"apply\") (2 . \"ape\") (2 . \"maple\") (3 . \"appeal\") (4 . \"grape\") (4 . \"pineapple\") (5 . \"apricot\") (5 . \"banana\") (5 . \"mango\") (7 . \"application\")) ((0 . \"banana\") (4 . \"mango\") (5 . \"ample\") (5 . \"ape\") (5 . \"appeal\") (5 . \"apple\") (5 . \"apply\") (5 . \"grape\") (5 . \"maple\") (6 . \"applet\") (7 . \"apricot\") (7 . \"pineapple\") (10 . \"application\")) ((1 . \"ape\") (2 . \"apple\") (2 . \"apply\") (3 . \"ample\") (3 . \"appeal\") (3 . \"applet\") (3 . \"grape\") (3 . \"maple\") (4 . \"mango\") (5 . \"apricot\") (5 . \"banana\") (6 . \"pineapple\") (8 . \"application\")) ((3 . \"ape\") (5 . \"ample\") (5 . \"apple\") (5 . \"apply\") (5 . \"grape\") (5 . \"mango\") (5 . \"maple\") (6 . \"appeal\") (6 . \"applet\") (6 . \"banana\") (7 . \"apricot\") (9 . \"pineapple\") (11 . \"application\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fuzzy matching with multi-criteria scoring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_fuzzy_match_scoring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-criteria fuzzy match scoring: combines edit distance, prefix bonus,
    // length penalty, and substring bonus. Returns top-N ranked results.
    let form = r#"
(progn
  (fset 'neovm--sdp-has-prefix
    (lambda (str prefix)
      (and (>= (length str) (length prefix))
           (string= (substring str 0 (length prefix)) prefix))))

  (fset 'neovm--sdp-contains
    (lambda (haystack needle)
      (let ((found nil) (nlen (length needle)) (hlen (length haystack)))
        (when (<= nlen hlen)
          (let ((i 0))
            (while (and (not found) (<= (+ i nlen) hlen))
              (when (string= (substring haystack i (+ i nlen)) needle)
                (setq found t))
              (setq i (1+ i)))))
        found)))

  (fset 'neovm--sdp-fuzzy-score
    (lambda (query candidate)
      (let* ((dist (string-distance query candidate))
             (maxlen (max (length query) (length candidate) 1))
             ;; Base score: inverse of normalized distance (0-1000)
             (base-score (- 1000 (/ (* 1000 dist) maxlen)))
             ;; Prefix bonus: +200 if candidate starts with query
             (prefix-bonus (if (funcall 'neovm--sdp-has-prefix candidate query)
                               200 0))
             ;; Substring bonus: +100 if query is a substring
             (substr-bonus (if (and (> (length query) 0)
                                    (funcall 'neovm--sdp-contains candidate query))
                               100 0))
             ;; Length penalty: -5 per extra char beyond query length
             (len-penalty (* 5 (max 0 (- (length candidate) (length query))))))
        (+ base-score prefix-bonus substr-bonus (- len-penalty)))))

  (fset 'neovm--sdp-fuzzy-top-n
    (lambda (query candidates n)
      (let* ((scored (mapcar (lambda (c)
                               (cons (funcall 'neovm--sdp-fuzzy-score query c) c))
                             candidates))
             (sorted (sort scored (lambda (a b) (> (car a) (car b))))))
        ;; Take top N
        (let ((result nil) (count 0))
          (while (and sorted (< count n))
            (setq result (cons (car sorted) result))
            (setq sorted (cdr sorted))
            (setq count (1+ count)))
          (nreverse result)))))

  (unwind-protect
      (let ((commands '("find-file" "find-file-other-window" "find-file-read-only"
                        "find-tag" "fill-paragraph" "fill-region"
                        "forward-char" "forward-word" "forward-line"
                        "fundamental-mode" "font-lock-mode"
                        "flycheck-mode" "flymake-mode")))
        (list
          ;; Top 5 matches for "find"
          (funcall 'neovm--sdp-fuzzy-top-n "find" commands 5)
          ;; Top 5 for "fill"
          (funcall 'neovm--sdp-fuzzy-top-n "fill" commands 5)
          ;; Top 5 for "for"
          (funcall 'neovm--sdp-fuzzy-top-n "for" commands 5)
          ;; Top 3 for "fly"
          (funcall 'neovm--sdp-fuzzy-top-n "fly" commands 3)
          ;; Top 5 for "f" (very short query)
          (funcall 'neovm--sdp-fuzzy-top-n "f" commands 5)))
    (fmakunbound 'neovm--sdp-has-prefix)
    (fmakunbound 'neovm--sdp-contains)
    (fmakunbound 'neovm--sdp-fuzzy-score)
    (fmakunbound 'neovm--sdp-fuzzy-top-n)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((780 . \"find-tag\") (720 . \"find-file\") (436 . \"find-file-read-only\") (392 . \"find-file-other-window\") (210 . \"forward-line\")) ((629 . \"fill-region\") (536 . \"fill-paragraph\") (309 . \"find-file\") (230 . \"find-tag\") (136 . \"find-file-read-only\")) ((505 . \"forward-char\") (505 . \"forward-word\") (505 . \"forward-line\") (142 . \"fill-region\") (122 . \"flymake-mode\")) ((505 . \"flymake-mode\") (481 . \"flycheck-mode\") (193 . \"find-file\")) ((390 . \"find-tag\") (372 . \"find-file\") (341 . \"fill-region\") (329 . \"forward-char\") (329 . \"forward-word\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Spell checker simulation using string-distance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_spell_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a spell checker that suggests corrections from a dictionary.
    // For each misspelled word, find all dictionary words within distance 2,
    // rank them, and return the best suggestion. Also handle already-correct words.
    let form = r#"
(progn
  (fset 'neovm--sdp-spell-suggest
    (lambda (word dictionary max-dist)
      (let ((suggestions nil))
        (dolist (dict-word dictionary)
          (let ((d (string-distance word dict-word)))
            (when (<= d max-dist)
              (setq suggestions (cons (cons d dict-word) suggestions)))))
        ;; Sort by distance then alphabetically
        (setq suggestions
              (sort suggestions
                    (lambda (a b)
                      (or (< (car a) (car b))
                          (and (= (car a) (car b))
                               (string< (cdr a) (cdr b)))))))
        suggestions)))

  (fset 'neovm--sdp-spell-check
    (lambda (words dictionary)
      (mapcar (lambda (word)
                (let ((suggestions (funcall 'neovm--sdp-spell-suggest
                                            word dictionary 2)))
                  (cond
                    ;; Exact match found (distance 0)
                    ((and suggestions (= (car (car suggestions)) 0))
                     (list word 'correct))
                    ;; Has suggestions
                    (suggestions
                     (list word 'misspelled
                           (mapcar 'cdr (let ((top nil) (n 0))
                                          (while (and suggestions (< n 3))
                                            (setq top (cons (car suggestions) top))
                                            (setq suggestions (cdr suggestions))
                                            (setq n (1+ n)))
                                          (nreverse top)))))
                    ;; No suggestions
                    (t (list word 'unknown)))))
              words)))

  (unwind-protect
      (let ((dictionary '("the" "their" "there" "then" "these" "those"
                          "this" "that" "than" "them"
                          "and" "any" "all" "are" "also"
                          "be" "been" "but" "both" "by"
                          "can" "could" "come" "car" "cat"
                          "do" "did" "does" "down"
                          "each" "even" "every"
                          "for" "from" "find" "first"
                          "get" "got" "good" "great"
                          "have" "has" "had" "help" "here")))
        (list
          ;; Check correct words
          (funcall 'neovm--sdp-spell-check
                   '("the" "and" "for" "have") dictionary)
          ;; Check misspelled words
          (funcall 'neovm--sdp-spell-check
                   '("teh" "adn" "fro" "hav") dictionary)
          ;; Check words with no close match
          (funcall 'neovm--sdp-spell-check
                   '("xyz" "qqq" "zzz") dictionary)
          ;; Mixed correct and misspelled
          (funcall 'neovm--sdp-spell-check
                   '("the" "thn" "and" "anf" "cat" "cta") dictionary)))
    (fmakunbound 'neovm--sdp-spell-suggest)
    (fmakunbound 'neovm--sdp-spell-check)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" correct) (\"and\" correct) (\"for\" correct) (\"have\" correct)) ((\"teh\" misspelled (\"be\" \"get\" \"the\")) (\"adn\" misspelled (\"all\" \"and\" \"any\")) (\"fro\" misspelled (\"from\" \"are\" \"do\")) (\"hav\" misspelled (\"had\" \"has\" \"have\"))) ((\"xyz\" misspelled (\"by\")) (\"qqq\" unknown) (\"zzz\" unknown)) ((\"the\" correct) (\"thn\" misspelled (\"than\" \"the\" \"then\")) (\"and\" correct) (\"anf\" misspelled (\"and\" \"any\" \"all\")) (\"cat\" correct) (\"cta\" misspelled (\"can\" \"car\" \"cat\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Byte-mode vs char-mode distance comparison with ASCII strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_distance_byte_vs_char_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For pure ASCII strings, byte mode (3rd arg t) should match char mode.
    // Verify this property and also test that distances combine correctly
    // in a pipeline that computes edit distance matrices.
    let form = r#"
(let ((strings '("" "a" "ab" "abc" "hello" "world" "test" "testing"
                 "algorithm" "logarithm" "kitten" "sitting"))
      (all-match t)
      (matrix nil))
  ;; Verify byte=char for ASCII strings
  (dolist (a strings)
    (dolist (b strings)
      (let ((d-char (string-distance a b))
            (d-byte (string-distance a b t)))
        (unless (= d-char d-byte)
          (setq all-match nil)))))

  ;; Build a distance matrix for a subset
  (let ((subset '("cat" "bat" "hat" "car" "bar" "cab")))
    (setq matrix
          (mapcar (lambda (a)
                    (cons a (mapcar (lambda (b)
                                     (string-distance a b))
                                   subset)))
                  subset)))

  ;; Find the pair with maximum distance and minimum non-zero distance
  (let ((max-dist 0) (max-pair nil)
        (min-dist most-positive-fixnum) (min-pair nil))
    (let ((subset '("cat" "bat" "hat" "car" "bar" "cab")))
      (dolist (a subset)
        (dolist (b subset)
          (unless (string= a b)
            (let ((d (string-distance a b)))
              (when (> d max-dist)
                (setq max-dist d max-pair (list a b)))
              (when (< d min-dist)
                (setq min-dist d min-pair (list a b))))))))
    (list 'ascii-byte-eq-char all-match
          'matrix matrix
          'max-pair (list max-dist max-pair)
          'min-pair (list min-dist min-pair))))
"#;
    let expect = expect_test::expect![[
        r#""OK (ascii-byte-eq-char t matrix ((\"cat\" 0 1 1 1 2 1) (\"bat\" 1 0 1 2 1 2) (\"hat\" 1 1 0 2 2 2) (\"car\" 1 2 2 0 1 1) (\"bar\" 2 1 2 1 0 2) (\"cab\" 1 2 2 1 2 0)) max-pair (2 (\"cat\" \"bar\")) min-pair (1 (\"cat\" \"bat\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
