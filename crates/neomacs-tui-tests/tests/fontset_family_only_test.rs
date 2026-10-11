//! Terminal text parity after installing family-only CJK fontset ranges.
//! Native font selection and glyph coverage are checked by the GUI suite.

use crate::support::{assert_pair_exact_display, eval_expression};
use neomacs_tui_tests::TuiSession;
use std::time::Duration;

#[test]
fn family_only_fontset_ranges_preserve_cjk_terminal_text() {
    let mut gnu = TuiSession::gnu_emacs("");
    let mut neo = TuiSession::neomacs("");
    let startup = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text"))
    };
    // This case permits a debug Neomacs startup without altering the shared
    // pair harness or masking a GNU launch failure behind the longer budget.
    gnu.read_until(Duration::from_secs(15), startup);
    neo.read_until(Duration::from_secs(120), startup);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            startup(&session.text_grid()),
            "{label} startup did not finish:\n{}",
            session.text_grid().join("\n")
        );
    }

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(progn
(set-fontset-font t '(#x4e00 . #x9fff) (font-spec :family "Noto Sans CJK SC"))
(set-fontset-font t '(#xac00 . #xd7af) (font-spec :family "Noto Sans CJK SC"))
(switch-to-buffer (get-buffer-create "fontset-family-only"))
(erase-buffer)
(setq-local mode-line-format '(" FONTSET 中 한글 "))
(setq-local header-line-format nil)
(setq-local display-line-numbers nil)
(insert "FONTSET-FAMILY-ONLY\n中 日本語 한글\nrange: 中文 / 日本語 / 한글\n")
(goto-char (point-min))
(set-buffer-modified-p nil)
(message nil)
(redisplay)
nil)"#,
    );
    let rendered = |grid: &[String]| {
        grid.iter().any(|row| row.contains("FONTSET-FAMILY-ONLY"))
            && grid.iter().any(|row| row.contains("中 日本語 한글"))
            && grid
                .iter()
                .any(|row| row.contains("range: 中文 / 日本語 / 한글"))
            && grid.iter().any(|row| row.contains("FONTSET 中 한글"))
    };
    gnu.read_until(Duration::from_secs(8), rendered);
    neo.read_until(Duration::from_secs(12), rendered);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            rendered(&session.text_grid()),
            "{label} lost CJK terminal text after family-only installation:\n{}",
            session.text_grid().join("\n")
        );
    }
    assert_pair_exact_display("family_only_fontset_cjk_terminal", &gnu, &neo);
}
