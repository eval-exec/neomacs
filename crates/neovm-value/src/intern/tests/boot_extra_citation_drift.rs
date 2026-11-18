//! Peer 5833 (b): every boot-extra allowlist row is a citation, and the
//! citation is checked against the pinned GNU source rather than trusted.
//!
//! The boot-extra list in `neovm_value::intern` exists because a startup
//! obarray member can come from a preloaded Lisp read or a C `syms_of_*`
//! interning call, not from a compiled `DEFSYM`. That is a compatibility
//! claim about GNU 31.1 at
//! `a360712c9d272d950d8d8255ef74570f7e90b7d9`, so each row carries
//! `(source_file, source_line, source_needle)` and this test re-reads the
//! checkout:
//!
//! * the checkout HEAD must be the pinned commit;
//! * the cited line must still contain the needle, and the needle must still
//!   contain the spelling;
//! * for `src/font.c` rows, the style-name tables must still be interned by
//!   `build_style_table`;
//! * for `lisp/` rows, `loadup.el` must still preload the citing file.
//!
//! `GNU_EMACS_SOURCE` names the checkout. Without it the test reports SKIP:
//! verifying a citation needs the source, and substituting another checkout
//! would check the wrong release. The long-term design (recorded in the
//! pull request) generates the boot-extra set from the pinned loadup closure,
//! which removes the hand citations this test protects.

use super::GNU_BOOT_EXTRA_DEFSYMS;
use std::path::PathBuf;
use std::process::Command;

/// The canonical oracle commit from `parity-reference.toml`.
const PINNED_COMMIT: &str = "a360712c9d272d950d8d8255ef74570f7e90b7d9";

fn pinned_source_root() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("GNU_EMACS_SOURCE")?);
    assert!(
        root.is_dir(),
        "GNU_EMACS_SOURCE must name the pinned emacs-31.1 checkout, got {}",
        root.display()
    );
    Some(root)
}

#[test]
fn boot_extra_citations_drift_from_the_pinned_source() {
    let Some(root) = pinned_source_root() else {
        eprintln!(
            "SKIP boot_extra_citations_drift_from_the_pinned_source: \
             set GNU_EMACS_SOURCE to the pinned emacs-31.1 checkout"
        );
        return;
    };

    let head = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse the GNU source checkout");
    assert!(
        head.status.success(),
        "git rev-parse failed for {}: {}",
        root.display(),
        String::from_utf8_lossy(&head.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        PINNED_COMMIT,
        "the citation check must run against the pinned GNU 31.1 commit"
    );

    let rows = GNU_BOOT_EXTRA_DEFSYMS;
    assert!(
        rows.len() >= 18,
        "the cited boot-extra allowlist shrank to {} rows",
        rows.len()
    );

    let loadup = std::fs::read_to_string(root.join("lisp/loadup.el"))
        .expect("read lisp/loadup.el from the pinned checkout");
    let font_c = std::fs::read_to_string(root.join("src/font.c"))
        .expect("read src/font.c from the pinned checkout");

    let mut seen: Vec<&str> = Vec::with_capacity(rows.len());
    for row in rows {
        assert!(
            !seen.contains(&row.spelling),
            "duplicate boot-extra row for {}",
            row.spelling
        );
        seen.push(row.spelling);
        assert!(
            row.source_needle.contains(row.spelling),
            "{}: the cited needle {:?} does not name the spelling",
            row.spelling,
            row.source_needle
        );
        let path = root.join(row.source_file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: cited file {} unreadable: {e}",
                row.spelling, row.source_file
            )
        });
        let line = text
            .lines()
            .nth(row.source_line as usize - 1)
            .unwrap_or_else(|| {
                panic!(
                    "{}: {} has fewer than {} lines",
                    row.spelling, row.source_file, row.source_line
                )
            });
        assert!(
            line.contains(row.source_needle),
            "{}: {}:{} no longer contains {:?}\n  actual: {}",
            row.spelling,
            row.source_file,
            row.source_line,
            row.source_needle,
            line.trim_end()
        );

        // The font style tables are interned by one loop, not by a DEFSYM;
        // the per-row cite locates the name string, this proves the table is
        // still what startup reads.
        if row.source_file == "src/font.c" {
            assert!(
                font_c.contains("intern_c_string (entry[i].names[j])"),
                "{}: src/font.c no longer interns the style-table name strings",
                row.spelling
            );
        }

        // A bare or quoted symbol in a Lisp file is only a startup member if
        // loadup preloads that file.
        if let Some(stem) = row
            .source_file
            .strip_prefix("lisp/")
            .and_then(|path| path.strip_suffix(".el"))
        {
            let load_form = format!("(load \"{stem}\")");
            assert!(
                loadup.contains(&load_form),
                "{}: lisp/loadup.el no longer preloads {}",
                row.spelling,
                row.source_file
            );
        }
    }
}
