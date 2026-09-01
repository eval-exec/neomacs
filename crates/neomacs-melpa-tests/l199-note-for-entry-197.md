**Note added 2026-08-26 by ledger 199.**  Entry 197 is not on this branch --
this worktree is cut at `79b418443`, whose `DIVERGENCES.md` ends at 194 -- so
this note cannot be placed in place.  It follows the shape ledger 194 used for
its own out-of-branch note (`l194-note-for-entry-189.md`), and it should be
folded into 197 at merge.

There are two halves: what 199 copied from 197's module, and a hole 199 found
in it.

### 1. `provide_coupled_vars.rs` is the variable-side twin of `c_features.rs`

199's task was the variable side of 197's question -- "a GTK-only name reaching
the obarray of a build that answers `(featurep 'gtk)` nil should be hard to
spell" -- and the shape is copied deliberately:

```rust
pub enum HereDecision {
    /// Not bound here either -- GNU's coupling is honoured.
    Absent,
    /// Bound here anyway.  `policy` names the entry that decided that and the
    /// pin that holds it; the fact cannot be recorded without one.
    BoundByPolicy { policy: &'static str },
}
```

-- the same property `c_features::HereDecision` has, that there is no variant
meaning "yes, because the list used to say so".  234 rows, 160 `Absent` and 74
`BoundByPolicy`, and `provide_coupled_vars_test.rs` checks both directions
against a booted obarray: an `Absent` row that is bound is an invention, and a
`BoundByPolicy` row that is *unbound* is a policy outliving the thing it
excused.

The rule the table encodes is GNU's own and is two-way: when a `DEFVAR_*` and
an `Fprovide` share a preprocessor block, one `configure` switch compiles both,
so `(boundp 'V)` and `(featurep 'F)` are the same question in every build GNU
can produce (`src/xfns.c:10539-10558` is the pair for `gtk` /
`gtk-version-string` and `cairo` / `cairo-version-string`).  Falsified against
GNU 31.0.90 gtk3 before use: it names 150 variables that build cannot have, and
GNU binds 0 of them.

### 2. The `Fprovide` derivation was `src/*.c` only, and GNU's NS backend is `.m`

`c_features_test.rs` records its own derivation:

```text
grep -rhn 'Fprovide (' src/*.c | sed 's/.*Fprovide (//;s/,.*//' | sort -u
```

-- "32 call sites, 26 distinct names".  Over `src/*.c` **and `src/*.m`** it is
**35 call sites, 29 distinct names**.  The three that were missing are `ns`
(`src/nsterm.m:11744`), `cocoa` (`:11757`) and `gnustep` (`:11760`), i.e. three
features GNU's C provides that the table had **no row for at all** -- which is
exactly the hole 192 built the table to close: "a feature GNU provides and this
table has no opinion about cannot be audited".

It stayed invisible because the pin encodes the same blind spot:
`the_table_covers_exactly_the_features_gnus_c_provides` compares the table
against `GNU_C_PROVIDES`, and `GNU_C_PROVIDES` was written from the same
`.c`-only command, so table and pin agreed with each other while both were short
by three.  A second copy sat in `every_row_cites_gnus_own_site`, which asserted
`row.gnu_site.contains(".c:")` and would have rejected the three rows on sight.

Fixed on this branch: three `NotBuilt` rows citing `nsterm.m`, `gnu_c_features()`
27 -> 30, the derivation command widened, and both assertions widened to accept
`src/*.m`.  `cocoa` and `gnustep` are the two arms of one `#ifdef`
(`nsterm.m:11756-11762`), so GNU provides exactly one of them in an NS build and
neither elsewhere.  `features` is unchanged: `initial_feature_names()` filters
on `provided()` and all three rows are `NotBuilt`.

**What 197 should re-check at merge.**  197's contribution over 192 is described
in 199's brief as "a runtime scan catching out-of-table providers in any crate".
That scan's *source of truth* for "in-table" is `gnu_c_features()`, so before
199's three rows it could not have flagged a crate providing `ns`, `cocoa` or
`gnustep` as out-of-table -- it would have flagged them as *unknown features*
rather than as *known features this build must not advertise*, or missed them
entirely, depending on how it enumerates.  Whichever it is, it should be
re-verified against `src/*.m` after the merge, not assumed.  The general lesson
is ledger 195's and it is worth restating in 197: **a sweep has to be checked
against the protocol, not only against the tree** -- here the protocol is GNU's
source, and the check was one file extension away from being right.
