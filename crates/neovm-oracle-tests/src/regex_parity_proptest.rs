//! Batched differential regex GNU-parity proptest.
//!
//! Generates random emacs-subset regex patterns × inputs from a seeded LCG,
//! packs a whole batch into ONE elisp form, evaluates it in BOTH the neomacs
//! binary and GNU Emacs (via the oracle harness), and diffs the per-case
//! `(string-match … + match-data)` results.  Any mismatch is a GNU-parity
//! divergence and fails with the reproducing `(pattern, input, case-fold)`.
//!
//! Batching (K cases per Emacs spawn) is what makes ≥50k-case sweeps tractable:
//! spawning GNU once per case would be far too slow.
//!
//! # Deliberately-excluded feature classes
//!
//! The generator stays inside the subset that is byte-exact with GNU so this
//! test is a *stable* regression guard, not a flaky bug-finder.  Three classes
//! are known, documented parity gaps and are intentionally NOT generated here
//! (they are covered — and their status recorded — by dedicated `div_ar_*`
//! notes and the audit report):
//!
//!   1. **Non-greedy `*?` / `+?` over a nullable group** — neomacs's backtracker
//!      can hit its fail-stack limit ("Stack overflow in regexp matcher") where
//!      GNU terminates with `nil`.  So lazy/greedy unbounded repetition is only
//!      applied to single-char atoms here, never to groups (groups take only
//!      bounded quantifiers `? ?? {n} {n,m}`).
//!   2. **A quantifier applied to the `\\`` begbuf anchor** (`\\`?`, `\\`*`,
//!      `\\`\{n\}`) — GNU's begbuf search-anchoring makes the whole match fail;
//!      neomacs matches empty.  Anchors are never quantified here.
//!   3. **Cross-script word boundaries** (`\\b \\B \\< \\> \\_< \\_>` between two
//!      word chars of *different* `char-script-table` scripts) — neomacs lacks
//!      GNU's `word_boundary_p` category/script check.  Word-boundary/edge ops
//!      are generated but only with ASCII inputs, so this never triggers;
//!      multibyte coverage comes from the non-word-boundary patterns.
//!
//! The two bugs this audit FIXED — non-greedy `??` behaving greedily, and
//! `\\b`/`\\B` at string edges — are exercised in-subset and guarded here.

use crate::common::{eval_oracle_and_neovm, oracle_prop_enabled};

/// Numerical-Recipes LCG — fully deterministic so any failure reproduces from
/// its seed.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
    fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den) < num
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u32) as usize]
    }
    /// Split off an independent stream so each case is reproducible on its own.
    fn fork(&mut self) -> Lcg {
        Lcg(self
            .next_u32()
            .wrapping_mul(2654435761)
            .wrapping_add(0x1234_5678) as u64
            ^ ((self.next_u32() as u64) << 32))
    }
}

const ASCII_LIT: &[char] = &['a', 'b', 'A', 'B', '0', '1', ' ', '_', '-', '.'];
const MULTIBYTE: &[char] = &['α', 'Σ', 'σ', 'é', 'ñ', '中', '文', '\u{0301}'];
const POSIX: &[&str] = &[
    "alpha", "alnum", "digit", "space", "upper", "lower", "punct", "word", "blank", "cntrl",
    "graph", "print", "xdigit", "ascii", "nonascii",
];
const SYNTAX_CODES: &[char] = &['-', '.', 'w', '_', '(', ')', '\'', '"', '$', '<', '>', ' '];
const CATEGORY_CODES: &[char] = &['a', 'A', 'c', 'l', 'g', 'h'];
const BOUNDARIES: &[&str] = &["\\b", "\\B", "\\<", "\\>", "\\_<", "\\_>"];
const ANCHORS: &[&str] = &["^", "$", "\\`", "\\'", "\\="];
/// Quantifiers legal on a single-char atom (includes unbounded + non-greedy).
const ATOM_QUANT: &[&str] = &[
    "",
    "*",
    "+",
    "?",
    "*?",
    "+?",
    "??",
    "\\{2\\}",
    "\\{1,\\}",
    "\\{0,2\\}",
    "\\{1,2\\}",
];
/// Quantifiers legal on a GROUP — bounded only, to avoid the documented
/// non-greedy-star-over-nullable-group stack-overflow class.
const GROUP_QUANT: &[&str] = &["", "?", "??", "\\{2\\}", "\\{0,2\\}", "\\{1,2\\}"];

struct Gen {
    ascii_only: bool,
    groups: u32,
}

