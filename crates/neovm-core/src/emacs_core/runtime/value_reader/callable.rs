//! Lazy GNU READCHARFUN input (`src/lread.c:323-327,486-496`).
//!
//! Characters are fetched lazily. GNU's token boundaries push back lookahead
//! immediately and evict it, so subsequent reads call Lisp again. The parser
//! runs once; callback mutations and signals occur at their GNU boundary.
//! State belongs exclusively to the calling mutator. RefCell borrows never
//! span a recursive parser access, and no Lisp state is shared across threads.

use super::{
    ReadLabels, Reader, ReaderEnvironment, ReaderShorthands, ReaderSource, ReaderSourceSemantics,
    SymbolPositionMode,
};
use crate::buffer::EmacsBytePos;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    Context, push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::value::{Value, ValueKind};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;

/// Validated GNU character: a fixnum in 0..=MAX_CHAR. Noncharacters are EOF.
#[derive(Clone, Copy, Debug)]
struct InputCharacter(u32);

#[derive(Debug, thiserror::Error)]
enum NonCharacter {
    #[error("READCHARFUN returned a non-character")]
    ReturnedNonCharacter,
}

impl TryFrom<Value> for InputCharacter {
    type Error = NonCharacter;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value.kind() {
            ValueKind::Fixnum(ch) if (0..=i64::from(super::emacs_char::MAX_CHAR)).contains(&ch) => {
                Ok(Self(ch as u32))
            }
            _ => Err(NonCharacter::ReturnedNonCharacter),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BufferedCharacter {
    start: EmacsBytePos,
    character: InputCharacter,
}

#[derive(Clone, Copy, Debug)]
struct SourceStep {
    character: InputCharacter,
    end: EmacsBytePos,
}

#[derive(Debug)]
enum InputState {
    Reading,
    End,
    Failed(Flow),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum CallbackFailed {
    #[error("READCHARFUN callback signalled")]
    Signalled,
}

pub(super) struct CallableSource<'a> {
    ctx: RefCell<&'a mut Context>,
    function: Value,
    characters: RefCell<Vec<BufferedCharacter>>,
    bytes: RefCell<Vec<u8>>,
    steps: RefCell<Vec<SourceStep>>,
    state: RefCell<InputState>,
}

impl std::fmt::Debug for CallableSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallableSource")
            .field("function", &self.function)
            .field("state", &self.state.borrow())
            .field("buffered_characters", &self.characters.borrow().len())
            .finish_non_exhaustive()
    }
}

impl CallableSource<'_> {
    pub(super) fn symbol_mode(&self) -> crate::tagged::SymbolMode {
        self.ctx.borrow().symbol_mode()
    }

    pub(super) fn variable(&self, name: &str) -> Option<Value> {
        self.ctx
            .borrow()
            .visible_runtime_variable_value_by_id(super::intern(name))
            .ok()
            .flatten()
    }

    pub(super) fn code_and_next(&self, position: EmacsBytePos) -> Option<(u32, EmacsBytePos)> {
        let pos = position.get();
        if matches!(*self.state.borrow(), InputState::Failed(_)) {
            return None;
        }
        if pos == self.bytes.borrow().len() && matches!(*self.state.borrow(), InputState::Reading) {
            match self
                .ctx
                .borrow_mut()
                .funcall_general(self.function, Vec::new())
            {
                Ok(value) => match InputCharacter::try_from(value) {
                    Ok(character) => {
                        let mut encoded = [0; super::emacs_char::MAX_MULTIBYTE_LENGTH];
                        let len = super::emacs_char::char_string(character.0, &mut encoded);
                        self.bytes.borrow_mut().extend_from_slice(&encoded[..len]);
                        self.steps.borrow_mut().extend(std::iter::repeat_n(
                            SourceStep {
                                character,
                                end: EmacsBytePos::new(pos + len),
                            },
                            len,
                        ));
                        self.characters.borrow_mut().push(BufferedCharacter {
                            start: EmacsBytePos::new(pos),
                            character,
                        });
                    }
                    Err(NonCharacter::ReturnedNonCharacter) => {
                        *self.state.borrow_mut() = InputState::End
                    }
                },
                Err(flow) => {
                    *self.state.borrow_mut() = InputState::Failed(flow);
                }
            }
        }
        self.steps
            .borrow()
            .get(pos)
            .map(|step| (step.character.0, step.end))
    }

    pub(super) fn character_count(&self, start: EmacsBytePos, end: EmacsBytePos) -> usize {
        let characters = self.characters.borrow();
        characters.partition_point(|ch| ch.start < end)
            - characters.partition_point(|ch| ch.start < start)
    }

    pub(super) fn slice(&self, start: EmacsBytePos, end: EmacsBytePos) -> String {
        super::emacs_char::to_utf8_lossy(&self.bytes.borrow()[start.get()..end.get()])
    }

    /// GNU unreadchar is observable even when the next read consumes the
    /// same delimiter. Remove cached lookahead before calling Lisp, so a
    /// callback that changes the returned character is respected.
    pub(super) fn unread(&self, position: EmacsBytePos) -> Result<(), CallbackFailed> {
        let pos = position.get();
        if matches!(*self.state.borrow(), InputState::Failed(_)) {
            return Err(CallbackFailed::Signalled);
        }
        // GNU unreadchar(-1) decrements the offset but makes no callback.
        // A READCHARFUN can return nil temporarily, then yield more input:
        // no EOF state survives the parser's unread boundary.
        *self.state.borrow_mut() = InputState::Reading;
        let tail = {
            let mut characters = self.characters.borrow_mut();
            let boundary = characters.partition_point(|ch| ch.start.get() < pos);
            let tail: smallvec::SmallVec<[BufferedCharacter; 2]> =
                characters.drain(boundary..).collect();
            tail
        };
        if tail.is_empty() {
            return Ok(());
        }
        self.bytes.borrow_mut().truncate(pos);
        self.steps.borrow_mut().truncate(pos);
        *self.state.borrow_mut() = InputState::Reading;
        for ch in tail.iter().rev() {
            if let Err(flow) = self.ctx.borrow_mut().funcall_general(
                self.function,
                vec![Value::fixnum(i64::from(ch.character.0))],
            ) {
                *self.state.borrow_mut() = InputState::Failed(flow);
                return Err(CallbackFailed::Signalled);
            }
        }
        Ok(())
    }

    fn finish(&self, pos: EmacsBytePos) -> Result<(), Flow> {
        let _ = self.unread(pos);
        match self.state.replace(InputState::End) {
            InputState::Failed(flow) => Err(flow),
            InputState::Reading | InputState::End => Ok(()),
        }
    }
}

