//! An anchored EXISTENCE lazy DFA in front of the backtracker (P3.3).
//!
//! The DFA answers one question per search candidate `p`: can any match start
//! at `p` and end by `stop`?  Existence is a property of the pattern's regular
//! language alone -- GNU's leftmost-first priority, greedy vs non-greedy,
//! POSIX-longest and the submatch registers never enter it -- so a DFA over
//! the same bytecode, stepping the same per-character tests, answers it
//! exactly.  A "no" lets `re_search` skip the candidate without entering the
//! backtracker; a "yes" or "unknown" runs the unchanged backtracker, which
//! produces GNU's registers, overflow, quit and Pike behaviour.
//!
//! Layers (one commit each):
//! * the NFA ([`Nfa`]): the consuming positions of the rewind view
//!   ([`RewindView`]), the per-character test each runs, and the epsilon
//!   closure, whose zero-width assertions are evaluated at the real text
//!   position by the matcher's own tests;
//! * character classes: the partition of characters by the vector of those
//!   tests' results;
//! * the lazy DFA: states are (NFA kernel, facts about the previous
//!   character), transitions are cached by (state, class).
//!
//! * the candidate filter ([`DfaLease`]): `re_search` asks the DFA at each
//!   candidate; a rejection skips the matcher unless the fail-stack bound
//!   says the backtracker could have overflowed there;
//! * `syntax-table` property runs and the lazy `syntax-propertize` frontier
//!   ([`ExistenceDfa::begin_search`]): where properties may apply, the DFA
//!   reads every syntax through the matcher's own lookup at the real
//!   position, steps its cached loop only through property-free stretches
//!   ([`SyntaxLookup::plain_syntax_until`]), classifies a character inside a
//!   run at its position ([`CharClasses::class_at_position`]), and leaves a
//!   candidate that reaches the frontier to the matcher, which records the
//!   read.
//!
//! No Lisp `Value` is stored here: only ids, byte tables and identity bits.
//!
//! # Knobs (read once per process)
//!
//! | Knob | Values (default) | Effect |
//! |------|------------------|--------|
//! | `NEOVM_REGEX_DFA` | `on` (default), `off`, `verify` | Candidate existence filter ([`DfaMode`]). |
//! | `NEOVM_REGEX_DFA_COLD` | `on` (default), `off` | Defer the lease; admit two nonempty failures during a scan, or one when the whole search fails ([`cold_path_enabled`]). |
//! | `NEOVM_REGEX_DFA_STATS` | unset (default), `1` | Print this thread's [`DfaStats`] on stderr at exit with the filter on. |
//! | `NEOVM_REGEX_DFA_FIRST_STEP` | `on` (default), `off` | Reject cached prefixes of at most eight bytes inline; verify also checks the predicate against the matcher ([`first_step_enabled`]). |

use super::{
    CompiledPattern, LookupClassKey, MatchRegisters, MatchScratch, RegexOp, SyntaxAssertion,
    SyntaxCacheKey, SyntaxLookup, evaluate_syntax_assertion, extract_number, extract_number_u16,
    fail_stack_overflow_free_span, match_anychar_at, match_categoryspec_at, match_charset_at,
    match_exactn_char_at, match_syntaxspec_at, match_syntaxspecset_at, matcher_overflow_pending,
    opcode_len, posix_class_bits_read_syntax, re_match_candidate_in, re_text_char,
    regex_syntax_char,
};
use crate::emacs_core::emacs_char;
use crate::emacs_core::syntax::SyntaxClass;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::{RefCell, RefMut};

// ---------------------------------------------------------------------------
// The NFA over the rewind view (C2)
// ---------------------------------------------------------------------------

/// Why a pattern has no existence DFA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DfaIneligible {
    /// `\N`: not a regular language.
    Backreference,
    /// `\{n,m\}`: interval counters (not modelled in v1).
    IntervalCounter,
    /// A non-greedy loop over a nullable body: its cycle check reads GNU's
    /// per-iteration marker frames.
    NullableNonGreedyLoop,
    /// The bytecode was not sealed (a hand-assembled buffer).
    Unsealed,
    /// A keep-string jump of an unexpected shape: no rewind view.
    NoRewindView,
    /// More consuming positions than a DFA state can index.
    TooManyPositions,
    /// The start closure reaches the end of the pattern with no test on the
    /// way, so every candidate matches and the filter could never reject.
    MatchesEmptyEverywhere,
    /// An opcode the walk does not know (a malformed buffer).
    MalformedBytecode,
}

/// Most consuming positions an NFA may have (position ids are `u16`).
const MAX_POSITIONS: usize = 4096;

/// A kernel item: where a thread resumes after consuming a character, packed
/// as `pc << 8 | lit_off`.  `lit_off == 0` is "run the closure from `pc`";
/// `lit_off > 0` is the `exactn` literal at `pc`, part way through.
pub(crate) type KernelItem = u32;

#[inline]
pub(crate) const fn kernel_item(pc: usize, lit_off: usize) -> KernelItem {
    ((pc as u32) << 8) | lit_off as u32
}

#[inline]
const fn item_pc(item: KernelItem) -> usize {
    (item >> 8) as usize
}

#[inline]
const fn item_lit_off(item: KernelItem) -> usize {
    (item & 0xFF) as usize
}

/// The per-character test a consuming position runs: one of the matcher's
/// shared `match_*_at` helpers with its operands.  Positions with identical
/// operands share one predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Predicate {
    /// One pattern character of the `exactn` literal at `pc`, `lit_off`
    /// bytes in (`match_exactn_char_at`).
    Literal { pc: u32, lit_off: u8 },
    /// `.` (`match_anychar_at`).
    AnyChar,
    /// `[...]` / `[^...]` at `pc` (`match_charset_at`: bitmap, range table
    /// and class bits, the last two in side tables keyed by `pc`).
    Charset { pc: u32 },
    /// `\sC` / `\SC` (`match_syntaxspec_at`).
    Syntax { class: u8, negate: bool },
    /// The fused `\sC\|\sD` set (`match_syntaxspecset_at`).
    SyntaxSet { mask: u16 },
    /// `\cC` / `\CC` (`match_categoryspec_at`).
    Category { category: u8, negate: bool },
}

/// One consuming position of the NFA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Position {
    /// The consuming opcode.
    pub(crate) pc: u32,
    /// Byte offset of the pattern character within an `exactn` literal.
    pub(crate) lit_off: u8,
    /// Index into [`Nfa::predicates`].
    pub(crate) predicate: u16,
    /// Where the thread resumes after the character is consumed.
    pub(crate) next: KernelItem,
}

/// The zero-width tests a pattern contains, which decide the facts a
/// character class must carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Assertions(u8);

