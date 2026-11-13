//! GNU keyboard.c's closed separator-name vocabulary. GTK paints these alike.
use strum::EnumString;

#[derive(EnumString)]
#[strum(serialize_all = "kebab-case")]
enum SeparatorName {
    Space,
    NoLine,
    SingleLine,
    DoubleLine,
    SingleDashedLine,
    DoubleDashedLine,
    ShadowEtchedIn,
    ShadowEtchedOut,
    ShadowEtchedInDash,
    ShadowEtchedOutDash,
    ShadowDoubleEtchedIn,
    ShadowDoubleEtchedOut,
    ShadowDoubleEtchedInDash,
    ShadowDoubleEtchedOutDash,
}

pub(super) fn is_separator(label: &str) -> bool {
    label.bytes().all(|byte| byte == b'-')
        || label
            .strip_prefix("--")
            .is_some_and(|name| name.parse::<SeparatorName>().is_ok())
}
