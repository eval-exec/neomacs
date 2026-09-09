//! Preflight numeric arithmetic using a snapshot of the native context.
//! ncurses still owns output formatting and all persistent variable updates.
use crate::Error;
use std::ops::Range;

pub(crate) struct Program<'a> {
    sequence: &'a [u8],
    tokens: Vec<Range<usize>>,
    pub(crate) variables: Vec<u8>,
}

pub(crate) fn variable(name: u8) -> usize {
    if name.is_ascii_lowercase() {
        usize::from(name - b'a')
    } else {
        26 + usize::from(name - b'A')
    }
}

fn binary(op: u8, x: i32, y: i32) -> Result<i32, Error> {
    Ok(match op {
        // ncurses' numeric stack uses 32-bit C int on the supported targets.
        b'+' => x.wrapping_add(y),
        b'-' => x.wrapping_sub(y),
        b'*' => x.wrapping_mul(y),
        b'/' if y != 0 => x.checked_div(y).ok_or(Error::InvalidNumericFormat)?,
        b'm' if y != 0 => x.checked_rem(y).ok_or(Error::InvalidNumericFormat)?,
        b'/' | b'm' => 0,
        b'&' => x & y,
        b'|' => x | y,
        b'^' => x ^ y,
        b'=' => i32::from(x == y),
        b'>' => i32::from(x > y),
        b'<' => i32::from(x < y),
        b'A' => i32::from(x != 0 && y != 0),
        b'O' => i32::from(x != 0 || y != 0),
        _ => unreachable!("validated binary operator"),
    })
}

// ncurses skips conditionals byte by byte, not by parsed tokens. In particular
// even a percent inside a quoted constant can affect the skipped path. Mirror
// that scan, and reject targets inside a token rather than assume a different
// control flow. All edges point forward; preflight follows the executed path.
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

impl<'a> Program<'a> {
    pub(crate) fn new(sequence: &'a [u8], tokens: Vec<Range<usize>>) -> Self {
        let mut variables = tokens
            .iter()
            .filter_map(|range| {
                let token = &sequence[range.clone()];
                token.starts_with(b"%g").then(|| token[2])
            })
            .collect::<Vec<_>>();
        variables.sort_unstable();
        variables.dedup();
        Self {
            sequence,
            tokens,
            variables,
        }
    }

    pub(crate) fn validate(&self, parameters: [i32; 9], variables: [i32; 52]) -> Result<(), Error> {
        let explicit = self
            .tokens
            .iter()
            .any(|range| self.sequence[range.clone()].starts_with(b"%p"));
        // Native releases differ in implicit termcap argument inference. Cover
        // every possible count without binding to a private ncurses function.
        for count in 0..=if explicit { 0 } else { 9 } {
            self.validate_stack(parameters, variables, count, explicit)?;
        }
        Ok(())
    }

    fn validate_stack(
        &self,
        mut parameters: [i32; 9],
        mut variables: [i32; 52],
        count: usize,
        explicit: bool,
    ) -> Result<(), Error> {
        let mut stack: Vec<i32> = parameters[..count].iter().rev().copied().collect();
        let mut incremented = false;
        let mut index = 0;
        while let Some(range) = self.tokens.get(index) {
            let token = &self.sequence[range.clone()];
            index += 1;
            if token[0] != b'%' || token == b"%%" {
                continue;
            }
            let value = match token[1] {
                b'p' => Some(parameters[usize::from(token[2] - b'1')]),
                b'g' => Some(variables[variable(token[2])]),
                b'P' => {
                    variables[variable(token[2])] = stack.pop().unwrap_or(0);
                    None
                }
                b'\'' => Some(i32::from(token[2])),
                b'{' => Some(
                    std::str::from_utf8(&token[2..token.len() - 1])
                        .expect("validated decimal")
                        .parse()
                        .expect("validated i32"),
                ),
                b'+' | b'-' | b'*' | b'/' | b'm' | b'&' | b'|' | b'^' | b'=' | b'>' | b'<'
                | b'A' | b'O' => {
                    let right = stack.pop().unwrap_or(0);
                    let left = stack.pop().unwrap_or(0);
                    Some(binary(token[1], left, right)?)
                }
                b'!' | b'~' => {
                    let value = stack.pop().unwrap_or(0);
                    Some(if token[1] == b'!' {
                        i32::from(value == 0)
                    } else {
                        !value
                    })
                }
                b't' | b'e' => {
                    if token[1] == b'e' || stack.pop().unwrap_or(0) == 0 {
                        let offset = skip_target(self.sequence, range.end, token[1] == b't');
                        index = if offset == self.sequence.len() {
                            self.tokens.len()
                        } else {
                            self.tokens
                                .binary_search_by_key(&offset, |range| range.start)
                                .map_err(|_| Error::InvalidNumericFormat)?
                        };
                    }
                    None
                }
                b'i' => {
                    if !incremented {
                        incremented = true;
                        for (slot, parameter) in parameters.iter_mut().take(2).enumerate() {
                            *parameter = parameter.wrapping_add(1);
                            if !explicit && let Some(value) = stack.get_mut(slot) {
                                *value = *parameter;
                            }
                        }
                    }
                    None
                }
                b'?' | b';' => None,
                _ => {
                    stack.pop();
                    None
                } // Validated numeric output conversion.
            };
            // Native underflow returns zero; overflow drops the pushed value.
            if let Some(value) = value
                && stack.len() < 20
            {
                stack.push(value);
            }
        }
        Ok(())
    }
}
