//! GNU font.c's positional/value restrictions for abbreviated XLFD fields.

use super::{
    FONT_SLANT_STYLE_TABLE, FONT_WEIGHT_STYLE_TABLE, FONT_WIDTH_STYLE_TABLE, XLFD_ADSTYLE,
    XLFD_AVGWIDTH, XLFD_ENCODING, XLFD_FAMILY, XLFD_FOUNDRY, XLFD_LAST, XLFD_PIXEL, XLFD_POINT,
    XLFD_REGISTRY, XLFD_RESX, XLFD_RESY, XLFD_SLANT, XLFD_SPACING, XLFD_SWIDTH, XLFD_WEIGHT,
    font_style_symbol_from_name,
};

pub(super) enum ParsedXlfdFields {
    Complete([String; XLFD_LAST]),
    Abbreviated([String; XLFD_LAST]),
}

#[derive(Clone)]
enum Field {
    Wildcard,
    Number(i64),
    Symbol(String),
}

impl Field {
    fn parse(text: &str) -> Option<Self> {
        if text == "*" {
            Some(Self::Wildcard)
        } else if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
            text.parse().ok().map(Self::Number)
        } else {
            Some(Self::Symbol(text.to_owned()))
        }
    }

    fn wildcard(&self) -> bool {
        matches!(self, Self::Wildcard)
    }

    fn text(&self) -> String {
        match self {
            Self::Wildcard => "*".to_owned(),
            Self::Number(number) => number.to_string(),
            Self::Symbol(text) => text.clone(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Range {
    from: i32,
    to: i32,
    mask: u16,
}

const fn bit(index: usize) -> u16 {
    1 << index
}

const SYMBOLS: u16 = bit(XLFD_FOUNDRY) | bit(XLFD_FAMILY) | bit(XLFD_ADSTYLE) | bit(XLFD_REGISTRY);
const EMPTY: u16 = bit(XLFD_FOUNDRY) | bit(XLFD_ADSTYLE);
const LARGE_NUMBERS: u16 = bit(XLFD_POINT) | bit(XLFD_RESX) | bit(XLFD_RESY) | bit(XLFD_AVGWIDTH);

fn value_range(field: &Field, index: usize, count: usize, positional: Range) -> Range {
    let range = |from, to, mask| Range { from, to, mask };
    match field {
        Field::Wildcard => positional,
        Field::Number(_) if index + 1 == count => range(13, 13, bit(XLFD_ENCODING)),
        Field::Number(0) => range(6, 11, bit(XLFD_PIXEL) | LARGE_NUMBERS),
        Field::Number(number) if *number <= 48 => range(6, 6, bit(XLFD_PIXEL)),
        Field::Number(_) => range(7, 11, LARGE_NUMBERS),
        Field::Symbol(text) if text.is_empty() => range(0, 5, EMPTY),
        Field::Symbol(_) if index == 0 => range(0, 0, bit(XLFD_FOUNDRY)),
        Field::Symbol(text) if index + 1 == count => {
            if text.ends_with('*') {
                range(12, 13, bit(XLFD_REGISTRY) | bit(XLFD_ENCODING))
            } else {
                range(13, 13, bit(XLFD_ENCODING))
            }
        }
        Field::Symbol(text) => {
            for (position, table) in [
                (XLFD_WEIGHT, FONT_WEIGHT_STYLE_TABLE),
                (XLFD_SLANT, FONT_SLANT_STYLE_TABLE),
                (XLFD_SWIDTH, FONT_WIDTH_STYLE_TABLE),
            ] {
                if positional.from <= position as i32
                    && positional.to >= position as i32
                    && font_style_symbol_from_name(table, text).is_some()
                {
                    return range(position as i32, position as i32, bit(position));
                }
            }
            if matches!(text.as_str(), "c" | "m" | "p" | "d") {
                range(10, 10, bit(XLFD_SPACING))
            } else {
                range(0, 13, SYMBOLS)
            }
        }
    }
}

fn expand(fields: &[Field]) -> Option<[String; XLFD_LAST]> {
    let count = fields.len();
    if count == 0 || count >= XLFD_LAST || !fields.iter().any(Field::wildcard) {
        return None;
    }
    let mut ranges = [Range::default(); XLFD_LAST];
    let mut from = 0;
    let mut mask = (1u16 << (XLFD_LAST - count + 1)) - 1;
    for (index, field) in fields.iter().enumerate() {
        let to = (XLFD_LAST - count + index) as i32;
        let positional = Range { from, to, mask };
        let mut restriction = value_range(field, index, count, positional);
        restriction.mask &= mask;
        restriction.from = restriction.from.max(from);
        restriction.to = restriction.to.min(to);
        while restriction.from < XLFD_LAST as i32
            && restriction.mask & bit(restriction.from as usize) == 0
        {
            restriction.from += 1;
        }
        while restriction.to >= 0 && restriction.mask & bit(restriction.to as usize) == 0 {
            restriction.to -= 1;
        }
        if restriction.from > restriction.to {
            return None;
        }
        ranges[index] = restriction;
        if !field.wildcard() && (restriction.from > from || restriction.to < to) {
            from = restriction.from;
            let mut preceding_from = restriction.from - 1;
            let mut preceding_to = restriction.to - 1;
            for previous in (0..index).rev() {
                let range = &mut ranges[previous];
                if !fields[previous].wildcard() && range.from < preceding_from {
                    range.from = preceding_from;
                    while preceding_from < XLFD_LAST as i32
                        && range.mask & bit(preceding_from as usize) == 0
                    {
                        preceding_from += 1;
                    }
                    range.from = preceding_from;
                } else {
                    preceding_from = range.from;
                }
                if range.to > preceding_to {
                    range.to = preceding_to;
                    while preceding_to >= 0 && range.mask & bit(preceding_to as usize) == 0 {
                        preceding_to -= 1;
                    }
                    range.to = preceding_to;
                } else {
                    preceding_to = range.to;
                }
                if preceding_from > preceding_to {
                    return None;
                }
                preceding_from -= 1;
                preceding_to -= 1;
            }
        }
        from += 1;
        mask <<= 1;
    }
    let mut expanded = std::array::from_fn(|_| "*".to_owned());
    let mut position = 0;
    for (index, field) in fields.iter().enumerate() {
        let first = usize::try_from(ranges[index].from).ok()?;
        if position < first {
            if index == 0 || !fields[index - 1].wildcard() {
                return None;
            }
            position = first;
        }
        *expanded.get_mut(position)? = field.text();
        position += 1;
    }
    if !fields.last()?.wildcard() && position < XLFD_REGISTRY {
        return None;
    }
    Some(expanded)
}

/// Split just as font_parse_xlfd_1 does. A retry gathers surplus separators
/// into FAMILY; the first attempt must still accept a registry with dashes.
pub(super) fn fields(name: &str, retry_family: bool) -> Option<ParsedXlfdFields> {
    if name.is_empty() {
        return None;
    }
    let leading_star = name.starts_with('*') && (name.len() == 1 || name.as_bytes()[1] == b'-');
    let mut starts = Vec::new();
    if leading_star {
        starts.push(0);
    }
    let mut segments = name.bytes().filter(|byte| *byte == b'-').count();
    for (offset, character) in name.char_indices().skip(usize::from(leading_star)) {
        if character != '-' {
            continue;
        }
        if retry_family && segments > XLFD_LAST && starts.len() == XLFD_WEIGHT {
            segments -= 1;
        } else {
            starts.push(offset + 1);
            if starts.len() == XLFD_LAST {
                break;
            }
        }
    }
    let raw: Vec<_> = starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).map_or(name.len(), |next| next - 1);
            &name[*start..end]
        })
        .collect();
    if raw.len() == XLFD_LAST {
        return Some(ParsedXlfdFields::Complete(std::array::from_fn(|index| {
            raw[index].to_owned()
        })));
    }
    let parsed: Option<Vec<_>> = raw
        .iter()
        .map(|text| {
            if text.starts_with('*') && *text != "*" {
                None
            } else {
                Field::parse(text)
            }
        })
        .collect();
    expand(&parsed?).map(ParsedXlfdFields::Abbreviated)
}

