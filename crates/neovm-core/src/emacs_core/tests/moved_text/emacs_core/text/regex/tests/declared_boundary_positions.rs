//! Per-opcode callback order/cache identity: public GNU parity is tested separately.

use neovm_text::emacs_core::regex_emacs::*;

use neovm_text::emacs_core::syntax::{
    StringSyntaxPositionError, StringSyntaxPositionSource, SyntaxProperties, SyntaxTable,
};

use neovm_value::heap_types::{GnuDeclaredCharacterCount, GnuStringCharPos, LispString};

use std::cell::RefCell;

#[test]
fn supplied_boundary_converts_current_once_before_previous_logical_lookup() {
    let context = Context::new();
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
    let context = Context::new();
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

fn request(kind: SyntaxAssertion, before: char, after: char) -> BoundarySyntaxRequest {
    BoundarySyntaxRequest {
        kind,
        current_byte: neovm_edit::buffer::EmacsBytePos::new(3),
        previous: Some(BoundarySyntaxCharacter {
            character: neovm_value::emacs_char::EmacsChar::from_code_unchecked(before as u32),
            byte_position: neovm_edit::buffer::EmacsBytePos::new(2),
        }),
        current: Some(BoundarySyntaxCharacter {
            character: neovm_value::emacs_char::EmacsChar::from_code_unchecked(after as u32),
            byte_position: neovm_edit::buffer::EmacsBytePos::new(3),
        }),
    }
}

fn supplied(bytes: &[u8]) -> Value {
    Value::heap_string(LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(2).expect("bounded test metadata"),
    ))
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

struct PlanReads {
    mode: BoundarySyntaxMode,
    selections: std::cell::Cell<usize>,
    callbacks: std::cell::Cell<usize>,
}

#[derive(Default)]
struct PhysicalReads(RefCell<Vec<usize>>);
