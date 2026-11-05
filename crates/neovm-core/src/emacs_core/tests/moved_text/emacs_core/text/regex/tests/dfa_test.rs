//! Tests of the existence DFA (P3.3): the NFA over the rewind view (C2).

use neovm_text::emacs_core::regex::*;

use neovm_text::emacs_core::regex_emacs::{
    DefaultSyntaxLookup, fail_stack_push_sites, re_match, regex_compile,
};

use neovm_text::emacs_core::regex_emacs::{
    CaseTranslation, LookupClassKey, SyntaxCacheKey, match_anychar_at, match_categoryspec_at,
    match_charset_at, match_exactn_char_at, match_syntaxspec_at, match_syntaxspecset_at,
    regex_compile_lisp_with_translation,
};

use neovm_text::emacs_core::syntax::SyntaxClass;

use neovm_value::value::Value;

use neovm_text::emacs_core::regex_emacs::{
    MatchRegisters, MatchScratch, re_match_internal, take_fail_stack_probe, take_matcher_overflow,
    take_pike_fallback,
};

use neovm_text::emacs_core::regex_emacs::{matcher_entry_count, re_search};

use crate::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

/// Lisp-level searches with the filter on across syntax-table edits,
/// `with-syntax-table`, a new char-table and a buffer switch answer exactly
/// as with it off.
#[test]
fn lisp_searches_follow_syntax_table_changes_with_the_filter_on() {
    // Primitives only: `Context::new()` loads no Lisp.
    let program = r#"
(let ((out nil) (round 0) (st (copy-syntax-table)))
  (set-buffer (get-buffer-create "dfa-a"))
  (insert (apply 'concat (make-list 30 "ab-x cd_x ef x-y gh-x ")))
  (set-syntax-table st)
  (while (< round 4)
    (goto-char (point-min))
    (let ((hits nil))
      (while (re-search-forward "\\w+-x\\|\\_<\\w+_x\\_>" nil t)
        (setq hits (cons (match-beginning 0) hits)))
      (setq out (cons (list round (length hits) (car hits)) out)))
    (cond ((= round 0) (modify-syntax-entry ?- "w" st))
          ((= round 1) (modify-syntax-entry ?_ "." st))
          ((= round 2) (make-char-table 'syntax-table)))
    (setq round (1+ round)))
  (set-syntax-table (standard-syntax-table))
  (goto-char (point-min))
  (setq out (cons (list 'standard (re-search-forward "\\w+-x" nil t)) out))
  (set-buffer (get-buffer-create "dfa-b"))
  (insert "zz-x " (make-string 50 ?q) " qq-x")
  (goto-char (point-min))
  (let ((hits nil))
    (while (re-search-forward "\\w+-x" nil t) (setq hits (cons (point) hits)))
    (setq out (cons (list 'other-buffer hits) out)))
  (nreverse out))
"#;
    let run = |mode| {
        with_dfa_mode(mode, || {
            let mut ev = Context::new();
            let value = ev.eval_str(program).expect("program evaluates");
            crate::emacs_core::print::print_value(&value)
        })
    };
    reset_dfa_stats();
    let off = run(DfaMode::Off);
    let on = run(DfaMode::On);
    let verify = run(DfaMode::Verify);
    assert_eq!(on, off);
    assert_eq!(verify, off);
    let stats = dfa_stats();
    assert!(stats.builds > 0 && stats.skipped > 0, "{stats:?}");
    assert!(stats.context_resets > 0, "{stats:?}");
}

/// Lisp-level searches in a buffer and over a string whose `syntax-table`
/// properties change characters' syntax, with `parse-sexp-lookup-properties`
/// on: the filter takes the lease, classifies characters inside the property
/// runs at their positions, and answers exactly as with it off.
#[test]
fn lisp_searches_over_syntax_table_properties_with_the_filter_on() {
    // Primitives only: `Context::new()` loads no Lisp.
    let program = r#"
(let ((out nil) (parse-sexp-lookup-properties t) (pos 1) (res nil))
  (set-buffer (get-buffer-create "dfa-props"))
  (insert (apply 'concat (make-list 40 "foo-bar (baz) qux_x foo.x é-x ")))
  (while (< pos (point-max))
    (let ((c (char-after pos)))
      (cond ((and (= c ?-) (= 0 (% pos 3)))
             (put-text-property pos (1+ pos) 'syntax-table '(2)))
            ((and (= c ?\() (= 0 (% pos 2)))
             (put-text-property pos (1+ pos) 'syntax-table '(1)))
            ((= c ?.)
             (put-text-property pos (1+ pos) 'syntax-table '(3)))
            ((and (= c ?q) (= 0 (% pos 5)))
             (put-text-property pos (+ pos 3) 'syntax-table '(0)))))
    (setq pos (1+ pos)))
  (setq res '("\\_<foo\\_>" "\\bbar" "\\w+-bar" "\\s(baz" "\\_<foo[.]x\\_>"
              "[[:space:]]ux" "\\sw+x\\b" "\\<x" "é\\w" "[[:word:]]-x"))
  (while res
    (let ((re (car res)) (round 0))
      (while (< round 3)
        (goto-char (point-min))
        (let ((hits nil))
          (while (re-search-forward re nil t)
            (setq hits (cons (match-beginning 0) hits)))
          (setq out (cons (list re round (length hits) hits) out)))
        (setq round (1+ round)))
      (goto-char (point-max))
      (let ((hits nil))
        (while (re-search-backward re nil t) (setq hits (cons (point) hits)))
        (setq out (cons (list re 'back (length hits) hits) out))))
    (setq res (cdr res)))
  (let ((s (apply 'concat (make-list 30 "ab-cd ef.gh "))) (round 0))
    (put-text-property 0 40 'syntax-table '(2) s)
    (put-text-property 100 130 'syntax-table '(3) s)
    (while (< round 3)
      (let ((i 0) (hits nil))
        (while (string-match "\\_<\\w+-cd\\_>\\|\\bef\\.\\w" s i)
          (setq hits (cons (match-beginning 0) hits) i (match-end 0)))
        (setq out (cons (list 'string round hits) out)))
      (setq round (1+ round))))
  (nreverse out))
"#;
    let run = |mode| {
        with_dfa_mode(mode, || {
            let mut ev = Context::new();
            let value = ev.eval_str(program).expect("program evaluates");
            crate::emacs_core::print::print_value(&value)
        })
    };
    let off = run(DfaMode::Off);
    reset_dfa_stats();
    let on = run(DfaMode::On);
    let stats = dfa_stats();
    let verify = run(DfaMode::Verify);
    assert_eq!(on, off);
    assert_eq!(verify, off);
    assert!(
        stats.positional > 0
            && stats.skipped > 0
            && stats.positional_chars > 0
            && stats.plain_runs > 0,
        "{stats:?}"
    );
    assert_eq!(stats.verify_bad_no + stats.verify_bad_yes, 0, "{stats:?}");
}

fn search(
    compiled: &CompiledPattern,
    text: &[u8],
    start: usize,
    range: isize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> (SearchResult, bool) {
    let _ = take_matcher_overflow();
    let found = re_search(compiled, text, start, range, syntax, point)
        .map(|(at, regs)| (at, regs.start.to_vec(), regs.end.to_vec()));
    (found, take_matcher_overflow())
}

/// The syntax a `syntax-table` property run gives its characters.
#[derive(Clone, Copy)]
enum RunSyntax {
    /// A descriptor cons: every character has this class.
    Descriptor(SyntaxClass),
    /// A syntax table as the property: each character's class in it.
    Table(&'static dyn SyntaxLookup),
}

/// A base table plus `syntax-table` property runs (sorted, disjoint
/// `[start, end)` input ranges), and a lazy-propertize frontier that
/// records the lowest syntax read at or past it, as the buffer lookup does.
struct PropertyRunLookup {
    base: &'static dyn SyntaxLookup,
    runs: Vec<(usize, usize, RunSyntax)>,
    /// Whether `plain_syntax_until` reports the runs; otherwise the trait's
    /// default treats every position as propertized.
    reports_runs: bool,
    frontier: usize,
    crossed: std::cell::Cell<Option<usize>>,
}

impl PropertyRunLookup {
    fn new(base: &'static dyn SyntaxLookup, runs: Vec<(usize, usize, RunSyntax)>) -> Self {
        Self {
            base,
            runs,
            reports_runs: true,
            frontier: usize::MAX,
            crossed: std::cell::Cell::new(None),
        }
    }

    fn run_at(&self, pos: usize) -> Option<RunSyntax> {
        self.runs
            .iter()
            .find(|&&(start, end, _)| start <= pos && pos < end)
            .map(|&(_, _, syntax)| syntax)
    }
}

impl SyntaxLookup for PropertyRunLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        self.base.char_syntax(c)
    }

    fn char_syntax_at(&self, c: char, pos: usize) -> SyntaxClass {
        if pos >= self.frontier && self.crossed.get().is_none_or(|seen| pos < seen) {
            self.crossed.set(Some(pos));
        }
        match self.run_at(pos) {
            Some(RunSyntax::Descriptor(class)) => class,
            Some(RunSyntax::Table(table)) => table.char_syntax(c),
            None => self.base.char_syntax(c),
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        self.base.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        self.base.word_boundary_between(c1, c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        self.base.cache_key()
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        self.base.class_cache_key()
    }

    fn position_dependent(&self) -> bool {
        true
    }

    fn plain_syntax_until(&self, pos: usize) -> usize {
        if !self.reports_runs || self.run_at(pos).is_some() {
            return pos;
        }
        self.runs
            .iter()
            .map(|&(start, _, _)| start)
            .filter(|&start| start > pos)
            .min()
            .unwrap_or(usize::MAX)
    }

    fn syntax_read_limit(&self) -> usize {
        self.frontier
    }
}

fn random_base(rng: &mut DfaRng) -> &'static dyn SyntaxLookup {
    if rng.below(2) == 0 {
        &DefaultSyntaxLookup
    } else {
        &CustomTableLookup
    }
}

/// The same unibyte byte has two GNU syntax identities: symbol opcodes
/// inspect the raw current byte, while every previous/word read promotes it
/// to byte8 (regex-emacs.c:55-56,67-95,5111,5163).
struct RawCurrentSymbolLookup;

impl SyntaxLookup for RawCurrentSymbolLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        if c == '\u{a9}' {
            SyntaxClass::Punctuation
        } else {
            DefaultSyntaxLookup.char_syntax(c)
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::External {
            id: usize::MAX - 1,
            epoch: 0,
        }
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Tables {
            syntax: usize::MAX - 1,
            category: 0,
        })
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

/// A syntax table unlike the standard one: `-` is a word constituent, the
/// newline ends comments, `é` and `中` are symbol constituents.
struct CustomTableLookup;

impl SyntaxLookup for CustomTableLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        match c {
            '-' => SyntaxClass::Word,
            '\n' => SyntaxClass::EndComment,
            'é' | '中' => SyntaxClass::Symbol,
            _ => neovm_text::emacs_core::syntax::standard_syntax_class_for_char(c),
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::External {
            id: usize::MAX,
            epoch: 0,
        }
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Tables {
            syntax: usize::MAX,
            category: 0,
        })
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

struct DfaRng(u64);

impl DfaRng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

impl SyntaxLookup for ScriptBoundaryLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        DefaultSyntaxLookup.char_syntax(c)
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        if c1 as u32 <= 0xFF && c2 as u32 <= 0xFF {
            return false;
        }
        let cjk = |c: char| ('\u{3000}'..='\u{9FFF}').contains(&c);
        cjk(c1) != cjk(c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Standard
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Tables {
            syntax: 1,
            category: 1,
        })
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

type SearchResult = Option<(usize, Vec<i64>, Vec<i64>)>;

/// A syntax lookup whose `WORD_BOUNDARY_P` separates CJK from other word
/// constituents, as GNU's char-script-table does.
struct ScriptBoundaryLookup;
