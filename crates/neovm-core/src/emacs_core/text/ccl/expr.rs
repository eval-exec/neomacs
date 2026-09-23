//! CCL arithmetic and comparison operators.
//!
//! Numbers are GNU `int` registers (`src/ccl.c`, `ccl_expr_self` and
//! `ccl_set_expr`). Operators match `CCL_PLUS` through `CCL_ENCODE_SJIS`.

use super::{Flow, invalid_ccl_program_at};

/// Arithmetic operator embedded in an expression command.
///
/// Comparison operators start at `0x10`. `0x0D..=0x0F` are unused in
/// `ccl-arith-table`, so [`CclArith::from_repr`] returns `None` for them.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::FromRepr)]
enum CclArith {
    Plus = 0x00,
    Minus = 0x01,
    Mul = 0x02,
    Div = 0x03,
    Modulo = 0x04,
    And = 0x05,
    Or = 0x06,
    Xor = 0x07,
    Lsh = 0x08,
    Rsh = 0x09,
    Lsh8 = 0x0a,
    Rsh8 = 0x0b,
    DivMod = 0x0c,
    Less = 0x10,
    Greater = 0x11,
    Equal = 0x12,
    LessEqual = 0x13,
    GreaterEqual = 0x14,
    NotEqual = 0x15,
    DecodeSjis = 0x16,
    EncodeSjis = 0x17,
}

fn load(registers: &[i64; 8], index: usize) -> i32 {
    registers[index] as i32
}

fn store(registers: &mut [i64; 8], index: usize, value: i32) {
    registers[index] = i64::from(value);
}

fn operator(code: i64, error_at: usize) -> Result<CclArith, Flow> {
    u8::try_from(code)
        .ok()
        .and_then(CclArith::from_repr)
        .ok_or_else(|| invalid_ccl_program_at(error_at))
}

fn lsh(value: i32, count: i32) -> Result<i32, ()> {
    if count < 0 {
        return Err(());
    }
    if count >= 32 {
        return Ok(0);
    }
    Ok(((value as u32) << (count as u32)) as i32)
}

fn rsh(value: i32, count: i32) -> Result<i32, ()> {
    if count < 0 {
        return Err(());
    }
    Ok(value >> count.min(31))
}

/// `SJIS_TO_JIS` from `src/coding.h`.
fn sjis_to_jis(code: i32) -> i32 {
    let s1 = code >> 8;
    let s2 = code & 0xff;
    let (j1, j2) = if s2 >= 0x9f {
        (s1 * 2 - if s1 >= 0xe0 { 0x160 } else { 0xe0 }, s2 - 0x7e)
    } else {
        (
            s1 * 2 - if s1 >= 0xe0 { 0x161 } else { 0xe1 },
            s2 - if s2 >= 0x7f { 0x20 } else { 0x1f },
        )
    };
    (j1 << 8) | j2
}

/// `JIS_TO_SJIS` from `src/coding.h`.
fn jis_to_sjis(code: i32) -> i32 {
    let j1 = code >> 8;
    let j2 = code & 0xff;
    let (s1, s2) = if j1 & 1 != 0 {
        (
            j1 / 2 + if j1 < 0x5f { 0x71 } else { 0xb1 },
            j2 + if j2 >= 0x60 { 0x20 } else { 0x1f },
        )
    } else {
        (j1 / 2 + if j1 < 0x5f { 0x70 } else { 0xb0 }, j2 + 0x7e)
    };
    (s1 << 8) | s2
}

fn combine_bytes(high: i32, low: i32) -> i32 {
    (((high as u32) << 8) | (low as u32)) as i32
}