impl Gen {
    fn char_class(&self, r: &mut Lcg) -> String {
        let mut s = String::from("[");
        if r.chance(35, 100) {
            s.push('^');
        }
        if r.chance(15, 100) {
            s.push(']'); // ] as first member is a literal
        }
        let n = 1 + r.below(3);
        for _ in 0..n {
            match r.below(10) {
                0..=2 => {
                    if !self.ascii_only && r.chance(3, 10) {
                        s.push(r.pick(MULTIBYTE));
                    } else {
                        s.push(r.pick(ASCII_LIT));
                    }
                }
                3..=4 => {
                    s.push(r.pick(&['a', 'c', 'x', '0', 'A']));
                    s.push('-');
                    s.push(r.pick(&['e', 'g', 'z', '9', 'Z']));
                }
                5..=6 => {
                    s.push_str("[:");
                    let posix: &str = r.pick(POSIX);
                    s.push_str(posix);
                    s.push_str(":]");
                }
                7 => s.push('-'),
                8 => s.push('^'),
                _ => s.push(r.pick(&['+', '*', '?', '.', '$'])),
            }
        }
        s.push(']');
        s
    }

    /// A single atom plus (for real atoms) an optional quantifier.
    fn piece(&mut self, r: &mut Lcg, depth: u32) -> String {
        let k = r.below(100);
        // Anchors / boundaries: never quantified (documented gap #2).
        if k < 8 {
            return String::from(r.pick(ANCHORS));
        }
        if k < 14 {
            return String::from(r.pick(BOUNDARIES));
        }
        // Group: bounded quantifier only (documented gap #1).
        if k < 24 && depth < 3 {
            let inner = self.regex(r, depth + 1, false);
            let atom = match r.below(10) {
                0..=5 => {
                    self.groups += 1;
                    format!("\\({inner}\\)")
                }
                6..=7 => format!("\\(?:{inner}\\)"),
                _ => {
                    self.groups += 1;
                    format!("\\(?{}:{inner}\\)", self.groups)
                }
            };
            return format!("{atom}{}", r.pick(GROUP_QUANT));
        }
        // Backref.
        if k < 30 && self.groups > 0 {
            let refn = 1 + r.below(self.groups.min(9));
            return format!("\\{refn}");
        }
        // Single-char / class atoms: any quantifier allowed.
        let atom = match r.below(100) {
            0..=39 => {
                if !self.ascii_only && r.chance(25, 100) {
                    r.pick(MULTIBYTE).to_string()
                } else {
                    r.pick(ASCII_LIT).to_string()
                }
            }
            40..=49 => ".".to_string(),
            50..=61 => self.char_class(r),
            62..=67 => r.pick(&["\\w", "\\W"]).to_string(),
            68..=73 => format!("\\s{}", r.pick(SYNTAX_CODES)),
            74..=79 => format!("\\S{}", r.pick(SYNTAX_CODES)),
            80..=85 => format!("\\c{}", r.pick(CATEGORY_CODES)),
            86..=91 => format!("\\C{}", r.pick(CATEGORY_CODES)),
            _ => r.pick(ASCII_LIT).to_string(),
        };
        format!("{atom}{}", r.pick(ATOM_QUANT))
    }

    fn concat(&mut self, r: &mut Lcg, depth: u32) -> String {
        let n = 1 + r.below(4);
        (0..n).map(|_| self.piece(r, depth)).collect()
    }

    fn regex(&mut self, r: &mut Lcg, depth: u32, _top: bool) -> String {
        let branches = 1 + r.below(if depth < 2 { 3 } else { 1 });
        (0..branches)
            .map(|_| self.concat(r, depth))
            .collect::<Vec<_>>()
            .join("\\|")
    }
}

fn gen_case(r: &mut Lcg) -> (bool, String, String) {
    // Word-boundary ops are parity-solid only on ASCII (documented gap #3), so
    // pin inputs to ASCII whenever the pattern may contain a boundary op.
    // Simplest robust split: half the cases are ASCII-only (with full boundary
    // coverage), half allow multibyte (still safe because boundary ops on
    // multibyte word runs are the only gap, and those need same-run word chars
    // — rare — but to be certain we drop boundary ops from multibyte patterns).
    let ascii_only = r.chance(1, 2);
    let mut g = Gen {
        ascii_only,
        groups: 0,
    };
    let pat = g.regex(r, 0, true);
    let input = gen_input(r, ascii_only);
    let cf = r.chance(1, 2);
    // If a multibyte pattern slipped in a boundary op, force ASCII input to stay
    // clear of the cross-script word_boundary_p gap.
    let input = if !ascii_only && BOUNDARIES.iter().any(|b| pat.contains(*b)) {
        gen_input(r, true)
    } else {
        input
    };
    (cf, pat, input)
}

