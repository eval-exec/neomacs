//! Canonical image codes, separate from the in-memory face-weight layout.
use super::FontWeight;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive, strum::EnumIter)]
#[repr(u16)]
pub(super) enum FontWeightDumpCode {
    Thin = 100,
    UltraLight = 101,
    Ultralight = 102,
    ExtraLight = 200,
    Extralight = 201,
    Light = 300,
    SemiLight = 350,
    Semilight = 351,
    Demilight = 352,
    Regular = 401,
    Normal = 400,
    Unspecified = 402,
    Book = 403,
    Medium = 500,
    SemiBold = 600,
    Semibold = 601,
    Demibold = 602,
    DemiBold = 603,
    Demi = 604,
    Bold = 700,
    ExtraBold = 800,
    Extrabold = 801,
    UltraBold = 802,
    Ultrabold = 803,
    Black = 900,
    Heavy = 901,
    UltraHeavy = 950,
    Ultraheavy = 951,
}

impl From<FontWeightDumpCode> for FontWeight {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn from(code: FontWeightDumpCode) -> Self {
        match code {
            FontWeightDumpCode::Thin => Self::Thin,
            FontWeightDumpCode::UltraLight => Self::UltraLight,
            FontWeightDumpCode::Ultralight => Self::Ultralight,
            FontWeightDumpCode::ExtraLight => Self::ExtraLight,
            FontWeightDumpCode::Extralight => Self::Extralight,
            FontWeightDumpCode::Light => Self::Light,
            FontWeightDumpCode::SemiLight => Self::SemiLight,
            FontWeightDumpCode::Semilight => Self::Semilight,
            FontWeightDumpCode::Demilight => Self::Demilight,
            FontWeightDumpCode::Regular => Self::Regular,
            FontWeightDumpCode::Normal => Self::Normal,
            FontWeightDumpCode::Unspecified => Self::Unspecified,
            FontWeightDumpCode::Book => Self::Book,
            FontWeightDumpCode::Medium => Self::Medium,
            FontWeightDumpCode::SemiBold => Self::SemiBold,
            FontWeightDumpCode::Semibold => Self::Semibold,
            FontWeightDumpCode::Demibold => Self::Demibold,
            FontWeightDumpCode::DemiBold => Self::DemiBold,
            FontWeightDumpCode::Demi => Self::Demi,
            FontWeightDumpCode::Bold => Self::Bold,
            FontWeightDumpCode::ExtraBold => Self::ExtraBold,
            FontWeightDumpCode::Extrabold => Self::Extrabold,
            FontWeightDumpCode::UltraBold => Self::UltraBold,
            FontWeightDumpCode::Ultrabold => Self::Ultrabold,
            FontWeightDumpCode::Black => Self::Black,
            FontWeightDumpCode::Heavy => Self::Heavy,
            FontWeightDumpCode::UltraHeavy => Self::UltraHeavy,
            FontWeightDumpCode::Ultraheavy => Self::Ultraheavy,
        }
    }
}

impl From<FontWeight> for FontWeightDumpCode {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn from(weight: FontWeight) -> Self {
        match weight {
            FontWeight::Thin => Self::Thin,
            FontWeight::UltraLight => Self::UltraLight,
            FontWeight::Ultralight => Self::Ultralight,
            FontWeight::ExtraLight => Self::ExtraLight,
            FontWeight::Extralight => Self::Extralight,
            FontWeight::Light => Self::Light,
            FontWeight::SemiLight => Self::SemiLight,
            FontWeight::Semilight => Self::Semilight,
            FontWeight::Demilight => Self::Demilight,
            FontWeight::Regular => Self::Regular,
            FontWeight::Normal => Self::Normal,
            FontWeight::Unspecified => Self::Unspecified,
            FontWeight::Book => Self::Book,
            FontWeight::Medium => Self::Medium,
            FontWeight::SemiBold => Self::SemiBold,
            FontWeight::Semibold => Self::Semibold,
            FontWeight::Demibold => Self::Demibold,
            FontWeight::DemiBold => Self::DemiBold,
            FontWeight::Demi => Self::Demi,
            FontWeight::Bold => Self::Bold,
            FontWeight::ExtraBold => Self::ExtraBold,
            FontWeight::Extrabold => Self::Extrabold,
            FontWeight::UltraBold => Self::UltraBold,
            FontWeight::Ultrabold => Self::Ultrabold,
            FontWeight::Black => Self::Black,
            FontWeight::Heavy => Self::Heavy,
            FontWeight::UltraHeavy => Self::UltraHeavy,
            FontWeight::Ultraheavy => Self::Ultraheavy,
        }
    }
}

#[cfg(test)]
#[path = "tests/dump_codes.rs"]
mod tests;