impl Assertions {
    pub(crate) const BEG_LINE: Self = Self(1 << 0);
    pub(crate) const END_LINE: Self = Self(1 << 1);
    pub(crate) const BEG_BUF: Self = Self(1 << 2);
    pub(crate) const END_BUF: Self = Self(1 << 3);
    pub(crate) const AT_DOT: Self = Self(1 << 4);
    /// `\b \B \< \>`: word syntax on both sides, and GNU's
    /// `WORD_BOUNDARY_P` for two word constituents.
    pub(crate) const WORD: Self = Self(1 << 5);
    /// `\_< \_>`: word-or-symbol syntax on both sides.
    pub(crate) const SYMBOL: Self = Self(1 << 6);

    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[inline]
    pub(crate) const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl std::ops::BitOr for Assertions {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOrAssign for Assertions {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// The NFA of one pattern: its consuming positions and their tests, walked
/// over the rewind view.
#[derive(Debug)]
pub(crate) struct Nfa {
    /// The rewind view (keep-string loops rewritten; same length and opcode
    /// positions as `CompiledPattern::buffer`, whose side tables it shares).
    pub(crate) bytecode: Box<[u8]>,
    pub(crate) positions: Vec<Position>,
    pub(crate) predicates: Vec<Predicate>,
    /// `kernel_item(pc, lit_off)` of each position, to its index.
    position_of: FxHashMap<KernelItem, u16>,
    pub(crate) assertions: Assertions,
    /// Opcodes that push onto the backtracker's fail stack (the overflow
    /// bound, [`super::fail_stack_may_overflow`]).
    pub(crate) push_sites: usize,
    /// Whether the pattern tests categories (`\cC`), which the class tables
    /// then depend on.
    pub(crate) uses_categories: bool,
    /// Whether some predicate or assertion reads the syntax table: then a
    /// `syntax-table` property can change a character's class or an
    /// assertion's answer, and the DFA reads syntax where the matcher does.
    pub(crate) reads_syntax: bool,
}

/// Decide whether `pattern` can have an existence DFA.
pub(crate) fn dfa_eligibility(pattern: &CompiledPattern) -> Result<(), DfaIneligible> {
    if !pattern.buffer_sealed {
        return Err(DfaIneligible::Unsealed);
    }
    let bytecode = &pattern.buffer;
    let mut pc = 0usize;
    while pc < bytecode.len() {
        match RegexOp::from_byte(bytecode[pc]) {
            Some(RegexOp::Duplicate) => return Err(DfaIneligible::Backreference),
            Some(RegexOp::SucceedN | RegexOp::JumpN | RegexOp::SetNumberAt) => {
                return Err(DfaIneligible::IntervalCounter);
            }
            Some(RegexOp::OnFailureJumpNastyloop) => {
                return Err(DfaIneligible::NullableNonGreedyLoop);
            }
            Some(_) => {}
            None => return Err(DfaIneligible::MalformedBytecode),
        }
        pc += opcode_len(bytecode, pc).ok_or(DfaIneligible::MalformedBytecode)?;
    }
    if pattern.rewind_bytecode().is_none() {
        return Err(DfaIneligible::NoRewindView);
    }
    Ok(())
}

impl Nfa {
    /// Build the NFA of an eligible pattern (see [`dfa_eligibility`]).
    pub(crate) fn build(pattern: &CompiledPattern) -> Result<Self, DfaIneligible> {
        dfa_eligibility(pattern)?;
        let code = pattern
            .rewind_bytecode()
            .ok_or(DfaIneligible::NoRewindView)?;
        let bytecode: Box<[u8]> = code.into();
        let mut nfa = Nfa {
            positions: Vec::new(),
            predicates: Vec::new(),
            position_of: FxHashMap::default(),
            assertions: Assertions::empty(),
            push_sites: super::fail_stack_push_sites(&pattern.buffer),
            uses_categories: false,
            reads_syntax: false,
            bytecode,
        };
        let mut predicate_of: FxHashMap<Predicate, u16> = FxHashMap::default();
        let mut literal_of: FxHashMap<SmallVec<[u8; 8]>, u16> = FxHashMap::default();
        let mut pc = 0usize;
        while pc < code.len() {
            let op = RegexOp::from_byte(code[pc]).ok_or(DfaIneligible::MalformedBytecode)?;
            let len = opcode_len(code, pc).ok_or(DfaIneligible::MalformedBytecode)?;
            let next = kernel_item(pc + len, 0);
            let mut single = |nfa: &mut Nfa, predicate: Predicate| -> Result<(), DfaIneligible> {
                let id = match predicate_of.get(&predicate) {
                    Some(&id) => id,
                    None => {
                        let id = nfa.predicates.len() as u16;
                        nfa.predicates.push(predicate);
                        predicate_of.insert(predicate, id);
                        id
                    }
                };
                nfa.push_position(pc, 0, id, next)
            };
            match op {
                RegexOp::Exactn => {
                    let count = code[pc + 1] as usize;
                    let literal = &code[pc + 2..pc + 2 + count];
                    let mut offsets: SmallVec<[usize; 16]> = SmallVec::new();
                    let mut off = 0usize;
                    while off < count {
                        offsets.push(off);
                        off += if pattern.multibyte {
                            emacs_char::string_char(&literal[off..]).1.max(1)
                        } else {
                            1
                        };
                    }
                    for (i, &off) in offsets.iter().enumerate() {
                        let end = offsets.get(i + 1).copied().unwrap_or(count);
                        let bytes: SmallVec<[u8; 8]> = SmallVec::from_slice(&literal[off..end]);
                        let id = match literal_of.get(&bytes) {
                            Some(&id) => id,
                            None => {
                                let id = nfa.predicates.len() as u16;
                                nfa.predicates.push(Predicate::Literal {
                                    pc: pc as u32,
                                    lit_off: off as u8,
                                });
                                literal_of.insert(bytes, id);
                                id
                            }
                        };
                        let after = match offsets.get(i + 1) {
                            Some(&next_off) => kernel_item(pc, next_off),
                            None => next,
                        };
                        nfa.push_position(pc, off, id, after)?;
                    }
                }
                RegexOp::AnyChar => single(&mut nfa, Predicate::AnyChar)?,
                RegexOp::Charset | RegexOp::CharsetNot => {
                    if pattern
                        .charset_class_bits
                        .get(&pc)
                        .is_some_and(|&bits| posix_class_bits_read_syntax(bits))
                    {
                        nfa.reads_syntax = true;
                    }
                    single(&mut nfa, Predicate::Charset { pc: pc as u32 })?
                }
                RegexOp::SyntaxSpec | RegexOp::NotSyntaxSpec => {
                    nfa.reads_syntax = true;
                    single(
                        &mut nfa,
                        Predicate::Syntax {
                            class: code[pc + 1],
                            negate: op == RegexOp::NotSyntaxSpec,
                        },
                    )?
                }
                RegexOp::SyntaxSpecSet => {
                    nfa.reads_syntax = true;
                    single(
                        &mut nfa,
                        Predicate::SyntaxSet {
                            mask: extract_number_u16(code, pc + 1),
                        },
                    )?
                }
                RegexOp::CategorySpec | RegexOp::NotCategorySpec => {
                    nfa.uses_categories = true;
                    single(
                        &mut nfa,
                        Predicate::Category {
                            category: code[pc + 1],
                            negate: op == RegexOp::NotCategorySpec,
                        },
                    )?
                }
                RegexOp::BegLine => nfa.assertions |= Assertions::BEG_LINE,
                RegexOp::EndLine => nfa.assertions |= Assertions::END_LINE,
                RegexOp::BegBuf => nfa.assertions |= Assertions::BEG_BUF,
                RegexOp::EndBuf => nfa.assertions |= Assertions::END_BUF,
                RegexOp::AtDot => nfa.assertions |= Assertions::AT_DOT,
                RegexOp::WordBound
                | RegexOp::NotWordBound
                | RegexOp::WordBeg
                | RegexOp::WordEnd => {
                    nfa.reads_syntax = true;
                    nfa.assertions |= Assertions::WORD;
                }
                RegexOp::SymBeg | RegexOp::SymEnd => {
                    nfa.reads_syntax = true;
                    nfa.assertions |= Assertions::SYMBOL;
                }
                _ => {}
            }
            pc += len;
        }
        if nfa.start_matches_unconditionally() {
            return Err(DfaIneligible::MatchesEmptyEverywhere);
        }
        Ok(nfa)
    }

    fn push_position(
        &mut self,
        pc: usize,
        lit_off: usize,
        predicate: u16,
        next: KernelItem,
    ) -> Result<(), DfaIneligible> {
        if self.positions.len() >= MAX_POSITIONS {
            return Err(DfaIneligible::TooManyPositions);
        }
        let id = self.positions.len() as u16;
        self.positions.push(Position {
            pc: pc as u32,
            lit_off: lit_off as u8,
            predicate,
            next,
        });
        self.position_of.insert(kernel_item(pc, lit_off), id);
        Ok(())
    }

    /// The position a kernel item names directly, if it is a consuming one.
    #[inline]
    pub(crate) fn position_at(&self, item: KernelItem) -> Option<u16> {
        self.position_of.get(&item).copied()
    }

    /// Whether the start closure reaches the end of the pattern through
    /// opcodes that test nothing (no assertion on the way).
    fn start_matches_unconditionally(&self) -> bool {
        let mut seen = vec![false; self.bytecode.len() + 1];
        let mut stack = vec![0usize];
        while let Some(pc) = stack.pop() {
            if pc >= self.bytecode.len() {
                return true;
            }
            if std::mem::replace(&mut seen[pc], true) {
                continue;
            }
            match self.epsilon(pc) {
                Epsilon::Accept => return true,
                Epsilon::Next(next) => stack.push(next),
                Epsilon::Split(a, b) => {
                    stack.push(a);
                    stack.push(b);
                }
                Epsilon::Test(..) | Epsilon::Consume => {}
            }
        }
        false
    }

    /// What the opcode at `pc` does in the epsilon closure.
    #[inline]
    fn epsilon(&self, pc: usize) -> Epsilon {
        let bytecode = &*self.bytecode;
        let jump_target =
            |at: usize| (at as i64 + 3 + extract_number(bytecode, at + 1) as i64) as usize;
        match RegexOp::from_byte(bytecode[pc]) {
            Some(RegexOp::NoOp) => Epsilon::Next(pc + 1),
            Some(RegexOp::StartMemory | RegexOp::StopMemory) => Epsilon::Next(pc + 2),
            Some(RegexOp::Jump) => Epsilon::Next(jump_target(pc)),
            Some(
                RegexOp::OnFailureJump
                | RegexOp::OnFailureKeepStringJump
                | RegexOp::OnFailureJumpLoop
                | RegexOp::OnFailureJumpNastyloop
                | RegexOp::OnFailureJumpSmart,
            ) => Epsilon::Split(pc + 3, jump_target(pc)),
            Some(RegexOp::Succeed | RegexOp::PosixEnd) => Epsilon::Accept,
            Some(
                op @ (RegexOp::BegLine
                | RegexOp::EndLine
                | RegexOp::BegBuf
                | RegexOp::EndBuf
                | RegexOp::AtDot
                | RegexOp::WordBound
                | RegexOp::NotWordBound
                | RegexOp::WordBeg
                | RegexOp::WordEnd
                | RegexOp::SymBeg
                | RegexOp::SymEnd),
            ) => Epsilon::Test(op, pc + 1),
            // Consuming opcodes (and, unreachable for an eligible pattern,
            // backreferences and counters) end the closure here.
            _ => Epsilon::Consume,
        }
    }

    /// The epsilon closure of `items` at the text position `at`: the
    /// consuming positions it reaches, appended to `out` (unsorted, no
    /// duplicates), and whether it reaches the end of the pattern (a match
    /// ending at `at`).  Zero-width assertions are the matcher's own tests at
    /// the real position, so the closure is exactly the set of configurations
    /// the backtracker can reach from `items` at `at` without consuming.
    pub(crate) fn closure(
        &self,
        items: &[KernelItem],
        at: &Place<'_>,
        scratch: &mut ClosureScratch,
        out: &mut Vec<u16>,
    ) -> bool {
        scratch.begin(self.bytecode.len() + 1);
        let mut accept = false;
        for &item in items {
            if item_lit_off(item) != 0 {
                // Part way through a literal: the thread waits on the literal's
                // next character, with nothing to close over.
                if let Some(position) = self.position_at(item)
                    && scratch.first_visit_position(position)
                {
                    out.push(position);
                }
                continue;
            }
            scratch.stack.push(item_pc(item));
            while let Some(pc) = scratch.stack.pop() {
                if !scratch.first_visit(pc) {
                    continue;
                }
                if pc >= self.bytecode.len() {
                    accept = true;
                    continue;
                }
                match self.epsilon(pc) {
                    Epsilon::Accept => accept = true,
                    Epsilon::Next(next) => scratch.stack.push(next),
                    Epsilon::Split(a, b) => {
                        scratch.stack.push(a);
                        scratch.stack.push(b);
                    }
                    Epsilon::Test(op, next) => {
                        if at.assertion_holds(op) {
                            scratch.stack.push(next);
                        }
                    }
                    Epsilon::Consume => {
                        if let Some(position) = self.position_at(kernel_item(pc, 0))
                            && scratch.first_visit_position(position)
                        {
                            out.push(position);
                        }
                    }
                }
            }
        }
        accept
    }
}

/// One opcode's role in the epsilon closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Epsilon {
    Next(usize),
    Split(usize, usize),
    /// A zero-width assertion, then `next` when it holds.
    Test(RegexOp, usize),
    Accept,
    Consume,
}

/// A text position the closure evaluates assertions at: the matcher's view
/// (`text`, `d`, `stop`, point, representation, syntax).
pub(crate) struct Place<'a> {
    pub(crate) text: &'a [u8],
    pub(crate) d: usize,
    pub(crate) stop: usize,
    pub(crate) point: usize,
    pub(crate) target_multibyte: bool,
    pub(crate) syntax: &'a dyn SyntaxLookup,
}

impl Place<'_> {
    /// The backtracker's test for `op` at this position (`re_match_loop`'s
    /// arms, verbatim).
    #[inline]
    fn assertion_holds(&self, op: RegexOp) -> bool {
        let (text, d) = (self.text, self.d);
        match op {
            RegexOp::BegLine => d == 0 || (d > 0 && text[d - 1] == b'\n'),
            RegexOp::EndLine => d >= text.len() || text[d] == b'\n',
            RegexOp::BegBuf => d == 0,
            RegexOp::EndBuf => d == text.len(),
            RegexOp::AtDot => d == self.point,
            _ => evaluate_syntax_assertion(
                SyntaxAssertion::from_regex_op(op),
                text,
                d,
                self.stop,
                self.target_multibyte,
                self.syntax,
            ),
        }
    }
}

/// Reusable closure state: a generation-stamped `seen` set over opcode
/// positions and consuming positions, and the DFS stack.
#[derive(Default, Debug)]
pub(crate) struct ClosureScratch {
    seen_pc: Vec<u32>,
    seen_position: Vec<u32>,
    generation: u32,
    stack: Vec<usize>,
}

impl ClosureScratch {
    fn begin(&mut self, pcs: usize) {
        if self.seen_pc.len() < pcs {
            self.seen_pc.resize(pcs, 0);
        }
        if self.seen_position.len() < MAX_POSITIONS {
            self.seen_position.resize(MAX_POSITIONS, 0);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen_pc.fill(0);
            self.seen_position.fill(0);
            self.generation = 1;
        }
        self.stack.clear();
    }

    #[inline]
    fn first_visit(&mut self, pc: usize) -> bool {
        let slot = &mut self.seen_pc[pc];
        let first = *slot != self.generation;
        *slot = self.generation;
        first
    }

    #[inline]
    fn first_visit_position(&mut self, position: u16) -> bool {
        let slot = &mut self.seen_position[position as usize];
        let first = *slot != self.generation;
        *slot = self.generation;
        first
    }
}

// ---------------------------------------------------------------------------
// Character classes (C3)
// ---------------------------------------------------------------------------

/// Facts about one character that the zero-width assertions read, besides
/// the predicates.  Only the facts a pattern's assertions read are kept
/// ([`Nfa::fact_mask`]), so they split no class needlessly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Facts(u8);

impl Facts {
    /// No character: the position is the start of the text (only ever a
    /// PREVIOUS-character fact).
    pub(crate) const EDGE: Self = Self(1 << 0);
    /// The character is a newline (`^` after it, `$` before it).
    pub(crate) const NEWLINE: Self = Self(1 << 1);
    /// Word syntax (`\b \B \< \>`).
    pub(crate) const WORD: Self = Self(1 << 2);
    /// Word or symbol syntax (`\_< \_>`).
    pub(crate) const WORD_OR_SYMBOL: Self = Self(1 << 3);
    /// A word constituent above U+00FF.  Between two word constituents GNU's
    /// `WORD_BOUNDARY_P` consults scripts and categories unless both are at
    /// or below U+00FF (`WordBoundaryLookup::boundary_between`), so such a
    /// pair decides `\b` per character pair, never per class.
    pub(crate) const WIDE_WORD: Self = Self(1 << 4);

    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[inline]
    pub(crate) const fn bits(self) -> u8 {
        self.0
    }

