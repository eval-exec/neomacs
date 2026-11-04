//! GNU logical syntax-run state for a supplied-count string.
//!
//! GNU syntax.c:277-311 and328-476 keep signed character bounds and two
//! interval cursors. Byte runs are insufficient when declared SCHARS differs
//! from decoding: cached byte→character coordinates need not be monotonic.
//! Ordinary computed-count strings keep the existing byte-run fast path.
//! Threading: one matcher invocation owns this state; no TLS is introduced.

use std::cell::Cell;

use neovm_edit::buffer::{CharLen, CharPos0, ObjectIntervalPlistRun, TextPropertyTable};
use neovm_edit::emacs_core::textprop::CharPropertyResolver;
use neovm_value::heap_types::GnuStringCharPos;
use neovm_value::value::Value;

use super::{StringSyntaxPositionError, StringSyntaxPositionSource, StringSyntaxPositionUpdate};

// GNU's look-ahead budget (syntax.c:207), not a Rust enum-table length.
const GNU_INTERVALS_AT_ONCE: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IntervalIndex(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IntervalCursor {
    Uninitialized,
    At(IntervalIndex),
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeInvalidation {
    Preserve,
    IfPropertyChanges,
}

/// Exclusive logical run bounds; i128 also represents SCHARS+1 when an
/// internal Rust test supplies the largest representable GNU count. These
/// coordinates never index physical bytes or escape into Lisp match data.
#[derive(Clone, Copy, Debug)]
struct LogicalCharRun {
    begin: i128,
    end: i128,
}

#[derive(Clone, Copy, Debug)]
struct LogicalState {
    run: LogicalCharRun,
    backward: IntervalCursor,
    forward: IntervalCursor,
    old_property: Value,
    current_property: Value,
}

pub(super) struct DeclaredStringSyntaxState<'a> {
    source: StringSyntaxPositionSource<'a>,
    resolver: CharPropertyResolver<'a>,
    intervals: Vec<ObjectIntervalPlistRun>,
    state: Cell<LogicalState>,
    _mutator: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl std::fmt::Debug for DeclaredStringSyntaxState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclaredStringSyntaxState")
            .field("source", &self.source)
            .field("interval_count", &self.intervals.len())
            .field("state", &self.state.get())
            .finish()
    }
}

impl<'a> DeclaredStringSyntaxState<'a> {
    pub(super) fn new(
        source: StringSyntaxPositionSource<'a>,
        resolver: CharPropertyResolver<'a>,
        intervals: Option<&TextPropertyTable>,
    ) -> Self {
        let runs = intervals
            .map(|table| {
                table
                    .object_interval_plist_runs_for_char_len(CharLen::new(source.string().schars()))
            })
            .unwrap_or_default();
        Self {
            source,
            resolver,
            intervals: runs,
            _mutator: std::marker::PhantomData,
            state: Cell::new(LogicalState {
                run: LogicalCharRun { begin: 0, end: 0 },
                backward: IntervalCursor::Uninitialized,
                forward: IntervalCursor::Uninitialized,
                old_property: Value::NIL,
                current_property: Value::NIL,
            }),
        }
    }

    /// Every syntax read first updates the same Heap object's GNU position
    /// cache, even when the logical syntax run already covers its coordinate.
    pub(super) fn lookup(
        &self,
        byte_pos: usize,
    ) -> Result<Option<Value>, StringSyntaxPositionError> {
        let position = self.character_position(byte_pos);
        self.lookup_character(position, StringSyntaxPositionUpdate::Both)
    }

    pub(super) fn character_position(&self, byte_pos: usize) -> GnuStringCharPos {
        self.source.byte_to_gnu_char_pos(byte_pos)
    }

