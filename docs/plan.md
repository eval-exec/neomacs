# NeoVM / Neomacs Plan

Last updated: 2026-02-14

## Done

- Full suite baseline verified after latest cleanup:
  - `cd test/neovm/vm-compat && make check-all-neovm` (pass)
- Added a builtin-registry-wide `fboundp` parity checker against GNU Emacs `-Q`:
  - `test/neovm/vm-compat/check-builtin-registry-fboundp.sh`
  - allowlist: `test/neovm/vm-compat/cases/builtin-registry-fboundp-allowlist.txt`
- Added `make` target:
  - `make -C test/neovm/vm-compat check-builtin-registry-fboundp`
- Wired the parity gate into CI:
  - `.github/workflows/vm-compat.yml`
- Documented the gate in:
  - `test/neovm/vm-compat/README.md`
  - `docs/elisp-vm-design.md`
- Removed multiple unexposed/stale builtin implementations to keep dispatch surface aligned with the registry and oracle policy:
  - `custom-group-p` implementation cleanup
  - `selected-terminal` implementation cleanup
  - `display-line-numbers-update-width` implementation cleanup
  - `register-to-string` implementation cleanup
  - `extract-rectangle-line` implementation cleanup
  - removed stale misc stubs superseded by evaluator paths
  - removed unexposed bookmark helper builtin implementations
  - removed unexposed `string-repeat` helper implementation
  - removed unexposed `bool-vector-complement` implementation
  - removed unexposed `base64url-decode-string` helper
  - removed unexposed `hash-table-keys` / `hash-table-values` helpers
  - removed unexposed `downcase-char` helper
  - removed unexposed `find-coding-system` helper
  - removed unexposed `word-at-point` helper
- Kept branch green with targeted Rust tests and vm-compat checks after each slice.

## Doing

- Continue compatibility-first maintenance with small commit slices:
  - keep builtin surface and registry in lock-step
  - run oracle/parity checks after each behavior-affecting change
  - remove dead helper code that is not part of exposed compatibility surface
- Reduce vm-compat operator friction for large case sets (small Makefile UX improvements).

## Next

1. Run a full vm-compat verification pass (`check-all-neovm`) after the latest cleanup batch and record results.
2. Expand focused oracle corpora for remaining high-risk areas still carrying stubs (search/input/minibuffer/display edges).
3. Prioritize one high-impact stub-to-real implementation slice with oracle lock-in, then repeat.
4. Keep Rust backend behind compile-time switch and preserve Emacs C core as default backend.