    #[inline]
    const fn masked(self, mask: Self) -> Self {
        Self(self.0 & mask.0)
    }

    /// The facts of the Emacs character `code` at input position `at`, whose
    /// first byte is `first_byte`, under `syntax` (the base table, or the
    /// real lookup inside a `syntax-table` property run).
    fn of_char(
        code: u32,
        first_byte: u8,
        syntax: &dyn SyntaxLookup,
        at: usize,
        mask: Self,
    ) -> Self {
        let mut facts = 0u8;
        if first_byte == b'\n' {
            facts |= Self::NEWLINE.0;
        }
        if mask.0 & (Self::WORD.0 | Self::WORD_OR_SYMBOL.0 | Self::WIDE_WORD.0) != 0 {
            // The matcher's `re_char_and_syntax`: raw bytes read the syntax
            // of their eight-bit character, at their position.
            let ch = regex_syntax_char(code);
            let class = syntax.char_syntax_at(ch, at);
            if class == SyntaxClass::Word {
                facts |= Self::WORD.0 | Self::WORD_OR_SYMBOL.0;
                if ch as u32 > 0xFF {
                    facts |= Self::WIDE_WORD.0;
                }
            } else if class == SyntaxClass::Symbol {
                facts |= Self::WORD_OR_SYMBOL.0;
            }
        }
        Self(facts).masked(mask)
    }
}

impl std::ops::BitOr for Facts {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Nfa {
    /// The character facts this pattern's assertions read.
    pub(crate) fn fact_mask(&self) -> Facts {
        let mut mask = Facts::empty();
        if self
            .assertions
            .intersects(Assertions::BEG_LINE | Assertions::END_LINE)
        {
            mask = mask | Facts::NEWLINE;
        }
        if self.assertions.intersects(Assertions::WORD) {
            mask = mask | Facts::WORD | Facts::WIDE_WORD;
        }
        if self.assertions.intersects(Assertions::SYMBOL) {
            mask = mask | Facts::WORD_OR_SYMBOL;
        }
        mask
    }
}

/// The class of a character: the set of predicates that accept it, and its
/// facts.  Two characters with the same key are indistinguishable to the
/// pattern, so the DFA steps them with one transition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClassKey {
    accepts: SmallVec<[u64; 2]>,
    pub(crate) facts: Facts,
}

impl ClassKey {
    #[inline]
    pub(crate) fn accepts(&self, predicate: u16) -> bool {
        let predicate = predicate as usize;
        self.accepts[predicate / 64] & (1 << (predicate % 64)) != 0
    }
}

/// A syntax lookup that answers every position from the base table.  The
/// memoized classes are computed through it, so they are functions of the
/// character alone; they serve only positions where no `syntax-table`
/// property applies ([`SyntaxLookup::plain_syntax_until`]), where the base
/// table is the answer.  A character inside a property run is classified
/// through the real lookup at its position
/// ([`CharClasses::class_at_position`]).
pub(crate) struct BaseTableView<'a>(pub(crate) &'a dyn SyntaxLookup);

impl SyntaxLookup for BaseTableView<'_> {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        self.0.char_syntax(c)
    }

    fn char_syntax_at(&self, c: char, _input_pos: usize) -> SyntaxClass {
        self.0.char_syntax(c)
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        self.0.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        self.0.word_boundary_between(c1, c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        self.0.cache_key()
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        self.0.class_cache_key()
    }

    fn position_dependent(&self) -> bool {
        false
    }
}

impl Nfa {
    /// Whether `predicate` accepts the character at `d` (`len` bytes): the
    /// matcher's own per-character test.
    fn predicate_accepts(
        &self,
        predicate: Predicate,
        pattern: &CompiledPattern,
        text: &[u8],
        d: usize,
        len: usize,
        syntax: &dyn SyntaxLookup,
    ) -> bool {
        let stop = d + len;
        let target_multibyte = pattern.target_multibyte;
        let accepted = match predicate {
            Predicate::Literal { pc, lit_off } => {
                let pc = pc as usize;
                let count = self.bytecode[pc + 1] as usize;
                let literal = &self.bytecode[pc + 2..pc + 2 + count];
                match_exactn_char_at(
                    literal,
                    lit_off as usize,
                    pattern.multibyte,
                    target_multibyte,
                    &pattern.translate,
                    text,
                    d,
                    stop,
                )
                .map(|(_, text_advance)| text_advance)
            }
            Predicate::AnyChar => {
                match_anychar_at(text, d, stop, target_multibyte, &pattern.translate)
            }
            Predicate::Charset { pc } => match_charset_at(
                pattern,
                pc as usize,
                text,
                d,
                stop,
                target_multibyte,
                &pattern.translate,
                syntax,
            ),
            Predicate::Syntax { class, negate } => {
                match_syntaxspec_at(class, negate, text, d, stop, target_multibyte, syntax)
            }
            Predicate::SyntaxSet { mask } => {
                match_syntaxspecset_at(mask, text, d, stop, target_multibyte, syntax)
            }
            Predicate::Category { category, negate } => {
                match_categoryspec_at(category, negate, text, d, stop, target_multibyte, syntax)
            }
        };
        debug_assert!(accepted.is_none_or(|advance| advance == len));
        accepted.is_some()
    }

    /// The class of the character at `d` of `text`, `len` bytes long, under
    /// `syntax` (the base table, or the real lookup at `d`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn class_key_at(
        &self,
        pattern: &CompiledPattern,
        text: &[u8],
        d: usize,
        code: u32,
        len: usize,
        syntax: &dyn SyntaxLookup,
        mask: Facts,
    ) -> ClassKey {
        let mut accepts: SmallVec<[u64; 2]> =
            SmallVec::from_elem(0, self.predicates.len().div_ceil(64).max(1));
        for (i, &predicate) in self.predicates.iter().enumerate() {
            if self.predicate_accepts(predicate, pattern, text, d, len, syntax) {
                accepts[i / 64] |= 1 << (i % 64);
            }
        }
        ClassKey {
            accepts,
            facts: Facts::of_char(code, text[d], syntax, d, mask),
        }
    }
}

/// `byte_class` value of a byte whose class is not known yet (or, for the
/// bytes of a non-ASCII character of multibyte text, is never kept there).
pub(crate) const UNKNOWN_CLASS: u8 = 0xFF;
/// Class ids are `u8` below [`UNKNOWN_CLASS`].
pub(crate) const MAX_CLASSES: usize = UNKNOWN_CLASS as usize;
const WIDE_SLOTS: usize = 512;
const WIDE_EMPTY: u32 = u32::MAX;
const POSITIONAL_SLOTS: usize = 64;
/// `byte_facts` value of a byte whose facts are not known yet (facts use
/// the low five bits).
const NO_FACTS: u8 = 0xFF;

/// What the character-to-class maps of one pattern are valid for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClassContext {
    /// The tables behind the syntax lookup, when the classes read it.
    lookup: Option<LookupClassKey>,
    /// `char_table_write_tick` when the classes read a char-table (the
    /// syntax or category table, or a case table's non-ASCII translations),
    /// else 0.  The tick moves with every char-table write and allocation.
    tick: u64,
    target_multibyte: bool,
}

impl ClassContext {
    /// The context of a search with `syntax`, or `None` when the lookup's
    /// tables have no identity to key a cache by.
    pub(crate) fn of_search(
        pattern: &CompiledPattern,
        nfa: &Nfa,
        syntax: &dyn SyntaxLookup,
    ) -> Option<Self> {
        let reads_lookup = pattern.uses_syntax || nfa.uses_categories;
        let lookup = if reads_lookup {
            Some(syntax.class_cache_key()?)
        } else {
            None
        };
        let reads_char_table = reads_lookup
            || pattern
                .translate
                .as_ref()
                .is_some_and(|translate| translate.table.is_some());
        Some(Self {
            lookup,
            tick: if reads_char_table {
                crate::emacs_core::chartable::char_table_write_tick()
            } else {
                0
            },
            target_multibyte: pattern.target_multibyte,
        })
    }
}

/// Too many distinct classes for `u8` ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TooManyClasses;

/// The classes of one pattern and the character-to-class maps.
///
/// Class ids name [`ClassKey`]s, which do not depend on the search context,
/// so a context change (another syntax table, a char-table write) only
/// empties the maps from characters to ids; the DFA's transitions, keyed by
/// class id, stay valid.
pub(crate) struct CharClasses {
    /// Class of each byte: every byte of unibyte text, ASCII of multibyte.
    pub(crate) byte_class: [u8; 256],
    /// Direct-mapped `(character code, class)` memo for the other characters.
    wide: Box<[(u32, u8)]>,
    /// Direct-mapped `(character code, syntax class there, class)` memo for
    /// characters inside `syntax-table` property runs.
    positional: Box<[(u32, u8, u8)]>,
    /// The facts of each tabled byte's character ([`NO_FACTS`]: not known
    /// yet), for the character before a candidate, which the DFA need not
    /// have classified.
    byte_facts: [u8; 256],
    keys: Vec<ClassKey>,
    ids: FxHashMap<ClassKey, u8>,
    context: Option<ClassContext>,
    mask: Facts,
    /// Context changes that emptied the maps.
    pub(crate) resets: u64,
}

impl CharClasses {
    pub(crate) fn new(mask: Facts) -> Self {
        Self {
            byte_class: [UNKNOWN_CLASS; 256],
            wide: vec![(WIDE_EMPTY, 0); WIDE_SLOTS].into_boxed_slice(),
            positional: vec![(WIDE_EMPTY, 0, 0); POSITIONAL_SLOTS].into_boxed_slice(),
            byte_facts: [NO_FACTS; 256],
            keys: Vec::new(),
            ids: FxHashMap::default(),
            context: None,
            mask,
            resets: 0,
        }
    }

    /// Make the maps valid for a search in `context`.
    pub(crate) fn sync(&mut self, context: ClassContext) {
        if self.context == Some(context) {
            return;
        }
        if self.context.is_some() {
            self.resets += 1;
        }
        self.byte_class = [UNKNOWN_CLASS; 256];
        self.wide.fill((WIDE_EMPTY, 0));
        self.positional.fill((WIDE_EMPTY, 0, 0));
        self.byte_facts = [NO_FACTS; 256];
        self.context = Some(context);
    }

    #[inline]
    pub(crate) fn key(&self, class: u8) -> &ClassKey {
        &self.keys[class as usize]
    }

