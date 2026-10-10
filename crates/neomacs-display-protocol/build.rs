//! Generate the X11 color-name database (`etc/rgb.txt`) into the crate.
//!
//! GNU resolves a color *name* through the frame terminal's `defined_color_hook`,
//! and every platform implementation of that hook answers from the same X11R6
//! `rgb.txt` data: the X server's copy of it under X (`XParseColor`,
//! src/xterm.c:9329), and `etc/rgb.txt` itself through `x-load-color-file`
//! (src/xfaces.c:7251) on NS, W32 and Android. The evaluator's face and
//! `color-values` paths and the image decoders that resolve names (XPM `c` keys)
//! therefore have to answer from ONE table -- two copies in two crates is
//! exactly the disagreement `docs/design/display-crate-layout.md` sends to
//! `neomacs-display-protocol`, and it is what made XPM render `gray14` black
//! (issue #545).
//!
//! Reading the file at compile time (rather than at startup) keeps the table
//! that ships identical to the `etc/rgb.txt` GNU ships, with no runtime I/O and
//! no second copy to drift.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_WORKSPACE_DIR");
    let workspace = PathBuf::from(
        std::env::var_os("CARGO_WORKSPACE_DIR")
            .expect("the workspace .cargo/config.toml must set CARGO_WORKSPACE_DIR"),
    );
    let rgb_path = workspace.join("etc/rgb.txt");
    println!("cargo:rerun-if-changed={}", rgb_path.display());

    // A missing or unreadable database must fail the build: a silently empty
    // table would resolve every name to nothing, which is the bug this table
    // exists to prevent, not a state to ship.
    let content = fs::read_to_string(&rgb_path).unwrap_or_else(|error| {
        panic!(
            "cannot read the X11 color database {}: {error}",
            rgb_path.display()
        )
    });

    // Parse rgb.txt: "R G B\t\tColorName". Names may contain spaces (`light
    // blue`), so everything after the three numbers is the name. Both the
    // spaced spelling and the spaces-collapsed one are keys: XPM colour values
    // and Lisp colour specs in the wild use either.
    let mut colors: BTreeMap<String, (u8, u8, u8)> = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let components = (
            parts.next().and_then(|s| s.parse::<u8>().ok()),
            parts.next().and_then(|s| s.parse::<u8>().ok()),
            parts.next().and_then(|s| s.parse::<u8>().ok()),
        );
        let name: String = parts.collect::<Vec<_>>().join(" ");
        let (Some(red), Some(green), Some(blue)) = components else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let lower = name.to_lowercase();
        let collapsed = lower.replace(' ', "");
        colors.entry(lower).or_insert((red, green, blue));
        colors.entry(collapsed).or_insert((red, green, blue));
    }

    let mut code = String::new();
    code.push_str("/// Look up one X11 color name, case-insensitively.\n");
    code.push_str("///\n");
    code.push_str("/// Generated at build time from `etc/rgb.txt` -- do not edit;\n");
    code.push_str("/// see `crates/neomacs-display-protocol/build.rs`.\n");
    code.push_str("#[must_use]\n");
    code.push_str("pub fn x11_color_lookup(name: &str) -> Option<(u8, u8, u8)> {\n");
    code.push_str("    match name.to_lowercase().as_str() {\n");
    for (name, (red, green, blue)) in &colors {
        code.push_str(&format!(
            "        {name:?} => Some(({red}, {green}, {blue})),\n"
        ));
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    // A table that came out short resolves names to nothing, which is the
    // failure this table exists to prevent (issue #545) -- so say so where
    // cargo will actually read it. Cargo only takes directives from stdout, and
    // a note on every build would be noise, so this is a real warning under a
    // floor rather than a per-build line.
    const EXPECTED_ENTRIES: usize = 700;
    if colors.len() < EXPECTED_ENTRIES {
        println!(
            "cargo:warning=the X11 color table has only {} entries (expected at least \
             {EXPECTED_ENTRIES}) from {}",
            colors.len(),
            rgb_path.display()
        );
    }

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("x11_colors.rs"), code).expect("failed to write x11_colors.rs");
}
