# Compiled Lisp Unicode symbol decoding

Normal startup on yibie-et stopped while loading nova-ui → nasy-theme → 彩.
The failing constant was `(defconst n-蠟白 "#FEF8DE")`. The resulting
`void-variable` named bytes `(159 231 153 189)`, the remainder after splitting
inside 蠟. The startup log shows Neomacs had just compiled 彩.el itself; the
file header reports Emacs 31.1. GNU Emacs loads that same artifact successfully.

## Reproduction and GNU behavior

The failure also reproduces on Linux. Compile the one-line constant using GNU
Emacs, then load the same .elc in both runtimes: GNU returns the color, whereas
unpatched Neomacs exits 255. Loading the .el source succeeds in both.

GNU src/lread.c `source_file_get` decodes a character before the reader sees
it. It uses `BYTES_BY_CHAR_HEAD` and checks trailing bytes, returning BYTE8 for
an incomplete sequence without consuming the following bytes. `skip_dyn_bytes`
uses byte counts independently of character decoding.

Neomacs exposed individual file bytes to tokenization and decoded UTF-8 only
inside string literals. Thus n-白 was interned with three separate byte-valued
characters, while byte A0 inside 蠟 was mistaken for a whitespace delimiter.
This is a reader defect, not evidence of an incompatible compiler version.

## Fix

Decode encoded-file input in the reader's source dispatch. The existing enum
keeps file input distinct from genuine unibyte Lisp strings/buffers. An
exhaustive match classifies file-decoded token text as multibyte, preserving
symbol identity. Positions remain file byte offsets. All literal parsers use
the shared character stream; the redundant string-only decoder is removed.
The decoder retains GNU's extended five-byte characters and BYTE8 fallback.

## Tests

`load_elc_preserves_unicode_symbol_identity` failed before the fix with the
same corrupted symbol as the original startup. It now checks both n-白 and
n-蠟白 through the loader and evaluator.

`load_elc_decodes_characters_but_skips_docstring_bytes` checks Unicode docstring
byte skipping, character literals, Unicode strings, malformed/truncated byte
sequences, and a five-byte Emacs character. Expected values were verified with
GNU Emacs. Existing unibyte reader tests remain unchanged.

All 364 selected reader and compiled-load tests passed with cargo nextest.
A broader loader run passed all 37 tests (some overlap with the first run).
Formatting and diff checks passed.

Release builds completed on Linux and macOS through `cargo xtask fresh-build
--release` (Linux additionally enables `--features webview`). Both rebuilt
runtimes successfully load the original Mac 彩.elc and return
`("#ffffff" "#FEF8DE" t)` for the two constants and feature membership.
The Linux binary also passes the minimal reproduction and mixed-encoding
probe with output identical to GNU Emacs 31.1.
Detailed reproduction/build logs are in tmp/startup-review on each machine.

## Normal macOS startup after the fix

A new GUI instance was launched without `-Q` using the normal user config.
It no longer entered the debugger for the corrupted Unicode symbol. It stopped
later with a different error:

```
Failed to apply resizing #<window 1 on *scratch*>
```

The post-init diagnostic timer was never reached, so full configured startup
is not verified as successful. The fontdb warning about Sauce Code Pro Nerd
Font Complete.ttf also remains. Those are separate findings; this patch does
not claim to fix them. Logs for this run are in
`tmp/startup-review/fixed-startup/` on the Mac; the debugger error is copied to
`tmp/startup-review/macos-fixed-startup-errors.txt` locally.