    #[inline]
    pub(crate) fn facts(&self, class: u8) -> Facts {
        self.keys[class as usize].facts
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    fn intern(&mut self, key: ClassKey) -> Result<u8, TooManyClasses> {
        if let Some(&id) = self.ids.get(&key) {
            return Ok(id);
        }
        if self.keys.len() >= MAX_CLASSES {
            return Err(TooManyClasses);
        }
        let id = self.keys.len() as u8;
        self.keys.push(key.clone());
        self.ids.insert(key, id);
        Ok(id)
    }

    /// The class and byte length of the character at `d` (`d < text.len()`),
    /// from the maps or computed now.
    pub(crate) fn class_at(
        &mut self,
        nfa: &Nfa,
        pattern: &CompiledPattern,
        text: &[u8],
        d: usize,
        base: &BaseTableView<'_>,
    ) -> Result<(u8, usize), TooManyClasses> {
        let byte = text[d];
        let tabled = !pattern.target_multibyte || byte < 0x80;
        if tabled {
            let class = self.byte_class[byte as usize];
            if class != UNKNOWN_CLASS {
                return Ok((class, 1));
            }
        }
        let (code, len) = re_text_char(text, d, pattern.target_multibyte)
            .expect("a class is asked for a character inside the text");
        if !tabled {
            let slot = self.wide[code as usize % WIDE_SLOTS];
            if slot.0 == code {
                return Ok((slot.1, len));
            }
        }
        let key = nfa.class_key_at(pattern, text, d, code, len, base, self.mask);
        let class = self.intern(key)?;
        if tabled {
            self.byte_class[byte as usize] = class;
        } else {
            self.wide[code as usize % WIDE_SLOTS] = (code, class);
        }
        Ok((class, len))
    }

    /// The class and byte length of the character at `d` (`d < text.len()`)
    /// where a `syntax-table` property may apply: what the matcher's tests
    /// answer there through `syntax` at `d`.
    ///
    /// Every syntax read a class makes is the syntax of this one character
    /// at `d` (`match_syntaxspec_at`, `posix_class_matches`, the facts), so
    /// the class is a function of the character and its syntax class there:
    /// that pair keys the memo, and the base-table memo serves the pair
    /// whenever the property leaves the table's answer unchanged.
    pub(crate) fn class_at_position(
        &mut self,
        nfa: &Nfa,
        pattern: &CompiledPattern,
        text: &[u8],
        d: usize,
        base: &BaseTableView<'_>,
        syntax: &dyn SyntaxLookup,
    ) -> Result<(u8, usize), TooManyClasses> {
        let (code, len) = re_text_char(text, d, pattern.target_multibyte)
            .expect("a class is asked for a character inside the text");
        let ch = regex_syntax_char(code);
        let here = syntax.char_syntax_at(ch, d);
        let slot_index = (code as usize ^ ((here as usize) << 4)) % POSITIONAL_SLOTS;
        let slot = self.positional[slot_index];
        if slot.0 == code && slot.1 == here as u8 {
            return Ok((slot.2, len));
        }
        let class = if here == base.char_syntax(ch) {
            self.class_at(nfa, pattern, text, d, base)?.0
        } else {
            let key = nfa.class_key_at(pattern, text, d, code, len, syntax, self.mask);
            self.intern(key)?
        };
        self.positional[slot_index] = (code, here as u8, class);
        Ok((class, len))
    }

    /// The facts of the tabled byte `text[at]` (a byte of unibyte text, or
    /// ASCII) where the base table applies: its class's facts, or memoized.
    #[inline]
    fn byte_facts_at(
        &mut self,
        text: &[u8],
        at: usize,
        target_multibyte: bool,
        base: &BaseTableView<'_>,
    ) -> Facts {
        let byte = text[at];
        let class = self.byte_class[byte as usize];
        if class != UNKNOWN_CLASS {
            return self.facts(class);
        }
        let memo = self.byte_facts[byte as usize];
        if memo != NO_FACTS {
            return Facts(memo);
        }
        let facts = self.facts_at(text, at, target_multibyte, base);
        self.byte_facts[byte as usize] = facts.0;
        facts
    }

    /// The facts of the character at `at` (the matcher's view of the
    /// character before a candidate), read through `syntax` at `at`.  A
    /// memoized class carries the same facts where the base table applies.
    pub(crate) fn facts_at(
        &self,
        text: &[u8],
        at: usize,
        target_multibyte: bool,
        syntax: &dyn SyntaxLookup,
    ) -> Facts {
        let (code, _) =
            re_text_char(text, at, target_multibyte).expect("the previous character exists");
        Facts::of_char(code, text[at], syntax, at, self.mask)
    }
}

// ---------------------------------------------------------------------------
// The lazy DFA (C4)
// ---------------------------------------------------------------------------

/// The DFA's verdict on one candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exists {
    /// No match starts at the candidate: every path died having consumed at
    /// most `consumed` bytes (the backtracker's fail stack is bounded by it).
    No { consumed: usize },
    /// Some match starts at the candidate.
    Yes,
    /// Not decided (quit pending, or a state cap was hit): run the matcher.
    Unknown,
}

/// Transition-table entries below the stride are sentinels; real entries are
/// row offsets, `(state index + 1) * stride`.
const UNKNOWN: u32 = 0;
const DEAD: u32 = 1;
const MATCH: u32 = 2;
/// The transition depends on the character pair (two word constituents, one
/// above U+00FF): computed at every use, never cached.
const SLOW: u32 = 3;
const MIN_STRIDE_SHIFT: u32 = 3;

/// The cache is cleared at the next candidate once it holds this many bytes.
const MEMORY_CAP: usize = 64 * 1024;
/// Within one candidate the cache may grow to this before the candidate is
/// left undecided.
const MEMORY_HARD_CAP: usize = 4 * MEMORY_CAP;
/// A quit is polled every this many bytes stepped.
const QUIT_POLL_BYTES: usize = 64 * 1024;
/// Bound the duplicated work for a cached prefix that eventually succeeds.
const CACHED_PREFIX_BYTES: usize = 8;

/// A DFA state: the NFA kernel (where threads resume, before the closure)
/// and the facts about the character before the position.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StateKey {
    kernel: Box<[KernelItem]>,
    prev: Facts,
}

/// Counters of one DFA (reported under `NEOVM_REGEX_DFA_STATS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DfaCounters {
    pub(crate) yes: u64,
    pub(crate) no: u64,
    pub(crate) unknown: u64,
    pub(crate) states: u64,
    pub(crate) clears: u64,
    pub(crate) slow_transitions: u64,
    pub(crate) bytes: u64,
    /// Characters classified at their position, inside a `syntax-table`
    /// property run.
    pub(crate) positional_chars: u64,
    /// Property-free stretches looked up (`plain_syntax_until`).
    pub(crate) plain_runs: u64,
    /// Candidates left to the matcher because the DFA would have read syntax
    /// at or past the lazy `syntax-propertize` frontier.
    pub(crate) frontier_unknown: u64,
}

/// What a search's syntax lookup means for the DFA, fixed for the search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchSyntax {
    /// The pattern reads syntax and `syntax-table` properties may apply: the
    /// cached loop stops at property runs, and syntax is read through the
    /// real lookup at the real position.
    positional: bool,
    /// The first input position whose syntax the matcher records for lazy
    /// `syntax-propertize` (`usize::MAX`: none).  A candidate the DFA cannot
    /// decide below it is left to the matcher.
    read_limit: usize,
}

impl SearchSyntax {
    const PLAIN: Self = Self {
        positional: false,
        read_limit: usize::MAX,
    };
}

/// Why a DFA gave up on its pattern for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DfaGaveUp {
    TooManyClasses,
    /// The cache kept overflowing (4 clears in one search).
    StateExplosion,
}

/// The anchored existence DFA of one pattern (see the module docs).
pub(crate) struct ExistenceDfa {
    nfa: Nfa,
    classes: CharClasses,
    /// Facts a state keeps about the previous character: the class facts
    /// plus [`Facts::EDGE`] when some assertion tests the text start.
    prev_mask: Facts,
    stride_shift: u32,
    trans: Vec<u32>,
    states: Vec<StateKey>,
    state_ids: FxHashMap<StateKey, u32>,
    /// Start state index + 1 per previous-character facts (0: not built).
    start: [u32; 32],
    scratch: ClosureScratch,
    positions: Vec<u16>,
    kernel: Vec<KernelItem>,
    memory: usize,
    clears_this_search: u32,
    search: SearchSyntax,
    /// A property-free input stretch this search has looked up.
    plain: std::ops::Range<usize>,
    /// Whether the pattern tests point (`\=`).
    has_at_dot: bool,
    pub(crate) counters: DfaCounters,
    gave_up: Option<DfaGaveUp>,
}

impl ExistenceDfa {
    pub(crate) fn new(nfa: Nfa) -> Self {
        let mask = nfa.fact_mask();
        let reads_edge = nfa.assertions.intersects(
            Assertions::BEG_LINE | Assertions::BEG_BUF | Assertions::WORD | Assertions::SYMBOL,
        );
        let prev_mask = if reads_edge { mask | Facts::EDGE } else { mask };
        let mut dfa = Self {
            classes: CharClasses::new(mask),
            nfa,
            prev_mask,
            stride_shift: MIN_STRIDE_SHIFT,
            trans: Vec::new(),
            states: Vec::new(),
            state_ids: FxHashMap::default(),
            start: [0; 32],
            scratch: ClosureScratch::default(),
            positions: Vec::new(),
            kernel: Vec::new(),
            memory: 0,
            clears_this_search: 0,
            search: SearchSyntax::PLAIN,
            plain: 0..0,
            has_at_dot: false,
            counters: DfaCounters::default(),
            gave_up: None,
        };
        dfa.has_at_dot = dfa.nfa.assertions.contains(Assertions::AT_DOT);
        dfa.trans.resize(dfa.stride(), UNKNOWN);
        dfa
    }

    pub(crate) fn nfa(&self) -> &Nfa {
        &self.nfa
    }

    #[cfg(test)]
    pub(crate) fn classes_mut(&mut self) -> &mut CharClasses {
        &mut self.classes
    }

    /// Why this DFA stopped deciding for good, if it did.
    pub(crate) fn gave_up(&self) -> Option<DfaGaveUp> {
        self.gave_up
    }

    /// Start a search of `pattern` under `syntax`: per-search limits reset,
    /// and what the lookup means for the DFA is read once.  Every
    /// [`Self::anchored_exists`] until the next call must pass the same
    /// lookup.
    pub(crate) fn begin_search(&mut self, pattern: &CompiledPattern, syntax: &dyn SyntaxLookup) {
        self.clears_this_search = 0;
        self.plain = 0..0;
        self.search = SearchSyntax {
            positional: self.nfa.reads_syntax && syntax.position_dependent(),
            // The matcher records syntax reads past the frontier only for a
            // pattern that reads syntax (`uses_syntax`, the builtin's test).
            read_limit: if pattern.uses_syntax {
                syntax.syntax_read_limit()
            } else {
                usize::MAX
            },
        };
    }

    #[inline]
    fn stride(&self) -> usize {
        1 << self.stride_shift
    }

    #[inline]
    fn row_of(&self, index: u32) -> u32 {
        (index + 1) << self.stride_shift
    }

    #[inline]
    fn index_of(&self, row: u32) -> u32 {
        (row >> self.stride_shift) - 1
    }

    /// Drop every state and transition, keeping the NFA and the classes.
    fn clear_states(&mut self) {
        self.states.clear();
        self.state_ids.clear();
        self.trans.clear();
        self.trans.resize(self.stride(), UNKNOWN);
        self.start = [0; 32];
        self.memory = 0;
        self.counters.clears += 1;
        self.clears_this_search += 1;
        if self.clears_this_search >= 4 {
            self.gave_up = Some(DfaGaveUp::StateExplosion);
        }
    }

    /// Widen the rows when the classes outgrow them (all transitions are
    /// dropped: they are relearned lazily).
    fn fit_classes(&mut self) {
        let classes = self.classes.len();
        if classes <= self.stride() {
            return;
        }
        while (1usize << self.stride_shift) < classes {
            self.stride_shift += 1;
        }
        self.trans.clear();
        self.trans
            .resize((self.states.len() + 1) << self.stride_shift, UNKNOWN);
        self.memory = self.trans.len() * 4
            + self
                .states
                .iter()
                .map(|state| state.kernel.len() * 4 + 48)
                .sum::<usize>();
    }

    /// The row of the state `(kernel, prev)`, added if new.
    fn intern_state(&mut self, kernel: &[KernelItem], prev: Facts) -> u32 {
        let key = StateKey {
            kernel: kernel.into(),
            prev,
        };
        if let Some(&index) = self.state_ids.get(&key) {
            return self.row_of(index);
        }
        let index = self.states.len() as u32;
        self.memory += key.kernel.len() * 4 + 48 + self.stride() * 4;
        self.states.push(key.clone());
        self.state_ids.insert(key, index);
        self.trans.resize(self.trans.len() + self.stride(), UNKNOWN);
        self.counters.states += 1;
        self.row_of(index)
    }

