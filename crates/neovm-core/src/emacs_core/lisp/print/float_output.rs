//! Validated float-output-format and GNU print.c:1222-1284 rendering.
//! All state is scalar or owned per call; no Lisp state is cached.

use super::{Value, format_float};

pub(crate) fn format_float_with_output_format(f: f64, output_format: Option<Value>) -> String {
    if f.is_nan() || f.is_infinite() {
        return format_float(f);
    }

    let Some(format) = output_format.and_then(|value| FloatOutputFormat::try_from(value).ok())
    else {
        return format_float(f);
    };

    let Some(mut text) = format.render(f) else {
        return format_float(f);
    };
    finish_float_output_format(&mut text, format.precision);
    text
}

/// Validated notation in GNU print.c:1253; adding a notation forces every
/// renderer to choose its precision semantics explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, num_enum::TryFromPrimitive)]
#[repr(u8)]
enum FloatNotation {
    Scientific = b'e',
    Fixed = b'f',
    General = b'g',
}

/// An omitted precision differs from explicit zero in GNU print.c:1266:
/// only explicit %.0f suppresses the float readability suffix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FloatPrecision {
    Omitted,
    Explicit(u8),
}

impl FloatPrecision {
    fn digits(self) -> u8 {
        match self {
            Self::Omitted => 0,
            Self::Explicit(digits) => digits,
        }
    }
}

/// Constructor guarantees precision <= DBL_DIG, and permits explicit zero
/// only for Fixed. This per-call scalar has no shared or mutator-local state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FloatOutputFormat {
    notation: FloatNotation,
    precision: FloatPrecision,
}

static_assertions::assert_impl_all!(FloatOutputFormat: Send, Sync);

#[derive(Debug, thiserror::Error)]
enum InvalidFloatOutputFormat {
    #[error("invalid float-output-format syntax")]
    Syntax,
    #[error("invalid float-output-format precision")]
    Precision,
}

impl TryFrom<Value> for FloatOutputFormat {
    type Error = InvalidFloatOutputFormat;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let bytes = value
            .as_lisp_string()
            .ok_or(InvalidFloatOutputFormat::Syntax)?
            .as_bytes();
        let nul = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        let format = &bytes[..nul];
        if format.len() < 3 || format[0] != b'%' || format[1] != b'.' {
            return Err(InvalidFloatOutputFormat::Syntax);
        }

        let mut index = 2;
        let precision = if format.get(index).is_some_and(u8::is_ascii_digit) {
            let mut digits = 0u8;
            while let Some(byte) = format.get(index).filter(|byte| byte.is_ascii_digit()) {
                digits = digits * 10 + (byte - b'0');
                if u32::from(digits) > f64::DIGITS {
                    return Err(InvalidFloatOutputFormat::Precision);
                }
                index += 1;
            }
            FloatPrecision::Explicit(digits)
        } else {
            FloatPrecision::Omitted
        };

        let notation =
            FloatNotation::try_from(*format.get(index).ok_or(InvalidFloatOutputFormat::Syntax)?)
                .map_err(|_| InvalidFloatOutputFormat::Syntax)?;
        if index + 1 != format.len()
            || (precision == FloatPrecision::Explicit(0) && notation != FloatNotation::Fixed)
        {
            return Err(InvalidFloatOutputFormat::Syntax);
        }
        Ok(Self {
            notation,
            precision,
        })
    }
}

impl FloatOutputFormat {
    fn render(self, value: f64) -> Option<String> {
        let precision = usize::from(self.precision.digits());
        match self.notation {
            FloatNotation::Scientific => {
                Some(normalize_float_exponent(&format!("{value:.precision$e}")))
            }
            FloatNotation::Fixed => Some(format!("{value:.precision$}")),
            FloatNotation::General => {
                // GNU print.c:1259 uses sprintf. A constant C format preserves
                // significant digits, rounded exponent selection, subnormal
                // rounding and mantissa-only zero trimming exactly. Startup
                // fixes LC_NUMERIC to C, as does GNU (main.rs:5807).
                let mut buffer = [0u8; 64];
                // SAFETY: destination is writable for buffer.len() bytes;
                // constant format consumes precisely an int and a double.
                // snprintf bounds the output, including the terminating NUL.
                let len = unsafe {
                    libc::snprintf(
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                        c"%.*g".as_ptr(),
                        libc::c_int::from(self.precision.digits()),
                        value,
                    )
                };
                let len = usize::try_from(len).ok()?;
                if len >= buffer.len() {
                    return None;
                }
                let mut bytes = Vec::with_capacity(len + 2);
                bytes.extend_from_slice(&buffer[..len]);
                String::from_utf8(bytes).ok()
            }
        }
    }
}

fn normalize_float_exponent(s: &str) -> String {
    let Some((mantissa, exponent)) = s.split_once('e') else {
        return s.to_string();
    };

    let exp = exponent.parse::<i32>().unwrap_or(0);
    if exp >= 0 {
        format!("{mantissa}e+{exp:02}")
    } else {
        format!("{mantissa}e-{abs:02}", abs = -exp)
    }
}

fn finish_float_output_format(text: &mut String, precision: FloatPrecision) {
    if precision == FloatPrecision::Explicit(0) {
        return;
    }

    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'-') {
        index += 1;
    }

    if bytes.get(index) == Some(&b'.') && index + 1 == bytes.len() {
        text.push('0');
    } else if index == bytes.len() {
        text.push_str(".0");
    }
}

#[cfg(test)]
#[path = "tests/float_format_units.rs"]
mod float_format_units;
