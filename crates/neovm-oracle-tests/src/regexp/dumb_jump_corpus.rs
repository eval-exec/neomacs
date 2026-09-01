//! Real-world regexp oracle corpus imported from dumb-jump.
//!
//! Upstream: <https://github.com/jacktasia/dumb-jump>
//! Revision: cf06b4ccdce6a39346c32f05139f9ee8b77ee229 (2026-06-03)
//! License: GPL-3.0 (compatible with this GPL-3.0-or-later workspace)
//!
//! dumb-jump's rules use a small search-tool-oriented dialect shared by
//! PCRE2/rg, ag, and grep.  The translator below converts the regular subset
//! to GNU Emacs syntax.  Rules requiring lookaround are deliberately listed
//! in `UNTRANSLATABLE_RULES`; GNU Emacs regexps have no lookaround operator.

use crate::common::{eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set};

#[derive(Debug)]
struct UpstreamRule {
    index: usize,
    language: &'static str,
    pattern: &'static str,
    positive: &'static [&'static str],
    negative: &'static [&'static str],
}

#[path = "data.rs"]
mod data;

use data::UPSTREAM_RULES;

const UPSTREAM_COMMIT: &str = "cf06b4ccdce6a39346c32f05139f9ee8b77ee229";

/// These patterns use PCRE lookaround, which has no GNU Emacs regexp
/// equivalent.  Keeping the audit explicit prevents a future import from
/// silently dropping upstream cases.
const UNTRANSLATABLE_RULES: &[(usize, &str)] = &[
    (36, "C++ variable rule uses negative lookahead"),
    (205, "Haskell function rule uses negative lookahead"),
    (
        207,
        "Haskell constructor rule uses negative lookahead and lookbehind",
    ),
];

#[derive(Debug)]
struct Case {
    rule_index: usize,
    language: &'static str,
    expected: bool,
    input: &'static str,
    pattern: String,
}

fn is_untranslatable_rule(index: usize) -> bool {
    UNTRANSLATABLE_RULES
        .iter()
        .any(|(untranslatable_index, _)| *untranslatable_index == index)
}

/// Translate dumb-jump's generic search pattern to GNU Emacs regexp syntax.
///
/// The upstream dialect uses PCRE grouping/alternation/repetition, `\\s` for
/// whitespace, `\\d` for a digit, and `\\j` for its custom Lisp-friendly
/// identifier boundary.  Escaped PCRE grouping characters are literals.
fn translate_pattern(pattern: &str) -> Result<String, String> {
    if pattern.contains(r"(?!")
        || pattern.contains(r"(?=")
        || pattern.contains(r"(?<")
        || pattern.contains(r"(?P")
    {
        return Err("PCRE lookaround/named group is not translatable".into());
    }

    let pattern = pattern
        .replace("JJJ", "test")
        // Character classes containing literal brackets are awkward to carry
        // between PCRE and Emacs.  Expand the two forms used upstream into
        // ordinary alternations before the general conversion.
        .replace(r"[\w\[\]]", r"(?:\w|\[|\])")
        .replace(r"[\w\[\]<>,\.?]", r"(?:\w|\[|\]|[<>,.?])");
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() * 2);
    let mut i = 0;
    let mut in_class = false;
    let mut in_interval = false;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if !in_class {
                match next {
                    's' => out.push_str(r"[[:space:]]"),
                    'd' => out.push_str(r"[[:digit:]]"),
                    // PCRE's `\\w` includes underscore; Emacs `\\w` follows
                    // the current syntax table and does not necessarily do so.
                    'w' => out.push_str(r"[[:word:]_]"),
                    'W' => out.push_str(r"[^[:word:]_]"),
                    'b' => out.push_str(r"\b"),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    // PCRE escapes this as a literal.  In Emacs `\\=` is an
                    // anchor at point, so retaining the slash changes meaning.
                    '=' => out.push('='),
                    'j' => out.push_str(r"\(?:[^a-zA-Z0-9?*-]\|$\)"),
                    '(' | ')' | '|' | '{' | '}' => out.push(next),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            } else {
                match next {
                    's' => out.push_str(r"[:space:]"),
                    'd' => out.push_str(r"[:digit:]"),
                    'w' => out.push_str(r"[:word:]_"),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
            i += 2;
            continue;
        }

        if in_class {
            out.push(ch);
            if ch == ']' {
                in_class = false;
            }
            i += 1;
            continue;
        }

        match ch {
            '[' => {
                in_class = true;
                out.push(ch);
            }
            '(' if chars.get(i + 1) == Some(&'?') && chars.get(i + 2) == Some(&':') => {
                out.push_str(r"\(?:");
                i += 3;
                continue;
            }
            '(' => out.push_str(r"\("),
            ')' => out.push_str(r"\)"),
            '|' => out.push_str(r"\|"),
            '{' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => {
                in_interval = true;
                out.push_str(r"\{");
            }
            '}' if in_interval => {
                in_interval = false;
                out.push_str(r"\}");
            }
            _ => out.push(ch),
        }
        i += 1;
    }

    Ok(out)
}

/// Emit an escaping-free Elisp string form so arbitrary upstream source text
/// cannot accidentally alter the generated batch form.
fn elisp_string(value: &str) -> String {
    if value.is_empty() {
        return String::from("\"\"");
    }
    let mut out = String::from("(string");
    for ch in value.chars() {
        out.push(' ');
        out.push_str(&(ch as u32).to_string());
    }
    out.push(')');
    out
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for rule in UPSTREAM_RULES
        .iter()
        .filter(|rule| !is_untranslatable_rule(rule.index))
    {
        let pattern = translate_pattern(rule.pattern).unwrap_or_else(|reason| {
            panic!(
                "unlisted untranslatable dumb-jump rule {} ({}): {reason}",
                rule.index, rule.language
            )
        });
        cases.extend(rule.positive.iter().map(|input| Case {
            rule_index: rule.index,
            language: rule.language,
            expected: true,
            input,
            pattern: pattern.clone(),
        }));
        cases.extend(rule.negative.iter().map(|input| Case {
            rule_index: rule.index,
            language: rule.language,
            expected: false,
            input,
            pattern: pattern.clone(),
        }));
    }
    cases
}

