use super::error::{Flow, signal};
use super::value::{Value, ValueKind, VecLikeType};
use crate::buffer::{Buffer, BufferManager, EmacsByteRange, LispCharPos1};
use crate::emacs_core::error::LispCondition;
use malachite::integer::Integer;

pub(crate) fn fix_position_with_buffers(
    buffers: &BufferManager,
    value: &Value,
) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => {
            super::marker::marker_position_as_int_with_buffers(buffers, value)
        }
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(fix_position_bignum(value)),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(crate) fn fix_position_eval(eval: &super::eval::Context, value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => {
            super::marker::marker_position_as_int_eval(eval, value)
        }
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(fix_position_bignum(value)),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

fn fix_position_bignum(value: &Value) -> i64 {
    let n = value.as_bignum().expect("bignum kind");
    if n >= &Integer::from(0) {
        Value::MOST_POSITIVE_FIXNUM
    } else {
        Value::MOST_NEGATIVE_FIXNUM
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LispRegionArgs {
    start: LispCharPos1,
    end: LispCharPos1,
    start_arg: Value,
    end_arg: Value,
}

impl LispRegionArgs {
    pub(crate) fn from_values(
        buffers: &BufferManager,
        start: Value,
        end: Value,
    ) -> Result<Self, Flow> {
        Ok(Self {
            start: LispCharPos1::new(fix_position_with_buffers(buffers, &start)?),
            end: LispCharPos1::new(fix_position_with_buffers(buffers, &end)?),
            start_arg: start,
            end_arg: end,
        })
    }

    pub(crate) fn from_optional_values(
        buffers: &BufferManager,
        start: Option<Value>,
        end: Option<Value>,
        default_start: LispCharPos1,
        default_end: LispCharPos1,
    ) -> Result<Self, Flow> {
        let (start, start_arg) = match start {
            None => (default_start, Value::fixnum(default_start.as_i64())),
            Some(value) if value.is_nil() => (default_start, Value::fixnum(default_start.as_i64())),
            Some(value) => (
                LispCharPos1::new(fix_position_with_buffers(buffers, &value)?),
                value,
            ),
        };
        let (end, end_arg) = match end {
            None => (default_end, Value::fixnum(default_end.as_i64())),
            Some(value) if value.is_nil() => (default_end, Value::fixnum(default_end.as_i64())),
            Some(value) => (
                LispCharPos1::new(fix_position_with_buffers(buffers, &value)?),
                value,
            ),
        };

        Ok(Self {
            start,
            end,
            start_arg,
            end_arg,
        })
    }

    pub(crate) fn accessible_byte_range(self, buffer: &Buffer) -> Result<EmacsByteRange, Flow> {
        let point_min = buffer.point_min_lisp_char_pos();
        let point_max = buffer.point_max_lisp_char_pos();
        let (start, end) = self.ordered_positions();
        if start < point_min || end > point_max {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![Value::make_buffer(buffer.id), self.start_arg, self.end_arg],
            ));
        }

        Ok(EmacsByteRange::new(
            buffer.lisp_pos_to_accessible_emacs_byte_pos(start),
            buffer.lisp_pos_to_accessible_emacs_byte_pos(end),
        ))
    }

    fn ordered_positions(self) -> (LispCharPos1, LispCharPos1) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}