    /// Adjacent logical queries must not perform another byte-cache update.
    pub(super) fn lookup_character(
        &self,
        position: GnuStringCharPos,
        update: StringSyntaxPositionUpdate,
    ) -> Result<Option<Value>, StringSyntaxPositionError> {
        let mut state = self.state.get();
        let logical = i128::from(position.get());
        // GNU UPDATE_SYNTAX_TABLE runs backward, THEN forward, after any
        // changed bounds (syntax.h:194-197). Preserve that order.
        if matches!(
            update,
            StringSyntaxPositionUpdate::Both | StringSyntaxPositionUpdate::Backward
        ) && logical < state.run.begin
        {
            self.update(&mut state, position, Direction::Backward)?;
        }
        if matches!(
            update,
            StringSyntaxPositionUpdate::Both | StringSyntaxPositionUpdate::Forward
        ) && logical >= state.run.end
        {
            self.update(&mut state, position, Direction::Forward)?;
        }
        self.state.set(state);
        Ok((!state.current_property.is_nil()).then_some(state.current_property))
    }

    fn interval_at(
        &self,
        position: GnuStringCharPos,
    ) -> Result<IntervalCursor, StringSyntaxPositionError> {
        let character = CharPos0::try_from(position)
            .ok()
            .filter(|p| p.get() <= self.source.string().schars())
            .ok_or(StringSyntaxPositionError::OutOfRange { position })?;
        if self.intervals.is_empty() || self.source.string().schars() == 0 {
            return Ok(IntervalCursor::End);
        }
        if character.get() == self.source.string().schars() {
            return Ok(IntervalCursor::At(IntervalIndex(self.intervals.len() - 1)));
        }
        Ok(self
            .intervals
            .iter()
            .position(|run| run.start() <= character && character < run.end())
            .map(|index| IntervalCursor::At(IntervalIndex(index)))
            .unwrap_or(IntervalCursor::End))
    }

    fn resolve(&self, run: &ObjectIntervalPlistRun) -> Value {
        self.resolver
            .resolve_interval_plist(run.plist())
            .unwrap_or(Value::NIL)
    }

    fn next(&self, index: IntervalIndex, direction: Direction) -> IntervalCursor {
        let next = match direction {
            Direction::Backward => index.0.checked_sub(1),
            Direction::Forward => index
                .0
                .checked_add(1)
                .filter(|next| *next < self.intervals.len()),
        };
        next.map(|index| IntervalCursor::At(IntervalIndex(index)))
            .unwrap_or(IntervalCursor::End)
    }

    fn set_run_start(state: &mut LogicalState, run: &ObjectIntervalPlistRun) {
        state.run.begin = run.start().get() as i128;
    }

    fn set_run_end(state: &mut LogicalState, run: &ObjectIntervalPlistRun) {
        state.run.end = run.end().get() as i128;
    }

