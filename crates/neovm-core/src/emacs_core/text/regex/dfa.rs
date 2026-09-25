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

#[cfg(test)]
#[path = "tests/dfa.rs"]
mod tests;
