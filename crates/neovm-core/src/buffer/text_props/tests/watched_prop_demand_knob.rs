use super::*;
use std::ffi::OsStr;

#[test]
fn watched_prop_demand_os_knob_defaults_on_only_when_absent_and_preserves_explicit_settings() {
    assert_eq!(
        parse_watched_prop_demand_os_mode(None),
        WatchedPropDemandMode::On
    );
    for value in ["on", " ON ", "\tOn\n", " on "] {
        assert_eq!(
            parse_watched_prop_demand_os_mode(Some(OsStr::new(value))),
            WatchedPropDemandMode::On,
            "{value:?}"
        );
    }
    for value in [
        "", " ", "off", "OFF", "0", "false", "no", "unknown", "onward", "on off", "été", "1",
        "true", "yes",
    ] {
        assert_eq!(
            parse_watched_prop_demand_os_mode(Some(OsStr::new(value))),
            WatchedPropDemandMode::Off,
            "{value:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn watched_prop_demand_os_knob_present_non_unicode_settings_keep_the_off_path() {
    use std::os::unix::ffi::OsStrExt;

    for value in [
        b"\xff".as_slice(),
        b"on\xff".as_slice(),
        b"\xffon".as_slice(),
        b" ON\xff ".as_slice(),
        b"\xc3(".as_slice(),
    ] {
        assert_eq!(
            parse_watched_prop_demand_os_mode(Some(OsStr::from_bytes(value))),
            WatchedPropDemandMode::Off,
            "{value:?}"
        );
    }
}
