//! Advanced oracle parity tests for `char-width` and related width calculations.
//!
//! Tests char-width with ASCII, wide CJK characters, combining characters,
//! tab/control chars, emoji, variation selectors, and integration with
//! string-width for alignment and padding calculations.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ---------------------------------------------------------------------------
// char-width across the full ASCII range and special categories
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_ascii_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test char-width on printable ASCII, space, digits, letters,
    // punctuation, and verify consistency with string-width of single-char strings.
    let form = r#"(let ((results nil))
  ;; All printable ASCII 32-126 should have char-width 1
  (let ((i 32)
        (all-one t))
    (while (<= i 126)
      (unless (= (char-width i) 1)
        (setq all-one nil))
      (setq i (1+ i)))
    (setq results (cons all-one results)))
  ;; Specific spot checks
  (setq results (cons (char-width ?A) results))
  (setq results (cons (char-width ?z) results))
  (setq results (cons (char-width ?0) results))
  (setq results (cons (char-width ?9) results))
  (setq results (cons (char-width ?\s) results))
  (setq results (cons (char-width ?~) results))
  (setq results (cons (char-width ?!) results))
  ;; Verify char-width matches string-width for single chars
  (let ((consistent t))
    (dolist (ch '(?A ?z ?0 ?\s ?!))
      (unless (= (char-width ch) (string-width (string ch)))
        (setq consistent nil)))
    (setq results (cons consistent results)))
  (nreverse results))"#;
    let expect = expect_test::expect![r#""OK (t 1 1 1 1 1 1 1 t)""#];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// CJK wide characters (width 2) across multiple Unicode blocks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_cjk_wide() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CJK Unified Ideographs, Hiragana, Katakana, Hangul Syllables,
    // fullwidth Latin, and other East Asian wide characters.
    let form = r#"(list
  ;; CJK Unified Ideographs (U+4E00-U+9FFF)
  (char-width #x4e00)   ;; first CJK ideograph
  (char-width #x4e16)   ;; 世
  (char-width #x9fff)   ;; last in block
  ;; Hiragana (U+3040-U+309F)
  (char-width #x3042)   ;; あ
  (char-width #x304b)   ;; か
  ;; Katakana (U+30A0-U+30FF)
  (char-width #x30a2)   ;; ア
  (char-width #x30ab)   ;; カ
  ;; Hangul Syllables (U+AC00-U+D7AF)
  (char-width #xac00)   ;; 가
  (char-width #xd7a3)   ;; last Hangul syllable
  ;; Fullwidth Latin (U+FF01-U+FF60)
  (char-width #xff21)   ;; Ａ fullwidth
  (char-width #xff41)   ;; ａ fullwidth
  ;; CJK Compatibility Ideographs (U+F900-U+FAFF)
  (char-width #xf900)
  ;; Verify all are width 2 via batch check
  (let ((all-two t))
    (dolist (ch (list #x4e16 #x3042 #x30a2 #xac00 #xff21))
      (unless (= (char-width ch) 2)
        (setq all-two nil)))
    all-two)
  ;; String-width of CJK string equals 2 * length
  (let ((s "\u4e16\u754c\u4f60\u597d"))
    (list (length s)
          (string-width s)
          (= (string-width s) (* 2 (length s))))))"#;
    let expect = expect_test::expect![r#""OK (2 2 2 2 2 2 2 2 2 2 2 2 t (4 8 t))""#];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Combining characters (width 0) and decomposed forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_combining_marks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combining characters take no additional column space.
    // Test combining accents, diacritics, and verify string-width behavior.
    let form = r#"(list
  ;; Combining acute accent U+0301
  (char-width #x0301)
  ;; Combining grave accent U+0300
  (char-width #x0300)
  ;; Combining tilde U+0303
  (char-width #x0303)
  ;; Combining diaeresis U+0308
  (char-width #x0308)
  ;; Combining cedilla U+0327
  (char-width #x0327)
  ;; Combining overline U+0305
  (char-width #x0305)
  ;; String with base + combining: should be width of base char only
  (string-width (string ?e #x0301))       ;; e + acute = width 1
  (string-width (string ?a #x0300 #x0301)) ;; a + 2 combiners = width 1
  ;; Pre-composed vs decomposed comparison
  (let ((precomposed (string #x00e9))       ;; é precomposed
        (decomposed (string ?e #x0301)))    ;; e + combining acute
    (list (string-width precomposed)
          (string-width decomposed)
          (= (string-width precomposed) (string-width decomposed))))
  ;; CJK base + combining: width 2 (base) + 0 (combiner)
  (string-width (string #x4e16 #x0301))
  ;; Multiple combining marks stacked
  (string-width (string ?x #x0300 #x0301 #x0302 #x0303)))"#;
    let expect = expect_test::expect![r#""OK (0 0 0 0 0 0 1 1 (1 1 t) 2 1)""#];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Control characters and tab
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_control_and_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Control chars (0-31, 127) have special display widths.
    // Tab width depends on tab-width variable. Test both char-width and string-width.
    let form = r#"(list
  ;; Tab character
  (char-width ?\t)
  ;; NUL
  (char-width 0)
  ;; BEL (bell)
  (char-width 7)
  ;; Backspace
  (char-width 8)
  ;; Newline
  (char-width ?\n)
  ;; Carriage return
  (char-width ?\r)
  ;; ESC
  (char-width 27)
  ;; DEL (127)
  (char-width 127)
  ;; Control chars in strings
  (string-width (string 0))
  (string-width (string 1))
  (string-width (string 7))
  (string-width (string 27))
  (string-width (string 127))
  ;; String with mixed control + printable
  (string-width (concat "abc" (string 0) "def"))
  ;; Collect char-width of all C0 controls (0-31)
  (let ((widths nil))
    (let ((i 0))
      (while (< i 32)
        (setq widths (cons (char-width i) widths))
        (setq i (1+ i))))
    (nreverse widths)))"#;
    let expect = expect_test::expect![
        r#""OK (8 2 2 2 0 2 2 2 2 2 2 2 2 8 (2 2 2 2 2 2 2 2 2 8 0 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2))""#
    ];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Emoji and variation selectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_emoji_variation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Emoji can be narrow or wide depending on Unicode properties.
    // Variation selectors (U+FE00-U+FE0F) have width 0.
    let form = r#"(list
  ;; Some basic emoji codepoints
  (char-width #x2764)   ;; ❤ heavy heart (text presentation default)
  (char-width #x2603)   ;; ☃ snowman
  (char-width #x2615)   ;; ☕ hot beverage
  (char-width #x263a)   ;; ☺ smiling face
  ;; Variation selectors have width 0
  (char-width #xfe00)   ;; VS1
  (char-width #xfe01)   ;; VS2
  (char-width #xfe0e)   ;; VS15 (text)
  (char-width #xfe0f)   ;; VS16 (emoji)
  ;; Emoji with variation selector string-width
  (string-width (string #x2764 #xfe0f))  ;; heart + emoji VS
  (string-width (string #x2764 #xfe0e))  ;; heart + text VS
  ;; Some wider emoji (in Emacs behavior)
  (char-width #x1f600)  ;; 😀 grinning face
  (char-width #x1f4a9)  ;; 💩 pile of poo
  (char-width #x1f680)  ;; 🚀 rocket
  ;; String width of emoji sequences
  (string-width (string #x1f600))
  (string-width (string #x1f4a9)))"#;
    let expect = expect_test::expect![r#""OK (1 1 2 1 0 0 0 0 1 1 2 2 2 2 2)""#];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Width-aware string truncation with char-width integration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_truncation_algorithm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a display-width-aware truncation function that uses char-width
    // per character, handles CJK, combining marks, and partial-width edge cases.
    let form = r#"(progn
  (fset 'neovm--cw-truncate
    (lambda (str max-width suffix)
      "Truncate STR to MAX-WIDTH display columns, appending SUFFIX if truncated.
       Handles wide chars by never exceeding max-width."
      (let ((sw (string-width str)))
        (if (<= sw max-width)
            (cons str nil)  ;; (result . was-truncated)
          (let* ((suffix-w (string-width suffix))
                 (budget (- max-width suffix-w))
                 (accum 0)
                 (i 0)
                 (len (length str))
                 (done nil))
            (while (and (< i len) (not done))
              (let ((cw (char-width (aref str i))))
                (if (<= (+ accum cw) budget)
                    (progn (setq accum (+ accum cw))
                           (setq i (1+ i)))
                  (setq done t))))
            (cons (concat (substring str 0 i) suffix) t))))))

  (fset 'neovm--cw-pad-right
    (lambda (str width)
      "Pad STR with spaces on right to fill exactly WIDTH display columns."
      (let ((sw (string-width str)))
        (if (>= sw width)
            str
          (concat str (make-string (- width sw) ?\s))))))

  (fset 'neovm--cw-format-table
    (lambda (rows col-widths)
      "Format a list of rows (each a list of strings) into aligned columns."
      (let ((formatted nil))
        (dolist (row rows)
          (let ((cells nil)
                (cols row)
                (ws col-widths))
            (while (and cols ws)
              (let* ((trunc-result (funcall 'neovm--cw-truncate (car cols) (car ws) ".."))
                     (truncated (car trunc-result))
                     (padded (funcall 'neovm--cw-pad-right truncated (car ws))))
                (setq cells (cons padded cells)))
              (setq cols (cdr cols))
              (setq ws (cdr ws)))
            (setq formatted (cons (mapconcat #'identity (nreverse cells) "|") formatted))))
        (nreverse formatted))))

  (unwind-protect
      (let ((widths '(12 8 6)))
        (let ((table (funcall 'neovm--cw-format-table
                       (list
                         (list "Name" "City" "Pts")
                         (list "Alice" "Boston" "100")
                         (list "\u5f20\u4e09\u4e30" "\u4e1c\u4eac\u90fd" "88")
                         (list "LongNameHere" "Xyzzy" "42")
                         (list "A\u4e2d\u6587Test" "\u5317\u4eac" "7"))
                       widths)))
          (list
            ;; All formatted rows
            table
            ;; Verify all rows have identical string-width
            (let ((first-w (string-width (car table)))
                  (all-same t))
              (dolist (row (cdr table))
                (unless (= (string-width row) first-w)
                  (setq all-same nil)))
              (list first-w all-same))
            ;; Individual truncation tests
            (funcall 'neovm--cw-truncate "hello" 10 "..")
            (funcall 'neovm--cw-truncate "\u4e16\u754c\u4f60\u597d\u6d4b\u8bd5" 8 "..")
            (funcall 'neovm--cw-truncate "A\u4e2dB\u6587C\u5b57D" 7 "..")
            ;; Edge: budget exactly matches a wide char boundary
            (funcall 'neovm--cw-truncate "\u4e16\u754c" 4 "..")
            ;; Edge: budget falls in middle of a wide char
            (funcall 'neovm--cw-truncate "\u4e16\u754c" 3 ".."))))
    (fmakunbound 'neovm--cw-truncate)
    (fmakunbound 'neovm--cw-pad-right)
    (fmakunbound 'neovm--cw-format-table)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Name        |City    |Pts   \" \"Alice       |Boston  |100   \" \"张三丰      |东京都  |88    \" \"LongNameHere|Xyzzy   |42    \" \"A中文Test   |北京    |7     \") (28 t) (\"hello\") (\"世界你..\" . t) (\"A中B..\" . t) (\"世界\") (\"..\" . t))""#
    ]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Width calculations used in word-wrap / line-break algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_word_wrap_ascii_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = char_width_word_wrap_form(r#"(funcall 'neovm--cw-wrap "hello world foo bar" 10)"#);
    let expect = expect_test::expect![[r#""OK (\" hello\" \"dlrow foo\" \"rab\")""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

fn char_width_word_wrap_form(body: &str) -> String {
    format!(
        r#"(fset 'neovm--cw-wrap
  (lambda (text max-width)
    (let ((lines nil)
          (current-line nil)
          (current-width 0)
          (word nil)
          (word-width 0)
          (i 0)
          (len (length text)))
      (while (< i len)
        (let* ((ch (aref text i))
               (cw (char-width ch))
               (is-space (= ch ?\s))
               (is-cjk (>= cw 2)))
          (cond
           (is-space
            (when word
              (let ((w-str (concat (nreverse word))))
                (if (> (+ current-width word-width) max-width)
                    (progn
                      (when current-line
                        (setq lines (cons (concat (nreverse current-line)) lines)))
                      (setq current-line (append (string-to-list w-str) nil))
                      (setq current-width word-width))
                  (setq current-line (append (nreverse (cons ?\s (string-to-list w-str))) current-line))
                  (setq current-width (+ current-width word-width (if current-line 1 0)))))
              (setq word nil word-width 0))
            (when (and current-line (> (+ current-width 1) max-width))
              (setq lines (cons (concat (nreverse current-line)) lines))
              (setq current-line nil current-width 0)))
           (is-cjk
            (when word
              (let ((w-str (concat (nreverse word))))
                (if (> (+ current-width word-width) max-width)
                    (progn
                      (when current-line
                        (setq lines (cons (concat (nreverse current-line)) lines)))
                      (setq current-line (string-to-list w-str))
                      (setq current-width word-width))
                  (dolist (c (string-to-list w-str))
                    (setq current-line (cons c current-line)))
                  (setq current-width (+ current-width word-width))))
              (setq word nil word-width 0))
            (when (> (+ current-width cw) max-width)
              (when current-line
                (setq lines (cons (concat (nreverse current-line)) lines)))
              (setq current-line nil current-width 0))
            (setq current-line (cons ch current-line))
            (setq current-width (+ current-width cw)))
           (t
            (setq word (cons ch word))
            (setq word-width (+ word-width cw)))))
        (setq i (1+ i)))
      (when word
        (let ((w-str (concat (nreverse word))))
          (if (> (+ current-width word-width) max-width)
              (progn
                (when current-line
                  (setq lines (cons (concat (nreverse current-line)) lines)))
                (setq current-line (string-to-list w-str))
                (setq current-width word-width))
            (dolist (c (string-to-list w-str))
              (setq current-line (cons c current-line)))
            (setq current-width (+ current-width word-width)))))
      (when current-line
        (setq lines (cons (concat (nreverse current-line)) lines)))
      (nreverse lines))))

{body}"#
    )
}

#[test]
fn oracle_prop_char_width_word_wrap_cjk_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = char_width_word_wrap_form(
        r#"(funcall 'neovm--cw-wrap "\u4e16\u754c\u4f60\u597d\u4e2d\u6587\u6d4b\u8bd5" 6)"#,
    );
    let expect = expect_test::expect![[r#""OK (\"世界你\" \"好中文\" \"测试\")""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_char_width_word_wrap_mixed_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = char_width_word_wrap_form(
        r#"(funcall 'neovm--cw-wrap "Hi\u4e16\u754cWorld\u4f60\u597d" 8)"#,
    );
    let expect = expect_test::expect![[r#""OK (\"Hi世界\" \"dlroW你\" \"好\")""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_char_width_word_wrap_long_word_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = char_width_word_wrap_form(r#"(funcall 'neovm--cw-wrap "abcdefghij" 5)"#);
    let expect = expect_test::expect![[r#""OK (\"jihgfedcba\")""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_char_width_word_wrap_empty_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = char_width_word_wrap_form(r#"(funcall 'neovm--cw-wrap "" 10)"#);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_char_width_word_wrap_width_bound_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = char_width_word_wrap_form(
        r#"(let ((lines (funcall 'neovm--cw-wrap "The \u4e16\u754c is \u7f8e\u4e3d" 8))
         (all-ok t))
  (dolist (line lines)
    (when (> (string-width line) 8)
      (setq all-ok nil)))
  (list lines all-ok))"#,
    );
    let expect = expect_test::expect![[r#""OK ((\" The世界\" \" is美丽\") t)""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Halfwidth / fullwidth conversions and width comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_width_halfwidth_fullwidth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare halfwidth vs fullwidth forms, halfwidth Katakana,
    // and build a width-class categorizer.
    let form = r#"(progn
  (fset 'neovm--cw-classify
    (lambda (ch)
      "Classify character by display width: narrow, wide, zero, or special."
      (let ((w (char-width ch)))
        (cond
          ((= w 0) 'zero)
          ((= w 1) 'narrow)
          ((= w 2) 'wide)
          (t (list 'other w))))))

  (unwind-protect
      (list
        ;; Halfwidth vs fullwidth Latin
        (list (char-width ?A) (char-width #xff21))   ;; A vs Ａ
        (list (char-width ?a) (char-width #xff41))   ;; a vs ａ
        ;; Halfwidth Katakana (U+FF65-U+FF9F) vs fullwidth
        (list (char-width #xff71) (char-width #x30a2))  ;; ｱ vs ア
        (list (char-width #xff72) (char-width #x30a4))  ;; ｲ vs イ
        ;; Fullwidth digits
        (list (char-width ?0) (char-width #xff10))   ;; 0 vs ０
        (list (char-width ?9) (char-width #xff19))   ;; 9 vs ９
        ;; Classify a range of characters
        (mapcar (lambda (ch) (list ch (funcall 'neovm--cw-classify ch)))
                (list ?A #xff21 #x4e16 #x0301 ?\t ?0 #x30a2 #xff71))
        ;; String-width comparison: halfwidth vs fullwidth strings
        (let ((hw "ABC123")
              (fw "\uff21\uff22\uff23\uff11\uff12\uff13"))
          (list (length hw) (string-width hw)
                (length fw) (string-width fw)
                (= (length hw) (length fw))
                (= (string-width fw) (* 2 (string-width hw)))))
        ;; Latin extended characters (accented) should be narrow
        (list (char-width #x00e9)   ;; é
              (char-width #x00f1)   ;; ñ
              (char-width #x00fc)   ;; ü
              (char-width #x00c0))) ;; À
    (fmakunbound 'neovm--cw-classify)))"#;
    let expect = expect_test::expect![
        r#""OK ((1 2) (1 2) (1 2) (1 2) (1 2) (1 2) ((65 narrow) (65313 wide) (19990 wide) (769 zero) (9 (other 8)) (48 narrow) (12450 wide) (65393 narrow)) (6 6 6 12 t t) (1 1 1 1))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