fn build_batch(cases: &[Case]) -> String {
    let mut form = String::from(
        "(with-temp-buffer\n(modify-syntax-entry 95 \"w\")\n\
         (let ((case-fold-search nil))\n(dolist (c (list\n",
    );
    for (batch_index, case) in cases.iter().enumerate() {
        form.push_str(&format!(
            "  (list {batch_index} {} {} {})\n",
            if case.expected { "t" } else { "nil" },
            elisp_string(&case.pattern),
            elisp_string(case.input),
        ));
    }
    form.push_str(
        "))\n  (let ((id (nth 0 c)) (expected (nth 1 c))\n\
                (pattern (nth 2 c)) (input (nth 3 c)))\n\
            (set-match-data nil)\n\
            (condition-case err\n\
                (let* ((match (string-match pattern input))\n\
                       (correct (eq expected (not (null match)))))\n\
                  (princ (format \"%d|%S|%S|%S\\n\"\n\
                                 id correct match (and match (match-data)))))\n\
              (error (princ (format \"%d|E|%S\\n\" id (car err)))))))))",
    );
    form
}

fn batch_result_lines(output: &str) -> Vec<&str> {
    let mut lines: Vec<_> = output.trim_end().split('\n').collect();
    assert_eq!(
        lines.pop(),
        Some("OK nil"),
        "oracle harness should append the batch form's nil result"
    );
    lines
}

#[test]
fn dumb_jump_corpus_metadata_is_complete() {
    let positive_cases: usize = UPSTREAM_RULES.iter().map(|rule| rule.positive.len()).sum();
    let negative_cases: usize = UPSTREAM_RULES.iter().map(|rule| rule.negative.len()).sum();
    let untranslatable_cases: usize = UPSTREAM_RULES
        .iter()
        .filter(|rule| is_untranslatable_rule(rule.index))
        .map(|rule| rule.positive.len() + rule.negative.len())
        .sum();
    let audit = format!(
        "commit={UPSTREAM_COMMIT}\nrules={}\npositive-cases={positive_cases}\n\
         negative-cases={negative_cases}\nuntranslatable-rules={}\n\
         untranslatable-cases={untranslatable_cases}",
        UPSTREAM_RULES.len(),
        UNTRANSLATABLE_RULES.len()
    );
    expect_test::expect![[r##"
        commit=cf06b4ccdce6a39346c32f05139f9ee8b77ee229
        rules=296
        positive-cases=852
        negative-cases=424
        untranslatable-rules=3
        untranslatable-cases=24"##]]
    .assert_eq(&audit);
    for (index, _) in UNTRANSLATABLE_RULES {
        let rule = UPSTREAM_RULES
            .iter()
            .find(|rule| rule.index == *index)
            .unwrap_or_else(|| panic!("excluded dumb-jump rule {index} is absent"));
        assert!(
            translate_pattern(rule.pattern).is_err(),
            "excluded dumb-jump rule {index} unexpectedly became translatable"
        );
    }
}

#[test]
fn dumb_jump_real_world_regex_corpus_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let cases = cases();
    assert!(cases.len() > 1_200, "unexpectedly small corpus");
    let mut failure_count = 0usize;
    let mut failures = Vec::new();

    for (batch_number, batch) in cases.chunks(160).enumerate() {
        let form = build_batch(batch);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        let oracle_lines = batch_result_lines(&oracle);
        let neovm_lines = batch_result_lines(&neovm);
        assert_eq!(
            oracle_lines.len(),
            batch.len(),
            "GNU output line count for dumb-jump batch {batch_number}"
        );
        assert_eq!(
            neovm_lines.len(),
            batch.len(),
            "Neomacs output line count for dumb-jump batch {batch_number}"
        );

        for (case_index, ((oracle_line, neovm_line), case)) in oracle_lines
            .iter()
            .zip(neovm_lines.iter())
            .zip(batch.iter())
            .enumerate()
        {
            let parity_ok = neovm_line == oracle_line;
            let expectation_ok = oracle_line.starts_with(&format!("{case_index}|t|"));
            if !parity_ok || !expectation_ok {
                failure_count += 1;
                if failures.len() < 100 {
                    failures.push(format!(
                        "batch={batch_number} case={case_index} rule={} language={} \
                         expected={} parity_ok={parity_ok} pattern={:?} input={:?} \
                         GNU={oracle_line} Neomacs={neovm_line}",
                        case.rule_index, case.language, case.expected, case.pattern, case.input
                    ));
                }
            }
        }
    }
    assert_eq!(
        failure_count,
        0,
        "dumb-jump corpus had {failure_count} failures:\n{}",
        failures.join("\n")
    );
    expect_test::expect![[r##"
        commit=cf06b4ccdce6a39346c32f05139f9ee8b77ee229
        translated-rules=293
        verified-cases=1252
        parity-failures=0
        expectation-failures=0"##]]
    .assert_eq(&format!(
        "commit={UPSTREAM_COMMIT}\ntranslated-rules={}\nverified-cases={}\n\
         parity-failures=0\nexpectation-failures=0",
        UPSTREAM_RULES.len() - UNTRANSLATABLE_RULES.len(),
        cases.len()
    ));
}