    fn start_row(&mut self, prev: Facts) -> u32 {
        let slot = prev.bits() as usize;
        match self.start[slot] {
            0 => {
                let row = self.intern_state(&[kernel_item(0, 0)], prev);
                self.start[slot] = self.index_of(row) + 1;
                row
            }
            index => self.row_of(index - 1),
        }
    }

    /// Whether a transition out of a state with previous-character `prev`
    /// on a character of class `class` depends on the character pair.
    #[inline]
    fn pair_dependent(&self, prev: Facts, class: u8) -> bool {
        if !self.nfa.assertions.intersects(Assertions::WORD) {
            return false;
        }
        let current = self.classes.facts(class);
        prev.contains(Facts::WORD)
            && current.contains(Facts::WORD)
            && (prev.contains(Facts::WIDE_WORD) || current.contains(Facts::WIDE_WORD))
    }

    /// Compute the transition out of the state at `row` on the character of
    /// class `class` at `at.d`: the closure at `at`, then every consuming
    /// position whose predicate accepts the class.  Cached unless `cache` is
    /// false or the pair decides it.
    fn transition(&mut self, row: u32, class: u8, at: &Place<'_>, cache: bool) -> u32 {
        let index = self.index_of(row) as usize;
        let prev = self.states[index].prev;
        let kernel = std::mem::take(&mut self.kernel);
        let mut positions = std::mem::take(&mut self.positions);
        positions.clear();
        let accept = {
            let state = &self.states[index];
            self.nfa
                .closure(&state.kernel, at, &mut self.scratch, &mut positions)
        };
        let result = if accept {
            self.kernel = kernel;
            MATCH
        } else {
            let mut next = kernel;
            next.clear();
            let key = self.classes.key(class);
            for &position in &positions {
                let position = self.nfa.positions[position as usize];
                if key.accepts(position.predicate) {
                    next.push(position.next);
                }
            }
            next.sort_unstable();
            next.dedup();
            let result = if next.is_empty() {
                DEAD
            } else {
                let facts = self.classes.facts(class);
                let facts = Facts(facts.bits() & self.prev_mask.bits());
                self.intern_state(&next, facts)
            };
            self.kernel = next;
            result
        };
        self.positions = positions;
        let slow = self.pair_dependent(prev, class);
        if slow {
            self.counters.slow_transitions += 1;
        }
        if cache {
            let entry = &mut self.trans[row as usize + class as usize];
            *entry = if slow { SLOW } else { result };
        }
        result
    }

    /// The closure of the state at `row` at `at`, and the consuming step on
    /// the character there, never cached: a special position (point for a
    /// `\=` pattern).  Returns the next row, `MATCH` or `DEAD`.
    fn special_transition(&mut self, row: u32, class: u8, at: &Place<'_>) -> u32 {
        self.transition(row, class, at, false)
    }

    /// Whether the state at `row`, at `at` (the match stop), accepts.
    fn accepts_at(&mut self, row: u32, at: &Place<'_>) -> bool {
        let index = self.index_of(row) as usize;
        let mut positions = std::mem::take(&mut self.positions);
        positions.clear();
        let accept = self.nfa.closure(
            &self.states[index].kernel,
            at,
            &mut self.scratch,
            &mut positions,
        );
        self.positions = positions;
        accept
    }

    /// Can a match of `pattern` start at `p` and end by `stop`?  The same
    /// question the backtracker answers at the candidate, without registers.
    /// Inlined into the out-of-line [`DfaLease::candidate`]: a rejection the
    /// cached loop settles costs one call from `re_search`.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn anchored_exists(
        &mut self,
        pattern: &CompiledPattern,
        text: &[u8],
        p: usize,
        stop: usize,
        point: usize,
        syntax: &dyn SyntaxLookup,
    ) -> Exists {
        self.anchored_exists_inner::<true>(pattern, text, p, stop, point, syntax)
    }

    /// The candidate path after a caller's inline probe omits that probe
    /// here (`FIRST_STEP == false`); direct and verify calls retain it.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn anchored_exists_inner<const FIRST_STEP: bool>(
        &mut self,
        pattern: &CompiledPattern,
        text: &[u8],
        p: usize,
        stop: usize,
        point: usize,
        syntax: &dyn SyntaxLookup,
    ) -> Exists {
        if self.gave_up.is_some() {
            return Exists::Unknown;
        }
        if self.memory > MEMORY_CAP {
            self.clear_states();
            if self.gave_up.is_some() {
                return Exists::Unknown;
            }
        }
        let cached = if FIRST_STEP && first_step_enabled() {
            self.cached_prefix_rejection(pattern, text, p, stop, point, usize::MAX)
        } else {
            None
        };
        let verdict = if let Some(consumed) = cached {
            self.counters.bytes += consumed as u64;
            Exists::No { consumed }
        } else {
            self.run(pattern, text, p, stop, point, syntax)
        };
        match verdict {
            Exists::Yes => self.counters.yes += 1,
            Exists::No { .. } => self.counters.no += 1,
            Exists::Unknown => self.counters.unknown += 1,
        }
        verdict
    }

    /// A bounded cached rejection, without classifying, interning, or reading
    /// syntax. Called only after the give-up and memory-cap guards. Returns
    /// the same consumed span as `run`, strictly below `overflow_free_span`.
    ///
    /// Threading: this cache and its compiled pattern are mutator-owned, not
    /// shared concurrently. `DfaLease::acquire` synchronizes character maps
    /// with the search context before calling this helper. A context change
    /// clears those maps; the context-independent transitions remain valid.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn cached_prefix_rejection(
        &self,
        pattern: &CompiledPattern,
        text: &[u8],
        p: usize,
        stop: usize,
        point: usize,
        overflow_free_span: usize,
    ) -> Option<usize> {
        // Establish memory safety first. The remaining guards apply only
        // once an existing DEAD entry could actually reject this candidate.
        if p >= text.len() {
            return None;
        }
        let class = self.classes.byte_class[text[p] as usize];
        if class == UNKNOWN_CLASS {
            return None;
        }
        let prev_mask = self.prev_mask.0;
        let reads_prev_syntax = prev_mask & !(Facts::EDGE.0 | Facts::NEWLINE.0) != 0;
        let facts = if prev_mask == 0 {
            // No assertion reads the previous character: use start[0]
            // without inspecting point zero or the preceding byte.
            0
        } else if p == 0 {
            Facts::EDGE.0
        } else if !reads_prev_syntax {
            // EDGE and newline facts do not depend on syntax or decoding.
            if text[p - 1] == b'\n' {
                Facts::NEWLINE.0
            } else {
                0
            }
        } else {
            let byte = text[p - 1];
            if pattern.target_multibyte && byte >= 0x80 {
                return None;
            }
            let prev_class = self.classes.byte_class[byte as usize];
            if prev_class != UNKNOWN_CLASS {
                self.classes.facts(prev_class).0
            } else {
                let facts = self.classes.byte_facts[byte as usize];
                if facts == NO_FACTS {
                    return None;
                }
                facts
            }
        };
        let start = self.start[(facts & prev_mask) as usize];
        if start == 0 {
            return None;
        }
        let mut row = start << self.stride_shift;
        // Every consumed byte must be before the stop, frontier and point's
        // special closure, and within already established plain syntax.
        // Eight probes cover short literal failures without making a true
        // candidate walk an unbounded prefix twice.
        let mut limit = stop
            .min(text.len())
            .min(self.search.read_limit)
            .min(p.saturating_add(CACHED_PREFIX_BYTES))
            .min(p.saturating_add(overflow_free_span));
        if self.has_at_dot && point >= p {
            limit = limit.min(point);
        }
        if self.search.positional {
            // The current class is a base-table class. Previous word/symbol
            // facts require base syntax at p - 1 too, in the same known run.
            let from = if p > 0 && reads_prev_syntax { p - 1 } else { p };
            if from < self.plain.start || p >= self.plain.end {
                return None;
            }
            limit = limit.min(self.plain.end);
        }
        let stride = 1u32 << self.stride_shift;
        let mut d = p;
        while d < limit {
            let class = if d == p {
                class
            } else {
                self.classes.byte_class[text[d] as usize]
            };
            if class == UNKNOWN_CLASS {
                return None;
            }
            let at = row as usize + class as usize;
            debug_assert!(at < self.trans.len());
            // SAFETY: as in `run`, cached rows have a complete stride and
            // known byte classes are below that stride. This path mutates
            // neither rows nor classes.
            let next = unsafe { *self.trans.get_unchecked(at) };
            if next == DEAD {
                return Some(d - p);
            }
            if next < stride {
                return None;
            }
            row = next;
            d += 1;
        }
        None
    }

    /// The one-step subset is kept explicit in the cache guard tests.
    #[cfg(test)]
    fn cached_first_step_dead(
        &self,
        pattern: &CompiledPattern,
        text: &[u8],
        p: usize,
        stop: usize,
        point: usize,
    ) -> bool {
        self.cached_prefix_rejection(pattern, text, p, stop, point, 1) == Some(0)
    }

    /// The end of the property-free stretch from `at` (see
    /// [`SyntaxLookup::plain_syntax_until`]), remembered for the search: the
    /// candidates of a scan mostly fall into the stretch the last one did.
    #[inline]
    fn plain_until(&mut self, at: usize, syntax: &dyn SyntaxLookup) -> usize {
        if self.plain.start <= at && at < self.plain.end {
            return self.plain.end;
        }
        self.counters.plain_runs += 1;
        let end = syntax.plain_syntax_until(at);
        if end > at {
            self.plain = at..end;
        }
        end
    }

    /// The start state at candidate `p` and the end of the property-free
    /// stretch the cached loop may run in from `p`: the previous character's
    /// facts come from its memoized class when the base table applies there.
    #[inline(always)]
    fn start_at(
        &mut self,
        text: &[u8],
        p: usize,
        target_multibyte: bool,
        syntax: &dyn SyntaxLookup,
    ) -> (u32, usize) {
        let positional = self.search.positional;
        if p == 0 {
            let plain_end = if positional {
                self.plain_until(0, syntax)
            } else {
                usize::MAX
            };
            return (
                self.start_row(Facts(Facts::EDGE.0 & self.prev_mask.0)),
                plain_end,
            );
        }
        let byte = text[p - 1];
        if byte < 0x80 || !target_multibyte {
            // A single-byte previous character at `p - 1`: one lookup says
            // both whether its memoized class applies and how far the
            // stretch runs (one that ends at `p` is renewed there).
            let (plain_end, prev_plain) = if positional {
                let end = self.plain_until(p - 1, syntax);
                (end, end > p - 1)
            } else {
                (usize::MAX, true)
            };
            if prev_plain {
                let base = BaseTableView(syntax);
                let facts = self
                    .classes
                    .byte_facts_at(text, p - 1, target_multibyte, &base);
                return (self.start_row(Facts(facts.0 & self.prev_mask.0)), plain_end);
            }
            return (
                self.start_row_slow(text, p, target_multibyte, syntax),
                plain_end,
            );
        }
        let plain_end = if positional {
            self.plain_until(p, syntax)
        } else {
            usize::MAX
        };
        (
            self.start_row_slow(text, p, target_multibyte, syntax),
            plain_end,
        )
    }

    /// The start state at `p` from the previous character read at its
    /// position (the lookup the closures use).
    #[cold]
    #[inline(never)]
    fn start_row_slow(
        &mut self,
        text: &[u8],
        p: usize,
        target_multibyte: bool,
        syntax: &dyn SyntaxLookup,
    ) -> u32 {
        let base = BaseTableView(syntax);
        let lookup: &dyn SyntaxLookup = if self.search.positional {
            syntax
        } else {
            &base
        };
        let facts = match super::re_prev_char_start(text, p, target_multibyte) {
            None => Facts::EDGE,
            Some(start) => self.classes.facts_at(text, start, target_multibyte, lookup),
        };
        self.start_row(Facts(facts.0 & self.prev_mask.0))
    }

    /// The decision at one candidate: the start state, then the cached loop,
    /// which settles most rejections by itself (a cached dead transition);
    /// everything else resumes out of line.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        pattern: &CompiledPattern,
        text: &[u8],
        p: usize,
        stop: usize,
        point: usize,
        syntax: &dyn SyntaxLookup,
    ) -> Exists {
        let stop = stop.min(text.len());
        if p > stop {
            return Exists::No { consumed: 0 };
        }
        // Every syntax read of a candidate is at or below the furthest
        // position its threads reach, and the matcher's threads are among
        // the DFA's (the rewind view only adds paths).  So a candidate the
        // DFA decides without reaching the lazy-propertize frontier is one
        // the matcher would have failed without recording a read there; one
        // that reaches it is left to the matcher, which records its reads.
        let read_limit = self.search.read_limit;
        if p >= read_limit {
            self.counters.frontier_unknown += 1;
            return Exists::Unknown;
        }
        let (mut row, plain_end) = self.start_at(text, p, pattern.target_multibyte, syntax);
        // `\=` holds at point only: that position's closure is never cached.
        let special = if self.has_at_dot && point >= p {
            point
        } else {
            usize::MAX
        };
        let limit = stop.min(special).min(plain_end).min(read_limit);
        let chunk = limit.min(p.saturating_add(QUIT_POLL_BYTES));
        let mut d = p;
        let stride = 1u32 << self.stride_shift;
        let mut next = stride;
        {
            let byte_class = &self.classes.byte_class;
            let trans = &self.trans[..];
            let window = &text[..chunk];
            while d < window.len() {
                let class = byte_class[window[d] as usize];
                if class == UNKNOWN_CLASS {
                    next = UNKNOWN;
                    break;
                }
                let at = row as usize + class as usize;
                debug_assert!(at < trans.len());
                // SAFETY: `row` is the row of a live state (every row is
                // `stride` entries of `trans`) and a class in the byte table
                // is below `classes.len() <= stride` (`fit_classes` runs after
                // every new class).
                next = unsafe { *trans.get_unchecked(at) };
                if next < stride {
                    break;
                }
                row = next;
                d += 1;
            }
        }
        // A cached dead or accepting transition at `d` (a plain position
        // short of every special one) is the answer the slow path would
        // compute there.
        match next {
            DEAD => {
                self.counters.bytes += (d - p) as u64;
                Exists::No { consumed: d - p }
            }
            MATCH => {
                self.counters.bytes += (d - p) as u64;
                Exists::Yes
            }
            _ => self.resume(
                pattern, text, p, d, row, stop, point, special, plain_end, syntax,
            ),
        }
    }

    /// The rest of a candidate the cached loop could not settle: new
    /// classes and transitions, property runs, point, the stop, quit polls.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn resume(
        &mut self,
        pattern: &CompiledPattern,
        text: &[u8],
        p: usize,
        mut d: usize,
        mut row: u32,
        stop: usize,
        point: usize,
        mut special: usize,
        mut plain_end: usize,
        syntax: &dyn SyntaxLookup,
    ) -> Exists {
        let base = BaseTableView(syntax);
        let target_multibyte = pattern.target_multibyte;
        let search = self.search;
        // Where `syntax-table` properties may apply, every syntax the DFA
        // reads -- assertions in the closure, the previous character, a
        // character inside a property run -- is read through the matcher's
        // own lookup at the real position.  The cached loop steps only
        // through property-free stretches, whose memoized base-table classes
        // are what that lookup answers there.  A cached transition stays a
        // function of (state, class): the assertions it evaluated read only
        // the facts both carry, and a class read at a position is interned
        // like any other.
        let lookup: &dyn SyntaxLookup = if search.positional { syntax } else { &base };
        let place = |d: usize| Place {
            text,
            d,
            stop,
            point,
            target_multibyte,
            syntax: lookup,
        };
        let mut polled = d - p;
        loop {
            // The cached loop: single-byte characters with a known class and
            // a cached live transition, in chunks between quit polls.
            let limit = stop.min(special).min(plain_end).min(search.read_limit);
            let chunk = limit.min(d.saturating_add(QUIT_POLL_BYTES));
            let start = d;
            {
                let byte_class = &self.classes.byte_class;
                let trans = &self.trans[..];
                let stride = 1u32 << self.stride_shift;
                let window = &text[..chunk];
                while d < window.len() {
                    let class = byte_class[window[d] as usize];
                    if class == UNKNOWN_CLASS {
                        break;
                    }
                    let at = row as usize + class as usize;
                    debug_assert!(at < trans.len());
                    // SAFETY: as in `run`.
                    let next = unsafe { *trans.get_unchecked(at) };
                    if next < stride {
                        break;
                    }
                    row = next;
                    d += 1;
                }
            }
            polled += d - start;
            if polled >= QUIT_POLL_BYTES {
                polled = 0;
                if crate::emacs_core::eval::tls_quit_pending() {
                    return Exists::Unknown;
                }
            }
            if d >= search.read_limit {
                self.counters.bytes += (d - p) as u64;
                self.counters.frontier_unknown += 1;
                return Exists::Unknown;
            }
            if d >= stop {
                self.counters.bytes += (d - p) as u64;
                return if self.accepts_at(row, &place(d)) {
                    Exists::Yes
                } else {
                    Exists::No { consumed: d - p }
                };
            }
            if d == chunk && d < limit {
                continue;
            }
            // Past the property-free stretch: the next one, or a character
            // inside a property run.
            let mut at_position = false;
            if d >= plain_end {
                plain_end = self.plain_until(d, syntax);
                if plain_end > d {
                    continue;
                }
                at_position = true;
            }
            // One character the cached loop could not take.
            let row_index = self.index_of(row);
            let classified = if at_position {
                self.counters.positional_chars += 1;
                self.classes
                    .class_at_position(&self.nfa, pattern, text, d, &base, syntax)
            } else {
                self.classes.class_at(&self.nfa, pattern, text, d, &base)
            };
            let (class, len) = match classified {
                Ok(found) => found,
                Err(TooManyClasses) => {
                    self.gave_up = Some(DfaGaveUp::TooManyClasses);
                    return Exists::Unknown;
                }
            };
            self.fit_classes();
            row = self.row_of(row_index);
            let next = if d == special {
                special = usize::MAX;
                self.special_transition(row, class, &place(d))
            } else {
                match self.trans[row as usize + class as usize] {
                    UNKNOWN => self.transition(row, class, &place(d), true),
                    SLOW => self.transition(row, class, &place(d), false),
                    cached => cached,
                }
            };
            match next {
                MATCH => {
                    self.counters.bytes += (d - p) as u64;
                    return Exists::Yes;
                }
                DEAD => {
                    self.counters.bytes += (d - p) as u64;
                    return Exists::No { consumed: d - p };
                }
                live => {
                    if d + len > stop {
                        // The character straddles the stop: no path that
                        // consumed it can end or go on.
                        self.counters.bytes += (d + len - p) as u64;
                        return Exists::No {
                            consumed: d + len - p,
                        };
                    }
                    row = live;
                    d += len;
                }
            }
            if self.memory > MEMORY_HARD_CAP {
                return Exists::Unknown;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The candidate filter in `re_search` (C5)
// ---------------------------------------------------------------------------

/// `NEOVM_REGEX_DFA`: whether `re_search` filters candidates through the
/// existence DFA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DfaMode {
    /// No DFA: the search path is unchanged but for one branch per search
    /// and per candidate.
    Off,
    /// The default: a candidate the DFA rejects is skipped.
    On,
    /// Every candidate runs the matcher too, and a verdict the matcher
    /// contradicts is reported (`tracing::error!`, and the stats' mismatch
    /// counts).  For tests and soaks.
    Verify,
}

impl DfaMode {
    /// The mode a value of `NEOVM_REGEX_DFA` selects: `off`/`0`/`false`/`no`,
    /// `verify`, anything else (or unset) on.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("verify") => Self::Verify,
            Some("0" | "off" | "false" | "no") => Self::Off,
            _ => Self::On,
        }
    }
}