/// A mutator-local root scope restores scratch roots on every exit path.
/// Drop never calls Lisp, blocks, or panics.
#[must_use = "dropping the scope restores the mutator scratch roots"]
#[derive(Debug)]
pub(super) struct RootScope {
    saved: usize,
    confined: PhantomData<*const ()>,
}
impl RootScope {
    pub(super) fn new() -> Self {
        let saved = save_scratch_gc_roots();
        Self {
            saved,
            confined: PhantomData,
        }
    }
}
impl Drop for RootScope {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.saved);
    }
}

pub(crate) fn read_from_callable(
    ctx: &mut Context,
    function: Value,
    symbol_position_mode: SymbolPositionMode,
) -> Result<Value, Flow> {
    let _roots = RootScope::new();
    push_scratch_gc_root(function);
    let source = CallableSource {
        ctx: RefCell::new(ctx),
        function,
        characters: RefCell::new(Vec::new()),
        bytes: RefCell::new(Vec::new()),
        steps: RefCell::new(Vec::new()),
        state: RefCell::new(InputState::Reading),
    };
    let mut reader = Reader {
        source: ReaderSource::Callable(&source),
        source_semantics: ReaderSourceSemantics::MultibyteCharacters,
        pos: 0,
        limit: usize::MAX,
        source_bytes: None,
        character_cursor: super::SourceCharacterCursor::Bytes,
        original_string: super::OriginalStringCacheBinding::Uncached,
        dot_unread: Cell::new(None),
        read_labels: ReadLabels::for_source(&ReaderSource::Callable(&source)),
        symbol_position_mode,
        // Callable readers query the live policy at each semantic boundary.
        symbol_mode: crate::tagged::SymbolMode::Bare,
        readchar_offset_origin: super::ReadcharOffsetOrigin::RelativeToSourceByte(0),
        environment: ReaderEnvironment::Callable(&source),
        lazy_doc_policy: Cell::new(super::lazy_doc::LazyDocPolicy::Unknown),
        file_origin: None,
        saved_doc_blocks: None,
        shorthands: ReaderShorthands::LiveCallable(&source),
        step_cache: Cell::new(None),
        dot_syntax: Cell::new(None),
    };
    let parsed = if reader.skip_ws_and_comments() {
        reader
            .read_form()
            .map_err(crate::emacs_core::reader::signal_reader_error_from_string)
    } else {
        Err(crate::emacs_core::reader::ReadSourceObject::Anonymous.end_of_file_error())
    };
    if let Ok(value) = parsed {
        push_scratch_gc_root(value);
    }
    source.finish(EmacsBytePos::new(reader.pos))?;
    parsed
}