/// Use the same numeric-prefix grammar and four-value rule as GNU's
/// parse_matrix, including exponent/hex forms and its tilde negative sign.
pub(super) fn matrix_size(text: &str) -> Option<i64> {
    if !text.starts_with('[') {
        return None;
    }
    let storage = std::ffi::CString::new(text).ok()?;
    let bytes = storage.as_bytes_with_nul();
    let mut offset = 1;
    let mut last = 0.0;
    for _ in 0..4 {
        let byte = *bytes.get(offset)?;
        if byte == 0 || byte == b']' {
            return None;
        }
        let negative = byte == b'~';
        if negative {
            offset += 1;
        }
        let mut end = std::ptr::null_mut();
        // SAFETY: offset is inside storage's owned NUL-terminated buffer.
        // Skipping '~' can at most reach its terminator. strtod reads from
        // that suffix and writes only our local end-pointer variable.
        last = unsafe { libc::strtod(storage.as_ptr().add(offset), &mut end) };
        if negative {
            last = -last;
        }
        // Never dereference strtod's returned pointer. Validate its address
        // before using the offset for another safe byte lookup or C call.
        offset = end.addr().checked_sub(storage.as_ptr().addr())?;
        if offset >= bytes.len() {
            return None;
        }
    }
    // GNU stores an int. Avoid invoking its undefined out-of-range casts.
    (last.is_finite() && last >= i32::MIN as f64 && last <= i32::MAX as f64)
        .then_some(last.trunc() as i64)
}