/// `reg[destination] OP= operand`. Shift-JIS encoding is not in this switch.
pub(super) fn eval_expr_self(
    registers: &mut [i64; 8],
    destination: usize,
    operator_code: i64,
    operand: i32,
    error_at: usize,
) -> Result<(), Flow> {
    let current = load(registers, destination);
    let value = match operator(operator_code, error_at)? {
        CclArith::Plus => current.wrapping_add(operand),
        CclArith::Minus => current.wrapping_sub(operand),
        CclArith::Mul => current.wrapping_mul(operand),
        CclArith::Div => {
            if operand == 0 {
                return Err(invalid_ccl_program_at(error_at));
            }
            if current == i32::MIN && operand == -1 {
                current
            } else {
                current / operand
            }
        }
        CclArith::Modulo => {
            if operand == 0 {
                return Err(invalid_ccl_program_at(error_at));
            }
            if operand == -1 { 0 } else { current % operand }
        }
        CclArith::And => current & operand,
        CclArith::Or => current | operand,
        CclArith::Xor => current ^ operand,
        CclArith::Lsh => lsh(current, operand).map_err(|_| invalid_ccl_program_at(error_at))?,
        CclArith::Rsh => rsh(current, operand).map_err(|_| invalid_ccl_program_at(error_at))?,
        CclArith::Lsh8 => ((current as u32) << 8 | (operand as u32)) as i32,
        CclArith::Rsh8 => {
            store(registers, 7, current & 0xff);
            current >> 8
        }
        CclArith::DivMod => {
            if operand == 0 {
                return Err(invalid_ccl_program_at(error_at));
            }
            if operand == -1 {
                store(registers, 7, 0);
                current.wrapping_neg()
            } else {
                store(registers, 7, current % operand);
                current / operand
            }
        }
        CclArith::Less => i32::from(current < operand),
        CclArith::Greater => i32::from(current > operand),
        CclArith::Equal => i32::from(current == operand),
        CclArith::LessEqual => i32::from(current <= operand),
        CclArith::GreaterEqual => i32::from(current >= operand),
        CclArith::NotEqual => i32::from(current != operand),
        CclArith::DecodeSjis | CclArith::EncodeSjis => {
            return Err(invalid_ccl_program_at(error_at));
        }
    };
    store(registers, destination, value);
    Ok(())
}

/// `reg[destination] = left OP right`.
///
/// `CCL_DIVMOD` with a divisor of -1 follows GNU and negates the register
/// already stored at `destination`, then clears `r7`.
pub(super) fn eval_set_expr(
    registers: &mut [i64; 8],
    destination: usize,
    operator_code: i64,
    left: i32,
    right: i32,
    error_at: usize,
) -> Result<(), Flow> {
    match operator(operator_code, error_at)? {
        CclArith::Plus => store(registers, destination, left.wrapping_add(right)),
        CclArith::Minus => store(registers, destination, left.wrapping_sub(right)),
        CclArith::Mul => store(registers, destination, left.wrapping_mul(right)),
        CclArith::Div => {
            if right == 0 {
                return Err(invalid_ccl_program_at(error_at));
            }
            let value = if left == i32::MIN && right == -1 {
                left
            } else {
                left / right
            };
            store(registers, destination, value);
        }
        CclArith::Modulo => {
            if right == 0 {
                return Err(invalid_ccl_program_at(error_at));
            }
            let value = if right == -1 { 0 } else { left % right };
            store(registers, destination, value);
        }
        CclArith::And => store(registers, destination, left & right),
        CclArith::Or => store(registers, destination, left | right),
        CclArith::Xor => store(registers, destination, left ^ right),
        CclArith::Lsh => {
            let value = lsh(left, right).map_err(|_| invalid_ccl_program_at(error_at))?;
            store(registers, destination, value);
        }
        CclArith::Rsh => {
            let value = rsh(left, right).map_err(|_| invalid_ccl_program_at(error_at))?;
            store(registers, destination, value);
        }
        CclArith::Lsh8 => {
            store(registers, destination, combine_bytes(left, right));
        }
        CclArith::Rsh8 => {
            store(registers, destination, left >> 8);
            store(registers, 7, left & 0xff);
        }
        CclArith::DivMod => {
            if right == 0 {
                return Err(invalid_ccl_program_at(error_at));
            }
            if right == -1 {
                let negated = load(registers, destination).wrapping_neg();
                store(registers, destination, negated);
                store(registers, 7, 0);
            } else {
                store(registers, destination, left / right);
                store(registers, 7, left % right);
            }
        }
        CclArith::Less => store(registers, destination, i32::from(left < right)),
        CclArith::Greater => store(registers, destination, i32::from(left > right)),
        CclArith::Equal => store(registers, destination, i32::from(left == right)),
        CclArith::LessEqual => store(registers, destination, i32::from(left <= right)),
        CclArith::GreaterEqual => store(registers, destination, i32::from(left >= right)),
        CclArith::NotEqual => store(registers, destination, i32::from(left != right)),
        CclArith::DecodeSjis => {
            let code = sjis_to_jis(combine_bytes(left, right));
            store(registers, destination, code >> 8);
            store(registers, 7, code & 0xff);
        }
        CclArith::EncodeSjis => {
            let code = jis_to_sjis(combine_bytes(left, right));
            store(registers, destination, code >> 8);
            store(registers, 7, code & 0xff);
        }
    }
    Ok(())
}