fn gen_input(r: &mut Lcg, ascii_only: bool) -> String {
    if r.chance(8, 100) {
        return String::new();
    }
    let n = 1 + r.below(12);
    let mut s = String::new();
    for _ in 0..n {
        match r.below(100) {
            0..=54 => s.push(r.pick(&['a', 'b', 'A', 'B', '0', '1', ' ', 'a', 'a'])),
            55..=69 => s.push('a'),
            70..=84 => {
                if ascii_only {
                    s.push(r.pick(ASCII_LIT));
                } else {
                    s.push(r.pick(MULTIBYTE));
                }
            }
            85..=91 => s.push(r.pick(&['\t', '\n', '.', ',', ';', '(', ')', '[', ']', '-'])),
            _ => s.push(r.pick(&['<', '>', '_', '@', '#', '$', '%', '|', '/', '\\'])),
        }
    }
    s
}

/// Emit `s` as an escaping-free elisp `(string c1 c2 …)` form.
fn elisp_string(s: &str) -> String {
    if s.is_empty() {
        return String::from("\"\"");
    }
    let mut out = String::from("(string");
    for ch in s.chars() {
        out.push(' ');
        out.push_str(&(ch as u32).to_string());
    }
    out.push(')');
    out
}

/// Build one elisp form that runs every case and prints one result line each.
/// `set-match-data nil` per case prevents a failed `string-match` from leaking
/// the prior case's match-data.
fn build_batch(cases: &[(bool, String, String)]) -> String {
    let mut form = String::from("(progn\n(dolist (c (list\n");
    for (cf, pat, inp) in cases {
        form.push_str(&format!(
            "  (list {} {} {})\n",
            if *cf { "t" } else { "nil" },
            elisp_string(pat),
            elisp_string(inp)
        ));
    }
    form.push_str(
        "  ))\n  (let ((case-fold-search (nth 0 c)) (pat (nth 1 c)) (inp (nth 2 c)))\n\
         \x20   (with-temp-buffer\n\
         \x20     (set-match-data nil)\n\
         \x20     (condition-case e\n\
         \x20         (let ((m (string-match pat inp)))\n\
         \x20           (princ (format \"%S|%S\\n\" m (and m (match-data)))))\n\
         \x20       (error (princ (format \"E|%S\\n\" (car e)))))))))",
    );
    form
}

/// Run `batches` batches of `k` cases each from `seed`; panic on the first
/// divergence with a reproducer.  Returns the number of cases compared.
fn run_sweep(seed: u64, batches: u64, k: usize) -> usize {
    let mut compared = 0usize;
    for b in 0..batches {
        let mut r = Lcg::new(seed.wrapping_mul(1_000_003).wrapping_add(b));
        let cases: Vec<(bool, String, String)> = (0..k)
            .map(|_| {
                let mut cr = r.fork();
                gen_case(&mut cr)
            })
            .collect();
        let form = build_batch(&cases);
        let (gnu, neo) = eval_oracle_and_neovm(&form);
        let neo_lines: Vec<&str> = neo.trim_end().split('\n').collect();
        let gnu_lines: Vec<&str> = gnu.trim_end().split('\n').collect();
        assert_eq!(
            neo_lines.len(),
            gnu_lines.len(),
            "batch {b}: line-count mismatch (neomacs {} vs GNU {})",
            neo_lines.len(),
            gnu_lines.len()
        );
        for (i, (nl, gl)) in neo_lines.iter().zip(gnu_lines.iter()).enumerate() {
            compared += 1;
            if nl != gl {
                let (cf, pat, inp) = &cases[i];
                panic!(
                    "regex GNU-parity divergence (seed={seed} batch={b} case={i}):\n  \
                     case-fold = {cf}\n  pattern   = {pat:?}\n  input     = {inp:?}\n  \
                     neomacs   = {nl}\n  GNU       = {gl}"
                );
            }
        }
    }
    compared
}

/// Small, fast smoke variant that runs in the normal suite.  In the default
/// snapshot mode this compares neomacs against itself (a structural/round-trip
/// check); with `NEOVM_FORCE_ORACLE_PATH` set it is a real GNU differential.
#[test]
fn regex_parity_proptest_smoke() {
    if !oracle_prop_enabled() {
        return;
    }
    let n = run_sweep(0xA11CE, 3, 60);
    assert!(n >= 150, "smoke sweep compared too few cases: {n}");
}

/// Full ≥50k-case sweep. In snapshot mode this is a structural self-check. All
/// non-snapshot modes compare every batch with GNU Emacs found on `PATH` or
/// selected via `NEOVM_FORCE_ORACLE_PATH`.
#[test]
fn regex_parity_proptest_full() {
    if !oracle_prop_enabled() {
        return;
    }
    // 4 independent seed streams × 45 batches × 300 cases ≈ 54k cases.
    let mut total = 0;
    for seed in [0x1111_2222u64, 0x3333_4444, 0x5555_6666, 0x7777_8888] {
        total += run_sweep(seed, 45, 300);
    }
    assert!(
        total >= 50_000,
        "full sweep compared too few cases: {total}"
    );
}
