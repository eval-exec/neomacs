//! The Elisp every Vertico scenario boots.
//!
//! Fixtures are fixed lists written here in full: a scenario may not read the
//! host's clock, locale, user name, CPU count or installed tools, because a
//! frozen grid that encodes one of those facts drifts on the next machine.

/// The suite's original fixture: three real buffers, a five-candidate window
/// and cycling, so the first scenario's grids stay the ones it was blessed
/// with.
pub(super) const VERTICO_TUI_PRELUDE: &str = r#"
(require 'vertico)
(setq vertico-count 5
      vertico-cycle t)
(vertico-mode 1)
(dolist (fixture '(("project-alpha" . "ALPHA BUFFER\n")
                   ("project-beta" . "BETA BUFFER\n")
                   ("project-notes" . "NOTES BUFFER\n")))
  (with-current-buffer (get-buffer-create (car fixture))
    (erase-buffer)
    (insert (cdr fixture))))
"#;

/// A fixture set: one buffer per name, each holding a line naming itself, so a
/// scenario can see which buffer was selected from the screen alone.
///
/// The names are written out in the prelude rather than derived from the clock,
/// the locale, the file system or anything else the host owns: the candidate
/// order every scenario freezes is a function of these names and the package's
/// sort function, and of nothing else.
pub(super) fn fixture_buffers(names: &[String]) -> String {
    let list = names
        .iter()
        .map(|name| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "(dolist (name (list {list}))\n  \
         (with-current-buffer (get-buffer-create name)\n    \
         (erase-buffer)\n    \
         (insert (concat name \"\\n\"))))\n"
    )
}

/// Uniformly named fixture buffers: `<prefix>-01` up to `<prefix>-<count>`.
///
/// Equal-length names make the order the default sort function produces the
/// names' own order, so a scenario can name the candidate it expects by
/// position.
pub(super) fn numbered_names(prefix: &str, count: usize) -> Vec<String> {
    (1..=count)
        .map(|number| format!("{prefix}-{number:02}"))
        .collect()
}

/// The buffer a scenario can display while it drives the minibuffer.
///
/// It holds one short line, so a terminal narrowing wraps nothing: the window
/// above the candidate window is then the same window at every width, which is
/// what a scenario that resizes the terminal wants to be looking at.
pub(super) const VERTICO_HELD_BUFFER: &str = "held";
pub(super) const VERTICO_HELD_PRELUDE: &str = r#"
(with-current-buffer (get-buffer-create "held")
  (erase-buffer)
  (insert "held buffer line
"))
"#;

/// Vertico as the package ships it: `vertico-mode` on, `vertico-count` and
/// `vertico-cycle` left at their defaults, over `fixture`.
///
/// The defaults are the point of the scenarios that use this: the candidate
/// window's height is `vertico-count`'s default of ten, not a configured value.
pub(super) fn default_vertico(fixture: &str) -> String {
    format!("(require 'vertico)\n(vertico-mode 1)\n{fixture}")
}

/// Vertico with `vertico-repeat`, saving each session as it is set up, over
/// `fixture`.
///
/// `vertico-repeat` replays the last completion session, and it only has one to
/// replay if `vertico-repeat-save` is on `minibuffer-setup-hook` -- that hook
/// is how the extension documents its use, so it is what this prelude sets.
pub(super) fn repeat_vertico(fixture: &str) -> String {
    format!(
        "(require 'vertico)\n\
         (require 'vertico-repeat)\n\
         (vertico-mode 1)\n\
         (add-hook 'minibuffer-setup-hook #'vertico-repeat-save)\n\
         {fixture}"
    )
}

/// Vertico with the display modes `vertico-multiform` toggles between, over
/// `fixture`.
///
/// The extension files ship in the package, next to `vertico.el`, so requiring
/// them by name is how a user gets them; loading them also defines the minor
/// modes `vertico-multiform-mode`'s toggles call.
///
/// `vertico-grid-separator` is set to a single bar. The default is a bar with
/// three spaces on either side and an `:inverse-video` display property on the
/// bar itself, and the two editors do not render it alike: Neomacs paints the
/// cell behind the bar where GNU leaves it at the terminal's default
/// background, and where GNU writes the separator's leading spaces Neomacs
/// moves the cursor over them and leaves the cells unwritten. Neither
/// difference is visible on screen, and neither is what this scenario is about
/// -- the grid's column arithmetic, which the separator's *length* alone
/// decides. A separator with no spaces around the bar leaves no blank cells
/// between candidates for the emissions to disagree about, so the frozen
/// screens and the wire-state checks pin the layout and nothing else.
/// Vertico with the display modes `vertico-multiform` toggles between, over
/// `fixture`, leaving `vertico-grid-separator` at the package's default.
///
/// The default separator is a bar with three spaces on either side and an
/// `:inverse-video` display property on the bar itself, and both of those are
/// visible in the terminal state the grid leaves behind: the blank cells the
/// separator's spaces occupy, and the cell the bar is painted on. A scenario
/// that is about either of them -- rather than about the grid's column
/// arithmetic -- boots the package the way it ships, which is what this does.
///
/// The extension files ship in the package, next to `vertico.el`, so requiring
/// them by name is how a user gets them.
pub(super) fn default_grid_separator_vertico(fixture: &str) -> String {
    format!(
        "(require 'vertico)\n\
         (require 'vertico-multiform)\n\
         (require 'vertico-grid)\n\
         (vertico-mode 1)\n\
         (vertico-multiform-mode 1)\n\
         {fixture}"
    )
}

/// Vertico with the display modes `vertico-multiform` toggles between, over
/// `fixture`, plus `vertico-buffer`.
///
/// `vertico-buffer` is the display mode that renders the candidates into a
/// window of its own rather than into the minibuffer window, and `M-B` is the
/// key `vertico-multiform` toggles it with.
pub(super) fn multiform_buffer_vertico(fixture: &str) -> String {
    format!(
        "(require 'vertico)\n\
         (require 'vertico-multiform)\n\
         (require 'vertico-buffer)\n\
         (vertico-mode 1)\n\
         (vertico-multiform-mode 1)\n\
         {fixture}"
    )
}

/// `vertico-quick` is loaded and bound the way its own commentary shows it:
/// the extension binds no keys itself, so a user adds the key column to
/// `vertico-map` -- `M-q` for `vertico-quick-insert`.
pub(super) fn multiform_vertico(fixture: &str) -> String {
    format!(
        "(require 'vertico)\n\
         (require 'vertico-multiform)\n\
         (require 'vertico-grid)\n\
         (require 'vertico-flat)\n\
         (require 'vertico-buffer)\n\
         (require 'vertico-quick)\n\
         (vertico-mode 1)\n\
         (vertico-multiform-mode 1)\n\
         (keymap-set vertico-map \"M-q\" #'vertico-quick-insert)\n\
         (setq vertico-grid-separator \"|\")\n\
         {fixture}"
    )
}
