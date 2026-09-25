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
//! No Lisp `Value` is stored here: only ids, byte tables and identity bits.

// Built up over P3.3 C2-C4 and unused by the search until C5 wires it in.
#![cfg_attr(not(test), allow(dead_code))]

use super::{
    CompiledPattern, LookupClassKey, RegexOp, SyntaxAssertion, SyntaxCacheKey, SyntaxLookup,
    evaluate_syntax_assertion, extract_number, extract_number_u16, match_anychar_at,
    match_categoryspec_at, match_charset_at, match_exactn_char_at, match_syntaxspec_at,
    match_syntaxspecset_at, opcode_len, re_text_char, regex_syntax_char,
};
use crate::emacs_core::emacs_char;
use crate::emacs_core::syntax::SyntaxClass;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

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
                    single(&mut nfa, Predicate::Charset { pc: pc as u32 })?
                }
                RegexOp::SyntaxSpec | RegexOp::NotSyntaxSpec => single(
                    &mut nfa,
                    Predicate::Syntax {
                        class: code[pc + 1],
                        negate: op == RegexOp::NotSyntaxSpec,
                    },
                )?,
                RegexOp::SyntaxSpecSet => single(
                    &mut nfa,
                    Predicate::SyntaxSet {
                        mask: extract_number_u16(code, pc + 1),
                    },
                )?,
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
                | RegexOp::WordEnd => nfa.assertions |= Assertions::WORD,
                RegexOp::SymBeg | RegexOp::SymEnd => nfa.assertions |= Assertions::SYMBOL,
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

    /// The facts of the Emacs character `code` whose first byte in the text
    /// is `first_byte`, under `syntax` (the base table).
    fn of_char(code: u32, first_byte: u8, syntax: &dyn SyntaxLookup, mask: Self) -> Self {
        let mut facts = 0u8;
        if first_byte == b'\n' {
            facts |= Self::NEWLINE.0;
        }
        if mask.0 & (Self::WORD.0 | Self::WORD_OR_SYMBOL.0 | Self::WIDE_WORD.0) != 0 {
            // The matcher's `re_char_and_syntax`: raw bytes read the syntax
            // of their eight-bit character.
            let ch = regex_syntax_char(code);
            let class = syntax.char_syntax(ch);
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

/// A syntax lookup that answers every position from the base table.  Classes
/// are computed through it, so they are functions of the character alone; the
/// DFA runs only where no `syntax-table` property applies (see
/// [`SyntaxLookup::position_dependent`]), where the base table is the answer.
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

    /// The class of the character at `d` of `text`, `len` bytes long.
    pub(crate) fn class_key_at(
        &self,
        pattern: &CompiledPattern,
        text: &[u8],
        d: usize,
        code: u32,
        len: usize,
        base: &BaseTableView<'_>,
        mask: Facts,
    ) -> ClassKey {
        let mut accepts: SmallVec<[u64; 2]> =
            SmallVec::from_elem(0, self.predicates.len().div_ceil(64).max(1));
        for (i, &predicate) in self.predicates.iter().enumerate() {
            if self.predicate_accepts(predicate, pattern, text, d, len, base) {
                accepts[i / 64] |= 1 << (i % 64);
            }
        }
        ClassKey {
            accepts,
            facts: Facts::of_char(code, text[d], base, mask),
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

    /// The facts of the character before `d`, or [`Facts::EDGE`] at 0 (the
    /// matcher's `re_prev_char_start` view of the previous character).
    pub(crate) fn previous_facts(
        &self,
        text: &[u8],
        d: usize,
        target_multibyte: bool,
        base: &BaseTableView<'_>,
    ) -> Facts {
        let Some(start) = super::re_prev_char_start(text, d, target_multibyte) else {
            return Facts::EDGE;
        };
        let (code, _) =
            re_text_char(text, start, target_multibyte).expect("the previous character exists");
        Facts::of_char(code, text[start], base, self.mask)
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
            counters: DfaCounters::default(),
            gave_up: None,
        };
        dfa.trans.resize(dfa.stride(), UNKNOWN);
        dfa
    }

    pub(crate) fn nfa(&self) -> &Nfa {
        &self.nfa
    }

    pub(crate) fn classes_mut(&mut self) -> &mut CharClasses {
        &mut self.classes
    }

    /// Why this DFA stopped deciding for good, if it did.
    pub(crate) fn gave_up(&self) -> Option<DfaGaveUp> {
        self.gave_up
    }

    /// Start a search: per-search limits reset.
    pub(crate) fn begin_search(&mut self) {
        self.clears_this_search = 0;
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
    #[inline(never)]
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
        if self.gave_up.is_some() {
            return Exists::Unknown;
        }
        if self.memory > MEMORY_CAP {
            self.clear_states();
            if self.gave_up.is_some() {
                return Exists::Unknown;
            }
        }
        let verdict = self.run(pattern, text, p, stop, point, syntax);
        match verdict {
            Exists::Yes => self.counters.yes += 1,
            Exists::No { .. } => self.counters.no += 1,
            Exists::Unknown => self.counters.unknown += 1,
        }
        verdict
    }

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
        let base = BaseTableView(syntax);
        let target_multibyte = pattern.target_multibyte;
        let stop = stop.min(text.len());
        if p > stop {
            return Exists::No { consumed: 0 };
        }
        let place = |d: usize| Place {
            text,
            d,
            stop,
            point,
            target_multibyte,
            syntax: &base,
        };
        // `\=` holds at point only: that position's closure is never cached.
        let mut special = if self.nfa.assertions.contains(Assertions::AT_DOT) && point >= p {
            point
        } else {
            usize::MAX
        };
        let prev = Facts(
            self.classes
                .previous_facts(text, p, target_multibyte, &base)
                .bits()
                & self.prev_mask.bits(),
        );
        let mut row = self.start_row(prev);
        let mut d = p;
        let mut polled = 0usize;
        loop {
            // The cached loop: single-byte characters with a known class and
            // a cached live transition.
            let limit = stop.min(special);
            let start = d;
            {
                let byte_class = &self.classes.byte_class;
                let trans = &self.trans;
                let stride = 1u32 << self.stride_shift;
                while d < limit {
                    let class = byte_class[text[d] as usize];
                    if class == UNKNOWN_CLASS {
                        break;
                    }
                    let next = trans[row as usize + class as usize];
                    if next < stride {
                        break;
                    }
                    row = next;
                    d += 1;
                }
            }
            polled += d - start;
            if d >= stop {
                self.counters.bytes += (d - p) as u64;
                return if self.accepts_at(row, &place(d)) {
                    Exists::Yes
                } else {
                    Exists::No { consumed: d - p }
                };
            }
            if polled >= QUIT_POLL_BYTES {
                polled = 0;
                if crate::emacs_core::eval::tls_quit_pending() {
                    return Exists::Unknown;
                }
            }
            // One character the cached loop could not take.
            let row_index = self.index_of(row);
            let (class, len) = match self.classes.class_at(&self.nfa, pattern, text, d, &base) {
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

#[cfg(test)]
#[path = "tests/dfa.rs"]
mod tests;