#[cfg(any(test, feature = "fuzzing"))]
thread_local! {
    static DFA_MODE_OVERRIDE: std::cell::Cell<Option<DfaMode>> =
        const { std::cell::Cell::new(None) };
}

/// Run `f` with `NEOVM_REGEX_DFA` forced to `mode` on this thread.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn with_dfa_mode<R>(mode: DfaMode, f: impl FnOnce() -> R) -> R {
    struct Guard(Option<DfaMode>);
    impl Drop for Guard {
        fn drop(&mut self) {
            DFA_MODE_OVERRIDE.with(|slot| slot.set(self.0));
        }
    }
    let _guard = Guard(DFA_MODE_OVERRIDE.with(|slot| slot.replace(Some(mode))));
    f()
}

#[cfg(any(test, feature = "fuzzing"))]
thread_local! {
    // A test-only knob override, not a cache of Lisp state.
    static DFA_COLD_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Run `f` with the cold-path knob forced to `on` on this thread.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn with_cold_path<R>(on: bool, f: impl FnOnce() -> R) -> R {
    struct Guard(Option<bool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            DFA_COLD_OVERRIDE.with(|slot| slot.set(self.0));
        }
    }
    let _guard = Guard(DFA_COLD_OVERRIDE.with(|slot| slot.replace(Some(on))));
    f()
}

/// `NEOVM_REGEX_DFA_COLD` (default on; `off` restores the eager lease): a
/// never-built slot pays no lease
/// or candidate wrapper until enough failed candidates justify a filter.
/// An isolated miss followed by a match does not heat the pattern. Two
/// nonempty failures in one scan are admitted together; a lone nonempty
/// failure is admitted when the whole search fails without overflow or quit. Dense
/// scans still build at the existing threshold, and repeated whole failures
/// build for later searches. Zero-span failures do not count. Read once per
/// process.
#[inline]
pub(crate) fn cold_path_enabled() -> bool {
    #[cfg(any(test, feature = "fuzzing"))]
    if let Some(on) = DFA_COLD_OVERRIDE.with(|slot| slot.get()) {
        return on;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = !super::regex_knob_off(std::env::var("NEOVM_REGEX_DFA_COLD").ok().as_deref());
        tracing::debug!(target: "neovm::regex", on, "NEOVM_REGEX_DFA_COLD");
        on
    })
}

/// The `NEOVM_REGEX_DFA` mode, read once per process.
#[inline]
pub(crate) fn dfa_mode() -> DfaMode {
    #[cfg(any(test, feature = "fuzzing"))]
    if let Some(mode) = DFA_MODE_OVERRIDE.with(|slot| slot.get()) {
        return mode;
    }
    static MODE: std::sync::OnceLock<DfaMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(read_dfa_mode)
}

