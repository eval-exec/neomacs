//! Per-opcode callback order/cache identity: public GNU parity is tested separately.

use super::*;
use std::cell::Cell;

use neovm_value::tagged::SymbolMode;
use regex_automata::{MatchKind, Span};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use neovm_edit::buffer::syntax_class::SyntaxClass;
use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::regex_emacs::dfa;
use neovm_text::emacs_core::regex_emacs::*;
use neovm_value::value::Value;

use neovm_text::emacs_core::syntax::{
    StringSyntaxPositionError, StringSyntaxPositionSource, SyntaxProperties, SyntaxTable,
};

use neovm_value::heap_types::{GnuDeclaredCharacterCount, GnuStringCharPos, LispString};

use std::cell::RefCell;

#[test]
fn ordinary_boundary_pair_keeps_physical_lookup_order_and_short_circuit() {
    let lookup = PhysicalReads::default();
    let classes = lookup.boundary_syntax_at(request(SyntaxAssertion::WordBoundary, 'c', 'd'));
    assert_eq!(*lookup.0.borrow(), [2, 3]);
    assert_eq!(
        classes.previous(),
        Some((
            neovm_value::emacs_char::EmacsChar::from_code_unchecked('c' as u32),
            SyntaxClass::Word
        ))
    );
    assert_eq!(
        classes.current(),
        Some((
            neovm_value::emacs_char::EmacsChar::from_code_unchecked('d' as u32),
            SyntaxClass::Word
        ))
    );
    lookup.0.borrow_mut().clear();
    lookup.boundary_syntax_at(request(SyntaxAssertion::WordBeginning, 'c', '!'));
    assert_eq!(*lookup.0.borrow(), [3]);
    lookup.0.borrow_mut().clear();
    lookup.boundary_syntax_at(request(SyntaxAssertion::WordEnd, '!', 'd'));
    assert_eq!(*lookup.0.borrow(), [2]);
}

#[test]
fn ordinary_symbol_boundaries_follow_gnu_lookup_order_and_short_circuit() {
    // GNU regex-emacs.c:5111-5130 reads current first for symbeg,
    // while :5147-5170 reads previous first for symend. A non-symbol
    // constituent fails before the other neighbour's syntax is read.
    let lookup = PhysicalReads::default();
    lookup.boundary_syntax_at(request(SyntaxAssertion::SymbolBeginning, 'c', '!'));
    assert_eq!(*lookup.0.borrow(), [3]);
    lookup.0.borrow_mut().clear();
    lookup.boundary_syntax_at(request(SyntaxAssertion::SymbolBeginning, 'c', 'd'));
    assert_eq!(*lookup.0.borrow(), [3, 2]);
    lookup.0.borrow_mut().clear();
    lookup.boundary_syntax_at(request(SyntaxAssertion::SymbolBeginning, '!', '_'));
    assert_eq!(*lookup.0.borrow(), [3, 2]);
    lookup.0.borrow_mut().clear();
    lookup.boundary_syntax_at(request(SyntaxAssertion::SymbolEnd, '!', 'd'));
    assert_eq!(*lookup.0.borrow(), [2]);
    lookup.0.borrow_mut().clear();
    lookup.boundary_syntax_at(request(SyntaxAssertion::SymbolEnd, '_', '!'));
    assert_eq!(*lookup.0.borrow(), [2, 3]);
    lookup.0.borrow_mut().clear();
    let at_beginning = neovm_text::test_support::authority::boundary_syntax_request(
        SyntaxAssertion::SymbolEnd,
        neovm_edit::buffer::EmacsBytePos::new(3),
        None,
        Some(
            neovm_text::test_support::authority::boundary_syntax_character(
                neovm_value::emacs_char::EmacsChar::from_code_unchecked('d' as u32),
                neovm_edit::buffer::EmacsBytePos::new(3),
            ),
        ),
    );
    lookup.boundary_syntax_at(at_beginning);
    assert!(lookup.0.borrow().is_empty());
}

