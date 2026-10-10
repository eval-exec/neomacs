use super::*;

/// The values GNU reports for these names on a graphic frame (`(color-values
/// NAME)`), i.e. `etc/rgb.txt` itself. The left column is the issue #545
/// report's color list.
#[test]
fn database_names_answer_the_gnu_rgb_txt_values() {
    for (name, expected) in [
        ("gray14", (36, 36, 36)),
        ("gray50", (127, 127, 127)),
        ("gray75", (191, 191, 191)),
        ("green", (0, 255, 0)),
        ("maroon", (176, 48, 96)),
        ("purple", (160, 32, 240)),
        ("gray", (190, 190, 190)),
        ("light blue", (173, 216, 230)),
    ] {
        assert_eq!(x11_color_lookup(name), Some(expected), "{name}");
    }
}

/// XParseColor and the W32 map both match names case-insensitively
/// (`lstrcmpi`, `src/w32fns.c:867`).
#[test]
fn database_names_match_case_insensitively() {
    assert_eq!(x11_color_lookup("GRAY14"), Some((36, 36, 36)));
    assert_eq!(x11_color_lookup("Light Blue"), Some((173, 216, 230)));
}

/// The names the X11R6 database GNU ships does not define stay unresolved, so
/// the caller can apply GNU's fallback instead of inventing a color. `aqua` is
/// a CSS/SVG addition that `etc/rgb.txt` never contained.
#[test]
fn names_absent_from_the_database_resolve_to_nothing() {
    assert_eq!(x11_color_lookup("aqua"), None);
    assert_eq!(x11_color_lookup("no-such-color"), None);
    // XPM's transparency keyword is not a database name: GNU checks it before
    // consulting the color hook at all (`src/image.c:6505`).
    assert_eq!(x11_color_lookup("none"), None);
}

/// GNU scales channel values instead of zero-extending them (`#f00` is
/// `#ff0000`, src/xterm.c:9276-9280) and accepts 1..=4 digits per channel
/// (`parse_color_spec`, src/xfaces.c:984-1017).
#[test]
fn hex_forms_scale_every_channel_width() {
    for (spec, expected) in [
        ("#f00", (255, 0, 0)),
        ("#abc", (170, 187, 204)),
        ("#aabbcc", (170, 187, 204)),
        ("#fff000000", (255, 0, 0)),
        ("#000fff000", (0, 255, 0)),
        ("#ffff00000000", (255, 0, 0)),
        ("#ffffffffffff", (255, 255, 255)),
        ("#F0F0F0", (240, 240, 240)),
    ] {
        assert_eq!(x11_hex_color(spec), Some(expected), "{spec}");
    }
}

/// Malformed hex resolves to nothing rather than to black, and a multi-byte
/// character is rejected as the non-hex byte GNU sees, not sliced.
#[test]
fn malformed_hex_resolves_to_nothing() {
    assert_eq!(x11_hex_color("#"), None);
    assert_eq!(x11_hex_color("#abcd"), None);
    assert_eq!(x11_hex_color("#fffffffffffff"), None);
    assert_eq!(x11_hex_color("#xyz"), None);
    assert_eq!(x11_hex_color("#ééé"), None);
    assert_eq!(x11_hex_color("ff0000"), None);
}

#[test]
fn values_resolve_as_hex_or_name() {
    assert_eq!(x11_color_value("#00ff00"), Some((0, 255, 0)));
    assert_eq!(x11_color_value("light blue"), Some((173, 216, 230)));
    assert_eq!(x11_color_value("aqua"), None);
}

/// The compiled table must be `etc/rgb.txt`, entry for entry: a build script
/// that dropped or mangled lines would otherwise resolve names to nothing and
/// paint them with the fallback, which is the bug this table prevents.
#[test]
fn compiled_table_matches_every_rgb_txt_entry() {
    let path = neomacs_infra::workspace_root().join("etc/rgb.txt");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut entries = 0;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let expected = (
            parts.next().and_then(|s| s.parse::<u8>().ok()),
            parts.next().and_then(|s| s.parse::<u8>().ok()),
            parts.next().and_then(|s| s.parse::<u8>().ok()),
        );
        let name = parts.collect::<Vec<_>>().join(" ");
        let (Some(red), Some(green), Some(blue)) = expected else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        entries += 1;
        assert_eq!(
            x11_color_lookup(&name),
            Some((red, green, blue)),
            "{name} from {}",
            path.display()
        );
        let collapsed = name.replace(' ', "");
        assert_eq!(
            x11_color_lookup(&collapsed),
            Some((red, green, blue)),
            "collapsed spelling of {name}"
        );
    }
    assert!(
        entries > 700,
        "parsed only {entries} entries from {}",
        path.display()
    );
}
