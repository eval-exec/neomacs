//! Behavioural parity for the pinned `ekp` Knuth-Plass line breaker.
//!
//! Probes cover the semantic layout plan (source ranges, hyphen flags), the
//! pixel string API for Latin and CJK text, range search, multiline input and
//! the overlong-token policy.  Measurement is stubbed to a deterministic
//! 10/20 px per ASCII/non-ASCII glyph so the snapshots pin the breaking
//! algorithm rather than whichever font the host resolved.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-ekp-fixed-pixel-width (string &optional _buffer)
  "Deterministic width: 10 px per ASCII glyph, 20 per non-ASCII glyph."
  (cl-loop for char across string
           sum (if (> char 127) 20 10)))

(defmacro neomacs-ekp-with-fixed-metrics (&rest body)
  "Measure with a host-independent font so the pins do not move with fonts."
  `(cl-letf (((symbol-function 'string-pixel-width)
              #'neomacs-ekp-fixed-pixel-width))
     (clrhash ekp--box-width-cache)
     ,@body))

(defun neomacs-ekp-plan-summary (plan)
  "Return PLAN's state, line count and per-line source/hyphen facts."
  (let ((lines (append (ekp-layout-plan-lines plan) nil)))
    (list (ekp-layout-plan-state plan)
          (length lines)
          (mapcar (lambda (line)
                    (list (ekp-layout-line-index line)
                          (ekp-layout-line-source-start line)
                          (ekp-layout-line-source-end line)
                          (ekp-layout-line-hyphen-p line)))
                  lines))))
"####;

fn ekp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("ekp", "1.0.1"), "ekp.el")
        .expect("prepare pinned ekp source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn ekp_layout_batch() {
    let cases = vec![
        // The semantic plan is the representation-free contract: source
        // offsets and discretionary-hyphen facts, no display glue.
        ParityBatchCase::value(
            "layout_plan_ascii",
            r####"
(neomacs-ekp-with-fixed-metrics
 (neomacs-ekp-plan-summary
  (ekp-layout-plan
   "The quick brown fox jumps over the lazy dog and keeps running." 200)))
"####,
            expect!["OK (planned 3 ((0 0 19 nil) (1 20 39 nil) (2 40 62 nil)))"],
        ),
        // The public string renderer for a Latin paragraph.
        ParityBatchCase::value(
            "pixel_justify_ascii",
            r####"
(neomacs-ekp-with-fixed-metrics
 (ekp-pixel-justify
  "The quick brown fox jumps over the lazy dog and keeps running." 300))
"####,
            expect![[
                r#"OK #("The quick brown fox jumps\nover the lazy dog and keeps\nrunning. " 3 4 (ekp-glue #1=" " display #2=(space :width (23))) 9 10 (ekp-glue #1# display #2#) 15 16 (ekp-glue #3=" " display #4=(space :width (22))) 19 20 (ekp-glue #3# display #4#) 25 26 (ekp-soft-break " ") 30 31 (ekp-glue #5=" " display #6=(space :width (16))) 34 35 (ekp-glue #5# display #6#) 39 40 (ekp-glue #5# display #6#) 43 44 (ekp-glue #5# display #6#) 47 48 (ekp-glue #5# display #6#) 53 54 (ekp-soft-break " ") 62 63 (ekp-glue "" display (space :width (220))))"#
            ]],
        ),
        // CJK text and kinsoku punctuation through the same renderer.
        ParityBatchCase::value(
            "pixel_justify_cjk",
            r####"
(neomacs-ekp-with-fixed-metrics
 (ekp-pixel-justify
  "中文排版需要遵守标点符号的行首行尾规则，例如逗号和句号不能出现在行首。" 220))
"####,
            expect![[
                r#"OK #("中文排版需要遵守标点符\n号的行首行尾规则，例如\n逗号和句号不能出现在行\n首。 " 11 12 (ekp-soft-break "") 23 24 (ekp-soft-break "") 35 36 (ekp-soft-break "") 38 39 (ekp-glue "" display (space :width (180))))"#
            ]],
        ),
        // Range search returns the justified text together with the chosen
        // width.
        ParityBatchCase::value(
            "pixel_range_justify",
            r####"
(neomacs-ekp-with-fixed-metrics
 (ekp-pixel-range-justify
  "Optimal line breaking distributes space over the whole paragraph." 180 480))
"####,
            expect![[
                r#"OK (#("Optimal line breaking distributes\nspace over the whole paragraph." 7 8 (ekp-glue #1=" " display #2=(space :width (6))) 12 13 (ekp-glue #1# display #2#) 21 22 (ekp-glue #1# display #2#) 33 34 (ekp-soft-break " ") 39 40 (ekp-glue #3=" " display #4=(space :width (10))) 44 45 (ekp-glue #3# display #4#) 48 49 (ekp-glue #3# display #4#) 54 55 (ekp-glue #3# display #4#)) . 306)"#
            ]],
        ),
        // Multiline input is one paragraph per line and preserves blank lines.
        ParityBatchCase::value(
            "multiline_and_blank_lines",
            r####"
(neomacs-ekp-with-fixed-metrics
 (ekp-pixel-justify
  "one two three four five six seven eight nine ten

alpha beta gamma delta epsilon zeta eta theta" 240))
"####,
            expect![[
                r#"OK #("one two three four five\nsix seven eight nine ten\n\nalpha beta gamma delta\nepsilon zeta eta theta " 3 4 (ekp-glue #1=" " display #2=(space :width (13))) 7 8 (ekp-glue #1# display #2#) 13 14 (ekp-glue #3=" " display #4=(space :width (12))) 18 19 (ekp-glue #3# display #4#) 23 24 (ekp-soft-break " ") 27 28 (ekp-glue #5=" " display #6=(space :width (10))) 33 34 (ekp-glue #5# display #6#) 39 40 (ekp-glue #5# display #6#) 44 45 (ekp-glue #5# display #6#) 55 56 (ekp-glue #7=" " display #8=(space :width (17))) 60 61 (ekp-glue #7# display #8#) 66 67 (ekp-glue " " display (space :width (16))) 72 73 (ekp-soft-break " ") 80 81 (ekp-glue #5# display #6#) 85 86 (ekp-glue #5# display #6#) 89 90 (ekp-glue #5# display #6#) 95 96 (ekp-glue "" display (space :width (20))))"#
            ]],
        ),
        // Ragged-right mode changes spacing decisions.
        ParityBatchCase::value(
            "ragged_right_alignment",
            r####"
(neomacs-ekp-with-fixed-metrics
 (let ((ekp-alignment 'ragged-right))
   (ekp-pixel-justify
    "Ragged right keeps natural spacing while still optimising break points." 280)))
"####,
            expect![[
                r#"OK #("Ragged right keeps natural \nspacing while still \noptimising break points. " 6 7 (ekp-glue #1=" " display #2=(space :width (10))) 12 13 (ekp-glue #1# display #2#) 18 19 (ekp-glue #1# display #2#) 26 27 (ekp-glue "" display (space :width (20))) 27 28 (ekp-soft-break " ") 35 36 (ekp-glue #1# display #2#) 41 42 (ekp-glue #1# display #2#) 47 48 (ekp-glue "" display (space :width (90))) 48 49 (ekp-soft-break " ") 59 60 (ekp-glue #1# display #2#) 65 66 (ekp-glue #1# display #2#) 73 74 (ekp-glue "" display (space :width (40))))"#
            ]],
        ),
        // An overlong unbreakable token must still produce a plan.
        ParityBatchCase::value(
            "overlong_token_policy",
            r####"
(neomacs-ekp-with-fixed-metrics
 (neomacs-ekp-plan-summary
  (ekp-layout-plan "https://example.test/a/very/long/path/that/cannot/break" 40)))
"####,
            expect![
                "OK (planned 8 ((0 0 7 nil) (1 7 21 nil) (2 21 28 nil) (3 28 33 nil) (4 33 38 nil) (5 38 43 nil) (6 43 50 nil) (7 50 55 nil)))"
            ],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("ekp parity batch");
    assert_oracle_batch_cases(ekp_oracle(), test_name, "ekp parity", &cases);
}
