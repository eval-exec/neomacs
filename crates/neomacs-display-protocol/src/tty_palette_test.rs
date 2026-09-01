use super::terminal_color::TerminalColor;
use super::tty_palette::{TtyPalette, TtyPaletteEntry};

/// One terminal's measured GNU palette and the answers GNU gave over it.
struct Recorded {
    term: &'static str,
    alist: &'static str,
    sweep: &'static str,
}

/// Four terminals whose palettes genuinely differ, so a table that happens to be
/// right for one is visibly wrong for another:
///
/// ```text
///                     display-color-cells   tty-color-alist
/// xterm                                 8         8 entries
/// rxvt-16color                         16        16 entries
/// linux-16color                        16         8 entries
/// xterm-256color                      256       256 entries
/// ```
///
/// `rxvt-16color`'s `blue` is (0,0,205) where xterm's is (0,0,238), its
/// `brightblack` (77,77,77) against (127,127,127), its `brightblue` (0,0,255)
/// against (92,92,255).  `linux-16color` reports 16 cells and registers 8
/// full-intensity colours, because nothing registers a 16-colour palette for it
/// and it falls back to `tty-register-default-colors` over
/// `tty-standard-colors` (lisp/term/tty-colors.el:748-757).
const RECORDED: [Recorded; 4] = [
    Recorded {
        term: "xterm",
        alist: include_str!("tty_palette_data/xterm-alist.txt"),
        sweep: include_str!("tty_palette_data/xterm-sweep.txt"),
    },
    Recorded {
        term: "rxvt-16color",
        alist: include_str!("tty_palette_data/rxvt-16color-alist.txt"),
        sweep: include_str!("tty_palette_data/rxvt-16color-sweep.txt"),
    },
    Recorded {
        term: "linux-16color",
        alist: include_str!("tty_palette_data/linux-16color-alist.txt"),
        sweep: include_str!("tty_palette_data/linux-16color-sweep.txt"),
    },
    Recorded {
        term: "xterm-256color",
        alist: include_str!("tty_palette_data/xterm-256color-alist.txt"),
        sweep: include_str!("tty_palette_data/xterm-256color-sweep.txt"),
    },
];

fn parse_palette(text: &str) -> (TtyPalette, i64) {
    let mut cells = 0_i64;
    let mut entries = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# CELLS ") {
            cells = rest.trim().parse().expect("cell count");
            continue;
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 5, "NAME INDEX R G B: {line:?}");
        let to8 = |field: &str| -> u8 {
            let value: i64 = field.parse().expect("16-bit component");
            (value.clamp(0, 65535) / 257) as u8
        };
        entries.push(TtyPaletteEntry {
            name: fields[0].to_owned(),
            index: fields[1].parse().expect("palette index"),
            rgb: Some((to8(fields[2]), to8(fields[3]), to8(fields[4]))),
        });
    }
    (TtyPalette::new(entries, cells), cells)
}

/// The search must reproduce GNU's answer for every sampled colour, on every
/// palette -- because the palette is what varies.
///
/// Ledger 153 pinned 5,832 GNU answers on one terminal against a Rust function
/// that searched a HARDCODED xterm table, and measured what that costs where the
/// table is not the terminal's: 18.2% of these very samples wrong on
/// `rxvt-16color`, 40.6% on `linux-16color`.  Handing the search GNU's own
/// palette is the whole fix, and this is the gate for it: same 5,832 samples,
/// four palettes instead of one.
#[test]
fn tty_palette_approximates_exactly_as_gnu_does() {
    let mut report = Vec::new();
    for recorded in &RECORDED {
        let (palette, cells) = parse_palette(recorded.alist);
        assert!(!palette.is_empty(), "{}: empty palette", recorded.term);
        let mut compared = 0_usize;
        let mut mismatches = Vec::new();
        for line in recorded.sweep.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let (hex, index) = line.split_once(' ').expect("RRGGBB INDEX");
            let channel =
                |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex channel");
            let expected: u16 = index.trim().parse().expect("palette index");
            compared += 1;
            let got = palette
                .approximate(channel(0), channel(2), channel(4))
                .map(|(color, _)| color);
            if got != Some(TerminalColor::Indexed(expected)) {
                if mismatches.len() < 8 {
                    mismatches.push(format!("#{hex}: GNU {expected}, got {got:?}"));
                }
            }
        }
        assert_eq!(compared, 5832, "{}: sweep lost samples", recorded.term);
        assert!(
            mismatches.is_empty(),
            "{} (cells {cells}): {} of {compared} disagree with GNU\n{}",
            recorded.term,
            mismatches.len(),
            mismatches.join("\n")
        );
        report.push(format!("{}: 0 of {compared} differ", recorded.term));
    }
    eprintln!("{}", report.join("\n"));
}