#[cold]
fn read_dfa_mode() -> DfaMode {
    let mode = DfaMode::parse(std::env::var("NEOVM_REGEX_DFA").ok().as_deref());
    tracing::debug!(target: "neovm::regex", ?mode, "NEOVM_REGEX_DFA");
    if mode != DfaMode::Off
        && super::regex_knob_on(std::env::var("NEOVM_REGEX_DFA_STATS").ok().as_deref())
    {
        STATS_ON.store(true, std::sync::atomic::Ordering::Relaxed);
        register_stats_report();
    }
    if mode == DfaMode::Verify {
        STATS_ON.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    mode
}

#[cfg(any(test, feature = "fuzzing"))]
thread_local! {
    // A test-only knob override; the runtime has no additional TLS cache.
    static DFA_FIRST_STEP_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Run `f` with the cached first-step knob forced to `on` on this thread.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn with_first_step<R>(on: bool, f: impl FnOnce() -> R) -> R {
    struct Guard(Option<bool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            DFA_FIRST_STEP_OVERRIDE.with(|slot| slot.set(self.0));
        }
    }
    let _guard = Guard(DFA_FIRST_STEP_OVERRIDE.with(|slot| slot.replace(Some(on))));
    f()
}

/// `NEOVM_REGEX_DFA_FIRST_STEP` (default on; `off` disables): consult an existing dead
/// transition within an eight-byte cached prefix before the general loop.
#[inline]
pub(crate) fn first_step_enabled() -> bool {
    #[cfg(any(test, feature = "fuzzing"))]
    if let Some(on) = DFA_FIRST_STEP_OVERRIDE.with(|slot| slot.get()) {
        return on;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on =
            !super::regex_knob_off(std::env::var("NEOVM_REGEX_DFA_FIRST_STEP").ok().as_deref());
        tracing::debug!(target: "neovm::regex", on, "NEOVM_REGEX_DFA_FIRST_STEP");
        on
    })
}

/// Whether the filter keeps its counters: under `NEOVM_REGEX_DFA_STATS=1`,
/// in verify mode (its contradicted verdicts are counted), and in tests.
/// Otherwise a search pays for none of them.
static STATS_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[inline]
fn stats_on() -> bool {
    cfg!(any(test, feature = "fuzzing")) || STATS_ON.load(std::sync::atomic::Ordering::Relaxed)
}

/// Counters of the candidate filter on this thread (the regexp engine runs
/// on the Lisp thread), reported at exit under `NEOVM_REGEX_DFA_STATS=1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DfaStats {
    /// Searches that used a lease.
    pub(crate) searches: u64,
    /// Searches of a pattern that reads syntax under a lookup where
    /// `syntax-table` properties may apply (the DFA splits its scan at
    /// property runs).
    pub(crate) positional: u64,
    /// Searches whose lazy `syntax-propertize` frontier lies within the
    /// positions they can read (candidates reaching it go to the matcher).
    pub(crate) frontier: u64,
    /// Searches in an adaptive-bypass holiday.
    pub(crate) holiday_off: u64,
    pub(crate) builds: u64,
    pub(crate) ineligible: u64,
    pub(crate) gave_up: u64,
    pub(crate) yes: u64,
    pub(crate) no: u64,
    pub(crate) unknown: u64,
    /// Rejections the fail-stack bound refused to act on.
    pub(crate) overflow_guarded: u64,
    /// Candidates skipped: matcher entries saved.
    pub(crate) skipped: u64,
    pub(crate) context_resets: u64,
    pub(crate) states: u64,
    pub(crate) clears: u64,
    pub(crate) slow_transitions: u64,
    pub(crate) bytes: u64,
    pub(crate) positional_chars: u64,
    pub(crate) plain_runs: u64,
    pub(crate) frontier_unknown: u64,
    /// Verify mode: a rejection the matcher contradicted (a missed match).
    pub(crate) verify_bad_no: u64,
    /// Verify mode: an acceptance the matcher contradicted.
    pub(crate) verify_bad_yes: u64,
}

thread_local! {
    static STATS: RefCell<DfaStats> = const {
        RefCell::new(DfaStats {
            searches: 0,
            positional: 0,
            frontier: 0,
            holiday_off: 0,
            builds: 0,
            ineligible: 0,
            gave_up: 0,
            yes: 0,
            no: 0,
            unknown: 0,
            overflow_guarded: 0,
            skipped: 0,
            context_resets: 0,
            states: 0,
            clears: 0,
            slow_transitions: 0,
            bytes: 0,
            positional_chars: 0,
            plain_runs: 0,
            frontier_unknown: 0,
            verify_bad_no: 0,
            verify_bad_yes: 0,
        })
    };
}

#[inline]
fn stat(update: impl FnOnce(&mut DfaStats)) {
    if !stats_on() {
        return;
    }
    STATS.with(|cell| {
        if let Ok(mut stats) = cell.try_borrow_mut() {
            update(&mut stats);
        }
    });
}

/// This thread's filter counters.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn dfa_stats() -> DfaStats {
    STATS.with(|cell| *cell.borrow())
}

/// Reset this thread's filter counters (tests).
#[cfg(test)]
pub(crate) fn reset_dfa_stats() {
    STATS.with(|cell| *cell.borrow_mut() = DfaStats::default());
}

fn register_stats_report() {
    extern "C" fn report() {
        let stats = STATS
            .try_with(|cell| cell.try_borrow().map(|stats| *stats).unwrap_or_default())
            .unwrap_or_default();
        let line = format!("[neovm-regex-dfa] {stats:?}\n");
        let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), line.as_bytes());
    }
    // SAFETY: `report` is an `extern "C" fn()` that only reads this
    // thread's counters and writes one line to stderr.
    unsafe {
        libc::atexit(report);
    }
}

/// Failed matcher entries a pattern's search sees before it builds a DFA.
const COLD_THRESHOLD: u32 = 16;
/// Decisions per adaptive-bypass window.
const BYPASS_WINDOW: u32 = 64;
/// A window in which more than this share (percent) of the verdicts are
/// "yes" sends the pattern on holiday: the DFA saves nothing there.
const BYPASS_YES_PERCENT: u32 = 60;
/// Searches a holiday lasts before the DFA is probed again.
const BYPASS_HOLIDAY: u32 = 256;

/// A pattern's DFA, with its adaptive-bypass window.
pub(crate) struct LiveDfa {
    pub(crate) dfa: ExistenceDfa,
    /// A rejection that consumed fewer bytes than this cannot hide a
    /// fail-stack overflow ([`fail_stack_overflow_free_span`]).
    overflow_free_span: usize,
    decisions: u32,
    yes: u32,
    holiday: u32,
}

impl LiveDfa {
    fn new(dfa: ExistenceDfa) -> Self {
        Self {
            overflow_free_span: fail_stack_overflow_free_span(dfa.nfa().push_sites),
            dfa,
            decisions: 0,
            yes: 0,
            holiday: 0,
        }
    }
}

