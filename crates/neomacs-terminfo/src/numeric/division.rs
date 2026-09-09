//! A bounded proof for native division, not an output interpreter.
//!
//! Track only constants established by this program. Unknown values include
//! parameters, inherited variables, overflowing arithmetic and branch joins
//! that disagree. Every possible branch is checked, independent of native state.
//! This avoids duplicating ncurses' formatting, parameter inference and variable
//! lifetime rules. Native INT_MIN / -1 (also %m) must never be reachable.
use crate::Error;
use std::collections::BTreeMap;
use std::ops::Range;

type Constant = Option<i32>;

#[derive(Clone)]
struct State {
    stack: Vec<Constant>,
    variables: [Constant; 52],
}

impl State {
    fn pop(&mut self) -> Constant {
        // Unknown also covers ncurses' zero on stack underflow, without making
        // assumptions about the number of implicit termcap parameters.
        self.stack.pop().flatten()
    }

    fn push(&mut self, value: Constant) -> Result<(), Error> {
        // ncurses (including Apple's version) has 20 slots and drops excess
        // pushes. Reject overflow rather than trusting a dropped divisor.
        if self.stack.len() == 20 {
            return Err(Error::InvalidNumericFormat);
        }
        self.stack.push(value);
        Ok(())
    }

    fn merge(&mut self, other: &Self) {
        // Align from the top: the longest stack bounds depth; missing values
        // on a shorter path are unknown. Only facts shared by both survive.
        let depth = self.stack.len().max(other.stack.len());
        let mut stack = vec![None; depth];
        for (offset, (left, right)) in self
            .stack
            .iter()
            .rev()
            .zip(other.stack.iter().rev())
            .enumerate()
        {
            if left == right {
                stack[depth - 1 - offset] = *left;
            }
        }
        self.stack = stack;
        for (left, right) in self.variables.iter_mut().zip(other.variables) {
            if *left != right {
                *left = None;
            }
        }
    }
}

fn variable(name: u8) -> usize {
    if name.is_ascii_lowercase() {
        usize::from(name - b'a')
    } else {
        26 + usize::from(name - b'A')
    }
}

fn binary(op: u8, left: Constant, right: Constant) -> Result<Constant, Error> {
    if matches!(op, b'/' | b'm')
        && (right.is_none() || (right == Some(-1) && matches!(left, None | Some(i32::MIN))))
    {
        return Err(Error::InvalidNumericFormat);
    }
    let value = left.zip(right).and_then(|(x, y)| match op {
        b'+' => x.checked_add(y),
        b'-' => x.checked_sub(y),
        b'*' => x.checked_mul(y),
        b'/' => Some(if y == 0 { 0 } else { x / y }),
        b'm' => Some(if y == 0 { 0 } else { x % y }),
        b'&' => Some(x & y),
        b'|' => Some(x | y),
        b'^' => Some(x ^ y),
        b'=' => Some(i32::from(x == y)),
        b'>' => Some(i32::from(x > y)),
        b'<' => Some(i32::from(x < y)),
        b'A' => Some(i32::from(x != 0 && y != 0)),
        b'O' => Some(i32::from(x != 0 || y != 0)),
        _ => unreachable!("validated binary operator"),
    });
    Ok(value)
}

// ncurses skips conditionals byte by byte, not by parsed tokens. In particular
// even a percent inside a quoted constant can affect the skipped path. Mirror
// that scan, and reject targets inside a token rather than assume a different
// control flow. All edges point forward, so one pass joins every incoming path.
fn skip_target(sequence: &[u8], mut offset: usize, allow_else: bool) -> usize {
    let mut depth = 0;
    while offset < sequence.len() {
        if sequence[offset] == b'%' {
            offset += 1;
            match sequence.get(offset) {
                Some(b'?') => depth += 1,
                Some(b';') if depth > 0 => depth -= 1,
                Some(b';') => return offset + 1,
                Some(b'e') if depth == 0 && allow_else => return offset + 1,
                _ => {}
            }
        }
        offset += 1;
    }
    sequence.len()
}

pub(super) fn validate(sequence: &[u8], tokens: &[Range<usize>]) -> Result<(), Error> {
    let explicit = tokens
        .iter()
        .any(|range| sequence[range.clone()].starts_with(b"%p"));
    let mut current = Some(State {
        // At most nine implicit arguments. Their exact count and values are
        // deliberately unknown; explicit %p programs start with an empty stack.
        stack: vec![None; if explicit { 0 } else { 9 }],
        variables: [None; 52],
    });
    let mut pending: BTreeMap<usize, State> = BTreeMap::new();
    for (index, range) in tokens.iter().enumerate() {
        if let Some(incoming) = pending.remove(&index) {
            if let Some(state) = &mut current {
                state.merge(&incoming);
            } else {
                current = Some(incoming);
            }
        }
        let Some(state) = &mut current else { continue };
        let token = &sequence[range.clone()];
        if token[0] != b'%' || token == b"%%" {
            continue;
        }
        match token[1] {
            b'p' => state.push(None)?,
            b'g' => state.push(state.variables[variable(token[2])])?,
            b'P' => state.variables[variable(token[2])] = state.pop(),
            b'\'' => state.push(Some(i32::from(token[2])))?,
            b'{' => state.push(
                std::str::from_utf8(&token[2..token.len() - 1])
                    .ok()
                    .and_then(|s| s.parse().ok()),
            )?,
            b'+' | b'-' | b'*' | b'/' | b'm' | b'&' | b'|' | b'^' | b'=' | b'>' | b'<' | b'A'
            | b'O' => {
                let right = state.pop();
                let left = state.pop();
                state.push(binary(token[1], left, right)?)?;
            }
            b'!' | b'~' => {
                let value = state.pop().map(|x| {
                    if token[1] == b'!' {
                        i32::from(x == 0)
                    } else {
                        !x
                    }
                });
                state.push(value)?;
            }
            b't' | b'e' => {
                if token[1] == b't' {
                    state.pop();
                }
                let offset = skip_target(sequence, range.end, token[1] == b't');
                let target = if offset == sequence.len() {
                    tokens.len()
                } else {
                    tokens
                        .binary_search_by_key(&offset, |range| range.start)
                        .map_err(|_| Error::InvalidNumericFormat)?
                };
                pending
                    .entry(target)
                    .and_modify(|other| other.merge(state))
                    .or_insert_with(|| state.clone());
                if token[1] == b'e' {
                    current = None;
                }
            }
            b'i' => {
                // In implicit termcap mode %i overwrites native stack slots.
                if !explicit {
                    state.stack.fill(None);
                }
            }
            b'?' | b';' => {}
            _ => {
                state.pop();
            } // Validated numeric output conversion.
        }
    }
    Ok(())
}
