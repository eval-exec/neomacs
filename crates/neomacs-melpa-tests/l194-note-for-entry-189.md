**Note added 2026-08-24 by ledger 194.**  The seventh branch is **declined
finally**, and §3's cost 1 -- "`easy-mmode` would enter a dump no GNU build has
one in" -- is **gone**, at a price two orders of magnitude below the one this
entry names.

> 1. **`term/neo-win.el` is not yet a `term/FOO-win.el`.**  305 of its 728
>    lines (`:420`-`:724`) are renderer knobs with no GNU counterpart ...
>    Splitting that 305-line block into a GUI-runtime file is the prerequisite,
>    and it removes cost 1 outright.

The premise is right; the prerequisite does not follow from it.  Counted over
GNU's own window-system files, `define-minor-mode` is **0** in all eight --
`x-win.el`, `pgtk-win.el`, `ns-win.el`, `haiku-win.el`, `android-win.el`,
`w32-win.el`, `pc-win.el`, `common-win.el` -- but `defcustom` is **3 / 2 / 1**
in the first three and `global-set-key` is **8 / 5 / 3 / 1** in `w32-win`,
`android-win`, `ns-win` and `x-win`.  So "registration, key defaults, selection
backends and command-line handling and nothing else" is not what GNU's files
contain: a `defcustom` and a `global-set-key` are ordinary contents of one, and
the single form class that is zero everywhere is the one that pulls
`easy-mmode` in.

And `define-minor-mode` is a **macro**.  A `:global t` mode without `:keymap`
expands to `defcustom` / `defun` / `add-minor-mode`, none of which lives in
`easy-mmode`.  Measured on one fixture in GNU 31.0.90 and in this port alike:
with `(eval-when-compile (require 'easy-mmode))` the byte-compiled file loads
AND the mode toggles with `(featurep 'easy-mmode)` still nil.
`lisp/term/neo-win.el:42` now reads that way, pinned by
`the_gui_terminal_layer_does_not_load_easy_mmode` (RED before: `OK (t t t)`),
and no GUI session loads the library any more.

Two corrections to what remains.  §3's cost 3 (`x-preedit-overlay`,
`x-display-cursor-at-start-of-preedit-string`) is a **prerequisite** and not
only a cost: both are X-only in GNU -- `grep` over all eight `term/*-win.el`
finds them in `x-win.el` alone, `:1500` and `:1535` -- so the rename to the
port's own prefix has to happen before the branch, with
`define-obsolete-variable-alias` for the `defcustom`.  And §3's prerequisite 2
is bigger than its line range says: `load.rs:4627-4637` is only the two
`set_variable` calls, while the same function also writes the **frame's**
`window-system` parameter at `:4650-4656`, which is where the prototype's `neo`
must be coming from -- measured in `-Q --batch` that
`configure_gnu_startup_state` runs and takes its `FrontendKind::Tty` arm
(`build-details` is `t`, assigned at exactly one site,
`crates/neomacs/src/main.rs:4378`; `frame-initial-frame` is nil, which is that
arm), so `main.rs:4402-4403` already sets both variables to nil and the frame
still won.

Finally, a blocker this entry could not have seen: §9's pin
`this_build_answers_gnus_second_loadup_question_with_no_branch_at_all` lives
only in `main`.  A branch cut at `79b418443` has two tests in
`window_system_preload_test.rs` where main has five, so the branch cannot be
landed from such a tree without merging into a red main.  Whoever lands it must
work from a tree that carries this entry's pin and update it in the same commit.