/// The state of a pattern's existence DFA.
pub(crate) enum DfaSlot {
    /// Not built yet: admitted failed matcher entries seen so far.
    Cold {
        failed: u32,
    },
    Live(Box<LiveDfa>),
    /// The reasons are kept for tests and debugging.
    Ineligible(#[allow(dead_code)] DfaIneligible),
    Disabled(#[allow(dead_code)] DfaGaveUp),
}

/// `CompiledPattern`'s DFA slot. A clone starts cold: the DFA's caches
/// belong to one pattern object.
///
/// Threading: compiled patterns and their `RefCell` slots are mutator-owned,
/// not `Sync`; another mutator cannot borrow or mutate this slot concurrently.
/// The atomic is an advisory publication hint, not permission to share the slot:
/// Release publishes a fully initialized non-cold slot, and Acquire reads it
/// before trying the existing checked borrow. A false hint runs the matcher.
/// Re-entrant searches still fall back when the slot is borrowed.
pub(crate) struct DfaCell {
    slot: RefCell<DfaSlot>,
    initialized: std::sync::atomic::AtomicBool,
}

impl Default for DfaCell {
    fn default() -> Self {
        Self {
            slot: RefCell::new(DfaSlot::Cold { failed: 0 }),
            initialized: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl Clone for DfaCell {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl DfaCell {
    /// The slot, for tests.
    #[cfg(test)]
    pub(crate) fn slot(&self) -> std::cell::Ref<'_, DfaSlot> {
        self.slot.borrow()
    }

    /// Whether initialization has published a non-cold slot.
    #[inline]
    pub(crate) fn initialized(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::Acquire)
    }

    fn publish_initialized(&self) {
        self.initialized
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Cold admission for one search. Its first nonempty failure stays local:
/// a second failure credits both during the scan; otherwise an isolated
/// failure is credited only when the whole search fails without overflow or quit.
///
/// Threading: this transient state belongs to one search's stack, and is
/// independent of another mutator or a re-entrant search of the same pattern.
#[derive(Default)]
pub(super) enum ColdSearchHeat {
    #[default]
    First,
    Second,
    Repeated,
}

impl ColdSearchHeat {
    #[inline]
    pub(super) fn is_isolated(&self) -> bool {
        matches!(self, Self::Second)
    }

    #[inline]
    pub(super) fn failure_credit(&mut self) -> u32 {
        match self {
            Self::First => {
                *self = Self::Second;
                0
            }
            Self::Second => {
                *self = Self::Repeated;
                2
            }
            Self::Repeated => 1,
        }
    }
}

/// One search's hold on its pattern's DFA slot.
pub(crate) struct DfaLease<'p> {
    slot: RefMut<'p, DfaSlot>,
    mode: DfaMode,
    /// The live DFA's counters when this search began using it; the delta is
    /// added to [`DfaStats`] when the lease is dropped.
    counters_before: Option<DfaCounters>,
    skipped: u64,
    overflow_guarded: u64,
}

impl Drop for DfaLease<'_> {
    fn drop(&mut self) {
        if !stats_on() {
            return;
        }
        let delta = match (&*self.slot, self.counters_before) {
            (DfaSlot::Live(live), Some(before)) => {
                let now = live.dfa.counters;
                DfaCounters {
                    yes: now.yes - before.yes,
                    no: now.no - before.no,
                    unknown: now.unknown - before.unknown,
                    states: now.states - before.states,
                    clears: now.clears - before.clears,
                    slow_transitions: now.slow_transitions - before.slow_transitions,
                    bytes: now.bytes - before.bytes,
                    positional_chars: now.positional_chars - before.positional_chars,
                    plain_runs: now.plain_runs - before.plain_runs,
                    frontier_unknown: now.frontier_unknown - before.frontier_unknown,
                }
            }
            _ => DfaCounters::default(),
        };
        let (skipped, overflow_guarded) = (self.skipped, self.overflow_guarded);
        stat(|s| {
            s.yes += delta.yes;
            s.no += delta.no;
            s.unknown += delta.unknown;
            s.states += delta.states;
            s.clears += delta.clears;
            s.slow_transitions += delta.slow_transitions;
            s.bytes += delta.bytes;
            s.positional_chars += delta.positional_chars;
            s.plain_runs += delta.plain_runs;
            s.frontier_unknown += delta.frontier_unknown;
            s.skipped += skipped;
            s.overflow_guarded += overflow_guarded;
        });
    }
}

/// The matcher alone at one candidate (`re_search`'s own call).
#[inline]
#[allow(clippy::too_many_arguments)]
fn classic_candidate(
    scratch: &mut MatchScratch,
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
    regs: &mut MatchRegisters,
) -> Option<usize> {
    #[cfg(test)]
    super::MATCHER_ENTRY_COUNT.with(|c| c.set(c.get() + 1));
    re_match_candidate_in(scratch, pattern, text, pos, stop, syntax, point, regs)
}

impl<'p> DfaLease<'p> {
    /// The lease for a search of `pattern` whose candidates all stop by
    /// `max_stop`, or `None` when the DFA cannot serve it:
    ///
    /// * the lookup has no table identity to key the classes by;
    /// * the pattern is ineligible, gave up, or is on holiday;
    /// * a re-entrant search holds the slot.
    ///
    /// `syntax-table` properties and the lazy `syntax-propertize` frontier
    /// are the DFA's own business ([`ExistenceDfa::begin_search`]): it reads
    /// syntax where the matcher does, and leaves a candidate that reaches the
    /// frontier to the matcher, which records the read.
    #[inline(never)]
    pub(crate) fn acquire(
        pattern: &'p CompiledPattern,
        syntax: &dyn SyntaxLookup,
        max_stop: usize,
    ) -> Option<Self> {
        let mode = dfa_mode();
        if stats_on() && pattern.uses_syntax {
            if syntax.position_dependent() {
                stat(|s| s.positional += 1);
            }
            if syntax.syntax_read_limit() <= max_stop {
                stat(|s| s.frontier += 1);
            }
        }
        let mut slot = pattern.dfa.slot.try_borrow_mut().ok()?;
        let mut counters_before = None;
        match &mut *slot {
            DfaSlot::Cold { .. } => {}
            DfaSlot::Live(live) => {
                if live.holiday > 0 {
                    live.holiday -= 1;
                    stat(|s| s.holiday_off += 1);
                    return None;
                }
                let context = ClassContext::of_search(pattern, live.dfa.nfa(), syntax)?;
                let resets = live.dfa.classes.resets;
                live.dfa.classes.sync(context);
                if live.dfa.classes.resets != resets {
                    stat(|s| s.context_resets += 1);
                }
                live.dfa.begin_search(pattern, syntax);
                counters_before = Some(live.dfa.counters);
            }
            DfaSlot::Ineligible(_) | DfaSlot::Disabled(_) => return None,
        }
        stat(|s| s.searches += 1);
        Some(Self {
            slot,
            mode,
            counters_before,
            skipped: 0,
            overflow_guarded: 0,
        })
    }

    /// A search admitted one or two classic failures without overflow. Record
    /// them without holding the slot across successful candidates; at the
    /// threshold, build and lease the remaining candidates of this search.
    /// Admission at whole-search failure drops the new lease immediately;
    /// its published slot serves later searches.
    #[cold]
    #[inline(never)]
    pub(crate) fn after_cold_failure(
        pattern: &'p CompiledPattern,
        syntax: &dyn SyntaxLookup,
        max_stop: usize,
        failed_candidates: u32,
    ) -> Option<Self> {
        debug_assert!((1..=2).contains(&failed_candidates));
        {
            let mut slot = pattern.dfa.slot.try_borrow_mut().ok()?;
            let DfaSlot::Cold { failed } = &mut *slot else {
                return None;
            };
            *failed += failed_candidates;
            if *failed < COLD_THRESHOLD {
                return None;
            }
        }
        let mut lease = Self::acquire(pattern, syntax, max_stop)?;
        lease.build(pattern, syntax);
        if matches!(*lease.slot, DfaSlot::Live(_)) {
            Some(lease)
        } else {
            None
        }
    }

    /// Select the inline path once for a search that actually has a lease.
    /// Verify keeps its anchored probe and always runs the matcher too.
    #[inline]
    pub(super) fn inline_first_step_enabled(&self) -> bool {
        self.mode == DfaMode::On && first_step_enabled()
    }

    /// Skip one bounded cached rejection without entering `candidate`.
    /// The caller selected this path with `inline_first_step_enabled`.
    ///
    /// Threading: this search holds its mutator-owned slot's checked borrow.
    /// Context maps have already been synchronized by `acquire` or `build`;
    /// the cache lookup reads no Lisp state and calls no syntax lookup.
    #[inline]
    pub(super) fn try_inline_first_step_skip(
        &mut self,
        pattern: &CompiledPattern,
        text: &[u8],
        pos: usize,
        stop: usize,
        point: usize,
    ) -> bool {
        debug_assert_eq!(self.mode, DfaMode::On);
        let DfaSlot::Live(live) = &mut *self.slot else {
            return false;
        };
        // Leave state clearing, retirement, and a possible overflow to the
        // outlined path. Its original ordering and accounting stay intact.
        if live.dfa.gave_up.is_some()
            || live.dfa.memory > MEMORY_CAP
            || live.overflow_free_span == 0
        {
            return false;
        }
        let Some(consumed) = live.dfa.cached_prefix_rejection(
            pattern,
            text,
            pos,
            stop,
            point,
            live.overflow_free_span,
        ) else {
            return false;
        };
        live.dfa.counters.no += 1;
        live.dfa.counters.bytes += consumed as u64;
        live.note(Exists::No { consumed });
        self.skipped += 1;
        true
    }

    /// Decide one candidate: the DFA's verdict, then the matcher unless the
    /// verdict is a rejection the fail-stack bound allows acting on.
    /// Returns what `re_match_candidate_in` would, with its side effects.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn candidate<const FIRST_STEP: bool>(
        &mut self,
        scratch: &mut MatchScratch,
        pattern: &CompiledPattern,
        text: &[u8],
        pos: usize,
        stop: usize,
        syntax: &dyn SyntaxLookup,
        point: usize,
        regs: &mut MatchRegisters,
    ) -> Option<usize> {
        if self.mode == DfaMode::On
            && let DfaSlot::Live(live) = &mut *self.slot
        {
            let verdict = live
                .dfa
                .anchored_exists_inner::<FIRST_STEP>(pattern, text, pos, stop, point, syntax);
            live.note(verdict);
            // A rejection within the fail-stack bound: the matcher would
            // fail here without a side effect.  (A DFA that gave up answers
            // `Unknown`, so a rejection never comes from one.)
            if let Exists::No { consumed } = verdict
                && consumed < live.overflow_free_span
            {
                self.skipped += 1;
                return None;
            }
            return self.settle(
                verdict, scratch, pattern, text, pos, stop, syntax, point, regs,
            );
        }
        // A pattern whose candidates keep matching never leaves the cold
        // state: it costs one branch more than the matcher alone.
        if let DfaSlot::Cold { failed } = &mut *self.slot {
            let found = classic_candidate(scratch, pattern, text, pos, stop, syntax, point, regs);
            if found.is_none() && !matcher_overflow_pending() {
                *failed += 1;
                if *failed >= COLD_THRESHOLD {
                    self.build(pattern, syntax);
                }
            }
            return found;
        }
        self.candidate_slow(scratch, pattern, text, pos, stop, syntax, point, regs)
    }

    /// A live DFA's verdict that does not skip the candidate: the matcher
    /// runs (after retiring a DFA that gave up).
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn settle(
        &mut self,
        verdict: Exists,
        scratch: &mut MatchScratch,
        pattern: &CompiledPattern,
        text: &[u8],
        pos: usize,
        stop: usize,
        syntax: &dyn SyntaxLookup,
        point: usize,
        regs: &mut MatchRegisters,
    ) -> Option<usize> {
        if let DfaSlot::Live(live) = &*self.slot
            && let Some(why) = live.dfa.gave_up()
        {
            self.retire(why);
        } else if matches!(verdict, Exists::No { .. }) {
            self.overflow_guarded += 1;
        }
        classic_candidate(scratch, pattern, text, pos, stop, syntax, point, regs)
    }

    #[cold]
    fn retire(&mut self, why: DfaGaveUp) {
        stat(|s| s.gave_up += 1);
        self.counters_before = None;
        *self.slot = DfaSlot::Disabled(why);
        tracing::debug!(target: "neovm::regex", ?why, "existence DFA gave up");
    }

    /// Every candidate of a slot that is not live, and of verify mode.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn candidate_slow(
        &mut self,
        scratch: &mut MatchScratch,
        pattern: &CompiledPattern,
        text: &[u8],
        pos: usize,
        stop: usize,
        syntax: &dyn SyntaxLookup,
        point: usize,
        regs: &mut MatchRegisters,
    ) -> Option<usize> {
        let classic = |scratch: &mut MatchScratch, regs: &mut MatchRegisters| {
            classic_candidate(scratch, pattern, text, pos, stop, syntax, point, regs)
        };
        let live = match &mut *self.slot {
            DfaSlot::Live(live) => live,
            DfaSlot::Cold { failed } => {
                let found = classic(scratch, regs);
                if found.is_none() && !matcher_overflow_pending() {
                    *failed += 1;
                    if *failed >= COLD_THRESHOLD {
                        self.build(pattern, syntax);
                    }
                }
                return found;
            }
            DfaSlot::Ineligible(_) | DfaSlot::Disabled(_) => return classic(scratch, regs),
        };
        let verdict = live
            .dfa
            .anchored_exists(pattern, text, pos, stop, point, syntax);
        live.note(verdict);
        if let Some(why) = live.dfa.gave_up() {
            self.retire(why);
            return classic(scratch, regs);
        }
        let overflow_free_span = live.overflow_free_span;
        match (self.mode, verdict) {
            (DfaMode::Verify, verdict) => {
                let found = classic(scratch, regs);
                let overflow = matcher_overflow_pending();
                match verdict {
                    Exists::No { .. } if found.is_some() => {
                        stat(|s| s.verify_bad_no += 1);
                        tracing::error!(
                            target: "neovm::regex",
                            pos,
                            stop,
                            "existence DFA rejected a candidate the matcher matched"
                        );
                        #[cfg(test)]
                        panic!("NEOVM_REGEX_DFA=verify: a rejected candidate matched at {pos}");
                    }
                    Exists::Yes if found.is_none() && !overflow => {
                        stat(|s| s.verify_bad_yes += 1);
                        tracing::error!(
                            target: "neovm::regex",
                            pos,
                            stop,
                            "existence DFA accepted a candidate the matcher failed"
                        );
                        #[cfg(test)]
                        panic!("NEOVM_REGEX_DFA=verify: an accepted candidate failed at {pos}");
                    }
                    _ => {}
                }
                found
            }
            (_, Exists::No { consumed }) => {
                if consumed < overflow_free_span {
                    self.skipped += 1;
                    None
                } else {
                    self.overflow_guarded += 1;
                    classic(scratch, regs)
                }
            }
            _ => classic(scratch, regs),
        }
    }

    #[cold]
    #[inline(never)]
    fn build(&mut self, pattern: &CompiledPattern, syntax: &dyn SyntaxLookup) {
        match Nfa::build(pattern) {
            Ok(nfa) => {
                let mut dfa = ExistenceDfa::new(nfa);
                // A lookup with no table identity cannot key the classes: stay
                // cold, and count the failures toward another try from zero.
                let Some(context) = ClassContext::of_search(pattern, dfa.nfa(), syntax) else {
                    *self.slot = DfaSlot::Cold { failed: 0 };
                    return;
                };
                dfa.classes.sync(context);
                dfa.begin_search(pattern, syntax);
                stat(|s| s.builds += 1);
                self.counters_before = Some(DfaCounters::default());
                *self.slot = DfaSlot::Live(Box::new(LiveDfa::new(dfa)));
            }
            Err(why) => {
                stat(|s| s.ineligible += 1);
                tracing::debug!(target: "neovm::regex", ?why, "no existence DFA");
                *self.slot = DfaSlot::Ineligible(why);
            }
        }
        // Publish only after the new state and its complete DFA are written.
        // A lookup without a class identity leaves the slot cold for a retry.
        pattern.dfa.publish_initialized();
    }
}

/// Build `pattern`'s DFA now, skipping the cold phase, so a short
/// differential case exercises the filter.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn prime(
    pattern: &CompiledPattern,
    syntax: &dyn SyntaxLookup,
) -> Result<(), DfaIneligible> {
    let nfa = Nfa::build(pattern)?;
    let mut dfa = ExistenceDfa::new(nfa);
    if let Some(context) = ClassContext::of_search(pattern, dfa.nfa(), syntax) {
        dfa.classes.sync(context);
    }
    *pattern.dfa.slot.borrow_mut() = DfaSlot::Live(Box::new(LiveDfa::new(dfa)));
    pattern.dfa.publish_initialized();
    Ok(())
}

impl LiveDfa {
    /// Count a verdict toward the adaptive bypass: a window of mostly "yes"
    /// sends the pattern on holiday.
    #[inline]
    fn note(&mut self, verdict: Exists) {
        self.decisions += 1;
        if verdict == Exists::Yes {
            self.yes += 1;
        }
        if self.decisions >= BYPASS_WINDOW {
            if self.yes * 100 > BYPASS_YES_PERCENT * self.decisions {
                self.holiday = BYPASS_HOLIDAY;
            }
            self.decisions = 0;
            self.yes = 0;
        }
    }
}

#[cfg(test)]
#[path = "tests/dfa_test.rs"]
mod tests;