    fn update(
        &self,
        state: &mut LogicalState,
        position: GnuStringCharPos,
        direction: Direction,
    ) -> Result<(), StringSyntaxPositionError> {
        let mut invalidation = RangeInvalidation::IfPropertyChanges;
        let mut cursor;
        if state.backward == IntervalCursor::Uninitialized {
            // textprop.c:523-550 validates before checking whether intervals
            // exist. A first negative syntax coordinate signals that value.
            cursor = self.interval_at(position)?;
            state.backward = cursor;
            state.forward = cursor;
            invalidation = RangeInvalidation::Preserve;
            match cursor {
                IntervalCursor::Uninitialized => {
                    return Err(StringSyntaxPositionError::ToEndIntervals);
                }
                IntervalCursor::End => {
                    state.run = LogicalCharRun {
                        begin: 0,
                        end: self.source.string().schars() as i128 + 1,
                    };
                    return Ok(());
                }
                IntervalCursor::At(index) => {
                    let Some(run) = self.intervals.get(index.0) else {
                        return Err(StringSyntaxPositionError::ToEndIntervals);
                    };
                    Self::set_run_start(state, run);
                    Self::set_run_end(state, run);
                }
            }
        } else {
            cursor = match direction {
                Direction::Backward => state.backward,
                Direction::Forward => state.forward,
            };
            let index = match cursor {
                IntervalCursor::At(index) => index,
                IntervalCursor::End | IntervalCursor::Uninitialized => {
                    return Err(StringSyntaxPositionError::ToEndIntervals);
                }
            };
            let Some(run) = self.intervals.get(index.0) else {
                return Err(StringSyntaxPositionError::ToEndIntervals);
            };
            if i128::from(position.get()) < run.start().get() as i128 {
                if direction == Direction::Forward {
                    return Err(StringSyntaxPositionError::ForwardIntervals);
                }
                cursor = self.interval_at(position)?;
                if let IntervalCursor::At(updated) = cursor {
                    let Some(run) = self.intervals.get(updated.0) else {
                        return Err(StringSyntaxPositionError::ToEndIntervals);
                    };
                    if run.end().get() as i128 != state.run.begin {
                        invalidation = RangeInvalidation::Preserve;
                        state.forward = cursor;
                        Self::set_run_end(state, run);
                    }
                }
            } else if i128::from(position.get()) >= run.end().get() as i128 {
                if direction == Direction::Backward {
                    return Err(StringSyntaxPositionError::BackwardIntervals);
                }
                cursor = self.interval_at(position)?;
                if let IntervalCursor::At(updated) = cursor {
                    let Some(run) = self.intervals.get(updated.0) else {
                        return Err(StringSyntaxPositionError::ToEndIntervals);
                    };
                    if run.start().get() as i128 != state.run.end {
                        invalidation = RangeInvalidation::Preserve;
                        state.backward = cursor;
                        Self::set_run_start(state, run);
                    }
                }
            }
        }

        let index = match cursor {
            IntervalCursor::At(index) => index,
            IntervalCursor::End | IntervalCursor::Uninitialized => {
                return Err(StringSyntaxPositionError::ToEndIntervals);
            }
        };
        let Some(current) = self.intervals.get(index.0) else {
            return Err(StringSyntaxPositionError::ToEndIntervals);
        };
        let property = self.resolve(current);
        if invalidation == RangeInvalidation::IfPropertyChanges
            && property.bits() != state.old_property.bits()
        {
            match direction {
                Direction::Backward => {
                    state.forward = cursor;
                    Self::set_run_end(state, current);
                }
                Direction::Forward => {
                    state.backward = cursor;
                    Self::set_run_start(state, current);
                }
            }
        }
        if property.bits() != state.old_property.bits() {
            state.current_property = property;
            state.old_property = property;
        }

        let mut count = 0;
        loop {
            let index = match cursor {
                IntervalCursor::At(index) => index,
                IntervalCursor::End => break,
                IntervalCursor::Uninitialized => {
                    return Err(StringSyntaxPositionError::ToEndIntervals);
                }
            };
            let Some(run) = self.intervals.get(index.0) else {
                return Err(StringSyntaxPositionError::ToEndIntervals);
            };
            if count > 0 && self.resolve(run).bits() != property.bits() {
                match direction {
                    Direction::Backward => {
                        state.run.begin = run.end().get() as i128;
                        state.backward = cursor;
                    }
                    Direction::Forward => {
                        state.run.end = run.start().get() as i128;
                        state.forward = cursor;
                    }
                }
                return Ok(());
            } else if count == GNU_INTERVALS_AT_ONCE {
                match direction {
                    Direction::Backward => {
                        Self::set_run_start(state, run);
                        state.backward = cursor;
                    }
                    Direction::Forward => {
                        let extra = match self.next(index, Direction::Forward) {
                            IntervalCursor::At(_) => 0,
                            IntervalCursor::End => 1,
                            IntervalCursor::Uninitialized => {
                                return Err(StringSyntaxPositionError::ToEndIntervals);
                            }
                        };
                        state.run.end = run.end().get() as i128 + extra;
                        state.forward = cursor;
                    }
                }
                return Ok(());
            }
            count += 1;
            cursor = self.next(index, direction);
        }
        match direction {
            Direction::Backward => state.run.begin = 0,
            Direction::Forward => {
                state.run.end = self.source.string().schars() as i128 + 1;
                state.forward = IntervalCursor::End;
            }
        }
        Ok(())
    }
}