#[test]
fn supplied_boundary_converts_current_once_before_previous_logical_lookup() {
    let context = crate::test_utils::new_context();
    let source = supplied(b"abcde");
    let base = BufferSyntaxLookup {
        syntax_table: SyntaxTable::for_buffer(
            context.edit.buffers.current_buffer().expect("test buffer"),
        ),
        category_table: None,
        word_boundary: WordBoundaryLookup::default(),
    };
    let result = neovm_text::emacs_core::regex::with_string_syntax_lookup(
        Some(base),
        StringSyntaxPositionSource::try_from(&source).expect("validated test source"),
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        |lookup| {
            neovm_value::string_pos_cache::reset_string_pos_cache();
            lookup.boundary_syntax_at(request(SyntaxAssertion::WordBoundary, 'c', 'd'));
        },
    );
    assert_eq!(
        result,
        Err(StringSyntaxPositionError::OutOfRange {
            position: GnuStringCharPos::new(-1)
        })
    );
    // A logical previous read leaves the actual byte3/char0 cache anchor alone.
    assert_eq!(
        neovm_value::string_pos_cache::string_byte_to_gnu_char(
            source,
            source.as_lisp_string().expect("test payload"),
            2
        )
        .get(),
        -1
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

#[test]
fn supplied_symbol_beginning_skips_invalid_previous_after_nonword_current() {
    let context = crate::test_utils::new_context();
    let source = supplied(b"abc!e");
    let base = BufferSyntaxLookup {
        syntax_table: SyntaxTable::for_buffer(
            context.edit.buffers.current_buffer().expect("test buffer"),
        ),
        category_table: None,
        word_boundary: WordBoundaryLookup::default(),
    };
    let classes = neovm_text::emacs_core::regex::with_string_syntax_lookup(
        Some(base),
        StringSyntaxPositionSource::try_from(&source).expect("validated test source"),
        SyntaxProperties::for_scan(true, context.variables(), &context.edit.buffers),
        |lookup| {
            neovm_value::string_pos_cache::reset_string_pos_cache();
            lookup.boundary_syntax_at(request(SyntaxAssertion::SymbolBeginning, 'c', '!'))
        },
    )
    .expect("nonword current must skip the invalid previous position");
    assert_eq!(classes.previous(), None);
    assert_eq!(
        classes.current(),
        Some((
            neovm_value::emacs_char::EmacsChar::from_code_unchecked('!' as u32),
            SyntaxClass::Punctuation
        ))
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

#[test]
fn invocation_boundary_plan_selects_once_across_search_candidates_and_engines() {
    // The probe's callback has physical semantics, but a supplied-mode probe
    // records that the selected route continues through the existing callback.
    // Real supplied coordinates/cache/errors are protected by the tests above.
    for mode in [BoundarySyntaxMode::Physical, BoundarySyntaxMode::Supplied] {
        let pattern = regex_compile("\\_<\\w\\_>", false, false).unwrap();
        let text = b"aa bb q";
        for dfa_mode in [dfa::DfaMode::Off, dfa::DfaMode::Verify] {
            let lookup = PlanReads::new(mode);
            dfa::prime(&pattern, &lookup).unwrap();
            let found = dfa::with_dfa_mode(dfa_mode, || {
                re_search(&pattern, text, 0, text.len() as isize, &lookup, 0)
            });
            assert_eq!(found.map(|(start, _)| start), Some(6));
            assert_eq!(lookup.selections.get(), 1, "{mode:?} {dfa_mode:?}");
            match mode {
                BoundarySyntaxMode::Physical => assert_eq!(lookup.callbacks.get(), 0),
                BoundarySyntaxMode::Supplied => assert!(lookup.callbacks.get() > 1),
            }
        }
        let lookup = PlanReads::new(mode);
        assert!(pike_match(&pattern, text, 6, text.len(), &lookup, 0).is_some());
        assert_eq!(lookup.selections.get(), 1);
        match mode {
            BoundarySyntaxMode::Physical => assert_eq!(lookup.callbacks.get(), 0),
            BoundarySyntaxMode::Supplied => assert_eq!(lookup.callbacks.get(), 2),
        }
        let lookup = PlanReads::new(mode);
        assert!(re_match(&pattern, text, 6, text.len(), &lookup, 0).is_some());
        assert_eq!(lookup.selections.get(), 1);
        match mode {
            BoundarySyntaxMode::Physical => assert_eq!(lookup.callbacks.get(), 0),
            BoundarySyntaxMode::Supplied => assert_eq!(lookup.callbacks.get(), 2),
        }
    }
}

#[test]
fn syntax_free_patterns_do_not_query_boundary_coordinates() {
    let pattern = regex_compile("q", false, false).unwrap();
    assert!(!pattern.uses_syntax);
    let lookup = PlanReads::new(BoundarySyntaxMode::Supplied);
    assert!(re_search(&pattern, b"aaq", 0, 3, &lookup, 0).is_some());
    assert!(re_match(&pattern, b"q", 0, 1, &lookup, 0).is_some());
    assert!(pike_match(&pattern, b"q", 0, 1, &lookup, 0).is_some());
    assert_eq!(lookup.selections.get(), 0);
    assert_eq!(lookup.callbacks.get(), 0);
}

#[test]
fn physical_match_plan_keeps_symbol_lookup_order_and_short_circuit() {
    let lookup = PhysicalReads::default();
    let plan = MatchSyntax::new(&lookup);
    re_boundary_syntax(SyntaxAssertion::SymbolBeginning, b"c!", 1, true, &plan);
    assert_eq!(*lookup.0.borrow(), [1]);
    lookup.0.borrow_mut().clear();
    re_boundary_syntax(SyntaxAssertion::SymbolBeginning, b"cd", 1, true, &plan);
    assert_eq!(*lookup.0.borrow(), [1, 0]);
    lookup.0.borrow_mut().clear();
    re_boundary_syntax(SyntaxAssertion::SymbolEnd, b"!c", 1, true, &plan);
    assert_eq!(*lookup.0.borrow(), [0]);
    lookup.0.borrow_mut().clear();
    re_boundary_syntax(SyntaxAssertion::SymbolEnd, b"_!", 1, true, &plan);
    assert_eq!(*lookup.0.borrow(), [0, 1]);
}

fn request(kind: SyntaxAssertion, before: char, after: char) -> BoundarySyntaxRequest {
    neovm_text::test_support::authority::boundary_syntax_request(
        kind,
        neovm_edit::buffer::EmacsBytePos::new(3),
        Some(
            neovm_text::test_support::authority::boundary_syntax_character(
                neovm_value::emacs_char::EmacsChar::from_code_unchecked(before as u32),
                neovm_edit::buffer::EmacsBytePos::new(2),
            ),
        ),
        Some(
            neovm_text::test_support::authority::boundary_syntax_character(
                neovm_value::emacs_char::EmacsChar::from_code_unchecked(after as u32),
                neovm_edit::buffer::EmacsBytePos::new(3),
            ),
        ),
    )
}

#[derive(Default)]
struct PhysicalReads(RefCell<Vec<usize>>);

impl SyntaxLookup for PhysicalReads {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        match c {
            '!' => SyntaxClass::Punctuation,
            '_' => SyntaxClass::Symbol,
            _ => SyntaxClass::Word,
        }
    }
    fn char_syntax_at(&self, c: char, pos: usize) -> SyntaxClass {
        self.0.borrow_mut().push(pos);
        self.char_syntax(c)
    }
    fn char_has_category(&self, _: char, _: u8) -> bool {
        false
    }
    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Standard
    }
}

fn supplied(bytes: &[u8]) -> Value {
    Value::heap_string(LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(2).expect("bounded test metadata"),
    ))
}

struct PlanReads {
    mode: BoundarySyntaxMode,
    selections: std::cell::Cell<usize>,
    callbacks: std::cell::Cell<usize>,
}

impl PlanReads {
    fn new(mode: BoundarySyntaxMode) -> Self {
        Self {
            mode,
            selections: std::cell::Cell::new(0),
            callbacks: std::cell::Cell::new(0),
        }
    }
}

impl SyntaxLookup for PlanReads {
    fn boundary_syntax_mode(&self) -> BoundarySyntaxMode {
        self.selections.set(self.selections.get() + 1);
        self.mode
    }

    fn boundary_syntax_at(&self, request: BoundarySyntaxRequest) -> BoundarySyntaxClasses {
        self.callbacks.set(self.callbacks.get() + 1);
        request.physical_lookup(self)
    }

    fn char_syntax(&self, ch: char) -> SyntaxClass {
        DefaultSyntaxLookup.char_syntax(ch)
    }

    fn char_has_category(&self, ch: char, category: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(ch, category)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Standard
    }

    fn class_cache_key(&self) -> Option<LookupClassKey> {
        Some(LookupClassKey::Standard)
    }
}
