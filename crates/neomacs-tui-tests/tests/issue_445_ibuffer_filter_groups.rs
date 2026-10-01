#![cfg(unix)]
//! TUI comparison tests: issue #445 — ibuffer with a configured filter-group
//! set that fills more than one screen.
//!
//! The reporter's ibuffer configuration is replayed here: their
//! `ibuffer-saved-filter-groups` list (emoji-named groups, the last one a
//! catch-all), their `ibuffer-formats` with the widened `icon-name` column,
//! and their replacement of `ibuffer-insert-filter-group`. The report is that
//! the list comes up incomplete — "*scratch* *Message* and png buffers" are
//! not shown — and that ibuffer "seems frequently freeze".
//!
//! What the replay shows is that the two halves are one symptom. The list
//! itself is complete in both editors: scrolled to the end, the two screens
//! agree exactly, summary line and all. On the way *up* the window they do
//! not: GNU paints the group that follows the last one that fits, its buffers
//! and the next group's header; Neomacs paints up to the end of the last group
//! that fits and leaves every row below it blank until the next command
//! repaints. The blank rows are exactly the ones carrying the buffers the
//! reporter named — the `*scratch*`/`*Messages*` group and the pictures group.
//!
//! Scope and fixture notes:
//!
//! * the packages the reporter's configuration needs are not available
//!   offline, so `all-the-icons`' two icon lookups are stubbed to a literal
//!   icon glyph and `ibuffer-projectile`'s hooks are left out; the group
//!   list, formats, columns and group-label advice are the reporter's, with
//!   the group filters shortened to the extensions the fixture actually uses;
//! * every buffer is created by the fixture with a literal name and literal
//!   contents: nothing here reads the clock, the locale, the user name or any
//!   installed tool, and no buffer visits a file, so no per-session path can
//!   reach the screen;
//! * the terminal is booted at the harness default size and narrowed
//!   afterwards, because a narrow boot can wrap the startup echo-area message
//!   and break the geometry the grid is compared on;
//! * the fixture's group set deliberately fills more than one window: the
//!   divergence above only appears once the list has groups past the bottom of
//!   the window, which is the state the reporter's own configuration puts
//!   ibuffer in.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Fixtures ───────────────────────────────────────────────

/// How many buffers of each kind the fixture creates.
///
/// Five kinds of fourteen, plus the two buffers both editors always have,
/// put the grouped list well past a thirty-five row window.
const BUFFERS_PER_GROUP: usize = 14;

/// Boot both editors with the reporter's ibuffer configuration loaded.
fn boot_reporter_fixture() -> (TuiSession, TuiSession, TuiTempFile) {
    let fixture = TuiTempFile::new(
        "neomacs-issue-445-",
        "issue-445-ibuffer.el",
        reporter_fixture_source(),
    );
    let extra = format!("-l {}", fixture.display());
    let (gnu, neo) = boot_pair(&extra);
    (gnu, neo, fixture)
}

