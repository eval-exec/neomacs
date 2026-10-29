# Category-aware visual word wrapping (#623)

GNU `src/xdisp.c:550–617` defines wrap opportunities from the displayed
character. With `word-wrap-by-category` nil, only space/tab allow a following
break, and a non-space/tab can start the next row. With it non-nil, category
`|` also allows a following break; `<` forbids an end-of-line position and `>`
forbids a beginning-of-line position. GNU swaps these restrictions for reversed
visual rows. Both `move_it_in_display_line_to` and `display_line` save a candidate
only when the preceding character allows a following break and the current
character allows a preceding one (`xdisp.c:10288–10300,26095–26115`).

This is a category-table rule, not a Unicode-range heuristic. Each buffer can
select and modify its own table. See the GNU manuals on
[categories](https://www.gnu.org/software/emacs/manual/html_node/elisp/Categories.html)
and [continuation lines](https://www.gnu.org/software/emacs/manual/html_node/emacs/Continuation-Lines.html).
`characters.el` installs the default CJK `|` membership; `kinsoku.el` installs
the punctuation restrictions.

The existing red regression in PR #626 tests the public `vertical-motion` seam
in a 24-column terminal frame. It returns `OK (24 2 15 15)` instead of GNU's
`OK (24 2 19 15)`. A private X11 GUI probe independently reproduced the symptom:
GNU starts the second row at 52 with category wrapping enabled and 15 with it
disabled; the existing pre-fix Neomacs release starts it at 15 in both cases.
The GUI baseline binary predates this PR; the PR-head unit regression provides
the source-matched red result.

Both Neomacs motion fallback and layout word-wrap checkpoint predicates were
hard-coded to whitespace. The fix shares native wrap opportunities between
these paths. Any category state captured for independent row workers must own
only native data, while retained geometry must also observe changes to the
option, the buffer-selected table and the categories beneath that table.
