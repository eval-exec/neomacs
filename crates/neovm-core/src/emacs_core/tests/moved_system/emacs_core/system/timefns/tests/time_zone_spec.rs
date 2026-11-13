use super::*;
use crate::emacs_core::timefns::time_zone_spec::TimeZoneSpec;

#[test]
fn timezone_spec_ends_at_gnu_c_string_terminator() {
    for (input, expected) in [
        ("UTC", "UTC"),
        ("UTC\0junk", "UTC"),
        ("\0EST5", ""),
        ("EST5\0\0", "EST5"),
    ] {
        assert_eq!(TimeZoneSpec::from(input).as_ref(), expected);
    }
}