/// The reporter's configuration, with the unavailable packages stubbed.
fn reporter_fixture_source() -> String {
    let mut source = String::from(
        ";;; issue-445-ibuffer.el --- reporter's ibuffer setup  -*- lexical-binding: t; -*-\n\
         (progn\n\
         \x20 ;; `use-package ibuffer' loads ibuffer before its :config; ibuf-ext\n\
         \x20 ;; is what makes filter groups partition the list at all.\n\
         \x20 (require 'ibuffer)\n\
         \x20 (require 'ibuf-ext)\n",
    );
    for index in 0..BUFFERS_PER_GROUP {
        for spec in [
            "many-%02d.py",
            "docs-%02d.org",
            "conf-%02d.yaml",
            "shot-%02d.png",
            "logs-%02d.log",
        ] {
            source.push_str(&format!(
                "  (with-current-buffer (get-buffer-create (format {:?} {index}))\n    (insert \"row\\n\")\n    (set-buffer-modified-p nil))\n",
                spec
            ));
        }
    }
    // The reporter's `all-the-icons` icon column, with both lookups stubbed
    // to one literal glyph (the package is not available to a paired test).
    source.push_str(
        r##"  (defun all-the-icons-icon-for-file (&rest _args) "📄")
  (defun all-the-icons-icon-for-mode (&rest _args) "📄")
  (define-ibuffer-column icon-name
    (:name "Name")
    (let* ((buf (get-buffer buffer))
           (icon (or (and (buffer-file-name buf)
                          (all-the-icons-icon-for-file
                           (file-name-nondirectory (buffer-file-name buf))))
                     (all-the-icons-icon-for-mode
                      (buffer-local-value 'major-mode buf))))
           (icon-str
            (cond ((stringp icon) icon)
                  ((symbolp icon) "")
                  ((listp icon) (format "%s" icon))
                  (t "")))
           (name (buffer-name buf)))
      (concat icon-str " " name)))
  (define-ibuffer-column simple-mode
    (:name "Mode")
    (let ((name-str (if (derived-mode-p 'dired-mode) "Dired" mode-name)))
      (if (stringp name-str) name-str (format "%s" name-str))))
  (setq ibuffer-formats
        '(("   " mark modified read-only locked " "
           (icon-name 28 28 :left :elide :prefix "| ")
           " " (size 8 -1 :right ibuffer-size-human-readable)
           " " (simple-mode 18 18 :left :elide)
           " " filename-and-process)))
  (setq ibuffer-saved-filter-groups
        '(("my-custom"
           ("🔍 Dired Remote" (and (mode . dired-mode) (filename . "/ssh:.*$")))
           ("📂 Dired Local" (mode . dired-mode))
           ("🗒️ Org" (mode . org-mode))
           ("📜 Codes" (or (name . "\\.\\(py\\|el\\|rs\\)$") (mode . python-mode)))
           ("🧪 Temp Async" (or (name . "^\\*Shell Command.*\\*$") (name . "^\\*Async.*$")))
           ("💻 Terminal" (or (mode . shell-mode) (mode . term-mode)))
           ("⚙️ Configs" (or (name . "\\.\\(json\\|ya?ml\\|toml\\|md\\|csv\\|conf\\|ini\\)$")))
           ("🛠️️ Emacs Internal" (or (name . "^\\*scratch\\*$")
                                     (name . "^\\*Help\\*$")
                                     (name . "^\\*Messages\\*$")))
           ("🖼️ Pictures" (or (name . "\\.\\(jpe?g\\|png\\|gif\\|svg\\|pdf\\)$")))
           ("📦 Archives" (or (name . "\\.\\(zip\\|gz\\|tar\\)$")))
           ("🧬 Molecular Structure" (or (name . "\\.\\(pdb\\|mol\\|xyz\\)$")))
           ("🧪 Quantum Chemistry" (or (name . "\\.\\(gjf\\|com\\|chk\\)$")))
           ("📊 Data Tables" (or (name . "\\.\\(dat\\|csv\\|tsv\\|txt\\)$")))
           ("📑 General Logs/Outputs" (or (name . "\\.\\(log\\|out\\)$")))
           ("📬 Gnus Mail" (or (name . "^\\.bbdb$")))
           ("❓ Others" (name . ".*")))))
  (setq ibuffer-show-empty-filter-groups nil)
  (defun my/ibuffer-setup ()
    (when (derived-mode-p 'ibuffer-mode)
      (setq ibuffer-filter-groups (cdr (assoc "my-custom" ibuffer-saved-filter-groups)))
      (ibuffer-update nil t)))
  (add-hook 'ibuffer-mode-hook #'my/ibuffer-setup)
  ;; The reporter's replacement of the group inserter: no brackets, the icon
  ;; kept out of the width computation, the label padded to 68 columns.
  (defun my/ibuffer-insert-filter-group (name display-name filter-string format bmarklist)
    (let* ((display-name-str
            (if (stringp display-name)
                display-name
              (let ((fmted (format-mode-line display-name)))
                (cond ((stringp fmted) fmted)
                      ((and (listp fmted) (stringp (car fmted))) (car fmted))
                      (t (format "%s" fmted))))))
           (line-length 68)
           (icon (if (string-match "^\\(.[️⃣]?\\)" display-name-str)
                     (match-string 1 display-name-str)
                   ""))
           (rest-text (substring display-name-str (length icon)))
           (icon-width (string-width icon))
           (text-width (max 0 (- line-length icon-width)))
           (rest-text-trunc (truncate-string-to-width rest-text text-width nil ?\s))
           (pad-length (max 0 (- text-width (string-width rest-text-trunc))))
           (pad-str (make-string pad-length ?\s))
           (underlined-text (propertize (concat rest-text-trunc pad-str)
                                        'face '(:underline t :weight bold)))
           (line (concat icon underlined-text))
           (line (truncate-string-to-width line line-length nil ?\s))
           (label (propertize line
                              'ibuffer-filter-group-name name
                              'keymap ibuffer-mode-filter-group-map
                              'mouse-face 'highlight
                              'help-echo "toggle")))
      (insert label "\n")
      (when bmarklist
        (put-text-property
         (point)
         (progn
           (dolist (entry bmarklist)
             (ibuffer-insert-buffer-line (car entry) (cdr entry) format))
           (point))
         'ibuffer-filter-group
         name))))
  (advice-add 'ibuffer-insert-filter-group :override #'my/ibuffer-insert-filter-group))
"##,
    );
    source
}

// ── Pair helpers ───────────────────────────────────────────

/// Wait for `predicate` on both editors, then drain the frame it paints.
fn step(gnu: &mut TuiSession, neo: &mut TuiSession, predicate: impl Fn(&[String]) -> bool + Copy) {
    gnu.read_until(Duration::from_secs(8), predicate);
    neo.read_until(Duration::from_secs(12), predicate);
    read_both(gnu, neo, Duration::from_millis(500));
}

/// Assert `predicate` against each editor's own grid, naming it on failure.
fn assert_both(
    what: &str,
    gnu: &TuiSession,
    neo: &TuiSession,
    predicate: impl Fn(&[String]) -> bool,
) {
    for (label, session) in [("GNU", gnu), ("Neomacs", neo)] {
        let grid = session.text_grid();
        assert!(
            predicate(&grid),
            "{label} should {what}; its grid was:\n{}",
            grid.join("\n")
        );
    }
}

// ── Tests ──────────────────────────────────────────────────

/// The reporter's scenario, first screen: with their filter groups active and
/// more of them than the window holds, both editors must paint the same rows.
///
/// GNU fills the window down to its last row: after the group that ends the
/// visible run come the `*scratch*`/`*Messages*` group, the pictures group and
/// the first of its buffers. Neomacs stops at the end of the last group that
/// fits and leaves those rows blank, so the buffers the reporter reported
/// missing are the ones that never get painted.
///
/// The two screens part at row 33 of the grid, column 0: GNU paints the
/// `🛠️️ Emacs Internal` label there, Neomacs leaves the cell empty, and the
/// four rows after it differ the same way (`*scratch*`, `*Messages*`,
/// `🖼️ Pictures`, `shot-00.png` against blank). Both editors agree again as
/// soon as a command repaints the window, which is what the companion case
/// below pins down.
///
/// This case is meant to stay RED: it is a real divergence and it is not to be
/// blessed from Neomacs, weakened, or skipped.
#[test]
fn issue_445_ibuffer_paints_every_row_of_a_grouped_list_like_gnu() {
    let (mut gnu, mut neo, _fixture) = boot_reporter_fixture();
    resize_both(&mut gnu, &mut neo, 40, 120);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "ibuffer");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("many-00.py"))
    });

    // The groups the configuration selects, in the order ibuffer renders
    // them: most recently used first.
    assert_both(
        "name each non-empty group above its buffers",
        &gnu,
        &neo,
        |grid| {
            grid.iter().any(|row| row.contains("📜 Codes"))
                && grid.iter().any(|row| row.contains("⚙️ Configs"))
                && grid.iter().any(|row| row.contains("🛠️️ Emacs Internal"))
                && grid.iter().any(|row| row.contains("🖼️ Pictures"))
        },
    );
    // The three buffers the report names, in the rows GNU paints for them.
    assert_both(
        "show the buffers the report says are missing",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("📄 *scratch*") && row.contains("Lisp Interaction"))
                && grid
                    .iter()
                    .any(|row| row.contains("📄 *Messages*") && row.contains("Messages"))
                && grid
                    .iter()
                    .any(|row| row.contains("📄 shot-00.png") && row.contains("Fundamental"))
        },
    );

    dump_pair_grids(
        "issue_445_ibuffer_paints_every_row_of_a_grouped_list_like_gnu",
        &gnu,
        &neo,
    );
    assert_pair_exact_display(
        "issue_445_ibuffer_paints_every_row_of_a_grouped_list_like_gnu",
        &gnu,
        &neo,
    );
}

/// The control for the case above: the same list, scrolled to its end.
///
/// This is where the report's "incomplete buffer list" is answered. The list
/// *content* is complete on both sides — every group, every buffer, the same
/// summary count — and the two screens agree exactly once a command has
/// repainted the window. So the missing buffers are a repaint symptom, not a
/// filtering one, and this case guards the filtering half against a "fix" that
/// makes the red case pass by dropping buffers from the list.
#[test]
fn issue_445_ibuffer_list_is_complete_and_agrees_at_its_end() {
    let (mut gnu, mut neo, _fixture) = boot_reporter_fixture();
    resize_both(&mut gnu, &mut neo, 40, 120);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "ibuffer");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("many-00.py"))
    });

    send_both(&mut gnu, &mut neo, "M->");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("Bot L"))
    });

    // The last group and its last buffer are on screen, and the summary line
    // below them counts the whole fixture in both editors.
    assert_both("reach the end of the list", &gnu, &neo, |grid| {
        grid.iter()
            .any(|row| row.contains("📑 General Logs/Outputs"))
            && grid.iter().any(|row| row.contains("📄 logs-13.log"))
            && grid.iter().any(|row| row.contains("processes"))
    });

    assert_pair_exact_display(
        "issue_445_ibuffer_list_is_complete_and_agrees_at_its_end",
        &gnu,
        &neo,
    );
}