/// The name match is the half no RGB search can reproduce.
///
/// `map_tty_color` takes it before anything else (src/xfaces.c:6640-6648), and
/// `tty-color-define` can put a name at an index its own RGB would never
/// approximate to.  Read out of GNU on a pty: after
/// `(tty-color-define "red" 200 '(65535 0 0))` on `TERM=xterm-256color`, a face
/// with `:foreground "red"` is emitted as `ESC [ 38;5;200 m`, where before the
/// redefinition it is `ESC [ 31 m` -- index 1, which is also what approximating
/// (255,0,0) answers.
#[test]
fn a_named_colour_answers_its_registered_index_not_its_approximation() {
    let (mut palette, _) = parse_palette(RECORDED[3].alist);
    assert_eq!(
        palette.named("red").map(|(color, _)| color),
        Some(TerminalColor::Indexed(1))
    );
    assert_eq!(
        palette.approximate(255, 0, 0).map(|(color, _)| color),
        Some(TerminalColor::Indexed(9))
    );

    let mut entries: Vec<TtyPaletteEntry> = palette.entries().to_vec();
    entries[1] = TtyPaletteEntry {
        name: "red".to_owned(),
        index: 200,
        rgb: Some((255, 0, 0)),
    };
    palette = TtyPalette::new(entries, 256);
    assert_eq!(
        palette.named("red").map(|(color, _)| color),
        Some(TerminalColor::Indexed(200)),
        "a redefined name answers its registered index"
    );
}

/// `tty-color-canonicalize` (lisp/term/tty-colors.el:820-826): all-lower case,
/// blanks removed, and untouched when there is nothing to change.
#[test]
fn palette_names_are_canonicalized_as_lisp_canonicalizes_them() {
    assert_eq!(TtyPalette::canonicalize("white"), "white");
    assert_eq!(TtyPalette::canonicalize("White"), "white");
    assert_eq!(TtyPalette::canonicalize("Light Blue"), "lightblue");
    assert_eq!(TtyPalette::canonicalize("#AABBCC"), "#aabbcc");
}

/// A direct-colour terminal answers `tty-color-24bit`'s packed pixel instead of
/// searching at all (lisp/term/tty-colors.el:829-838), which is why
/// `(tty-color-desc "#123456")` measures as `("#123456" 1193046 ...)` under
/// `COLORTERM=truecolor` -- 1193046 is 0x123456.
#[test]
fn a_direct_colour_terminal_does_not_search() {
    let (palette, _) = parse_palette(RECORDED[3].alist);
    let direct = TtyPalette::new(palette.entries().to_vec(), 16_777_216);
    assert_eq!(
        direct.approximate(0x12, 0x34, 0x56).map(|(color, _)| color),
        Some(TerminalColor::Direct {
            r: 0x12,
            g: 0x34,
            b: 0x56
        })
    );
}

/// A row registered without RGB is reachable by name but is never a candidate
/// for approximating another colour: "If the RGB values of the candidate color
/// are unknown, we never consider it" (lisp/term/tty-colors.el:895-896).
#[test]
fn a_row_without_rgb_is_never_approximated_into() {
    let palette = TtyPalette::new(
        vec![
            TtyPaletteEntry {
                name: "known".to_owned(),
                index: 1,
                rgb: Some((10, 20, 30)),
            },
            TtyPaletteEntry {
                name: "unknown".to_owned(),
                index: 2,
                rgb: None,
            },
        ],
        8,
    );
    assert_eq!(
        palette.named("unknown").map(|(color, _)| color),
        Some(TerminalColor::Indexed(2))
    );
    assert_eq!(
        palette.approximate(10, 20, 30).map(|(color, _)| color),
        Some(TerminalColor::Indexed(1))
    );
}
