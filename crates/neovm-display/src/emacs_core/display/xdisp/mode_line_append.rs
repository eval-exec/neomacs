//! One stop/decimal/spec loop. Admission, input representation, and emission
//! belong to the admitted target driver; this module holds no Lisp authority.

use super::{CharLen, CharPos0, CharRange, EmacsChar};

pub(super) enum AppendAdmission {
    Continue,
    Stop,
}
pub(super) enum ZeroTerminator {
    Literal,
    Stop,
}
pub(super) enum TrailingPercent {
    Emit,
    Stop,
}

/// A decimal digit admitted by the single stop loop (or its legacy parser
/// adapter); arithmetic policies cannot receive a non-digit native integer.
#[derive(Clone, Copy)]
pub(super) struct DecimalDigit(u8);
impl DecimalDigit {
    pub(super) fn from_code(code: EmacsChar) -> Option<Self> {
        u8::try_from(code.code())
            .ok()
            .filter(u8::is_ascii_digit)
            .map(|code| Self(code - b'0'))
    }
    pub(super) fn value(self) -> u8 {
        self.0
    }
}

pub(super) trait PercentWidth: Sized {
    type Error;
    const ZERO: Self;
    fn decimal_digit(self, digit: DecimalDigit) -> Result<Self, Self::Error>;
}

pub(super) trait PercentAppendDriver {
    type Error;
    type Width: PercentWidth<Error = Self::Error>;
    const ZERO: ZeroTerminator;
    const TRAILING: TrailingPercent;

    fn code_at(&self, position: CharPos0) -> Option<EmacsChar>;
    fn admit(&mut self) -> Result<AppendAdmission, Self::Error>;
    fn literal(&mut self, range: CharRange) -> Result<(), Self::Error>;
    fn field(
        &mut self,
        range: CharRange,
        spec: Option<EmacsChar>,
        width: Self::Width,
    ) -> Result<(), Self::Error>;
}

fn source_end<D: PercentAppendDriver>(driver: &D, cursor: CharPos0) -> bool {
    match driver.code_at(cursor) {
        None => true,
        Some(code) => match D::ZERO {
            ZeroTerminator::Literal => false,
            ZeroTerminator::Stop => code == EmacsChar::from_char('\0'),
        },
    }
}

#[inline(always)]
pub(super) fn append_percent_stops<D: PercentAppendDriver>(driver: &mut D) -> Result<(), D::Error> {
    let mut cursor = CharPos0::ZERO;
    while !source_end(driver, cursor) {
        match driver.admit()? {
            AppendAdmission::Continue => {}
            AppendAdmission::Stop => break,
        }
        let start = cursor;
        if driver.code_at(cursor) != Some(EmacsChar::from_char('%')) {
            while !source_end(driver, cursor)
                && driver.code_at(cursor) != Some(EmacsChar::from_char('%'))
            {
                cursor = cursor.add_len(CharLen::new(1));
            }
            driver.literal(CharRange::new(start, cursor))?;
            continue;
        }
        cursor = cursor.add_len(CharLen::new(1));
        let mut width = D::Width::ZERO;
        while let Some(code) = driver.code_at(cursor) {
            let Some(digit) = DecimalDigit::from_code(code) else {
                break;
            };
            width = width.decimal_digit(digit)?;
            cursor = cursor.add_len(CharLen::new(1));
        }
        if source_end(driver, cursor) {
            match D::TRAILING {
                TrailingPercent::Stop => break,
                TrailingPercent::Emit => {}
            }
        }
        let spec = driver.code_at(cursor);
        if spec.is_some() {
            cursor = cursor.add_len(CharLen::new(1));
        }
        driver.field(CharRange::new(start, cursor), spec, width)?;
    }
    Ok(())
}
